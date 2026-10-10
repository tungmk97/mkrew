# Inbox 2026-10-10: Redesign — Title Bar, Usage, Status Bar, Sidebar, Tabs and Panes, Pin, Session Panel, Plan Checklist, File Navigation, Diff Modes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Handle the ten new items in `docs/INBOX.md` (lines 39–113), all taken from the redesign choices in `docs/superpowers/specs/2026-10-10-ui-redesign-options.md`:
- One custom title bar replaces the OS title bar and the toolbar; Run colors; branch chip and `Git N` badge for 1 or N repos.
- `session.usage` splits cache tokens, carries the model, and works for omp.
- A status bar at the bottom; the sidebar can be hidden.
- The sidebar gets a filter, chips, a cleaner row layout and a new palette; the Inbox card goes; Change color no longer navigates.
- Tabs get type icons, status dots, `+` / split / `⋯`; the pane header gets a breadcrumb and one status pill; panes can be renamed and colored from a right-click menu.
- Workspaces, sessions and tabs can be pinned.
- A session panel on the right (usage, transcript, Stop, changes per repo) and a plan checklist linked to a session.
- Open in File Explorer / Zed menus, Ctrl+Click on paths and URLs in terminals, a rendered Markdown preview in Files.
- Unified / Split diff modes in the Git view.

**Architecture:**
- Item 1 (Tasks 1–3): `run::action_colors` (Start green, Restart amber, Stop red, new `danger_bg` tint) for the Run button and its ▾ menu. A pure `git_summary.rs` (per-repo branch, HEAD, changed counts) is filled for the shown workspace only by a single-flight loader (`git.repos`, then one `git.status` per repo) on a switch and on `git.changed`. One 44 px custom title bar replaces the OS bar and `render_toolbar` on Windows (`WindowControlArea` Drag/Min/Max/Close, so Snap Layouts and double-click come from the OS) and macOS; Linux keeps the OS bar; the root becomes a column (ADR 0034 amends 0027). No protocol change.
- Item 2 (Tasks 4–5): core `Usage`/`UsageReader` stop folding cache into `input` and keep `cache_read`, `cache_write` and the newest model; `SessionUsageEvent` carries them with serde defaults so stored events still replay; UI totals add all four counts, so totals and ctx% do not move (protocol 33, ADR 0035). omp is one more `UsageFormat` (`omp_jsonl`) located by id under `sessions/**`, with `context_window` in `agents/omp.toml`.
- Item 3 (Task 6): `Shell::render_status_bar` at the bottom reads only state the shell already keeps (inbox entries, git summary, usages, focused session); pure formatting in `usage::fields`, `inbox::label`, `GitSummary::head_label`. `Command::ToggleSidebar` (Ctrl+Shift+B), hidden state in memory. No timer: elapsed time refreshes with the existing `status::redraw_due` tick.
- Item 4 (Tasks 7–9): root cause of the Change-color jump: the workspace menu panel lacked `.occlude()`, so the click also reached the row behind; fixed once in a shared `menu_panel` helper used by every deferred menu. The palette gains `lime` (9 names, old names stay valid; ADR 0020 amended). Pure `sidebar.rs` filter / chips / summary logic over `inbox::needs_you`; the Inbox card, `LastLines`, `Place::Inbox` and `workspace_total` are removed (ADR 0036).
- Item 5 (Tasks 10–12): pure `tabs::tab_kind` and `pane_text` (breadcrumb, folder, pill) drive the new tab strip, `TabMenu` and pane header. `Session.color` end to end (migration 0015, `session.set_color`, `Command::SetColor` through the session actor via a shared `Server::edit_session` that `session.rename` also uses, resume copy; protocol 34, ADR 0037). `PaneMenu` + generalised `color_swatch`; the color shows as a tinted agent chip and a 2 px line under the header.
- Item 6 (Tasks 13–14): `pinned` on project, session and terminal_tab (migration 0016), `ProjectUpdateParams.pinned`, `session.pin` (through `edit_session`), `tab.pin` (protocol 35); resume copies the pin. The UI sorts pinned first, stably (`Sidebar`, `Tabs::sort`); Pin/Unpin in the workspace, pane (also from a sidebar session-row right-click) and tab menus.
- Item 7 (Tasks 15–16): a third column right of Terminals (closed at start, Ctrl+Shift+Y and a status-bar button); width saved as `[ui] inspector_width` through a generic `settings::set_ui_value` / `write_ui_value`; the edge is dragged with GPUI `on_drag` / `on_drag_move`. The transcript path travels on `session.usage` (`transcript`, protocol 36, ADR 0038). The Changes card is built from the git summary by the pure `inspector::changes_card`.
- Item 8 (Tasks 17–18): `session.link_plan` stores `session.plan_path` (migration 0017, kept by resume); the daemon runs one one-file watcher per linked live session (extracted from `settings::watch`), parses task-list lines with a pure scanner in mkrew-core and sends `session.plan` only when the checklist changed (protocol 37, ADR 0039). The UI keeps a pure `Plans` state, draws the Plan card and a thin bar on the selected sidebar row.
- Item 9 (Tasks 19–21): UI only. GPUI `App::reveal_path` / `open_with_system` (no new crate) behind a Files-tree right-click menu and a workspace-menu item. A pure `terminal/links.rs` detector reads one hovered row of the cached grid while Ctrl/Cmd is held, plus an open-at-line path in `FilesView`. Files reuses the Docs preview (`docs::Shown::from_text`, `preview::blocks`) for `.md`, built off-thread from the editor text; Source mode is the ADR 0028 editor.
- Item 10 (Task 22): Split mode is one `uniform_list` of paired rows (indices into `Shown.rows`), so the single `git_diff_scroll` handle syncs both sides; comment picking stays on new-side line numbers; hunk n/p is mode-aware. The mode is a global `[ui] diff_mode` written through `write_ui_value`. No protocol or store change.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), alacritty_terminal 0.26, rusqlite, notify, serde, pulldown-cmark, mermaid-rs-renderer, insta. No new dependencies.

**Spec:** `docs/SPEC.md` §5, §7, §10, §11, §12A, §12B, §13, §14, §20.0, §20E.3, cited per task. Redesign source: `docs/superpowers/specs/2026-10-10-ui-redesign-options.md` (IDs A1–A13, B1–B4, B6, B8, B9, B16, C1). Open questions are at the end, each with the default this plan uses.

**Order:** tasks run in number order; later tasks build on earlier ones (Task 6 needs Tasks 2 and 4; Task 9 needs Tasks 3 and 6; Tasks 12 and 14 need Tasks 7, 8 and 10; Tasks 15–16 need Tasks 2, 4 and 6; Task 18 needs Tasks 9 and 15; Task 22 needs Task 15's `write_ui_value`). Every task names what it consumes in its **Interfaces** block.

**Inbox mapping:**

| Inbox line | Task |
| --- | --- |
| 39 `1. Title bar + toolbar redesign` | 1, 2, 3 |
| 47 `2. Usage data: omp usage, cache split, model` | 4, 5 |
| 54 `3. Status bar + hide sidebar` | 6 |
| 61 `4. Workspace sidebar redesign + cleanup` | 7, 8, 9 |
| 72 `5. Terminal tabs + pane header` | 10, 11, 12 |
| 80 `6. Pin feature` | 13, 14 |
| 88 `7. Right panel (session inspector)` | 15, 16 |
| 95 `8. Plan checklist in the right panel` | 17, 18 |
| 101 `9. File navigation & Markdown preview` | 19, 20, 21 |
| 107 `10. Git module: diff view modes` | 22 |

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1). Every agent difference lives in its TOML profile. There is no `match "claude"` anywhere in Rust code; terminal key bytes stay generic.
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14). Keep the current background colors (redesign source: "giữ nguyên" token `theme.rs`).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change. ADR numbers in this plan: 0034 (Task 3), 0035 (Task 4), 0036 (Task 9), 0037 (Task 12), 0038 (Task 15), 0039 (Task 17); Task 8 amends ADR 0020 in place. Before creating one, list `docs/decisions/` and take the next free number if the plan's number is taken (renumber the task's references).
- **Protocol version:** Tasks 4 (→ 33), 12 (→ 34), 13 (→ 35), 15 (→ 36) and 17 (→ 37) change the wire format. Each one bumps `PROTOCOL_VERSION` (`crates/mkrew-protocol/src/lib.rs:16`, 32 today) by one from its value when the task lands. Numbers quoted inside tasks assume plan order.
- **Migrations:** `0015_session_color.sql` (Task 12), `0016_pinned.sql` (Task 13), `0017` plan path (Task 17). Register each in `MIGRATIONS` (`crates/mkrew-store/src/lib.rs`). Before creating one, list `crates/mkrew-store/migrations/` and take the next free number if the plan's number is taken.
- Few dependencies: add no new crates.
- GPUI rule: set `.hover()` at most once per element (`button()`/`quiet_button()`/`row_action()`/`menu_item()` already set it). `group_hover` is a separate slot and is safe.
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use; do not run `git commit`.
- Leave uncommitted human edits alone. Today the working tree also holds the uncommitted result of plan `2026-10-09-inbox-editor-git-repos-keys.md` (76 modified files, ADRs 0028–0033, migration 0014); that is the baseline, do not revert or reformat it beyond what a task needs. Never touch `.github/workflows/*`, `packaging/`, `docs/BUILD_VI.md`, `design/`, `icon/`, `docs/superpowers/specs/`, or the x11/wayland lines in `crates/mkrew-ui/Cargo.toml`.
- Never write files outside this repo. Edits to the user's own `<config>/mkrew/*` files are **user steps**, listed in HUMAN_CHECKS.
- **Inbox:** when the last task of an item passes verification, change its `docs/INBOX.md` line from `[~]` to `[x]` (see the mapping above). Items with several tasks are ticked after their last task.
- Verify on Windows with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these three in order:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace -j 4`
- The C: drive is nearly full: no `cargo clean`, no extra target dirs, no downloads.
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions or the daemon.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.
- `crates/mkrew-ui/src/main.rs` is ~18k lines: read ranges, never the whole file. Line numbers below are from 2026-10-10 and drift as tasks land. Re-find each function by name (`grep -n "fn render_title_bar"`).
- UI work cannot be unit-tested headless: pure logic gets tests; every visible change gets a HUMAN_CHECKS smoke line that the user runs with `cargo run -p mkrew-ui`.

## Review Focus

1. **A workspace folder with no `.git` that holds several repos (ADR 0029).** Expected: the title-bar branch chip says `⎇ N repos` (tooltip lists each repo's branch), the `Git N` badge sums every repo, the status bar shows `N repos`, and the Changes card lists each repo with changes; nothing is blank because the root is not a repo. Pinned by Task 2 `a_non_git_root_with_child_repos_is_never_blank` and `the_badge_sums_every_repo`, Task 6 `the_head_label_is_a_hash_or_a_repo_count`, Task 16 `only_repos_with_changes_get_a_line_in_repo_order`.
2. **Usage stored before the cache split, and totals after it.** Expected: `session.usage` still lists old events after the upgrade, and token totals and Claude's ctx % show the same numbers as before (cache moved out of `input`, not lost). Pinned by Task 4 `session_usage_lists_events_stored_before_the_cache_split` and `totals_count_cached_tokens_and_a_split_event_replaces_an_unsplit_one`; omp by Task 5 `omp_transcripts_are_found_in_slug_folders`.
3. **Pin, color and plan link through the actor's next save and a daemon restart (resume makes a new session id).** Expected: none of them is lost. Pinned by Task 12 `sessions_are_colored_live_and_exited_and_the_name_is_checked`, Task 13 `sessions_are_pinned_live_and_exited_and_keep_the_pin_through_saves` and `workspace_and_tab_pins_persist_across_daemon_restart`, Task 17 `a_linked_plan_is_saved_with_the_session` (each task also extends the resume test it names).
4. **Ctrl+Click on real terminal text.** Expected: Windows drive paths keep their colon, trailing punctuation / quotes / brackets are not part of the link, wide characters do not shift the hit cell, and a path outside the workspace or a missing file is not a link. Pinned by Task 20 `windows_paths_keep_their_drive_colon`, `quotes_brackets_and_punctuation_are_not_part_of_the_path`, `wide_chars_take_two_cells_and_none_of_the_text`, `only_existing_files_inside_the_folder_resolve`.
5. **Unsaved edits in Files while switching Preview / Source or opening a file at a line.** Expected: no edit is lost, the preview shows the unsaved text, and a late preview build never replaces a newer one. Pinned by Task 21 `unsaved_edits_survive_the_toggle_and_the_preview_shows_them` and `a_late_preview_never_replaces_a_newer_one`, Task 20 `a_line_in_the_shown_file_applies_at_once_and_keeps_unsaved_text`.

The custom title bar (drag, double-click, Snap Layouts on □, resize at every edge and DPI) cannot be unit-tested; Task 3's HUMAN_CHECKS lines cover it.

---

### Task 1: Run button colors: Start green, Restart amber, Stop red

Inbox: line 39 `1. Title bar + toolbar redesign` (bullet "Run button colors", Tasks 1–3; do not tick yet). Spec: §10 (SPEC.md:730-747), §14 (:1011-1042, :1074).

**Current state (investigated):**
- `main.rs` `Shell::render_run_button`: idle `run-main` and `run-menu ▾` are `button(.., primary = true)`, so they are accent filled. While live, `run-restart` `↻` and `run-stop` `■` are ghost `button(id, label, theme, false)`: muted text, hover `theme.hover`. There is no per-action color.
- `Shell::render_run_menu`: the row controls (`↻ ■` or `▶`) are plain divs that inherit the default text color. Their hover is `theme.surface_raised`, which is also the panel background, so the hover is invisible in both themes.
- `RunAction { Start, Restart, Stop }` is in `crates/mkrew-ui/src/run.rs:12`. `run.rs` is the pure module of the Run button and has a test module (`mod tests`, `use super::*`).
- Tokens that already exist (`theme.rs`): green `status_done` (dark 0x4CB782 / light 0x2F855A), amber `status_needs_input` (0xE3A63E / 0xB7791F), red `status_exited` (0xE5676B / 0xC53030). Faint fills exist for green (`success_bg`) and amber (`warning_bg`). Red has none except the diff-only `diff_del_bg`, so this task adds `danger_bg`.
- Measured contrast (WCAG, computed with `theme::contrast`'s formula):

  | fg on bg | dark bg / hover / surface_raised / tint | light bg / hover / surface_raised / tint |
  | --- | --- | --- |
  | `status_done` (tint `success_bg`) | 7.1 / 6.2 / 5.4 / 4.9 | 4.5 / 4.0 / 4.0 / 4.0 |
  | `status_needs_input` (tint `warning_bg`) | 8.3 / 7.2 / 6.3 / 6.0 | 3.6 / 3.2 / 3.2 / 3.3 |
  | `status_exited` (tint `danger_bg` 0x3A2327 / 0xFDECEC) | 5.5 / 4.8 / 4.2 / 4.5 | 5.5 / 4.8 / 4.8 / 4.8 |

  All are ≥ 3:1 (graphics/large-glyph threshold). Dark `on_accent` on `status_done` is 7.1 and light is 4.5, so the idle Run button can be a green fill with `on_accent` text.
- Decision: the hover state of a colored control is its faint tint (`success_bg` / `warning_bg` / `danger_bg`), and the glyph keeps its color. The menu's `theme.border` hover cannot be used: light amber on it is 2.9:1.

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (field `danger_bg` in `Theme` after `success_bg`, `dark()`, `light()`, `shared()`)
- Modify: `crates/mkrew-ui/src/run.rs` (`ActionColors`, `action_colors`, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (`render_run_button`, `render_run_menu`, two new helper fns next to `button`)
- Modify: `docs/SPEC.md` §10 (:730-731), §14 token table (:1032) and :1074
- Modify: `docs/HUMAN_CHECKS.md` (creates the shared section)

**Interfaces:**
- Produces: `Theme.danger_bg: u32`.
- Produces: `run::ActionColors { pub fg: u32, pub tint: u32 }` and `run::action_colors(action: RunAction, theme: &Theme) -> ActionColors` (Start → `status_done` / `success_bg`, Restart → `status_needs_input` / `warning_bg`, Stop → `status_exited` / `danger_bg`).
- Produces: in `main.rs`, `fn run_icon_button(id: &'static str, label: &'static str, action: RunAction, theme: &Theme) -> gpui::Stateful<gpui::Div>` and `fn run_fill_button(id: &'static str, label: impl Into<SharedString>, theme: &Theme) -> gpui::Stateful<gpui::Div>`.

- [ ] **Step 1: Failing tests**

In `run.rs` `mod tests`, add at the end:

```rust
    /// Start is green, Restart amber, Stop red, each with its own faint hover fill.
    #[test]
    fn run_actions_are_green_amber_red() {
        for theme in [Theme::dark(), Theme::light()] {
            let pairs = [
                (RunAction::Start, theme.status_done, theme.success_bg),
                (RunAction::Restart, theme.status_needs_input, theme.warning_bg),
                (RunAction::Stop, theme.status_exited, theme.danger_bg),
            ];
            for (action, fg, tint) in pairs {
                let colors = action_colors(action, &theme);
                assert_eq!((colors.fg, colors.tint), (fg, tint), "{action:?}");
            }
        }
    }

    /// The glyph stays visible on every surface it sits on (title bar, menu panel, its own
    /// hover fill), and the idle green Run button keeps readable text.
    #[test]
    fn run_action_colors_are_visible_in_both_themes() {
        use crate::theme::contrast;
        for theme in [Theme::dark(), Theme::light()] {
            for action in [RunAction::Start, RunAction::Restart, RunAction::Stop] {
                let colors = action_colors(action, &theme);
                for bg in [theme.bg, theme.hover, theme.surface_raised, colors.tint] {
                    let ratio = contrast(colors.fg, bg);
                    assert!(ratio >= 3.0, "{action:?} {:06X} on {bg:06X}: {ratio:.2}", colors.fg);
                }
            }
            let fill = action_colors(RunAction::Start, &theme).fg;
            assert!(contrast(theme.on_accent, fill) >= 3.0);
            // A hovered tint differs from the menu panel it sits on.
            for action in [RunAction::Start, RunAction::Restart, RunAction::Stop] {
                assert_ne!(action_colors(action, &theme).tint, theme.surface_raised);
            }
        }
    }
```

Run: `cargo test -p mkrew-ui --lib run::tests::run_action -j 4`
Expected: FAIL to compile (`action_colors` and `danger_bg` do not exist).

- [ ] **Step 2: Token `danger_bg`**

`theme.rs` `Theme`, after `pub success_bg: u32,`:

```rust
    /// Faint error fill (hover of a Stop control); the red twin of `warning_bg` / `success_bg`.
    pub danger_bg: u32,
```

`dark()` after `success_bg: 0x1D3A31,`: `danger_bg: 0x3A2327,`
`light()` after `success_bg: 0xE1F5EC,`: `danger_bg: 0xFDECEC,`
`shared()` after `success_bg: 0,`: `danger_bg: 0,`

- [ ] **Step 3: `action_colors`**

`run.rs`, imports: add `use crate::theme::Theme;`. After `RunAction`:

```rust
/// The glyph color of a Run control and the faint fill it shows on hover: Start green,
/// Restart amber, Stop red, in the title bar button and in the ▾ menu alike (SPEC §10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionColors {
    pub fg: u32,
    pub tint: u32,
}

pub fn action_colors(action: RunAction, theme: &Theme) -> ActionColors {
    let (fg, tint) = match action {
        RunAction::Start => (theme.status_done, theme.success_bg),
        RunAction::Restart => (theme.status_needs_input, theme.warning_bg),
        RunAction::Stop => (theme.status_exited, theme.danger_bg),
    };
    ActionColors { fg, tint }
}
```

Run: `cargo test -p mkrew-ui --lib run::tests::run_action -j 4`
Expected: PASS.

- [ ] **Step 4: Render with the colors**

`main.rs`, after `fn button(...)` (find it with `grep -n "^fn button("`), add:

```rust
/// The idle Run button: a green fill (`Start`'s color) with `on_accent` text, like a primary
/// [`button`] in green.
fn run_fill_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    button_frame(id, label)
        .bg(rgb(run::action_colors(RunAction::Start, theme).fg))
        .text_color(rgb(theme.on_accent))
        .font_weight(FontWeight(weight::MEDIUM))
        .hover(|style| style.opacity(opacity::HOVER))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A ghost glyph button of a live Run: the glyph in the action's color, its tint on hover.
fn run_icon_button(
    id: &'static str,
    label: &'static str,
    action: RunAction,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let colors = run::action_colors(action, theme);
    button_frame(id, label)
        .text_color(rgb(colors.fg))
        .hover(move |style| style.bg(rgb(colors.tint)))
        .active(|style| style.opacity(opacity::ACTIVE))
}
```

`render_run_button`: replace

```rust
        let main = button("run-main", summary.label(), theme, !live)
            .flex_none()
```
with
```rust
        let main = if live {
            button("run-main", summary.label(), theme, false)
        } else {
            run_fill_button("run-main", summary.label(), theme)
        }
        .flex_none()
```
(keep the following `.when(!live, …)`, `.when(summary.warning(), …)`, `.tooltip`, `.on_click` lines unchanged).

In the `if live { for (id, label, tip, action) in [...] {` loop replace `button(id, label, theme, false)` with `run_icon_button(id, label, action, theme)`.

Replace `let menu = button("run-menu", "▾", theme, !live).when(!live, |el| {` with
```rust
        let menu = if live {
            button("run-menu", "▾", theme, false)
        } else {
            run_fill_button("run-menu", "▾", theme)
        }
        .when(!live, |el| {
```

`render_run_menu`: delete `let hover = theme.surface_raised;`. In the `for &(label, tip, action) in controls {` loop, replace
```rust
                let name = row.name.clone();
                line = line.child(
                    div()
                        .id(SharedString::from(format!("run-{label}-{}", row.name)))
                        .px(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
```
with
```rust
                let name = row.name.clone();
                let colors = run::action_colors(action, theme);
                line = line.child(
                    div()
                        .id(SharedString::from(format!("run-{label}-{}", row.name)))
                        .px(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .text_color(rgb(colors.fg))
                        .hover(move |style| style.bg(rgb(colors.tint)))
```
(`.hover` is set once; the panel's own children keep no other hover.)

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors. (`run::action_colors` is reachable because `main.rs` already has `use mkrew_ui::run::{self, …}`.)

- [ ] **Step 5: SPEC**

§10 (:730-731), replace `a split accent button:\n  `[▶ Run │▾]` with` by:

```markdown
a split button, **green** while idle (`status-done` fill, `on-accent` text):
  `[▶ Run │▾]` with
```
and after the sentence ending `…`Run setup…`.` (:736-737) add:

```markdown
  Colors (same in the title bar and in the menu, glyph and hover alike): Run / Start green
  (`status-done`, hover `success-bg`), Restart ↻ amber (`status-needs-input`, hover
  `warning-bg`), Stop ■ red (`status-exited`, hover `danger-bg`).
```

§14 token table, after the `warning-bg` / `success-bg` row:

```markdown
| `danger-bg` | #3A2327 | #FDECEC |
```
and line :1074, append to the design check list: `Run actions: Start / Restart / Stop glyphs ≥ 3:1 on every surface they sit on (contrast tests).` (keep the sentence form of the existing list: `…status never conveyed by color alone; run action glyphs ≥ 3:1.`).

- [ ] **Step 6: HUMAN_CHECKS (creates the shared section)**

In `docs/HUMAN_CHECKS.md`, directly above the `### To verify — 2026-10-09 inbox: …` heading (line 18), add:

```markdown
### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)

- [ ] **Run colors:** in light and dark, with a workspace that has two run commands:
  - Idle: `▶ Run` and `▾` are green (text readable). Hover dims them slightly.
  - Running: `↻` is amber and `■` red; hovering each shows a faint amber / red fill and keeps the glyph color.
  - Open `▾`: the row `↻` is amber, `■` red, `▶` green; hovering shows the same tints (visible on the panel in both themes).

```

- [ ] **Step 7: Verify and tick**

Run: `cargo test -p mkrew-ui --lib run theme -j 4` (expect PASS), then the full verification. Do not tick inbox line 39 yet (Tasks 2 and 3 follow).
Suggested message: `feat(ui): Run controls are green / amber / red, with matching hover fills`

---

### Task 2: Git summary of the shown workspace (1 or N repos)

Inbox: line 39 (bullets "Git branch + `Git N` badge" and "Perf"; do not tick yet). Spec: §12B (SPEC.md:857-910), ADR 0029.

**Current state (investigated):**
- `git.status` (`Request::GitStatus(GitStatusParams)`, protocol `files.rs:44`) answers for ONE repo: `branch: Option<String>` (None = detached), `head: Option<String>` (full hash, None = no commit), and the file lists `staged / unstaged / untracked` (Working mode). No reply carries a count. `repo: Some(name)` addresses a direct child repo; `None` = the workspace folder (ADR 0029).
- `git.repos` (`Request::GitRepos(ProjectRef)` → `Vec<String>`): `""` first when the workspace folder is itself in a repo, then each direct child repo. A folder with no `.git` and no children answers `[]`; `git.status` of such a root fails (`NotARepo`).
- UI today: `Shell.git: HashMap<Ulid, GitView>` holds one repo, mode- and target-dependent (Base mode leaves `staged/unstaged/untracked` empty), so it cannot feed a badge. `Shell.git_repos` is filled only when the Git view is shown (`load_git_repos`). `Shell::git_event` returns early unless the Git view is on screen, so nothing follows `git.changed` while another view shows. A `git.status` makes the daemon watch the workspace (`watched_target` → `keep_watched`), so `git.changed` flows once the summary asked.
- `activate_workspace` (main.rs, `fn activate_workspace`) is the single place every workspace switch passes through. `GitChanged` is dispatched in `Shell::dispatch` (`if matches!(event, Event::GitChanged(_)) { self.files_event(..); self.git_event(..); }`).
- `git_history::short_hash(&str) -> &str` exists (7 characters). `git_view.rs` tests have a `file(path, kind)` helper to copy.
- `main.rs:2460` has a ponytail note: a root without `.git` flashes "Cannot read the git status" when status runs before `git.repos`. The summary loader asks `git.repos` first, so it never hits `NotARepo`.

**Files:**
- Create: `crates/mkrew-ui/src/git_summary.rs` (pure, with tests)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod git_summary;` after `pub mod git_history;`)
- Modify: `crates/mkrew-ui/src/main.rs` (import `GitStatusParams`, `git_summary::{GitSummary, RepoSummary}`; fields; `load_git_summary`; `git_summary_of`; accessor; hooks in `activate_workspace`, the `GitChanged` dispatch and the disconnect reset)

**Interfaces:**
- Consumes: `mkrew_protocol::GitStatus`, `git_history::short_hash`.
- Produces (exactly as in the shared contract):
  ```rust
  pub struct RepoSummary { pub repo: String, pub branch: Option<String>, pub head: Option<String>, pub modified: usize, pub untracked: usize }
  impl RepoSummary { pub fn of(repo: &str, status: &GitStatus) -> Self; pub fn changed(&self) -> usize }
  #[derive(Default)] pub struct GitSummary { pub repos: Vec<RepoSummary> }
  impl GitSummary { pub fn changed(&self) -> usize; pub fn branch_label(&self) -> Option<String>; pub fn branch_tooltip(&self) -> Option<String> }
  ```
  (`GitSummary` and `RepoSummary` also derive `Debug, Clone, PartialEq, Eq`.)
- Produces: `Shell::git_summary(&self) -> Option<&GitSummary>` (None unless it belongs to the active workspace) and `Shell::load_git_summary(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>)`, which calls `cx.notify()` when the summary changed. Task 6 adds `head_label` later; this task does not.

- [ ] **Step 1: Failing tests (pure module)**

Create `crates/mkrew-ui/src/git_summary.rs` with the tests only first (`mod tests` at the end of the file in Step 2; write it now in a scratch buffer, or create the file with just this module and `use super::*;` so the compile error is the expected one):

```rust
#[cfg(test)]
mod tests {
    use mkrew_protocol::{ChangeKind, ChangedFile, GitStatus};

    use super::*;

    fn file(path: &str, kind: ChangeKind) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            old_path: None,
            kind,
            added: Some(1),
            removed: Some(0),
        }
    }

    fn status(branch: Option<&str>, head: Option<&str>) -> GitStatus {
        GitStatus {
            branch: branch.map(str::to_owned),
            head: head.map(str::to_owned),
            ..GitStatus::default()
        }
    }

    fn summary(repos: &[(&str, GitStatus)]) -> GitSummary {
        GitSummary {
            repos: repos.iter().map(|(name, s)| RepoSummary::of(name, s)).collect(),
        }
    }

    #[test]
    fn one_repo_label_is_its_branch() {
        let s = summary(&[("", status(Some("master"), Some("1a2b3c4d5e6f")))]);
        assert_eq!(s.branch_label().as_deref(), Some("⎇ master"));
        assert_eq!(s.branch_tooltip().as_deref(), Some("(root) — master · 1a2b3c4"));
    }

    #[test]
    fn many_repos_label_counts_them_and_the_tooltip_lists_each() {
        let s = summary(&[
            ("fe", status(Some("main"), Some("1a2b3c4d5e6f"))),
            ("be", status(Some("feature/x"), Some("9f8e7d6c5b4a"))),
        ]);
        assert_eq!(s.branch_label().as_deref(), Some("⎇ 2 repos"));
        assert_eq!(
            s.branch_tooltip().as_deref(),
            Some("fe — main · 1a2b3c4\nbe — feature/x · 9f8e7d6")
        );
    }

    /// The workspace folder has no `.git` (git.repos has no `""`), but two children do: the
    /// chip and the badge must still show.
    #[test]
    fn a_non_git_root_with_child_repos_is_never_blank() {
        let mut dirty = status(Some("main"), Some("aaaaaaaa"));
        dirty.unstaged = vec![file("a.rs", ChangeKind::Modified)];
        dirty.untracked = vec![file("n.txt", ChangeKind::Untracked)];
        let s = summary(&[("fe", dirty), ("be", status(Some("dev"), Some("bbbbbbbb")))]);
        assert_eq!(s.branch_label().as_deref(), Some("⎇ 2 repos"));
        assert_eq!(s.changed(), 2);
        // One child only: its own branch, not "1 repos".
        let one = summary(&[("be", status(Some("dev"), Some("bbbbbbbb")))]);
        assert_eq!(one.branch_label().as_deref(), Some("⎇ dev"));
    }

    #[test]
    fn the_badge_sums_every_repo() {
        let mut a = status(Some("main"), Some("aaaaaaaa"));
        a.staged = vec![file("a.rs", ChangeKind::Modified), file("b.rs", ChangeKind::Added)];
        let mut b = status(Some("main"), Some("bbbbbbbb"));
        b.unstaged = vec![file("c.rs", ChangeKind::Deleted)];
        b.untracked = vec![file("d.txt", ChangeKind::Untracked)];
        let s = summary(&[("fe", a), ("be", b)]);
        assert_eq!(s.repos[0].changed(), 2);
        assert_eq!(s.repos[1].changed(), 2);
        assert_eq!(s.changed(), 4);
    }

    /// A file staged and then edited again is in both lists: one changed file.
    #[test]
    fn staged_and_unstaged_changes_of_one_path_count_once() {
        let mut st = status(Some("main"), Some("aaaaaaaa"));
        st.staged = vec![file("a.rs", ChangeKind::Modified)];
        st.unstaged = vec![file("a.rs", ChangeKind::Modified), file("b.rs", ChangeKind::Deleted)];
        st.untracked = vec![file("new.txt", ChangeKind::Untracked)];
        let repo = RepoSummary::of("", &st);
        assert_eq!((repo.modified, repo.untracked, repo.changed()), (2, 1, 3));
    }

    #[test]
    fn a_detached_head_still_has_a_label() {
        let s = summary(&[("", status(None, Some("1a2b3c4d5e6f")))]);
        assert_eq!(s.branch_label().as_deref(), Some("⎇ detached"));
        assert_eq!(s.branch_tooltip().as_deref(), Some("(root) — detached HEAD · 1a2b3c4"));
        let fresh = summary(&[("", status(Some("main"), None))]);
        assert_eq!(fresh.branch_tooltip().as_deref(), Some("(root) — main · no commits"));
    }

    #[test]
    fn no_repository_means_no_chip_and_no_badge() {
        let s = GitSummary::default();
        assert_eq!((s.branch_label(), s.branch_tooltip(), s.changed()), (None, None, 0));
    }
}
```

Run: `cargo test -p mkrew-ui --lib git_summary -j 4`
Expected: FAIL to compile (`RepoSummary`, `GitSummary` not defined; add `pub mod git_summary;` to `lib.rs` first so the module is found).

- [ ] **Step 2: Implement the pure module**

Put above the tests in `git_summary.rs`:

```rust
//! The git state of the shown workspace for the title bar (SPEC §10, ADR 0029): the branch
//! chip (`⎇ master` / `⎇ 2 repos`) and the `Git N` badge. One `RepoSummary` per repository of
//! the workspace folder, reduced from its `git.status` (Working mode). Pure; `main.rs` loads it.

use std::collections::HashSet;

use mkrew_protocol::GitStatus;

use crate::git_history::short_hash;

/// How the workspace folder's own repository is named in the tooltip.
const ROOT_NAME: &str = "(root)";

/// One repository of a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RepoSummary {
    /// `""` = the workspace folder itself, else the child folder name (ADR 0029).
    pub repo: String,
    /// `None` = detached HEAD / unknown.
    pub branch: Option<String>,
    /// Full HEAD hash; `None` = no commit yet.
    pub head: Option<String>,
    /// Distinct paths in staged ∪ unstaged.
    pub modified: usize,
    pub untracked: usize,
}

impl RepoSummary {
    pub fn of(repo: &str, status: &GitStatus) -> Self {
        let modified = status
            .staged
            .iter()
            .chain(&status.unstaged)
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>()
            .len();
        Self {
            repo: repo.to_owned(),
            branch: status.branch.clone(),
            head: status.head.clone(),
            modified,
            untracked: status.untracked.len(),
        }
    }

    /// Changed files of this repository.
    pub fn changed(&self) -> usize {
        self.modified + self.untracked
    }

    /// `fe — main · 1a2b3c4`.
    fn line(&self) -> String {
        let name = if self.repo.is_empty() { ROOT_NAME } else { &self.repo };
        let branch = self.branch.as_deref().unwrap_or("detached HEAD");
        match self.head.as_deref() {
            Some(head) => format!("{name} — {branch} · {}", short_hash(head)),
            None => format!("{name} — {branch} · no commits"),
        }
    }
}

/// Every repository of the shown workspace, in `git.repos` order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitSummary {
    pub repos: Vec<RepoSummary>,
}

impl GitSummary {
    /// Changed files over all repositories: the `Git N` badge.
    pub fn changed(&self) -> usize {
        self.repos.iter().map(RepoSummary::changed).sum()
    }

    /// `⎇ master` for one repository, `⎇ 2 repos` for several; `None` when there is none.
    pub fn branch_label(&self) -> Option<String> {
        match self.repos.as_slice() {
            [] => None,
            [one] => Some(format!("⎇ {}", one.branch.as_deref().unwrap_or("detached"))),
            many => Some(format!("⎇ {} repos", many.len())),
        }
    }

    /// One line per repository; `None` when there is none.
    pub fn branch_tooltip(&self) -> Option<String> {
        if self.repos.is_empty() {
            return None;
        }
        Some(self.repos.iter().map(RepoSummary::line).collect::<Vec<_>>().join("\n"))
    }
}
```

Run: `cargo test -p mkrew-ui --lib git_summary -j 4`
Expected: PASS (7 tests).

- [ ] **Step 3: Shell state and loader**

`main.rs` imports: add `GitStatusParams` to the `mkrew_protocol::{…}` list and `use mkrew_ui::git_summary::{GitSummary, RepoSummary};` after `use mkrew_ui::git_history::…`.

Fields in `Shell`, after `git_repos`:

```rust
    /// The title bar's branch chip and `Git N` badge: the shown workspace only (hidden ones
    /// are never loaded), in memory only.
    git_summary: Option<(Ulid, GitSummary)>,
    /// A summary request is in flight / another change arrived meanwhile.
    git_summary_busy: bool,
    git_summary_again: bool,
```
In `Shell::new`, after `git_repos: HashMap::new(),`:
```rust
            git_summary: None,
            git_summary_busy: false,
            git_summary_again: false,
```
Disconnect reset (next to `shell.git.clear();`): `shell.git_summary = None;`

Accessor and loader, after `load_git_repos`:

```rust
    /// The summary of the shown workspace (None until loaded, and for other workspaces).
    fn git_summary(&self) -> Option<&GitSummary> {
        self.git_summary
            .as_ref()
            .filter(|(id, _)| Some(*id) == self.workspace)
            .map(|(_, summary)| summary)
    }

    /// `git.repos`, then one `git.status` per repository, for the shown workspace (title bar).
    /// Asked on a workspace switch and on its `git.changed`, never polled and never for a
    /// hidden workspace. One request set at a time: a change that arrives meanwhile asks once
    /// more when it answers.
    fn load_git_summary(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.git_summary_busy {
            self.git_summary_again = true;
            return;
        }
        self.git_summary_busy = true;
        cx.spawn_in(window, async move |this, cx| {
            let loaded = git_summary_of(&connection, project_id).await;
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.git_summary_busy = false;
                let again = std::mem::take(&mut shell.git_summary_again);
                let shown = shell.workspace;
                match loaded {
                    // An answer for a workspace that is no longer shown is dropped.
                    Ok(summary) if shown == Some(project_id) => {
                        let same = matches!(
                            &shell.git_summary,
                            Some((id, old)) if *id == project_id && *old == summary
                        );
                        if !same {
                            shell.git_summary = Some((project_id, summary));
                            cx.notify();
                        }
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(%project_id, "git summary failed: {error:#}"),
                }
                if let Some(shown) = shown
                    && (again || shown != project_id)
                {
                    shell.load_git_summary(shown, window, cx);
                }
            });
        })
        .detach();
    }
```

Free function, next to `fn detach(...)`:

```rust
/// The repositories of `project_id` and each one's Working-mode status. Asking `git.repos`
/// first means a root without `.git` never sends the status that would fail. The status
/// requests go out together; a repository whose status fails is left out (logged).
async fn git_summary_of(connection: &Connection, project_id: Ulid) -> anyhow::Result<GitSummary> {
    let names = connection
        .call::<Vec<String>>(Request::GitRepos(ProjectRef { project_id }))
        .await?;
    let pending: Vec<_> = names
        .into_iter()
        .map(|name| {
            let params = GitStatusParams {
                project_id,
                worktree_task_id: None,
                repo: (!name.is_empty()).then(|| name.clone()),
                mode: GitMode::Working,
                since: None,
            };
            let status = connection.call::<GitStatus>(Request::GitStatus(params));
            (name, status)
        })
        .collect();
    let mut repos = Vec::new();
    for (name, status) in pending {
        match status.await {
            Ok(status) => repos.push(RepoSummary::of(&name, &status)),
            Err(error) => tracing::warn!(%project_id, repo = name, "git.status failed: {error:#}"),
        }
    }
    Ok(GitSummary { repos })
}
```

Hooks. `activate_workspace`, after `self.workspace = Some(project_id);`:

```rust
        if switched {
            // Nothing is kept for the workspace that is no longer shown.
            self.git_summary = None;
        }
        if switched || self.git_summary.is_none() {
            self.load_git_summary(project_id, window, cx);
        }
```
`Shell::dispatch` GitChanged block, add after `self.git_event(event, window, cx);`:

```rust
                    if let Event::GitChanged(changed) = event
                        && changed.worktree_task_id.is_none()
                        && self.workspace == Some(changed.project_id)
                    {
                        self.load_git_summary(changed.project_id, window, cx);
                    }
```
(A task-worktree change does not alter the workspace folder's repositories, so it is skipped.)

Run: `cargo check -p mkrew-ui -j 4` and `cargo test -p mkrew-ui --lib git_summary -j 4`
Expected: PASS, no warnings (the accessor is used by Task 3; until then it may warn `dead_code`: Task 3 lands right after, so run clippy only after Task 3).

- [ ] **Step 4: HUMAN_CHECKS (smoke after Task 3)**

Added in Task 3 (the chip and badge are only visible there).

- [ ] **Step 5: Verify and tick**

Run: `cargo test -p mkrew-ui --lib git_summary -j 4` (PASS). Run the full verification after Task 3 (dead-code warning until the title bar reads the accessor). Do not tick inbox line 39 yet.
Suggested message: `feat(ui): git summary (branch, changed files) of every repo of the shown workspace`

---

### Task 3: Custom title bar replaces the OS title bar and the toolbar (ADR `0034`)

Inbox: line 39 `1. Title bar + toolbar redesign` (last task of the item: tick it). Spec: §10 (SPEC.md:673-747), §14 (:1068), §13 (nothing changes), ADR 0027 (amended), ADR 0029.

**Current state (investigated):**
- `main()` (main.rs:140-151) opens the window with `TitlebarOptions { title: Some("Mkrew"), ..Default::default() }`: the OS draws a title bar. No `window_min_size`, no `window_decorations`.
- `Shell::render_main` builds a column `[render_toolbar(..), view body]`. `render_toolbar` (44 px, `theme::TOOLBAR_HEIGHT`) holds the views segmented control, the theme toggle, History, + Agent, + Shell and `render_run_button`; its only caller is `render_main`. The sidebar has its own header of the same height (`render_sidebar`: logo, "Workspaces", `+`), so the two borders line up today.
- `impl Render for Shell` returns a ROW root (`div().size_full().flex()`): `[render_sidebar, render_main]`, then the dialogs as children. `Shell::modal_shade` is `.absolute().inset_0()`, so it covers whatever the root's box is: with a column root the shade still covers the whole window, title bar included.
- When there is no connection/workspace, `render_main` returns just the status message: no toolbar. The window must stay movable and closable in that state, so the title bar has to render without a workspace.
- GPUI (`gpui-pre-0.3.7`, verified in the vendored source):
  - `WindowControlArea { Drag, Close, Max, Min }`, fluent `.window_control_area(area)` (`elements/div.rs:1251`). Hit-test callback `window.rs:1947-1960` returns the FIRST registered area whose hitbox the mouse is over, in paint order. A `Drag` ancestor would therefore swallow `Max`/`Min`/`Close` children: the `Drag` element must be a sibling, never a parent, of the controls.
  - Windows backend (`gpui-pre-windows-0.3.7/src/events.rs`): `WM_NCHITTEST` maps `Drag`→`HTCAPTION`, `Max`→`HTMAXBUTTON` (this is what makes the Win11 Snap Layouts flyout appear on hover over □), `Min`→`HTMINBUTTON`, `Close`→`HTCLOSE` (:963-1020). Clicks on them are handled natively in `WM_NCLBUTTONUP` (:1101-1165): minimize, maximize/restore and `WM_CLOSE`. So these controls need **no `on_click`**. A double-click on `HTCAPTION` is `WM_NCLBUTTONDBLCLK` → `handle_nc_mouse_down_msg` → returns `None` when the app does not stop propagation → `DefWindowProc` toggles maximize.
  - With `TitlebarOptions.appears_transparent = true` (`hide_title_bar`) the backend removes the caption (`WM_NCCALCSIZE`, :780-801), and hit-tests the TOP resize edge itself (:995-1026); left, right and bottom resize edges stay with the system frame. While maximized it adds the frame thickness to the top, so nothing is clipped. Fullscreen: hit test returns `None`, so drawn controls would be dead (hidden in fullscreen).
  - macOS: `appears_transparent = true` keeps the native traffic lights top-left (`traffic_light_position`); the window is moved with `window.start_window_move()`, a double-click goes to `window.titlebar_double_click()` (honors the system "double-click a window's title bar to…" setting); `WindowOptions.app_owns_titlebar_drag = true` stops AppKit from delaying the click. `WindowControlArea` is a no-op there.
  - Linux: client-side decorations need resize edges, shadows, tiling insets and a window menu drawn by the app (`WindowDecorations::Client`, `start_window_resize`, `set_client_inset`); without a compositor X11 falls back to server decorations and the OS bar would show twice. The ADR picks the OS title bar on Linux (see Open question below).
- Existing pieces reused as they are: `logo(theme)`, `workspace_avatar(name, color, theme)`, `menu_item`, `icon_button`, `button`, `new_pane_button`, `render_run_button`, `tooltip`, `Shell::add_workspace`, `Shell::activate_workspace`, `Shell::show_view`, `Shell::toggle_theme`, `Shell::show_history`, the Run-menu popover pattern (`deferred(anchored().snap_to_window_with_margin(..))` + `on_mouse_down_out`), and Task 2's `Shell::git_summary()`.
- Hidden-workspace cost: none. The title bar reads `Shell::git_summary()` (cached); it never requests anything.

**Decision (ADR):**
- Windows and macOS: custom title bar (`appears_transparent`). Linux: keeps the OS bar; the new bar is then just a toolbar without drag handling or drawn controls.
- One 44 px bar (`TITLE_BAR_HEIGHT`, renamed from `TOOLBAR_HEIGHT`) replaces the OS bar (~32 px) and the toolbar (44 px).
- Sidebar header: the logo moves to the title bar; the header stays as `Workspaces` + `+`, at `WORKSPACE_ROW_HEIGHT` (30 px) instead of 44.
- Left to right: [macOS: 72 px for the traffic lights] logo · workspace name ▾ (switcher dropdown) · branch chip · views (`Git N` badge) · drag area · theme toggle · History · + Agent · + Shell · Run · [Windows: ─ □ ✕].
- Minimum window size 1100 × 560: the bar's contents fit; the workspace name and the branch truncate first.

**Files:**
- Create: `docs/decisions/0034-custom-title-bar.md`
- Modify: `docs/decisions/0027-calm-redesign.md` (header `Amended by` line + one consequence)
- Modify: `docs/SPEC.md` §10 (:675-703, :730), §14 (:1068), §20 settings comment (:1374)
- Modify: `crates/mkrew-ui/src/theme.rs` (consts, test `layout_heights_scale_with_the_ui_font`, new test)
- Modify: `crates/mkrew-ui/src/main.rs` (`main()` window options; `Shell.workspace_switcher`; `render_title_bar`, `render_view_tabs`, `render_workspace_switcher`, `render_branch_chip`, `window_control`; delete `render_toolbar`; `render_main`; `render_sidebar` header; `impl Render for Shell`; `activate_workspace`; disconnect reset; `new_pane_button` comment)
- Modify: `docs/HUMAN_CHECKS.md`, `docs/INBOX.md`

**Interfaces:**
- Consumes: `Shell::git_summary() -> Option<&GitSummary>`, `GitSummary::{changed, branch_label, branch_tooltip}` (Task 2); `run::action_colors` (Task 1, indirectly through `render_run_button`).
- Produces: `Shell::render_title_bar(&self, shown: View, theme: &Theme, window: &Window, cx: &mut Context<'_, Self>) -> impl IntoElement`. The root of `Render for Shell` becomes a column: title bar on top, then a row (sidebar | main). Later tasks (6 and others) add to that root: a status bar goes after the row.
- Produces: `theme::TITLE_BAR_HEIGHT` (replaces `TOOLBAR_HEIGHT`), `theme::WINDOW_CONTROL_WIDTH`, `theme::WINDOW_MIN_WIDTH`, `theme::WINDOW_MIN_HEIGHT`, `theme::MAC_TRAFFIC_LIGHTS_WIDTH`, `theme::TITLE_CHIP_MAX_WIDTH`.

- [ ] **Step 1: ADR `0034`**

Create `docs/decisions/0034-custom-title-bar.md` (follow `docs/decisions/0000-template.md` headings):

```markdown
# 0034 — Custom title bar replaces the OS title bar and the toolbar

- Date: 2026-10-10
- Status: accepted
- Milestone: inbox 2026-10-10
- Amends: 0027

## Context

ADR 0027 put a 44 px toolbar (views, History, + Agent, + Shell, Run) under the OS title bar.
Together they take ~76 px of height, the logo sits in the sidebar header, and the toolbar shows
neither the workspace nor its git state. The inbox asks for one bar: logo, workspace switcher,
branch, views, History, + Agent, + Shell, Run and drawn window controls. A folder holding
several repositories (ADR 0029) must show a branch and a `Git N` badge too.

## Options

1. Keep the OS title bar and only add the branch and the switcher to the toolbar — saves no
   height; the OS bar stays a dead strip.
2. One custom title bar on every platform, with client-side decorations on Linux — Linux needs
   resize edges, shadows, tiling insets and a window menu drawn by us, and X11 without a
   compositor falls back to the OS bar anyway; none of it can be tested here.
3. A custom title bar on Windows and macOS, the OS bar on Linux — the platforms GPUI gives
   everything for: on Windows `HTCAPTION` / `HTMAXBUTTON` / `HTMINBUTTON` / `HTCLOSE` through
   `WindowControlArea`, on macOS the native traffic lights.

## Decision

Option 3.

- The bar is 44 px (`TITLE_BAR_HEIGHT`) and holds, left to right: logo, workspace name ▾
  (a dropdown of the workspaces and "Add workspace…"), the branch chip, the views with the
  `Git N` badge, a drag area, the theme toggle, History, + Agent, + Shell, Run and the window
  controls. It shows without a workspace too (logo, drag area, controls), so the window can always
  be moved and closed.
- Windows: `TitlebarOptions.appears_transparent = true`. The drag area is a
  `WindowControlArea::Drag` (system move, double-click maximizes or restores). ─ □ ✕ are
  `WindowControlArea::Min` / `Max` / `Close` without click handlers: the native hit test
  (`HTMAXBUTTON`) is what shows the Windows 11 Snap Layouts flyout on □ and runs the action. The
  drag area is a sibling of the controls, never their parent (the first registered area wins a
  hit test). Resize edges come from GPUI's frame hit test and the system frame. The controls
  are not drawn in full screen (F11).
- macOS: `appears_transparent = true` keeps the native traffic lights (72 px reserved at the
  left); the drag area calls `Window::start_window_move` and, on a double-click,
  `Window::titlebar_double_click`; `app_owns_titlebar_drag = true`.
- Linux: the OS title bar stays and our bar is a toolbar below it (no drawn controls, no drag
  handling). Client-side decorations are not worth an untestable ~150 lines now.
- The minimum window size is 1100 × 560 so the bar never overflows; the workspace name and the
  branch truncate first.
- The branch chip reads a cached `GitSummary` of the shown workspace (1 repo: `⎇ master`; N
  repos: `⎇ N repos`, tooltip lists each; a root without `.git` but with child repos is never blank).
  It is loaded on a workspace switch and on `git.changed` of that workspace only: no polling,
  nothing for hidden workspaces.
- The sidebar header loses the logo and shrinks to 30 px (`WORKSPACE_ROW_HEIGHT`).
- Run controls are green (Start), amber (Restart) and red (Stop) in the bar and in the menu.

## Consequences

- SPEC §10 (layout diagram, "Title bar" bullet, Run button) and §14 (theme toggle) change; ADR
  0027's toolbar is part of the title bar now (0027 gets an "Amended by" line).
- Pressed (mouse-down) styling of ─ □ ✕ on Windows is the system's: only hover is drawn.
- Under the top 1–2 px of the Close button the system resize edge (`HTTOPRIGHT`) wins.
- A Linux user gets two stacked bars (OS bar + toolbar); `window_decorations` can switch to
  client-side later without touching the bar's content.
- Tests: the bar itself is checked by hand (HUMAN_CHECKS); the git summary behind the chip and
  badge is unit tested (`git_summary`).
```

`docs/decisions/0027-calm-redesign.md`: after `- Supersedes: 0023` add `- Amended by: 0034 (the toolbar became part of the custom title bar)`.

- [ ] **Step 2: Theme constants and their test**

`theme.rs`: replace `pub const TOOLBAR_HEIGHT: f32 = 44.0;` by

```rust
/// The custom title bar (ADR 0034; was the 44 px toolbar of ADR 0027).
pub const TITLE_BAR_HEIGHT: f32 = 44.0;
```
and next to `WINDOW_WIDTH` / `WINDOW_HEIGHT` (:95-96) add:

```rust
/// Smallest main window: the title bar's contents fit (ADR 0034).
pub const WINDOW_MIN_WIDTH: f32 = 1100.0;
pub const WINDOW_MIN_HEIGHT: f32 = 560.0;
/// One ─ □ ✕ button of the Windows title bar (the system caption button width).
pub const WINDOW_CONTROL_WIDTH: f32 = 46.0;
/// Room for the macOS traffic lights at the left of the title bar.
pub const MAC_TRAFFIC_LIGHTS_WIDTH: f32 = 72.0;
/// Longest workspace name / branch chip in the title bar before it truncates.
pub const TITLE_CHIP_MAX_WIDTH: f32 = 160.0;
```

Test `layout_heights_scale_with_the_ui_font`: replace `TOOLBAR_HEIGHT` with `TITLE_BAR_HEIGHT` and the comment's word "toolbar" with "title bar". Add after it:

```rust
    /// ✕ on the red hover of the Close button must stay readable in both themes.
    #[test]
    fn window_close_hover_is_readable() {
        for theme in [Theme::dark(), Theme::light()] {
            let ratio = contrast(theme.on_accent, theme.status_exited);
            assert!(ratio >= 3.0, "{ratio:.2}");
        }
    }
```

Run: `cargo test -p mkrew-ui --lib theme -j 4`
Expected: FAIL to compile in `main.rs` only if you build the bin; the lib test passes (`on_accent` on `status_exited` is 5.5 in both themes: dark 0x17181C on 0xE5676B, light white on 0xC53030). Main.rs still refers to `TOOLBAR_HEIGHT` until Step 5; do Steps 3–5 before building the bin.

- [ ] **Step 3: Window options**

`main.rs` `use gpui::{…}`: add `WindowControlArea` and `point` (if `point` is not imported yet). Add near the top consts:

```rust
/// Windows and macOS draw their own title bar (ADR 0034); elsewhere the OS bar stays.
const CUSTOM_TITLE_BAR: bool = cfg!(any(target_os = "windows", target_os = "macos"));
/// Only Windows draws ─ □ ✕ (macOS keeps its traffic lights).
const DRAWN_CONTROLS: bool = cfg!(target_os = "windows");
```

In `main()`, replace the `titlebar:` field and the `..Default::default()` of `WindowOptions` with:

```rust
            titlebar: Some(TitlebarOptions {
                title: Some("Mkrew".into()),
                appears_transparent: CUSTOM_TITLE_BAR,
                traffic_light_position: cfg!(target_os = "macos")
                    .then(|| point(px(12.0), px(14.0))),
            }),
            window_min_size: Some(size(
                px(theme::WINDOW_MIN_WIDTH),
                px(theme::WINDOW_MIN_HEIGHT),
            )),
            // macOS: we move the window from our own bar, so AppKit must not delay the click.
            app_owns_titlebar_drag: cfg!(target_os = "macos"),
            ..Default::default()
```
If `TitlebarOptions` has more fields than these three in `gpui-pre-0.3.7/src/platform.rs:2375`, append `..Default::default()` inside it.

- [ ] **Step 4: Switcher state**

`Shell` fields, after `run_menu`:
```rust
    /// The title bar's workspace dropdown is open.
    workspace_switcher: bool,
```
`Shell::new`, after `run_menu: None,`: `workspace_switcher: false,`. Disconnect reset, after `shell.run_menu = None;`: `shell.workspace_switcher = false;`. `activate_workspace`, right after `self.workspace = Some(project_id);`: `self.workspace_switcher = false;` (every way a workspace becomes active closes the dropdown).

- [ ] **Step 5: Render the title bar; delete the toolbar**

Delete `fn render_toolbar` entirely (its views code moves to `render_view_tabs` below). In `render_main` delete the line `let main = main.child(self.render_toolbar(project_id, view, theme, cx));` and change `let main = div().flex_1()…` so it stays `let main = div().flex_1().min_w_0().h_full().flex().flex_col();`. Fix the comment in `new_pane_button`: `// The toolbar shows it in every view…` → `// The title bar shows it in every view; the new pane is seen in Terminals.`

Add these methods in `impl Shell` where `render_toolbar` was:

```rust
    /// The title bar above everything (SPEC §10, ADR 0034): logo, workspace switcher,
    /// branch chip and views on the left; a drag area; the theme toggle, History, + Agent,
    /// + Shell and Run; on Windows ─ □ ✕. Without a workspace only the logo, the drag area and
    /// the controls show, so the window can always be moved and closed.
    fn render_title_bar(
        &self,
        shown: View,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let workspace = self.workspace.filter(|_| self.connection.is_some());
        let mode = match theme.mode {
            theme::ThemeMode::System => "Theme: System",
            theme::ThemeMode::Light => "Theme: Light",
            theme::ThemeMode::Dark => "Theme: Dark",
        };
        let mut left = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(div().flex_none().child(logo(theme)));
        if let Some(project_id) = workspace {
            left = left
                .child(self.render_workspace_switcher(project_id, theme, cx))
                .children(self.render_branch_chip(theme, cx))
                .child(self.render_view_tabs(shown, theme, cx));
        }
        // A sibling of the controls, never their parent: the first area a hit test finds wins.
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
            .pl(px(if cfg!(target_os = "macos") {
                theme::MAC_TRAFFIC_LIGHTS_WIDTH
            } else {
                space::M
            }))
            .bg(rgb(theme.bg))
            .border_b(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(left)
            .child(drag);
        let controls = DRAWN_CONTROLS && !window.is_fullscreen();
        let mut right = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::S))
            .when(!controls, |el| el.pr(px(space::M)))
            .child(
                icon_button("toggle-theme", theme::mode_glyph(theme.mode), theme)
                    .tooltip(tooltip(
                        theme,
                        mode,
                        self.keymap.label(Command::ToggleTheme),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.toggle_theme(window, cx);
                    })),
            );
        if let Some(project_id) = workspace {
            right = right
                .child(
                    button("history", "History", theme, false)
                        .flex_none()
                        .tooltip(tooltip(
                            theme,
                            "Logs of the workspace's finished sessions",
                            "",
                        ))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.show_history(project_id, window, cx);
                        })),
                )
                .child(new_pane_button(
                    "new-agent-pane",
                    "+ Agent",
                    SessionKind::Agent,
                    self.keymap.label(Command::SplitAgent),
                    theme,
                    cx,
                ))
                .child(new_pane_button(
                    "new-shell-pane",
                    "+ Shell",
                    SessionKind::Shell,
                    self.keymap.label(Command::SplitShell),
                    theme,
                    cx,
                ))
                .child(self.render_run_button(project_id, theme, cx));
        }
        bar = bar.child(right);
        if controls {
            // No click handlers: Windows hit-tests these areas (`HTMINBUTTON` / `HTMAXBUTTON` /
            // `HTCLOSE`) and runs the action itself; `HTMAXBUTTON` also brings the Snap
            // Layouts flyout.
            let max_glyph = if window.is_maximized() { "❐" } else { "□" };
            bar = bar.child(
                div()
                    .flex_none()
                    .h_full()
                    .flex()
                    .child(window_control("win-min", "─", WindowControlArea::Min, theme))
                    .child(window_control("win-max", max_glyph, WindowControlArea::Max, theme))
                    .child(window_control("win-close", "✕", WindowControlArea::Close, theme)),
            );
        }
        bar
    }

    /// The views as a segmented control; the Git one carries the `Git N` badge (N = changed
    /// files over all repositories of the workspace, hidden at 0).
    fn render_view_tabs(
        &self,
        shown: View,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let hover = theme.hover;
        let changed = self.git_summary().map_or(0, GitSummary::changed);
        div()
            .flex_none()
            .flex()
            .items_center()
            .p(px(theme::SEGMENT_PADDING))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.surface))
            .children(View::ALL.into_iter().map(|view| {
                let active = view == shown;
                div()
                    .id(SharedString::from(format!("view-{}", view.title())))
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .px(px(space::M))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .border(px(theme::BORDER_WIDTH))
                    .cursor_pointer()
                    .whitespace_nowrap()
                    .text_size(font_size::BODY)
                    .when(active, |el| {
                        el.bg(rgb(theme.surface_raised))
                            .border_color(rgb(theme.border))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!active, |el| {
                        el.border_color(gpui::transparent_black())
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                    })
                    .tooltip(tooltip(
                        theme,
                        view.title(),
                        self.keymap.label(view.command()),
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.show_view(view, window, cx);
                    }))
                    .child(view.title())
                    .when(view == View::Git && changed > 0, |el| {
                        el.child(
                            div()
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .bg(rgb(theme.accent_bg))
                                .text_size(font_size::META)
                                .text_color(rgb(theme.accent_text))
                                .child(changed.to_string()),
                        )
                    })
            }))
    }

    /// `⎇ master` / `⎇ 2 repos` (tooltip: one line per repository); a click shows the Git
    /// view. Nothing while the summary is not loaded or the folder holds no repository.
    fn render_branch_chip(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let summary = self.git_summary()?;
        let label = summary.branch_label()?;
        let tip = summary.branch_tooltip().unwrap_or_default();
        let hover = theme.hover;
        Some(
            div()
                .id("title-branch")
                .min_w_0()
                .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text_muted))
                .hover(move |style| style.bg(rgb(hover)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .tooltip(tooltip(theme, &tip, ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.show_view(View::Git, window, cx);
                }))
                .child(label)
                .into_any_element(),
        )
    }

    /// The workspace name with ▾: opens a dropdown of every workspace (click switches) and
    /// "Add workspace…".
    fn render_workspace_switcher(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let projects = self.sidebar.projects();
        let current = projects.iter().find(|project| project.id == project_id);
        let name = current.map_or("Workspace", |project| project.name.as_str());
        let color = current
            .and_then(|project| project.color.as_deref())
            .and_then(|name| theme.workspace_color(name));
        let hover = theme.hover;
        let button = div()
            .id("workspace-switch")
            .max_w(theme::rems_px(theme::TITLE_CHIP_MAX_WIDTH))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::S))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_size(font_size::BODY)
            .font_weight(FontWeight(weight::MEDIUM))
            .text_color(rgb(theme.text))
            .hover(move |style| style.bg(rgb(hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
            .tooltip(tooltip(
                theme,
                "Switch workspace",
                self.keymap.label(Command::NextWorkspace),
            ))
            // An open menu already closed on this mouse down (outside it): the click reopens it.
            .on_click(cx.listener(|shell, _, _, cx| {
                shell.workspace_switcher = true;
                cx.notify();
            }))
            .child(workspace_avatar(name, color, theme))
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(name.to_owned()),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("▾"),
            );
        let menu = self.workspace_switcher.then(|| {
            let rows = projects.iter().map(|project| {
                let id = project.id;
                let color = project
                    .color
                    .as_deref()
                    .and_then(|name| theme.workspace_color(name));
                menu_item(SharedString::from(format!("switch-{id}")), theme)
                    .child(workspace_avatar(&project.name, color, theme))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text))
                            .child(project.name.clone()),
                    )
                    .when(id == project_id, |el| {
                        el.child(div().text_color(rgb(theme.accent_text)).child("✓"))
                    })
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        cx.stop_propagation();
                        shell.activate_workspace(id, window, cx);
                    }))
            });
            let panel = div()
                .id("workspace-switch-panel")
                .w(px(theme::MENU_WIDTH))
                .flex()
                .flex_col()
                .p(px(space::XS))
                .rounded(px(radius::MEDIUM))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(theme.border))
                .bg(rgb(theme.surface_raised))
                .shadow_md()
                .font_family(theme.ui_font_family.clone())
                .occlude()
                .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                    shell.workspace_switcher = false;
                    cx.notify();
                }))
                .children(rows)
                .child(
                    div()
                        .h(px(theme::BORDER_WIDTH))
                        .my(px(space::XS))
                        .bg(rgb(theme.border)),
                )
                .child(
                    menu_item("switch-add".into(), theme)
                        .child(
                            div()
                                .flex_1()
                                .text_color(rgb(theme.text))
                                .child("Add workspace…"),
                        )
                        .on_click(cx.listener(|shell, _, window, cx| {
                            cx.stop_propagation();
                            shell.workspace_switcher = false;
                            shell.add_workspace(window, cx);
                        })),
                );
            deferred(
                anchored()
                    .snap_to_window_with_margin(px(space::S))
                    .child(div().pt(px(space::XS)).child(panel)),
            )
        });
        div().child(button).children(menu).into_any_element()
    }
```

Free function next to `icon_button`:

```rust
/// One ─ □ ✕ button of the Windows title bar. `area` makes Windows treat it as the system
/// caption button (hover shows here; the click and the Snap Layouts flyout are the system's).
fn window_control(
    id: &'static str,
    glyph: &'static str,
    area: WindowControlArea,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let close = matches!(area, WindowControlArea::Close);
    let (hover, on_hover) = (theme.hover, theme.on_accent);
    let red = theme.status_exited;
    div()
        .id(id)
        .window_control_area(area)
        .flex_none()
        .w(theme::rems_px(theme::WINDOW_CONTROL_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(font_size::BODY)
        .text_color(rgb(theme.text_muted))
        .hover(move |style| {
            if close {
                style.bg(rgb(red)).text_color(rgb(on_hover))
            } else {
                style.bg(rgb(hover))
            }
        })
        .child(glyph)
}
```

- [ ] **Step 6: Root as a column; sidebar header**

`impl Render for Shell`: replace

```rust
        div()
            .size_full()
            .flex()
            .bg(rgb(theme.bg))
```
with
```rust
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
```
and replace
```rust
            .child(self.render_sidebar(now, &theme, cx))
            .child(self.render_main(now, &theme, window, cx))
```
with
```rust
            .child(self.render_title_bar(self.view(), &theme, window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.render_sidebar(now, &theme, cx))
                    .child(self.render_main(now, &theme, window, cx)),
            )
```
(`self.view()` is the shown view; Board is the fallback it already returns without a workspace. The dialogs stay `.children(...)` of the root: they are `.absolute().inset_0()` and cover the whole window.)

`render_sidebar` header: `.h(theme::rems_px(theme::TOOLBAR_HEIGHT))` → `.h(theme::rems_px(theme::WORKSPACE_ROW_HEIGHT))`, and replace
```rust
                    .child(logo(theme))
                    .child(
```
with
```rust
                    .child(
```
(the logo is in the title bar now).

Run: `cargo check -p mkrew-ui -j 4`
Expected: no errors. Likely fix-ups if the compiler objects: `Project.color` is `Option<String>` (same as `WorkspaceRow.color`); `Window::is_fullscreen` exists (`window.rs` next to `toggle_fullscreen`); import `MouseDownEvent` (already imported) and `deferred`/`anchored` (already used by `render_run_menu`).

If the sidebar or main column no longer fills the height (`h_full` inside the new flex row), drop `.h_full()` from the `div()` roots of `render_sidebar` and `render_main`; flex stretch gives the same height.

- [ ] **Step 7: SPEC**

§10 (:675), replace `One window, two columns; the main area has a toolbar on top:` and the diagram (:677-688) with:

```markdown
One window. A title bar on top spans both columns (Windows and macOS draw it; Linux keeps the OS
title bar and shows the same bar under it, without window controls; ADR 0034):

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│ ◈ A api ▾  ⎇ master  [Board│Terminals│Docs│Files│Git 3]   ◐ History +Agent +Shell [▶ Run│▾]  ─ □ ✕ │
├──────────────┬────────────────────────────────────────────────────────────────────────┤
│ Workspaces + │ tab strip (Terminals only): [● tab ✕] [tab ✕] +                          │
│ ▾ A api  (•••2)│                                                                       │
│ │ ••• ✻ claude │  Todo │ Running │ Needs input │ …                                     │
│ │ ?   ⌬ codex  │  …or the terminal tabs/panes                                         │
│ ▸ W web     ? │  …or the Markdown review view                                         │
│ ▸ I infra     │                                                                       │
│ ┌ Inbox ────┐ │                                                                       │
│ │! 2 waiting│ │                                                                       │
└──────────────┴────────────────────────────────────────────────────────────────────────┘
```
```

Replace the **Toolbar** bullet (:700-703) with:

```markdown
- **Title bar** (44 px, ADR 0027, ADR 0034), left to right: the Mkrew logo; the workspace name
  ▾ (a dropdown of the workspaces, click switches, plus "Add workspace…"); the git chip
  `⎇ master` (one repository) or `⎇ N repos` (several; the tooltip lists each repository's
  branch and HEAD; a workspace folder without `.git` but with child repositories shows the
  children, ADR 0029; hidden when there is no repository), a click shows the Git view; the
  views Board / Terminals / Docs / Files / Git as a segmented control (tooltips show their
  shortcuts, §13), the Git one with a `Git N` badge (N = changed files summed over every
  repository, hidden at 0); then, right-aligned, the theme toggle, History, + Agent, + Shell and
  the split Run button, in every view (+ Agent / + Shell switch to Terminals first; their
  tooltips show Alt+Shift+A / Alt+Shift+S, §13). The git chip and badge come from `git.repos`
  and one `git.status` per repository, asked for the shown workspace only, on a workspace switch
  and on its `git.changed`; no polling, nothing for hidden workspaces. Window: the empty part is
  the drag area. **Windows:** double-click maximizes or restores, ─ □ ✕ at the right (hovering
  □ shows the Windows 11 Snap Layouts flyout), all edges resize; the controls are hidden in full
  screen. **macOS:** native traffic lights at the left, drag and double-click as the OS does.
  **Linux:** the OS title bar stays. Minimum window size 1100 × 560. Without a workspace only
  the logo, the drag area and the window controls show.
```

§10 Run button line (:730): `at the right end of the toolbar (ADR 0027)` → `at the right end of the title bar (ADR 0034)`. §14 (:1068): `cycled by the toolbar toggle` → `cycled by the title bar toggle`. §20 settings comment (:1374): `the toolbar toggle writes it` → `the title bar toggle writes it`. The `crates/mkrew-core/src/settings.rs` template comment keeps the word "toolbar" (a user-visible template; changing it is not needed).

- [ ] **Step 8: HUMAN_CHECKS (appended to the 2026-10-10 section)**

```markdown
- [ ] **Title bar, window (Windows):** `cargo run -p mkrew-ui` (or the installed app). One 44 px bar on top, no OS title bar:
  - Drag the empty part of the bar → the window moves. Double-click it → maximizes; again → restores.
  - Hover □ (also when maximized, then ❐) → the Windows 11 Snap Layouts flyout shows; pick a layout → the window snaps. Click □ → maximize / restore; ─ minimizes; ✕ closes (terminals keep running in the daemon); ✕ is red on hover.
  - Resize from all four edges and all four corners, including the top edge (a thin strip at the very top) and the top-right corner next to ✕. Try at 100 % and 150 % display scaling.
  - Drag it down from maximized → restores under the cursor. Win+Arrow keys work. Minimum size stops at 1100 × 560 with nothing overlapping; a long workspace name / branch truncate with `…`.
  - F11: full screen hides ─ □ ✕; F11 again brings them back.
  - Open a dialog (Ctrl+Shift+P palette, a task dialog): the shade covers the title bar too; Esc closes it.
  - With the daemon stopped (no workspace shown): the logo, drag area and ─ □ ✕ still show and work.
- [ ] **Title bar, macOS / Linux (if available):** macOS: traffic lights sit at the left (not over the logo), dragging the bar moves the window, double-click zooms or minimizes as System Settings says. Linux: the OS title bar is there and the Mkrew bar below it has no ─ □ ✕.
- [ ] **Workspace switcher:** click the workspace name ▾ → every workspace with its avatar, ✓ on the shown one; click another → switches (sidebar highlight, tabs, branch chip follow) and the dropdown closes; click outside / Esc-less outside click closes it; "Add workspace…" opens the folder picker. Alt+1/Alt+2 and sidebar clicks still switch and close an open dropdown.
- [ ] **Branch chip and `Git N` badge:**
  - A workspace that is one repo (branch `master`) → `⎇ master`; edit a file → `Git 1` appears within about a second; stage + edit the same file again → still `Git 1`; add an untracked file → `Git 2`; commit everything → the badge disappears.
  - A folder holding `fe/` and `be/` repos, no `.git` at the root → `⎇ 2 repos`; hovering lists `fe — main · 1a2b3c4` and `be — …`; change one file in each → `Git 2`; a click on the chip opens the Git view.
  - A folder with a single child repo → `⎇ <its branch>`. A folder with no repository → no chip and no badge (the bar is not blank otherwise).
  - Detached HEAD (`git checkout <hash>`) → `⎇ detached`.
  - Switch workspaces back and forth: each shows its own branch; with the Terminals view shown (Git view never opened), a commit made in a terminal updates the chip.
- [ ] **Layout after the title bar:** sidebar header is "Workspaces +" (no logo), the sidebar and the main area reach the window bottom, no clipped tab strip; the Peek popover, quick open and the palette still open centered and the Run ▾ menu and the workspace menu (⋯) still drop below their buttons.
```

- [ ] **Step 9: Verify and tick**

Run: `cargo test -p mkrew-ui --lib -j 4` (PASS), then the full verification (`fmt`, `clippy -D warnings`, `cargo test --workspace -j 4`). Smoke run the app and walk the HUMAN_CHECKS above that you can (Windows: drag, double-click, □ flyout, resize, ✕). Tick inbox line 39 `[x]` (all three tasks of item 1 are done).
Suggested message: `feat(ui): custom title bar with workspace switcher, branch chip and Git badge (ADR 0034)`

---

### Task 4: `session.usage` splits cache tokens and carries the model (protocol `33`), ADR `0035`

Inbox: line 47 `Usage data: omp usage, cache split, model` (with Task 5; do not tick yet). Spec: §20E.3 (SPEC.md:1586-1593, protocol note :1603-1604), §7 events (:554-562), profile example (:1390-1392).

**Current state (investigated):**
- `crates/mkrew-core/src/usage.rs`: `Usage { input, output, context }` is `Copy`. `claude_line` sums `input_tokens + cache_creation_input_tokens + cache_read_input_tokens` into `input` (the `seen` map holds `(input, output)` per `message.id`+`requestId`, replaced on a repeat); `context` = that same sum of the newest non-sidechain message, without output. `codex_line` (free fn) takes the newest cumulative `total_token_usage.input_tokens` (already INCLUDES `cached_input_tokens`) and `last_token_usage.total_tokens` as context. No format reads a model.
- Model sources in the real files (fixtures `tests/fixtures/usage/{claude,codex}.jsonl` have them): Claude `message.model` on each assistant line (the sidechain line is `claude-haiku-4-5-20251001`, `<synthetic>` lines are already skipped); Codex `turn_context.payload.model` (`gpt-5.5-codex`), emitted per turn and BEFORE that turn's `token_count`, so the model is a separate line, not part of the count line.
- `crates/mkrew-daemon/src/usage.rs`: `run` builds `SessionUsageEvent { session_id, input, output, context, context_window }` (:205) and skips a read when `last == Some(*u)` (needs `Copy`), stores it as session_event kind `usage` and broadcasts `Event::SessionUsage`. The read happens only at a turn end (`session.rs` sends the native id on `TurnEvent::Ended`): no watcher, no mid-turn read. That stays.
- `crates/mkrew-daemon/src/server/turn.rs:138-160` `session_usage` replays the newest stored `usage` event per session with `serde_json::from_value::<SessionUsageEvent>`; a failed parse is only logged and the session drops out. New fields without `#[serde(default)]` would make EVERY stored event unreadable, so both cache fields get `#[serde(default)]` and `model` is `Option`.
- Protocol: `SessionUsageEvent` at `crates/mkrew-protocol/src/message.rs:1374`; round-trip test at :2294 (`event_wire_shape_matches_spec`) expects `{session_id, input, output, context}`. `PROTOCOL_VERSION = 32` at `lib.rs:16`. Other `SessionUsageEvent { .. }` literals: `crates/mkrew-daemon/tests/daemon.rs:2108`, `crates/mkrew-ui/src/usage.rs:175` (test helper). `main.rs` only imports the type.
- UI `crates/mkrew-ui/src/usage.rs`: `total()` = `input + output` (:114) is used by `Usages::total`, `task_total`, `workspace_total` AND as the monotonic guard in `keep()`. `context_share` rebuilds a core `Usage` (:130) and shows hover `… ({in} in, {out} out)`. Shown at main.rs `context_share` (pane header) and `format_count`/`format_tokens` (sidebar row, task card).
- ctx% must not change for Claude: `context` stays `input + cache_creation + cache_read` of the newest non-sidechain message; only `input` (the running sum) loses the cache.
- HUMAN_CHECKS T13.9 (`docs/HUMAN_CHECKS.md:2127`) records "Claude input includes cache, same meaning as Codex's `input_tokens`" as a decision, and SPEC §20E.3 says "sums input/output tokens" without saying whether cache counts. Making `input` fresh-only changes a recorded meaning on the wire, so this task writes ADR `0035` and the SPEC edit in one change.
- Old stored events: `input` still includes cache, both cache fields read as 0, no model. Their total (`input + 0 + 0 + output`) equals what the new reader will report for the same transcript, so `keep()`'s "larger total wins" guard still accepts the new-shape event (equal totals replace). A daemon restart builds a new `Tracker` (`last = None`), so the first turn end after the upgrade re-reads the whole transcript from offset 0 and stores the new shape.

**Files:**
- Modify: `crates/mkrew-core/src/usage.rs` (`Usage`, `UsageReader` fields/`new`/`usage`/`feed`, `claude_line`, `codex_line`, new `set_model`, tests)
- Modify: `crates/mkrew-protocol/src/message.rs:1372-1385` (`SessionUsageEvent`), test at :2294
- Modify: `crates/mkrew-protocol/src/lib.rs:16` (`PROTOCOL_VERSION = 33`)
- Modify: `crates/mkrew-daemon/src/usage.rs` (`run`, test `each_read_takes_only_the_appended_bytes`)
- Modify: `crates/mkrew-daemon/tests/daemon.rs:2107-2115` (expected event), new test after :2128
- Modify: `crates/mkrew-ui/src/usage.rs` (`total`, `context_share`, tests)
- Create: `docs/decisions/0035-usage-input-excludes-cache.md`
- Modify: `docs/SPEC.md` §20E.3 (:1586-1593), protocol note (:1603-1604), §7 event table (after :557), `docs/HUMAN_CHECKS.md` (+ note on :2127)

**Interfaces:**
- Produces (core): `mkrew_core::usage::Usage { input: u64 /* fresh, cache excluded */, cache_read: u64, cache_write: u64, output: u64, context: u64, model: Option<String> }` — `#[derive(Debug, Clone, PartialEq, Eq, Default)]`, no longer `Copy`. `UsageReader::usage(&self) -> Option<Usage>` and `feed(&mut self, &[u8]) -> Option<Usage>` return it by value, with `model` filled.
- Produces (protocol): `SessionUsageEvent { session_id: Ulid, input: u64, output: u64, cache_read: u64 /* #[serde(default)] */, cache_write: u64 /* #[serde(default)] */, context: u64, context_window: Option<u64>, model: Option<String> /* #[serde(default, skip_serializing_if = "Option::is_none")] */ }`; `PROTOCOL_VERSION = 33`. Tasks 6 and 15 consume exactly these fields.
- Produces (UI): `Usages::total(session_id) -> Option<u64>` = input + cache_read + cache_write + output (all tokens, the same number as before for Claude and Codex).

- [ ] **Step 1: Failing core tests**

In `crates/mkrew-core/src/usage.rs` `mod tests`, replace the two `const`s `CLAUDE_USAGE` / `CODEX_USAGE` and `fixtures()` (they cannot be `const` once `Usage` holds a `String`):

```rust
    /// Two content-block lines of one message counted once, the sidechain line in the totals
    /// but not the context, the synthetic line ignored. `input` is fresh input only; the cache
    /// is split out. Context is unchanged: input + cache write + cache read of the newest
    /// main-thread message (so the ctx % did not move with the split).
    fn claude_usage() -> Usage {
        Usage {
            input: 2 + 3 + 1,
            cache_read: 48215 + 9000 + 48903,
            cache_write: 688 + 1200,
            output: 206 + 40 + 512,
            context: 1 + 1200 + 48903,
            // The sidechain line (haiku) is between the two opus messages and never counts.
            model: Some("claude-opus-5".into()),
        }
    }

    /// The newest cumulative totals, not their sum. Codex's `input_tokens` includes the cached
    /// ones: fresh = 51122 - 26880. Context = last `total_tokens`; model = `turn_context`.
    fn codex_usage() -> Usage {
        Usage {
            input: 51122 - 26880,
            cache_read: 26880,
            cache_write: 0,
            output: 1561,
            context: 35579,
            model: Some("gpt-5.5-codex".into()),
        }
    }

    fn fixtures() -> [(UsageFormat, &'static [u8], Usage); 2] {
        [
            (UsageFormat::ClaudeJsonl, CLAUDE, claude_usage()),
            (UsageFormat::CodexJsonl, CODEX, codex_usage()),
        ]
    }
```

`fixtures_give_the_expected_totals`: `Some(expected)` → `Some(expected.clone())` is not needed (one use per iteration), leave it. In `reading_in_any_chunks_gives_the_same_totals` change the last two asserts to clone (the inner loop reuses `expected`):

```rust
                assert_eq!(r.usage(), Some(expected.clone()), "{format:?} in chunks of {size}");
```

`partial_last_line_waits_for_its_newline`:

```rust
        let usage = Usage {
            input: 5,
            output: 2,
            context: 7,
            ..Usage::default()
        };
        assert_eq!(r.feed(b"\r\n"), Some(usage.clone()));
        assert_eq!(r.feed(b"{\"type\":"), Some(usage));
```

`turns_appended_later_add_up`:

```rust
        let early = r.feed(first).unwrap();
        assert!(early.output < claude_usage().output, "{early:?}");
        assert_eq!(r.feed(second), Some(claude_usage()));
```

Add two tests after `turns_appended_later_add_up`:

```rust
    /// The model is the newest main-thread one: a sidechain (sub-agent) line and a synthetic
    /// line after it do not replace it, and a line without a model keeps the old one.
    #[test]
    fn claude_model_is_the_newest_main_thread_one() {
        let line = |side: bool, model: &str, id: &str| {
            format!(
                "{{\"type\":\"assistant\",\"isSidechain\":{side},\"requestId\":\"{id}\",\"message\":{{\"id\":\"{id}\",\"model\":\"{model}\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}}}\n"
            )
        };
        let mut r = reader(&UsageFormat::ClaudeJsonl);
        r.feed(line(false, "claude-opus-5", "a").as_bytes());
        r.feed(line(true, "claude-haiku-4-5", "b").as_bytes());
        r.feed(line(false, "<synthetic>", "c").as_bytes());
        let usage = r.usage().unwrap();
        assert_eq!(usage.model.as_deref(), Some("claude-opus-5"));
        // Both non-synthetic lines are in the totals; the synthetic one is not.
        assert_eq!((usage.input, usage.output), (2, 2));
        r.feed(line(false, "claude-sonnet-5", "d").as_bytes());
        assert_eq!(r.usage().unwrap().model.as_deref(), Some("claude-sonnet-5"));
    }

    /// Codex names the model on `turn_context`, before the turn's counts: the newest one wins,
    /// a model alone is no usage, and cached tokens leave `input`.
    #[test]
    fn codex_model_is_the_newest_turn_context_and_cached_tokens_leave_input() {
        let turn = |model: &str| {
            format!("{{\"type\":\"turn_context\",\"payload\":{{\"model\":\"{model}\"}}}}\n")
        };
        let count = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":100,\"cached_input_tokens\":60,\"output_tokens\":5},\"last_token_usage\":{\"total_tokens\":105}}}}\n";
        let mut r = reader(&UsageFormat::CodexJsonl);
        assert_eq!(r.feed(turn("gpt-a").as_bytes()), None);
        r.feed(count.as_bytes());
        let usage = r.feed(turn("gpt-b").as_bytes()).unwrap();
        assert_eq!(
            (usage.input, usage.cache_read, usage.cache_write, usage.model.as_deref()),
            (40, 60, 0, Some("gpt-b"))
        );
    }
```

Run: `cargo test -p mkrew-core --lib usage -j 4`
Expected: FAIL to compile (`no field cache_read on type Usage`).

- [ ] **Step 2: Core reader**

`usage.rs`, replace the `Usage` struct (the `impl Usage` is unchanged):

```rust
/// Token totals of one agent session. The four counts add up to every token of the
/// conversation (ADR 0035).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Usage {
    /// Fresh input tokens over the whole session: the cache is NOT in here.
    pub input: u64,
    /// Input tokens served from the prompt cache.
    pub cache_read: u64,
    /// Input tokens written to the prompt cache (Codex has none).
    pub cache_write: u64,
    /// Output tokens over the whole session.
    pub output: u64,
    /// Context size of the newest message.
    pub context: u64,
    /// The model of the newest message; `None` while no line named one.
    pub model: Option<String>,
}
```

`UsageReader`: replace the `seen` field and add `model`:

```rust
    usage: Option<Usage>,
    /// The model of the newest message that names one (Codex: the newest `turn_context`).
    /// Not in `usage`: Codex names it on another line than the counts. [`Self::usage`] joins them.
    model: Option<String>,
    /// Claude repeats one API message's usage on every content-block line: the counts last
    /// added per `message.id` + `requestId` (`[input, cache write, cache read, output]`),
    /// replaced (not added) on a repeat. Kept for the whole session, not just the last key,
    /// since sidechain lines can interleave.
    seen: HashMap<String, [u64; 4]>,
```

In `new`, add `model: None,` after `usage: None,`.

Replace `usage()` and the start/end of `feed`, and the match arm:

```rust
    /// The totals so far; `None` until a usage line was seen.
    pub fn usage(&self) -> Option<Usage> {
        let usage = self.usage.clone()?;
        Some(Usage {
            model: self.model.clone(),
            ..usage
        })
    }

    /// Consumes bytes appended to the file at [`Self::offset`] and returns the totals so far.
    pub fn feed(&mut self, appended: &[u8]) -> Option<Usage> {
        self.offset += appended.len() as u64;
        self.partial.extend_from_slice(appended);
        let Some(end) = self.partial.iter().rposition(|b| *b == b'\n') else {
            return self.usage();
        };
        let rest = self.partial.split_off(end + 1);
        let complete = std::mem::replace(&mut self.partial, rest);
        for line in complete.split(|b| *b == b'\n') {
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                continue;
            };
            let parsed = match self.format {
                Format::Claude => self.claude_line(&value),
                Format::Codex => self.codex_line(&value),
            };
            match parsed {
                Line::Other => {}
                Line::Usage(usage) => self.usage = Some(usage),
                Line::Broken(what) => self.warn(what),
            }
        }
        self.usage()
    }

    /// Remembers `model` as the newest one (no allocation while it is unchanged).
    fn set_model(&mut self, model: Option<&str>) {
        if let Some(model) = model.filter(|m| !m.is_empty() && self.model.as_deref() != Some(m)) {
            self.model = Some(model.to_owned());
        }
    }
```

Replace `claude_line` (doc comment included):

```rust
    /// `type: "assistant"` lines carry `message.usage`. `input` counts `input_tokens` only;
    /// cache creation and cache read are `cache_write` and `cache_read`. Context is the sum of
    /// the three on the newest non-sidechain message (without its output, unlike Codex's), so it
    /// is what it was before the split. The model is `message.model` of that same message.
    fn claude_line(&mut self, value: &Value) -> Line {
        if value["type"] != "assistant" {
            return Line::Other;
        }
        let message = &value["message"];
        if message["model"] == "<synthetic>" {
            return Line::Other;
        }
        let usage = &message["usage"];
        let (Some(fresh), Some(output)) = (
            usage["input_tokens"].as_u64(),
            usage["output_tokens"].as_u64(),
        ) else {
            return Line::Broken("assistant line without message.usage token counts");
        };
        let cache = |key: &str| usage[key].as_u64().unwrap_or(0);
        // [input, cache write, cache read, output]
        let counts = [
            fresh,
            cache("cache_creation_input_tokens"),
            cache("cache_read_input_tokens"),
            output,
        ];

        // `model` of `self.usage` is always `None` (see `UsageReader::model`): this clone
        // allocates nothing.
        let mut total = self.usage.clone().unwrap_or_default();
        let key = format!(
            "{}\n{}",
            message["id"].as_str().unwrap_or_default(),
            value["requestId"].as_str().unwrap_or_default()
        );
        // A line without ids cannot be matched to its siblings: count it on its own.
        let key = (key != "\n").then_some(key);
        if let Some(old) = key.as_ref().and_then(|k| self.seen.get(k)).copied() {
            total.input -= old[0];
            total.cache_write -= old[1];
            total.cache_read -= old[2];
            total.output -= old[3];
        }
        if let Some(key) = key {
            self.seen.insert(key, counts);
        }
        total.input += counts[0];
        total.cache_write += counts[1];
        total.cache_read += counts[2];
        total.output += counts[3];
        if value["isSidechain"] != true {
            total.context = counts[0] + counts[1] + counts[2];
            self.set_model(message["model"].as_str());
        }
        Line::Usage(total)
    }
```

Replace the free `codex_line` (and its doc comment) with a method in the same `impl UsageReader` block (put it right after `claude_line`):

```rust
    /// `event_msg` lines with `payload.type: "token_count"` carry cumulative totals in
    /// `info.total_token_usage` (`info` is null before the first answer); context is
    /// `info.last_token_usage.total_tokens`, what Codex itself counts as in the window. Its
    /// `input_tokens` INCLUDES `cached_input_tokens`: fresh input is the difference. The model
    /// is `payload.model` of the newest `turn_context` line.
    fn codex_line(&mut self, value: &Value) -> Line {
        let payload = &value["payload"];
        if value["type"] == "turn_context" {
            self.set_model(payload["model"].as_str());
            return Line::Other;
        }
        if value["type"] != "event_msg" || payload["type"] != "token_count" {
            return Line::Other;
        }
        let info = &payload["info"];
        if info.is_null() {
            return Line::Other;
        }
        let total = &info["total_token_usage"];
        match (
            total["input_tokens"].as_u64(),
            total["output_tokens"].as_u64(),
            info["last_token_usage"]["total_tokens"].as_u64(),
        ) {
            (Some(input), Some(output), Some(context)) => {
                let cached = total["cached_input_tokens"]
                    .as_u64()
                    .unwrap_or(0)
                    .min(input);
                Line::Usage(Usage {
                    input: input - cached,
                    cache_read: cached,
                    output,
                    context,
                    ..Usage::default()
                })
            }
            _ => Line::Broken("token_count line without total/last token usage"),
        }
    }
```

Run: `cargo test -p mkrew-core --lib usage -j 4`
Expected: PASS (`fixtures_give_the_expected_totals` pins the Claude context at the old value, so ctx % is unchanged).

- [ ] **Step 3: Failing protocol tests, then the type**

`crates/mkrew-protocol/src/message.rs` test `event_wire_shape_matches_spec`, replace the `usage` block (:2294-2305):

```rust
        let usage = round_trip(&Message::Event(Event::SessionUsage(SessionUsageEvent {
            session_id: sid(),
            input: 10,
            output: 2,
            cache_read: 5,
            cache_write: 1,
            context: 7,
            context_window: None,
            model: Some("claude-opus-5".into()),
        })));
        assert_eq!(usage["event"], "session.usage");
        assert_eq!(
            usage["data"],
            json!({
                "session_id": sid(), "input": 10, "output": 2, "cache_read": 5,
                "cache_write": 1, "context": 7, "model": "claude-opus-5"
            })
        );
        // Stored by a daemon before the cache split (SPEC §20E.3, ADR 0035): no cache
        // fields, no model. It must still read, with the new fields at their defaults.
        let old: SessionUsageEvent = serde_json::from_value(json!({
            "session_id": sid(), "input": 10, "output": 2, "context": 7, "context_window": 200000
        }))
        .unwrap();
        assert_eq!(
            (old.cache_read, old.cache_write, old.model, old.context_window),
            (0, 0, None, Some(200_000))
        );
```

Run: `cargo test -p mkrew-protocol --lib event_wire_shape -j 4`
Expected: FAIL to compile (`struct SessionUsageEvent has no field named cache_read`).

Replace the struct (message.rs:1372-1385):

```rust
/// Token totals of an agent session, read from its transcript (SPEC §20E.3, ADR 0035).
/// `input + cache_read + cache_write + output` is every token of the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsageEvent {
    pub session_id: Ulid,
    /// Fresh input tokens over the agent's whole conversation, cache excluded. An event stored
    /// before protocol 33 still has the cache in here (and 0 in both cache fields).
    pub input: u64,
    /// Output tokens over the agent's whole conversation.
    pub output: u64,
    /// Input tokens served from the prompt cache.
    #[serde(default)]
    pub cache_read: u64,
    /// Input tokens written to the prompt cache (0 for agents without that figure).
    #[serde(default)]
    pub cache_write: u64,
    /// Context size of the newest message.
    pub context: u64,
    /// The profile's `context_window`, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// The model of the newest message, when the transcript names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}
```

`lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 33;`

Run: `cargo test -p mkrew-protocol --lib -j 4`
Expected: PASS. (`mkrew-daemon` and `mkrew-ui` do not compile until Steps 4 and 5; run only `-p` of the crate you are in.)

- [ ] **Step 4: Daemon**

`crates/mkrew-daemon/src/usage.rs` `run`, replace the comparison and the event (`Usage` is no longer `Copy`):

```rust
    let mut last: Option<Usage> = None;
    while let Some(native) = ends.recv().await {
        // … (unchanged until the filter)
        let Some(usage) = usage.filter(|u| last.as_ref() != Some(u)) else {
            continue;
        };
        last = Some(usage.clone());
        let event = SessionUsageEvent {
            session_id: session,
            input: usage.input,
            output: usage.output,
            cache_read: usage.cache_read,
            cache_write: usage.cache_write,
            context: usage.context,
            context_window,
            model: usage.model,
        };
```

(`last` now also changes when only the model changes, e.g. a Codex `turn_context`: the comparison is on the whole `Usage`.)

Test `each_read_takes_only_the_appended_bytes`: `Usage` has no `Copy`, so clone where a value is used twice:

```rust
        assert_eq!(Some(all.clone()), whole.feed(CLAUDE));
```

Integration test `a_turn_end_reads_token_usage_from_the_agent_transcript` (daemon.rs:2107-2115), replace `expected`:

```rust
    // The totals of `mkrew-core`'s Claude fixture: cache split out of `input` (ADR 0035),
    // the context as before, the model of the newest main-thread message.
    let expected = SessionUsageEvent {
        session_id: id,
        input: 2 + 3 + 1,
        output: 206 + 40 + 512,
        cache_read: 48215 + 9000 + 48903,
        cache_write: 688 + 1200,
        context: 1 + 1200 + 48903,
        context_window: Some(200_000),
        model: Some("claude-opus-5".into()),
    };
```

New test after it (the replay of events an older daemon stored; pattern of `startup_marks_stale_sessions_exited`, daemon.rs:665):

```rust
/// `session.usage` lists an event stored before the cache split (no cache fields, no model)
/// as it was: both cache fields 0, no model, the old `input` (SPEC §20E.3, ADR 0035).
#[test]
fn session_usage_lists_events_stored_before_the_cache_split() {
    let env = Env::new();
    std::fs::create_dir_all(env.data_dir()).unwrap();
    let session_id = Ulid::new();
    let project_id = {
        let store = Store::open(&env.data_dir().join("mkrew.db")).unwrap();
        let project = store.add_project(env.dir.path(), None).unwrap();
        store
            .insert_session(&Session {
                id: session_id,
                task_id: None,
                project_id: project.id,
                agent_profile: "fake".into(),
                kind: SessionKind::Agent,
                title: None,
                terminal_title: None,
                native_session_id: None,
                state: SessionState::Exited,
                state_source: StateSource::Daemon,
                state_changed_at: 1,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 1,
                ended_at: Some(2),
            })
            .unwrap();
        let old = json!({
            "session_id": session_id, "input": 1_000, "output": 20, "context": 900,
            "context_window": 200_000
        });
        store
            .insert_session_event(session_id, "usage", &old, 10)
            .unwrap();
        project.id
    };
    let (_daemon, mut client) = env.start();
    let listed: Vec<SessionUsageEvent> = client
        .call(Request::SessionUsage(ProjectRef { project_id }))
        .into_result()
        .unwrap();
    assert_eq!(
        listed,
        [SessionUsageEvent {
            session_id,
            input: 1_000,
            output: 20,
            cache_read: 0,
            cache_write: 0,
            context: 900,
            context_window: Some(200_000),
            model: None,
        }]
    );
    client.stop();
}
```

Run: `cargo test -p mkrew-daemon --lib usage -j 4`, then `cargo test -p mkrew-daemon --test daemon token_usage -j 4` and `cargo test -p mkrew-daemon --test daemon before_the_cache_split -j 4`
Expected: PASS. (Before this step's edit of `run` the crate does not compile: that is the failing state.)

- [ ] **Step 5: UI**

Failing tests first, in `crates/mkrew-ui/src/usage.rs` `mod tests`. The helper gets the new fields:

```rust
    fn usage(session_id: Ulid, input: u64, output: u64) -> SessionUsageEvent {
        SessionUsageEvent {
            session_id,
            input,
            output,
            cache_read: 0,
            cache_write: 0,
            context: 0,
            context_window: None,
            model: None,
        }
    }
```

Add after `events_and_loads_keep_the_larger_totals`:

```rust
    /// The total counts the cache (so it is the number it was before the split), and the
    /// split event of a turn replaces the unsplit one an older daemon stored for it: equal
    /// totals are not "smaller".
    #[test]
    fn totals_count_cached_tokens_and_a_split_event_replaces_an_unsplit_one() {
        let id = Ulid::new();
        let mut usages = Usages::default();
        // Stored before the split: `input` still had the cache in it.
        assert!(usages.apply(&Event::SessionUsage(usage(id, 1_000, 20))));
        let split = SessionUsageEvent {
            cache_read: 800,
            cache_write: 100,
            ..usage(id, 100, 20)
        };
        assert!(usages.apply(&Event::SessionUsage(split.clone())));
        assert_eq!(usages.total(id), Some(1_020));
        assert_eq!(usages.get(id), Some(&split));
        // A smaller (older) answer still does not undo it.
        assert!(!usages.load(vec![usage(id, 100, 20)]));
    }

    /// The hover says how many of the input tokens came from the cache, only when there are any.
    #[test]
    fn the_hover_names_cached_tokens() {
        let id = Ulid::new();
        let split = SessionUsageEvent {
            cache_read: 48_000,
            cache_write: 2_000,
            context: 51_000,
            context_window: Some(200_000),
            ..usage(id, 1_000, 500)
        };
        let share = context_share(&split).unwrap();
        assert_eq!(share.label, "ctx 25%");
        assert_eq!(
            share.hover,
            "Context 51.0k of 200.0k tokens · 51.5k tokens used (1.0k in, 50.0k cached, 500 out)"
        );
    }
```

Run: `cargo test -p mkrew-ui --lib usage -j 4`
Expected: FAIL (`totals_count_…`: left `Some(120)`, right `Some(1020)`; `the_hover_names_…`: no "cached").

Implement. `total` and its doc:

```rust
/// Every token of the conversation: fresh input, cache read, cache write and output.
fn total(usage: &SessionUsageEvent) -> u64 {
    usage.input + usage.cache_read + usage.cache_write + usage.output
}
```

`Usages::total` doc: `/// All tokens of `session_id` (input, cache and output).` `context_share`:

```rust
pub fn context_share(usage: &SessionUsageEvent) -> Option<ContextShare> {
    let window = usage.context_window?;
    let percent = Usage {
        context: usage.context,
        ..Usage::default()
    }
    .context_percent(Some(window))?;
    let cached = usage.cache_read + usage.cache_write;
    let cached = if cached > 0 {
        format!(", {} cached", format_count(cached))
    } else {
        String::new()
    };
    Some(ContextShare {
        label: format!("ctx {percent}%"),
        warning: percent >= WARNING_PERCENT,
        hover: format!(
            "Context {} of {} tokens · {} tokens used ({} in{cached}, {} out)",
            format_count(usage.context),
            format_count(window),
            format_count(total(usage)),
            format_count(usage.input),
            format_count(usage.output),
        ),
    })
}
```

Run: `cargo test -p mkrew-ui --lib usage -j 4`
Expected: PASS. The existing snapshot `labels_show_tokens_and_the_context_share` passes UNCHANGED (its events have no cache, so the text is identical): that is the proof ctx % and the hover of an old event did not move. Do not accept a changed snapshot.

- [ ] **Step 6: ADR**

Next free number: `ls docs/decisions` (last is 0033); the assembler fills `0035`. Create `docs/decisions/0035-usage-input-excludes-cache.md`:

```markdown
# 0035 — `session.usage` input excludes the cache

- Date: 2026-10-10
- Status: accepted
- Milestone: inbox 2026-10-10

## Context

SPEC §20E.3 sums "input/output tokens" and does not say whether the prompt cache counts. The code
(T13.9, HUMAN_CHECKS) made `input` = every token the model read: Claude `input_tokens` + cache
creation + cache read, Codex `input_tokens` (which already contains `cached_input_tokens`). The
inbox wants cache read and cache write shown on their own (status bar, right panel) and the model
name, and omp joins with its own counts. What the three agents write natively:

| Agent | fresh input | cache read | cache write | `input` in the file |
|---|---|---|---|---|
| Claude | `input_tokens` | `cache_read_input_tokens` | `cache_creation_input_tokens` | fresh only |
| omp | `input` | `cacheRead` | `cacheWrite` | fresh only (`totalTokens` = all four) |
| Codex | `input_tokens − cached_input_tokens` | `cached_input_tokens` | none | includes the cache |

## Options

1. Keep `input` = everything read; add `cache_read` / `cache_write` as subsets of it. Old stored
   events stay correct. But a consumer must subtract to get fresh input, omp and Claude must be
   ADDED up to match, and `input + cache_read + …` double-counts if a consumer forgets.
2. `input` = fresh input only; `cache_read` and `cache_write` beside it; all four add up to every
   token. Old stored events keep the old `input` (cache inside, cache fields 0) until the session's
   next turn end re-reads the transcript.
3. A new field `fresh_input`, leaving `input` as it is. No mixed meaning, but two fields say
   "input" and every format still needs the arithmetic.

## Decision

Option 2. It is what Claude and omp write natively and what the item asks for ("split cache out of
`input`"); only Codex needs one subtraction, done once in the reader. The four counts add up, so
"total tokens" is a sum with no subset rule (`Usages::total` = input + cache_read + cache_write +
output, the same number as before for Claude and Codex, so card and sidebar totals do not move).
The context size is untouched (Claude: input + cache write + cache read of the newest main-thread
message; Codex: `last_token_usage.total_tokens`), so `ctx %` does not change.
`PROTOCOL_VERSION` goes to 33; the new fields are `#[serde(default)]`, so events stored by an
older daemon still load (`session.usage` replays them).

## Consequences

- SPEC §20E.3, its protocol note and the §7 event table change in the same commit; the T13.9 note
  in HUMAN_CHECKS is marked as changed.
- Until a session's next turn end after the upgrade, its stored event has the old meaning: the
  hover of `ctx` shows the whole input as "in" and no "cached" part; totals and ctx % are right.
  The daemon restart that the protocol bump needs makes the first turn end re-read from the start.
- Sub-agent (Claude sidechain) tokens stay in the totals but not in the context, as before.
- Still no money estimate (SPEC §20E.3, §1).
```

- [ ] **Step 7: SPEC**

§20E.3 (SPEC.md:1586-1593): replace the whole item 3 with

```markdown
3. **Usage.** For profiles with `[usage]`, the daemon finds the session's transcript file by
   `native_session_id` — `claude_jsonl`: `<CLAUDE_CONFIG_DIR or ~/.claude>/projects/*/<id>.jsonl`;
   `codex_jsonl`: `<CODEX_HOME or ~/.codex>/sessions/**/*<id>*.jsonl` — and reads only appended
   bytes on each turn end (no file watching, nothing while a turn runs). It sums the tokens of the
   session and takes the context size and the model name of the newest message (field names
   checked against the CURRENT formats when implementing; fixtures recorded from real files).
   The counts add up (ADR 0035): `input` = fresh input, cache excluded (Claude
   `input_tokens`; Codex `input_tokens` − `cached_input_tokens`); `cache_read` (Claude
   `cache_read_input_tokens`; Codex `cached_input_tokens`); `cache_write` (Claude
   `cache_creation_input_tokens`; Codex has none, 0); `output`. Total tokens = `input` +
   `cache_read` + `cache_write` + `output`. Context = Claude: `input_tokens` + both cache counts of
   the newest non-sidechain message; Codex: `last_token_usage.total_tokens`. `model` = Claude
   `message.model` of the newest non-sidechain, non-`<synthetic>` assistant line; Codex `model` of
   the newest `turn_context` line. Event `session.usage` (`input`, `output`, `cache_read`,
   `cache_write`, `context`, `context_window?`, `model?`); the pane header shows `ctx 62%` (warning
   color ≥ 80 %), task cards and sidebar workspace rows show total tokens. No money estimate.
   Unknown/changed format → no usage, one warning in the log.
```

Protocol note (:1603-1604), append after "restarts)." on the same paragraph:

```markdown
event `session.usage`; request `session.usage` (`project_id`) → the newest usage of each session
of the project (kept as `session_event` kind `usage`, so totals survive reconnects and restarts).
Since protocol 33 `input` excludes the cache and `cache_read`, `cache_write` and `model?` are
new; an event stored before it reads back with both cache fields 0, no model and its old `input`
(cache included) until the session's next turn end.
```

§7 event table, a new row after the `session.ports` row (:557):

```markdown
| `session.usage` | an agent session's token usage changed at a turn end (§20E.3): `session_id`, `input` (fresh, cache excluded), `output`, `cache_read`, `cache_write`, `context`, `context_window?`, `model?` |
```

- [ ] **Step 8: HUMAN_CHECKS**

Append to the section `### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)`:

```markdown
- [ ] **Usage cache split (Claude, Codex):** after one turn in each, restart the daemon, run another
  turn, hover the pane header `ctx N%`: the text reads `… tokens used (X in, Y cached, Z out)`, `X`
  is small (fresh input only) and `Y` is most of the input. `ctx N%` and the total on the task
  card / sidebar row equal what they were before this change for the same session (compare with
  Claude `/context` and Codex `/status`). A session that had no turn end since the upgrade still
  shows its old `ctx` and total, with no `cached` in the hover. The model name is not shown yet
  (the status bar of a later task shows it).
```

Also append to the end of the T13.9 bullet (`docs/HUMAN_CHECKS.md:2137`, after `check both against the models you use.`):

```markdown
  Changed 2026-10-10: `input` is fresh input only, the cache has its own fields (ADR 0035).
```

- [ ] **Step 9: Verify and tick**

Run the full verification (`scripts/verify.sh`). Do NOT tick inbox line 47 yet (Task 5 ticks it).
Suggested message: `feat(usage): session.usage splits cache read/write out of input and carries the model (protocol 33)`

---

### Task 5: omp usage: `omp_jsonl` format + `[usage]` in `agents/omp.toml`

Inbox: line 47 `Usage data: omp usage, cache split, model` (Task 4 first; this task ticks it). Spec: §20E.3 (SPEC.md:1586-1593 as rewritten by Task 4), profile example (:1390-1392).

**Current state (investigated):**
- omp writes `<PI_CODING_AGENT_DIR or ~/.omp/agent>/sessions/<cwd-slug>/<UTC-ts>_<session-id>.jsonl` (same env var as `crates/mkrew-cli/src/integration.rs:91`). The id the Mkrew extension reports (`ctx.sessionManager.getSessionId()`, `crates/mkrew-cli/assets/omp-extension.ts:13-19`) is the `"id"` of the file's first line `{"type":"session","version":3,"id":"<id>",…}` and the `_<id>` suffix of the file name. The slug rule (`C:\Users\<me>\…` → `-Workspace-…`) is not documented for paths outside home, so `locate` does NOT derive it: it searches by id, like Codex.
- Sub-agent transcripts live in a DIRECTORY `<ts>_<id>/` next to the file (`ScoutX.jsonl` inside, names without the id). The walk must return the `.jsonl` file and never the directory or the sub-agent files; omp totals therefore exclude sub-agent tokens (SPEC says so).
- Usage lines: `type:"message"`, `message.role:"assistant"`, `message.model` (bare, e.g. `claude-opus-5-5`), `message.usage { input, output, cacheRead, cacheWrite, totalTokens, cost{…} }`. Usage is PER API CALL (no running total in the file): the reader sums. `totalTokens = input + output + cacheRead + cacheWrite` (checked: 2+94+45333+2002 = 47431; 2+77+72843+4748 = 77670), so omp `input` already excludes cache. `cost` is not read (no money, SPEC §20E.3).
- An aborted call is an assistant line with `content:[]`, all-zero usage and the model of the attempt. It must not reset `context` to 0 or change the model. `model_change` lines (`anthropic/claude-sonnet-5-5`, a fallback switch) are NOT reliable for the model that answered (the next assistant line still named the old one): use `message.model` of the newest assistant line with tokens. `compaction`, `title`, `session`, … are ignored; after a compaction the next call's input and cache drop, so context falls by itself.
- The context window is not in the file (no `contextWindow` anywhere in the session files or `~/.omp/agent` settings). It can only come from the profile `context_window`, and omp runs many models, so a fixed value is an approximation the user edits (open question; default 200000, the same as `agents/claude.toml`).
- `agents/omp.toml` has no `[usage]`. `UsageFormat` (`profile.rs:50`) has `ClaudeJsonl`, `CodexJsonl`, `Unknown(String)`; `parse` at :57; matches on it: core `UsageReader::new`, daemon `usage.rs` `root` and `locate`. No other match exists (grep `UsageFormat`).
- The turn-end hook of omp may fire before the last assistant line is flushed to disk [INFERENCE: not observed]. The reader keeps its offset, so the next turn end picks the line up: worst case the display lags one turn. Listed in HUMAN_CHECKS.

**Files:**
- Create: `crates/mkrew-core/tests/fixtures/usage/omp.jsonl`
- Modify: `crates/mkrew-core/src/profile.rs:50-63` (`UsageFormat::OmpJsonl`, `parse`), tests `usage_section_parses_known_and_unknown_formats` (:465), `repo_sample_profiles_parse` (:633-681)
- Modify: `crates/mkrew-core/src/usage.rs` (`Format::Omp`, `new`, `feed`, new `omp_line`, tests)
- Modify: `crates/mkrew-daemon/src/usage.rs` (`root`, `locate`, doc comments, new test)
- Modify: `agents/omp.toml`, `agents/claude.toml:26` (comment)
- Modify: `docs/SPEC.md` §20E.3, profile example (:1391), `docs/HUMAN_CHECKS.md`, `docs/INBOX.md:47`

**Interfaces:**
- Consumes (Task 4): `mkrew_core::usage::Usage { input, cache_read, cache_write, output, context, model }`, `UsageReader::feed/usage`, `Usage::default()`.
- Produces: `mkrew_core::profile::UsageFormat::OmpJsonl` (profile name `"omp_jsonl"`), `agents/omp.toml` `[usage] format = "omp_jsonl"`, `context_window = 200000`. No protocol change: omp sessions ride on Task 4's `session.usage`.

- [ ] **Step 1: Fixture**

Create `crates/mkrew-core/tests/fixtures/usage/omp.jsonl` (one JSON object per line, the final newline kept). Lines 3, 5 and 9 are the real shapes recorded from `~/.omp/agent/sessions` with the message text and paths removed (`content` is `[]`; the reader never looks at it; `cost` is kept to prove it is ignored). Line 7 is the one hand-made call, there only to put a second model after the fallback switch.

```
{"type":"session","version":3,"id":"01a1214d-3acf-767f-a2fd-27de51e5c182","timestamp":"2026-10-09T15:34:31.887Z","cwd":"C:\\Users\\me\\proj"}
{"type":"message","id":"ce53a81b","parentId":null,"timestamp":"2026-10-09T15:41:50.120Z","message":{"role":"user","content":[]}}
{"type":"message","id":"c98a7566","parentId":"ce53a81b","timestamp":"2026-10-09T15:42:02.556Z","message":{"role":"assistant","content":[],"api":"anthropic-messages","provider":"anthropic","model":"claude-opus-5-5","usage":{"input":2,"output":94,"cacheRead":45333,"cacheWrite":2002,"totalTokens":47431,"cost":{"input":0.00001,"output":0.00235,"cacheRead":0.0227,"cacheWrite":0.0125,"total":0.03756}},"stopReason":"toolUse"}}
{"type":"model_change","id":"2b18d986","parentId":"c98a7566","timestamp":"2026-10-09T15:42:10.000Z","model":"anthropic/claude-sonnet-5-5","role":"fallback","resolvedModelIsFallback":true}
{"type":"message","id":"e1f40b72","parentId":"2b18d986","timestamp":"2026-10-09T15:42:31.204Z","message":{"role":"assistant","content":[],"api":"anthropic-messages","provider":"anthropic","model":"claude-opus-5-5","usage":{"input":2,"output":77,"cacheRead":72843,"cacheWrite":4748,"totalTokens":77670,"cost":{"input":0.00001,"output":0.0019,"cacheRead":0.0364,"cacheWrite":0.0297,"total":0.068}},"stopReason":"toolUse"}}
{"type":"message","id":"a0d9c3e5","parentId":"e1f40b72","timestamp":"2026-10-09T15:42:40.000Z","message":{"role":"user","content":[]}}
{"type":"message","id":"b4c71d08","parentId":"a0d9c3e5","timestamp":"2026-10-09T15:43:02.000Z","message":{"role":"assistant","content":[],"api":"anthropic-messages","provider":"anthropic","model":"claude-sonnet-5-5","usage":{"input":1,"output":310,"cacheRead":74000,"cacheWrite":120,"totalTokens":74431,"cost":{"input":0.00001,"output":0.0046,"cacheRead":0.0222,"cacheWrite":0.0005,"total":0.0273}},"stopReason":"stop"}}
{"type":"compaction","id":"5d2e9f10","parentId":"b4c71d08","timestamp":"2026-10-09T15:44:00.000Z","summary":""}
{"type":"message","id":"e583a28e","parentId":"5d2e9f10","timestamp":"2026-10-09T15:44:20.000Z","message":{"role":"assistant","content":[],"api":"anthropic-messages","provider":"anthropic","model":"claude-opus-5-5","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"aborted","errorMessage":"Interrupted by user"}}
```

What it pins: three counted calls (lines 3, 5, 7); the `model_change` line to sonnet is ignored (line 5 still says opus); the newest counted call (line 7) names sonnet; the aborted zero-usage line with `claude-opus-5-5` AFTER it changes neither the model nor the context.

- [ ] **Step 2: Failing tests**

`crates/mkrew-core/src/usage.rs` `mod tests`: add the fixture const next to the others, the expected totals, and the third row:

```rust
    const OMP: &[u8] = include_bytes!("../tests/fixtures/usage/omp.jsonl");
```

```rust
    /// Per-call usage is added up; `input` is already fresh in omp. Context = input + cache
    /// read + cache write of the newest counted call (without its output, like Claude). The
    /// aborted zero-usage call at the end, and the `model_change` line, change nothing.
    fn omp_usage() -> Usage {
        Usage {
            input: 2 + 2 + 1,
            cache_read: 45333 + 72843 + 74000,
            cache_write: 2002 + 4748 + 120,
            output: 94 + 77 + 310,
            context: 1 + 74000 + 120,
            model: Some("claude-sonnet-5-5".into()),
        }
    }
```

```rust
    fn fixtures() -> [(UsageFormat, &'static [u8], Usage); 3] {
        [
            (UsageFormat::ClaudeJsonl, CLAUDE, claude_usage()),
            (UsageFormat::CodexJsonl, CODEX, codex_usage()),
            (UsageFormat::OmpJsonl, OMP, omp_usage()),
        ]
    }
```

`fixtures_give_the_expected_totals` and `reading_in_any_chunks_gives_the_same_totals` now cover omp (whole file, and chunks of 1, 7, 100 and 1000 bytes). Add:

```rust
    /// An aborted or failed call has zero tokens: it is not a reading (no ctx 0 %, no model).
    #[test]
    fn omp_calls_that_used_no_tokens_are_not_usage() {
        let aborted = br#"{"type":"message","message":{"role":"assistant","model":"claude-opus-5-5","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0},"stopReason":"aborted"}}"#;
        let mut r = reader(&UsageFormat::OmpJsonl);
        assert_eq!(r.feed(&[aborted.as_slice(), b"\n"].concat()), None);
        assert_eq!(r.take_warning(), None);
    }
```

In `changed_format_warns_once`, after the Codex block:

```rust
        let mut r = reader(&UsageFormat::OmpJsonl);
        r.feed(b"{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"model\":\"m\"}}\n");
        assert!(r.take_warning().unwrap().contains("message.usage"));
        assert_eq!(r.usage(), None);
```

`profile.rs` tests: in `usage_section_parses_known_and_unknown_formats` after the codex lines:

```rust
        let p = parse("command = \"x\"\n[usage]\nformat = \"omp_jsonl\"").unwrap();
        assert_eq!(p.usage.unwrap().format, UsageFormat::OmpJsonl);
```

and in `repo_sample_profiles_parse` after the codex assert:

```rust
        assert_eq!(format("omp"), Some(UsageFormat::OmpJsonl));
```

Daemon `crates/mkrew-daemon/src/usage.rs` tests, after `codex_transcripts_are_found_in_dated_folders`:

```rust
    /// omp keeps `sessions/<cwd-slug>/<ts>_<id>.jsonl`; the slug is not derived, the id is
    /// searched. The sub-agent folder `<ts>_<id>/` (its files do not carry the id) is never it.
    #[test]
    fn omp_transcripts_are_found_in_slug_folders() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("sessions/-Workspace-proj");
        let name = "2026-10-09T15-34-31-887Z_01a1214d-3acf.jsonl";
        let sub_agents = dir.join("2026-10-09T15-34-31-887Z_01a1214d-3acf");
        std::fs::create_dir_all(&sub_agents).unwrap();
        std::fs::write(dir.join(name), "").unwrap();
        std::fs::write(sub_agents.join("ScoutX.jsonl"), "").unwrap();
        std::fs::write(dir.join("2026-10-09T16-00-00-000Z_other.jsonl"), "").unwrap();
        let found = locate(&UsageFormat::OmpJsonl, root.path(), "01a1214d-3acf");
        assert_eq!(found, Some(dir.join(name)));
        assert_eq!(locate(&UsageFormat::OmpJsonl, root.path(), "nope"), None);
    }
```

Run: `cargo test -p mkrew-core --lib usage -j 4` and `cargo test -p mkrew-core --lib profile -j 4`
Expected: FAIL to compile (`no variant OmpJsonl`).

- [ ] **Step 3: Profile format + core reader**

`profile.rs`: `UsageFormat` gets `OmpJsonl` after `CodexJsonl`; `parse` gets `"omp_jsonl" => Self::OmpJsonl,`.

`usage.rs` core: `enum Format { Claude, Codex, Omp }`; `new`: `UsageFormat::OmpJsonl => Format::Omp,`; `feed` match: `Format::Omp => self.omp_line(&value),`. New method after `codex_line`:

```rust
    /// `type: "message"` lines with `message.role: "assistant"` carry that API call's own
    /// `message.usage`; the calls are added up. `input` is already without the cache
    /// (`cacheRead` / `cacheWrite`), `cost` is not read. Context is input + both cache counts of
    /// the newest call (without its output, like Claude). A call with no tokens at all (aborted,
    /// failed) is not a reading: it keeps the context and the model. `model_change` lines are
    /// ignored: after a fallback switch the next call can still name the old model, so only
    /// `message.model` of a counted call says what answered.
    fn omp_line(&mut self, value: &Value) -> Line {
        let message = &value["message"];
        if value["type"] != "message" || message["role"] != "assistant" {
            return Line::Other;
        }
        let usage = &message["usage"];
        let (Some(input), Some(output)) = (usage["input"].as_u64(), usage["output"].as_u64())
        else {
            return Line::Broken("assistant line without message.usage token counts");
        };
        let cache = |key: &str| usage[key].as_u64().unwrap_or(0);
        let (cache_read, cache_write) = (cache("cacheRead"), cache("cacheWrite"));
        if input + output + cache_read + cache_write == 0 {
            return Line::Other;
        }
        let mut total = self.usage.clone().unwrap_or_default();
        total.input += input;
        total.output += output;
        total.cache_read += cache_read;
        total.cache_write += cache_write;
        total.context = input + cache_read + cache_write;
        self.set_model(message["model"].as_str());
        Line::Usage(total)
    }
```

Run: `cargo test -p mkrew-core --lib usage -j 4` and `cargo test -p mkrew-core --lib profile -j 4`
Expected: `usage` PASS (omp numbers pinned: input 5, output 481, cache read 192176, cache write 6870, context 74121, model `claude-sonnet-5-5`); `profile` still FAILS only `repo_sample_profiles_parse` until Step 5 adds `[usage]` to `agents/omp.toml`.

- [ ] **Step 4: Daemon: root and locate**

`crates/mkrew-daemon/src/usage.rs`. `root` (doc and match):

```rust
/// The folder the agent keeps its transcripts under: `CLAUDE_CONFIG_DIR` or `~/.claude`,
/// `CODEX_HOME` or `~/.codex`, `PI_CODING_AGENT_DIR` or `~/.omp/agent` (omp).
fn root(format: &UsageFormat) -> Option<PathBuf> {
    let (var, dir) = match format {
        UsageFormat::ClaudeJsonl => ("CLAUDE_CONFIG_DIR", ".claude"),
        UsageFormat::CodexJsonl => ("CODEX_HOME", ".codex"),
        UsageFormat::OmpJsonl => ("PI_CODING_AGENT_DIR", ".omp/agent"),
        UsageFormat::Unknown(_) => return None,
    };
```

`locate` (doc and the Codex arm shared with omp; both are `sessions/**/*<id>*.jsonl`):

```rust
/// The transcript of agent session `id` under `root`: `projects/*/<id>.jsonl` (Claude) or
/// `sessions/**/*<id>*.jsonl` (Codex, omp). An id that could leave the folder finds nothing.
```

```rust
        UsageFormat::CodexJsonl | UsageFormat::OmpJsonl => {
```

(The walk already skips directories as results and only takes names that end in `.jsonl` and contain the id; omp's sub-agent folder `<ts>_<id>/` is walked but its `ScoutX.jsonl` files do not contain the id. When several files match, the last by path is read: omp names start with the UTC timestamp, so that is the newest.)

Run: `cargo test -p mkrew-daemon --lib usage -j 4`
Expected: PASS (`omp_transcripts_are_found_in_slug_folders`; the existing Claude/Codex locate tests unchanged). Before this step the crate fails to compile on the non-exhaustive `match`: that is the failing state.

- [ ] **Step 5: Profile files**

`agents/omp.toml`, append (the `[usage]` table after `[state]`):

```toml

# Token usage from omp's session file (SPEC §20E.3). The window is not in that file and omp runs
# many models: set it to the context window of the model you use, or remove the line for no ctx %.
[usage]
format = "omp_jsonl"
context_window = 200000         # tokens, for the ctx % shown
```

`agents/claude.toml:26`: `format = "claude_jsonl"         # "claude_jsonl" | "codex_jsonl" | "omp_jsonl"`.

Run: `cargo test -p mkrew-core --lib profile -j 4`
Expected: PASS.

- [ ] **Step 6: SPEC**

Profile example (SPEC.md:1391):

```markdown
format = "claude_jsonl"        # "claude_jsonl" | "codex_jsonl" | "omp_jsonl"; absent = no usage
```

§20E.3, as left by Task 4, change the path list (the lines `…projects/*/<id>.jsonl`;` and `` `codex_jsonl`: … `` ) to

```markdown
   `native_session_id` — `claude_jsonl`: `<CLAUDE_CONFIG_DIR or ~/.claude>/projects/*/<id>.jsonl`;
   `codex_jsonl`: `<CODEX_HOME or ~/.codex>/sessions/**/*<id>*.jsonl`; `omp_jsonl`:
   `<PI_CODING_AGENT_DIR or ~/.omp/agent>/sessions/**/*<id>*.jsonl` (the file, never the
   sub-agent folder next to it: omp totals leave out sub-agent tokens) — and reads only appended
```

and, after the sentence that gives the Claude and Codex `model`, add:

```markdown
   omp (per API call, summed): `message.usage` `input` (already fresh), `cacheRead`, `cacheWrite`,
   `output` of `type: "message"` assistant lines; context = `input` + `cacheRead` + `cacheWrite` of the
   newest call; `model` = `message.model` of the newest call (`model_change` lines are not
   trusted). A call with no tokens (aborted) changes neither. omp's file does not hold the
   context window: `context_window` in the profile is the only source.
```

- [ ] **Step 7: HUMAN_CHECKS**

Append to the section `### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)`:

```markdown
- [ ] **omp usage:** run `mk integration install omp` if not done, start an omp agent pane, finish one
  turn: the pane header shows `ctx N%` and a token total on the card / sidebar row. If it only
  appears after the NEXT turn, omp's turn-end hook fires before the last line is flushed: note it
  here (the reader keeps its offset, so nothing is lost). Interrupt a turn with Esc: ctx % and total
  do not drop to 0. Compare `ctx N%` with omp's own context figure. `context_window` in
  `agents/omp.toml` is 200000 for every model: set your model's real window in your profile copy
  and check the percentage follows.
```

- [ ] **Step 8: Verify and tick**

Run the full verification (`scripts/verify.sh`). Tick inbox line 47 `[x]` (`docs/INBOX.md:47`, `Usage data: omp usage, cache split, model`).
Suggested message: `feat(usage): omp token usage from its session file (omp_jsonl) and a context window in agents/omp.toml`

---

### Task 6: Status bar + Toggle sidebar

Inbox: line 54 `Status bar + hide sidebar` (last task of item 3; tick it at the end). Spec: §10 (:673-748), §13 (:912-990), §20E.3 (usage).

**Current state (investigated):**
- No status bar exists. After Task 3 the root of `impl Render for Shell` (main.rs, find `impl Render for Shell`) is a column: `render_title_bar`, then a `flex_1().min_h_0().flex()` row `[render_sidebar | render_main]`, then the modal dialogs as `.children(...)`. `track_focus`, `key_context` and `on_action(cx.listener(Self::run))` sit on that root; they stay there.
- Sidebar: `Shell::render_sidebar` is a fixed `theme::SIDEBAR_WIDTH` column. No "hidden" state exists (grep `sidebar_hidden|sidebar_visible` finds nothing). Pane sizes come from layout bounds (no code uses `SIDEBAR_WIDTH` for pane math, only `render_sidebar`), so hiding it just widens the panes and the existing `session.resize` path follows.
- Inbox: `Shell::inbox_entries()` = `inbox::entries(projects, sessions, &self.viewed)` (needs_input, plus done not yet viewed). The `Command::NextInbox` arm of `Shell::run` inlines "entries → focused session → `inbox::next` → `open_session`". `Shell::render` marks the focused pane viewed (`self.viewed.view(session)`) before it builds the children, so a count read inside `render_status_bar` already sees it.
- Focused pane: `Shell::focused_session()` is `None` unless the Terminals view shows (so the agent fields are hidden on Board / Docs / Files / Git: by design, "focused agent pane"). `Session { kind: SessionKind::Agent|Shell, state, state_source, state_changed_at, .. }`.
- Usage: `Usages::get(session_id) -> Option<&SessionUsageEvent>`; `usage::context_share` (the pane header's `ctx 62%`), `usage::format_count` (`850` / `12.3k` / `1.2M`). After Task 4 `SessionUsageEvent` has `input` (fresh, cache excluded), `output`, `cache_read`, `cache_write`, `context`, `context_window: Option<u64>`, `model: Option<String>`.
- Elapsed time: the pane header shows `status::text(state, exit_code, state_changed_at, now)`, whose time is `status::time(state, state_changed_at, now)` (Some only for `working` / `idle`). The bar uses the same `status::time` and `status::hover`.
- Redraw: the existing `STATUS_TICK` (500 ms) task in `Shell::new` (main.rs, "Spinner and elapsed/idle times move on their own") calls `cx.notify()` when `status::redraw_due(sessions, last, now)` (working: every tick; idle: each second under a minute, then each minute). The bar reads `now` from the same per-frame clock, so its elapsed text refreshes with that tick and **adds no timer**. Other refreshes ride on existing notifies: `session.usage` (`usages.apply` → `cx.notify()`), focus change, session state / inbox (`sidebar.apply` redraw), and Task 2's `git.changed` → `load_git_summary` (replies `cx.notify()`). The bar adds no notify of its own except the toggle.
- Keymap: `ctrl-shift-b` is unbound (grep of `default_keys` finds none). Plain Ctrl+B would steal tmux's prefix and readline's back-char from terminals (SPEC §13: defaults avoid plain Ctrl+letter). The wiring to copy is `ToggleFullScreen`: keymap.rs enum, `all()`, `name()`, `default_keys()`; palette.rs `title()`, `available()`; main.rs `Shell::run`.
- Git (Task 2, consumed): `Shell::git_summary() -> Option<&GitSummary>`, `GitSummary::branch_tooltip() -> Option<String>` (one line per repo, `fe — main · 1a2b3c4`), `git_history::short_hash`. A root without `.git` is already covered: `git.repos` lists its child repos, so `GitSummary.repos` is non-empty.
- Persisting the hidden flag would need a `[ui]` key and a writer (`settings.rs` only line-edits `theme` today). Decision: **in memory only, shown on start** (the toggle is one key press; Window size is not remembered either). Recorded in SPEC.
- No ADR: a bottom bar and a toggle are not forbidden by SPEC or an ADR; Task 3's ADR already amends ADR 0027's layout. SPEC §10 + §13 are updated here.

**Files:**
- Modify: `crates/mkrew-ui/src/usage.rs` (`ContextShare.fill_percent`, `UsageFields`, `fields`, tests)
- Modify: `crates/mkrew-ui/src/inbox.rs` (`label`, test)
- Modify: `crates/mkrew-ui/src/git_summary.rs` (Task 2's file: `GitSummary::head_label`, test)
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum, `all()`, `name()`, `default_keys()`, test `spec_defaults`)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title()`, `available()`, test `availability_follows_the_view`)
- Modify: `crates/mkrew-ui/src/theme.rs` (3 layout consts after `SESSION_ROW_HEIGHT`)
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell.sidebar_hidden`, `toggle_sidebar`, `jump_next_inbox`, `render_status_bar`, `status_agent_fields`, `context_meter`, root layout, `Shell::run`)
- Modify: `docs/SPEC.md` §10, §13; `docs/HUMAN_CHECKS.md`; `docs/INBOX.md` (tick)

**Interfaces:**
- Consumes (Task 2): `Shell::git_summary(&self) -> Option<&GitSummary>`, `GitSummary { repos: Vec<RepoSummary> }`, `RepoSummary { repo: String, branch: Option<String>, head: Option<String>, modified: usize, untracked: usize }`, `GitSummary::branch_tooltip(&self) -> Option<String>`.
- Consumes (Task 3): the root column of `Shell::render`.
- Consumes (Task 4): `SessionUsageEvent { input, output, cache_read, cache_write, context, context_window, model: Option<String>, .. }`.
- Produces (used by Tasks 15 and later):
  ```rust
  // usage.rs
  pub struct UsageFields { pub context: Option<ContextShare>, pub tokens: Option<String>, pub cache: Option<String>, pub model: Option<String> }
  pub fn fields(usage: &SessionUsageEvent) -> UsageFields; // a field with no data is None
  // ContextShare gains: pub fill_percent: u8  (0..=100, for the meter; the label keeps the true %)
  // inbox.rs
  pub fn label(waiting: usize) -> String; // "Inbox clear" | "1 needs you" | "3 need you"
  // git_summary.rs
  impl GitSummary { pub fn head_label(&self) -> Option<String>; }
  // keymap.rs
  Command::ToggleSidebar // name "toggle_sidebar", default "ctrl-shift-b", context None
  // main.rs
  Shell.sidebar_hidden: bool; Shell::toggle_sidebar(&mut self, cx); Shell::jump_next_inbox(&mut self, window, cx);
  Shell::render_status_bar(&self, now: i64, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::Div
  // theme.rs
  STATUS_BAR_HEIGHT, STATUS_BAR_METER_WIDTH, STATUS_BAR_METER_HEIGHT
  ```
  **Right-end slot for Task 15:** the bar's last child is the middle `flex_1` container (agent fields, right-aligned). Task 15 appends its right-panel toggle as the bar's next `.child(...)` after it. Nothing is drawn there now (no dead button).

- [ ] **Step 1: Failing tests (pure logic)**

`usage.rs`, `mod tests`, after `labels_show_tokens_and_the_context_share`. The module's `usage(session_id, input, output)` helper already builds a `SessionUsageEvent` (Task 4 added the cache and model fields to it with `0` / `None`):

```rust
    /// A field with no data is `None`, never `0` or an empty string.
    #[test]
    fn fields_without_data_are_none() {
        let id = Ulid::new();
        assert_eq!(fields(&usage(id, 0, 0)), UsageFields::default());
        // A blank model name is no model.
        let blank = SessionUsageEvent {
            model: Some("  ".to_owned()),
            ..usage(id, 0, 0)
        };
        assert_eq!(fields(&blank).model, None);
        // Context needs the profile's window.
        let no_window = SessionUsageEvent {
            context: 50_000,
            context_window: None,
            ..usage(id, 1, 1)
        };
        assert_eq!(fields(&no_window).context, None);
    }

    #[test]
    fn fields_format_the_real_data() {
        let id = Ulid::new();
        let full = SessionUsageEvent {
            cache_read: 1_250_000,
            cache_write: 0,
            context: 124_000,
            context_window: Some(200_000),
            model: Some(" claude-opus-4 ".to_owned()),
            ..usage(id, 12_345, 800)
        };
        let shown = fields(&full);
        assert_eq!(shown.tokens.as_deref(), Some("in 12.3k · out 800"));
        // Only the side that has tokens is named.
        assert_eq!(shown.cache.as_deref(), Some("cache 1.3M read"));
        assert_eq!(shown.model.as_deref(), Some("claude-opus-4"));
        let context = shown.context.expect("window known");
        assert_eq!((context.label.as_str(), context.fill_percent), ("ctx 62%", 62));

        let both = SessionUsageEvent {
            cache_read: 800,
            cache_write: 40_000,
            ..usage(id, 1, 1)
        };
        assert_eq!(
            fields(&both).cache.as_deref(),
            Some("cache 800 read · 40.0k write")
        );
        // Output only (every input token came from the cache) is still real data.
        assert_eq!(
            fields(&usage(id, 0, 300)).tokens.as_deref(),
            Some("in 0 · out 300")
        );
    }

    /// The label keeps the true share; only the meter is capped at full.
    #[test]
    fn the_meter_is_capped_at_full() {
        let over = SessionUsageEvent {
            context: 250_000,
            context_window: Some(200_000),
            ..usage(Ulid::new(), 1, 1)
        };
        let share = context_share(&over).expect("window known");
        assert_eq!((share.label.as_str(), share.fill_percent), ("ctx 125%", 100));
        assert!(share.warning);
    }
```

`inbox.rs`, `mod tests` (end of module):

```rust
    #[test]
    fn the_status_bar_names_the_inbox_state() {
        assert_eq!(label(0), "Inbox clear");
        assert_eq!(label(1), "1 needs you");
        assert_eq!(label(3), "3 need you");
    }
```

`git_summary.rs`, `mod tests` (end of module; Task 2's tests build `GitSummary` the same way):

```rust
    fn repo(name: &str, head: Option<&str>) -> RepoSummary {
        RepoSummary {
            repo: name.to_owned(),
            branch: Some("main".to_owned()),
            head: head.map(str::to_owned),
            modified: 0,
            untracked: 0,
        }
    }

    /// 1 repo → its short hash; N repos → `N repos`; a root without `.git` still shows what its
    /// child repos hold; nothing to show → `None` (never a placeholder).
    #[test]
    fn the_head_label_is_a_hash_or_a_repo_count() {
        let hash = "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d";
        let one = GitSummary { repos: vec![repo("", Some(hash))] };
        assert_eq!(one.head_label().as_deref(), Some("1a2b3c4"));

        // Root without `.git`, one child repo.
        let child = GitSummary { repos: vec![repo("fe", Some(hash))] };
        assert_eq!(child.head_label().as_deref(), Some("1a2b3c4"));

        let many = GitSummary {
            repos: vec![repo("fe", Some(hash)), repo("be", None)],
        };
        assert_eq!(many.head_label().as_deref(), Some("2 repos"));

        // A repo before its first commit has no hash to show.
        let fresh = GitSummary { repos: vec![repo("", None)] };
        assert_eq!(fresh.head_label(), None);
        assert_eq!(GitSummary::default().head_label(), None);
    }
```

`keymap.rs` `spec_defaults`, after the `ToggleFullScreen` asserts (the `f11` block):

```rust
        assert_eq!(bound("ctrl-shift-b"), Some(Command::ToggleSidebar));
        assert_eq!(keymap.label(Command::ToggleSidebar), "Ctrl+Shift+B");
        // Global: it works while a terminal has focus.
        assert_eq!(Command::ToggleSidebar.context(), None);
```

`palette.rs` `availability_follows_the_view`, the no-workspace list (it follows `Command::all()` order):

```rust
            [
                Command::AddWorkspace,
                Command::NextInbox,
                Command::Peek,
                Command::OpenKeymapSettings,
                Command::ToggleTheme,
                Command::ToggleFullScreen,
                Command::ToggleSidebar
            ]
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib usage -j 4`
Expected: FAIL to compile (`cannot find function fields`, `no field fill_percent`).
Run: `cargo test -p mkrew-ui --lib inbox -j 4` → FAIL (`cannot find function label`).
Run: `cargo test -p mkrew-ui --lib git_summary -j 4` → FAIL (`no method head_label`).
Run: `cargo test -p mkrew-ui --lib keymap -j 4` and `palette` → FAIL (`no variant ToggleSidebar`).

- [ ] **Step 3: Implement the pure logic**

`usage.rs`. `ContextShare` gets the meter fill (the label keeps the true percentage, which can pass 100):

```rust
pub struct ContextShare {
    pub label: String,
    pub warning: bool,
    /// The share capped at 100, for the status bar's meter.
    pub fill_percent: u8,
    /// Hover text: context size of the window and the tokens used.
    pub hover: String,
}
```

In `context_share`, add the field to the literal (the rest stays as it is):

```rust
    Some(ContextShare {
        label: format!("ctx {percent}%"),
        warning: percent >= WARNING_PERCENT,
        fill_percent: u8::try_from(percent.min(100)).unwrap_or(100),
        hover: format!(
```

After `format_count`:

```rust
/// What the status bar shows of the focused agent's usage. A field with no data is `None`:
/// hidden, never `0` (SPEC §10; no money estimate, no tokens per second).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageFields {
    pub context: Option<ContextShare>,
    /// `in 12.3k · out 800` (`in` excludes cache tokens); `None` until a turn reported any.
    pub tokens: Option<String>,
    /// `cache 1.3M read · 40.0k write`; a side without tokens is left out.
    pub cache: Option<String>,
    pub model: Option<String>,
}

pub fn fields(usage: &SessionUsageEvent) -> UsageFields {
    let tokens = (usage.input.saturating_add(usage.output) > 0).then(|| {
        format!(
            "in {} · out {}",
            format_count(usage.input),
            format_count(usage.output)
        )
    });
    let cache: Vec<String> = [(usage.cache_read, "read"), (usage.cache_write, "write")]
        .into_iter()
        .filter(|(tokens, _)| *tokens > 0)
        .map(|(tokens, side)| format!("{} {side}", format_count(tokens)))
        .collect();
    UsageFields {
        context: context_share(usage),
        tokens,
        cache: (!cache.is_empty()).then(|| format!("cache {}", cache.join(" · "))),
        model: usage
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_owned),
    }
}
```

`inbox.rs`, after `next`:

```rust
/// The status bar's Inbox state: `Inbox clear`, `1 needs you`, `3 need you`.
pub fn label(waiting: usize) -> String {
    match waiting {
        0 => "Inbox clear".to_owned(),
        1 => "1 needs you".to_owned(),
        n => format!("{n} need you"),
    }
}
```

`git_summary.rs`, in `impl GitSummary` after `branch_label`:

```rust
    /// The status bar's HEAD: one repo → its short hash (`None` before its first commit);
    /// several → `3 repos`; none → `None`. The hover is [`GitSummary::branch_tooltip`].
    pub fn head_label(&self) -> Option<String> {
        match self.repos.as_slice() {
            [] => None,
            [only] => only
                .head
                .as_deref()
                .map(|head| crate::git_history::short_hash(head).to_owned()),
            many => Some(format!("{} repos", many.len())),
        }
    }
```

- [ ] **Step 4: Run the pure tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib usage -j 4`, `... inbox -j 4`, `... git_summary -j 4`
Expected: PASS (the insta snapshot of `labels_show_tokens_and_the_context_share` is unchanged: it prints `label`, `warning`, `hover` only).

- [ ] **Step 5: Command, palette and theme constants**

keymap.rs, enum after `ToggleFullScreen`:

```rust
    /// Hide or show the workspace sidebar (not remembered across restarts).
    ToggleSidebar,
```

`all()`: `Command::ToggleSidebar,` after `Command::ToggleFullScreen,`. `name()`: `Command::ToggleSidebar => "toggle_sidebar".into(),`. `default_keys()`: `Command::ToggleSidebar => "ctrl-shift-b".into(),`. `context()` needs no change (`_ => None`).

palette.rs `title()`: `Command::ToggleSidebar => "Global: Toggle sidebar".into(),`. `available()`: add `| Command::ToggleSidebar` to the always-true arm after `Command::ToggleFullScreen`.

theme.rs, after `SESSION_ROW_HEIGHT`:

```rust
/// The status bar at the window bottom (SPEC §10), in px at the default font size. It draws
/// with existing tokens (`sidebar` background, `divider` top line, `text_muted`,
/// `status_needs_input`, `accent`, `border`), so dark and light need no new values.
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
/// The context meter's track.
pub const STATUS_BAR_METER_WIDTH: f32 = 48.0;
pub const STATUS_BAR_METER_HEIGHT: f32 = 4.0;
```

Run: `cargo test -p mkrew-ui --lib keymap -j 4` and `cargo test -p mkrew-ui --lib palette -j 4`
Expected: PASS (the lib crate builds without main.rs; `every_default_parses_and_is_unique`, `names_round_trip` and `template_parses_to_the_defaults_commented_or_not` cover the new entry through `all()`). `cargo check -p mkrew-ui` fails until Step 6 adds the `Shell::run` arm; that is expected.

- [ ] **Step 6: Wire the shell (main.rs)**

State, after `usages: Usages,` in `struct Shell`, and `sidebar_hidden: false,` after `usages: Usages::default(),` in `Shell::new`:

```rust
    /// The sidebar is hidden (Toggle sidebar). Not remembered across restarts.
    sidebar_hidden: bool,
```

Next to `inbox_entries`:

```rust
    fn toggle_sidebar(&mut self, cx: &mut Context<'_, Self>) {
        self.sidebar_hidden = !self.sidebar_hidden;
        cx.notify();
    }

    /// Opens the Inbox entry after the focused pane's session (wrapping); nothing while the
    /// Inbox is empty. The key and the status bar's Inbox state both land here.
    fn jump_next_inbox(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let entries = self.inbox_entries();
        let focused = self.focused_session().and_then(|id| {
            self.sidebar
                .sessions()
                .iter()
                .find(|session| session.id == id)
        });
        if let Some(session) = inbox::next(&entries, focused) {
            self.open_session(session, window, cx);
        }
    }
```

`Shell::run`: replace the whole `Command::NextInbox => { ... }` arm and add the new one after `ToggleFullScreen`:

```rust
            Command::NextInbox => self.jump_next_inbox(window, cx),
```
```rust
            Command::ToggleSidebar => self.toggle_sidebar(cx),
```

The bar. Add after `render_inbox`:

```rust
    /// The status bar (SPEC §10): sidebar toggle, Inbox state, git HEAD; then the focused agent
    /// pane's usage. Everything is read from state the shell already keeps: it never asks the
    /// daemon and has no timer (its elapsed time refreshes with the `STATUS_TICK` redraw).
    /// The last child is the usage container; the right-panel toggle (Task 15) is appended
    /// after it.
    fn render_status_bar(
        &self,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let hover = theme.hover;
        let toggle = div()
            .id("toggle-sidebar")
            .flex_none()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(hover)))
            .tooltip(tooltip(
                theme,
                if self.sidebar_hidden {
                    "Show sidebar"
                } else {
                    "Hide sidebar"
                },
                self.keymap.label(Command::ToggleSidebar),
            ))
            .on_click(cx.listener(|shell, _, _, cx| shell.toggle_sidebar(cx)))
            .child("◧");

        let waiting = self.inbox_entries().len();
        let inbox = div()
            .id("status-inbox")
            .flex_none()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .text_color(rgb(if waiting > 0 {
                theme.status_needs_input
            } else {
                theme.text_muted
            }))
            .child(inbox::label(waiting))
            // Nothing to jump to while clear: no pointer, no hover, no click.
            .when(waiting > 0, |el| {
                el.cursor_pointer()
                    .hover(move |style| style.bg(rgb(hover)))
                    .tooltip(tooltip(
                        theme,
                        "Jump to next item needing attention",
                        self.keymap.label(Command::NextInbox),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.jump_next_inbox(window, cx);
                    }))
            });

        let git = self
            .git_summary()
            .and_then(|git| Some((git.head_label()?, git.branch_tooltip()?)))
            .map(|(label, lines)| {
                div()
                    .id("status-git")
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, &lines, ""))
                    .child(label)
            });

        let agent = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id))
            .filter(|session| session.kind == SessionKind::Agent)
            .map(|session| self.status_agent_fields(session, now, theme))
            .unwrap_or_default();

        div()
            .flex_none()
            .h(theme::rems_px(theme::STATUS_BAR_HEIGHT))
            .flex()
            .items_center()
            .gap(px(space::M))
            .px(px(space::S))
            .overflow_hidden()
            .bg(rgb(theme.sidebar))
            .border_t(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::META)
            .child(toggle)
            .child(inbox)
            .children(git)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(space::M))
                    .children(agent),
            )
    }

    /// The focused agent's fields, in order: context meter, tokens, cache, model, elapsed.
    /// Each is left out without data (never `0`): no usage entry yet, no window, no cache,
    /// no model, a state without a time (`needs input`, `done`, `exited`).
    fn status_agent_fields(
        &self,
        session: &Session,
        now: i64,
        theme: &Theme,
    ) -> Vec<gpui::AnyElement> {
        let text = |text: String| {
            div()
                .flex_none()
                .text_color(rgb(theme.text_muted))
                .child(text)
                .into_any_element()
        };
        let mut fields = Vec::new();
        if let Some(shown) = self.usages.get(session.id).map(usage::fields) {
            fields.extend(
                shown
                    .context
                    .map(|share| context_meter(&share, theme).into_any_element()),
            );
            fields.extend(shown.tokens.map(&text));
            fields.extend(shown.cache.map(&text));
            fields.extend(shown.model.map(&text));
        }
        // The pane header's time (`status::time`), so the two never disagree.
        if let Some(time) = status::time(session.state, session.state_changed_at, now) {
            fields.push(
                div()
                    .id("status-elapsed")
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(
                        theme,
                        &status::hover(
                            session.state,
                            session.state_source,
                            session.state_changed_at,
                            now,
                        ),
                        "",
                    ))
                    .child(time)
                    .into_any_element(),
            );
        }
        fields
    }
```

A free function next to `pane_header`:

```rust
/// The status bar's context meter: a thin track filled to the share, then `ctx 62%`. From
/// 80 % (`ContextShare::warning`) the fill and the text turn amber, as in the pane header.
fn context_meter(share: &ContextShare, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let color = if share.warning {
        theme.status_needs_input
    } else {
        theme.accent
    };
    div()
        .id("status-context")
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::XS))
        .text_color(rgb(if share.warning {
            theme.status_needs_input
        } else {
            theme.text_muted
        }))
        .tooltip(tooltip(theme, &share.hover, ""))
        .child(
            div()
                .w(theme::rems_px(theme::STATUS_BAR_METER_WIDTH))
                .h(px(theme::STATUS_BAR_METER_HEIGHT))
                .rounded_full()
                .bg(rgb(theme.border))
                .child(
                    div()
                        .h_full()
                        .w(relative(f32::from(share.fill_percent) / 100.0))
                        .rounded_full()
                        .bg(rgb(color)),
                ),
        )
        .child(share.label.clone())
}
```

Root of `impl Render for Shell`: wrap the sidebar in the hide check and add the bar after the row. After Task 3 the children read `render_title_bar` then the row; make them:

```rust
            .child(self.render_title_bar(now, &theme, window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    // Hidden: not built at all (nothing to lay out or paint).
                    .children((!self.sidebar_hidden).then(|| self.render_sidebar(now, &theme, cx)))
                    .child(self.render_main(now, &theme, window, cx)),
            )
            .child(self.render_status_bar(now, &theme, cx))
```

(keep Task 3's exact `render_title_bar(...)` arguments; only the sidebar child and the new last child change. The dialogs `.children(...)` stay after it.) The bar is computed after the `self.viewed.view(session)` block at the top of `render`, so the Inbox count never lags the pane that was just focused.

Run: `cargo check -p mkrew-ui -j 4`
Expected: compiles with no warnings (if `SessionKind` is not imported in main.rs, add it to the `mkrew_protocol` import; fix any borrow error in `render_status_bar` by computing `waiting`/`git`/`agent` before building `div()`, as written).

- [ ] **Step 7: SPEC**

§10 (after Task 3's rewrite). In the layout diagram add a last row before the closing `└──…┘` line:

```
├──────────────┴───────────────────────────────────────────────────────┤
│ ◧ Inbox clear · 1a2b3c4              ctx ▮▮▯ 62% · in 12.3k · out 800 · 2m │
```

After the **Title bar** bullet add:

```markdown
- **Status bar** (24 px, bottom of the window, every view): on the left the sidebar toggle
  (`◧`), the Inbox state and the git HEAD; on the right the focused agent pane's usage. Inbox:
  `Inbox clear`, or `N need you` (needs_input plus done-not-yet-viewed, as the Inbox lists);
  click = Jump to next (the same as Ctrl+Shift+I). Git: one repo → its short HEAD hash; several
  repos → `N repos`; the hover lists `branch · hash` per repo; a root without `.git` still shows
  its child repos (ADR 0029); nothing before the first commit. Agent fields (agent panes only,
  Terminals view): context meter + `ctx 62%` (amber from 80 %), `in` / `out` tokens (`in`
  excludes cache), cache read / write, model, and the time in the state (the pane header's). Only
  real data is shown: a field without data is hidden, never `0`; no money, no tokens per second.
  It redraws on `session.usage`, focus change, an Inbox change and `git.changed`, plus the
  status tick for the time; it has no timer of its own. The right end is reserved for the
  right-panel toggle.
```

In the **Sidebar** bullet, after the first sentence add: ` Toggle sidebar (Ctrl+Shift+B, status bar, palette) hides it; not remembered across restarts.`

§13, after the Toggle full screen row:

```markdown
| Global | Toggle sidebar | Ctrl+Shift+B |
```

- [ ] **Step 8: HUMAN_CHECKS**

Append to the `### To verify — 2026-10-10 inbox: redesign` section:

```markdown
- [ ] **Status bar:** a thin bar sits at the bottom in every view (Board, Terminals, Docs, Files,
  Git), in dark and light; nothing overlaps the dialogs; the `◧` glyph renders (not a box).
- [ ] **Toggle sidebar:** the `◧` button, Ctrl+Shift+B (also with a terminal focused) and the
  palette's "Global: Toggle sidebar" hide and show the sidebar; panes fill the width and the shell
  inside re-wraps; the tooltip flips between "Hide sidebar" and "Show sidebar"; the sidebar is back
  after a restart (not remembered).
- [ ] **Status bar Inbox:** with nothing waiting it reads `Inbox clear` (no pointer, no click);
  when an agent asks a question it reads `1 needs you` in amber; clicking it jumps to that pane
  like Ctrl+Shift+I; focusing a done pane makes it count down.
- [ ] **Status bar git:** one repo → the 7-char HEAD hash (hover: `branch · hash`); a folder with
  no `.git` holding two repos → `2 repos`, hover lists both; with one child repo it shows that
  hash; a new repo with no commit shows nothing; after `git commit` the hash changes without
  reopening the Git view.
- [ ] **Status bar agent fields:** focus a claude / codex / omp pane: after a turn the bar shows
  the context meter and `ctx N%`, `in … · out …`, cache and the model; the time counts up
  (`working · 12s` in the pane header and `12s` in the bar agree, also at 1m and 1h 5m). A fresh
  agent with no turn yet shows no usage fields at all (no `0`). A shell pane, and the Board / Docs
  / Files / Git views, show none. An agent whose profile has no `[usage]` shows no usage fields.
- [ ] **Status bar redraw:** with Task Manager open and every pane idle, the app's CPU stays at
  zero (the bar adds no timer).
```

- [ ] **Step 9: Verify and tick**

Run the full verification. Tick inbox line 54 `[x]` (last task of item 3).
Suggested message: `feat(ui): status bar with inbox, git HEAD and agent usage; toggle sidebar`

---

### Task 7: Bug: Change color keeps the current view (workspace menu occludes the rows under it)

Inbox: line 61 `Workspace sidebar redesign + cleanup`, bullet `After right-click → Change color on a project, stay on the current view instead of navigating to that project` (not the last task of item 4: do not tick yet). Spec: §10 (:690-698).

**Current state (investigated, systematic-debugging):**
- Symptom path, traced in code. The workspace menu is `Shell::render_workspace_menu` (main.rs). Its `panel` div is wrapped in `deferred(anchored(...))`, so it paints over the rows below its workspace row. The panel has `.on_mouse_down_out(...)` but **no `.occlude()`**. The color choice is `color_swatch(...)` (main.rs), whose `.on_click` calls `Shell::set_workspace_color` → `Shell::update_project`, which sets `self.workspace_menu = None`.
- Why the click leaks (gpui-pre-0.3.7): `Window`'s hit test (`window.rs`, `fn hit_test`) walks the hitboxes front to back and stops only at a hitbox whose behavior is `HitboxBehavior::BlockMouse` (`InteractiveElement::occlude`). Without it, the row behind the menu is still "hovered" at mouse down, so its `pending_mouse_down` is set (`div.rs`, the `MouseDownEvent` listener checks `hitbox.is_hovered(window)`). At mouse up the swatch's click fires first and closes the menu; then the same event bubbles to the row behind, whose click listener runs. The row behind is a `workspace_row` (`on_click` → `activate_workspace` → the view moves to that workspace) or a `session_row` (`on_click` → `open_session`). That is the reported navigation.
- Pattern check (what works): the Run menu panel (`render_run_menu`, id `run-menu-panel`), the "Run with" panel (`run_with_button`, id `run-with-menu`) and the "Several agents" panel (`fan_out_button`, id `fan-out-menu`) all call `.occlude()` before `.on_mouse_down_out(...)`. The comment in `run_with_button`'s pick closure ("Not a click on the card under the menu (which would open its terminal)") documents this same leak. The workspace menu is the only deferred menu without it.
- Every other `deferred(`/`anchored(` in `crates/mkrew-ui/src` (grep `deferred\(|anchored\(`): `render_peek` (the hover popover, id `peek`) and the three above. The peek panel is **left alone on purpose**: its parent `peek-area` has an `on_hover` (`hover_peek`), and an occluding child would make the parent not hovered while the mouse is over the panel (the hit test stops at the child), so the peek would close under the mouse. Tasks 10 and 12 add the tab and pane menus; they build their panel with the helper below.
- Shared cause: four panels copy the same 9 style lines, and one copy forgot `.occlude()`. The root-cause fix is one helper that owns `.occlude()`, used by all four. The change cannot regress when a menu is added later.
- No GPUI test harness exists in `mkrew-ui` (grep `TestAppContext|gpui::test|VisualTestContext|test-support` finds nothing; `Cargo.toml` dev-dependencies are `insta`, `sysinfo`, `tempfile`; the hit test needs a laid-out window). So there is no unit test for the click path; the regression check is a HUMAN_CHECKS line (below). The pure parts of the flow are unchanged.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (new `menu_panel` next to `menu_item`; `render_workspace_menu`, `render_run_menu`, `run_with_button`, `fan_out_button`)
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces: `fn menu_panel(id: impl Into<gpui::ElementId>, theme: &Theme) -> gpui::Stateful<gpui::Div>` in main.rs: a floating menu panel (width `theme::MENU_WIDTH`, column, `space::XS` padding, `radius::MEDIUM`, `theme.border` border, `theme.surface_raised`, `shadow_md`, UI font) that occludes the mouse. Tasks 10 and 12 use it for `TabMenu` / `PaneMenu`.

- [ ] **Step 1: Add the helper**

In main.rs, right after `fn menu_item(...)` (find it with `grep -n "^fn menu_item"`):

```rust
/// The floating panel of a menu drawn in a `deferred(anchored(..))` (workspace, Run, Run with,
/// Several agents, tab and pane menus). It occludes the mouse: gpui hit-tests every hitbox
/// until one blocks, so without `.occlude()` a click on the menu also reaches the row or card
/// painted under it (a color pick would navigate to the row behind the swatch).
fn menu_panel(id: impl Into<gpui::ElementId>, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .w(px(theme::MENU_WIDTH))
        .flex()
        .flex_col()
        .p(px(space::XS))
        .rounded(px(radius::MEDIUM))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface_raised))
        .shadow_md()
        .font_family(theme.ui_font_family.clone())
        .occlude()
}
```

- [ ] **Step 2: Use it in the four panels**

`render_workspace_menu`: replace the `let panel = div().id("workspace-menu") … .font_family(theme.ui_font_family.clone())` chain (everything before `.on_mouse_down_out(`) with the helper; the `.on_mouse_down_out(...)` and all `.child(...)` lines stay as they are:

```rust
        let panel = menu_panel("workspace-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.workspace_menu = None;
                cx.notify();
            }))
```

`render_run_menu`: the same, id `"run-menu-panel"`, dropping its own `.occlude()`:

```rust
        let panel = menu_panel("run-menu-panel", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.run_menu = None;
                cx.notify();
            }))
```

`run_with_button`: id `"run-with-menu"`, `shell.run_with = None`. `fan_out_button`: id `"fan-out-menu"`, `shell.fan_out = None`. In each, delete the old `div().id(..).w(..) … .occlude()` lines and keep the following `.on_mouse_down_out(...)` and `.child(...)` lines.

Run: `cargo check -p mkrew-ui -j 4`
Expected: compiles with no warnings (`Stateful`, `ElementId` paths are already used in main.rs as `gpui::Stateful` / `gpui::ElementId`).

- [ ] **Step 3: HUMAN_CHECKS**

Create the section at the top of "To verify (non-blocking)" if it is not there yet (Task 1 normally did):
`### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)`. Append to it:

```markdown
- [ ] **Change color stays put:** with workspace A shown, right-click workspace B (or its `⋯`) →
  pick a color. The view stays on A (no switch to B, no session opened), the menu closes, B's
  avatar takes the color. Repeat with the menu sitting over session rows of B and over A's rows;
  also click the menu's padding and a separator: nothing behind reacts. The Run menu (▾), "Run
  with…" and "Several agents…" still close on a click outside.
```

- [ ] **Step 4: Verify (no tick)**

Run: `cargo test -p mkrew-ui --lib -j 4` → PASS (nothing pure changed). Do not tick inbox line 61 yet (Task 9 ticks it).
Suggested message: `fix(ui): menu panels occlude the mouse so a color pick no longer clicks the row behind`

---

### Task 8: New workspace color palette (9 distinct hues, readable in light and dark)

Inbox: line 61 `Workspace sidebar redesign + cleanup`, bullet `Replace the project color palette with 8–10 clearly distinct hues that are readable in both light and dark themes` (not the last task of item 4: do not tick yet). Spec: §14 (:1064-1066), §10 (:697), ADR 0020.

**Current state (investigated):**
- `project.color` is stored as a **name** (`red`…`pink`), migration `0013_project_color.sql`. The list is `mkrew_protocol::WORKSPACE_COLORS: [&str; 8]` (message.rs, next to `SessionKind`). The daemon (`server.rs`, the `ProjectUpdate` branch `if let Some(Some(color)) = &params.color && !WORKSPACE_COLORS.contains(..)`) refuses any other name using that constant, so it needs **no code change**.
- The UI maps a name to a color by position: `Theme::workspace_color(name)` = `WORKSPACE_COLORS.iter().position(..)` indexing `Theme.workspace_colors: [u32; 8]` (theme.rs; dark in `Theme::dark`, light in `Theme::light`, zeros in `shared()`). Other readers: `diagram.rs::token_color` (`workspace_colors[i % len]`, slice colors) and its test (`workspace_colors[0]`); `color_swatch` and `render_workspace_menu` (`WORKSPACE_COLORS.iter()`), `workspace_avatar`.
- Today's weakness (measured, CIELAB ΔE between neighbours): dark `blue`/`purple` ΔE 22, light `orange`/`yellow` ΔE 21, `red`/`orange` 23–33; the hues are 8 pastel-ish steps with a missing lime. Existing tests: `workspace_colors_stand_out_from_the_surfaces` (≥ 3:1 on `bg`, `surface`, `surface_raised` only) and `avatar_letters_and_chevrons_stay_readable` (letter ≥ 4.5:1 on the tint over `sidebar`, `hover`, `selected`).
- Decision (names): add one name, `lime`, between `yellow` and `green`; every old name stays valid, so stored values and the daemon check keep working. 9 hues is inside the requested 8–10.
- Decision (protocol): **no `PROTOCOL_VERSION` bump**. `color` is already an optional string; adding an accepted value changes no wire shape, and ADR 0020 already says the protocol stays put for colors. An old UI talking to the new daemon shows no color for `lime` (unknown name → `None`), it does not break.
- Decision (docs): **amend ADR 0020** (an `## Amendment 2026-10-10` section, like the 0027 one) and update SPEC §10 and §14. No new ADR number: the decision (named colors, per-appearance tokens, ≥ 3:1) is unchanged; only the list and its values change.
- New values (OKLCH-style even hue steps, checked with a script: min contrast over all 7 surfaces `bg surface surface_raised sidebar card hover selected` is **4.89** dark / **3.34** light, min hue gap 21° dark / 13.7° light, min ΔE 38 dark / 23 light):

  | name | dark | light |
  | --- | --- | --- |
  | red | `0xF87171` | `0xDC2626` |
  | orange | `0xFB923C` | `0xD9560B` |
  | yellow | `0xFACC15` | `0xA16207` |
  | lime | `0xA3E635` | `0x4D7C0F` |
  | green | `0x4ADE80` | `0x15803D` |
  | teal | `0x2DD4BF` | `0x0F766E` |
  | blue | `0x60A5FA` | `0x2563EB` |
  | purple | `0xC084FC` | `0x9333EA` |
  | pink | `0xF472B6` | `0xDB2777` |

  The avatar letter is `theme.text` on a low-alpha tint (`tint_alpha` 0x2E dark / 0x24 light), so it stays ≥ 4.5:1 for every hue (worst case computed: 8.0 dark, 11.8 light). The existing avatar test checks this for the new list.

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs` (`WORKSPACE_COLORS`, a test in its `mod tests`)
- Modify: `crates/mkrew-ui/src/theme.rs` (`workspace_colors` type and three values, `workspace_color` doc, tests)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (`workspace_colors_are_set_cleared_and_checked`, import)
- Modify: `docs/SPEC.md` §10 (:697), §14 (:1064-1066); `docs/decisions/0020-workspace-colors.md`; `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces (Tasks 12 and 14 consume): `mkrew_protocol::WORKSPACE_COLORS: [&str; 9] = ["red", "orange", "yellow", "lime", "green", "teal", "blue", "purple", "pink"]` (the order is the palette order); `Theme.workspace_colors: [u32; WORKSPACE_COLORS.len()]`; `Theme::workspace_color(&self, name: &str) -> Option<u32>` (unchanged signature); `color_swatch(project_id, color, current, theme, cx)` in main.rs is **not** changed.

- [ ] **Step 1: Failing tests**

theme.rs `mod tests`: replace `workspace_colors_stand_out_from_the_surfaces` with:

```rust
    /// Hue angle (0..360) of an `0xRRGGBB` color.
    fn hue(color: u32) -> f32 {
        let [r, g, b] = [16, 8, 0].map(|shift| ((color >> shift) & 0xFF) as f32);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let d = max - min;
        if d == 0.0 {
            return 0.0;
        }
        let sector = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        sector * 60.0
    }

    #[test]
    fn workspace_colors_stand_out_from_every_surface_and_from_each_other() {
        for theme in [Theme::dark(), Theme::light()] {
            // Where a swatch or avatar sits: the dialogs, the menu, the sidebar and its row fills.
            let surfaces = [
                theme.bg,
                theme.surface,
                theme.surface_raised,
                theme.sidebar,
                theme.card,
                theme.hover,
                theme.selected,
            ];
            for (name, color) in WORKSPACE_COLORS.iter().zip(theme.workspace_colors) {
                for bg in surfaces {
                    // WCAG 1.4.11: graphical objects ≥ 3:1.
                    let ratio = contrast(color, bg);
                    assert!(
                        ratio >= 3.0,
                        "{name} {color:06X} on {bg:06X} (dark: {}): {ratio:.2}",
                        theme.dark
                    );
                }
            }
            // Clearly distinct: neighbouring hues (round the color wheel) differ by ≥ 12°.
            let mut hues: Vec<f32> = theme.workspace_colors.iter().map(|c| hue(*c)).collect();
            hues.sort_by(f32::total_cmp);
            for pair in hues.windows(2) {
                assert!(pair[1] - pair[0] >= 12.0, "hues too close: {hues:?}");
            }
            let wrap = hues[0] + 360.0 - hues[hues.len() - 1];
            assert!(wrap >= 12.0, "hues too close round the wheel: {hues:?}");
            // Every name has a color; a hex value or unknown name has none.
            for name in WORKSPACE_COLORS {
                assert!(theme.workspace_color(name).is_some(), "{name}");
            }
            let teal = WORKSPACE_COLORS.iter().position(|n| *n == "teal").unwrap();
            assert_eq!(
                theme.workspace_color("teal"),
                Some(theme.workspace_colors[teal])
            );
            assert_eq!(theme.workspace_color("#ff0000"), None);
        }
    }

    #[test]
    fn the_palette_keeps_every_name_a_workspace_may_already_have() {
        for old in ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"] {
            assert!(Theme::dark().workspace_color(old).is_some(), "{old}");
        }
        assert!(Theme::dark().workspace_color("lime").is_some());
    }
```

(`WORKSPACE_COLORS` is already imported at the top of theme.rs; `.unwrap()` is fine in tests.)

message.rs `mod tests` (find it with `grep -n "mod tests" crates/mkrew-protocol/src/message.rs`), add:

```rust
    /// A stored `project.color` must keep passing the daemon's check after the palette grew.
    #[test]
    fn workspace_colors_keep_the_names_stored_before_lime_was_added() {
        for old in ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"] {
            assert!(WORKSPACE_COLORS.contains(&old), "{old}");
        }
        assert!(WORKSPACE_COLORS.contains(&"lime"));
        let mut sorted = WORKSPACE_COLORS;
        sorted.sort_unstable();
        assert!(sorted.windows(2).all(|pair| pair[0] != pair[1]), "unique names");
    }
```

daemon.rs: add `WORKSPACE_COLORS` to the `use mkrew_protocol::{ … }` list (line ~19-30, alphabetical position after `TerminalTab` if sorted) and extend `workspace_colors_are_set_cleared_and_checked`, after the `teal` assertions and before the `#ff0000` check:

```rust
    // Every palette name is accepted, including `lime` (added in 2026-10-10).
    for name in WORKSPACE_COLORS {
        let set: Project = client.call(color(Some(name))).into_result().unwrap();
        assert_eq!(set.color.as_deref(), Some(name));
    }
```

Run: `cargo test -p mkrew-ui --lib theme::tests::workspace_colors -j 4`
Expected: FAIL on `lime` (`assertion failed: theme.workspace_color(name).is_some()` is not reached first: the `the_palette_keeps_every_name…` test fails with `left: None` for `lime`). The protocol and daemon tests fail the same way once the list lacks `lime`: `cargo test -p mkrew-protocol workspace_colors_keep -j 4` → FAIL (`assertion failed: WORKSPACE_COLORS.contains(&"lime")`).

- [ ] **Step 2: Implement**

message.rs:

```rust
/// Workspace color names (SPEC §14, ADR 0020); clients map them to theme tokens. The order is
/// the palette order of `Theme.workspace_colors`; names are only ever added, because they are
/// stored in `project.color`.
pub const WORKSPACE_COLORS: [&str; 9] = [
    "red", "orange", "yellow", "lime", "green", "teal", "blue", "purple", "pink",
];
```

theme.rs: in the `Theme` struct:

```rust
    /// Workspace colors, in [`WORKSPACE_COLORS`] order (≥ 3:1 on every surface, ADR 0020).
    pub workspace_colors: [u32; WORKSPACE_COLORS.len()],
```

`Theme::dark()`:

```rust
            workspace_colors: [
                0xF87171, 0xFB923C, 0xFACC15, 0xA3E635, 0x4ADE80, 0x2DD4BF, 0x60A5FA, 0xC084FC,
                0xF472B6,
            ],
```

`Theme::light()`:

```rust
            workspace_colors: [
                0xDC2626, 0xD9560B, 0xA16207, 0x4D7C0F, 0x15803D, 0x0F766E, 0x2563EB, 0x9333EA,
                0xDB2777,
            ],
```

`shared()`: `workspace_colors: [0; WORKSPACE_COLORS.len()],`.

Nothing else changes: `diagram.rs::token_color` indexes modulo the length, its test reads `workspace_colors[0]` (now red `F87171` / `DC2626`: the diagram test builds its expectation from the theme, so it follows). The daemon check, the menu loop and `color_swatch` read `WORKSPACE_COLORS`, so the new swatch appears by itself.

Run: `cargo test -p mkrew-ui --lib theme -j 4`, `cargo test -p mkrew-ui --lib diagram -j 4`, `cargo test -p mkrew-protocol workspace_colors -j 4`
Expected: PASS (if `workspace_colors_stand_out_from_every_surface_and_from_each_other` reports a ratio below 3 or a hue gap below 12, the message names the color: fix that value, do not lower the bound).
Run: `cargo test -p mkrew-daemon --test daemon workspace_colors -j 4` → PASS.

- [ ] **Step 3: ADR 0020 amendment and SPEC**

Append to `docs/decisions/0020-workspace-colors.md`:

```markdown

## Amendment 2026-10-10 (inbox redesign, item 4)

The palette was too close in places (dark blue/purple, light orange/yellow) and had no lime. It
is now nine names: red, orange, yellow, **lime**, green, teal, blue, purple, pink. Every old name
stays valid (stored values and the daemon check are untouched; `lime` is the only new name); the
protocol stays as it is (`color` was already an optional string). The per-appearance tokens were
re-picked: neighbouring hues differ by at least 12° on the color wheel, and each color is ≥ 3:1
on `bg`, `surface`, `surface-raised`, `sidebar`, `card`, `hover` and `selected` in both themes
(tests in `theme.rs`). Values are in SPEC §14.
```

SPEC §10, the **Color** bullet (:697): change `one of 8 colors` to `one of 9 colors`.

SPEC §14 (:1064-1066), replace the bullet with:

```markdown
- Workspace colors (red, orange, yellow, lime, green, teal, blue, purple, pink): dark #F87171
  #FB923C #FACC15 #A3E635 #4ADE80 #2DD4BF #60A5FA #C084FC #F472B6 / light #DC2626 #D9560B
  #A16207 #4D7C0F #15803D #0F766E #2563EB #9333EA #DB2777 (≥ 3:1 on every surface; neighbouring
  hues ≥ 12° apart).
```

- [ ] **Step 4: HUMAN_CHECKS**

Append to the `### To verify — 2026-10-10 inbox: redesign` section:

```markdown
- [ ] **Workspace palette:** the workspace menu shows 9 swatches + "no color"; in dark and in light
  every swatch is easy to tell from its neighbours and visible on the menu background; give
  workspaces 3 neighbouring colors (e.g. red, orange, yellow) and check the sidebar avatars (letter
  on a tint) are distinct and the letters stay readable on the hovered and the selected row. A
  workspace that was `teal`/`pink`/… before keeps its color (a new shade of the same name).
```

- [ ] **Step 5: Verify (no tick)**

Run `cargo test -p mkrew-protocol -j 4`, `cargo test -p mkrew-ui --lib -j 4`, `cargo test -p mkrew-daemon --test daemon workspace_colors -j 4`. Do not tick inbox line 61 yet.
Suggested message: `feat(ui): nine distinct workspace colors, readable in light and dark (adds lime)`

---

### Task 9: Sidebar: filter, chips, header buttons, row text; Inbox card removed

Inbox: line 61 `Workspace sidebar redesign + cleanup` (**last task of item 4: tick it at the end**). Spec: §10 (:690-692), §11 (:760-777), §13 (:977), §20A.1/.3/.4 (:1397-1413), acceptance (:1260-1264, :1270), §20E.3 (:1592). Contracts §6 (the selected row's second line; Task 18 adds the plan progress bar under it).

**Current state (investigated):**
- Sidebar (main.rs `Shell::render_sidebar`, after Task 3 the logo is gone from its header): a header (`Workspaces` + `icon_button("add-workspace", "+")`), error lines, then the body from `self.sidebar.rows()`, then `render_inbox(entries, ..)` pinned below as a card (`entries` from `inbox::entries`). `Sidebar::rows()` (sidebar.rs) filters all sessions once per project, i.e. O(projects × sessions).
- Inbox pieces and who uses them:
  - `inbox::entries(projects, sessions, &Viewed)` = every `NeedsInput` session and every `Done` one not yet viewed, newest first. Users that stay: `Shell::inbox_entries()` (the `Peek` command, `update_peek`, peek ↑/↓ cycle), `jump_next_inbox` / `inbox::next` (Task 6: Ctrl+Shift+I and the status bar), the status bar count (Task 6). `Viewed` stays (`Shell::render` marks the focused pane viewed).
  - Only the Inbox card uses: `Shell::render_inbox`, `inbox_row`, `Shell::read_last_lines` + `Shell.last_lines` + `inbox::LastLines` (and its test `last_lines_are_read_once_per_appearance`), `Place::Inbox` in peek.rs, `theme::INBOX_TILE_SIZE`.
- Row texts today: `session_row` (main.rs) shows `agent · title` and, on the right, `row.status_text(now)` (e.g. `idle · 17m`, `needs input`, `working · 2m`), with the status tooltip on that text. `workspace_row` shows the working pill, the `?` marker and the token total `usage::format_count(tokens)` from `Usages::workspace_total` (called per row per frame).
- Text field reuse: `text_field(id, text, placeholder, focused, selected, theme)` (main.rs), `Input` (text_box.rs), `find_key`/`FindKey` (terminal/find.rs; the rename field uses them: `Shell::rename_key_down`), `Shell::typing(focus, on_key, cx)` (IME), `Shell.rename_focus` pattern. Single-line paste: `.replace(['\r', '\n'], " ")` as in `rename_key_down`.
- Peek: `Peek.place: Place` (peek.rs) only decides the ↑/↓ cycle (Inbox only) and the hint text. With the card gone every peek is a sidebar peek. Hover peek on session rows stays (`with_peek`, `hover_row`). The keyboard peek (`Command::Peek`, first needs-you session) hangs on that session's row, so the row must be visible: the filter and a collapsed workspace would hide it → `Sidebar::reveal`.
- Rendering cost: `render_sidebar` runs every frame the shell renders. After this task it calls `Sidebar::view` once: one pass over sessions grouped per project (O(sessions + projects)); the per-workspace sort of the group is the one `n log n` that was already there.
- Decisions (open questions with defaults): the filter box, chips and header stay pinned at the top while the list scrolls; the workspace row keeps the `?` marker (urgency while collapsed) but drops the working-dots pill (the summary text says `N working`); `done` sessions are not named in the workspace summary (only `working`, `needs you` = `needs_input`, `idle`); while a filter or chip is active, workspaces without a match are hidden and the kept ones are shown open, the user's open/closed choice returns when the filter clears; filter text and chip are in memory only.
- Not touched: `turn::status_text` (pub, tested; its Inbox-row caller goes away, the function stays as library API for the pane header's summary), `Usages::workspace_total` is removed (step 8) because its only caller was the workspace row.

**Files:**
- Modify: `crates/mkrew-ui/src/inbox.rs` (`needs_you`, `entries` uses it, delete `LastLines`, docs, tests)
- Modify: `crates/mkrew-ui/src/sidebar.rs` (`Chip`, `Counts`, `View`, `matches`, `workspace_summary`, filter state, `view`/`rows`, `SessionRow::time_text`, `WorkspaceRow.summary`, tests)
- Modify: `crates/mkrew-ui/src/peek.rs` (delete `Place`; `Peek::new`, `Hover`, tests)
- Modify: `crates/mkrew-ui/src/usage.rs` (delete `Usages::workspace_total`, trim its test)
- Modify: `crates/mkrew-ui/src/theme.rs` (delete `INBOX_TILE_SIZE`)
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell.filter_focus`, `filter_key_down`, `render_sidebar`, `workspace_row`, `session_row`; delete `render_inbox`, `inbox_row`, `read_last_lines`, `last_lines`; peek functions)
- Modify: `crates/mkrew-ui/tests/lifecycle.rs` only if `rows()` stops compiling (it does not: `rows()` keeps its signature)
- Modify: `docs/SPEC.md` §10, §11, §13, §20A, acceptance lines, §20E.3; Create: `docs/decisions/0036-sidebar-filter-replaces-inbox-card.md`; Modify: `docs/HUMAN_CHECKS.md`, `docs/INBOX.md` (tick)

**Interfaces:**
- Consumes: `inbox::entries`, `inbox::Viewed`, `history::title(&Session) -> Option<&str>`, `status::time(state, changed_at, now_ms) -> Option<String>`, Task 6's `jump_next_inbox`, `inbox_entries()` (unchanged).
- Produces:
  ```rust
  // inbox.rs
  pub fn needs_you(session: &Session, viewed: &Viewed) -> bool; // NeedsInput, or Done and not yet viewed
  // sidebar.rs
  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
  pub enum Chip { #[default] All, Working, NeedsYou }
  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
  pub struct Counts { pub all: usize, pub working: usize, pub needs_you: usize }
  pub struct View { pub counts: Counts, pub rows: Vec<Row> }
  pub fn matches(query: &str, fields: [&str; 3]) -> bool;       // query already lowercase
  pub fn workspace_summary(working: usize, asking: usize, idle: usize) -> Option<String>;
  impl Sidebar {
      pub fn view(&self, viewed: &Viewed) -> View;
      pub fn rows(&self) -> Vec<Row>;                           // view() with nothing viewed; tests/lifecycle
      pub fn filter(&self) -> &Input;  pub fn filter_mut(&mut self) -> &mut Input;
      pub fn chip(&self) -> Chip;      pub fn set_chip(&mut self, chip: Chip);
      pub fn filtering(&self) -> bool;                          // a query or a chip other than All
      pub fn any_expanded(&self) -> bool;
      pub fn toggle_all(&mut self);                             // any open → close all, else open all
      pub fn reveal(&mut self, project_id: Ulid);               // open it, clear query and chip
  }
  // WorkspaceRow gains: pub summary: Option<String>
  // SessionRow gains: pub fn time_text(&self, now_ms: i64) -> Option<String>  (replaces status_text)
  // peek.rs: Peek::new(session_id: Ulid, focused: bool); Hover::{enter(session_id)->u64, leave(session_id), fire(serial)->Option<Ulid>, is_over(session_id)->bool}; Place is removed
  ```

- [ ] **Step 1: Failing tests, `inbox.rs`**

`mod tests` of inbox.rs (uses its `project`, `session` helpers), next to `sessions_of_unknown_workspaces_are_left_out`:

```rust
    #[test]
    fn needs_you_is_input_wanted_or_done_and_unseen() {
        let api = project("api");
        let asking = session(&api, SessionState::NeedsInput, 1);
        let done = session(&api, SessionState::Done, 2);
        let seen = session(&api, SessionState::Done, 3);
        let working = session(&api, SessionState::Working, 4);
        let idle = session(&api, SessionState::Idle, 5);
        let mut viewed = Viewed::default();
        viewed.view(&seen);
        let all = [asking.clone(), done.clone(), seen.clone(), working, idle];
        let flagged: Vec<Ulid> = all
            .iter()
            .filter(|s| needs_you(s, &viewed))
            .map(|s| s.id)
            .collect();
        assert_eq!(flagged, vec![asking.id, done.id]);
        // The chips and the status bar count exactly what the Inbox lists.
        let listed: Vec<Ulid> = entries(&[api], &all, &viewed).iter().map(|e| e.session_id).collect();
        assert_eq!(listed.len(), flagged.len());
        assert!(flagged.iter().all(|id| listed.contains(id)));
    }
```

Run: `cargo test -p mkrew-ui --lib inbox::tests::needs_you -j 4`
Expected: FAIL to compile (`cannot find function needs_you`).

- [ ] **Step 2: `inbox.rs` implementation**

Module doc (lines 1-2) becomes:

```rust
//! The "needs you" set (SPEC §10, §11): every session that needs input, or is done and not yet
//! viewed, newest first. It feeds the sidebar's `Needs you` chip, the status bar and "next
//! Inbox entry". Pure and tested headlessly.
```

Before `entries`, add the predicate and use it:

```rust
/// A session that needs the user: it asks for input, or it finished and its pane was not
/// focused since (a later `done` shows again). The one definition behind the Inbox entries,
/// the sidebar chip and the status bar.
pub fn needs_you(session: &Session, viewed: &Viewed) -> bool {
    match session.state {
        SessionState::NeedsInput => true,
        SessionState::Done => !viewed.seen(session),
        _ => false,
    }
}
```

In `entries`, replace the `.filter(|session| match session.state { … })` closure with `.filter(|session| needs_you(session, viewed))`.

Delete the whole `LastLines` struct and `impl` (the doc comment block "The last non-blank screen line of each Inbox entry (§20A.4)…" through its closing `}`) and the test `last_lines_are_read_once_per_appearance`. `use std::collections::HashMap;` stays (`Viewed`).

Run: `cargo test -p mkrew-ui --lib inbox -j 4`
Expected: PASS. (main.rs does not compile until Step 7: do not run it.)

- [ ] **Step 3: Failing tests, `sidebar.rs`**

`mod tests` in sidebar.rs. Add to the `use` block: `use crate::inbox::Viewed;`. Existing helpers `project`, `session`, `workspace`, `session_ids` are reused. Add:

```rust
    #[test]
    fn the_filter_matches_workspace_agent_and_title() {
        assert!(matches("api", ["API server", "claude", ""]));
        assert!(matches("codex", ["api", "codex", ""]));
        assert!(matches("login", ["api", "claude", "Fix Login"]));
        assert!(!matches("login", ["api", "claude", ""]));
        // Every word has to hit some field, not the same one.
        assert!(matches("api login", ["api", "claude", "Fix login"]));
        assert!(!matches("api nope", ["api", "claude", "Fix login"]));
        assert!(matches("", ["api", "claude", ""]));
    }

    #[test]
    fn a_filter_keeps_matching_sessions_and_opens_their_workspace() {
        let (api, web) = (project("api"), project("web"));
        let login = Session {
            title: Some("Fix login".into()),
            ..session(&api, 1, SessionState::Idle)
        };
        let other = session(&api, 2, SessionState::Idle);
        let codex = Session {
            agent_profile: "codex".into(),
            ..session(&web, 3, SessionState::Working)
        };
        let mut sidebar = Sidebar::default();
        sidebar.load(
            vec![api.clone(), web.clone()],
            vec![login.clone(), other, codex.clone()],
        );
        sidebar.toggle_all();
        assert!(!sidebar.any_expanded(), "everything was open, now closed");
        assert_eq!(sidebar.rows().len(), 2);

        sidebar.filter_mut().insert("login");
        let rows = sidebar.rows();
        assert_eq!(rows.len(), 2, "api and its one match: {rows:?}");
        assert!(workspace(&rows[0]).expanded, "a filter opens what it keeps");
        assert_eq!(session_ids(&rows), vec![login.id]);

        // A workspace name matches all of its sessions; so does an agent name.
        for query in ["WEB", "codex"] {
            sidebar.filter_mut().clear();
            sidebar.filter_mut().insert(query);
            assert_eq!(session_ids(&sidebar.rows()), vec![codex.id], "{query}");
        }
        sidebar.filter_mut().clear();
        sidebar.filter_mut().insert("zzz");
        assert!(sidebar.rows().is_empty());

        // Clearing the filter brings the user's own open/closed state back.
        sidebar.filter_mut().clear();
        assert!(sidebar.rows().iter().all(|row| !matches!(row, Row::Session(_))));
        assert_eq!(sidebar.rows().len(), 2);
    }

    #[test]
    fn chips_count_what_matches_the_query_and_narrow_the_rows() {
        let api = project("api");
        let working = session(&api, 1, SessionState::Working);
        let asking = session(&api, 2, SessionState::NeedsInput);
        let done = session(&api, 3, SessionState::Done);
        let seen = session(&api, 4, SessionState::Done);
        let idle = session(&api, 5, SessionState::Idle);
        let mut sidebar = Sidebar::default();
        sidebar.load(
            vec![api.clone()],
            vec![working.clone(), asking.clone(), done.clone(), seen.clone(), idle],
        );
        let mut viewed = Viewed::default();
        viewed.view(&seen);

        let counts = sidebar.view(&viewed).counts;
        assert_eq!(counts, Counts { all: 5, working: 1, needs_you: 2 });
        // "Needs you" is exactly what the Inbox lists.
        let listed = crate::inbox::entries(sidebar.projects(), sidebar.sessions(), &viewed);
        assert_eq!(counts.needs_you, listed.len());

        sidebar.set_chip(Chip::NeedsYou);
        assert_eq!(session_ids(&sidebar.view(&viewed).rows), vec![done.id, asking.id]);
        sidebar.set_chip(Chip::Working);
        let view = sidebar.view(&viewed);
        assert_eq!(session_ids(&view.rows), vec![working.id]);
        assert_eq!(view.counts.all, 5, "chips count, they do not shrink their own numbers");

        // The query narrows the counts too.
        sidebar.set_chip(Chip::All);
        sidebar.filter_mut().insert("nothing like it");
        assert_eq!(sidebar.view(&viewed).counts, Counts::default());
    }

    #[test]
    fn the_workspace_summary_names_what_is_going_on() {
        assert_eq!(workspace_summary(1, 0, 0).as_deref(), Some("1 working"));
        assert_eq!(workspace_summary(0, 0, 2).as_deref(), Some("2 idle"));
        assert_eq!(
            workspace_summary(1, 1, 2).as_deref(),
            Some("1 working · 1 needs you · 2 idle")
        );
        assert_eq!(workspace_summary(0, 0, 0), None);
    }

    #[test]
    fn a_workspace_row_carries_its_summary_and_a_session_row_only_a_time() {
        let api = project("api");
        let mut sidebar = Sidebar::default();
        sidebar.load(
            vec![api.clone()],
            vec![
                session(&api, 1, SessionState::Working),
                session(&api, 2, SessionState::Idle),
                session(&api, 3, SessionState::Idle),
            ],
        );
        let rows = sidebar.rows();
        assert_eq!(workspace(&rows[0]).summary.as_deref(), Some("1 working · 2 idle"));

        let row = |state| SessionRow {
            session_id: Ulid::new(),
            project_id: api.id,
            agent: "claude".into(),
            title: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
        };
        // The status icon says the state; the right side is only the time (working / idle).
        assert_eq!(row(SessionState::Working).time_text(120_000).as_deref(), Some("2m"));
        assert_eq!(row(SessionState::Idle).time_text(17 * 60_000).as_deref(), Some("17m"));
        assert_eq!(row(SessionState::NeedsInput).time_text(120_000), None);
        assert_eq!(row(SessionState::Done).time_text(120_000), None);
    }

    #[test]
    fn toggle_all_closes_when_any_is_open_and_reveal_clears_what_hides_a_session() {
        let (api, web) = (project("api"), project("web"));
        let s = session(&web, 1, SessionState::Idle);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone(), web.clone()], vec![s.clone()]);
        assert!(sidebar.any_expanded(), "web has a live session");
        sidebar.toggle_all();
        assert!(!sidebar.any_expanded());
        sidebar.toggle_all();
        assert!(workspace(&sidebar.rows()[0]).expanded && sidebar.any_expanded());

        sidebar.toggle_all();
        sidebar.filter_mut().insert("api");
        sidebar.set_chip(Chip::Working);
        assert!(sidebar.filtering());
        sidebar.reveal(web.id);
        assert!(!sidebar.filtering());
        assert_eq!(session_ids(&sidebar.rows()), vec![s.id]);
    }
```

Run: `cargo test -p mkrew-ui --lib sidebar -j 4`
Expected: FAIL to compile (`cannot find type Chip`, `no method toggle_all`, `no field summary`, …).

- [ ] **Step 4: `sidebar.rs` implementation**

Imports at the top become:

```rust
use std::collections::{HashMap, HashSet};

use mkrew_protocol::{Event, Project, Session, SessionState, StateSource};
use ulid::Ulid;

use crate::history;
use crate::inbox::{self, Viewed};
use crate::status;
use crate::text_box::Input;
use crate::theme::Theme;
```

After `status_display`/`agent_badge` helpers (before `pub enum Row`), add:

```rust
/// The chips under the filter box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Chip {
    #[default]
    All,
    Working,
    /// [`inbox::needs_you`]: needs input, or done and not yet viewed.
    NeedsYou,
}

impl Chip {
    fn admits(self, session: &Session, viewed: &Viewed) -> bool {
        match self {
            Chip::All => true,
            Chip::Working => session.state == SessionState::Working,
            Chip::NeedsYou => inbox::needs_you(session, viewed),
        }
    }
}

/// The chip numbers: live sessions matching the filter text (the chip itself does not shrink them).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub all: usize,
    pub working: usize,
    pub needs_you: usize,
}

/// What the sidebar draws.
#[derive(Debug)]
pub struct View {
    pub counts: Counts,
    pub rows: Vec<Row>,
}

/// Whether the (lowercase) `query` matches a session: every whitespace-separated word must be
/// found in one of `fields` (workspace name, agent, title), ignoring case.
// ponytail: lowercases the three fields per session per frame while a query is typed; cache
// the lowercase strings on `Sidebar::load` if a few hundred sessions ever make that visible.
pub fn matches(query: &str, fields: [&str; 3]) -> bool {
    let fields = fields.map(str::to_lowercase);
    query
        .split_whitespace()
        .all(|word| fields.iter().any(|field| field.contains(word)))
}

/// The workspace row's text: `1 working`, `2 idle`, `1 working · 1 needs you · 2 idle`; `None`
/// when none of the three exists (done, starting and unknown sessions are not named here).
pub fn workspace_summary(working: usize, asking: usize, idle: usize) -> Option<String> {
    let parts: Vec<String> = [(working, "working"), (asking, "needs you"), (idle, "idle")]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, what)| format!("{count} {what}"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}
```

`WorkspaceRow`: replace the `working`/`needs_input` doc lines and add the summary:

```rust
    /// Sessions in `working`.
    pub working: usize,
    /// Any session needs input (shown as `?`).
    pub needs_input: bool,
    /// `1 working` / `2 idle` …, `None` without sessions in those states.
    pub summary: Option<String>,
```

`SessionRow`: replace `status_text` with:

```rust
impl SessionRow {
    /// The only text on the row's right: the elapsed time of a working session or the idle
    /// time of an idle one (`17m`). The status icon already says the state; the hover says it
    /// in words.
    pub fn time_text(&self, now_ms: i64) -> Option<String> {
        status::time(self.state, self.state_changed_at, now_ms)
    }
}
```

`Sidebar` struct, after `expanded`:

```rust
    /// The filter box text (client-side, SPEC §10); in memory only.
    filter: Input,
    chip: Chip,
```

New methods in `impl Sidebar` (next to `toggle`):

```rust
    pub fn filter(&self) -> &Input {
        &self.filter
    }

    pub fn filter_mut(&mut self) -> &mut Input {
        &mut self.filter
    }

    pub fn chip(&self) -> Chip {
        self.chip
    }

    pub fn set_chip(&mut self, chip: Chip) {
        self.chip = chip;
    }

    /// A query or a chip other than `All` narrows the list.
    pub fn filtering(&self) -> bool {
        !self.filter.trim().is_empty() || self.chip != Chip::All
    }

    pub fn any_expanded(&self) -> bool {
        self.projects.iter().any(|p| self.expanded.contains(&p.id))
    }

    /// The header button: something open → close everything, else open everything.
    pub fn toggle_all(&mut self) {
        if self.any_expanded() {
            self.expanded.clear();
        } else {
            self.expanded = self.projects.iter().map(|p| p.id).collect();
        }
    }

    /// Makes `project_id`'s session rows visible: opened, and no query or chip hides them
    /// (the keyboard peek hangs on a row).
    pub fn reveal(&mut self, project_id: Ulid) {
        self.expanded.insert(project_id);
        self.filter.clear();
        self.chip = Chip::All;
    }
```

Replace `rows()` with `view()` + a thin `rows()` (delete the old body):

```rust
    /// Rows to show and the chip numbers, in one pass over the sessions (O(sessions) plus the
    /// newest-first sort inside each workspace). Each workspace, then (if open, or while a filter
    /// is active) its live sessions that pass the filter and the chip. While filtering,
    /// workspaces without such a session are left out.
    pub fn view(&self, viewed: &Viewed) -> View {
        let filtering = self.filtering();
        let mut counts = Counts::default();
        let mut rows = Vec::new();
        for (project, sessions) in self.grouped() {
            for session in &sessions {
                counts.all += 1;
                counts.working += usize::from(session.state == SessionState::Working);
                counts.needs_you += usize::from(inbox::needs_you(session, viewed));
            }
            let shown: Vec<&Session> = sessions
                .iter()
                .copied()
                .filter(|session| self.chip.admits(session, viewed))
                .collect();
            if filtering && shown.is_empty() {
                continue;
            }
            let expanded = filtering || self.expanded.contains(&project.id);
            let count = |state| sessions.iter().filter(|s| s.state == state).count();
            let (working, asking, idle) = (
                count(SessionState::Working),
                count(SessionState::NeedsInput),
                count(SessionState::Idle),
            );
            rows.push(Row::Workspace(WorkspaceRow {
                project_id: project.id,
                name: project.name.clone(),
                path: project.path.clone(),
                expanded,
                working,
                needs_input: asking > 0,
                summary: workspace_summary(working, asking, idle),
                color: project.color.clone(),
            }));
            if !expanded {
                continue;
            }
            if shown.is_empty() {
                rows.push(Row::NoSessions {
                    project_id: project.id,
                });
            }
            rows.extend(shown.into_iter().map(|session| {
                Row::Session(SessionRow {
                    session_id: session.id,
                    project_id: session.project_id,
                    agent: session.agent_profile.clone(),
                    title: history::title(session).map(str::to_owned),
                    state: session.state,
                    state_source: session.state_source,
                    state_changed_at: session.state_changed_at,
                })
            }));
        }
        View { counts, rows }
    }

    /// [`Sidebar::view`]'s rows with no `done` session counted as viewed: what the tests and the
    /// pane-lifecycle check read (only the `Needs you` chip looks at `viewed`).
    pub fn rows(&self) -> Vec<Row> {
        self.view(&Viewed::default()).rows
    }

    /// Per workspace (in list order): its live sessions matching the filter text, newest first.
    /// An exited session cannot be opened: no row (History and the Inbox read it from
    /// `sessions()`; ADR 0033).
    fn grouped(&self) -> Vec<(&Project, Vec<&Session>)> {
        let query = self.filter.trim().to_lowercase();
        let mut by_project: HashMap<Ulid, Vec<&Session>> = HashMap::new();
        for session in self
            .sessions
            .iter()
            .filter(|session| session.state != SessionState::Exited)
        {
            by_project.entry(session.project_id).or_default().push(session);
        }
        self.projects
            .iter()
            .map(|project| {
                let mut sessions = by_project.remove(&project.id).unwrap_or_default();
                if !query.is_empty() {
                    sessions.retain(|session| {
                        let fields = [
                            project.name.as_str(),
                            session.agent_profile.as_str(),
                            history::title(session).unwrap_or(""),
                        ];
                        matches(&query, fields)
                    });
                }
                sessions.sort_by_key(|session| std::cmp::Reverse((session.started_at, session.id)));
                (project, sessions)
            })
            .collect()
    }
```

Existing test `upsert…` at the literal `Row::Workspace(WorkspaceRow { project_id: api.id, … })` (the test that compares `sidebar.rows()` to a `vec![…]`, around "Row::Workspace(WorkspaceRow {" in `mod tests`): add `summary: None,` to that literal (the project has no sessions there). Any other `WorkspaceRow {` literal gets the same field (grep `WorkspaceRow {` in the crate).

Run: `cargo test -p mkrew-ui --lib sidebar -j 4` and `cargo test -p mkrew-ui --test lifecycle -j 4`
Expected: PASS (the old `first_load_expands_workspaces_with_live_sessions_newest_first` and the other row tests still pass: `rows()` gives the same rows with no filter).

- [ ] **Step 5: `peek.rs`: remove `Place`**

peek.rs: update the module doc (first lines) so it says `a popover next to a sidebar session row` and that the Peek shortcut opens the first needs-you session. Delete the `Place` enum. Then:

```rust
pub struct Peek {
    pub session_id: Ulid,
    pub screen: Screen,
    …
}

impl Peek {
    pub fn new(session_id: Ulid, focused: bool) -> Self {
        Self {
            session_id,
            screen: Screen::Reading,
            …
        }
    }
```

(remove the `place` field and its line in `new`). `Hover`:

```rust
/// The row under the mouse, and the serial of its pending hover delay.
#[derive(Debug, Default)]
pub struct Hover {
    row: Option<Ulid>,
    serial: u64,
}

impl Hover {
    /// The mouse entered a row: returns the serial to pass to [`Hover::fire`] after the delay.
    pub fn enter(&mut self, session_id: Ulid) -> u64 {
        self.serial += 1;
        self.row = Some(session_id);
        self.serial
    }

    /// The mouse left a row (ignored if it is already over another one).
    pub fn leave(&mut self, session_id: Ulid) {
        if self.row == Some(session_id) {
            self.row = None;
        }
    }

    /// The delay of hover `serial` ran out: the row to peek, if the mouse stayed on it.
    pub fn fire(&self, serial: u64) -> Option<Ulid> {
        self.row.filter(|_| serial == self.serial)
    }

    pub fn is_over(&self, session_id: Ulid) -> bool {
        self.row == Some(session_id)
    }
}
```

Tests in peek.rs `mod tests`: `Peek::new(session, Place::Inbox, true)` → `Peek::new(session, true)`; `Peek::new(Ulid::new(), Place::Inbox, true)` → `Peek::new(Ulid::new(), true)`; `Peek::new(session, Place::Sidebar, false)` → `Peek::new(session, false)`; `Peek::new(Ulid::new(), Place::Inbox, false)` → `Peek::new(Ulid::new(), false)`. Replace `hover_fires_only_while_the_mouse_stays_on_the_row`:

```rust
    #[test]
    fn hover_fires_only_while_the_mouse_stays_on_the_row() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut hover = Hover::default();
        let first = hover.enter(a);
        assert_eq!(hover.fire(first), Some(a));
        assert!(hover.is_over(a));
        assert!(!hover.is_over(b));

        // Moving to another row: the old delay is void; a late leave of the old row is ignored.
        let second = hover.enter(b);
        hover.leave(a);
        assert_eq!(hover.fire(first), None);
        assert_eq!(hover.fire(second), Some(b));

        hover.leave(b);
        assert_eq!(hover.fire(second), None);
        assert!(!hover.is_over(b));
    }
```

Keep `cycling_wraps_through_the_inbox` as it is (`peek::cycle` still walks `inbox_entries()`).

Run: `cargo test -p mkrew-ui --lib peek -j 4`
Expected: PASS.

- [ ] **Step 6: `usage.rs` and `theme.rs` clean-up**

usage.rs: delete `Usages::workspace_total` (the whole `pub fn workspace_total` and its doc comment); grep first: `grep -rn "workspace_total" crates` must show only usage.rs and the `workspace_row` call that Step 7 removes. In the test `a_workspace_counts_each_tasks_newest_session_and_sessions_without_a_task`, delete the four `usages.workspace_total(...)` assertions, the now unused `other`, `loose`, `shell` fixtures only if nothing else reads them, and rename it to `a_task_shows_its_newest_session_with_usage`:

```rust
    #[test]
    fn a_task_shows_its_newest_session_with_usage() {
        let (project, task) = (Ulid::new(), Ulid::new());
        let first_run = session(project, Some(task), 1);
        let resumed = session(project, Some(task), 2);
        let sessions = [resumed.clone(), first_run.clone()];
        let mut usages = Usages::default();
        usages.load(vec![usage(first_run.id, 1_000, 100)]);
        // The resumed run has no turn end yet: its task still shows the first run's.
        assert_eq!(usages.task_total(task, &sessions), Some(1_100));
        usages.load(vec![usage(resumed.id, 3_000, 300)]);
        // The resumed run already counts the first one: 3_300, not 4_400.
        assert_eq!(usages.task_total(task, &sessions), Some(3_300));
    }
```

(`HashMap` stays imported in usage.rs if other code uses it; if `cargo check` warns it is unused, drop it.) SPEC §20E.3 is fixed in Step 9.

theme.rs: delete the `INBOX_TILE_SIZE` const and its doc line.

- [ ] **Step 7: `main.rs`: state, filter keys**

Imports: `use mkrew_ui::inbox::{self, Entry, Jump, Viewed};` (drop `LastLines`), `use mkrew_ui::peek::{self, Peek};` (drop `Place`), `use mkrew_ui::sidebar::{self, Chip, Change, Row, SessionRow, Sidebar, WorkspaceRow};`.

`struct Shell`: delete the `last_lines: LastLines,` field with its doc comment and `last_lines: LastLines::default(),` in `Shell::new`. Next to `peek_focus` add:

```rust
    /// Focused while the sidebar's filter box has the keyboard.
    filter_focus: FocusHandle,
```

and `filter_focus: cx.focus_handle(),` in `Shell::new` (beside `peek_focus: cx.focus_handle(),`).

Delete `fn read_last_lines` and both calls (`shell.read_last_lines(window, cx);` in `refresh`, and the `if inbox_changed { self.read_last_lines(window, cx); }` block in `dispatch`). In `dispatch`, remove `let mut inbox_changed = false;` and the `inbox_changed = true;` line inside `if let Event::SessionState(state) = event` (keep `peek_changed |= …`).

Add next to `rename_key_down`:

```rust
    /// Keys typed in the sidebar's filter box edit it; Esc clears it and gives the keyboard
    /// back to the pane, Enter just gives it back.
    fn filter_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        match key {
            FindKey::Close => {
                self.sidebar.filter_mut().clear();
                self.focus_pane(window, cx);
            }
            FindKey::Older | FindKey::Newer => self.focus_pane(window, cx),
            FindKey::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    self.sidebar.filter_mut().insert(&text.replace(['\r', '\n'], " "));
                }
            }
            FindKey::Insert(text) => {
                self.sidebar.filter_mut().insert(&text);
            }
            // Backspace, Ctrl+Backspace, Ctrl+A.
            key => {
                key.edit(self.sidebar.filter_mut());
            }
        }
        cx.notify();
    }
```

Peek functions (find each with grep):

`Command::Peek` arm of `Shell::run`:

```rust
            Command::Peek => {
                if self.peek.as_ref().is_some_and(|peek| peek.focused) {
                    self.close_peek(window, cx);
                } else if let Some(first) = self.inbox_entries().first() {
                    self.open_peek(first.session_id, true, window, cx);
                }
            }
```

`open_peek`: drop the `place` parameter and reveal the row for a keyboard peek:

```rust
    fn open_peek(
        &mut self,
        session_id: Ulid,
        focused: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !self.peekable(session_id) {
            return;
        }
        // A keyboard peek hangs on the session's row: open its workspace, drop what hides it.
        if focused && let Some(project_id) = self.project_of(session_id) {
            self.sidebar.reveal(project_id);
        }
        let had_keyboard = self.peek.as_ref().is_some_and(|peek| peek.focused);
        self.peek = Some(Peek::new(session_id, focused || had_keyboard));
        if focused && !had_keyboard {
            self.take_peek_focus(window, cx);
        }
        self.read_peek(window, cx);
        cx.notify();
    }
```

`update_peek` (its doc loses the "follows a session that left the Inbox" sentence):

```rust
    /// After sessions changed: the peek closes once its session ended and reads the screen
    /// again when its session's state `changed`.
    fn update_peek(&mut self, changed: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(peek) = self.peek.as_ref() else {
            return;
        };
        if !self.peekable(peek.session_id) {
            return self.close_peek(window, cx);
        }
        if changed {
            self.read_peek(window, cx);
        }
    }
```

`hover_row`:

```rust
    fn hover_row(
        &mut self,
        session_id: Ulid,
        hovered: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if !hovered {
            self.peek_hover.leave(session_id);
            self.close_unused_peek(window, cx);
            return;
        }
        let serial = self.peek_hover.enter(session_id);
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor()
                .timer(theme::PEEK_HOVER_DELAY)
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(session_id) = shell.peek_hover.fire(serial) else {
                    return;
                };
                let shown = shell
                    .peek
                    .as_ref()
                    .is_some_and(|peek| peek.session_id == session_id);
                if !shown && shell.peek.as_ref().is_none_or(Peek::yields_to_hover) {
                    shell.open_peek(session_id, false, window, cx);
                }
            });
        })
        .detach();
    }
```

`close_unused_peek`: `if !peek.stays(shell.peek_hover.is_over(peek.session_id)) {`.

`peek_key_down`: delete the `// A sidebar peek has no Inbox place …` arm and make the cycle arm:

```rust
            peek::Outcome::Cycle(by) => {
                let current = peek.session_id;
                if let Some(next) = peek::cycle(&self.inbox_entries(), current, by)
                    && next != current
                {
                    self.open_peek(next, true, window, cx);
                }
            }
```

The doc comment of `peek_key_down` says "↑ / ↓ peek the other Inbox entries": reword to `↑ / ↓ peek the other sessions that need you`. `with_peek`: remove the `place: Place` parameter and the `place` in its filter (`.filter(|peek| peek.session_id == session_id)`). `render_peek`: the hint becomes

```rust
        } else {
            "Enter sends · ↑ ↓ other sessions that need you · Esc closes".to_owned()
        };
```

replacing the `peek.place == Place::Inbox` / sidebar branches; the not-focused hint (`"Click the field to reply · {} peeks the Inbox"`) becomes `"Click the field to reply · {} peeks the first session that needs you"`. Delete `fn render_inbox` and `fn inbox_row` (both in main.rs, with their doc comments).

`workspace_row`: new signature and body parts (the rest of the function is unchanged):

```rust
fn workspace_row(
    row: WorkspaceRow,
    active: bool,
    run_tip: Option<String>,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
```

Replace the `.when_some(tokens, …)` block and the `.when(row.working > 0, …)` pill block with one summary block (the `.when(row.needs_input, …)` `?` block stays right after it):

```rust
        .when_some(row.summary.clone(), |el, summary| {
            el.child(
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(summary),
            )
        })
```

(`now` is still used by nothing in this function after the pill is gone: if `cargo check` warns `unused variable: now`, remove the parameter and its argument at the call site; `status_mark` is still used by `session_row`.)

`session_row` (full replacement; `now` is still needed for the working dots):

```rust
fn session_row(
    row: SessionRow,
    badge: String,
    selected: bool,
    now: i64,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let session_id = row.session_id;
    let time = row.time_text(now);
    // Only the selected row spells the title out on a second line (SPEC §11); the others keep
    // `agent · title` on one line. Task 18 hangs the plan progress bar under this column.
    let (name, subtitle) = match (&row.title, selected) {
        (Some(title), true) => (row.agent.clone(), Some(title.clone())),
        (Some(title), false) => (format!("{} · {title}", row.agent), None),
        (None, _) => (row.agent.clone(), None),
    };
    let hover = theme.hover;
    let row_id = SharedString::from(format!("session-{session_id}"));
    div()
        .id(row_id.clone())
        .group(row_id.clone())
        .min_h(theme::rems_px(theme::SESSION_ROW_HEIGHT))
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .when(subtitle.is_some(), |el| el.py(px(space::XS)))
        .rounded(px(radius::SMALL))
        .when(selected, |el| el.bg(rgb(theme.selected)))
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_session(session_id, window, cx);
        }))
        .on_hover(cx.listener(move |shell, hovered: &bool, window, cx| {
            shell.hover_row(session_id, *hovered, window, cx);
        }))
        .child(
            // The icon says the state; its hover says it in words and when it changed.
            div()
                .id(SharedString::from(format!("session-status-{session_id}")))
                .flex_none()
                .tooltip(status_tooltip(
                    theme,
                    row.state,
                    row.state_source,
                    row.state_changed_at,
                ))
                .child(status_mark(row.state, now, theme)),
        )
        .child(agent_chip(badge, theme))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .truncate()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .when(selected, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                        .child(name),
                )
                .children(subtitle.map(|title| {
                    div()
                        .truncate()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(title)
                })),
        )
        .children(time.map(|time| {
            div()
                .flex_none()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(time)
        }))
        .child(
            row_action(
                SharedString::from(format!("session-remove-{session_id}")),
                "✕",
                theme,
            )
            // Shown on hover (SPEC §14 progressive disclosure).
            .opacity(opacity::HIDDEN)
            .group_hover(row_id.clone(), |style| style.opacity(opacity::FULL))
            .tooltip(tooltip(theme, "Remove from the sidebar", ""))
            .on_click(cx.listener(move |shell, _, window, cx| {
                // Not a click on the row (which would open the session).
                cx.stop_propagation();
                shell.remove_session(session_id, window, cx);
            })),
        )
        .into_any_element()
}
```

`render_sidebar`: new signature (a `window` parameter, to know whether the filter box has the keyboard) and body. Keep the header exactly as Task 3 left it (its height line, its `Workspaces` label child) and replace only its right-hand `.when(connected, |header| { header.child(icon_button("add-workspace", …)) })` block. Replace the whole function from `let shown = …` down to the end with:

```rust
    fn render_sidebar(
        &self,
        now: i64,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<'_, Self>,
    ) -> impl IntoElement {
        let shown = self.focused_session();
        let connected = self.connection.is_some();
        let add_workspace_keys = self.keymap.label(Command::AddWorkspace);
        let view = self.sidebar.view(&self.viewed);
        let any_open = self.sidebar.any_expanded();
        // <the header div exactly as Task 3 left it, with the `.when(connected, …)` block below>
        let header = header.when(connected && !self.sidebar.is_empty(), |header| {
            header.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .child(
                        icon_button(
                            "collapse-workspaces",
                            if any_open { "▾" } else { "▸" },
                            theme,
                        )
                        .tooltip(tooltip(
                            theme,
                            if any_open {
                                "Collapse all workspaces"
                            } else {
                                "Expand all workspaces"
                            },
                            "",
                        ))
                        .on_click(cx.listener(|shell, _, _, cx| {
                            shell.sidebar.toggle_all();
                            cx.notify();
                        })),
                    )
                    .child(
                        icon_button("add-workspace", "+", theme)
                            .tooltip(tooltip(theme, "Add workspace", add_workspace_keys.clone()))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.add_workspace(window, cx);
                            })),
                    ),
            )
        });
```

(Concretely: the existing `let header = div()… .child(<label>)` keeps its label child and loses the old `.when(connected, …)` tail; the code above is that tail. The `add-workspace` button must also show when the sidebar is empty and connected, so write the tail as two `.when`s if needed: `.when(connected && self.sidebar.is_empty(), |h| h.child(<the add-workspace button only>))`. In the empty state the body already offers an "Add workspace" button, so it is fine to show only the `+` there.)

Then the filter and the chips, the errors, and the body:

```rust
        let filter_focused = self.filter_focus.is_focused(window);
        let controls = (connected && !self.sidebar.is_empty()).then(|| {
            let filter = self.sidebar.filter();
            let hover = theme.hover;
            let chip = |id: &'static str, label: &'static str, count: usize, which: Chip| {
                let active = self.sidebar.chip() == which;
                div()
                    .id(id)
                    .flex_none()
                    .px(px(space::S))
                    .rounded_full()
                    .cursor_pointer()
                    .text_size(font_size::META)
                    .when(active, |el| {
                        el.bg(rgb(theme.accent_bg)).text_color(rgb(theme.accent_text))
                    })
                    .when(!active, |el| {
                        el.text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                    })
                    .child(format!("{label} {count}"))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.sidebar.set_chip(which);
                        cx.notify();
                    }))
            };
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(space::S))
                .px(px(space::M))
                .pb(px(space::S))
                .child(
                    // A tracked element takes the keyboard on a click.
                    text_field(
                        "sidebar-filter",
                        filter.as_str(),
                        "Filter sessions…",
                        filter_focused,
                        filter.all_selected(),
                        theme,
                    )
                    .track_focus(&self.filter_focus)
                    .on_key_down(cx.listener(Self::filter_key_down))
                    .when(filter_focused, |el| {
                        el.relative().child(self.typing(
                            &self.filter_focus,
                            Self::filter_key_down,
                            cx,
                        ))
                    }),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(space::XS))
                        .child(chip("chip-all", "All", view.counts.all, Chip::All))
                        .child(chip(
                            "chip-working",
                            "Working",
                            view.counts.working,
                            Chip::Working,
                        ))
                        .child(chip(
                            "chip-needs-you",
                            "Needs you",
                            view.counts.needs_you,
                            Chip::NeedsYou,
                        )),
                )
        });
        let errors = [&self.sidebar_error, &self.keymap_error].map(|error| {
            error.clone().map(|error| {
                div()
                    .px(px(space::M))
                    .pb(px(space::S))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("✕ {error}"))
            })
        });
        let body = if connected && self.sidebar.is_empty() {
            // <the empty state exactly as before>
        } else {
            // A workspace's sessions hang under a faint guide line.
            let guide = |rows: Vec<gpui::AnyElement>| {
                // <as before>
            };
            let mut list = Vec::new();
            let mut sessions = Vec::new();
            for row in view.rows {
                match row {
                    Row::Workspace(row) => {
                        if !sessions.is_empty() {
                            list.push(guide(std::mem::take(&mut sessions)));
                        }
                        let active = self.workspace == Some(row.project_id);
                        let menu = self
                            .workspace_menu
                            .as_ref()
                            .filter(|menu| menu.project_id == row.project_id);
                        let summary = self.run_summary(row.project_id);
                        let run_tip = summary.any_live().then(|| summary.tooltip());
                        let row = workspace_row(row, active, run_tip, now, theme, cx);
                        list.push(match menu {
                            // The menu drops down from below its row.
                            Some(menu) => div()
                                .child(row)
                                .child(self.render_workspace_menu(menu, theme, cx))
                                .into_any_element(),
                            None => row,
                        });
                    }
                    Row::Session(row) => {
                        let (selected, session_id) =
                            (shown == Some(row.session_id), row.session_id);
                        let badge = self.agent_badge(&row.agent);
                        let row = session_row(row, badge, selected, now, theme, cx);
                        sessions.push(self.with_peek(row, session_id, now, theme, cx));
                    }
                    Row::NoSessions { .. } => sessions.push(
                        div()
                            .px(px(space::S))
                            .py(px(space::XS))
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text_muted))
                            .child("No sessions yet.")
                            .into_any_element(),
                    ),
                }
            }
            if !sessions.is_empty() {
                list.push(guide(sessions));
            }
            if list.is_empty() {
                // A query or chip that nothing matches.
                list.push(
                    div()
                        .px(px(space::S))
                        .py(px(space::XS))
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("No sessions match.")
                        .into_any_element(),
                );
            }
            div()
                .flex()
                .flex_col()
                .px(px(space::XS))
                .children(list)
                .into_any_element()
        };
        // The header and the filter stay in view; the workspace list scrolls below them.
        div()
            .flex_none()
            .w(px(theme::SIDEBAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.sidebar))
            .border_r(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .font_family(theme.ui_font_family.clone())
            .child(header)
            .children(controls)
            .child(
                div()
                    .id("sidebar")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_y_scroll()
                    .children(errors.into_iter().flatten())
                    .child(body),
            )
    }
```

(`// <as before>` marks code that does not change: keep it exactly. A "No sessions match." line only shows while filtering: with no filter an empty list means no workspaces, which the empty-state branch already handles.)

Root `impl Render for Shell`: the call becomes `self.render_sidebar(now, &theme, window, cx)` (keep Task 6's `.children((!self.sidebar_hidden).then(|| …))` wrapper around it).

Run: `cargo check -p mkrew-ui -j 4`
Expected: compiles, no warnings. Likely fixes, all mechanical: `Place` or `LastLines` left in a `use`; `now` unused in `workspace_row` (remove the parameter and its argument); `Entry` is still used by `inbox_entries`; if `usage` import goes unused (`usage::format_count` was only the token total there) drop only the unused item from the `use mkrew_ui::usage::{…}` line; if `.min_h(theme::rems_px(..))` does not type-check, use `.min_h(px(theme::SESSION_ROW_HEIGHT))`.
Run: `cargo test -p mkrew-ui --lib -j 4` and `cargo test -p mkrew-ui --test lifecycle -j 4`
Expected: PASS, including all sidebar tests from Step 3.

- [ ] **Step 8: SPEC**

§10 (:675-692). In the mock, replace the sidebar column of the rows below the title row (keep Task 3's top row and the right column as they are): swap the two Inbox card rows (`┌ Inbox ────┐` and `│! 2 waiting│`) for the filter and the chips, placed right under the `Workspaces +` header row; the cells are 14 wide:

```
│ [Filter…]    │
│ All Work Need│
```

Replace the **Sidebar** bullet (:690-692) text after `exited leaves the list and its pane leaves its tab (ADR 0033; History keeps it).` with:

```markdown
  A **filter box** (`Filter sessions…`) matches workspace name, agent and session title as you
  type (every word must match; client-side, not remembered); under it three **chips** `All N`,
  `Working N` and `Needs you N` narrow the list (the numbers count the sessions matching the
  filter text). *Needs you* is what the Inbox listed before: needs_input, plus done and not yet
  viewed (ADR 0036). While a filter or chip is on, only workspaces with a match show, open.
  The header has `+` (add workspace) and a button that collapses or expands every workspace.
  There is no Inbox card: the chips, the status bar and Ctrl+Shift+I replace it; hovering a
  session row still peeks it (§20A).
```

§11, the workspace-row sentence (:775-777), replace `Workspace row: number of working sessions (the working dots in an accent pill) and ` … `The Inbox is a card at the bottom of the sidebar.` with:

```markdown
- Workspace row: a summary of its sessions (`1 working`, `2 idle`, `1 working · 1 needs you ·
  2 idle`) and `?` if any needs_input; no token total (the status bar shows usage of the focused
  agent). Sidebar session rows hang under a faint guide line: status mark, agent chip, name and,
  on the right, only the time of a working or idle session (`17m`; the status icon already says
  the state, its hover says it in words). The selected row has a second line with the
  session/task title (the other rows show `agent · title` on one line).
```

§11 table (:761, :763): `highlighted pane border, listed in Inbox` → `highlighted pane border, counted in "Needs you"`; `listed in Inbox until viewed` → `counted in "Needs you" until viewed`. §11 pane-header paragraph (:772-773): delete `; it also shows in the Inbox row, on board cards and in agent menus` → `; it also shows on board cards and in agent menus`.

§13 (:977): `Peek the first Inbox entry (↑/↓ cycle, Esc close); hovering a row also peeks` → `Peek the first session that needs you (↑/↓ cycle, Esc close); hovering a row also peeks`.

§20A.1 (:1397-1398): `Hovering an Inbox or sidebar session row` → `Hovering a sidebar session row`, `(first Inbox entry; ↑/↓ cycles entries, Esc closes)` → `(first session that needs you; ↑/↓ cycles those sessions, Esc closes; the row is shown first: its workspace opens and the filter clears)`. §20A.3 (:1410-1411): replace `The Inbox row of a `done` session shows `✓ done · 3 files · 4m` (no files → `✓ done · 4m`; durations as everywhere else, ADR 0011); the pane header shows the same after the status.` with `The pane header of a `done` session shows `✓ done · 3 files · 4m` (no files → `✓ done · 4m`; durations as everywhere else, ADR 0011).` §20A.4 (:1412-1413): replace the item with `4. **Last line.** (Removed with the Inbox card, ADR 0036: no list shows it any more; the peek popover shows the last 8 lines.)`.

Acceptance lines: :1260 `the peek popover shows them for an Inbox or sidebar row` → `the peek popover shows them for a sidebar row`; :1264 `Inbox row and pane header show the summary` → `the pane header shows the summary`; delete :1270 `- [ ] Inbox rows show the last non-blank screen line (§20A.4).` (replace with `- [ ] ~~Inbox rows show the last non-blank screen line~~ removed with the Inbox card (ADR 0036).`). §20E.3 (:1592): `task cards and sidebar workspace rows show total tokens` → `task cards show total tokens (the workspace row shows a session summary, ADR 0036)`.

- [ ] **Step 9: ADR**

Create `docs/decisions/0036-sidebar-filter-replaces-inbox-card.md`:

```markdown
# 0036 — Sidebar filter and chips replace the Inbox card

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; inbox item "Workspace sidebar redesign + cleanup")

## Context

- SPEC §10/§11 put an **Inbox card** at the bottom of the sidebar: every needs_input or
  freshly-done session, newest first, with its last screen line (§20A.4) and turn summary
  (§20A.3), `Place::Inbox` in the peek.
- The redesign adds a status bar with an Inbox state (`Inbox clear` / `N need you`, click = next),
  and a filter box with chips `All` / `Working` / `Needs you` at the top of the sidebar. The
  card now shows the same sessions a third time and takes up to half of the sidebar's height.
- The user asked to remove the card (inbox line 61): "the chips + status bar replace it; peek on
  hover stays".

## Options

1. Keep the card next to the chips. Triple display, less room for workspaces.
2. Remove the card; keep the Inbox *logic* (`inbox::entries`, `needs_you`, `next`) for the chip,
   the status bar, Ctrl+Shift+I and the keyboard peek. (Chosen.)
3. Remove the Inbox concept completely. Loses "jump to the next session that needs me".

## Decision

- The Inbox card, its rows, the last-line read per entry (`session.read lines=1`) and the
  `Place::Inbox` peek variant are deleted. `inbox::entries` / `needs_you` / `next` stay: the
  `Needs you` chip, the status bar and `NextInbox` (Ctrl+Shift+I) use them, so "needs you" has
  one definition.
- The peek popover stays on sidebar session rows (hover) and on `Ctrl+Shift+Space`, which peeks
  the first needs-you session and reveals its row (opens the workspace, clears filter and chip).
  ↑ / ↓ still cycle the needs-you sessions.
- The workspace row shows a summary (`1 working`, `2 idle`) instead of the token total; usage
  lives in the status bar for the focused agent. Session rows show only the time on the right.
- The filter and the chip are client-side and in memory only; nothing new on the wire.

## Consequences

- SPEC §10, §11, §13, §20A.1, §20A.3, §20A.4 and §20E.3 and three acceptance lines are updated
  in the same change (the last-line feature is gone; the pane header keeps the turn summary).
- `Usages::workspace_total` is deleted (task cards keep `task_total`).
- Risk: a `done` session the user did not look at is now visible only as a chip count and in the
  status bar until they open it; the sidebar row still shows its ✓.
```

- [ ] **Step 10: HUMAN_CHECKS**

Append to the `### To verify — 2026-10-10 inbox: redesign` section:

```markdown
- [ ] **Sidebar filter and chips:** `Filter sessions…` at the top; type a workspace name, an agent
  name and part of a session title: only matching sessions show, in open workspaces, others vanish;
  two words match across fields; Backspace, Ctrl+Backspace, Ctrl+A and paste work; Esc clears it and
  gives the keyboard back to the terminal; an unmatched text shows "No sessions match."; Vietnamese
  Telex typing works. Chips `All N` / `Working N` / `Needs you N` show the right numbers and narrow
  the list; `Needs you` matches the status bar's "N need you" (needs input + done not yet viewed;
  focusing a done pane counts it down).
- [ ] **No Inbox card:** nothing sits under the workspace list; hovering a session row still opens
  the peek popover; Ctrl+Shift+Space peeks the first session that needs you even when its
  workspace is collapsed or the filter hides it (the sidebar opens it), ↑ / ↓ cycle, Esc closes.
- [ ] **Sidebar header and rows:** `+` adds a workspace; the `▾`/`▸` button collapses or expands
  all (its tooltip flips); the workspace row says `1 working` / `2 idle` (no token total) and
  keeps the amber `?` when something needs input; a session row shows only the time (`17m`) on
  the right (none for needs-input / done), the status icon hover still explains the state; the
  selected row shows the session title on a second line (rows without a title stay one line), the
  row does not jump in height otherwise.
```

- [ ] **Step 11: Verify and tick**

Run the full verification (`& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`, or fmt / `cargo clippy --workspace --all-targets -- -D warnings` / `cargo test --workspace -j 4`). Tick inbox line 61 `[x]` (last task of item 4; Tasks 7 and 8 are done).
Suggested message: `feat(ui): sidebar filter and chips, header buttons, calmer rows; remove the Inbox card (ADR 0036)`

---

### Task 10: Tab strip: type icon, status dot, `+`, split, `⋯` menu

Inbox: line 73 `Tab strip: icon per tab type (agent badge / shell >_), status dot, title; on the right + (new tab), a split button and a ⋯ menu (split right/down, rename, close).` Do not tick yet (Task 12 ticks line 72). Spec: §10 (:681, :707-715).

**Current state (investigated):**
- `crates/mkrew-ui/src/main.rs` `Shell::render_tab_bar` builds the strip: per tab a div with `status_mark(urgent.state, now, theme)` (only when `status::most_urgent(self.sidebar.sessions(), &tab.sessions())` is `Some`), the title and a `row_action` ✕. Click activates; `click_count() == 2` calls `start_rename` (renames the ACTIVE tab). There is no icon and no right-click handler.
- The `+` (`button("new-tab", …)`) sits right after the last tab, before the `div().flex_1()` spacer. After the spacer: `tab_error`, the zoom notice + "Show all panes", `lifecycle::running_warning`, and a text button `close-pane` ("Close pane", `Command::ClosePane`). There is no split button and no menu.
- `tabs::Tab` (`tabs.rs:15`) has `id, title, position, layout` and local `focused`; it has no kind. A tab's panes are sessions (`Tab::sessions()`); the kind comes from the `Session` in `self.sidebar.sessions()` (`kind`, `agent_profile`).
- Existing commands already do what the menu needs: `Command::SplitRight` / `SplitDown` (`Shell::split(axis)` → `lifecycle::split_kind(focused)` → `new_pane`), `Command::RenameTab` (`start_rename`), `Shell::close_tab(tab_id, …)`. Labels: `self.keymap.label(Command::X)`.
- Popover pattern: `render_workspace_menu` (`deferred(anchored()…child(panel))` with `on_mouse_down_out`, rows from `menu_item(id, theme)`). Task 7 added `fn menu_panel(id: impl Into<gpui::ElementId>, theme: &Theme) -> gpui::Stateful<gpui::Div>` (menu width, column, padding, radius, border, `surface_raised`, shadow, UI font, `.occlude()`): use it, do not copy the panel styling. `anchored().position(Point<Pixels>)` exists in gpui-pre-0.3.7 `elements/anchored.rs:47` (window coordinates).
- `ClickEvent::position()` (gpui `interactive.rs:328`) gives the click point; `MouseDownEvent.position` the right-click point.
- Agent chip: `agent_chip(badge, theme)` (`main.rs`, bordered 18 px square, `theme::AGENT_CHIP_SIZE`); badge from `Shell::agent_badge(profile)`.

**Decisions (defaults, see open questions):**
- Icon of a tab = the kind of its **focused pane** (a tab with several panes shows its focused pane's icon, no count): agent → the agent chip, shell → a chip with `>_`, no panes → no icon.
- The status dot stays `status_mark` of the most urgent pane (already SPEC §11).
- The right group is `+` · split `◫` · `⋯`, after the spacer, next to the existing notices and the "Close pane" button (kept: the inbox item does not remove it).
- The `⋯` menu and a right-click on any tab open the same `TabMenu`. A right-click first activates that tab, so Split / Rename (which act on the active tab) hit the tab that was clicked.

**Files:**
- Modify: `crates/mkrew-ui/src/tabs.rs` (`TabKind`, `tab_kind`, import `SessionKind`; test after `attach_diff_lists_new_and_hidden_sessions`)
- Create: `crates/mkrew-ui/src/context_menu.rs` (`TabMenu`; Task 12 adds `PaneMenu`); declare in `crates/mkrew-ui/src/lib.rs` (`pub mod context_menu;` after `pub mod conflicts;`)
- Modify: `crates/mkrew-ui/src/main.rs` (gpui import `Point`; `Shell.tab_menu`; `open_tab_menu`; `render_tab_menu`; `render_tab_bar`; root `render`; disconnect reset)
- Modify: `docs/SPEC.md` §10, `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `menu_panel(id, theme)` (Task 7); `Shell::render_title_bar` is untouched; `theme::TAB_STRIP_HEIGHT` unchanged.
- Produces (Task 14 relies on these):
  ```rust
  // crates/mkrew-ui/src/context_menu.rs
  #[derive(Debug, Clone, Copy, PartialEq)]
  pub struct TabMenu { pub tab_id: Ulid, pub position: Point<Pixels> }
  // crates/mkrew-ui/src/tabs.rs
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum TabKind { Agent(String), Shell, Empty }
  pub fn tab_kind(tab: &Tab, sessions: &[Session]) -> TabKind;
  // main.rs, impl Shell
  fn open_tab_menu(&mut self, tab_id: Ulid, position: Point<Pixels>, window: &mut Window, cx: &mut Context<'_, Self>);
  fn render_tab_menu(&self, menu: &TabMenu, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement;
  // field: tab_menu: Option<TabMenu>
  ```

- [ ] **Step 1: Failing test for the tab icon kind**

In `crates/mkrew-ui/src/tabs.rs` `mod tests`, after `attach_diff_lists_new_and_hidden_sessions`:

```rust
    /// The icon of a tab follows its focused pane: an agent shows its profile, a shell `>_`,
    /// a tab without panes (or with a pane no session is known for) no icon.
    #[test]
    fn a_tab_shows_the_kind_of_its_focused_pane() {
        let (mut tabs, a, b) = two_panes(); // focused on b
        let project = tabs.project_id();
        let mut agent = session(project, SessionState::Idle, 1);
        agent.id = a;
        let mut shell = session(project, SessionState::Idle, 2);
        shell.id = b;
        shell.kind = mkrew_protocol::SessionKind::Shell;
        let sessions = [agent, shell];

        assert_eq!(tab_kind(tabs.active().unwrap(), &sessions), TabKind::Shell);
        tabs.focus(a);
        assert_eq!(
            tab_kind(tabs.active().unwrap(), &sessions),
            TabKind::Agent("fake".into())
        );
        assert_eq!(tab_kind(tabs.active().unwrap(), &[]), TabKind::Empty);

        let mut empty = Tabs::new(project);
        empty.load(vec![wire(project, "new", 1.0, serde_json::Value::Null)]);
        assert_eq!(tab_kind(empty.active().unwrap(), &sessions), TabKind::Empty);
    }
```

Run: `cargo test -p mkrew-ui --lib tabs::tests::a_tab_shows -j 4`
Expected: FAIL to compile (`cannot find type TabKind`, `tab_kind`).

- [ ] **Step 2: Implement `tab_kind`**

`tabs.rs` imports: add `SessionKind` to the `mkrew_protocol::{…}` list. After `rename_params` add:

```rust
/// What a tab's icon shows (SPEC §10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabKind {
    /// An agent pane; the profile id (its badge is looked up by the caller).
    Agent(String),
    Shell,
    Empty,
}

/// The kind of the tab's focused pane (`fix_focus` keeps one focused whenever the tab has
/// panes). `Empty` without panes or when no session is known for the focused one.
pub fn tab_kind(tab: &Tab, sessions: &[Session]) -> TabKind {
    let focused = tab.focused().and_then(|id| sessions.iter().find(|s| s.id == id));
    match focused {
        Some(session) if session.kind == SessionKind::Agent => {
            TabKind::Agent(session.agent_profile.clone())
        }
        Some(_) => TabKind::Shell,
        None => TabKind::Empty,
    }
}
```

Run: `cargo test -p mkrew-ui --lib tabs -j 4`
Expected: PASS.

- [ ] **Step 3: `TabMenu` state**

Create `crates/mkrew-ui/src/context_menu.rs`:

```rust
//! State of the right-click menus of the Terminals view (SPEC §10): the tab menu (also opened by
//! the tab strip's `⋯`) and, from the pane header, the pane menu. `main.rs` renders them.

use gpui::{Pixels, Point};
use ulid::Ulid;

/// The open menu of one tab, at `position` (window coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TabMenu {
    pub tab_id: Ulid,
    pub position: Point<Pixels>,
}
```

`lib.rs`: add `pub mod context_menu;` after `pub mod conflicts;`.

- [ ] **Step 4: Shell state and open/render functions**

`main.rs`:

1. gpui import: add `Point` after `Pixels`. mkrew_ui import: `use mkrew_ui::context_menu::TabMenu;` (alphabetical, after `conflicts`); `use mkrew_ui::tabs::{…, TabKind, tab_kind}` joins the existing `tabs` import.
2. Field next to `workspace_menu`: 
   ```rust
       /// The open tab menu (Split right / down, Rename, Close).
       tab_menu: Option<TabMenu>,
   ```
   Initialise `tab_menu: None,` next to `workspace_menu: None,` in `Shell::new`, and `shell.tab_menu = None;` after `shell.workspace_menu = None;` in the disconnect reset.
3. After `start_rename` add:

```rust
    /// Opens the menu of `tab_id` at `position`: the tab becomes the active one first, because
    /// Split and Rename act on the active tab.
    fn open_tab_menu(
        &mut self,
        tab_id: Ulid,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.activate(tab_id);
        }
        self.tab_menu = Some(TabMenu { tab_id, position });
        self.sync_panes(window, cx);
        cx.notify();
    }
```

4. After `render_workspace_menu` add:

```rust
    /// The tab menu: Split right, Split down, Rename, Close (shortcuts on the right). Floats at
    /// the click; a click outside closes it.
    fn render_tab_menu(
        &self,
        menu: &TabMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let tab_id = menu.tab_id;
        let item = |id: &'static str, label: &'static str, command: Command| {
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
                .child(
                    div()
                        .text_size(font_size::META)
                        .child(self.keymap.label(command)),
                )
        };
        let panel = menu_panel("tab-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.tab_menu = None;
                cx.notify();
            }))
            .child(
                item("tab-menu-split-right", "Split right", Command::SplitRight).on_click(
                    cx.listener(|shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.run(&Run(Command::SplitRight), window, cx);
                    }),
                ),
            )
            .child(
                item("tab-menu-split-down", "Split down", Command::SplitDown).on_click(
                    cx.listener(|shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.run(&Run(Command::SplitDown), window, cx);
                    }),
                ),
            )
            .child(
                item("tab-menu-rename", "Rename", Command::RenameTab).on_click(cx.listener(
                    |shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.start_rename(window, cx);
                    },
                )),
            )
            .child(
                item("tab-menu-close", "Close", Command::CloseTab).on_click(cx.listener(
                    move |shell, _, window, cx| {
                        shell.tab_menu = None;
                        shell.close_tab(tab_id, window, cx);
                    },
                )),
            );
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }
```

5. Root `impl Render for Shell`, with the dialogs' `.children(…)` (end of the chain):

```rust
            .children(
                self.tab_menu
                    .as_ref()
                    .map(|menu| self.render_tab_menu(menu, &theme, cx)),
            )
```

- [ ] **Step 5: Tab strip: icon, right-click, right group**

In `render_tab_bar`:

a. Before the `div()` of one tab (after `let group = …`), compute the icon:

```rust
                let icon = match tab_kind(tab, self.sidebar.sessions()) {
                    TabKind::Agent(profile) => {
                        Some(agent_chip(self.agent_badge(&profile), theme))
                    }
                    TabKind::Shell => Some(agent_chip(">_".to_owned(), theme)),
                    TabKind::Empty => None,
                };
```

b. On the tab div, after the existing `.on_click(…)`, add the right-click handler, and put the icon before the status dot (replace `.when_some(urgent, …)` by itself preceded by `.children(icon)`):

```rust
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                            shell.open_tab_menu(tab_id, event.position, window, cx);
                        }),
                    )
                    .children(icon)
                    .when_some(urgent, |el, session| {
```

c. Remove the `.child(button("new-tab", "+", …))` that follows the tab list (it moves to the right group). Keep `.child(div().flex_1())`, the notices and the `close-pane` block. After the `close-pane` `.when(self.focused_session().is_some(), …)` append the right group:

```rust
            .child(
                button("new-tab", "+", theme, false)
                    .tooltip(tooltip(theme, "New tab", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.create_tab(project_id, None, window, cx);
                    })),
            )
            .child(
                button("tab-split", "◫", theme, false)
                    .tooltip(tooltip(
                        theme,
                        "Split right",
                        self.keymap.label(Command::SplitRight),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.run(&Run(Command::SplitRight), window, cx);
                    })),
            )
            .when_some(active, |bar, tab_id| {
                bar.child(
                    button("tab-menu-button", "⋯", theme, false)
                        .tooltip(tooltip(theme, "Tab menu", ""))
                        .on_click(cx.listener(
                            move |shell, event: &gpui::ClickEvent, window, cx| {
                                shell.open_tab_menu(tab_id, event.position(), window, cx);
                            },
                        )),
                )
            })
```

Update the doc comment above `render_tab_bar`'s notices comment `// Pushes the notices and pane actions to the right.` to `// Pushes the notices, Close pane and the + / split / ⋯ group to the right.`

Run: `cargo check -p mkrew-ui` (the C: drive is nearly full: no clean build) 
Expected: compiles; `cargo test -p mkrew-ui --lib tabs -j 4` PASS.

- [ ] **Step 6: SPEC and HUMAN_CHECKS**

SPEC §10, Terminals bullet: replace `Each tab has a ✕ (always on the active tab, on hover for the others); closing never activates the tab first.` with:

```markdown
  Each tab shows an icon of its focused pane (the agent's badge chip, or `>_` for a shell; none
  for a tab without panes), the status mark of its most urgent pane (§11), its title and a ✕
  (always on the active tab, on hover for the others); closing never activates the tab first.
  At the right end of the strip: `+` (new tab), a split button (Split right) and `⋯`. `⋯`, and a
  right-click on a tab, open the tab menu: Split right, Split down, Rename, Close; the tab is
  activated first, so the menu acts on the tab it was opened on.
```

In the §10 layout diagram, in the row containing `tab strip (Terminals only)`, replace `[● tab ✕] [tab ✕] +` by `[▣ ● tab ✕] [>_ tab ✕]   + ◫ ⋯` (keep the box width: trim the spaces after it).

HUMAN_CHECKS: Task 1 created the section `### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)`; append there:

```markdown
- [ ] **Tab strip (item 5):** each tab shows an icon before its status mark: the agent's badge chip
  for an agent pane, `>_` for a shell, nothing for a tab with no panes; a tab with two panes shows
  the icon of the pane that has focus and changes when you focus the other. The status mark still
  shows the most urgent pane. `+`, `◫` and `⋯` sit at the right end (check the `◫` glyph is not a
  box). `+` makes a tab; `◫` splits the focused pane to the right (same as Alt+Shift+=).
- [ ] **Tab menu:** `⋯` and a right-click on any tab (also an inactive one) open Split right / Split
  down / Rename / Close with their shortcuts; the clicked tab becomes active; Rename edits that
  tab's title inline; Close closes that tab; a click outside closes the menu.
```

- [ ] **Step 7: Verify**

Run the full verification (`& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`). Do not tick the inbox yet (Task 12 ticks line 72).
Suggested message: `feat(ui): tab strip with type icon, split button and tab menu`

---

### Task 11: Pane header: breadcrumb and one status pill

Inbox: line 74 `Pane header: breadcrumb <workspace> › <folder> › <agent> + one status pill Working · 1h 25m on the left. Remove the status/time text from the pane's top-right bar … Follow in Zed and Turns stay on the right.` Do not tick yet. Spec: §11 (:767-773), §10.

**Current state (investigated):**
- `main.rs` `pane_header(session, badge, turn, title, context, conflict, turns, queued, now, theme)` draws, left to right: `status_mark(state, now, theme)`, `agent_chip(badge)`, the agent name (`session.agent_profile`, medium), `title` (flex_1: inline rename field, or `run::pane_title(info, ports)`), the conflict badge, `⧗ N queued`, the context share, then `div#pane-status-{id}` with `turn::status_text(state, exit_code, changed_at, now, turn)` and `status_tooltip(...)` (the top-right status/time text), then `turns` (Follow in Zed + Turns…).
- Status therefore shows twice (the mark at the left, the text at the right). `status::text` → `working · 2m` (lowercase label from `sidebar::status_display(state).label` + `status::time`); `turn::status_text` appends `· 3 files · 4m` while `done` (SPEC §20A.3). Durations: `status::format_duration` gives `1h 25m` for 5100 s.
- `Session` has no folder. The daemon runs a plain pane in the project folder; only a task with a worktree runs elsewhere: `Task.worktree_path` is `<data>/worktrees/<project>/<task_id>` (basename is a ULID, useless) and `Task.branch` is `mkrew/<slug>-<8 chars>` (`daemon/src/worktree.rs:30`). The UI resolves a session's task with `self.board.task(task_id)` (`Board::task`, `board.rs:307`).
- Workspace name: `self.sidebar.projects()` (`Project.name`). Exited panes close (ADR 0033), so the `exited (N)` text is unreachable in a pane but stays correct.
- `pane_frame` (`main.rs`) builds the `turns` div, the title element and calls `pane_header(...)`.

**Decisions (defaults, see open questions):**
- Folder = the worktree of the session's task, shown as its branch without the `mkrew/` prefix; omitted for every other pane (a plain pane's folder is the workspace itself, so repeating it would mislead). No new stored data.
- One pill on the left: the status mark (dots / glyph, as now) + `Working · 1h 25m`. It keeps the hover tooltip and the done summary (`Done · 3 files · 4m`). The top-right text is removed.
- The agent chip stays before the breadcrumb; the last breadcrumb segment is the agent profile id (what the header showed). The pane title (user title / terminal title / run name) stays after the pill, as flex-fill.

**Files:**
- Create: `crates/mkrew-ui/src/pane_text.rs` (pure helpers + tests); declare `pub mod pane_text;` in `lib.rs` after `pub mod palette;`
- Modify: `crates/mkrew-ui/src/main.rs` (`pane_frame`, `pane_header`)
- Modify: `docs/SPEC.md` §11, §10 ; `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `turn::status_text`, `status::format_duration`, `Board::task`.
- Produces (Task 12 extends `pane_header` further):
  ```rust
  // crates/mkrew-ui/src/pane_text.rs
  pub fn breadcrumb(workspace: Option<&str>, folder: Option<&str>, agent: &str) -> String;
  pub fn folder(task: Option<&Task>) -> Option<String>;
  pub fn pill_text(state: SessionState, exit_code: Option<i32>, changed_at: i64, now_ms: i64, turn: Option<&SessionTurnEvent>) -> String;
  // main.rs
  fn pane_header(session: &Session, badge: String, breadcrumb: String, pill: String,
                 title: gpui::AnyElement, context: Option<ContextShare>,
                 conflict: Option<gpui::Stateful<gpui::Div>>, turns: Option<gpui::Stateful<gpui::Div>>,
                 queued: usize, now: i64, theme: &Theme) -> gpui::Div;
  ```

- [ ] **Step 1: Failing tests**

Create `crates/mkrew-ui/src/pane_text.rs` with the tests only (`mod tests` at the end; no functions yet) and add `pub mod pane_text;` to `lib.rs`:

```rust
//! Text of a pane header (SPEC §11): the `<workspace> › <folder> › <agent>` breadcrumb and the
//! status pill. Pure and tested headlessly; `main.rs` draws it.

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionState, SessionTurnEvent, Task, TaskStatus, TurnFile};
    use ulid::Ulid;

    use super::*;

    const SEC: i64 = 1000;

    fn task(worktree: bool, branch: Option<&str>) -> Task {
        Task {
            id: Ulid::new(),
            project_id: Ulid::new(),
            title: "Fix login".into(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Running,
            position: 0.0,
            checkpoint: None,
            error_flag: false,
            worktree,
            worktree_path: worktree.then(|| "/data/worktrees/p/t".to_owned()),
            branch: branch.map(str::to_owned),
            fanout_of: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn the_breadcrumb_skips_what_is_unknown() {
        assert_eq!(breadcrumb(Some("kit"), None, "omp"), "kit › omp");
        assert_eq!(
            breadcrumb(Some("kit"), Some("fix-login-ab12cd34"), "claude"),
            "kit › fix-login-ab12cd34 › claude"
        );
        // A workspace that is not loaded yet leaves the agent alone.
        assert_eq!(breadcrumb(None, None, "claude"), "claude");
        assert_eq!(breadcrumb(Some(""), Some(""), "claude"), "claude");
    }

    #[test]
    fn only_a_worktree_task_names_a_folder() {
        let branch = Some("mkrew/fix-login-ab12cd34");
        assert_eq!(
            folder(Some(&task(true, branch))).as_deref(),
            Some("fix-login-ab12cd34")
        );
        // The worktree is gone (or never made): the pane runs in the workspace folder.
        assert_eq!(folder(Some(&task(false, branch))), None);
        assert_eq!(folder(Some(&task(true, None))), None);
        assert_eq!(folder(None), None);
        // Not one of ours: shown whole.
        assert_eq!(
            folder(Some(&task(true, Some("feature/x")))).as_deref(),
            Some("feature/x")
        );
    }

    #[test]
    fn the_pill_names_the_state_and_how_long_it_lasts() {
        let now = 10_000 * SEC;
        let since = now - (85 * 60 * SEC);
        assert_eq!(
            pill_text(SessionState::Working, None, since, now, None),
            "Working · 1h 25m"
        );
        assert_eq!(
            pill_text(SessionState::NeedsInput, None, since, now, None),
            "Needs input"
        );
        assert_eq!(
            pill_text(SessionState::Exited, Some(1), since, now, None),
            "Exited (1)"
        );
        // A finished turn keeps its summary (SPEC §20A.3).
        let turn = SessionTurnEvent {
            session_id: Ulid::new(),
            n: 1,
            started_at: 0,
            ended_at: 4 * 60 * SEC,
            files: vec![TurnFile {
                path: "a.rs".into(),
                added: None,
                removed: None,
            }],
        };
        assert_eq!(
            pill_text(SessionState::Done, None, since, now, Some(&turn)),
            "Done · 1 file · 4m"
        );
    }
}
```

Run: `cargo test -p mkrew-ui --lib pane_text -j 4`
Expected: FAIL to compile (`breadcrumb`, `folder`, `pill_text` not found).

- [ ] **Step 2: Implement the helpers**

At the top of `pane_text.rs`, above `#[cfg(test)]`:

```rust
use mkrew_protocol::{SessionState, SessionTurnEvent, Task};

use crate::turn;

/// The `›`-joined path shown in a pane header; unknown or empty parts are left out.
pub fn breadcrumb(workspace: Option<&str>, folder: Option<&str>, agent: &str) -> String {
    [workspace, folder, Some(agent)]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" › ")
}

/// The folder a pane works in when it is not the workspace folder: the worktree of its task
/// (SPEC §20E.1), named by its branch without the `mkrew/` prefix. `None` for every other pane.
pub fn folder(task: Option<&Task>) -> Option<String> {
    let task = task.filter(|task| task.worktree_path.is_some())?;
    let branch = task.branch.as_deref()?;
    Some(branch.strip_prefix("mkrew/").unwrap_or(branch).to_owned())
}

/// The status pill: [`turn::status_text`] (`working · 1h 25m`, `done · 3 files · 4m`) with a
/// capital first letter.
pub fn pill_text(
    state: SessionState,
    exit_code: Option<i32>,
    changed_at: i64,
    now_ms: i64,
    turn: Option<&SessionTurnEvent>,
) -> String {
    let text = turn::status_text(state, exit_code, changed_at, now_ms, turn);
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}
```

Run: `cargo test -p mkrew-ui --lib pane_text -j 4`
Expected: PASS.

- [ ] **Step 3: Draw the header**

`main.rs`, `pane_frame`, inside the `.children(info.map(|info| { … }))` block, just before `pane_header(`:

```rust
                let workspace = (self.sidebar.projects().iter())
                    .find(|project| project.id == info.project_id)
                    .map(|project| project.name.as_str());
                let folder = pane_text::folder(info.task_id.and_then(|id| self.board.task(id)));
                let breadcrumb =
                    pane_text::breadcrumb(workspace, folder.as_deref(), &info.agent_profile);
                let pill = pane_text::pill_text(
                    info.state,
                    info.exit_code,
                    info.state_changed_at,
                    now,
                    self.turns.get(session),
                );
```

and change the call to `pane_header(info, self.agent_badge(&info.agent_profile), breadcrumb, pill, title, self.usages.get(session).and_then(usage::context_share), conflict, turns, queued, now, theme)` (drop the `self.turns.get(session)` argument). Add `use mkrew_ui::pane_text;` to the mkrew_ui imports (alphabetical, after `palette`).

Replace the doc comment and body of `pane_header`:

```rust
/// A pane's header (SPEC §11): the agent badge, the `<workspace> › <folder> › <agent>`
/// breadcrumb, one status pill (status mark + `Working · 1h 25m`, with the last turn's summary
/// when done; hovering it shows the status source and last change), the title (renamed inline
/// with F2 or a double-click, ADR 0031), an optional conflict warning, the number of queued
/// sends (§20B.2), the context share (`⚠` and amber from 80 %, §20E.3), and on the right an
/// agent's "Follow in Zed" and "Turns…" (§20C.5, §20C.2).
#[allow(clippy::too_many_arguments)]
fn pane_header(
    session: &Session,
    badge: String,
    breadcrumb: String,
    pill: String,
    title: gpui::AnyElement,
    context: Option<ContextShare>,
    conflict: Option<gpui::Stateful<gpui::Div>>,
    turns: Option<gpui::Stateful<gpui::Div>>,
    queued: usize,
    now: i64,
    theme: &Theme,
) -> gpui::Div {
    let (state, changed_at) = (session.state, session.state_changed_at);
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(space::S))
        .px(px(space::S))
        .py(px(space::XS))
        .font_family(theme.ui_font_family.clone())
        .text_size(font_size::META)
        .child(agent_chip(badge, theme))
        .child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text))
                .child(breadcrumb),
        )
        .child(
            div()
                .id(SharedString::from(format!("pane-status-{}", session.id)))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::XS))
                .px(px(space::S))
                .rounded_full()
                .bg(rgb(theme.surface_raised))
                .text_color(rgb(theme.text_muted))
                .tooltip(status_tooltip(
                    theme,
                    state,
                    session.state_source,
                    changed_at,
                ))
                .child(status_mark(state, now, theme))
                .child(pill),
        )
        .child(title)
        .children(conflict.map(|badge| {
            badge
                .flex_none()
                .max_w(relative(theme::PANE_CONFLICT_MAX_FRACTION))
        }))
        .when(queued > 0, |el| {
            el.child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.text_muted))
                    .child(format!("⧗ {queued} queued")),
            )
        })
        .children(context.map(|share| {
            div()
                .id(SharedString::from(format!("pane-context-{}", session.id)))
                .flex_none()
                .text_color(rgb(if share.warning {
                    theme.status_needs_input
                } else {
                    theme.text_muted
                }))
                .tooltip(tooltip(theme, &share.hover, ""))
                .child(if share.warning {
                    format!("⚠ {}", share.label)
                } else {
                    share.label
                })
        }))
        .children(turns)
}
```

(The context-share and conflict blocks are copied unchanged from the current function; the old `status_mark`, agent-name div and top-right `pane-status` div are gone. If `SessionTurnEvent` is no longer used in `main.rs` after this edit, drop it from the `mkrew_protocol` import; `self.turns.get(session)` still returns it.)

Run: `cargo check -p mkrew-ui`
Expected: compiles with no unused-import warning (`-D warnings` in verify).

- [ ] **Step 4: SPEC**

SPEC §11, replace the bullet that starts `- Pane header: status icon, agent badge (§6 \`icon\`), agent name, session/task title …` through `… \`time\`. The` (the sentence ends before `sidebar session row also puts the badge`) with:

```markdown
- Pane header, left to right: the agent badge (§6 `icon`, a bordered square chip), the breadcrumb
  `<workspace> › <folder> › <agent>` (the folder only for a task that runs in a worktree, shown
  as its branch without `mkrew/`; a plain pane runs in the workspace folder, so none), **one
  status pill** (the status mark and `Working · 1h 25m`; a done pane adds the last turn,
  `Done · 3 files · 4m`; hovering it shows the source and last change) and the session/task title
  (F2 or a double-click renames it inline: Enter saves, Esc cancels, blank clears; run panes keep
  their command name — ADR 0031; without a title an agent pane shows the title its agent gave the
  terminal, e.g. Claude's conversation summary; everywhere a session is named the order is: user
  title > terminal title > agent name). The right end keeps `⧗ N queued`, the context share,
  and an agent's "Follow in Zed" and "Turns…"; the status is not repeated there. The
```

and make sure the following words `sidebar session row also puts the badge before the agent name.` still read correctly (the original line began `time. The` + newline + `sidebar session row`).

HUMAN_CHECKS (same section):

```markdown
- [ ] **Pane header (item 5):** `<workspace> › <agent>` after the agent chip (an agent started on a
  worktree task shows `<workspace> › <branch> › <agent>`); one pill `Working · 1m` with the three
  dots, then the pane title; no status/time text at the right, only `⧗ queued`, the context share,
  Follow in Zed and Turns…; the pill turns `Needs input` / `Idle · 3m` / `Done · 2 files · 1m` with the
  state; hovering it shows `working · from hook · 12s ago`. Narrow a pane (2×2 split): the
  breadcrumb is clipped with `…`, the pill and the buttons stay whole.
```

- [ ] **Step 5: Verify**

Run the full verification. Do not tick the inbox yet.
Suggested message: `feat(ui): pane header shows a breadcrumb and one status pill`

---

### Task 12: Pane right-click menu: Rename and Change color (`session.color`, protocol v34)

Inbox: line 76-78 `Right-click menu on panes: "Rename" (inline edit, Enter to save, Esc to cancel); "Change color" (same palette as projects, item 4)`. Tick line 72 at the end (last task of item 5). Spec: §5 (:234-248), §7 (:520, :559), §10 (:697-698), §11, §14 (:1001-1003); ADR 0020, 0027, 0031.

**Current state (investigated):**
- Rename: `Shell::start_pane_rename(session_id, window, cx)` (skips run sessions via `mkrew_core::run::command_name`), `finish_rename`, `rename_key_down` (Enter commits, Esc cancels, blur commits), `rename_session` → `lifecycle::session_rename_params` → `send_rename(Request::SessionRename(params), "pane", shown, …)`. The pane header title already switches to the inline field when `renaming.target == RenameTarget::Session(session)`.
- Daemon `Server::session_rename` (`server.rs:1112`): validates, then a live session goes through `Command::Rename { title, reply }` (`session.rs:248`; handler `:1089` sets `self.session.title`, `save()`, emits `Event::SessionUpdated`) because the actor owns the row and its next `save()` would clobber a store-only edit; an exited session is edited in the store (`update_session`) after checking `state == Exited`. `Store::update_session` writes `title` and the state columns but not a color; `insert_session` / both SELECT lists / `RawSession` list the session columns.
- Resume: `server/resume.rs:114` builds a `Start { title: last.title.clone(), … }`; `start_session` (`server.rs:802`) builds the `Session` literal. Three other `Start` literals: `server.rs:510` (spawn), `:598` (task run), `server/run.rs:130`.
- Palette: Task 8 produced `mkrew_protocol::WORKSPACE_COLORS` (final list) and `Theme::workspace_color(name) -> Option<u32>`. `color_swatch(project_id, color, current, theme, cx)` (`main.rs`) is hard-wired to `set_workspace_color` and the ids `workspace-color-*`; its only caller is `render_workspace_menu`. Tint helper: `Theme::tint(color) -> Rgba` (avatars, ADR 0027). Every color is ≥ 3:1 on `bg` (ADR 0020, test `workspace_colors_stand_out_from_the_surfaces`).
- `Session { … }` literals (every one has `terminal_title: None,`): `mkrew-core/src/run.rs:351`; `mkrew-daemon/src/{awake.rs:389, notify.rs:320, turns.rs:297, server.rs:811, server/resume.rs:228}`, `mkrew-daemon/tests/daemon.rs:681`; `mkrew-protocol/src/message.rs:2272`; `mkrew-store/src/{lib.rs:1453, tests.rs:17}`; `mkrew-ui/src/{board.rs:587, broadcast.rs:255, conflicts.rs:178, doc_dialog.rs:332, fanout.rs:408, git_view.rs:2434, history.rs:221, inbox.rs:219, lifecycle.rs:183, palette.rs:634, run.rs:291, sidebar.rs:372, status.rs:193, tabs.rs:899, turns_dialog.rs:198, undo_dialog.rs:333, usage.rs:192, worktree_dialog.rs:449}`. Earlier tasks of this plan may have added literals: grep `terminal_title: None` and let the compiler list the rest.
- ADR 0020 limited workspace colors to workspaces (and SPEC §14 principle 2 calls the letter avatar "the one exception") → coloring panes needs an ADR and a SPEC §14 edit.

**Decisions (defaults, see open questions):**
- New request `session.set_color { session_id, color? }` (not a field on `session.rename`: rename's `None` means "clear the title", a second optional there would conflate two verbs). Reply = the `Session`, also `session.updated`.
- A live session goes through a new `Command::SetColor` like `Command::Rename`; an exited one through the store. Both share one new helper `Server::edit_session`, which `session_rename` now uses too (no copied tail).
- Run sessions may be colored (only their title is fixed).
- Resume copies the color. The daemon refuses any name outside `WORKSPACE_COLORS`; `null` clears.
- Where it shows: the agent chip in the pane header is tinted and bordered with the color, and a thin accent line (`theme::PANE_ACCENT_WIDTH`, 2 px) runs under the header. The state-driven pane border (`status::pane_border`) is untouched. The sidebar row is not colored (workspace avatars keep the sidebar's one color signal).

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs` (`Session.color`, `SessionSetColorParams`, `Request::SessionSetColor`, tests), `crates/mkrew-protocol/src/lib.rs:16`
- Create: `crates/mkrew-store/migrations/0015_session_color.sql`
- Modify: `crates/mkrew-store/src/lib.rs` (MIGRATIONS, `insert_session`, `update_session`, both SELECTs, `RawSession`), `crates/mkrew-store/src/tests.rs`
- Modify: `crates/mkrew-daemon/src/session.rs` (`Command::SetColor`), `src/server.rs` (dispatch, `edit_session`, `session_set_color`, `Start.color`, literals), `src/server/resume.rs`, `src/server/run.rs`, `tests/daemon.rs`
- Modify: every `Session { … }` literal listed above (`color: None,`)
- Modify: `crates/mkrew-ui/src/{lifecycle.rs, sidebar.rs, context_menu.rs, theme.rs, main.rs}`
- Create: `docs/decisions/0037-pane-colors.md`
- Modify: `docs/SPEC.md` §5, §7, §10, §11, §14; `docs/HUMAN_CHECKS.md`; `docs/INBOX.md` (tick)

**Interfaces:**
- Consumes: `WORKSPACE_COLORS`, `Theme::workspace_color` and `color_swatch` (Task 8); `menu_panel` (Task 7); `agent_chip`; Task 11's `pane_header`; Task 10's `context_menu.rs`.
- Produces (Task 14 relies on these):
  ```rust
  // mkrew-protocol
  pub struct Session { …, pub color: Option<String> /* #[serde(default, skip_serializing_if = "Option::is_none")] */ }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct SessionSetColorParams { pub session_id: Ulid, #[serde(default)] pub color: Option<String> }
  Request::SessionSetColor(SessionSetColorParams) // "session.set_color" → Session
  // mkrew-ui context_menu.rs
  #[derive(Debug, Clone, Copy, PartialEq)]
  pub struct PaneMenu { pub session_id: Ulid, pub position: Point<Pixels> }
  // mkrew-ui lifecycle.rs
  pub fn session_color_params(session: &Session, color: Option<&str>) -> Option<SessionSetColorParams>;
  // mkrew-ui sidebar.rs
  impl Sidebar { pub fn set_color(&mut self, session_id: Ulid, color: Option<String>) }
  // main.rs, impl Shell
  fn open_pane_menu(&mut self, session_id: Ulid, position: Point<Pixels>, cx: &mut Context<'_, Self>);
  fn render_pane_menu(&self, menu: &PaneMenu, theme: &Theme, cx: &mut Context<'_, Self>) -> Option<gpui::AnyElement>;
  fn set_pane_color(&mut self, session_id: Ulid, color: Option<&'static str>, window: &mut Window, cx: &mut Context<'_, Self>);
  // field: pane_menu: Option<PaneMenu>
  ```
  `PROTOCOL_VERSION` goes `33` → `34`; migration `0015`.

- [ ] **Step 1: Failing protocol tests**

`message.rs` `mod tests`, in the list of requests (after the `Request::SessionRename(…)` entry):

```rust
            Request::SessionSetColor(SessionSetColorParams {
                session_id: sid(),
                color: Some("teal".into()),
            }),
```

In `event_wire_shape_matches_spec`, in the `SessionUpdated(Session { … })` literal add `color: Some("teal".into()),` after `terminal_title: None,`, and after `assert_eq!(updated["data"]["title"], "Fix login");` add:

```rust
        assert_eq!(updated["data"]["color"], "teal");
        // An older daemon's session has no color: it reads as none.
        let mut old = updated["data"].clone();
        old.as_object_mut().unwrap().remove("color");
        assert_eq!(serde_json::from_value::<Session>(old).unwrap().color, None);
```

Run: `cargo test -p mkrew-protocol --lib -j 4`
Expected: FAIL to compile (`SessionSetColorParams`, field `color`).

- [ ] **Step 2: Protocol types**

`message.rs`, `Session`, after `terminal_title`:

```rust
    /// One of [`WORKSPACE_COLORS`] (ADR 0037): the pane's color; `None` = no color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
```

`Request`, after `SessionRename`:

```rust
    /// Returns the [`Session`] with the new color, also announced as `session.updated`; `null`
    /// clears it, a name outside [`WORKSPACE_COLORS`] is `invalid_request` (ADR 0037).
    #[serde(rename = "session.set_color")]
    SessionSetColor(SessionSetColorParams),
```

After `SessionRenameParams`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSetColorParams {
    pub session_id: Ulid,
    /// A [`WORKSPACE_COLORS`] name; `None` clears the color.
    #[serde(default)]
    pub color: Option<String>,
}
```

`lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 34;`

Also update the doc of `Event::SessionUpdated`: `/// A session's title or color changed (`session.rename`, `session.set_color`; ADR 0031, ADR 0037); carries the whole session.`

Run: `cargo test -p mkrew-protocol --lib -j 4`
Expected: PASS (the crate's own `Session` literal in the test was updated in Step 1).

- [ ] **Step 3: Failing store test**

`crates/mkrew-store/src/tests.rs`: in `fn session(...)` add `color: None,` after `terminal_title: None,`. After `sessions_insert_update_list` add:

```rust
/// A session's color is stored, changed by `update_session` (the actor's every save writes it,
/// so a live session keeps it) and cleared with `None`.
#[test]
fn session_color_is_stored_updated_and_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let mut s = Session {
        color: Some("teal".into()),
        ..session(p.id, 1)
    };
    store.insert_session(&s).unwrap();
    assert_eq!(store.get_session(s.id).unwrap().color.as_deref(), Some("teal"));

    s.color = Some("red".into());
    store.update_session(&s).unwrap();
    assert_eq!(store.list_sessions(None).unwrap()[0].color.as_deref(), Some("red"));

    s.color = None;
    store.update_session(&s).unwrap();
    assert_eq!(store.get_session(s.id).unwrap().color, None);
}
```

Run: `cargo test -p mkrew-store -j 4`
Expected: FAIL to compile (`Session` has no `color` in `lib.rs`'s `into_session`, or the test literal fails: the store crate itself does not build until Step 4).

- [ ] **Step 4: Store: migration and columns**

Create `crates/mkrew-store/migrations/0015_session_color.sql`:

```sql
-- SPEC §5, ADR 0037. Never edit: schema changes go in a new numbered migration.
ALTER TABLE session ADD COLUMN color TEXT; -- a WORKSPACE_COLORS name, NULL = none
```

`lib.rs`: append `include_str!("../migrations/0015_session_color.sql"),` to `MIGRATIONS` (after the last entry). `insert_session`: add `color` to the column list and `?16` to VALUES, `session.color,` last in `params!`:

```rust
            "INSERT INTO session (id, task_id, project_id, agent_profile, kind, title,
                native_session_id, state, exit_code, cols, rows, started_at, ended_at,
                state_source, state_changed_at, color)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
```

`update_session`:

```rust
            "UPDATE session SET title = ?2, native_session_id = ?3, state = ?4, exit_code = ?5,
                cols = ?6, rows = ?7, ended_at = ?8, state_source = ?9, state_changed_at = ?10,
                color = ?11
             WHERE id = ?1",
```
with `session.color.as_deref()`? (`session.color` is `Option<String>`: pass `session.color,` as the 11th param.) Update the doc comment: `(everything but ids, profile, kind and `started_at`)` stays true.

Both SELECT lists (`list_sessions`, `get_session`): append `, color` after `state_changed_at`. `RawSession`: field `color: Option<String>,`; `from_row`: `color: row.get(15)?,`; `into_session`: replace `terminal_title: None,` by `terminal_title: None,\n            color: self.color,`.

Run: `cargo test -p mkrew-store -j 4`
Expected: PASS (`migrations_are_idempotent`-style tests use `MIGRATIONS.len()`).

- [ ] **Step 5: Failing daemon test**

In `crates/mkrew-daemon/tests/daemon.rs`: add `SessionSetColorParams` to the `mkrew_protocol::{…}` import. After the `rename` helper (`fn rename(…)`) add:

```rust
fn set_color(session_id: Ulid, color: Option<&str>) -> Request {
    Request::SessionSetColor(SessionSetColorParams {
        session_id,
        color: color.map(Into::into),
    })
}
```

In the literal at `:681` add `color: None,`. After `sessions_are_renamed_live_and_exited_but_run_sessions_refuse` add:

```rust
/// `session.set_color` (ADR 0037): a live session keeps its color through its own later
/// saves (state changes, exit); an exited one is colored in the store; `null` clears; a name
/// outside the palette is refused; run sessions may be colored.
#[test]
fn sessions_are_colored_live_and_exited_and_the_name_is_checked() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    let colored: Session = client
        .call(set_color(id, Some("teal")))
        .into_result()
        .unwrap();
    assert_eq!(colored.color.as_deref(), Some("teal"));
    client.wait_event_where(|e| {
        matches!(e, Event::SessionUpdated(s) if s.id == id && s.color.as_deref() == Some("teal"))
    });
    // The actor's final save (exit) keeps the color.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    let exited = client.wait_state(id, SessionState::Exited);
    assert_eq!(exited.color.as_deref(), Some("teal"));

    // Exited: colored in the store; null clears.
    let red: Session = client.call(set_color(id, Some("red"))).into_result().unwrap();
    assert_eq!(red.color.as_deref(), Some("red"));
    assert_eq!(client.session(id).color.as_deref(), Some("red"));
    let cleared: Session = client.call(set_color(id, None)).into_result().unwrap();
    assert_eq!(cleared.color, None);
    assert_eq!(client.session(id).color, None);
    client.wait_event_where(
        |e| matches!(e, Event::SessionUpdated(s) if s.id == id && s.color.is_none()),
    );

    assert_eq!(
        error_code(client.call(set_color(id, Some("#ff0000")))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set_color(Ulid::new(), Some("red")))),
        ErrorCode::NotFound
    );

    // A run session is named by its command, but its color is free.
    let web = run_command("web", ".", &long("echo up", "echo up"));
    client
        .call(set_runs(&project, vec![web], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let web = started.sessions[0].session_id;
    let blue: Session = client.call(set_color(web, Some("blue"))).into_result().unwrap();
    assert_eq!(blue.color.as_deref(), Some("blue"));
    client
        .call(run_stop(&project, None))
        .into_result::<()>()
        .unwrap();
    client.wait_state(web, SessionState::Exited);
    client.stop();
}
```

In `restart_resumes_free_agent_panes_in_tabs_once`, after the `rename(shown.session_id, Some("Fix login"))` call add:

```rust
    client
        .call(set_color(shown.session_id, Some("teal")))
        .into_result::<Session>()
        .unwrap();
```

and after the `resumed.title … "the resumed pane keeps its name"` assertion:

```rust
    assert_eq!(
        resumed.color.as_deref(),
        Some("teal"),
        "the resumed pane keeps its color"
    );
```

Run: `cargo test -p mkrew-daemon --test daemon sessions_are_colored -j 4`
Expected: FAIL to compile (the daemon library does not know `SessionSetColor`; non-exhaustive match in `server.rs`).

- [ ] **Step 6: Daemon implementation and every `Session` literal**

`session.rs`, `Command` after `Rename`:

```rust
    /// `session.set_color`: like `Rename`, the actor owns the row. Replies with the session,
    /// also announced as `session.updated`.
    SetColor {
        color: Option<String>,
        reply: oneshot::Sender<Session>,
    },
```

Handler, after the `Some(Command::Rename { … })` arm:

```rust
                    Some(Command::SetColor { color, reply }) => {
                        self.session.color = color;
                        self.save();
                        let _ = self.hub.events.send(Event::SessionUpdated(self.session.clone()));
                        let _ = reply.send(self.session.clone());
                    }
```

`server.rs`: add `SessionSetColorParams` to the `mkrew_protocol::{…}` import; dispatch after `Request::SessionRename`:

```rust
            Request::SessionSetColor(params) => {
                (self.session_set_color(params).await, After::Continue)
            }
```

Replace the tail of `session_rename` (from `if let Some(sender) = self.hub.sender(session.id) {` to the end of the function) and add the two new functions, so the block from `session_rename` on reads:

```rust
        if command_name(&session).is_some() {
            let message = "a run session is named by its command";
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        let for_actor = title.clone();
        self.edit_session(
            session.id,
            move |reply| Command::Rename {
                title: for_actor,
                reply,
            },
            move |session| session.title = title,
        )
        .await
    }

    /// `session.set_color` (ADR 0037): `null` clears; any other value must be a palette
    /// name. Run sessions may be colored.
    async fn session_set_color(&self, params: SessionSetColorParams) -> Outcome {
        if let Some(color) = &params.color
            && !WORKSPACE_COLORS.contains(&color.as_str())
        {
            let message = format!(
                "unknown color {color:?}; use one of {}",
                WORKSPACE_COLORS.join(", ")
            );
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let for_actor = params.color.clone();
        let color = params.color;
        self.edit_session(
            params.session_id,
            move |reply| Command::SetColor {
                color: for_actor,
                reply,
            },
            move |session| session.color = color,
        )
        .await
    }

    /// Edits a session's row: a live session through its task (`command` builds the message;
    /// the task owns the row and saves it on every state change, so a store-only edit would be
    /// undone), an exited one in the store (`stored` applies the edit). Replies with the edited
    /// session, announced as `session.updated`.
    async fn edit_session(
        &self,
        session_id: Ulid,
        command: impl FnOnce(oneshot::Sender<Session>) -> Command,
        stored: impl FnOnce(&mut Session),
    ) -> Outcome {
        if let Some(sender) = self.hub.sender(session_id) {
            let (reply, edited) = oneshot::channel();
            if sender.send(command(reply)).await.is_ok()
                && let Ok(edited) = edited.await
            {
                return ok(&edited);
            }
        }
        // Not live (any more): re-read, its task may have saved the exit meanwhile.
        let found = self.store().get_session(session_id);
        let mut session = match found {
            Ok(session) => session,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if session.state != SessionState::Exited {
            // Between leaving `live` and its final save: that save would undo this edit.
            let message = format!("session {} is exiting; try again", session.id);
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        stored(&mut session);
        let saved = self.store().update_session(&session);
        if let Err(e) = saved {
            return store_outcome::<()>(Err(e));
        }
        session.terminal_title = self.hub.terminal_title(session.id);
        let _ = self.hub.events.send(Event::SessionUpdated(session.clone()));
        ok(&session)
    }
```

Update the `session_rename` doc comment's second sentence to stay accurate (unchanged text is fine). The pre-checks of `session_rename` above (`title` validation, `get_session`, run check) stay as they are.

Resume and start: `Start` gets `/// The pane's color, kept by a resume (ADR 0037).\n    color: Option<String>,`; `start_session`'s literal: `terminal_title: None,\n            color: start.color,`. The four `Start { … }` literals: `server.rs` spawn and task-run and `server/run.rs`: `color: None,`; `server/resume.rs`: `color: last.color.clone(),` (after `title: last.title.clone(),`).

Every other `Session { … }` literal in the "Current state" list: add `color: None,` after `terminal_title: None,` (grep `terminal_title: None` for the exact lines). 

Run: `cargo test -p mkrew-daemon -j 4` then `cargo test -p mkrew-daemon --test daemon sessions_are -j 4` and `… restart_resumes_free_agent_panes_in_tabs_once -j 4`
Expected: PASS (the rename test still passes through `edit_session`).

- [ ] **Step 7: Failing UI tests (pure)**

`lifecycle.rs`: in its test `session()` literal add `color: None,`; add `SessionSetColorParams` to the `mkrew_protocol` import; after `session_rename_params_trim_clear_and_skip_unchanged` add:

```rust
    #[test]
    fn session_color_params_skip_unchanged_and_clear_with_none() {
        let mut pane = session(Ulid::new(), SessionKind::Agent, SessionState::Working);
        let params = session_color_params(&pane, Some("teal")).unwrap();
        assert_eq!(params.session_id, pane.id);
        assert_eq!(params.color.as_deref(), Some("teal"));
        // No color on an uncolored pane: nothing to send.
        assert_eq!(session_color_params(&pane, None), None);
        pane.color = Some("teal".into());
        assert_eq!(session_color_params(&pane, Some("teal")), None);
        let cleared = session_color_params(&pane, None).unwrap();
        assert_eq!(cleared.color, None);
    }
```

`sidebar.rs` tests (and its `session()` literal `color: None,`), after `a_rename_reply_sets_only_the_title`:

```rust
    #[test]
    fn a_color_reply_sets_only_the_color() {
        let api = project("api");
        let live = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![live.clone()]);
        sidebar.apply(&Event::SessionExited(mkrew_protocol::SessionExitedEvent {
            session_id: live.id,
            exit_code: Some(0),
        }));
        sidebar.set_color(live.id, Some("teal".into()));
        let shown = &sidebar.sessions()[0];
        assert_eq!(shown.color.as_deref(), Some("teal"));
        assert_eq!(shown.state, SessionState::Exited, "the reply's state is not applied");
    }
```

(Check the exact `SessionExitedEvent` fields against the sibling test `a_rename_reply_sets_only_the_title` and copy its construction.) Add `color: None,` to the other UI literals listed in the Current state.

Run: `cargo test -p mkrew-ui --lib lifecycle sidebar -j 4` (two runs if the filter form is rejected: `cargo test -p mkrew-ui --lib lifecycle -j 4`, then `… sidebar -j 4`)
Expected: FAIL to compile (`session_color_params`, `Sidebar::set_color`).

- [ ] **Step 8: UI pure code**

`lifecycle.rs`, after `session_rename_params`:

```rust
/// The `session.set_color` for a picked color (`None` = no color); `None` when nothing
/// changes (ADR 0037).
pub fn session_color_params(
    session: &Session,
    color: Option<&str>,
) -> Option<SessionSetColorParams> {
    (color != session.color.as_deref()).then(|| SessionSetColorParams {
        session_id: session.id,
        color: color.map(str::to_owned),
    })
}
```

`sidebar.rs`, after `set_title`:

```rust
    /// The color from a `session.set_color` reply: only the color, like [`Sidebar::set_title`].
    pub fn set_color(&mut self, session_id: Ulid, color: Option<String>) {
        self.update_session(session_id, |session| session.color = color);
    }
```

`context_menu.rs`, append:

```rust
/// The open menu of one pane (Rename, Change color), at `position` (window coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneMenu {
    pub session_id: Ulid,
    pub position: Point<Pixels>,
}
```

Run: `cargo test -p mkrew-ui --lib lifecycle -j 4` then `… sidebar -j 4`
Expected: PASS.

- [ ] **Step 9: Draw and wire the menu**

`theme.rs`, after `PANE_HALF_GAP`:

```rust
/// Thickness of the accent line under the header of a colored pane (ADR 0037).
pub const PANE_ACCENT_WIDTH: f32 = 2.0;
```

`main.rs`:

1. Imports: `use mkrew_ui::context_menu::{PaneMenu, TabMenu};`; `SessionSetColorParams` is not needed in `main.rs` (the request is built from `lifecycle`'s value: `Request::SessionSetColor(params)`).
2. Field after `tab_menu`: `/// The open pane menu (Rename, Change color).\n    pane_menu: Option<PaneMenu>,` init `pane_menu: None,`, disconnect reset `shell.pane_menu = None;`.
3. Generalise the rename sender. Rename `fn send_rename` → `fn send_edit`, replace its `what: &'static str` parameter by `failure: &'static str` and its doc by `/// Sends a `tab.rename` / `session.rename` / `session.set_color`; `shown` applies the reply, a failure goes to the tab bar's error line after `failure`.` Body: the two failure lines become

```rust
                        tracing::warn!("{failure}: {error:#}");
                        shell.tab_error = Some(format!("{failure}: {error:#}"));
```
   Callers: `rename_tab` → `self.send_edit(Request::TabRename(params), "Cannot rename the tab", shown, window, cx)`; `rename_session` → `… Request::SessionRename(params), "Cannot rename the pane", …`.
4. After `rename_session` add:

```rust
    /// Sets (or clears) the color of `session_id`'s pane from its menu.
    fn set_pane_color(
        &mut self,
        session_id: Ulid,
        color: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.pane_menu = None;
        let params = (self.sidebar.sessions().iter())
            .find(|s| s.id == session_id)
            .and_then(|session| lifecycle::session_color_params(session, color));
        if let Some(params) = params {
            // Shown at once (header, chip), without waiting for the echo.
            let shown = |shell: &mut Self, session: Session| {
                shell.sidebar.set_color(session.id, session.color);
            };
            self.send_edit(
                Request::SessionSetColor(params),
                "Cannot change the pane color",
                shown,
                window,
                cx,
            );
        }
        cx.notify();
    }

    /// Opens the menu of `session_id`'s pane at `position`, focusing the pane.
    fn open_pane_menu(
        &mut self,
        session_id: Ulid,
        position: Point<Pixels>,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        self.tab_menu = None;
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.focus(session_id);
        }
        self.pane_menu = Some(PaneMenu {
            session_id,
            position,
        });
        cx.notify();
    }
```

5. `render_pane_menu`, after `render_tab_menu`:

```rust
    /// The pane menu: Rename (inline, like F2; greyed for a run pane, which is named by its
    /// command) and the color swatches of the workspace palette. Floats at the click.
    fn render_pane_menu(
        &self,
        menu: &PaneMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        let session_id = menu.session_id;
        let session = self.sidebar.sessions().iter().find(|s| s.id == session_id)?;
        let renamable = mkrew_core::run::command_name(session).is_none();
        let current = session.color.as_deref();
        let separator = || {
            div()
                .h(px(theme::BORDER_WIDTH))
                .my(px(space::XS))
                .bg(rgb(theme.border))
        };
        let panel = menu_panel("pane-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.pane_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("pane-menu-rename".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child("Rename"))
                    .child(
                        div()
                            .text_size(font_size::META)
                            .child(self.keymap.label(Command::RenamePane)),
                    )
                    .when(renamable, |el| {
                        el.on_click(cx.listener(move |shell, _, window, cx| {
                            shell.pane_menu = None;
                            shell.start_pane_rename(session_id, window, cx);
                        }))
                    })
                    .when(!renamable, |el| el.opacity(opacity::DISABLED)),
            )
            .child(separator())
            .child(
                div()
                    .px(px(space::S))
                    .pt(px(space::XS))
                    .text_size(font_size::META)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child("Change color"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::S))
                    .p(px(space::S))
                    .children(
                        std::iter::once(None)
                            .chain(WORKSPACE_COLORS.iter().map(|name| Some(*name)))
                            .map(|color| {
                                color_swatch(
                                    "pane-color",
                                    color,
                                    current,
                                    theme,
                                    cx,
                                    move |shell, color, window, cx| {
                                        shell.set_pane_color(session_id, color, window, cx);
                                    },
                                )
                            }),
                    ),
            );
        Some(
            deferred(
                anchored()
                    .position(menu.position)
                    .snap_to_window_with_margin(px(space::S))
                    .child(panel),
            )
            .into_any_element(),
        )
    }
```

6. Generalise `color_swatch` (Task 8 left its signature and ids as they were):

```rust
/// One color choice of a color menu: a filled dot (`None` = an empty ring = no color); the
/// current one is ringed in the text color. A click calls `pick`; the element id is
/// `<id_prefix>-<color>`.
fn color_swatch(
    id_prefix: &'static str,
    color: Option<&'static str>,
    current: Option<&str>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
    pick: impl Fn(&mut Shell, Option<&'static str>, &mut Window, &mut Context<'_, Shell>) + 'static,
) -> gpui::AnyElement {
    let fill = color.and_then(|name| theme.workspace_color(name));
    let ring = if color == current {
        theme.text
    } else {
        fill.unwrap_or(theme.text_muted)
    };
    div()
        .id(SharedString::from(format!(
            "{id_prefix}-{}",
            color.unwrap_or("none")
        )))
        .flex_none()
        .size(px(theme::WORKSPACE_SWATCH_SIZE))
        .rounded_full()
        .cursor_pointer()
        .border(px(theme::WORKSPACE_SWATCH_BORDER))
        .border_color(rgb(ring))
        .when_some(fill, |el, fill| el.bg(rgb(fill)))
        .hover(|style| style.opacity(opacity::HOVER))
        .tooltip(tooltip(theme, color.unwrap_or("No color"), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            pick(shell, color, window, cx);
        }))
        .into_any_element()
}
```

   Its one existing caller in `render_workspace_menu` (the two swatch calls inside the Color row) becomes:

```rust
                    .child(color_swatch(
                        "workspace-color",
                        None,
                        current_color,
                        theme,
                        cx,
                        move |shell, color, window, cx| {
                            shell.set_workspace_color(project_id, color, window, cx);
                        },
                    ))
                    .children(WORKSPACE_COLORS.iter().map(|name| {
                        color_swatch(
                            "workspace-color",
                            Some(*name),
                            current_color,
                            theme,
                            cx,
                            move |shell, color, window, cx| {
                                shell.set_workspace_color(project_id, color, window, cx);
                            },
                        )
                    }))
```
   (Grep `color_swatch(`: if Task 8/9 added another caller, give it the same two extra arguments.)

7. Root `render`, after the `tab_menu` children:

```rust
            .children(
                self.pane_menu
                    .as_ref()
                    .and_then(|menu| self.render_pane_menu(menu, &theme, cx)),
            )
```

8. `pane_frame`: wrap the header with the right-click handler and pass the color. Replace the call `pane_header(…)` by

```rust
                let color = info
                    .color
                    .as_deref()
                    .and_then(|name| theme.workspace_color(name));
                pane_header(
                    info,
                    self.agent_badge(&info.agent_profile),
                    breadcrumb,
                    pill,
                    title,
                    self.usages.get(session).and_then(usage::context_share),
                    conflict,
                    turns,
                    queued,
                    color,
                    now,
                    theme,
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |shell, event: &MouseDownEvent, _, cx| {
                        shell.open_pane_menu(session, event.position, cx);
                    }),
                )
```

9. `pane_header`: add `color: Option<u32>,` before `now`; doc: append `A colored pane (ADR 0037) tints its chip and draws an accent line under the header.` The chip and the header container become:

```rust
    div()
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
        .child(agent_chip(badge, theme).when_some(color, |chip, color| {
            chip.bg(theme.tint(color)).border_color(rgb(color))
        }))
```
   (the rest of the children as in Task 11).

Run: `cargo check -p mkrew-ui` then `cargo test -p mkrew-ui --lib -j 4`
Expected: compiles; PASS.

- [ ] **Step 10: ADR 0037**

Create `docs/decisions/0037-pane-colors.md`:

```markdown
# 0037 — Pane colors (session.set_color)

- Date: 2026-10-10
- Status: accepted
- Milestone: inbox 2026-10-10

## Context

The redesign adds a right-click menu on panes with "Rename" and "Change color", the color from
the same palette as workspaces (ADR 0020). ADR 0020 kept workspace colors to workspaces
("no coloring of tabs or inbox rows yet") and SPEC §14 principle 2 allows no saturated color
besides the accent, the status colors and the workspace avatar. A pane is a session; its name
already lives in `session.title` and changes with `session.rename` (ADR 0031).

## Options

1. A `color` column on `session`, set with a new `session.set_color` request; shown as a tinted
   agent chip and a thin line under the pane header. A live session is edited through its task
   (like a rename), so the task's next save keeps it.
2. Store the color in the tab layout JSON: a second place that nothing outside the tab sees, lost
   when a pane moves to another tab or is resumed.
3. Add a color field to `session.rename`: its `null` already means "clear the title", so two
   optional fields would conflate two verbs.
4. Color the whole header or the pane border. The header fill fights the status pill; the border
   already carries state (needs input, focus).

## Decision

Option 1. `session.color` (migration `0015_session_color.sql`) holds one of
`WORKSPACE_COLORS` or `NULL`; the daemon refuses any other name and `null` clears it. Reply =
the Session, also `session.updated`. A live session goes through `Command::SetColor`, an exited
one through the store; both share one helper with `session.rename`. A resume copies the color.
Run sessions may be colored (only their title is fixed). The UI tints the agent chip in the
pane header with the color and draws a 2 px line under the header; the state-driven pane border,
the status pill and the sidebar rows are unchanged. The colors keep ADR 0020's ≥ 3:1 contrast on
`bg`.

## Consequences

Protocol: `Session.color` (optional on the wire), request `session.set_color`, protocol version
`34`. SPEC: §5 (column), §7 (request and `session.updated` text), §10 (color bullet), §11
(pane menu), §14 principle 2 (a second place where workspace colors show). ADR 0020's "never
used for state" still holds: a pane color is decoration chosen by the user. No CLI flag
(`mk session color` only if someone asks).
```

- [ ] **Step 11: SPEC**

§5 session table, after the `title` row:

```sql
  color             TEXT,                -- user's pane color: a workspace color name (session.set_color, ADR 0037)
```

§7, after the `session.rename` row:

```markdown
| `session.set_color` | `session_id`, `color?` (a workspace color name; `null` clears) | the Session, also broadcast as `session.updated` — `invalid_request` for a name outside the palette, `invalid_state` for one caught exiting (ADR 0037) |
```

§7 events, replace the `session.updated` row text with: `| \`session.updated\` | a session's title or color changed (\`session.rename\`, \`session.set_color\`): the whole Session |`.

§10 Color bullet, append: 

```markdown
  A pane has its own color too: right-click its header → Change color (same palette; ADR 0037).
```

§11, after the pane header bullet:

```markdown
- Pane menu (right-click on the header): **Rename** (inline like F2; greyed for a run pane) and
  **Change color** (the workspace palette; none clears). A colored pane tints its agent chip and
  draws a thin line under its header; the border still shows state (ADR 0037).
```

§14 principle 2: replace `Workspace colors\n   (ADR 0020) are the one exception: a small tinted letter avatar per workspace.` by:

```markdown
Workspace colors
   (ADR 0020) and pane colors (ADR 0037) are the exception: a small tinted letter avatar per
   workspace, and a tinted agent chip with a thin line under the header of a colored pane.
```

- [ ] **Step 12: HUMAN_CHECKS**

Append to the 2026-10-10 section:

```markdown
- [ ] **Pane menu (item 5):** right-click a pane header → Rename / Change color; Rename edits the title
  in place (Enter saves, Esc cancels; the menu closes); on a Run pane Rename is greyed. Pick a color:
  the agent chip is tinted, a thin line of that color shows under the header, the border and pill are
  unchanged; the first swatch (empty ring) clears it. Colors look distinct in light and dark.
  Restart the daemon (`mk daemon stop`, reopen): the color and name come back on the resumed pane.
  Open the same pane from the sidebar and from a second UI window: the color shows in both.
```

- [ ] **Step 13: Verify and tick**

Run the full verification (`& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`; fallback: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`).
Tick `docs/INBOX.md` line 72 (`**5. Terminal tabs + pane header**`): `[~]` (or `[ ]`) → `[x]`.
Suggested message: `feat: pane right-click menu with rename and per-pane color (session.set_color, protocol v34)`

---

### Task 13: Pin: storage + protocol (project, session, tab)

Inbox: line 80 `Pin feature` (with Task 14; do not tick yet). Spec: §5 (SPEC.md:316-320), §7 (:501, :520-522, :558-559).

**Current state (investigated):**
- No pin exists. Project order is `ORDER BY created_at, id` (`mkrew-store/src/lib.rs` `list_projects`); sessions `ORDER BY started_at, id` (`list_sessions`); tabs `ORDER BY position, id` (`list_tabs`). The UI re-sorts sessions (newest first, `Sidebar::rows`) and tabs (`Tabs::sort`), so pinned-first is a UI sort (Task 14). The store order stays as it is: no client needs the DB to sort by pin, and unpinning must return an item to its old place.
- Tabs are SQLite rows (`terminal_tab`, migration `0002_terminals.sql`), not a JSON file. `tab.move` / `tab.rename` go through `Store::update_tab` and `Server::tab_updated` (server.rs), which broadcasts `tab.updated`.
- `session.rename` (ADR 0031) is the pattern for a session field: a live session's task owns the row (`Command::Rename`, session.rs actor loop), because its next `save()` (`Store::update_session`, which lists every mutable column) and every `session.updated` it sends come from its own `Session` clone. A store-only write would be undone by the next save. An exited session is written in the store (`Server::session_rename`, the block after the actor reply).
- `Store::update_session` writes columns explicitly, so `pinned` MUST be added there or the actor's save keeps the old value. `session.updated` carries the whole `Session`, so the sidebar replaces its copy, pin included.
- Resume (`server/resume.rs` `resume`) starts a NEW session (new id, `Server::start_session` builds the `Session`) and only copies title, native id, size and task. Without a change the pin of a resumed pane is lost. Tab pins survive (the tab row is untouched, only its layout is edited by `replace_in_tabs`); project pins are untouched.
- `ProjectUpdateParams` has no `Default`: every literal lists all fields. Literals of `Project`, `Session`, `TerminalTab` and `ProjectUpdateParams` are in many files (list in Step 7).
- `Request::SessionRename` / `TabMove` are matched only in `server.rs` `dispatch`; no other exhaustive `Request` match exists.

**Files:**
- Create: `crates/mkrew-store/migrations/0016_pinned.sql`
- Modify: `crates/mkrew-store/src/lib.rs` (`MIGRATIONS`, `PROJECT_COLUMNS`, `ProjectChanges`, `add_project`, `update_project`, `list_tabs`, `get_tab`, `create_tab`, `set_tab_pinned`, `insert_session`, `update_session`, `list_sessions`, `get_session`, `RawProject`, `RawSession`, `RawTab`), `crates/mkrew-store/src/tests.rs`
- Modify: `crates/mkrew-protocol/src/message.rs` (`Request`, `Project`, `Session`, `TerminalTab`, `ProjectUpdateParams`, new `SessionPinParams`, `TabPinParams`, tests), `crates/mkrew-protocol/src/lib.rs:16`
- Modify: `crates/mkrew-daemon/src/server.rs` (imports, `dispatch`, `project_update`, `session_pin`, `Start`, `start_session`, the `Start` literals), `crates/mkrew-daemon/src/session.rs` (`Command::Pin` + handler), `crates/mkrew-daemon/src/server/resume.rs`, `crates/mkrew-daemon/src/server/run.rs`
- Modify (literals only, Step 7): every file listed there
- Test: `crates/mkrew-protocol/src/message.rs`, `crates/mkrew-store/src/tests.rs`, `crates/mkrew-daemon/tests/daemon.rs`
- Modify: `docs/SPEC.md` §5, §7

**Interfaces:**
- Consumes: Task 12's `Session.color` and `session.set_color` already exist (their SQL columns and `Command::SetColor` sit beside the new ones; `pinned` is appended after `color`).
- Produces (Task 14 relies on these names):
  - `mkrew_protocol::Project.pinned: bool`, `Session.pinned: bool`, `TerminalTab.pinned: bool`, all `#[serde(default)]`.
  - `ProjectUpdateParams.pinned: Option<bool>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`).
  - `Request::SessionPin(SessionPinParams { session_id: Ulid, pinned: bool })` = `session.pin` → `Session`, also broadcast as `session.updated`.
  - `Request::TabPin(TabPinParams { tab_id: Ulid, pinned: bool })` = `tab.pin` → `TerminalTab`, also broadcast as `tab.updated`.
  - `mkrew_store::ProjectChanges.pinned: Option<bool>`, `Store::set_tab_pinned(&self, id: Ulid, pinned: bool) -> Result<TerminalTab>`.
  - `PROTOCOL_VERSION = 35` (was 34).

- [ ] **Step 1: Failing protocol tests**

In `message.rs` `mod tests`, `every_m1_request_round_trips`:
- in the `Request::ProjectUpdate(ProjectUpdateParams { .. })` literal add after `color: Some(Some("teal".into())),`:

```rust
                pinned: Some(true),
```

- after the `Request::TabMove(TabMoveParams { .. }),` entry add:

```rust
            Request::TabPin(TabPinParams {
                tab_id: sid(),
                pinned: true,
            }),
```

- after the `Request::SessionRename(SessionRenameParams { .. }),` entry add:

```rust
            Request::SessionPin(SessionPinParams {
                session_id: sid(),
                pinned: true,
            }),
```

In `project_update_tells_missing_from_null`, after the `color` asserts (`Some(Some("teal".into()))`) add:

```rust
        // A pin is a plain bool: absent = unchanged, `false` = unpin.
        assert_eq!(parse(json!({"project_id": id})).pinned, None);
        assert_eq!(
            parse(json!({"project_id": id, "pinned": false})).pinned,
            Some(false)
        );
```

and in the `keep` literal add `pinned: None,` after `color: None,` (the next assert already proves an unchanged `pinned` is left out of the wire).

In the events test, the `Event::ProjectUpdated(Project { .. })` literal gets `pinned: true,` after `color: Some("teal".into()),`; the `Event::TabUpdated(TerminalTab { .. })` literal gets `pinned: true,` after `layout: serde_json::Value::Null,`, and after `assert_eq!(tab["data"]["layout"], serde_json::Value::Null);` add:

```rust
        assert_eq!(tab["data"]["pinned"], true);
```

the `Event::SessionUpdated(Session { .. })` literal gets `pinned: true,` after `ended_at: None,`, and after `assert_eq!(updated["data"]["title"], "Fix login");` add:

```rust
        assert_eq!(updated["data"]["pinned"], true);
```

Run: `cargo test -p mkrew-protocol -j 4`
Expected: FAIL to compile (`no variant SessionPin`, `no field pinned`).

- [ ] **Step 2: Protocol types and version**

`message.rs` `Project`, after `color`:

```rust
    /// Pinned workspaces lead the sidebar (SPEC §10).
    #[serde(default)]
    pub pinned: bool,
```

`Session`, after `ended_at` (and after `color`, which Task 12 added):

```rust
    /// Pinned sessions lead their workspace's sidebar list (SPEC §10). Set with `session.pin`.
    #[serde(default)]
    pub pinned: bool,
```

`TerminalTab`, after `layout`:

```rust
    /// Pinned tabs sit left of the others, each group in `position` order (SPEC §10). Set with
    /// `tab.pin`, which leaves `position` alone, so unpinning puts the tab back.
    #[serde(default)]
    pub pinned: bool,
```

`ProjectUpdateParams`, after `color`:

```rust
    /// Pin or unpin the workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
```

After `TabMoveParams`:

```rust
/// `tab.pin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabPinParams {
    pub tab_id: Ulid,
    pub pinned: bool,
}
```

After `SessionRenameParams`:

```rust
/// `session.pin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPinParams {
    pub session_id: Ulid,
    pub pinned: bool,
}
```

`Request`, after `TabMove(TabMoveParams),`:

```rust
    /// Returns the [`TerminalTab`]; also broadcast as `tab.updated`.
    #[serde(rename = "tab.pin")]
    TabPin(TabPinParams),
```

after `SessionRename(SessionRenameParams),`:

```rust
    /// Returns the [`Session`]; also broadcast as `session.updated`.
    #[serde(rename = "session.pin")]
    SessionPin(SessionPinParams),
```

`crates/mkrew-protocol/src/lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 35;`

Run: `cargo test -p mkrew-protocol -j 4`
Expected: PASS (`every_m1_request_round_trips`, `project_update_tells_missing_from_null`, the event round-trips).

- [ ] **Step 3: Failing store test**

Create `crates/mkrew-store/migrations/0016_pinned.sql` later (Step 4). In `tests.rs`, after `project_color_persists_and_clears`:

```rust
#[test]
fn pinned_persists_for_workspaces_sessions_and_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert!(!p.pinned);

    // Workspace: another change keeps the pin.
    let pin = ProjectChanges {
        pinned: Some(true),
        ..ProjectChanges::default()
    };
    assert!(store.update_project(p.id, pin).unwrap().pinned);
    let teal = ProjectChanges {
        color: Some(Some("teal")),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, teal).unwrap();
    assert!(open(&dir).get_project(p.id).unwrap().pinned);

    // Session: inserted pinned; a save that does not touch the pin keeps it; update_session
    // is what a live session's task writes, so it must carry the flag both ways.
    let mut s = Session {
        pinned: true,
        ..session(p.id, 1)
    };
    store.insert_session(&s).unwrap();
    assert!(open(&dir).get_session(s.id).unwrap().pinned);
    s.state = SessionState::Working;
    store.update_session(&s).unwrap();
    assert!(store.get_session(s.id).unwrap().pinned);
    s.pinned = false;
    store.update_session(&s).unwrap();
    assert!(!open(&dir).list_sessions(None).unwrap()[0].pinned);

    // Tab: pinning leaves the position, so unpinning returns it to its place.
    let tab = store.create_tab(p.id, "one", None).unwrap();
    assert!(!tab.pinned);
    let pinned = store.set_tab_pinned(tab.id, true).unwrap();
    assert!(pinned.pinned);
    assert_eq!(pinned.position, tab.position);
    assert_eq!(open(&dir).list_tabs(p.id).unwrap(), vec![pinned]);
    assert!(matches!(
        store.set_tab_pinned(Ulid::new(), true),
        Err(StoreError::TabNotFound(_))
    ));
}
```

In the test helper `session()` (tests.rs:9) add `pinned: false,` after `ended_at: None,`.

Run: `cargo test -p mkrew-store pinned -j 4`
Expected: FAIL to compile (`no field pinned` on `ProjectChanges`, no `set_tab_pinned`).

- [ ] **Step 4: Migration and store**

Create `crates/mkrew-store/migrations/0016_pinned.sql`:

```sql
-- SPEC §5. Never edit: schema changes go in a new numbered migration.
-- Pinned workspaces, sessions and tabs lead their lists (SPEC §10).
ALTER TABLE project ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE session ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE terminal_tab ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
```

`lib.rs` `MIGRATIONS`, after the last entry (before `];`; list `crates/mkrew-store/migrations/` first and use the next free number if `0016` is taken):

```rust
    include_str!("../migrations/0016_pinned.sql"),
```

Project:
- `PROJECT_COLUMNS`: append `, pinned` (the string now ends `…, color, default_shell, pinned"`).
- `ProjectChanges`, after `color`: `pub pinned: Option<bool>,`
- `add_project` literal: `pinned: false,` after `color: None,`.
- `update_project`: after the `color` block add

```rust
        if let Some(pinned) = changes.pinned {
            project.pinned = pinned;
        }
```

and replace the UPDATE:

```rust
        tx.execute(
            "UPDATE project SET name = ?2, default_agent = ?3, run_autostart = ?4, color = ?5, default_shell = ?6, pinned = ?7 WHERE id = ?1",
            params![
                id.to_string(),
                project.name,
                project.default_agent,
                project.run_autostart,
                project.color,
                project.default_shell,
                project.pinned
            ],
        )?;
```

- `RawProject`: field `pinned: bool,` after `default_shell`; `from_row`: `pinned: row.get(11)?,`; `into_project`: `pinned: self.pinned,`.

Tab:
- `list_tabs` and `get_tab`: the column list becomes `id, project_id, title, position, layout, pinned`.
- `create_tab` literal: `pinned: false,` (the INSERT keeps the column default).
- after `move_tab` add:

```rust
    /// Pins or unpins the tab. `position` stays, so unpinning puts it back in place.
    pub fn set_tab_pinned(&self, id: Ulid, pinned: bool) -> Result<TerminalTab> {
        self.update_tab(
            id,
            "UPDATE terminal_tab SET pinned = ?2 WHERE id = ?1",
            pinned,
        )
    }
```

- `RawTab`: `pinned: bool,` after `layout`; `from_row`: `pinned: row.get(5)?,`; `into_tab`: `pinned: self.pinned,`.

Session (Task 12 appended `color` last: SELECT index 15, `?16` in the INSERT, `color = ?11` in the UPDATE, as in its Step 4; `pinned` goes right after it. If the current text differs, keep `pinned` last and renumber):
- `list_sessions` and `get_session`: append `, pinned` to the SELECT column list (after `color`).
- `insert_session`: append `, pinned` to the column list, `, ?17` to `VALUES (…)` and `session.pinned,` as the last param.
- `update_session`: append `, pinned = ?12` after `color = ?11` and `session.pinned,` as the last param. Without this the actor's next `save()` would write the old value back.
- `RawSession`: `pinned: bool,`; `from_row`: `pinned: row.get(16)?,`; `into_session`: `pinned: self.pinned,`.

Run: `cargo test -p mkrew-store -j 4`
Expected: PASS (`pinned_persists_for_workspaces_sessions_and_tabs`; the migration-count tests compare with `MIGRATIONS.len()`, so they follow).

- [ ] **Step 5: Failing daemon tests**

`crates/mkrew-daemon/tests/daemon.rs`. Add `SessionPinParams` and `TabPinParams` to the `mkrew_protocol::{…}` import (alphabetical: after `SessionKillParams, SessionKind, SessionListParams,` insert `SessionPinParams,` before `SessionPortsEvent`; after `TabMoveParams,` insert `TabPinParams,`).

After the `rename()` helper (`fn rename(session_id: Ulid, title: Option<&str>) -> Request`) add:

```rust
fn pin(session_id: Ulid, pinned: bool) -> Request {
    Request::SessionPin(SessionPinParams { session_id, pinned })
}
```

After `sessions_are_renamed_live_and_exited_but_run_sessions_refuse` add:

```rust
/// `session.pin` (SPEC §7): like a rename, a live session keeps its pin through its own later
/// saves (state changes, exit); an exited one is pinned in the store; both are announced.
#[test]
fn sessions_are_pinned_live_and_exited_and_keep_the_pin_through_saves() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);
    assert!(!client.session(id).pinned);

    let pinned: Session = client.call(pin(id, true)).into_result().unwrap();
    assert!(pinned.pinned);
    client.wait_event_where(|e| matches!(e, Event::SessionUpdated(s) if s.id == id && s.pinned));
    // The actor's final save (exit) keeps the pin.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    let exited = client.wait_state(id, SessionState::Exited);
    assert!(exited.pinned);

    // Exited: unpinned in the store.
    let unpinned: Session = client.call(pin(id, false)).into_result().unwrap();
    assert!(!unpinned.pinned);
    assert!(!client.session(id).pinned);
    client.wait_event_where(|e| matches!(e, Event::SessionUpdated(s) if s.id == id && !s.pinned));
    assert_eq!(
        error_code(client.call(pin(Ulid::new(), true))),
        ErrorCode::NotFound
    );
    client.stop();
}

/// `tab.pin` and `project.update` `pinned`: stored, announced, `position` untouched, kept by
/// other changes, and still there after a daemon restart.
#[test]
fn workspace_and_tab_pins_persist_across_daemon_restart() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let first: TerminalTab = client
        .call(create_tab(&project, "one"))
        .into_result()
        .unwrap();
    let second: TerminalTab = client
        .call(create_tab(&project, "two"))
        .into_result()
        .unwrap();
    let tab_pin = |tab: &TerminalTab, pinned| {
        Request::TabPin(TabPinParams {
            tab_id: tab.id,
            pinned,
        })
    };
    let pin_project = |pinned| {
        Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: None,
            run_autostart: None,
            color: None,
            pinned: Some(pinned),
        })
    };

    let pinned: TerminalTab = client.call(tab_pin(&second, true)).into_result().unwrap();
    assert!(pinned.pinned);
    assert_eq!(pinned.position, second.position, "pinning never moves a tab");
    client.wait_event(&Event::TabUpdated(pinned));
    let unknown = Request::TabPin(TabPinParams {
        tab_id: Ulid::new(),
        pinned: true,
    });
    assert_eq!(error_code(client.call(unknown)), ErrorCode::NotFound);

    let saved: Project = client.call(pin_project(true)).into_result().unwrap();
    assert!(saved.pinned);
    client.wait_event(&Event::ProjectUpdated(saved));
    // Another field changes, the pin stays.
    let other: Project = client
        .call(set_default_agent(&project, None))
        .into_result()
        .unwrap();
    assert!(other.pinned);

    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert!(listed[0].pinned);
    let tabs = list_tabs(&mut client, &project);
    let tabs: Vec<(&str, bool)> = tabs.iter().map(|t| (t.title.as_str(), t.pinned)).collect();
    assert_eq!(tabs, [("one", false), ("two", true)]);

    let unpinned: TerminalTab = client.call(tab_pin(&first, false)).into_result().unwrap();
    assert!(!unpinned.pinned);
    let unpinned: Project = client.call(pin_project(false)).into_result().unwrap();
    assert!(!unpinned.pinned);
    client.stop();
}
```

In the same file, `restart_resumes_free_agent_panes_in_tabs_once` (resume creates a new session id; the pin must move to it): after the `client.call(rename(shown.session_id, Some("Fix login")))…unwrap();` statement add

```rust
    client
        .call(pin(shown.session_id, true))
        .into_result::<Session>()
        .unwrap();
```

after the `assert_eq!(resumed.title.as_deref(), Some("Fix login"), "the resumed pane keeps its name");` add

```rust
    assert!(resumed.pinned, "the resumed pane keeps its pin");
```

and after `let newest = sessions.iter().max_by_key(|s| (s.started_at, s.id)).unwrap();` add

```rust
    assert!(newest.pinned, "every resume carries the pin on");
```

Every `Request::ProjectUpdate(ProjectUpdateParams { .. })` literal and every `Session { .. }` literal in this file gets the new field in Step 7 (the file does not compile before).

Run: `cargo test -p mkrew-daemon --test daemon pin -j 4`
Expected: FAIL to compile (`no field pinned`).

- [ ] **Step 6: Daemon**

`session.rs` `Command`, after `Rename { … }`:

```rust
    /// `session.pin`: the actor owns the row (see `Rename`). Replies with the session, also
    /// announced as `session.updated`.
    Pin {
        pinned: bool,
        reply: oneshot::Sender<Session>,
    },
```

Actor loop, after the `Some(Command::Rename { title, reply }) => { … }` arm:

```rust
                    Some(Command::Pin { pinned, reply }) => {
                        self.session.pinned = pinned;
                        self.save();
                        let _ = self.hub.events.send(Event::SessionUpdated(self.session.clone()));
                        // The caller may have gone away; the pin stands.
                        let _ = reply.send(self.session.clone());
                    }
```

`server.rs`:
- import `SessionPinParams` in the `mkrew_protocol::{…}` list (after `SessionKind,`).
- `dispatch`, after the `Request::TabMove(params) => { … }` arm:

```rust
            Request::TabPin(params) => {
                let tab = self.store().set_tab_pinned(params.tab_id, params.pinned);
                (self.tab_updated(tab), After::Continue)
            }
```

- `dispatch`, after `Request::SessionRename(params) => …`:

```rust
            Request::SessionPin(params) => (self.session_pin(params).await, After::Continue),
```

- `project_update`: in `ProjectChanges { … }` add `pinned: params.pinned,` after the `color:` line.
- after `session_rename` / `session_set_color` add (Task 12's `edit_session` does the live-vs-exited work; nothing is copied):

```rust
    /// `session.pin`: through the session's task while it lives (it owns the row, ADR 0031),
    /// in the store once it exited. Run sessions can be pinned too.
    async fn session_pin(&self, params: SessionPinParams) -> Outcome {
        let pinned = params.pinned;
        self.edit_session(
            params.session_id,
            move |reply| Command::Pin { pinned, reply },
            move |session| session.pinned = pinned,
        )
        .await
    }
```

- `struct Start`, next to Task 12's `color` field:

```rust
    /// A resumed session keeps the pin of the one it replaces.
    pinned: bool,
```

- `start_session`: in the `Session { … }` literal add `pinned: start.pinned,` after Task 12's `color: start.color,`.
- `Start` literals set `pinned: false,` next to `color: None,` at `session_spawn`, `task_run` (server.rs) and `server/run.rs` (`run_start`); `server/resume.rs` `resume`: `pinned: last.pinned,` next to `color: last.color.clone(),`.

Run: `cargo test -p mkrew-daemon --test daemon pin -j 4` and `cargo test -p mkrew-daemon --test daemon restart_resumes_free_agent_panes -j 4`
Expected: PASS (needs Step 7's literals first: do Step 7, then run these).

- [ ] **Step 7: Every literal**

Add the new field wherever a struct literal is spelled out (compile errors `missing field pinned` name any this list misses; other tasks may have added more sites):
- `pinned: None,` in every `ProjectUpdateParams { … }`: `crates/mkrew-cli/src/project.rs:58`; `crates/mkrew-ui/src/run_setup.rs:196` (`begin`); `crates/mkrew-ui/src/workspace_menu.rs` (`set_default_agent`, `set_default_shell`, `set_color`); `crates/mkrew-ui/tests/connect.rs:91`; `crates/mkrew-ui/tests/lifecycle.rs:126`; `crates/mkrew-daemon/tests/daemon.rs` (`set_default_agent`, `set_default_shell`, `set_runs`, the two literals at the project.update default-shell test, the `color` closure in `workspace_colors_are_set_cleared_and_checked`).
- `pinned: false,` in every `Session { … }`: `crates/mkrew-core/src/run.rs` (test `shell`), `crates/mkrew-daemon/src/awake.rs`, `notify.rs`, `turns.rs` (tests), `server/resume.rs` (test `session`), `crates/mkrew-daemon/tests/daemon.rs` (`insert_session` literal), `crates/mkrew-ui/src/{board,broadcast,conflicts,doc_dialog,fanout,git_view,history,lifecycle,palette,sidebar,status,tabs,turns_dialog,undo_dialog,usage,worktree_dialog}.rs` (test helpers `session()`).
- `pinned: false,` in every `Project { … }`: `crates/mkrew-ui/src/palette.rs` and `sidebar.rs` (test helpers `project()`).
- `pinned: false,` in every `TerminalTab { … }` that is spelled out: `crates/mkrew-ui/src/tabs.rs` tests (`wire()` and the `echo` literal in the "echo of the write keeps the local layout" test).

Run: `cargo check --workspace --all-targets -j 4`
Expected: no errors. Then `cargo test -p mkrew-daemon --test daemon pin -j 4` and `cargo test -p mkrew-daemon --test daemon restart_resumes_free_agent_panes -j 4`: PASS.

- [ ] **Step 8: SPEC §5 and §7**

§5, after the `0014_project_default_shell.sql` sentence (SPEC.md:319-320) add:

```markdown
`0016_pinned.sql` adds `pinned INTEGER NOT NULL DEFAULT 0` to `project`, `session` and
`terminal_tab` (pinned workspaces, sessions and tabs lead their lists, §10).
```

§7:
- `project.list/add/update/archive` row: change `` `color` (update only), `` to `` `color`, `pinned` (update only), `` and, in the same row's result cell after `` `null` = cleared; ``, add `` `pinned` is a bool; ``.
- After the `session.rename` row (SPEC.md:520) add:

```markdown
| `session.pin` | `session_id`, `pinned` | the Session, also broadcast as `session.updated`; a live session through its task, an exited one in the store (`invalid_state` for one caught exiting); a resumed session keeps the pin (ADR 0017) |
```

- After the `tab.list/create/rename/close/move` row (SPEC.md:522) add:

```markdown
| `tab.pin` | `tab_id`, `pinned` | TerminalTab, also broadcast as `tab.updated`; `position` is left alone, so unpinning puts the tab back |
```

- `session.updated` event row (SPEC.md:559): replace `a session's title changed (`session.rename`)` with `a session's title or pin changed (`session.rename`, `session.pin`)` (keep any request Task 12 already added to the parentheses).

No HUMAN_CHECKS (nothing visible yet; Task 14 adds them).

- [ ] **Step 9: Verify**

Run the full verification. Do not tick inbox line 80 yet (Task 14).
Suggested message: `feat(protocol): pinned flag on workspaces, sessions and tabs (session.pin, tab.pin)`

---

### Task 14: Pin: menus, ordering, pin icon

Inbox: line 80 `Pin feature`. Spec: §10 (:697-698), §11 (:775-777).

**Current state (investigated):**
- Only the workspace row has a context menu: right-click → `Shell::open_workspace_menu` → `render_workspace_menu` (main.rs, `menu_item(id, theme)` rows; closes through `update_project`). `Shell::update_project(params, failure, window, cx)` sends `project.update` and shows a failure in the sidebar.
- Session rows (`session_row`, main.rs) have only a click (`open_session`), a hover peek and the hover ✕. Task 12 added `Shell::open_pane_menu(session_id, position, cx)` / `render_pane_menu` (items Rename, Change color) for the pane header; this task opens the same menu from the sidebar row and adds Pin / Unpin to it. Task 10 added `TabMenu` / `open_tab_menu` / `render_tab_menu` (Split right, Split down, Rename, Close); Pin / Unpin goes in there.
- Order today: `Sidebar::rows` (sidebar.rs) walks `self.projects` in daemon order and sorts a workspace's sessions `Reverse((started_at, id))`; `Sidebar::load` / `upsert_project` keep the list as received (`ProjectUpdated` replaces in place). `Tabs::sort` (tabs.rs) sorts by `(position, id)`; `Tabs::apply` copies only `title` and `position` from `tab.updated`. `Tabs::activate_offset` and `Sidebar::workspace_at_offset` walk the sorted `Vec`, so keyboard order follows the visual order once the `Vec` is sorted.
- Replies are not needed to show a pin: `project.updated`, `session.updated` and `tab.updated` events reach the sidebar (`Sidebar::apply`) and the tabs (`Tabs::apply`) and re-sort them.
- Glyphs are plain text (`▶`, `⋯`, `✕`, `◐`); there is no SVG icon system. A pin glyph `📌` (U+1F4CC) is an emoji: GPUI's Windows text system draws it as a color glyph through DirectWrite's system font fallback (Segoe UI Emoji): `gpui-pre-windows-0.3.7/src/direct_write.rs` `is_color_glyph` (:1913) and `rasterize_color` (:796). It does not take `text_color`. That is acceptable for a small mark; HUMAN_CHECKS confirms it reads well in both themes.

**Files:**
- Modify: `crates/mkrew-ui/src/sidebar.rs` (`WorkspaceRow`, `SessionRow`, `rows`, `load`, `upsert_project`, tests)
- Modify: `crates/mkrew-ui/src/tabs.rs` (`Tab`, `Tab::from_wire`, `Tabs::apply`, `Tabs::sort`, tests)
- Modify: `crates/mkrew-ui/src/workspace_menu.rs` (`set_pinned`, test)
- Modify: `crates/mkrew-ui/src/theme.rs` (`PIN_GLYPH`)
- Modify: `crates/mkrew-ui/src/main.rs` (`pin_mark`, `workspace_row`, `session_row`, `render_tab_bar`, `render_workspace_menu`, `render_pane_menu`, `render_tab_menu`, `pin_workspace`, `pin_session`, `pin_tab`, `send_pin`, imports)
- Modify: `docs/SPEC.md` §10, §11; `docs/HUMAN_CHECKS.md`; `docs/INBOX.md:80`
- No ADR: pins add a capability the SPEC does not forbid and change no decision (SPEC update only).

**Interfaces:**
- Consumes (Task 13): `Project.pinned`, `Session.pinned`, `TerminalTab.pinned`, `ProjectUpdateParams.pinned: Option<bool>`, `Request::SessionPin(SessionPinParams { session_id, pinned })`, `Request::TabPin(TabPinParams { tab_id, pinned })`.
- Consumes (Task 12): `PaneMenu { session_id: Ulid, position: Point<Pixels> }`, `Shell::open_pane_menu(&mut self, session_id: Ulid, position: Point<Pixels>, cx: &mut Context<'_, Self>)`, `Shell::render_pane_menu(…)`.
- Consumes (Task 10): `TabMenu { tab_id: Ulid, position: Point<Pixels> }`, `Shell::render_tab_menu(…)`.
- Produces: `sidebar::WorkspaceRow.pinned: bool`, `sidebar::SessionRow.pinned: bool`, `tabs::Tab.pinned: bool`, `workspace_menu::set_pinned(project_id: Ulid, pinned: bool) -> ProjectUpdateParams`, `theme::PIN_GLYPH: &str`, `Shell::pin_workspace / pin_session / pin_tab(…, pinned: bool, window, cx)`.

- [ ] **Step 1: Failing tests (pure ordering)**

`sidebar.rs` `mod tests`, after `workspaces_cycle_in_sidebar_order`:

```rust
    /// Pinned workspaces lead (SPEC §10), each group in its usual (age) order; unpinning puts
    /// one back where its age says, not at the head of the unpinned ones. Ctrl+Tab between
    /// workspaces follows the same order.
    #[test]
    fn pinned_workspaces_lead_and_unpinning_restores_the_old_order() {
        let (a, b, c) = (
            Project {
                created_at: 1,
                ..project("a")
            },
            Project {
                created_at: 2,
                ..project("b")
            },
            Project {
                created_at: 3,
                ..project("c")
            },
        );
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![a.clone(), b.clone(), c.clone()], Vec::new());
        let names = |sidebar: &Sidebar| -> Vec<String> {
            sidebar
                .rows()
                .iter()
                .filter_map(|row| match row {
                    Row::Workspace(row) => Some(row.name.clone()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(names(&sidebar), ["a", "b", "c"]);

        sidebar.apply(&Event::ProjectUpdated(Project {
            pinned: true,
            ..c.clone()
        }));
        sidebar.apply(&Event::ProjectUpdated(Project {
            pinned: true,
            ..b.clone()
        }));
        assert_eq!(names(&sidebar), ["b", "c", "a"]);
        assert!(workspace(&sidebar.rows()[0]).pinned);
        assert_eq!(sidebar.workspace_at_offset(Some(c.id), 1), Some(a.id));

        sidebar.apply(&Event::ProjectUpdated(Project {
            pinned: false,
            ..b.clone()
        }));
        assert_eq!(names(&sidebar), ["c", "a", "b"]);

        // A fresh list puts pinned workspaces first too.
        let mut reloaded = Sidebar::default();
        reloaded.load(
            vec![a, Project { pinned: true, ..c }],
            Vec::new(),
        );
        assert_eq!(names(&reloaded), ["c", "a"]);
    }

    /// A pinned session stays above newer ones; within each group it is newest first.
    #[test]
    fn pinned_sessions_lead_their_workspace_newest_first() {
        let api = project("api");
        let old = session(&api, 1, SessionState::Idle);
        let mid = session(&api, 2, SessionState::Working);
        let new = session(&api, 3, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api], vec![old.clone(), mid.clone(), new.clone()]);
        assert_eq!(session_ids(&sidebar.rows()), [new.id, mid.id, old.id]);

        sidebar.apply(&Event::SessionUpdated(Session {
            pinned: true,
            ..old.clone()
        }));
        assert_eq!(session_ids(&sidebar.rows()), [old.id, new.id, mid.id]);
        sidebar.apply(&Event::SessionUpdated(Session {
            pinned: true,
            ..mid.clone()
        }));
        assert_eq!(session_ids(&sidebar.rows()), [mid.id, old.id, new.id]);
        let rows = sidebar.rows();
        assert!(matches!(&rows[1], Row::Session(row) if row.pinned));
        assert!(matches!(&rows[3], Row::Session(row) if !row.pinned));
    }
```

`tabs.rs` `mod tests`, after `updates_add_rename_and_reorder_tabs`:

```rust
    /// Pinned tabs sit left of every unpinned one, each group in position order. Pinning
    /// never changes `position`, so unpinning puts the tab back at its own place. The active
    /// tab follows its id when the order changes.
    #[test]
    fn pinned_tabs_lead_and_unpinning_returns_to_position() {
        let project = Ulid::new();
        let [one, two, three] = [
            wire(project, "one", 1.0, serde_json::Value::Null),
            wire(project, "two", 2.0, serde_json::Value::Null),
            wire(project, "three", 3.0, serde_json::Value::Null),
        ];
        let mut tabs = Tabs::new(project);
        tabs.load(vec![one.clone(), two.clone(), three.clone()]);
        let titles = |tabs: &Tabs| -> Vec<String> {
            tabs.tabs().iter().map(|t| t.title.clone()).collect()
        };

        assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
            pinned: true,
            ..three.clone()
        })));
        assert_eq!(titles(&tabs), ["three", "one", "two"]);
        assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
            pinned: true,
            ..two.clone()
        })));
        assert_eq!(titles(&tabs), ["two", "three", "one"]);
        assert!(tabs.tabs()[0].pinned && !tabs.tabs()[2].pinned);
        assert_eq!(tabs.active().map(|t| t.id), Some(one.id));

        assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
            pinned: false,
            ..three.clone()
        })));
        assert_eq!(titles(&tabs), ["two", "one", "three"]);

        // A fresh list puts pinned tabs first too.
        let mut reloaded = Tabs::new(project);
        reloaded.load(vec![one, two, TerminalTab { pinned: true, ..three }]);
        assert_eq!(titles(&reloaded), ["three", "one", "two"]);
    }
```

`workspace_menu.rs` `mod tests`, after `choosing_an_agent_sets_only_the_default_agent`:

```rust
    #[test]
    fn pinning_sets_only_the_pin() {
        let id = Ulid::new();
        assert_eq!(
            serde_json::to_value(set_pinned(id, true)).unwrap(),
            serde_json::json!({"project_id": id, "pinned": true})
        );
    }
```

Run: `cargo test -p mkrew-ui --lib pinned -j 4`
Expected: FAIL to compile (`no field pinned on WorkspaceRow` / `Tab`, no `set_pinned`). If rustc crashes on the mkrew-ui test build (STATUS_STACK_BUFFER_OVERRUN), see the verification fallback in the plan header.

- [ ] **Step 2: Sidebar order and row flags**

`sidebar.rs`: `WorkspaceRow`, after `color`:

```rust
    /// Pinned workspaces lead the list (SPEC §10).
    pub pinned: bool,
```

`SessionRow`, after `state_changed_at`:

```rust
    /// Pinned sessions lead their workspace's list (SPEC §10).
    pub pinned: bool,
```

`rows()`: replace the sort line

```rust
            sessions.sort_by_key(|session| std::cmp::Reverse((session.started_at, session.id)));
```

with

```rust
            // Pinned first, then newest first.
            sessions.sort_by_key(|session| {
                (
                    !session.pinned,
                    std::cmp::Reverse((session.started_at, session.id)),
                )
            });
```

and in the `WorkspaceRow { … }` literal add `pinned: project.pinned,` after `color`; in the `SessionRow { … }` literal add `pinned: session.pinned,` after `state_changed_at`; update the doc line above `rows` to `/// Rows to show: each workspace (pinned first), then (if expanded) its live sessions (pinned first, then newest first).`

Workspaces: add after `upsert_project`:

```rust
    /// Pinned workspaces first. Within a group the daemon's order is by age (`created_at`), so
    /// an unpinned workspace returns to its own place; the sort is stable for equal ages.
    fn sort_projects(&mut self) {
        self.projects.sort_by_key(|p| (!p.pinned, p.created_at));
    }
```

call `self.sort_projects();` at the end of `load` (right after `self.projects = …filter…;`, before the replay of `since_load`) and as the last statement of `upsert_project` (after the `match`). Update the field doc `/// In `project.list` order; archived projects are left out.` to `/// Pinned first, then in `project.list` (age) order; archived projects are left out.`

Run: `cargo test -p mkrew-ui --lib sidebar -j 4`
Expected: the two new sidebar tests PASS together with the existing ones (`workspaces_cycle_in_sidebar_order`, `added_and_updated_workspaces_show_up_and_archived_ones_leave`: their test projects share `created_at: 0`, so the stable sort keeps their order).

- [ ] **Step 3: Tabs order**

`tabs.rs`: `Tab`, after `position`:

```rust
    /// Pinned tabs lead the strip (SPEC §10).
    pub pinned: bool,
```

`Tab::from_wire`: add `pinned: tab.pinned,` after `position: tab.position,`.

`Tabs::apply`: after `tab.position = incoming.position;` add `tab.pinned = incoming.pinned;`.

Replace `Tabs::sort`:

```rust
    /// Pinned tabs first, then by position (pinning leaves `position` alone, so unpinning
    /// puts a tab back).
    fn sort(&mut self) {
        self.tabs.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| a.position.total_cmp(&b.position))
                .then_with(|| a.id.cmp(&b.id))
        });
    }
```

and the field doc `/// Ordered by position.` on `Tabs::tabs` becomes `/// Pinned first, then ordered by position.`

Run: `cargo test -p mkrew-ui --lib tabs -j 4`
Expected: PASS (the new test and the existing `load_orders_by_position_and_activates_the_first_tab`).

- [ ] **Step 4: Menu builder, glyph, pin mark**

`workspace_menu.rs`, after `set_color`:

```rust
/// `project.update` pinning or unpinning the workspace.
pub fn set_pinned(project_id: Ulid, pinned: bool) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: Some(pinned),
    }
}
```

Run: `cargo test -p mkrew-ui --lib workspace_menu -j 4`
Expected: PASS.

`theme.rs`, after `mode_glyph`:

```rust
/// Marks a pinned workspace, session row or tab (SPEC §10). Plain text like the other glyphs:
/// on Windows GPUI draws it as a color emoji through DirectWrite's font fallback (Segoe UI
/// Emoji), so it ignores `text_color`.
pub const PIN_GLYPH: &str = "📌";
```

`main.rs`, near `workspace_avatar`:

```rust
/// The pin before the name of a pinned workspace, session or tab.
fn pin_mark() -> gpui::Div {
    div()
        .flex_none()
        .text_size(font_size::META)
        .child(theme::PIN_GLYPH)
}
```

- [ ] **Step 5: Marks on the three lists**

`workspace_row`: right after the name child (`.child(div().flex_1()… .child(row.name))`), before `.when_some(run_tip, …)`:

```rust
        .when(row.pinned, |el| el.child(pin_mark()))
```

`session_row`: right after the name child (`… .child(name),`), before the `session-status-` child:

```rust
        .when(row.pinned, |el| el.child(pin_mark()))
```

`render_tab_bar`, in the tab chip, right before `.child(tab.title.clone())` (after Task 10's type icon and status dot):

```rust
                    .when(tab.pinned, |el| el.child(pin_mark()))
```

- [ ] **Step 6: Pin actions**

`main.rs` imports: add `SessionPinParams` and `TabPinParams` to the `mkrew_protocol::{…}` list.

After `set_workspace_color` (before `update_project`):

```rust
    /// "Pin" / "Unpin" in the workspace menu: `project.update` with `pinned`; the
    /// `project.updated` event re-sorts the sidebar.
    fn pin_workspace(
        &mut self,
        project_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_pinned(project_id, pinned);
        self.update_project(params, "Cannot pin the workspace", window, cx);
    }
```

After `send_rename`:

```rust
    /// "Pin" / "Unpin" of a session (sidebar row or pane menu): `session.pin`.
    fn pin_session(
        &mut self,
        session_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.pane_menu = None;
        let params = SessionPinParams { session_id, pinned };
        self.send_pin(Request::SessionPin(params), "pane", window, cx);
    }

    /// "Pin" / "Unpin" of a tab (tab menu): `tab.pin`.
    fn pin_tab(
        &mut self,
        tab_id: Ulid,
        pinned: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.tab_menu = None;
        let params = TabPinParams { tab_id, pinned };
        self.send_pin(Request::TabPin(params), "tab", window, cx);
    }

    /// Sends `session.pin` / `tab.pin`. The `session.updated` / `tab.updated` event that
    /// follows moves the item; a failure goes to the tab bar's error line (`what` names it).
    fn send_pin(
        &mut self,
        request: Request,
        what: &'static str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.tab_error = None;
        let pinned = connection.call::<serde_json::Value>(request);
        cx.spawn_in(window, async move |this, cx| {
            let pinned = pinned.await;
            let _ = this.update(cx, |shell, cx| {
                if let Err(error) = pinned {
                    tracing::warn!("pinning the {what} failed: {error:#}");
                    shell.tab_error = Some(format!("Cannot pin the {what}: {error:#}"));
                }
                cx.notify();
            });
        })
        .detach();
    }
```

(`Shell.pane_menu` / `Shell.tab_menu` are the `Option<PaneMenu>` / `Option<TabMenu>` fields of Tasks 12 / 10; if they were named differently, `grep -n "Option<PaneMenu>\|Option<TabMenu>" crates/mkrew-ui/src/main.rs` and use those names.)

- [ ] **Step 7: Menu items**

`render_workspace_menu`: before `current` is built nothing changes; add next to `current_color`:

```rust
        let pinned = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .is_some_and(|project| project.pinned);
```

and make this the first row of the panel: insert before the `open-in-zed` item

```rust
            .child(
                menu_item("pin-workspace".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child(if pinned { "Unpin" } else { "Pin" }),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_workspace(project_id, !pinned, window, cx);
                    })),
            )
            .child(separator())
```

`render_pane_menu` (Task 12; the `&PaneMenu` argument is `menu`): compute

```rust
        let pinned = (self.sidebar.sessions().iter())
            .find(|session| session.id == menu.session_id)
            .is_some_and(|session| session.pinned);
        let session_id = menu.session_id;
```

and make this the first item of the panel, followed by the same separator the panel uses between its sections:

```rust
            .child(
                menu_item("pin-pane".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child(if pinned { "Unpin" } else { "Pin" }),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_session(session_id, !pinned, window, cx);
                    })),
            )
```

`render_tab_menu` (Task 10; argument `menu: &TabMenu`): compute

```rust
        let pinned = (self.tabs.values())
            .find_map(|tabs| tabs.tab(menu.tab_id))
            .is_some_and(|tab| tab.pinned);
        let tab_id = menu.tab_id;
```

and make this the first item of the panel (then a separator):

```rust
            .child(
                menu_item("pin-tab".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child(if pinned { "Unpin" } else { "Pin" }),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.pin_tab(tab_id, !pinned, window, cx);
                    })),
            )
```

- [ ] **Step 8: Right-click on a sidebar session row**

`session_row`, after the `.on_click(…open_session…)` call add:

```rust
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                // Show the pane first: the menu's Rename edits the pane header's title, which
                // exists only for the pane on screen (same as a left click).
                shell.open_session(session_id, window, cx);
                shell.open_pane_menu(session_id, event.position, cx);
            }),
        )
```

Where Task 12 renders `render_pane_menu`: if it is drawn inside the Terminals view only, move that one call to the root element of `Render for Shell` (next to the other overlays), so the menu is also drawn when the shown view is Board / Docs / Files / Git. It is `deferred(anchored().position(menu.position)…)`, so window coordinates work from anywhere.

Run: `cargo check -p mkrew-ui -j 4`
Expected: no errors.

- [ ] **Step 9: SPEC §10 and §11**

§10, after the **Color** bullet (SPEC.md:697-698) add:

```markdown
- **Pin:** the workspace menu, the pane menu (right-click a sidebar session row or a pane header)
  and the tab menu have Pin / Unpin (`project.update` `pinned`, `session.pin`, `tab.pin`, §7). A
  pinned workspace leads the sidebar, a pinned session leads its workspace's list and a pinned tab
  sits at the left of the tab strip; each group keeps its usual order (workspaces by age,
  sessions newest first, tabs by position), and unpinning puts the item back in it. A pinned item
  shows 📌 before its name. Pins are stored (§5), so they survive a restart; a resumed session
  keeps its pin (ADR 0017).
```

§11, after the **Workspace row** bullet (SPEC.md:775-777) add:

```markdown
- Pinned workspaces, sessions and tabs lead their lists and show 📌 before the name (§10 Pin);
  the order does not change a pane's status.
```

- [ ] **Step 10: HUMAN_CHECKS**

Append to the 2026-10-10 section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Pin workspace:** right-click a workspace row → Pin: it jumps to the top of the sidebar with
  📌 before its name; Unpin puts it back among the others by age. Restart the app and the daemon:
  the pin is still there. The next / previous workspace shortcuts follow the new order.
- [ ] **Pin session:** right-click a sidebar session row → the pane menu opens at the pointer with
  Pin, Rename, Change color; Pin moves the row above newer ones with 📌; Unpin returns it.
  Right-clicking a pane header shows the same Pin / Unpin. Stop and restart the daemon: the
  resumed pane is still pinned.
- [ ] **Pin tab:** right-click a tab (or its `⋯`) → Pin: the tab moves to the far left with 📌;
  Unpin returns it to its old place; Ctrl+Tab visits tabs in the shown order.
- [ ] **Pin icon:** 📌 is readable in the dark and light themes and at a small and a large
  `[ui] font_size`, sits on the same line as the name, and does not change any row or tab height.
```

- [ ] **Step 11: Verify and tick**

Run the full verification. Tick inbox line 80 `[~]` → `[x]`.
Suggested message: `feat(ui): pin workspaces, sessions and tabs from the right-click menus`

---

### Task 15: Right panel (session inspector) + Session card

Inbox: line 88 `Right panel (session inspector)`. Spec: §10 (:673-748), §13 (:912-980), §20.0 (:1346-1379), §20E.3 (:1586-1593). Do not tick yet (Task 16 ticks it).

**Current state (investigated):**
- There is no right column, no drag handle and no UI-side width setting. The only UI preference writer is `mkrew-core/src/settings.rs` `set_ui_theme` / `write_ui_theme` (the theme toggle, `Shell::toggle_theme`, main.rs ~:4435, its only caller). Pane dividers (`split.rs`) are keyboard-only, so a new handle is needed.
- `Shell::render_main` (main.rs ~:8458) puts `render_tab_bar` + `render_tab` in a flex column for `View::Terminals`. Panes use fractional rects of their parent (`render_tab`), so a narrower parent narrows them; `TerminalPane::fit` (terminal/view.rs:185) sends `session.resize` only when cols/rows change.
- `Shell::focused_session()` (:4169) is `None` outside Terminals; the sessions list is `self.sidebar.sessions()`; the latest usage is `self.usages.get(session)` (`usage::Usages`). The transcript path is never sent: it lives in the daemon's `Tracker.path` (`crates/mkrew-daemon/src/usage.rs` `Tracker`, `run`).
- Closing a pane with the close dialog offers Kill / Hide / Cancel (`close_pane` :7232), which is wrong for "Stop" (Stop must not hide the pane). `remove_session` (:7303) shows the right shape: `window.prompt(PromptLevel::Warning, …, &workspace_menu::REMOVE_ANSWERS, cx)` + `task_dialog::confirms_delete(answer)`; `Shell::kill` (:7281) is `session.kill`. `lifecycle::close_action` already decides "exited / shell / running agent".
- `zed::open(path, line)` (zed.rs:33) is the only open-a-file helper; errors go to `sidebar_error` (`open_in_zed`, :4385).
- GPUI 0.3.7 (`~/.cargo/registry/src/*/gpui-pre-0.3.7`): `CursorStyle::ResizeLeftRight` (platform.rs:2599); `InteractiveElement::on_drag_move::<T>` (elements/div.rs:360) is called on every mouse move, inside or outside the element, while a drag of payload type `T` that started on the element is active; `on_drag(value, |_, _, _, cx| cx.new(|_| EmptyView))` starts it (div.rs:615; `EmptyView` is exported from `view.rs`); `on_mouse_up` / `on_mouse_up_out` (div.rs:210, :283); `App::set_active_drag_cursor_style(style, window)` (app.rs:2714). `DragMoveEvent` is already imported in main.rs. The existing `on_drag` use (board `DraggedCard`, :16882) is the pattern.
- Keys: `ctrl-shift-y` is unused (keymap.rs `default_keys`, the only other text hit is a test string in palette.rs:712 that rebinds `new_shell_tab`, not a default). Task 3/6 use `ctrl-shift-b` for the sidebar.

**Design decisions (defaults; change here if the user objects):**
1. Transcript path: `transcript: Option<String>` on `SessionUsageEvent` (no new request, no migration: the event is stored as JSON in `session_event`, so `session.usage` replies after a reconnect carry it; old rows lack it and `#[serde(default)]` reads `None`). It is re-sent when the path changes even if the totals do not.
2. "Open transcript" opens the `.jsonl` in Zed (`zed::open`), like "Follow in Zed" / "Open in Zed" (SPEC §10); a `.jsonl` has no reliable OS handler. Failures show in `sidebar_error`.
3. Width: `[ui] inspector_width` (px, default 300) in `settings.toml`, written on mouse-up only. The UI clamps it to 240–560 (consts in `theme.rs`); core only parses a `u16` (core cannot see UI constants, and the template test needs the parsed default to equal the template value). The open/closed state is not remembered: closed at every start. It is per window, not per workspace.
4. The key outside Terminals: `ToggleInspector` shows Terminals and opens the panel (it never silently toggles state the user cannot see).
5. Stop: a running agent asks first ("Stop session" / "Cancel"); a live shell stops without asking (same rule as closing a shell pane, `close_action`); exited sessions have no Stop (ADR 0033 removes them anyway). Stop does not touch the pane: the pane closes by itself when the session exits (ADR 0033).
6. While dragging, the width changes live. Terminals reflow once per changed column count (`fit` skips unchanged sizes); the width is saved on mouse-up. ponytail: no live-resize throttle; add one if an agent TUI visibly flickers during a drag.

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`set_ui_value`, `write_ui_value` replace `set_ui_theme` / `write_ui_theme`; `UiSettings.inspector_width`; `INSPECTOR_WIDTH_DEFAULT`; template; tests)
- Modify: `crates/mkrew-protocol/src/message.rs` (`SessionUsageEvent.transcript`, round-trip tests), `crates/mkrew-protocol/src/lib.rs` (`PROTOCOL_VERSION`)
- Modify: `crates/mkrew-daemon/src/usage.rs` (`Tracker::path`, `run`, tests), `crates/mkrew-daemon/tests/daemon.rs` (`a_turn_end_reads_token_usage_from_the_agent_transcript`)
- Create: `crates/mkrew-ui/src/inspector.rs` (pure: width, stop rule, Session card); Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod inspector;` after `pub mod inbox;`)
- Modify: `crates/mkrew-ui/src/usage.rs` (test literals), `crates/mkrew-ui/src/theme.rs` (consts), `crates/mkrew-ui/src/keymap.rs`, `crates/mkrew-ui/src/palette.rs` (`ToggleInspector`)
- Modify: `crates/mkrew-ui/src/main.rs` (fields, `toggle_inspector`, `render_main`, `render_inspector`, `render_session_card`, `inspector_card`, `open_transcript`, `stop_session`, `end_inspector_drag`, status-bar button, `Shell::run`, `toggle_theme`)
- Modify: `docs/SPEC.md` (§10, §13, §20.0, §20E.3), `docs/HUMAN_CHECKS.md`
- Create: `docs/decisions/0038-session-panel.md`

**Interfaces:**
- Consumes: Task 4 `SessionUsageEvent { session_id, input, output, cache_read, cache_write, context, context_window, model }`; Task 6 `usage::fields(&SessionUsageEvent) -> UsageFields { context: Option<ContextShare>, tokens: Option<String>, cache: Option<String>, model: Option<String> }`, `Shell::render_status_bar` (its right end), `keymap::Command::ToggleSidebar`; Task 3 title bar (no `render_toolbar`; do not touch the root column except as written below); Task 2 `Shell::git_summary` (Task 16 only).
- Produces:
  - `mkrew_core::settings::set_ui_value(src: &str, key: &str, value: &str) -> String`, `write_ui_value(path: &Path, key: &str, value: &str) -> Result<(), SettingsError>` (value is a TOML literal: `"split"` with quotes, or `320`); `UiSettings.inspector_width: u16`; `settings::INSPECTOR_WIDTH_DEFAULT: u16 = 300`.
  - `SessionUsageEvent.transcript: Option<String>`; `PROTOCOL_VERSION = 36`.
  - `inspector::{saved_width, width_from_pointer, StopAction, stop_action, STOP_ANSWERS, SessionCard, session_card, usage_rows}`; `theme::{INSPECTOR_MIN_WIDTH, INSPECTOR_MAX_WIDTH, INSPECTOR_GRIP_WIDTH}`.
  - `keymap::Command::ToggleInspector` (`toggle_inspector`, `ctrl-shift-y`, context `None`).
  - `Shell.inspector_open`, `Shell.inspector_width`, `Shell::toggle_inspector`, `Shell::render_inspector`, and `fn inspector_card(theme: &Theme, title: &'static str) -> gpui::Div` (the card frame Tasks 16 and 18 reuse).

- [ ] **Step 1: Failing settings tests**

In `settings.rs` `mod tests`, add a helper next to `parse_err` and migrate the theme tests: replace every `set_ui_theme(X, ThemeMode::M)` in the existing tests (`set_ui_theme_keeps_comments_and_other_keys`, `set_ui_theme_inserts_ui_section`, `set_ui_theme_finds_a_commented_or_spaced_ui_header`, `set_ui_theme_keeps_crlf_line_endings`) by `set_theme(X, ThemeMode::M)`, and `write_ui_theme(&path, ThemeMode::M)` in `write_ui_theme_never_turns_a_good_file_bad` by `write_theme(&path, ThemeMode::M)`. Keep their assertions as they are.

```rust
    fn set_theme(src: &str, mode: ThemeMode) -> String {
        set_ui_value(src, "theme", &format!("\"{}\"", mode.name()))
    }

    fn write_theme(path: &Path, mode: ThemeMode) -> Result<(), SettingsError> {
        write_ui_value(path, "theme", &format!("\"{}\"", mode.name()))
    }
```

Add the new tests:

```rust
    #[test]
    fn inspector_width_parses_and_defaults_to_300() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert_eq!(ui("").inspector_width, INSPECTOR_WIDTH_DEFAULT);
        assert_eq!(INSPECTOR_WIDTH_DEFAULT, 300);
        assert_eq!(ui("[ui]\ninspector_width = 420").inspector_width, 420);
        // A negative or fractional width is a typed error, not a silent default.
        assert!(parse_err("[ui]\ninspector_width = -1").contains("invalid value"));
        assert!(parse_err("[ui]\ninspector_width = \"wide\"").contains("invalid type"));
    }

    #[test]
    fn set_ui_value_replaces_only_its_own_key() {
        // `theme_x` and the commented line are not `theme`; the other keys stay.
        let src = "[ui]\n# inspector_width = 300\ninspector_width_x = 1\ninspector_width = 300 # px\nfont_size = 15\n";
        let out = set_ui_value(src, "inspector_width", "420");
        assert_eq!(
            out,
            "[ui]\n# inspector_width = 300\ninspector_width_x = 1\ninspector_width = 420\nfont_size = 15\n"
        );
    }

    #[test]
    fn set_ui_value_adds_the_key_or_the_section_and_keeps_crlf() {
        assert_eq!(
            set_ui_value("[ui]\r\nfont_size = 15\r\n", "inspector_width", "320"),
            "[ui]\r\ninspector_width = 320\r\nfont_size = 15\r\n"
        );
        assert_eq!(
            set_ui_value("# mine\n[zed]\nfollow_debounce_ms = 1\n", "inspector_width", "320"),
            "# mine\n[zed]\nfollow_debounce_ms = 1\n\n[ui]\ninspector_width = 320\n"
        );
        let out = set_ui_value("[ui]\nfont_size = 15\n", "inspector_width", "320");
        let settings = parse(Path::new("s.toml"), &out).unwrap();
        assert_eq!((settings.ui.inspector_width, settings.ui.font_size), (320, FontSize(15)));
    }

    #[test]
    fn write_ui_value_saves_next_to_the_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        // Missing file: the template plus the value.
        write_ui_value(&path, "inspector_width", "400").unwrap();
        assert_eq!(load(&path).unwrap().ui.inspector_width, 400);
        write_theme(&path, ThemeMode::Dark).unwrap();
        write_ui_value(&path, "inspector_width", "350").unwrap();
        let ui = load(&path).unwrap().ui;
        assert_eq!((ui.inspector_width, ui.theme), (350, ThemeMode::Dark));
    }
```

Also update `full_file_parses`: add `inspector_width: INSPECTOR_WIDTH_DEFAULT,` to its `UiSettings { … }` literal.

Run: `cargo test -p mkrew-core --lib settings -j 4`
Expected: FAIL to compile (`set_ui_value`, `write_ui_value`, `inspector_width`, `INSPECTOR_WIDTH_DEFAULT` not defined).

- [ ] **Step 2: Implement `set_ui_value`, `write_ui_value`, `inspector_width`**

Replace `set_ui_theme` and `write_ui_theme` (settings.rs ~:148-210) by:

```rust
/// `src` with `[ui] <key> = <value>` (`value` is a TOML literal such as `"dark"` or `320`):
/// replaces an uncommented `<key> =` line inside `[ui]`, else inserts one right after the `[ui]`
/// header, else appends a `[ui]` section. Everything else (comments, order, other keys, CRLF
/// line ends) is kept byte for byte. Forms this does not recognise (`ui.theme = …`, an inline
/// `ui = {…}`) can yield invalid TOML: callers parse the result before writing it.
pub fn set_ui_value(src: &str, key: &str, value: &str) -> String {
    let eol = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let line = format!("{key} = {value}");
    // `[ui]`, `[ ui ]` and `[ui] # note` are the same table header.
    let is_ui_header = |l: &&str| {
        let code = l.split('#').next().unwrap_or_default();
        code.split_whitespace().collect::<String>() == "[ui]"
    };
    let mut lines: Vec<&str> = src.lines().collect();
    let Some(header) = lines.iter().position(is_ui_header) else {
        let sep = if src.is_empty() || src.ends_with(&format!("{eol}{eol}")) {
            ""
        } else if src.ends_with('\n') {
            eol
        } else {
            "\n\n"
        };
        return format!("{src}{sep}[ui]{eol}{line}{eol}");
    };
    let end = lines[header + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| header + 1 + i);
    // `key` followed by `=`: `theme_x = …` is another key.
    let is_key = |l: &&str| {
        l.trim_start()
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    match lines[header + 1..end].iter().position(is_key) {
        Some(i) => lines[header + 1 + i] = &line,
        None => lines.insert(header + 1, &line),
    }
    let mut out = lines.join(eol);
    if src.ends_with('\n') {
        out.push_str(eol);
    }
    out
}

/// Saves `[ui] <key> = <value>` in `path` (the template when it is missing) with
/// [`set_ui_value`]. Writes nothing when the file does not parse or the edit would not parse
/// (an unusual `ui` form, a value of the wrong type), so a good file never turns bad.
pub fn write_ui_value(path: &Path, key: &str, value: &str) -> Result<(), SettingsError> {
    let io_error = |e: io::Error| SettingsError {
        file: path.to_path_buf(),
        line_col: None,
        message: e.to_string(),
    };
    let src = match std::fs::read_to_string(path) {
        Ok(src) => src,
        Err(e) if e.kind() == io::ErrorKind::NotFound => template().to_owned(),
        Err(e) => return Err(io_error(e)),
    };
    parse(path, &src)?;
    let out = set_ui_value(&src, key, value);
    parse(path, &out)?;
    std::fs::write(path, out).map_err(io_error)
}
```

`UiSettings`: add the field, its default, the const, and the template lines.

```rust
/// `[ui] inspector_width` default, px (the UI clamps what it reads to its own limits).
pub const INSPECTOR_WIDTH_DEFAULT: u16 = 300;
```

```rust
pub struct UiSettings {
    pub font_family: String,
    pub font_size: FontSize,
    pub theme: ThemeMode,
    /// Width of the session panel right of Terminals, px (SPEC §10); dragging its edge saves it.
    pub inspector_width: u16,
}
```

In `impl Default for UiSettings` add `inspector_width: INSPECTOR_WIDTH_DEFAULT,`. In `template()` replace the two comment lines above `[ui]` and its keys with:

```rust
     # App font: an installed family name (empty: the system UI font); size 8–32 px.\n\
     # Theme: system (follows the OS), light or dark; the toolbar toggle writes it.\n\
     # Session panel width in px (dragging its edge writes it; Mkrew keeps it 240–560).\n\
     [ui]\n\
     # font_family = \"\"\n\
     # font_size = 13\n\
     # theme = \"system\"\n\
     # inspector_width = 300\n\
```

Migrate the one caller, `Shell::toggle_theme` in main.rs:

```rust
        let value = format!("\"{}\"", mode.name());
        if let Err(error) = mkrew_core::settings::write_ui_value(&self.settings_file, "theme", &value)
        {
            self.settings_error = Some(error.to_string());
        }
```

(`ThemeMode::name` is `pub`; `mode` is `theme::ThemeMode`, the same type re-exported from core.) Run `grep -rn "set_ui_theme\|write_ui_theme" crates docs/SPEC.md` and expect no hit left except in old plan files under `docs/superpowers/plans/`, which stay as history.

Run: `cargo test -p mkrew-core --lib settings -j 4`
Expected: PASS (the migrated theme tests and the four new ones; `the_template_parses_to_the_defaults_commented_or_not` passes because the template line equals the default).

- [ ] **Step 3: Failing protocol and daemon tests**

`message.rs`, in the test that holds `Event::SessionUsage` (`event_wire_shape_matches_spec`, ~:2294) add `transcript: None,` to the existing literal (keep its `json!` assertion: `transcript` is absent on the wire when unset) and, right after the assertion, add:

```rust
        let with_path = round_trip(&Message::Event(Event::SessionUsage(SessionUsageEvent {
            transcript: Some("C:\\Users\\a\\.claude\\projects\\p\\s1.jsonl".into()),
            ..usage_event()
        })));
        assert_eq!(
            with_path["data"]["transcript"],
            "C:\\Users\\a\\.claude\\projects\\p\\s1.jsonl"
        );
        // A `session.usage` stored before the field existed still reads.
        let old: SessionUsageEvent = serde_json::from_value(
            json!({ "session_id": sid(), "input": 1, "output": 2, "context": 3 }),
        )
        .unwrap();
        assert_eq!(old.transcript, None);
```

To avoid repeating the literal, hoist the existing literal into a small helper at the end of that test module and use it for both:

```rust
    fn usage_event() -> SessionUsageEvent {
        SessionUsageEvent {
            session_id: sid(),
            input: 10,
            output: 2,
            cache_read: 0,
            cache_write: 0,
            context: 7,
            context_window: None,
            model: None,
            transcript: None,
        }
    }
```

and make the first round trip `Event::SessionUsage(usage_event())`.

`daemon/src/usage.rs` `mod tests`, after `each_read_takes_only_the_appended_bytes`:

```rust
    #[test]
    fn the_tracker_reports_the_transcript_it_located() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("projects/p");
        std::fs::create_dir_all(&dir).unwrap();
        let mut tracker =
            Tracker::new(UsageFormat::ClaudeJsonl, Some(root.path().to_owned())).unwrap();
        assert_eq!(tracker.path(), None, "nothing located yet");
        std::fs::write(dir.join("s1.jsonl"), CLAUDE).unwrap();
        tracker.read("s1").unwrap();
        assert_eq!(tracker.path(), Some(dir.join("s1.jsonl").as_path()));
        // Another native session (e.g. after `/clear`) starts without a path.
        tracker.read("s2");
        assert_eq!(tracker.path(), None);
    }
```

`daemon/tests/daemon.rs` `a_turn_end_reads_token_usage_from_the_agent_transcript` (~:2067): in `expected` add `transcript: Some(transcripts.join("native-7.jsonl").to_string_lossy().into_owned()),` (plus whatever Task 4 fields the literal already lists). The `listed` assertion then also proves `session.usage` returns the path after the event.

Run: `cargo test -p mkrew-protocol -j 4` → FAIL to compile (`transcript`). Then (after Step 4's struct change) `cargo test -p mkrew-daemon --lib usage -j 4` → FAIL to compile (`Tracker::path`).

- [ ] **Step 4: Implement the protocol field and the daemon**

`message.rs` `SessionUsageEvent`, after `model` (Task 4):

```rust
    /// The agent's transcript file (an absolute path on the daemon's machine, which is the
    /// UI's: the socket is local, SPEC §3), once located. Sent again when it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
```

`protocol/src/lib.rs`: `PROTOCOL_VERSION: u32 = 36;` (was 35).

`daemon/src/usage.rs`: add to `impl Tracker`:

```rust
    /// The transcript located for the current native session, if any.
    fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
```

In `run`, add `let mut last_path: Option<String> = None;` next to `last`. After `tracker = back;` compute the path, extend the "changed" test of the existing `usage.filter(...)` line with `|| last_path != path` (keep its first condition exactly as Task 4 left it), and send it:

```rust
        tracker = back;
        // A located (or moved) transcript is news even when the totals did not change.
        let path = tracker.path().map(|p| p.to_string_lossy().into_owned());
```

```rust
        let Some(usage) = usage.filter(|u| last != Some(*u) || last_path != path) else {
            continue;
        };
        last = Some(usage);
        last_path = path.clone();
```

(If Task 4 made `Usage` non-`Copy`, keep whichever form it used for `last` and only add the `|| last_path != path` term.) In the `SessionUsageEvent { … }` literal add `transcript: path,`.

Run: `cargo test -p mkrew-protocol -j 4` → PASS. `cargo test -p mkrew-daemon --lib usage -j 4` → PASS. `cargo test -p mkrew-daemon --test daemon a_turn_end_reads_token_usage -j 4` → PASS. Add `transcript: None,` to every other `SessionUsageEvent { … }` literal the compiler flags: `crates/mkrew-ui/src/usage.rs` tests (the `usage()` helper ~:174; spreads with `..usage(…)` need nothing).

- [ ] **Step 5: Failing pure tests for the panel**

Create `crates/mkrew-ui/src/inspector.rs` with only the test module first (and `pub mod inspector;` in `lib.rs`), so the compile error is the expected one:

```rust
//! The session panel right of Terminals (SPEC §10, ADR 0038): its width, the Stop rule and
//! the text of the Session card. Pure; `main.rs` draws it.

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, SessionState, StateSource};
    use ulid::Ulid;

    use super::*;
    use crate::usage::ContextShare;

    fn session(kind: SessionKind, state: SessionState) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "claude".into(),
            kind,
            title: None,
            terminal_title: None,
            native_session_id: None,
            state,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    #[test]
    fn the_saved_width_is_kept_between_the_limits() {
        assert_eq!(saved_width(300), 300.0);
        assert_eq!(saved_width(0), theme::INSPECTOR_MIN_WIDTH);
        assert_eq!(saved_width(65_535), theme::INSPECTOR_MAX_WIDTH);
    }

    #[test]
    fn dragging_the_edge_sets_the_width_from_the_pointer() {
        // The panel is flush with the window's right edge: width = window − pointer.
        assert_eq!(width_from_pointer(1200.0, 900.0), 300.0);
        assert_eq!(width_from_pointer(1200.0, 1190.0), theme::INSPECTOR_MIN_WIDTH);
        assert_eq!(width_from_pointer(1200.0, 100.0), theme::INSPECTOR_MAX_WIDTH);
        // Never more than half of a narrow window (but never below the minimum).
        assert_eq!(width_from_pointer(700.0, 0.0), 350.0);
        assert_eq!(width_from_pointer(400.0, 0.0), theme::INSPECTOR_MIN_WIDTH);
    }

    #[test]
    fn only_a_running_agent_asks_before_stopping() {
        let agent = session(SessionKind::Agent, SessionState::NeedsInput);
        let shell = session(SessionKind::Shell, SessionState::Idle);
        let exited = session(SessionKind::Agent, SessionState::Exited);
        assert_eq!(stop_action(Some(&agent)), StopAction::Ask);
        assert_eq!(stop_action(Some(&shell)), StopAction::Kill);
        assert_eq!(stop_action(Some(&exited)), StopAction::Nothing);
        // Just spawned, not listed yet: ask, like closing its pane.
        assert_eq!(stop_action(None), StopAction::Ask);
    }

    #[test]
    fn the_card_names_the_session_and_its_state() {
        let mut working = session(SessionKind::Agent, SessionState::Working);
        working.state_changed_at = 1_000;
        let card = session_card(&working, None, 121_000);
        assert_eq!(card.agent, "claude");
        assert_eq!(card.title, "Untitled", "no title, no terminal title");
        assert_eq!(card.state, "working · 2m");
        assert!(card.usage.is_empty() && card.transcript.is_none());
        working.terminal_title = Some("Fix login".into());
        assert_eq!(session_card(&working, None, 0).title, "Fix login");
        working.title = Some("Mine".into());
        assert_eq!(session_card(&working, None, 0).title, "Mine");
    }

    #[test]
    fn usage_rows_skip_what_has_no_data() {
        let fields = UsageFields {
            context: Some(ContextShare {
                label: "ctx 62%".into(),
                warning: false,
                hover: String::new(),
            }),
            tokens: Some("12.3k tok".into()),
            cache: None,
            model: Some("opus-4".into()),
        };
        assert_eq!(
            usage_rows(fields),
            [
                ("Context", "ctx 62%".to_owned()),
                ("Tokens", "12.3k tok".to_owned()),
                ("Model", "opus-4".to_owned())
            ]
        );
        assert!(usage_rows(UsageFields::default()).is_empty());
    }

    #[test]
    fn the_card_carries_the_transcript_of_its_usage() {
        let working = session(SessionKind::Agent, SessionState::Working);
        let event = SessionUsageEvent {
            session_id: working.id,
            input: 1,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            context: 1,
            context_window: None,
            model: None,
            transcript: Some("/t/s1.jsonl".into()),
        };
        let card = session_card(&working, Some(&event), 0);
        assert_eq!(card.transcript.as_deref(), Some("/t/s1.jsonl"));
        assert!(!card.usage.is_empty(), "tokens at least");
    }
}
```

Run: `cargo test -p mkrew-ui --lib inspector -j 4`
Expected: FAIL to compile (items not defined). If `UsageFields` does not derive `Default`, add `#[derive(Default)]` to it in `usage.rs` in this step (Task 6 defines it; the contract says "a field with no data is None").

- [ ] **Step 6: Implement the pure module and the theme consts**

`theme.rs`, next to `SIDEBAR_WIDTH` (:119):

```rust
/// Narrowest and widest the session panel gets when dragged (SPEC §10); `[ui] inspector_width`
/// is clamped to them.
pub const INSPECTOR_MIN_WIDTH: f32 = 240.0;
pub const INSPECTOR_MAX_WIDTH: f32 = 560.0;

/// Width of the grab area on the session panel's left edge.
pub const INSPECTOR_GRIP_WIDTH: f32 = 6.0;
```

`inspector.rs` above the tests:

```rust
use mkrew_protocol::{Session, SessionUsageEvent};

use crate::lifecycle::{self, CloseAction};
use crate::usage::{self, UsageFields};
use crate::{history, status, theme};

/// `[ui] inspector_width` as drawn: kept between the limits.
pub fn saved_width(px: u16) -> f32 {
    f32::from(px).clamp(theme::INSPECTOR_MIN_WIDTH, theme::INSPECTOR_MAX_WIDTH)
}

/// The width while the edge is dragged to `pointer_x` (window coordinates): the panel is flush
/// with the right edge. At most half of the window, so the terminals keep room.
pub fn width_from_pointer(window_width: f32, pointer_x: f32) -> f32 {
    let max = theme::INSPECTOR_MAX_WIDTH
        .min(window_width / 2.0)
        .max(theme::INSPECTOR_MIN_WIDTH);
    (window_width - pointer_x).clamp(theme::INSPECTOR_MIN_WIDTH, max)
}

/// What the card's Stop does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopAction {
    /// The session has exited: no Stop.
    Nothing,
    /// A live shell: stop it without asking.
    Kill,
    /// A running agent (or a session not listed yet): ask first.
    Ask,
}

/// Stop follows closing a pane (`lifecycle::close_action`), minus closing the pane: it closes
/// itself when the session exits (ADR 0033).
pub fn stop_action(session: Option<&Session>) -> StopAction {
    match lifecycle::close_action(session) {
        CloseAction::Remove => StopAction::Nothing,
        CloseAction::Kill => StopAction::Kill,
        CloseAction::Ask => StopAction::Ask,
    }
}

/// Buttons of the Stop confirmation; index 0 stops ([`crate::task_dialog::confirms_delete`]).
pub const STOP_ANSWERS: [&str; 2] = ["Stop session", "Cancel"];

/// The text of the Session card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCard {
    /// The agent profile id (the badge is drawn from the profile's icon).
    pub agent: String,
    pub title: String,
    /// `working · 2m`, `needs input`, `exited (1)`.
    pub state: String,
    /// `(label, value)` rows of the usage the agent reported.
    pub usage: Vec<(&'static str, String)>,
    /// The agent's transcript file, once the daemon located it.
    pub transcript: Option<String>,
}

pub fn session_card(
    session: &Session,
    usage: Option<&SessionUsageEvent>,
    now_ms: i64,
) -> SessionCard {
    SessionCard {
        agent: session.agent_profile.clone(),
        title: history::title(session).unwrap_or("Untitled").to_owned(),
        state: status::text(
            session.state,
            session.exit_code,
            session.state_changed_at,
            now_ms,
        ),
        usage: usage.map(usage::fields).map(usage_rows).unwrap_or_default(),
        transcript: usage.and_then(|u| u.transcript.clone()),
    }
}

/// The usage rows that have data, in card order.
pub fn usage_rows(fields: UsageFields) -> Vec<(&'static str, String)> {
    [
        ("Context", fields.context.map(|share| share.label)),
        ("Tokens", fields.tokens),
        ("Cache", fields.cache),
        ("Model", fields.model),
    ]
    .into_iter()
    .filter_map(|(label, value)| Some((label, value?)))
    .collect()
}
```

Run: `cargo test -p mkrew-ui --lib inspector -j 4`
Expected: PASS.

- [ ] **Step 7: Failing keymap / palette tests, then `ToggleInspector`**

`keymap.rs` defaults test (~:740, after the `f11` assertions) add:

```rust
        assert_eq!(bound("ctrl-shift-y"), Some(Command::ToggleInspector));
        assert_eq!(keymap.label(Command::ToggleInspector), "Ctrl+Shift+Y");
        // Global: also while a terminal has focus (it must not reach the agent).
        assert_eq!(Command::ToggleInspector.context(), None);
```

`palette.rs` `availability_follows_the_view` (~:724): add `Command::ToggleInspector,` to the first `for command in [ … ]` list (available with a workspace in Terminals) and, in the `git` block, `assert!(git.contains(&Command::ToggleInspector), "shows the terminals");`. The `none` (no workspace) assertion list stays as it is: the command needs a workspace.

Run: `cargo test -p mkrew-ui --lib keymap palette -j 4` → FAIL to compile (`Command::ToggleInspector`).

Wire it exactly like `ToggleFullScreen` (find each by name): `keymap.rs` enum variant after the last global one:

```rust
    /// Show or hide the session panel right of Terminals (SPEC §10); from another view it
    /// shows Terminals with the panel open.
    ToggleInspector,
```

`all()`: `Command::ToggleInspector,` as the last entry of the final `all.extend([…])` list. `name()`: `Command::ToggleInspector => "toggle_inspector".into(),`. `default_keys()`: `Command::ToggleInspector => "ctrl-shift-y".into(),`. `context()` needs no change (`_ => None`). `palette.rs` `title()`: `Command::ToggleInspector => "View: Toggle session panel".into(),`; `available()`: add `| Command::ToggleInspector` to the `=> context.workspace` arm (the one with `NewShellTab … NewTask`).

Run: `cargo test -p mkrew-ui --lib keymap palette -j 4`
Expected: PASS (including the template and every-command-has-a-unique-default tests).

- [ ] **Step 8: Shell state, toggle, layout, drag**

Fields (near `views`; init in `Shell::new`):

```rust
    /// The session panel right of Terminals is open (closed at every start; SPEC §10).
    inspector_open: bool,
    /// Its width in px, from `[ui] inspector_width` and the drag.
    inspector_width: f32,
    /// The edge was dragged since the last mouse-up: the width is saved when it ends.
    inspector_dragging: bool,
```

`Shell::new`, before the struct literal (the other settings reads there ignore a broken file the same way):

```rust
        let inspector_width = inspector::saved_width(
            mkrew_core::settings::load(&settings_file)
                .unwrap_or_default()
                .ui
                .inspector_width,
        );
```

and in the literal `inspector_open: false, inspector_width, inspector_dragging: false,`. Import `use mkrew_ui::inspector;` with the other `mkrew_ui::…` imports, and `EmptyView` in the `gpui::{…}` list. Add the drag payload next to the other top-level marker types:

```rust
/// Payload of a drag of the session panel's left edge (a resize, nothing is dropped).
struct InspectorGrip;
```

Methods (near `toggle_theme`):

```rust
    /// `ToggleInspector` / the status-bar button: opens or closes the session panel; from
    /// another view it shows Terminals with the panel open (the panel lives there only).
    fn toggle_inspector(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.workspace.is_none() {
            return;
        }
        if self.view() != View::Terminals {
            self.inspector_open = true;
            self.show_view(View::Terminals, window, cx);
        } else {
            self.inspector_open = !self.inspector_open;
        }
        cx.notify();
    }

    /// The edge was released: saves the dragged width as `[ui] inspector_width` (one write per
    /// drag; the file is hand-edited config). A file that does not parse is not written; the
    /// palette lists why (§20.0).
    fn end_inspector_drag(&mut self) {
        if !std::mem::take(&mut self.inspector_dragging) {
            return;
        }
        // Clamped to 240–560 by `width_from_pointer`, so it fits a u16.
        let width = (self.inspector_width.round() as u16).to_string();
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "inspector_width", &width)
        {
            self.settings_error = Some(error.to_string());
        }
    }
```

`Shell::run`: `Command::ToggleInspector => self.toggle_inspector(window, cx),` next to `Command::ToggleTheme`.

`render_main`, Terminals arm (do not touch the lines Task 3 left above `match`): replace the `(View::Terminals, Some(tabs))` arm with

```rust
            (View::Terminals, Some(tabs)) => {
                let terminals = div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.render_tab_bar(project_id, tabs, now, theme, cx))
                    .child(self.render_tab(
                        tabs,
                        now,
                        self.composer_focus.is_focused(window),
                        theme,
                        cx,
                    ));
                main.child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .child(terminals)
                        .when(self.inspector_open, |el| {
                            el.child(self.render_inspector(now, theme, cx))
                        }),
                )
            }
```

(If the tab bar / `render_tab` signature differs after Tasks 10–11, keep their call arguments as they are and only wrap them.)

`render_inspector` and the card frame (near `render_tab_bar`):

```rust
    /// The session panel (SPEC §10): the focused pane's cards, right of Terminals. Rendered
    /// only while open; it is a sibling of the panes, never inside the terminal element.
    fn render_inspector(
        &self,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let grip = div()
            .id("inspector-grip")
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .w(px(theme::INSPECTOR_GRIP_WIDTH))
            .cursor(CursorStyle::ResizeLeftRight)
            .on_drag(InspectorGrip, |_, _, _, cx| cx.new(|_| EmptyView))
            .on_drag_move(cx.listener(
                |shell, event: &DragMoveEvent<InspectorGrip>, window, cx| {
                    cx.set_active_drag_cursor_style(CursorStyle::ResizeLeftRight, window);
                    // ponytail: live width, one `session.resize` per changed column count;
                    // throttle if an agent TUI flickers while dragging.
                    shell.inspector_width = inspector::width_from_pointer(
                        f32::from(window.viewport_size().width),
                        f32::from(event.event.position.x),
                    );
                    shell.inspector_dragging = true;
                    cx.notify();
                },
            ))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.end_inspector_drag()),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, _, _, _| shell.end_inspector_drag()),
            );
        let cards = div()
            .id("inspector-body")
            .size_full()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::M))
            .children(match session {
                Some(session) => vec![self.render_session_card(session, now, theme, cx)],
                None => vec![meta_line(theme, "No session focused".into()).into_any_element()],
            });
        div()
            .flex_none()
            .w(px(self.inspector_width))
            .h_full()
            .relative()
            .border_l(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .bg(rgb(theme.bg))
            .font_family(theme.ui_font_family.clone())
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text))
            .child(cards)
            .child(grip)
            .into_any_element()
    }
```

```rust
/// A card of the session panel: a muted title over its content (a block on `bg`, like a board
/// card). Tasks 16 and 18 add the Changes and Plan cards with it.
fn inspector_card(theme: &Theme, title: &'static str) -> gpui::Div {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(space::S))
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.card))
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(title),
        )
}
```

The Session card, its two actions and its buttons:

```rust
    /// Agent, title, state + time, usage, Open transcript and Stop of `session`.
    fn render_session_card(
        &self,
        session: &Session,
        now: i64,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let card = inspector::session_card(session, self.usages.get(session.id), now);
        let id = session.id;
        let open = match &card.transcript {
            Some(_) => button("inspector-transcript", "Open transcript", theme, false)
                .tooltip(tooltip(theme, "Open the agent's transcript file in Zed", ""))
                .on_click(cx.listener(move |shell, _, _, cx| shell.open_transcript(id, cx)))
                .into_any_element(),
            None => disabled_button("inspector-transcript", "Open transcript", theme, false)
                .tooltip(tooltip(theme, "No transcript yet", ""))
                .into_any_element(),
        };
        let stop = (inspector::stop_action(Some(session)) != inspector::StopAction::Nothing)
            .then(|| {
                danger_button("inspector-stop", "Stop", theme)
                    .tooltip(tooltip(theme, "Stop this session (asks first)", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.stop_session(id, window, cx);
                    }))
            });
        inspector_card(theme, "Session")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .child(agent_chip(self.agent_badge(&session.agent_profile), theme))
                    .child(div().min_w_0().truncate().child(card.agent.clone()))
                    .child(div().flex_1())
                    .child(status_mark(session.state, now, theme)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight(weight::MEDIUM))
                    .child(card.title.clone()),
            )
            .child(meta_line(theme, card.state.clone().into()))
            .children(card.usage.iter().map(|(label, value)| {
                div()
                    .flex()
                    .gap(px(space::S))
                    .text_size(font_size::META)
                    .child(
                        div()
                            .w(px(space::XL))
                            .flex_none()
                            .text_color(rgb(theme.text_muted))
                            .child(*label),
                    )
                    .child(div().min_w_0().truncate().child(value.clone()))
            }))
            .child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(open)
                    .children(stop),
            )
            .into_any_element()
    }

    /// "Open transcript": the agent's `.jsonl` in Zed. A failure (Zed missing, file gone) shows
    /// in the sidebar like the other Zed actions.
    fn open_transcript(&mut self, session: Ulid, cx: &mut Context<'_, Self>) {
        let Some(path) = self
            .usages
            .get(session)
            .and_then(|usage| usage.transcript.clone())
        else {
            return;
        };
        match zed::open(Path::new(&path), None) {
            Ok(()) => self.sidebar_error = None,
            Err(error) => {
                tracing::warn!(%session, path, "open transcript failed: {error}");
                self.sidebar_error = Some(error.to_string());
            }
        }
        cx.notify();
    }

    /// The card's Stop: `session.kill`. A running agent asks first; the pane stays until the
    /// session exits (ADR 0033).
    fn stop_session(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let known = self.sidebar.sessions().iter().find(|s| s.id == session);
        match inspector::stop_action(known) {
            inspector::StopAction::Nothing => {}
            inspector::StopAction::Kill => self.kill(session, window, cx),
            inspector::StopAction::Ask => {
                let answer = window.prompt(
                    PromptLevel::Warning,
                    "This session is still running.",
                    Some("Stops the agent. Its pane closes when it exits."),
                    &inspector::STOP_ANSWERS,
                    cx,
                );
                cx.spawn_in(window, async move |this, cx| {
                    if !task_dialog::confirms_delete(answer.await.ok()) {
                        return;
                    }
                    let _ = this.update_in(cx, |shell, window, cx| {
                        // It may have exited while the dialog was open.
                        if !shell.is_exited(session) {
                            shell.kill(session, window, cx);
                        }
                    });
                })
                .detach();
            }
        }
    }
```

If `space::XL` does not exist, use the widest existing `space::` token that fits a 4-character label at META size, or give the label column `theme::rems_px(theme::INSPECTOR_LABEL_WIDTH)` with a new const `INSPECTOR_LABEL_WIDTH: f32 = 56.0` in `theme.rs` (check `pub mod space` at theme.rs:13 first). If `Session`, `Path` or `weight` are not yet imported in main.rs, add them (`weight` comes with `font_size` from `theme`).

Status-bar button. In `render_status_bar` (Task 6) append after the usage container, as the last child of the bar:

```rust
            .child(self.inspector_toggle(theme, cx))
```

```rust
    /// The right-end status-bar button: shows or hides the session panel.
    fn inspector_toggle(&self, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::Stateful<gpui::Div> {
        let hover = theme.hover;
        div()
            .id("toggle-inspector")
            .flex_none()
            .ml_auto()
            .px(px(space::XS))
            .rounded(px(radius::SMALL))
            .cursor_pointer()
            .text_color(rgb(if self.inspector_open {
                theme.accent_text
            } else {
                theme.text_muted
            }))
            .when(self.inspector_open, |el| el.bg(rgb(theme.accent_bg)))
            .when(!self.inspector_open, |el| {
                el.hover(move |style| style.bg(rgb(hover)))
            })
            .tooltip(tooltip(
                theme,
                if self.inspector_open {
                    "Hide session panel"
                } else {
                    "Show session panel"
                },
                self.keymap.label(Command::ToggleInspector),
            ))
            .on_click(cx.listener(|shell, _, window, cx| shell.toggle_inspector(window, cx)))
            .child("◨")
    }
```

Run: `cargo build -p mkrew-ui` (fix imports/borrows the compiler names; `kill` takes `&mut self`, so `known` must not outlive the match arm that calls it: copy `stop_action(known)` into a local first if the borrow checker complains).
Expected: builds with no warnings.

- [ ] **Step 9: SPEC and ADR**

`docs/SPEC.md` §10, after the **Terminals** bullet (the one ending `No hard limit on terminals; warn above 12 running sessions per workspace.`) add:

```markdown
- **Session panel** (ADR 0038): a column right of the Terminals view, closed at every start,
  toggled by the status bar's right-end button and `Ctrl+Shift+Y` (from another view the key shows
  Terminals first). Drag its left edge to resize (240–560 px, saved as `[ui] inspector_width` when
  the button is released); the terminals lose that width, so an agent redraws once. It shows the
  focused pane's session. **Session card:** agent, title, state + time, the usage of §20E.3 (context
  share, tokens, cache, model; rows without data are left out), **Open transcript** (the agent's
  `.jsonl` in Zed; disabled until the daemon located it) and **Stop** (`session.kill`; a running
  agent asks first, a shell does not; the pane closes when the session exits, ADR 0033).
```

Also change the sentence `One window, two columns; the main area has a toolbar on top:` only if Task 3 left it unchanged; otherwise leave Task 3's wording and add after the diagram's last line `The session panel (below) is a third column right of Terminals while open.`

§13 table, after the `Toggle full screen` row (or after Task 6's sidebar row if it is last in the group):

```markdown
| View | Toggle the session panel | Ctrl+Shift+Y |
```

§20.0 settings block, in `[ui]` after `theme = …`:

```toml
inspector_width = 300       # §10; px, the session panel's width (dragging its edge writes it; kept 240–560)
```

§20E.3, replace `Event `session.usage` (`input`, `output`, `context`, `context_window?`);` by the same text with `transcript?` appended inside the parentheses (keep Task 4's fields as they are): `… `context_window?`, `transcript?` (the located transcript file's absolute path, sent again when it changes)`. In the Protocol (M13) paragraph, after `event `session.usage`;` nothing else changes.

Create `docs/decisions/0038-session-panel.md`:

```markdown
# 0038 — Session panel (right inspector)

- Date: 2026-10-10
- Status: accepted
- Milestone: inbox 2026-10-10
- Amends: 0027

## Context

The redesign asks for a panel on the right of the Terminals view that shows the focused pane's
session: agent, title, state, usage, a way to open its transcript, and Stop (later: the git
changes and the plan). SPEC §10 described one window with two columns (ADR 0027). There was no
resizable handle anywhere (pane dividers are keyboard-only), no UI-side place for a remembered
width, and the UI never learned the transcript path: only the daemon's usage reader found it.

## Options

1. A third column right of Terminals, closed by default; width in `settings.toml [ui]`; the
   transcript path on `session.usage`.
2. Same, with the width in a new UI state file or a daemon table.
3. A dialog or popover per pane instead of a column.

## Decision

Option 1.

- The panel exists in the Terminals view only and is closed at every start; the width is
  remembered (`[ui] inspector_width`, 240–560 px, written once when a drag ends with the
  text-preserving `settings::write_ui_value`, the same path as the theme toggle). No new file
  and no new table: `settings.toml` already holds the UI's own preferences.
- `session.usage` carries `transcript?`, the absolute path the daemon's reader located (daemon
  and UI share a machine, SPEC §3). No new request: the event is stored as JSON, so a reconnect
  gets the path too. `PROTOCOL_VERSION` 36.
- "Open transcript" opens the `.jsonl` in Zed (SPEC §10): no OS default app is reliable for it.
- Stop reuses the close-pane rule: a running agent asks first, a shell stops at once. It does
  not close the pane; the pane leaves when the session exits (ADR 0033).
- Default key `Ctrl+Shift+Y` (unused; plain Ctrl+letter would reach the terminal).

## Consequences

SPEC §10 (Session panel bullet), §13 (row), §20.0 (`inspector_width`) and §20E.3 (`transcript?`)
are updated. The terminals lose the panel's width while it is open and their agents redraw once
per size change. Changes and Plan cards (later tasks) reuse the column. The panel's open state is
not remembered; add `[ui] inspector_open` if that is asked for.
```

- [ ] **Step 10: HUMAN_CHECKS**

Append to the `### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)` section of `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Session panel:** it is closed at start. Click `◨` at the right end of the status bar (or
  `Ctrl+Shift+Y` with a terminal focused): a column opens right of the terminals; the agent TUI
  redraws once, narrower. Click another pane: the card follows it. `Ctrl+Shift+Y` again closes it.
  In Git or Files, `Ctrl+Shift+Y` shows Terminals with the panel open.
- [ ] **Resize:** hover the panel's left edge (the cursor becomes ↔), drag left and right: the
  width follows the pointer between 240 and 560 px, never more than half the window. Release,
  quit and restart Mkrew, open the panel: same width, and `settings.toml` has `inspector_width`
  (comments and other keys untouched). A click on the edge without dragging writes nothing.
- [ ] **Session card:** for a Claude pane after a turn: agent, title, `working · 2m` / `idle · 30s`,
  ctx %, tokens, cache and model rows. A shell pane shows no usage rows and no transcript.
  **Open transcript** opens the `.jsonl` in Zed (disabled with the tooltip "No transcript yet"
  before the first turn ends); with Zed missing, the sidebar shows the install hint.
- [ ] **Stop:** on a running agent, **Stop** asks "This session is still running."; Cancel does
  nothing; Stop ends it and the pane closes by itself. On a shell, Stop ends it without asking.
```

- [ ] **Step 11: Verify and tick**

Run `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh` (fallback: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). Do NOT tick INBOX line 88 yet; Task 16 does.
Suggested message: `feat(ui): session panel with the Session card, resizable and remembered; session.usage carries the transcript path`

---

### Task 16: Right panel: Changes card (1 or N repos)

Inbox: line 88 `Right panel (session inspector)` (Changes card). Spec: §10 (Session panel bullet from Task 15), §12B (:857-910). Ticks line 88.

**Current state (investigated):**
- Task 2 gives `Shell::git_summary() -> Option<&GitSummary>` (only for the shown workspace; filled on workspace switch and on `git.changed` for it; no polling, nothing for hidden workspaces), with `GitSummary.repos: Vec<RepoSummary { repo: String /* "" = the workspace folder */, branch, head, modified, untracked }` in `git.repos` order, root first when it is a repo; a root without `.git` lists only its child repos (ADR 0029). Task 15 left `Shell::render_inspector` with the Session card and `inspector_card`.
- Opening the Git view on one repo: `retarget_git(project_id, None, repo)` (main.rs ~:2404; `repo = None` is the workspace folder) puts a fresh `GitView` on that repo and working changes (keeps the commit draft); `show_view(View::Git, …)` (~:1425) then calls `refresh_git` (which runs `load_git_repos` → `git_view::pick_repo`, keeping a listed repo). The panel only exists in Terminals, so `show_view` always switches from another view and always refreshes.
- Existing marker colors for changed files: `M` = `theme.accent`, `?` = `theme.diff_add_text` (main.rs `git_file_row` / `ChangeKind` letters ~:15744); reuse them.
- `row_action(id: SharedString, label: &'static str, theme)` (main.rs ~:15953) is a small quiet text action with a dynamic id.

**Design decisions:**
1. Line format `M 23 ?3` (`M` + modified paths, `?` + untracked paths; a zero part is left out). Modified = distinct paths in staged ∪ unstaged (Task 2 `RepoSummary::of`); a conflicted file counts as modified.
2. Only repos with changes get a line; label = the workspace name for the folder itself (`""`), the child folder name otherwise. All repos clean → `Working tree clean`. No repo at all (root without `.git` and no child repo) or no summary yet → the card is hidden.
3. Review diff is per line and opens the Git view on that repo, in its working changes.

**Files:**
- Modify: `crates/mkrew-ui/src/inspector.rs` (`ChangeLine`, `changes_card`, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (`render_changes_card`, `review_diff`, `render_inspector`)
- Modify: `docs/SPEC.md` §10 (Session panel bullet), `docs/HUMAN_CHECKS.md`, `docs/INBOX.md` (tick)

**Interfaces:**
- Consumes: Task 2 `git_summary::{GitSummary, RepoSummary}` (fields above; `RepoSummary::changed()`), `Shell::git_summary()`; Task 15 `inspector_card`, `Shell::render_inspector`, `row_action`.
- Produces: `inspector::ChangeLine { repo: String, label: String, modified: Option<String>, untracked: Option<String> }`; `inspector::changes_card(summary: Option<&GitSummary>, workspace: &str) -> Option<Vec<ChangeLine>>` (`None` = hide the card, `Some(empty)` = clean); `Shell::review_diff(project_id: Ulid, repo: String, window, cx)`.

- [ ] **Step 1: Failing tests for the card lines**

In `inspector.rs` `mod tests` add (and `use crate::git_summary::{GitSummary, RepoSummary};` to the test imports):

```rust
    fn repo(name: &str, modified: usize, untracked: usize) -> RepoSummary {
        RepoSummary {
            repo: name.into(),
            branch: Some("main".into()),
            head: Some("1a2b3c4d".into()),
            modified,
            untracked,
        }
    }

    fn lines(repos: Vec<RepoSummary>) -> Vec<(String, String, Option<String>, Option<String>)> {
        changes_card(Some(&GitSummary { repos }), "api")
            .unwrap()
            .into_iter()
            .map(|l| (l.repo, l.label, l.modified, l.untracked))
            .collect()
    }

    #[test]
    fn one_repo_is_one_line_named_after_the_workspace() {
        assert_eq!(
            lines(vec![repo("", 23, 3)]),
            [("".into(), "api".into(), Some("M 23".into()), Some("?3".into()))]
        );
    }

    #[test]
    fn a_zero_part_is_left_out() {
        assert_eq!(
            lines(vec![repo("", 2, 0)]),
            [("".into(), "api".into(), Some("M 2".into()), None)]
        );
        assert_eq!(
            lines(vec![repo("", 0, 5)]),
            [("".into(), "api".into(), None, Some("?5".into()))]
        );
    }

    #[test]
    fn only_repos_with_changes_get_a_line_in_repo_order() {
        // A root without `.git` lists just its children; clean ones have no line.
        let got = lines(vec![repo("fe", 1, 0), repo("be", 0, 0), repo("ops", 0, 2)]);
        let labels: Vec<&str> = got.iter().map(|l| l.1.as_str()).collect();
        assert_eq!(labels, ["fe", "ops"]);
        assert_eq!(got[1].0, "ops", "Review diff opens the child repo by its name");
    }

    #[test]
    fn the_card_says_clean_or_hides() {
        // Repos, none changed: a card with no line ("Working tree clean").
        assert_eq!(
            changes_card(Some(&GitSummary { repos: vec![repo("", 0, 0)] }), "api"),
            Some(Vec::new())
        );
        // No repository at all, or nothing loaded yet: no card.
        assert_eq!(changes_card(Some(&GitSummary::default()), "api"), None);
        assert_eq!(changes_card(None, "api"), None);
    }
```

Run: `cargo test -p mkrew-ui --lib inspector -j 4`
Expected: FAIL to compile (`changes_card` not defined).

- [ ] **Step 2: Implement the card lines**

`inspector.rs`, add `use crate::git_summary::{GitSummary, RepoSummary};` and above the tests:

```rust
/// One repository with changes on the Changes card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeLine {
    /// The repository as `git.repos` names it (`""` = the workspace folder): what Review diff
    /// opens.
    pub repo: String,
    /// The workspace name for the folder itself, else the child folder.
    pub label: String,
    /// `M 23`, when any path is modified.
    pub modified: Option<String>,
    /// `?3`, when any file is untracked.
    pub untracked: Option<String>,
}

fn change_line(repo: &RepoSummary, workspace: &str) -> ChangeLine {
    ChangeLine {
        repo: repo.repo.clone(),
        label: if repo.repo.is_empty() {
            workspace.to_owned()
        } else {
            repo.repo.clone()
        },
        modified: (repo.modified > 0).then(|| format!("M {}", repo.modified)),
        untracked: (repo.untracked > 0).then(|| format!("?{}", repo.untracked)),
    }
}

/// The Changes card: a line per repository with changes, in `git.repos` order. `Some(empty)` =
/// every repository is clean; `None` = hide the card (no repository, or no summary loaded).
pub fn changes_card(summary: Option<&GitSummary>, workspace: &str) -> Option<Vec<ChangeLine>> {
    let summary = summary.filter(|s| !s.repos.is_empty())?;
    Some(
        summary
            .repos
            .iter()
            .filter(|repo| repo.changed() > 0)
            .map(|repo| change_line(repo, workspace))
            .collect(),
    )
}
```

Run: `cargo test -p mkrew-ui --lib inspector -j 4`
Expected: PASS.

- [ ] **Step 3: Render the card and open the Git view**

`main.rs`. `review_diff` (near `set_git_repo`):

```rust
    /// "Review diff" on the Changes card: the Git view of `repo` (`""` = the workspace folder),
    /// in its working changes. `retarget_git` first, so `show_view`'s refresh already asks for
    /// that repository (it stays: `pick_repo` keeps a listed repo).
    fn review_diff(
        &mut self,
        project_id: Ulid,
        repo: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.retarget_git(project_id, None, (!repo.is_empty()).then_some(repo));
        self.show_view(View::Git, window, cx);
    }
```

The card (near `render_session_card`):

```rust
    /// One line per repository with changes (`M 23 ?3` + Review diff); `None` when the
    /// workspace has no repository (the card is hidden).
    fn render_changes_card(
        &self,
        project_id: Ulid,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or("workspace", |p| p.name.as_str());
        let lines = inspector::changes_card(self.git_summary(), workspace)?;
        let card = inspector_card(theme, "Changes");
        if lines.is_empty() {
            return Some(card.child(meta_line(theme, "Working tree clean".into())));
        }
        Some(card.children(lines.into_iter().enumerate().map(|(i, line)| {
            let repo = line.repo.clone();
            div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .child(div().flex_1().min_w_0().truncate().child(line.label))
                .children(
                    line.modified
                        .map(|text| div().flex_none().text_color(rgb(theme.accent)).child(text)),
                )
                .children(line.untracked.map(|text| {
                    div()
                        .flex_none()
                        .text_color(rgb(theme.diff_add_text))
                        .child(text)
                }))
                .child(
                    row_action(
                        SharedString::from(format!("inspector-review-{i}")),
                        "Review diff",
                        theme,
                    )
                    .flex_none()
                    .tooltip(tooltip(theme, "Open the Git view on this repository", ""))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.review_diff(project_id, repo.clone(), window, cx);
                    })),
                )
        })))
    }
```

In `render_inspector` (Task 15) change the `cards` body so the Changes card follows the Session card. `render_inspector` needs the workspace id: add a `project_id: Ulid` parameter (the call in `render_main` passes `project_id`), then replace the `.children(match session { … })` of `cards` by:

```rust
            .children(match session {
                Some(session) => vec![self.render_session_card(session, now, theme, cx)],
                None => vec![meta_line(theme, "No session focused".into()).into_any_element()],
            })
            // Follows the workspace, not the focused pane.
            .children(
                self.render_changes_card(project_id, theme, cx)
                    .map(IntoElement::into_any_element),
            )
```

and update the call: `el.child(self.render_inspector(project_id, now, theme, cx))`.

Run: `cargo build -p mkrew-ui`
Expected: builds with no warnings.

- [ ] **Step 4: SPEC and HUMAN_CHECKS**

SPEC §10, in the Session panel bullet (Task 15) append:

```markdown
  **Changes card:** one line per repository with changes, `M 23 ?3` (modified paths, untracked
  files) with **Review diff**, which opens the Git view on that repository (§12B); a workspace
  folder without `.git` still lists its child repositories (ADR 0029). Clean repositories have no
  line (`Working tree clean` when none changed); no repository, no card. The counts are the
  shown workspace's git summary, refreshed on `git.changed`.
```

`docs/HUMAN_CHECKS.md`, append to the same 2026-10-10 section:

```markdown
- [ ] **Changes card (one repo):** in a git workspace with 2 edited files and 1 new file the panel
  shows `<workspace> M 2 ?1 Review diff`; edit another file: it updates without reopening the
  panel. **Review diff** opens the Git view on that repo; clean → `Working tree clean`.
- [ ] **Changes card (several repos):** on a folder without `.git` holding repos `fe` and `be`,
  only the one with changes has a line (`fe M 3`), and Review diff opens the Git view on `fe`
  (the Repo chips show `fe` selected). A folder with no repo shows no Changes card.
```

- [ ] **Step 5: Verify and tick**

Run `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh` (fallback: fmt check, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`). In `docs/INBOX.md` change line 88 (`**7. Right panel (session inspector)**`) from `[~]` (or `[ ]` if it was never marked) to `[x]`.
Suggested message: `feat(ui): Changes card in the session panel with Review diff per repository`

---

### Task 17: Plan link: daemon (store, protocol, one-file watcher, counts) (ADR 0039)

Inbox: line 95 `Plan checklist in the right panel` (with Task 18; do not tick yet). Spec: §5 (:240, :316-320), §7 (:519-521, :557-560), §8 resume (:609-612), §20.0 settings watcher (the template). Does not change ADR 0033 (only live sessions are watched and shown).

**Current state (investigated):**
- No single-file watcher exists. `crates/mkrew-daemon/src/settings.rs` `watch(holder)` (:83-102) is the template: non-recursive watch on the parent dir, a file-name filter (editors save through temp file + rename), and `debounce` (:105-119, `QUIET` = 300 ms). The task ends when the `Watch` handle drops. `watch.rs` is the recursive per-project watcher; it only lives while the project has a live session, and the plan may lie outside the project, so it is not reused.
- No markdown parser in `mkrew-core` or the daemon (`pulldown-cmark` is `mkrew-ui` only). A line scanner is enough: `- [ ]` / `- [x]` / `* [X]` lines, outside fenced code.
- `crates/mkrew-daemon/src/docs.rs` `read(path)` (:60-71) reads a UTF-8 file up to `MAX_BYTES` (2 MiB) and returns `ReadError` (`NotFound`, `TooLarge`, `NotUtf8`, `Io`); `is_markdown(path)` (:33). Both are reused.
- `Session` is built by hand in 28 places (list below). The store hand-writes the 15-column `SELECT` in `list_sessions` (`crates/mkrew-store/src/lib.rs` :787-796) and `get_session` (:799-810), `insert_session` (:702-727), `update_session` (:731-753) and `RawSession` (:1403-1465).
- The session task (`Actor`) owns the session row: `Command::Rename` (`session.rs` :246-251, handler :1089-1095) exists because a store-only change would be undone by the actor's next `save()`. `plan_path` follows the same rule: `session.link_plan` goes through a new `Command::LinkPlan`, and `update_session` writes `plan_path`, so every later save keeps it.
- Resume builds a NEW session from the old one (`server/resume.rs` :114-127 builds a `Start`; `Daemon::start_session` `server.rs` :802-821 builds the `Session` field by field). Without a `Start.plan_path` the link is lost at each daemon restart. `Start` literals: `server.rs` (:510, :598), `server/resume.rs` (:114), `server/run.rs` (:130).
- `Hub::spawn` (`session.rs` :331-520) already calls `self.watch_project(session.project_id, &project_dir, false)` after the row is inserted: the place to start the plan watch of a resumed session. `Actor::finish` (:1421-1431) removes the session from `live`: the place to stop it. `session.remove` kills the session, so it ends in `finish` too.
- `session.usage` / `session.ports` (`server.rs` :421-426) are the pattern for "a reconnecting UI asks once per workspace, events keep it live". `session.plan` (request) copies it; the answer comes from the watcher's last announced event.
- Every exhaustive `match` on `Event` outside the daemon: `crates/mkrew-ui/src/sidebar.rs` :260-262 (`Change::None` arm). Nothing in `mkrew-cli`.

**Design (defaults chosen, see the ADR):**
- Path form: the UI sends the picked absolute path; the daemon stores it relative to the project folder with `/` when it lies inside (the folder can move), else absolute. Only `.md` files that `docs::read` accepts can be linked (else `invalid_request`, nothing changes).
- Only live sessions can be linked (`session.link_plan` on an exited or unknown one → `not_live`, as `session.attach`). Exited sessions leave the UI anyway (ADR 0033).
- One watcher per linked live session. Each change re-reads the file on the debounce task (≤ 2 MiB, parse is a line scan) and announces `session.plan` only when it differs from the last announced event, so a prose-only edit sends nothing.
- Wire: the event carries at most 200 items (each ≤ 120 chars) and the exact `done`/`total`; the UI shows 8 rows.
- A linked file that disappears or cannot be read keeps the link and announces `total: 0` with `error` (the watcher stays; when the file comes back the counts return).

**Files:**
- Create: `crates/mkrew-core/src/plan.rs` (pure parser)
- Create: `crates/mkrew-daemon/src/plan.rs` (`Plans`, `locate`, `snapshot`)
- Create: `crates/mkrew-store/migrations/0017_session_plan.sql`
- Create: `docs/decisions/0039-plan-checklist-from-a-linked-file.md`
- Modify: `crates/mkrew-core/src/lib.rs` (`pub mod plan;` between `paths` and `profile`)
- Modify: `crates/mkrew-protocol/src/message.rs` (`Request` :239-247, `Event` :364-376, `Session` :473-497, new types near `SessionRenameParams` :1013 and `SessionPortsEvent` :1388, tests :1755, :2265-2306), `crates/mkrew-protocol/src/lib.rs:16` (`PROTOCOL_VERSION` 36 → 37)
- Modify: `crates/mkrew-store/src/lib.rs` (MIGRATIONS :17-32, `insert_session`, `update_session`, `list_sessions`, `get_session`, `RawSession`), tests `crates/mkrew-store/src/tests.rs` (new test after `hidden_sessions_leave_the_list_but_keep_their_history` :1183)
- Modify: `crates/mkrew-daemon/src/lib.rs` (`mod plan;`, alphabetical among :9-30), `settings.rs` (:76-119), `session.rs` (`Hub` :73-104 + `Hub::new` :291-309, `Command` :201-252, handler :1089, `Hub::spawn` :474, `Actor::finish` :1427), `server.rs` (imports :14-18, dispatch :421-426, new `session_link_plan`, `start_session` :804-821, `Start` :1463-1478, literals :510, :598), `server/resume.rs` (:114-127), `server/run.rs` (:130-142)
- Modify (one new field `plan_path: None,` in each `Session { .. }` literal; find them with `cargo check --workspace --all-targets -j 2`, "missing field `plan_path`"): `crates/mkrew-core/src/run.rs:351`; `crates/mkrew-daemon/src/{awake.rs:389, notify.rs:320, turns.rs:297, server/resume.rs:228}`; `crates/mkrew-daemon/tests/daemon.rs:681`; `crates/mkrew-protocol/src/message.rs:2272`; `crates/mkrew-store/src/tests.rs:17`; `crates/mkrew-ui/src/{board.rs:587, broadcast.rs:255, conflicts.rs:178, doc_dialog.rs:332, fanout.rs:408, git_view.rs:2434, history.rs:221, inbox.rs:219, lifecycle.rs:183, palette.rs:634, run.rs:291, sidebar.rs:372, status.rs:193, tabs.rs:899, turns_dialog.rs:198, undo_dialog.rs:333, usage.rs:192, worktree_dialog.rs:449}`. The real constructors (`store/src/lib.rs:1453` `into_session`, `server.rs:811` `start_session`) get real values in the steps below.
- Modify: `crates/mkrew-ui/src/sidebar.rs:260-262` (`Event::SessionPlan(_)` → `Change::None`)
- Test: `crates/mkrew-daemon/tests/daemon.rs`
- Modify: `docs/SPEC.md` §5, §7; `docs/HUMAN_CHECKS.md` is edited by Task 18 (the UI makes the daemon part checkable)

**Interfaces:**
- Produces (protocol, `mkrew_protocol`):
  ```rust
  pub struct PlanItem { pub done: bool, pub text: String }
  pub struct SessionLinkPlanParams { pub session_id: Ulid, pub path: Option<String> } // None = unlink
  pub struct SessionPlanEvent {
      pub session_id: Ulid,
      pub name: String,            // file stem, "PLAN" for docs/PLAN.md
      pub done: u32, pub total: u32,
      pub items: Vec<PlanItem>,    // first 200 items in file order
      pub error: Option<String>,   // None = read fine; Some = why not (counts are 0)
  }
  // Session gains: #[serde(default, skip_serializing_if = "Option::is_none")] pub plan_path: Option<String>
  Request::SessionLinkPlan(SessionLinkPlanParams) // "session.link_plan" → Session (also session.updated)
  Request::SessionPlan(ProjectRef)                // "session.plan" → Vec<SessionPlanEvent> (live linked sessions of the project)
  Event::SessionPlan(SessionPlanEvent)            // "session.plan"
  ```
  `PROTOCOL_VERSION = 37`.
- Produces (core): `mkrew_core::plan::{Checklist { done: u32, total: u32, items: Vec<PlanItem> }, parse(src: &str) -> Checklist, MAX_ITEMS: usize = 200, MAX_TEXT_CHARS: usize = 120}`.
- Produces (store): `Session.plan_path: Option<String>` stored in `session.plan_path` (`0017`).
- Consumes by Task 18: the three protocol items above; `Session.plan_path` in `session.list` / `session.updated`.

- [ ] **Step 1: Protocol types and tests (fail to compile first)**

In `crates/mkrew-protocol/src/message.rs`, `event_wire_shape_matches_spec`, after the `usage` asserts (the end of the test), add:

```rust
        // A plan link is absent on the wire when unset.
        assert!(updated["data"].get("plan_path").is_none());
        let plan = round_trip(&Message::Event(Event::SessionPlan(SessionPlanEvent {
            session_id: sid(),
            name: "PLAN".into(),
            done: 1,
            total: 2,
            items: vec![
                PlanItem {
                    done: true,
                    text: "a".into(),
                },
                PlanItem {
                    done: false,
                    text: "b".into(),
                },
            ],
            error: None,
        })));
        assert_eq!(plan["event"], "session.plan");
        assert_eq!(
            plan["data"],
            json!({
                "session_id": sid(), "name": "PLAN", "done": 1, "total": 2,
                "items": [{ "done": true, "text": "a" }, { "done": false, "text": "b" }]
            })
        );
```

In the same test's `Session` literal (the `updated` event, `ended_at: None,`) add `plan_path: None,` after `ended_at: None,`. In the request round-trip list, after `Request::SessionPorts(ProjectRef { project_id: sid() }),` add:

```rust
            Request::SessionLinkPlan(SessionLinkPlanParams {
                session_id: sid(),
                path: Some("docs/PLAN.md".into()),
            }),
            Request::SessionLinkPlan(SessionLinkPlanParams {
                session_id: sid(),
                path: None,
            }),
            Request::SessionPlan(ProjectRef { project_id: sid() }),
```

Run: `cargo test -p mkrew-protocol -j 4`
Expected: FAIL to compile (`cannot find type PlanItem`, `no field plan_path`).

- [ ] **Step 2: Protocol implementation**

`Session` (after `ended_at`), add:

```rust
    /// The linked plan `.md` (`session.link_plan`): relative to the project folder with `/`
    /// when it lies inside, else absolute. `None` = no plan (SPEC §5, ADR 0039).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_path: Option<String>,
```

`Request`, after `SessionPorts`:

```rust
    /// Returns the [`Session`] (also announced as `session.updated`): links the Markdown file
    /// `path` as the plan of a live session, or unlinks it (`null`). The daemon announces
    /// `session.plan` at once and after each change of the file. `invalid_request` for a path
    /// that is not a readable `.md` file (ADR 0039).
    #[serde(rename = "session.link_plan")]
    SessionLinkPlan(SessionLinkPlanParams),
    /// Returns `SessionPlanEvent[]`: the checklist of each live session of the project that
    /// has a plan (for a client that connects later).
    #[serde(rename = "session.plan")]
    SessionPlan(ProjectRef),
```

`Event`, after `SessionTerminalTitle`:

```rust
    /// The checklist of a linked plan changed (ADR 0039).
    #[serde(rename = "session.plan")]
    SessionPlan(SessionPlanEvent),
```

After `SessionRenameParams`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLinkPlanParams {
    pub session_id: Ulid,
    /// A path picked by the user (absolute) or relative to the project folder; `None` unlinks.
    #[serde(default)]
    pub path: Option<String>,
}
```

After `SessionPortsEvent`:

```rust
/// One `- [ ]` / `- [x]` line of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    pub done: bool,
    pub text: String,
}

/// The checklist of a session's linked plan (ADR 0039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPlanEvent {
    pub session_id: Ulid,
    /// The file name without `.md`.
    pub name: String,
    pub done: u32,
    pub total: u32,
    /// The first items in file order (at most 200); `total` counts all of them.
    pub items: Vec<PlanItem>,
    /// Why the file could not be read (then the counts are 0); the link stays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
```

`crates/mkrew-protocol/src/lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 37;`

Run: `cargo test -p mkrew-protocol -j 4`
Expected: PASS.

- [ ] **Step 3: Failing parser tests**

Create `crates/mkrew-core/src/plan.rs` with only the tests first (types `Checklist`, `parse`, consts absent):

````rust
//! Plan checklists (SPEC §10, ADR 0039): the `- [ ]` / `- [x]` lines of a Markdown file.

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(list: &Checklist) -> Vec<(&str, bool)> {
        list.items
            .iter()
            .map(|item| (item.text.as_str(), item.done))
            .collect()
    }

    #[test]
    fn counts_every_bullet_style_and_box_case() {
        let list = parse("- [ ] a\n- [x] b\n* [X] c\n+ [ ] d\n");
        assert_eq!((list.done, list.total), (2, 4));
        assert_eq!(
            texts(&list),
            [("a", false), ("b", true), ("c", true), ("d", false)]
        );
        assert_eq!(parse(""), Checklist::default());
    }

    #[test]
    fn nested_items_and_crlf_count_and_text_is_trimmed() {
        let list = parse("# Plan\r\n- [x]   one  \r\n  - [ ] two\r\n\t- [ ] three\r\nprose\r\n");
        assert_eq!((list.done, list.total), (1, 3));
        assert_eq!(
            texts(&list),
            [("one", true), ("two", false), ("three", false)]
        );
    }

    #[test]
    fn fenced_code_is_not_a_checklist() {
        let src = "- [ ] a\n```md\n- [ ] hidden\n```\n~~~~\n- [x] hidden\n~~~\n- [x] still hidden\n~~~~\n- [x] b\n";
        assert_eq!(texts(&parse(src)), [("a", false), ("b", true)]);
        // An unclosed fence hides the rest of the file, as Markdown renders it.
        assert_eq!(parse("- [ ] a\n```\n- [ ] b\n").total, 1);
    }

    #[test]
    fn lines_that_are_not_task_items_are_ignored() {
        for line in [
            "- [] a",
            "- [y] a",
            "-[ ] a",
            "[ ] a",
            "- [x]a",
            "1. [ ] a",
            "- [ ]",
            "- [ ]   ",
            "text - [ ] a",
            "-- [ ] a",
        ] {
            assert_eq!(parse(line).total, 0, "{line:?}");
        }
    }

    #[test]
    fn long_text_is_cut_and_items_are_capped_but_counted() {
        let long = "x".repeat(MAX_TEXT_CHARS + 30);
        let list = parse(&format!("- [ ] {long}\n"));
        assert_eq!(list.items[0].text.chars().count(), MAX_TEXT_CHARS + 1);
        assert!(list.items[0].text.ends_with('…'));
        // Never cut inside a multi-byte character.
        let wide = "é".repeat(MAX_TEXT_CHARS + 1);
        assert!(parse(&format!("- [x] {wide}")).items[0].text.ends_with('…'));

        let many: String = (0..MAX_ITEMS + 50)
            .map(|i| format!("- [{}] item {i}\n", if i % 2 == 0 { 'x' } else { ' ' }))
            .collect();
        let list = parse(&many);
        assert_eq!(
            (list.total as usize, list.items.len()),
            (MAX_ITEMS + 50, MAX_ITEMS)
        );
        assert_eq!(list.done as usize, (MAX_ITEMS + 50) / 2);
    }
}
````

Add `pub mod plan;` to `crates/mkrew-core/src/lib.rs` (between `paths` and `profile`).

Run: `cargo test -p mkrew-core plan -j 4`
Expected: FAIL to compile (`cannot find function parse`).

- [ ] **Step 4: Parser implementation**

Above the test module in `crates/mkrew-core/src/plan.rs`:

```rust
use mkrew_protocol::PlanItem;

/// Longest item text kept, in characters; a longer one is cut and ends with `…`.
pub const MAX_TEXT_CHARS: usize = 120;
/// Most items kept (`total` still counts every one): the event stays small whatever the file.
pub const MAX_ITEMS: usize = 200;

/// The task-list items of one file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Checklist {
    pub done: u32,
    pub total: u32,
    /// The first [`MAX_ITEMS`] items in file order.
    pub items: Vec<PlanItem>,
}

/// The `- [ ]` / `- [x]` / `* [X]` / `+ [ ]` lines of `src`, at any indent, outside fenced
/// code blocks (``` or ~~~; an unclosed fence runs to the end, as rendered). A box needs a
/// space after it and some text; ordered lists (`1. [ ]`) are not counted.
pub fn parse(src: &str) -> Checklist {
    let mut list = Checklist::default();
    let mut fence: Option<(char, usize)> = None;
    for line in src.lines() {
        let line = line.trim_start();
        if let Some((marker, len)) = fence {
            let run = line.chars().take_while(|&c| c == marker).count();
            if run >= len && line[run..].trim().is_empty() {
                fence = None;
            }
            continue;
        }
        if let Some(open) = fence_open(line) {
            fence = Some(open);
            continue;
        }
        let Some((done, text)) = task_item(line) else {
            continue;
        };
        list.total += 1;
        list.done += u32::from(done);
        if list.items.len() < MAX_ITEMS {
            list.items.push(PlanItem {
                done,
                text: cut(text),
            });
        }
    }
    list
}

/// The fence character and length of a line that opens a code fence.
fn fence_open(line: &str) -> Option<(char, usize)> {
    let marker = line.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let len = line.chars().take_while(|&c| c == marker).count();
    (len >= 3).then_some((marker, len))
}

/// `(done, text)` of a task-list line (already left-trimmed).
fn task_item(line: &str) -> Option<(bool, &str)> {
    let rest = line.strip_prefix(['-', '*', '+'])?;
    let rest = rest.strip_prefix([' ', '\t'])?.trim_start();
    let (done, rest) = if let Some(rest) = rest.strip_prefix("[ ]") {
        (false, rest)
    } else if let Some(rest) = rest.strip_prefix("[x]").or_else(|| rest.strip_prefix("[X]")) {
        (true, rest)
    } else {
        return None;
    };
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    let text = rest.trim();
    (!text.is_empty()).then_some((done, text))
}

fn cut(text: &str) -> String {
    match text.char_indices().nth(MAX_TEXT_CHARS) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}
```

Run: `cargo test -p mkrew-core plan -j 4`
Expected: PASS (5 tests).

- [ ] **Step 5: Failing store test, migration, store implementation**

`crates/mkrew-store/src/tests.rs`, in the `session()` helper add `plan_path: None,` after `ended_at: None,`. After `hidden_sessions_leave_the_list_but_keep_their_history` add:

```rust
/// A session's linked plan (ADR 0039) is stored with it, survives a reopen, and follows
/// the session's own saves (the actor owns the row): changed, then cleared.
#[test]
fn a_linked_plan_is_saved_with_the_session() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let mut linked = session(p.id, 1);
    linked.plan_path = Some("docs/PLAN.md".into());
    store.insert_session(&linked).unwrap();
    let plain = session(p.id, 2);
    store.insert_session(&plain).unwrap();
    drop(store);

    let store = open(&dir);
    assert_eq!(
        store.get_session(linked.id).unwrap().plan_path.as_deref(),
        Some("docs/PLAN.md")
    );
    assert_eq!(
        store.list_sessions(Some(p.id)).unwrap(),
        vec![linked.clone(), plain]
    );

    linked.plan_path = Some("/elsewhere/other.md".into());
    linked.state = SessionState::Idle;
    store.update_session(&linked).unwrap();
    assert_eq!(store.get_session(linked.id).unwrap(), linked);
    linked.plan_path = None;
    store.update_session(&linked).unwrap();
    assert_eq!(store.get_session(linked.id).unwrap().plan_path, None);
}
```

Run: `cargo test -p mkrew-store a_linked_plan -j 4`
Expected: FAIL to compile (`no field plan_path`).

Create `crates/mkrew-store/migrations/0017_session_plan.sql`:

```sql
-- SPEC §5, ADR 0039. Never edit: schema changes go in a new numbered migration.
ALTER TABLE session ADD COLUMN plan_path TEXT; -- linked plan .md: project-relative with '/', or absolute; NULL = none (session.link_plan)
```

`crates/mkrew-store/src/lib.rs`:
- `MIGRATIONS`: append `include_str!("../migrations/0017_session_plan.sql"),`.
- `insert_session`: add `plan_path` as the LAST column and the next placeholder. With today's 15 columns:
  ```rust
              "INSERT INTO session (id, task_id, project_id, agent_profile, kind, title,
                  native_session_id, state, exit_code, cols, rows, started_at, ended_at,
                  state_source, state_changed_at, plan_path)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
  ```
  and `session.plan_path,` as the last entry of `params![..]`. (If Tasks 12/13 already appended `color` / `pinned` here, add `plan_path` after them with the next number.)
- `update_session`: add `, plan_path = ?11` to the `SET` list (after `state_changed_at = ?10`) and `session.plan_path,` as the last param (the next number if earlier tasks added theirs).
- `list_sessions` and `get_session`: append `, plan_path` to the end of the column list of both `SELECT`s.
- `RawSession`: add `plan_path: Option<String>,` as the last field; `from_row`: `plan_path: row.get(N)?,` where `N` = the number of columns before it (15 today; count the SELECT list); `into_session`: `plan_path: self.plan_path,`.

Run: `cargo test -p mkrew-store -j 4`
Expected: PASS (`a_linked_plan_is_saved_with_the_session`, and `open_runs_migrations_and_enables_wal` follows `MIGRATIONS.len()`).

- [ ] **Step 6: Daemon: shared one-file watcher**

`crates/mkrew-daemon/src/settings.rs`: replace `watch` (:81-102) so the settings and the plan watcher share one implementation:

```rust
/// Watches the directory of the settings file and reloads `holder` [`QUIET`] after the file
/// last changed. Must run inside the tokio runtime.
pub fn watch(holder: Arc<SettingsHolder>) -> ::notify::Result<Watch> {
    let path = holder.path.clone();
    watch_file(&path, move || holder.reload())
}

/// Calls `on_change` [`QUIET`] after the file `path` last changed, was created, replaced or
/// removed. Watches its directory (non-recursive) and filters on the file name, because
/// editors save through a temp file + rename. Must run inside the tokio runtime; dropping the
/// [`Watch`] stops it (a change still pending then is delivered once more).
pub fn watch_file(
    path: &Path,
    on_change: impl Fn() + Send + 'static,
) -> ::notify::Result<Watch> {
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let name: Option<OsString> = path.file_name().map(Into::into);
    let (tx, rx) = mpsc::unbounded_channel();
    let mut watcher = ::notify::recommended_watcher(
        move |event: ::notify::Result<::notify::Event>| match event {
            Ok(event) if !event.kind.is_access() => {
                // Editors often save via a temp file + rename: any event naming the file counts.
                if event.paths.iter().any(|p| p.file_name() == name.as_deref()) {
                    let _ = tx.send(());
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("file watcher error: {e}"),
        },
    )?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    tokio::spawn(debounce(rx, on_change));
    Ok(Watch { _watcher: watcher })
}
```

Also change the doc of `Watch` (:76) to `/// A running one-file watcher; dropping it stops watching.` and the module doc's first paragraph keeps its text. The existing `debounce_reloads_once_after_quiet` test still covers the debounce.

Run: `cargo test -p mkrew-daemon --lib settings -j 4`
Expected: PASS.

- [ ] **Step 7: Failing daemon unit tests (`plan.rs`)**

Create `crates/mkrew-daemon/src/plan.rs` with the tests only:

```rust
//! Plan checklists (SPEC §7 `session.link_plan`, ADR 0039).

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// Inside the project the path is stored relative with `/` (the folder can move); outside
    /// it stays absolute. A relative request resolves against the project folder.
    #[test]
    fn locate_stores_project_files_relative() {
        let root = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("docs")).unwrap();
        std::fs::write(root.path().join("docs/PLAN.md"), "- [ ] a\n").unwrap();
        std::fs::write(other.path().join("far.md"), "- [ ] a\n").unwrap();

        let inside = root.path().join("docs").join("PLAN.md");
        let expected = ("docs/PLAN.md".to_owned(), inside.clone());
        assert_eq!(
            locate(root.path(), inside.to_str().unwrap()).unwrap(),
            expected
        );
        assert_eq!(locate(root.path(), " docs/PLAN.md ").unwrap(), expected);

        let far = other.path().join("far.md");
        let (stored, file) = locate(root.path(), far.to_str().unwrap()).unwrap();
        assert_eq!((PathBuf::from(stored), file), (far.clone(), far));
    }

    #[test]
    fn locate_refuses_what_is_not_a_readable_markdown_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("notes.txt"), "- [ ] a\n").unwrap();
        std::fs::create_dir(root.path().join("dir.md")).unwrap();
        for bad in ["notes.txt", "missing.md", "dir.md", "", "   "] {
            assert!(locate(root.path(), bad).is_err(), "{bad:?}");
        }
        std::fs::write(
            root.path().join("big.md"),
            vec![b'a'; docs::MAX_BYTES as usize + 1],
        )
        .unwrap();
        let error = locate(root.path(), "big.md").unwrap_err();
        assert!(error.contains("at most"), "{error}");
    }

    /// A plan that cannot be read keeps its name and says why; the link stays, so the counts
    /// return when the file does.
    #[test]
    fn snapshot_reports_a_missing_file_instead_of_counts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("PLAN.md");
        let id = Ulid::new();

        let gone = snapshot(id, &path);
        assert_eq!(
            (gone.name.as_str(), gone.total, gone.error.as_deref()),
            ("PLAN", 0, Some("no such file"))
        );
        assert!(gone.items.is_empty());

        std::fs::write(&path, "- [x] a\n- [ ] b\n").unwrap();
        let read = snapshot(id, &path);
        assert_eq!((read.done, read.total, read.error), (1, 2, None));
        assert_eq!(read.items.len(), 2);
    }
}
```

In `crates/mkrew-daemon/src/lib.rs` add `mod plan;` in alphabetical position among the `mod` lines.

Run: `cargo test -p mkrew-daemon --lib plan -j 4`
Expected: FAIL to compile (`cannot find function locate` / `snapshot`).

- [ ] **Step 8: Daemon `plan.rs` implementation**

Above `mod tests` in `crates/mkrew-daemon/src/plan.rs`:

```rust
//! Plan checklists (SPEC §7 `session.link_plan`, ADR 0039): one file watcher per live
//! session with a linked plan. A change re-reads the file, parses it ([`mkrew_core::plan`])
//! and announces `session.plan` only when the checklist differs from the last one announced.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mkrew_protocol::{Event, SessionPlanEvent};
use tokio::sync::broadcast;
use ulid::Ulid;

use crate::docs;
use crate::settings::{Watch, watch_file};

/// The last announced checklist and the watch of one linked session.
struct Linked {
    project_id: Ulid,
    last: Arc<Mutex<SessionPlanEvent>>,
    _watch: Option<Watch>,
}

/// The plans of the live sessions, by session id. Never held with another lock.
#[derive(Default)]
pub struct Plans {
    linked: Mutex<HashMap<Ulid, Linked>>,
}

impl Plans {
    /// Links `file` as the plan of `session_id` (replacing an earlier link): announces its
    /// checklist at once and again after each change. `None` unlinks and stops the watch.
    /// Must run inside the tokio runtime.
    pub fn set(
        &self,
        events: &broadcast::Sender<Event>,
        session_id: Ulid,
        project_id: Ulid,
        file: Option<PathBuf>,
    ) {
        let Some(file) = file else {
            lock(&self.linked).remove(&session_id);
            return;
        };
        let mut first = snapshot(session_id, &file);
        let last = Arc::new(Mutex::new(first.clone()));
        let on_change = {
            let (events, last, file) = (events.clone(), Arc::downgrade(&last), file.clone());
            move || {
                // Unlinked meanwhile: a change still pending in the debounce reports nothing.
                let Some(last) = last.upgrade() else {
                    return;
                };
                let next = snapshot(session_id, &file);
                let mut current = lock(&last);
                if *current != next {
                    *current = next.clone();
                    let _ = events.send(Event::SessionPlan(next));
                }
            }
        };
        let watch = match watch_file(&file, on_change) {
            Ok(watch) => Some(watch),
            Err(e) => {
                tracing::warn!(%session_id, file = %file.display(), "cannot watch the plan: {e}");
                first.error = Some(format!("cannot watch the file: {e}"));
                *lock(&last) = first.clone();
                None
            }
        };
        let _ = events.send(Event::SessionPlan(first));
        lock(&self.linked).insert(
            session_id,
            Linked {
                project_id,
                last,
                _watch: watch,
            },
        );
    }

    /// Stops the watch of an exiting session.
    pub fn stop(&self, session_id: Ulid) {
        lock(&self.linked).remove(&session_id);
    }

    /// The last announced checklist of each linked live session of the project.
    pub fn for_project(&self, project_id: Ulid) -> Vec<SessionPlanEvent> {
        let mut plans: Vec<SessionPlanEvent> = lock(&self.linked)
            .values()
            .filter(|linked| linked.project_id == project_id)
            .map(|linked| lock(&linked.last).clone())
            .collect();
        plans.sort_by_key(|plan| plan.session_id);
        plans
    }
}

/// A client's plan path as `(stored form, file)`: relative to `root` with `/` when it lies
/// inside the project (the folder can move), else as given (absolute). `Err` = the message for
/// `invalid_request`: not `.md`, missing, a folder, too large, not UTF-8 text.
// ponytail: a path that differs from `root` only in case or `\\?\` form is stored absolute;
// it still works, it just does not follow a moved folder.
pub fn locate(root: &Path, picked: &str) -> Result<(String, PathBuf), String> {
    let picked = picked.trim();
    if !docs::is_markdown(picked) {
        return Err("a plan must be a .md file".to_owned());
    }
    let file = root.join(picked);
    docs::read(&file).map_err(|e| format!("cannot read the plan: {e}"))?;
    let stored = match file.strip_prefix(root) {
        Ok(relative) => relative
            .components()
            .map(|part| part.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => file.to_string_lossy().into_owned(),
    };
    Ok((stored, file))
}

/// The checklist of `file` now; unreadable = no counts and the reason.
pub fn snapshot(session_id: Ulid, file: &Path) -> SessionPlanEvent {
    let name = file
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    match docs::read(file) {
        Ok(text) => {
            let list = mkrew_core::plan::parse(&text);
            SessionPlanEvent {
                session_id,
                name,
                done: list.done,
                total: list.total,
                items: list.items,
                error: None,
            }
        }
        Err(e) => SessionPlanEvent {
            session_id,
            name,
            done: 0,
            total: 0,
            items: Vec::new(),
            error: Some(e.to_string()),
        },
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A map and a last event: still consistent if a holder panicked.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
```

Run: `cargo test -p mkrew-daemon --lib plan -j 4`
Expected: PASS (3 tests).

- [ ] **Step 9: Daemon wiring (hub, actor, server, resume)**

`session.rs`:
- `Hub` struct, after `terminal_titles`:
  ```rust
      /// Linked plans of live sessions, each with its file watcher (ADR 0039).
      pub plans: crate::plan::Plans,
  ```
  and in `Hub::new`: `plans: crate::plan::Plans::default(),`.
- `Command`, after `Rename`:
  ```rust
      /// `session.link_plan`: the actor owns the row (its next save would undo a store-only
      /// change). `plan_path` is the stored form, `file` the resolved path to watch; both
      /// `None` unlinks. Replies with the updated session, also announced as `session.updated`.
      LinkPlan {
          plan_path: Option<String>,
          file: Option<PathBuf>,
          reply: oneshot::Sender<Session>,
      },
  ```
- Handler, after the `Command::Rename` arm (:1089-1095):
  ```rust
                      Some(Command::LinkPlan { plan_path, file, reply }) => {
                          self.session.plan_path = plan_path;
                          self.save();
                          let _ = self.hub.events.send(Event::SessionUpdated(self.session.clone()));
                          // After `session.updated`, so a client knows the link before its counts.
                          self.hub.plans.set(
                              &self.hub.events,
                              self.session.id,
                              self.session.project_id,
                              file,
                          );
                          let _ = reply.send(self.session.clone());
                      }
  ```
- `Hub::spawn`, right after `self.watch_project(session.project_id, &project_dir, false);`:
  ```rust
        // A resumed session keeps its plan (SPEC §8): read and watch it again.
        if let Some(plan) = &session.plan_path {
            let file = project_dir.join(plan);
            self.plans
                .set(&self.events, session.id, session.project_id, Some(file));
        }
  ```
- `Actor::finish`, after `lock(&self.hub.live).remove(&self.session.id);`: `self.hub.plans.stop(self.session.id);`

`server.rs`:
- Import `SessionLinkPlanParams` in the `mkrew_protocol::{..}` list (:14-18) and `use crate::plan;`.
- `Start` (:1463): add `/// The plan of the session being resumed (SPEC §8); `None` for a new one.` + `plan_path: Option<String>,`. Add `plan_path: None,` to the `Start` literals at :510 and :598 and in `server/run.rs` :130; in `server/resume.rs` :114-127 add `plan_path: last.plan_path.clone(),` (after `env: Vec::new(),`).
- `start_session` (:804-821): add `plan_path: start.plan_path,` to the `Session` literal (after `ended_at: None,`).
- Dispatch, after `Request::SessionPorts`:
  ```rust
              Request::SessionLinkPlan(params) => {
                  (self.session_link_plan(params).await, After::Continue)
              }
              Request::SessionPlan(params) => (
                  ok(&self.hub.plans.for_project(params.project_id)),
                  After::Continue,
              ),
  ```
- New handler after `session_rename`:
  ```rust
    /// `session.link_plan` (ADR 0039): a live session's task stores the link and starts or
    /// stops the file watch; the file is checked first, so a bad path changes nothing.
    async fn session_link_plan(&self, params: SessionLinkPlanParams) -> Outcome {
        let Some(sender) = self.hub.sender(params.session_id) else {
            return self.not_live(params.session_id);
        };
        let found = self.store().get_session(params.session_id);
        let project_id = match found {
            Ok(session) => session.project_id,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let found = self.store().get_project(project_id);
        let root = match found {
            Ok(project) => PathBuf::from(project.path),
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let (plan_path, file) = match params.path.as_deref() {
            None => (None, None),
            Some(picked) => match plan::locate(&root, picked) {
                Ok((stored, file)) => (Some(stored), Some(file)),
                Err(message) => return Outcome::error(ErrorCode::InvalidRequest, message),
            },
        };
        let (reply, linked) = oneshot::channel();
        let command = Command::LinkPlan {
            plan_path,
            file,
            reply,
        };
        if sender.send(command).await.is_ok()
            && let Ok(session) = linked.await
        {
            return ok(&session);
        }
        // Exited while the request was on its way.
        self.not_live(params.session_id)
    }
  ```
  (`locate` reads the file, at most 2 MiB, on the request task: the same cost `fs.read` has.)

`crates/mkrew-ui/src/sidebar.rs:260-262`: add `| Event::SessionPlan(_)` to the `Change::None` arm after `Event::SessionPorts(_)`.

Run: `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` then `cargo check --workspace --all-targets -j 2`
Expected: the only errors left are missing `plan_path` in `Session { .. }` literals (list in **Files**). Add `plan_path: None,` to each, then re-run until clean.

- [ ] **Step 10: Failing then passing daemon integration tests**

`crates/mkrew-daemon/tests/daemon.rs`: add `PlanItem`, `SessionLinkPlanParams`, `SessionPlanEvent` to the `use mkrew_protocol::{..}` list (:27-31). Next to `rename` (:6050) add:

```rust
fn link_plan(session_id: Ulid, path: Option<&str>) -> Request {
    Request::SessionLinkPlan(SessionLinkPlanParams {
        session_id,
        path: path.map(Into::into),
    })
}
```

After `sessions_are_renamed_live_and_exited_but_run_sessions_refuse` add:

```rust
/// `session.link_plan` (ADR 0039): the checklist is announced at once and after each
/// change of the file; a change that keeps the checklist sends nothing; a deleted file reports
/// an error; unlinking stops the watch. Bad links are refused and change nothing.
#[test]
fn a_linked_plan_reports_its_checklist_until_it_is_unlinked() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    let plan = repo.join("PLAN.md");
    std::fs::write(&plan, "# Plan\n- [x] a\n- [ ] b\n").unwrap();
    std::fs::write(repo.join("notes.txt"), "- [ ] x\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    for bad in ["notes.txt", "missing.md"] {
        assert_eq!(
            error_code(client.call(link_plan(id, Some(bad)))),
            ErrorCode::InvalidRequest,
            "{bad}"
        );
    }
    assert_eq!(
        error_code(client.call(link_plan(Ulid::new(), Some("PLAN.md")))),
        ErrorCode::NotFound
    );
    assert_eq!(client.session(id).plan_path, None);

    let linked: Session = client
        .call(link_plan(id, Some(plan.to_str().unwrap())))
        .into_result()
        .unwrap();
    assert_eq!(
        linked.plan_path.as_deref(),
        Some("PLAN.md"),
        "inside the project: stored relative"
    );
    assert_eq!(client.session(id).plan_path.as_deref(), Some("PLAN.md"));
    let counts = |done: u32, total: u32| {
        move |e: &Event| {
            matches!(e, Event::SessionPlan(p) if p.session_id == id
                && (p.done, p.total) == (done, total) && p.error.is_none())
        }
    };
    let Event::SessionPlan(first) = client.wait_event_where(counts(1, 2)) else {
        unreachable!()
    };
    assert_eq!(first.name, "PLAN");
    assert_eq!(
        first.items,
        [
            PlanItem {
                done: true,
                text: "a".into()
            },
            PlanItem {
                done: false,
                text: "b".into()
            }
        ]
    );
    let listed = |client: &mut Client| -> Vec<SessionPlanEvent> {
        client
            .call(Request::SessionPlan(ProjectRef {
                project_id: project.id,
            }))
            .into_result()
            .unwrap()
    };
    assert_eq!(listed(&mut client), vec![first]);

    // A prose-only edit keeps the checklist: nothing is announced (wait out the debounce).
    std::fs::write(&plan, "# Plan, renamed\n- [x] a\n- [ ] b\n").unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    std::fs::write(&plan, "# Plan, renamed\n- [x] a\n- [x] b\n").unwrap();
    client.wait_event_where(counts(2, 2));
    let announced = client
        .events
        .iter()
        .filter(|e| matches!(e, Event::SessionPlan(p) if p.session_id == id))
        .count();
    assert_eq!(announced, 2, "the link and the edit; the prose edit sent nothing");

    std::fs::remove_file(&plan).unwrap();
    let Event::SessionPlan(gone) = client.wait_event_where(
        |e| matches!(e, Event::SessionPlan(p) if p.session_id == id && p.error.is_some()),
    ) else {
        unreachable!()
    };
    assert_eq!((gone.total, gone.items.len()), (0, 0));

    let unlinked: Session = client.call(link_plan(id, None)).into_result().unwrap();
    assert_eq!(unlinked.plan_path, None);
    assert!(listed(&mut client).is_empty());
    let seen = client.events.len();
    std::fs::write(&plan, "- [ ] a\n- [ ] b\n- [ ] c\n").unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    client.session(id); // reads what the daemon pushed meanwhile
    assert!(
        !client.events[seen..]
            .iter()
            .any(|e| matches!(e, Event::SessionPlan(_))),
        "no watch after unlink"
    );
    client.stop();
}
```

In `restart_resumes_free_agent_panes_in_tabs_once`, after the `rename(shown.session_id, Some("Fix login"))` call (:4472-4475) add:

```rust
    let repo = env.repo();
    std::fs::write(repo.join("PLAN.md"), "- [ ] a\n- [x] b\n").unwrap();
    client
        .call(link_plan(shown.session_id, Some("PLAN.md")))
        .into_result::<Session>()
        .unwrap();
```

and after the `resumed.title` "keeps its name" assert (:4513-4517):

```rust
    assert_eq!(
        resumed.plan_path.as_deref(),
        Some("PLAN.md"),
        "the resumed pane keeps its plan"
    );
    let plans: Vec<SessionPlanEvent> = client
        .call(Request::SessionPlan(ProjectRef {
            project_id: project.id,
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        plans
            .iter()
            .map(|p| (p.session_id, p.done, p.total))
            .collect::<Vec<_>>(),
        [(resumed.id, 1, 2)],
        "and watches it again"
    );
```

Run (the prebuilt `mk.exe` must be fresh after a protocol bump, see PROGRESS traps):
`cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` then
`cargo test -p mkrew-daemon --test daemon a_linked_plan -j 2` and `cargo test -p mkrew-daemon --test daemon restart_resumes_free_agent_panes_in_tabs_once -j 2`
Expected: PASS. (Before Step 9 they do not compile; to see them fail on behavior, comment out `self.hub.plans.set(..)` in the `LinkPlan` arm: the first test then times out waiting for `counts(1, 2)`.)

- [ ] **Step 11: ADR 0039**

Create `docs/decisions/0039-plan-checklist-from-a-linked-file.md`:

```markdown
# 0039 — Plan checklist from a linked Markdown file

- Date: 2026-10-10
- Status: accepted
- Milestone: outside the milestone loop (inbox 2026-10-10, item 8)

## Context

The redesign (B4) wants a session's plan progress in the right panel and on the selected
sidebar row: `Plan <name> 6/20`, a bar, the checklist. The first idea, a "Setup / Tasks /
Finish" phase bar, needs each agent's output parsed: per-agent code, which SPEC §6 forbids
(agents differ only in `agents/*.toml`), and it breaks with every agent release. Agents that
plan do write a Markdown checklist (`- [ ]` / `- [x]`) somewhere, in the project or outside.
The daemon already reads Markdown (`docs::read`, 2 MiB cap) and has a one-file watcher shape
(`settings.rs`).

## Options

1. Phase bar parsed from agent output — per-agent, fragile. Rejected.
2. Reuse the project watcher (`watch.rs`) and read plans from its batches — no new watcher,
   but it only lives while the project has a live session, the plan may lie outside the
   project, and every unrelated change of a big tree would be filtered per session.
3. One watcher per linked live session on that file only, parse in the daemon — one tiny
   watcher each, works for any path, the UI never parses Markdown.
4. Parse in the UI after `fs.read` — a round trip per change and a second parser; the file
   may be outside the project (`fs.read` is project-relative).

## Decision

Option 3. The user links a `.md` file to a live session (`session.link_plan`; stored as
`session.plan_path`, relative to the project folder when inside, else absolute; kept by
resume). The daemon watches the file's directory non-recursively with a name filter
(editors save through rename), debounces 300 ms, re-reads the file, scans it line by line for
task-list items outside code fences (`mkrew_core::plan::parse`) and announces `session.plan`
only when the checklist changed. The event carries exact `done`/`total` and at most 200 items
of ≤ 120 characters. A file that cannot be read keeps the link and reports an `error`.
Exited sessions are neither watched nor shown (ADR 0033). No phase bar.

## Consequences

SPEC §5 (`session.plan_path`, migration `0017`), §7 (`session.link_plan`,
`session.plan` request and event, protocol 37), §10 (Plan card, sidebar bar; Task 18).
Ceiling: N sessions linked to the same file run N small watchers (one directory handle
each); share them per path if that ever matters. Nested items count like top-level ones;
ordered lists (`1. [ ]`) are not counted. A linked file edited by an agent updates the counts
within about a second without any polling.
```

- [ ] **Step 12: SPEC**

§5, migrations paragraph (SPEC.md:316-320), append after the `0014_project_default_shell.sql` sentence:

```markdown
`0017_session_plan.sql` adds `session.plan_path TEXT` (the linked plan `.md`: relative to the project folder with `/` when inside, else absolute; NULL = none; ADR 0039).
```

§7 requests, after the `session.history` row (:521):

```markdown
| `session.link_plan` | `session_id`, `path?` (a `.md` file, absolute or project-relative; `null` unlinks) | the Session (also `session.updated`) — live sessions only (`invalid_state`/`not_found` otherwise); `invalid_request` when the file is not `.md`, missing, a folder, over 2 MiB or not UTF-8; the daemon then watches that one file (ADR 0039) |
| `session.plan` | `project_id` | `[{session_id, name, done, total, items[], error?}]` of the project's live sessions with a plan (the last announced checklist, for a client that connects later) |
```

§7 events, after the `session.terminal_title` row (:560):

```markdown
| `session.plan` | the checklist of a linked plan changed (`session_id`, `name` = file name without `.md`, `done`, `total`, `items[]` = first 200 task items `{done, text ≤ 120 chars}`, `error?` = why the file cannot be read, counts then 0): sent at once on link and ≤ 1 s after each change of the file (debounced), never twice with the same content; `- [ ]`/`- [x]`/`* [X]` lines outside code fences (ADR 0039) |
```

§8 (resume, :609-612), after "so each session is resumed at most once." add: "A resumed session keeps its linked plan (`plan_path`)."

- [ ] **Step 13: Verify and tick**

Run the full verification: `"C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. Do not tick inbox line 95 yet (Task 18 ticks it).
Suggested message: `feat(daemon): link a plan .md to a session and watch its checklist (session.link_plan, session.plan)`

---

### Task 18: Plan checklist card + sidebar progress bar

Inbox: line 95 `Plan checklist in the right panel`. Spec: §10 (:690-692 sidebar; the Right panel bullet Task 15 added), §7 (Task 17 rows), §5. ADR 0039 already covers the design (no new ADR). Needs Task 9 (the selected sidebar row's second line), Task 15 (`Shell::render_inspector`) and Task 17 (protocol).

**Current state (investigated):**
- `Usages` (`crates/mkrew-ui/src/usage.rs` :14-66) is the pattern for a per-session pure state loaded per workspace on connect and kept live by events: `to_request` / `request_failed` / `apply(&Event) -> bool` / `load`. `Shell` loads it in `load_usage` (`main.rs` :824-849, called from the refresh at :776) and applies events in `dispatch` (:7700). `Plans` copies it.
- Nothing draws a progress bar yet. Theme colors needed exist: `theme.border` (track), `theme.accent` (fill), `theme.text` / `text_dim` / `text_muted`, `theme.accent_text` (current item), `theme.diff_del_text` (error, used by `danger_button`). Check box size: `theme::DOC_TASK_BOX_SIZE` (:237). No new color, so dark and light themes need nothing.
- The only file picker is `add_workspace` (`main.rs` :4298-4348): `cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, .. })` then `cx.spawn_in(window, ..)`. The plan picker is the same with `files: true, directories: false`. The picker has no extension filter; the daemon refuses non-`.md` (`invalid_request`) and the card shows that message.
- `button(id: &'static str, ..)`, `quiet_button(id, label, theme, hover)` (`main.rs` :15155-15200) already set `.hover()` once: never add another `.hover()` on them.
- `session_row` (`main.rs` :16530-16607) is called once from `render_sidebar` (:7924-7928) with `selected = shown == Some(row.session_id)`. Task 9 made the selected row two lines (name line + title/task line in a column). The bar goes under that second line.
- Task 15's `Shell::render_inspector` renders the cards (Session, Changes) for the focused pane's session; Task 18 adds the Plan card after Changes.
- Daemon facts (Task 17): `Session.plan_path` arrives with `session.list` / `session.updated`; `session.plan` event on link, on each change and with `error` when unreadable; unlink sends `session.updated` with `plan_path: None` and no plan event; `session.plan` request lists a workspace's live linked sessions.

**Design (defaults chosen):**
- The card shows for live agent sessions only (shells have no plan; exited sessions are gone from the UI, ADR 0033). Unlinked: title `Plan` and a primary **Link plan…** button. Linked: `Plan <name>` + `6/20`, bar, up to 8 rows (2 above the first unchecked, which is highlighted in `accent_text`; checked rows muted), `N more`, and **Change…** / **Unlink**. Unreadable file: the error line replaces bar and rows; Change / Unlink stay.
- The sidebar bar is shown only on the selected session row with a known non-zero total (one `HashMap` lookup per frame, no loop over rows).
- A `session.plan` answer that crosses a newer event can be one change stale; the next change corrects it (same trade-off as `Usages`, no timestamps).

**Files:**
- Create: `crates/mkrew-ui/src/plan.rs` (pure state + view model + tests)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod plan;` between `palette` and `router`; keep alphabetical)
- Modify: `crates/mkrew-ui/src/theme.rs` (two heights after `SESSION_ROW_HEIGHT` :714)
- Modify: `crates/mkrew-ui/src/main.rs`: imports (`mkrew_ui::plan::{self, Plans}`; `SessionLinkPlanParams`, `SessionPlanEvent` in the `mkrew_protocol` list :33-37), `Shell` fields (next to `usages` :287 / init :656), `load_plans` (after `load_ports`), `dispatch` (after `ports.apply` :7703), `pick_plan` / `link_plan`, `render_plan_card` + `plan_row` + `progress_bar`, `render_inspector` (Task 15), `render_sidebar` (:7924-7928) and `session_row` (:16530)
- Modify: `docs/SPEC.md` §10; `docs/HUMAN_CHECKS.md`; `docs/INBOX.md:95`

**Interfaces:**
- Consumes (Task 17): `Request::SessionLinkPlan(SessionLinkPlanParams { session_id, path })` → `Session`; `Request::SessionPlan(ProjectRef)` → `Vec<SessionPlanEvent>`; `Event::SessionPlan`; `Session.plan_path`. (Task 9): the selected session row's second line. (Task 15): `Shell::render_inspector`, which has the focused pane's `&Session`.
- Produces (`mkrew_ui::plan`):
  ```rust
  pub const VISIBLE_ROWS: usize = 8;
  #[derive(Debug, Default)] pub struct Plans { .. }
  impl Plans {
      pub fn to_request(&mut self, project_ids: impl IntoIterator<Item = Ulid>) -> Vec<Ulid>;
      pub fn request_failed(&mut self, project_id: Ulid);
      pub fn apply(&mut self, event: &Event) -> bool;      // session.plan / session.updated (unlinked) / session.exited
      pub fn load(&mut self, listed: Vec<SessionPlanEvent>) -> bool;
      pub fn get(&self, session_id: Ulid) -> Option<&SessionPlanEvent>;
      pub fn fraction(&self, session_id: Ulid) -> Option<f32>; // sidebar bar; None without total
  }
  pub struct Card { pub name: String, pub count: String, pub fraction: f32, pub rows: Vec<Row>, pub more: usize, pub error: Option<String> }
  pub struct Row { pub text: String, pub done: bool, pub current: bool }
  pub fn card(plan: &SessionPlanEvent) -> Card;
  pub fn fraction(done: u32, total: u32) -> Option<f32>;
  pub fn name_of(plan_path: &str) -> String;                // "PLAN" for "docs/PLAN.md"
  ```
  `theme::PLAN_BAR_HEIGHT` (4.0) and `theme::PLAN_ROW_BAR_HEIGHT` (2.0).

- [ ] **Step 1: Failing tests for the view model**

Create `crates/mkrew-ui/src/plan.rs` with only the tests (types absent), and add `pub mod plan;` to `lib.rs`:

```rust
//! Plan checklist (SPEC §10, ADR 0039): the newest `session.plan` of each linked
//! session, loaded per workspace on connect and kept live by events; the right panel's Plan
//! card and the sidebar's progress bar. Pure; `main.rs` draws it.

#[cfg(test)]
mod tests {
    use mkrew_protocol::{
        Session, SessionExitedEvent, SessionKind, SessionState, StateSource,
    };

    use super::*;

    fn plan(session_id: Ulid, done: &[bool]) -> SessionPlanEvent {
        SessionPlanEvent {
            session_id,
            name: "PLAN".into(),
            done: done.iter().filter(|d| **d).count() as u32,
            total: done.len() as u32,
            items: done
                .iter()
                .enumerate()
                .map(|(i, done)| PlanItem {
                    done: *done,
                    text: format!("item {i}"),
                })
                .collect(),
            error: None,
        }
    }

    /// The first `n` boxes checked, `len` boxes in all.
    fn first_done(n: usize, len: usize) -> Vec<bool> {
        (0..len).map(|i| i < n).collect()
    }

    fn session(id: Ulid, plan_path: Option<&str>) -> Session {
        Session {
            id,
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            plan_path: plan_path.map(Into::into),
        }
    }

    #[test]
    fn the_first_unchecked_item_is_highlighted_with_context_and_the_rest_collapsed() {
        let card = card(&plan(Ulid::new(), &first_done(10, 20)));
        assert_eq!((card.count.as_str(), card.fraction), ("10/20", 0.5));
        // Two rows of context above the first unchecked one, then the next ones.
        let shown: Vec<&str> = card.rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            shown,
            ["item 8", "item 9", "item 10", "item 11", "item 12", "item 13", "item 14", "item 15"]
        );
        let current: Vec<&str> = card
            .rows
            .iter()
            .filter(|r| r.current)
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(current, ["item 10"]);
        assert_eq!(card.more, 12);
        assert!(card.rows[0].done && !card.rows[2].done);
    }

    #[test]
    fn the_window_stops_at_the_end_of_the_list() {
        let card = card(&plan(Ulid::new(), &first_done(19, 20)));
        assert_eq!(card.rows.len(), VISIBLE_ROWS);
        assert_eq!(card.rows[0].text, "item 12");
        assert_eq!(card.rows.last().unwrap().text, "item 19");
        assert!(card.rows.last().unwrap().current);
        assert_eq!(card.more, 12);
    }

    #[test]
    fn a_short_or_finished_plan_shows_from_the_top_without_a_highlight() {
        let short = card(&plan(Ulid::new(), &[true, false, false]));
        assert_eq!((short.rows.len(), short.more), (3, 0));
        assert!(short.rows[1].current);

        let finished = card(&plan(Ulid::new(), &first_done(20, 20)));
        assert_eq!(finished.rows[0].text, "item 0");
        assert_eq!((finished.rows.len(), finished.more), (VISIBLE_ROWS, 12));
        assert!(finished.rows.iter().all(|r| r.done && !r.current));
        assert_eq!(finished.fraction, 1.0);
    }

    /// The daemon sends at most 200 items but the exact total: "N more" counts the ones not sent.
    #[test]
    fn more_counts_items_beyond_the_wire_cap() {
        let mut capped = plan(Ulid::new(), &first_done(0, 5));
        capped.total = 300;
        assert_eq!(card(&capped).more, 295);
    }

    #[test]
    fn an_unreadable_plan_shows_its_error_and_no_progress() {
        let mut broken = plan(Ulid::new(), &[]);
        broken.error = Some("no such file".into());
        let card = card(&broken);
        assert_eq!(card.error.as_deref(), Some("no such file"));
        assert_eq!((card.rows.len(), card.more, card.fraction), (0, 0, 0.0));
    }

    #[test]
    fn fraction_needs_a_total_and_never_exceeds_one() {
        assert_eq!(fraction(0, 0), None);
        assert_eq!(fraction(3, 12), Some(0.25));
        assert_eq!(fraction(5, 3), Some(1.0));
    }

    #[test]
    fn name_is_the_file_stem() {
        assert_eq!(name_of("docs/PLAN.md"), "PLAN");
        assert_eq!(name_of("2026-10-10-inbox.md"), "2026-10-10-inbox");
    }

    #[test]
    fn events_replace_and_unlinking_or_exit_clears() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut plans = Plans::default();
        let one = plan(a, &[true, false]);
        assert!(plans.apply(&Event::SessionPlan(one.clone())));
        assert!(!plans.apply(&Event::SessionPlan(one)), "same content: no redraw");
        assert!(plans.apply(&Event::SessionPlan(plan(a, &[true, true]))));
        assert_eq!(plans.fraction(a), Some(1.0));
        assert!(plans.apply(&Event::SessionPlan(plan(b, &[false]))));

        // Still linked: other updates (a rename) keep it.
        assert!(!plans.apply(&Event::SessionUpdated(session(a, Some("PLAN.md")))));
        assert!(plans.get(a).is_some());
        // Unlinked: gone. Exited: gone.
        assert!(plans.apply(&Event::SessionUpdated(session(a, None))));
        assert_eq!(plans.get(a), None);
        assert!(plans.apply(&Event::SessionExited(SessionExitedEvent {
            session_id: b,
            exit_code: Some(0),
        })));
        assert_eq!(plans.get(b), None);
    }

    #[test]
    fn each_workspace_is_requested_once_until_it_fails() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut plans = Plans::default();
        assert_eq!(plans.to_request([a, b]), [a, b]);
        assert!(plans.to_request([a, b]).is_empty());
        plans.request_failed(a);
        assert_eq!(plans.to_request([a, b]), [a]);
    }
}
```

Run: `cargo test -p mkrew-ui --lib plan -j 4`
Expected: FAIL to compile (`cannot find function card`).

- [ ] **Step 2: Implement the view model**

Above `mod tests` in `crates/mkrew-ui/src/plan.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::path::Path;

use mkrew_protocol::{Event, PlanItem, SessionPlanEvent};
use ulid::Ulid;

/// Checklist rows the card shows; the rest is "N more".
pub const VISIBLE_ROWS: usize = 8;
/// Rows kept above the first unchecked item, so it is seen in context.
const ROWS_BEFORE_CURRENT: usize = 2;

#[derive(Debug, Default)]
pub struct Plans {
    by_session: HashMap<Ulid, SessionPlanEvent>,
    /// Workspaces whose `session.plan` was requested (once each: events keep them live).
    requested: HashSet<Ulid>,
}

impl Plans {
    /// The workspaces among `project_ids` whose plans were not requested yet, marked requested.
    pub fn to_request(&mut self, project_ids: impl IntoIterator<Item = Ulid>) -> Vec<Ulid> {
        project_ids
            .into_iter()
            .filter(|id| self.requested.insert(*id))
            .collect()
    }

    /// `project_id`'s `session.plan` failed: the next [`Plans::to_request`] asks again.
    pub fn request_failed(&mut self, project_id: Ulid) {
        self.requested.remove(&project_id);
    }

    /// Keeps a `session.plan` event and forgets a plan whose session was unlinked
    /// (`session.updated` without `plan_path`) or exited; returns whether anything shown changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::SessionPlan(plan) => self.keep(plan.clone()),
            Event::SessionUpdated(session) if session.plan_path.is_none() => {
                self.by_session.remove(&session.id).is_some()
            }
            Event::SessionExited(exited) => self.by_session.remove(&exited.session_id).is_some(),
            _ => false,
        }
    }

    /// Keeps one workspace's `session.plan` answer; returns whether anything shown changed.
    pub fn load(&mut self, listed: Vec<SessionPlanEvent>) -> bool {
        let mut changed = false;
        for plan in listed {
            changed |= self.keep(plan);
        }
        changed
    }

    fn keep(&mut self, plan: SessionPlanEvent) -> bool {
        if self.by_session.get(&plan.session_id) == Some(&plan) {
            return false;
        }
        self.by_session.insert(plan.session_id, plan);
        true
    }

    pub fn get(&self, session_id: Ulid) -> Option<&SessionPlanEvent> {
        self.by_session.get(&session_id)
    }

    /// The sidebar bar's fill; `None` without a plan or without items.
    pub fn fraction(&self, session_id: Ulid) -> Option<f32> {
        let plan = self.get(session_id)?;
        fraction(plan.done, plan.total)
    }
}

/// `done` of `total` as 0–1; `None` when there is nothing to count.
pub fn fraction(done: u32, total: u32) -> Option<f32> {
    (total > 0).then(|| done.min(total) as f32 / total as f32)
}

/// The plan's name from its stored path: the file name without `.md`.
pub fn name_of(plan_path: &str) -> String {
    Path::new(plan_path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What the Plan card shows for one `session.plan`.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub name: String,
    /// `6/20`.
    pub count: String,
    pub fraction: f32,
    pub rows: Vec<Row>,
    /// Items not in `rows` (also those the daemon did not send).
    pub more: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub done: bool,
    /// The first unchecked item of the plan.
    pub current: bool,
}

/// At most [`VISIBLE_ROWS`] rows: the first unchecked item with [`ROWS_BEFORE_CURRENT`] rows
/// above it (the top of the list when everything is checked), moved up at the end of the list
/// so the card stays full.
pub fn card(plan: &SessionPlanEvent) -> Card {
    let items = &plan.items;
    let current = items.iter().position(|item| !item.done);
    let start = current
        .unwrap_or(0)
        .saturating_sub(ROWS_BEFORE_CURRENT)
        .min(items.len().saturating_sub(VISIBLE_ROWS));
    let end = (start + VISIBLE_ROWS).min(items.len());
    let rows: Vec<Row> = items[start..end]
        .iter()
        .enumerate()
        .map(|(i, item)| Row {
            text: item.text.clone(),
            done: item.done,
            current: current == Some(start + i),
        })
        .collect();
    Card {
        name: plan.name.clone(),
        count: format!("{}/{}", plan.done, plan.total),
        fraction: fraction(plan.done, plan.total).unwrap_or(0.0),
        more: (plan.total as usize).saturating_sub(rows.len()),
        rows,
        error: plan.error.clone(),
    }
}
```

`PlanItem` is imported for the tests' `plan()` helper (`use super::*`); if clippy reports it unused outside tests, move it into the test module's `use mkrew_protocol::{PlanItem, ..}`.

Run: `cargo test -p mkrew-ui --lib plan -j 4`
Expected: PASS (9 tests). Check by hand the numbers in `the_first_unchecked_item..`: first unchecked = index 10, start = min(10 − 2, 20 − 8) = 8, rows 8..16, `more` = 20 − 8 = 12.

- [ ] **Step 3: Theme tokens**

`crates/mkrew-ui/src/theme.rs`, after `SESSION_ROW_HEIGHT` (:714):

```rust
/// Height of the Plan card's progress bar, and of the thin one under the selected sidebar row
/// (ADR 0039). The track is `Theme::border`, the fill `Theme::accent` (both themes).
pub const PLAN_BAR_HEIGHT: f32 = 4.0;
pub const PLAN_ROW_BAR_HEIGHT: f32 = 2.0;
```

- [ ] **Step 4: Shell state, loading, events**

`main.rs`:
- Imports: add `use mkrew_ui::plan::{self, Plans};` (alphabetical among the `mkrew_ui` uses) and `SessionLinkPlanParams, SessionPlanEvent` to the `mkrew_protocol` list.
- `Shell` fields, after `usages: Usages,` (:287):
  ```rust
    /// Each linked session's plan checklist (ADR 0039).
    plans: Plans,
    /// Why the last link / unlink failed (a file that is not a readable `.md`), shown in the Plan card.
    plan_error: Option<String>,
  ```
  and in the initializer, after `usages: Usages::default(),` (:656): `plans: Plans::default(), plan_error: None,`.
- In the refresh that calls `load_usage` / `load_ports` (:776-777) add `shell.load_plans(window, cx);`.
- After `load_ports` add:
  ```rust
    /// Reads the plans of each listed workspace not read yet (ADR 0039); events keep them live.
    fn load_plans(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let projects = self.sidebar.projects().iter().map(|project| project.id);
        for project_id in self.plans.to_request(projects.collect::<Vec<_>>()) {
            let listed = connection
                .call::<Vec<SessionPlanEvent>>(Request::SessionPlan(ProjectRef { project_id }));
            cx.spawn_in(window, async move |this, cx| {
                let listed = listed.await;
                let _ = this.update(cx, |shell, cx| match listed {
                    Ok(listed) => {
                        if shell.plans.load(listed) {
                            cx.notify();
                        }
                    }
                    Err(error) => {
                        tracing::warn!("session.plan failed: {error:#}");
                        // Asked again on the next refresh.
                        shell.plans.request_failed(project_id);
                    }
                });
            })
            .detach();
        }
    }
  ```
- `dispatch`, after the `ports.apply` block (:7703-7705):
  ```rust
                if self.plans.apply(event) {
                    cx.notify();
                }
  ```
- After `add_workspace` add the picker and the request:
  ```rust
    /// Picks a Markdown file and links it as `session_id`'s plan (`session.link_plan`).
    fn pick_plan(&mut self, session_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.plan_error = None;
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Link plan".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let path = match picked.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    let _ = this.update(cx, |shell, cx| {
                        shell.plan_error = Some(format!("Cannot open the file picker: {error:#}"));
                        cx.notify();
                    });
                    None
                }
            };
            let Some(path) = path else {
                return;
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let path = path.to_string_lossy().into_owned();
                shell.link_plan(session_id, Some(path), window, cx);
            });
        })
        .detach();
    }

    /// Links `path` as the plan of `session_id`, or unlinks with `None`. The daemon answers with
    /// `session.updated` and `session.plan`; only a refusal is kept here.
    fn link_plan(
        &mut self,
        session_id: Ulid,
        path: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        self.plan_error = None;
        cx.notify();
        let linked = connection.call::<Session>(Request::SessionLinkPlan(SessionLinkPlanParams {
            session_id,
            path,
        }));
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = linked.await {
                let _ = this.update(cx, |shell, cx| {
                    shell.plan_error = Some(format!("{error:#}"));
                    cx.notify();
                });
            }
        })
        .detach();
    }
  ```

Run: `cargo check -p mkrew-ui --all-targets -j 2`
Expected: PASS (the new methods are used in Step 5; if `dead_code` fires, do Step 5 before re-checking).

- [ ] **Step 5: The Plan card in the right panel**

Next to the other free draw helpers in `main.rs` (near `session_row`):

```rust
/// A thin progress bar: `fraction` (0–1) of the track in the accent color.
fn progress_bar(fraction: f32, height: f32, theme: &Theme) -> gpui::Div {
    div()
        .w_full()
        .h(px(height))
        .rounded(px(radius::SMALL))
        .overflow_hidden()
        .bg(rgb(theme.border))
        .child(div().h_full().w(relative(fraction)).bg(rgb(theme.accent)))
}

/// One checklist line of the Plan card: a check box and the item text; the first unchecked
/// item is in the accent text color, checked ones are muted.
fn plan_row(row: &plan::Row, theme: &Theme) -> gpui::Div {
    let color = match (row.done, row.current) {
        (true, _) => theme.text_muted,
        (false, true) => theme.accent_text,
        (false, false) => theme.text_dim,
    };
    let check = div()
        .flex_none()
        .mt(px(space::XS / 2.0))
        .w(px(theme::DOC_TASK_BOX_SIZE))
        .h(px(theme::DOC_TASK_BOX_SIZE))
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(if row.done { theme.accent } else { theme.subtle }))
        .when(row.done, |el| el.bg(rgb(theme.accent)));
    div()
        .flex()
        .items_start()
        .gap(px(space::S))
        .child(check)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(font_size::BODY)
                .text_color(rgb(color))
                .when(row.current, |el| el.font_weight(FontWeight(weight::MEDIUM)))
                .child(row.text.clone()),
        )
}
```

In `impl Shell`, next to `render_workspace_menu`-style render methods:

```rust
    /// The right panel's Plan card for an agent session (ADR 0039): the linked file's
    /// progress and checklist, or a button to link one.
    fn render_plan_card(
        &self,
        session: &Session,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let id = session.id;
        let linked = session.plan_path.is_some();
        let card = self.plans.get(id).map(plan::card);
        let title = match (&card, &session.plan_path) {
            (Some(card), _) => format!("Plan {}", card.name),
            (None, Some(path)) => format!("Plan {}", plan::name_of(path)),
            (None, None) => "Plan".to_owned(),
        };
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let header = div()
            .flex()
            .items_center()
            .gap(px(space::S))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(font_size::BODY)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text))
                    .child(title),
            )
            .children(card.as_ref().map(|card| muted(card.count.clone()).flex_none()));
        let body = match &card {
            Some(card) => match &card.error {
                Some(error) => div().child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.diff_del_text))
                        .child(error.clone()),
                ),
                None => div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(progress_bar(card.fraction, theme::PLAN_BAR_HEIGHT, theme))
                    .children(card.rows.iter().map(|row| plan_row(row, theme)))
                    .children((card.more > 0).then(|| muted(format!("{} more", card.more)))),
            },
            None if linked => div().child(muted("Reading the plan…".to_owned())),
            None => div().child(muted(
                "Link a Markdown checklist to follow this session's progress.".to_owned(),
            )),
        };
        let actions = if linked {
            div()
                .flex()
                .gap(px(space::S))
                .child(
                    quiet_button("plan-change", "Change…", theme, theme.hover).on_click(
                        cx.listener(move |shell, _, window, cx| shell.pick_plan(id, window, cx)),
                    ),
                )
                .child(
                    quiet_button("plan-unlink", "Unlink", theme, theme.hover).on_click(
                        cx.listener(move |shell, _, window, cx| {
                            shell.link_plan(id, None, window, cx);
                        }),
                    ),
                )
        } else {
            div().flex().child(
                button("plan-link", "Link plan…", theme, true).on_click(cx.listener(
                    move |shell, _, window, cx| shell.pick_plan(id, window, cx),
                )),
            )
        };
        // Same container as the Session and Changes cards (Task 15); this is its shape if it
        // is not a shared helper.
        div()
            .id("plan-card")
            .flex()
            .flex_col()
            .gap(px(space::S))
            .p(px(space::M))
            .rounded(px(radius::MEDIUM))
            .bg(rgb(theme.card))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.divider))
            .child(header)
            .child(body)
            .child(actions)
            .children(self.plan_error.as_ref().map(|error| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.diff_del_text))
                    .child(error.clone())
            }))
            .into_any_element()
    }
```

In Task 15's `render_inspector`, after the Changes card, add (use the `&Session` the Session card shows; look it up by id from `self.sidebar.sessions()` if the function only holds an id):

```rust
            .children(
                (session.kind == SessionKind::Agent && session.state != SessionState::Exited)
                    .then(|| self.render_plan_card(session, theme, cx)),
            )
```

Run: `cargo check -p mkrew-ui --all-targets -j 2`
Expected: PASS. Fix API mismatches against the vendored GPUI (`~/.cargo/registry/src/*/gpui-pre-0.3.7/`: `relative`, `Styled::mt`, `truncate`, `when`), never by guessing. Do NOT add `.hover()` to `button` / `quiet_button` results.

- [ ] **Step 6: The progress bar on the selected sidebar row**

`render_sidebar` (the `Row::Session(row)` arm, :7924-7928): compute the bar for the selected row only and pass it:

```rust
                    Row::Session(row) => {
                        let (selected, session_id) =
                            (shown == Some(row.session_id), row.session_id);
                        // One lookup, for the selected row only.
                        let progress = selected.then(|| self.plans.fraction(session_id)).flatten();
                        let badge = self.agent_badge(&row.agent);
                        let row = session_row(row, badge, selected, progress, now, theme, cx);
```

`session_row`: add the parameter `progress: Option<f32>,` after `selected: bool,` and, in the selected row's column Task 9 built (name line + second line), append as its last child:

```rust
                .children(progress.map(|fraction| {
                    progress_bar(fraction, theme::PLAN_ROW_BAR_HEIGHT, theme)
                }))
```

If Task 9 only builds that column when a second line exists, build it also when `progress.is_some()` (the bar then sits alone under the name line). The row keeps its width: the bar is `w_full` inside the column. No other row, no tooltip, no click behavior changes.

Run: `cargo test -p mkrew-ui --lib -j 4`
Expected: PASS (pure tests; the render itself is checked by HUMAN_CHECKS, GPUI cannot run headless here).

- [ ] **Step 7: SPEC §10**

After the **Sidebar** bullet (SPEC.md:690-692) add:

```markdown
- **Plan progress:** the selected session row of a session with a linked plan shows a thin
  progress bar (done / total of its checklist) under its second line (ADR 0039).
```

In the Right panel bullet Task 15 added (grep `Right panel`), after its Changes card sentence add:

```markdown
**Plan card** (agent sessions, after Changes): `Plan <name> 6/20`, a progress bar and the
checklist of the linked file's `- [ ]` / `- [x]` lines: the first unchecked item highlighted
(two rows of context above it), at most 8 rows, `N more` for the rest. **Link plan…** (a file
picker) links a `.md` file; **Change…** and **Unlink** replace or remove it. The link is per
session and survives restarts (the resumed pane keeps it). The counts follow the file within
about a second (the daemon watches only that file); a file that cannot be read shows the
reason instead of the bar. There is no "Setup / Tasks / Finish" phase bar: progress comes
from the file only, for any agent (ADR 0039).
```

- [ ] **Step 8: HUMAN_CHECKS**

Append to the section `### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)` in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Plan card (item 8):** open the right panel on an agent pane → "Plan" with **Link plan…** →
  pick a `.md` with `- [ ]` / `- [x]` lines → the card shows `Plan <name> a/b`, a bar, up to 8
  rows with the first unchecked one in the accent color and `N more` below; the selected sidebar
  row shows the same bar under its second line, other rows none. Tick a box in the file with an
  editor (also one that saves through a temp file, e.g. Notepad or VS Code) → the counts and bar
  change within about a second, no click. A prose-only edit changes nothing on screen.
- [ ] **Plan link life:** **Change…** with a `.txt` file → an error line in the card, the old plan
  stays; **Unlink** → the card offers **Link plan…** again and the sidebar bar is gone; editing the
  file afterwards does nothing. Link, delete the file → the card shows "no such file" and keeps
  Change / Unlink; put the file back → the counts return. Link, restart the daemon and the UI →
  the resumed pane still shows the plan. Light and dark theme: the bar track and fill are visible.
```

- [ ] **Step 9: Verify and tick**

Run the full verification: `"C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. Tick inbox line 95 `[x]` (docs/INBOX.md:95, the last task of item 8).
Suggested message: `feat(ui): plan checklist card in the right panel and progress bar on the selected session row`

---

### Task 19: "Open in File Explorer" and "Open in Zed" menus (Files tree, workspace)

Inbox: line 101 `File navigation & Markdown preview` (first bullet; Tasks 19–21; tick at Task 21, do not tick yet). Spec: §10 (SPEC.md:728-729), §12A (:825-855).

**Current state (investigated):**
- GPUI `App::reveal_path(&self, path: &Path)` (`gpui-pre-0.3.7/src/app.rs:1712`) and `App::open_with_system(&self, path: &Path)` (:1717) exist. `Context<Shell>` derefs to `App`, so `cx.reveal_path(..)` works in `Shell` methods. On Windows (`gpui-pre-windows-0.3.7/src/platform.rs:707, :1327`) `reveal_path` runs on the background executor, needs a parent folder and selects the item with `SHOpenFolderAndSelectItems`. It selects a folder in its parent instead of opening it. `open_with_system` is `ShellExecuteW "open"` (:1305) and opens a folder in Explorer. macOS and Linux have the same two entry points. No new crate is needed.
- `main.rs` `workspace_file` (:2279) and `target_file` (:2292) build a path with `path.split('/').fold(base, |full, part| full.join(part))`. For `path == ""` (the tree root) this joins an empty part and gives `C:\proj\` with a trailing separator, which breaks `reveal_path`.
- The workspace menu is `render_workspace_menu` (:8092). It has `menu_item("open-in-zed")` (:8165), then `separator()`. `open_in_zed(project_id, cx)` (:4385) opens the project folder through `zed::open` and puts a failure in `sidebar_error`. `workspace_menu` is cleared in many places (`open_workspace_menu` :4351 sets it).
- The Files tree has no context menu. `file_row` (:15542) has `.on_click(..click_file..)` only. The tree container is `div().id("files-tree")` in `render_files` (:9459, child of the returned row). Rows live in a `uniform_list`, so the blank area below the rows is inside the list element and a right click there bubbles to `#files-tree`.
- Floating menus use `deferred(anchored().snap_to_window_with_margin(..).child(..))` (:8228). `anchored().position(point)` (gpui `elements/anchored.rs:47`) places it at a window position. `on_mouse_down_out` runs in the capture phase, so a right click on another row closes the old menu first and the row handler opens the new one.
- The Files target is `FilesView.worktree_task_id` (SPEC §20E.1). `target_file(project_id, target, path)` already returns the worktree folder (via `worktree_target::project_dir`).

**Files:**
- Modify: `crates/mkrew-ui/src/worktree_target.rs` (new `join_relative` after `project_dir` :77, test in `mod tests`)
- Modify: `crates/mkrew-ui/src/main.rs` (`workspace_file` :2279, `target_file` :2292, struct `FileMenu` before `struct Shell`, `Shell` fields `workspace_menu` :267 / init :646 / reset :577, `show_view` :1425, `open_workspace_menu` :4351, new methods after `open_in_zed` :4398, `render_workspace_menu` :8165, new `render_file_menu` after it, `render_files` :9583-9607, `file_row` :15588-15658)
- Modify: `docs/SPEC.md` §10 (:728), §12A (:848)
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces: `worktree_target::join_relative(base: PathBuf, rel: &str) -> PathBuf` (an empty `rel` or empty parts add nothing); `Shell.file_menu: Option<FileMenu>` with `struct FileMenu { project_id: Ulid, target: Option<Ulid>, path: String, is_dir: bool, position: gpui::Point<gpui::Pixels> }` (`path` relative to the folder the Files view shows, `""` = that folder).
- Consumes: `zed::open(&Path, Option<u32>) -> Result<(), ZedError>`, `Shell::target_file`, `menu_item(id: SharedString, theme) -> Stateful<Div>`.

- [ ] **Step 1: Failing test**

In `worktree_target.rs` `mod tests`, after the `project_dir` tests:

```rust
    /// The Files tree's root is `""`: Explorer must get the folder itself, not `proj\`.
    #[test]
    fn a_relative_path_joins_below_its_folder_and_empty_means_the_folder() {
        let base = PathBuf::from("proj");
        assert_eq!(
            join_relative(base.clone(), "src/a.rs"),
            Path::new("proj").join("src").join("a.rs")
        );
        assert_eq!(join_relative(base.clone(), "").as_os_str(), "proj");
        assert_eq!(join_relative(base, "src//a.rs/").as_os_str(), Path::new("proj").join("src").join("a.rs").as_os_str());
    }
```

Run: `cargo test -p mkrew-ui --lib worktree_target -j 4`
Expected: FAIL to compile (`cannot find function join_relative`).

- [ ] **Step 2: Implement `join_relative` and use it**

`worktree_target.rs`, after `project_dir`:

```rust
/// `rel` (`/`-separated, relative to `base`) below `base` with native separators. An empty
/// `rel` is `base` itself, without a trailing separator (the OS file manager needs that).
pub fn join_relative(base: PathBuf, rel: &str) -> PathBuf {
    rel.split('/')
        .filter(|part| !part.is_empty())
        .fold(base, |full, part| full.join(part))
}
```

`main.rs` `workspace_file`: replace the body of the `Some(..)`:

```rust
        Some(worktree_target::join_relative(
            PathBuf::from(&project.path),
            path,
        ))
```

`target_file`, last line:

```rust
        Some(worktree_target::join_relative(dir, path))
```

Run: `cargo test -p mkrew-ui --lib worktree_target -j 4`
Expected: PASS.

- [ ] **Step 3: The menu state and the actions (main.rs)**

Before `struct Shell` add:

```rust
/// The right-click menu of a Files tree row or of the tree itself (SPEC §12A): Open in File
/// Explorer, Open in Zed.
struct FileMenu {
    project_id: Ulid,
    /// The worktree the Files view showed when the menu opened (`None` = the project folder).
    target: Option<Ulid>,
    /// Relative to that folder, `/` separators; `""` = the folder itself.
    path: String,
    is_dir: bool,
    /// Window position of the click.
    position: gpui::Point<gpui::Pixels>,
}
```

`Shell`, after the `workspace_menu` field (:267):

```rust
    /// The open right-click menu of the Files tree.
    file_menu: Option<FileMenu>,
```

Init, after `workspace_menu: None,` (:646): `file_menu: None,`. In the disconnect reset, after `shell.workspace_menu = None;` (:577): `shell.file_menu = None;`.

`show_view` (:1425), as the first statement after the `let Some(project_id) ... else` block: `self.file_menu = None;`. `open_workspace_menu` (:4351), before `self.workspace_menu = Some(..)`: `self.file_menu = None;`.

After `open_in_zed` (:4398):

```rust
    /// "Open in File Explorer" on a workspace: opens its folder in the OS file manager.
    fn open_workspace_in_explorer(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        self.workspace_menu = None;
        if let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) {
            cx.open_with_system(Path::new(&project.path));
        }
        cx.notify();
    }

    /// Right click on a Files tree row (`path`) or on the tree itself (`""`).
    fn open_file_menu(
        &mut self,
        project_id: Ulid,
        path: String,
        is_dir: bool,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<'_, Self>,
    ) {
        let target = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        self.workspace_menu = None;
        self.file_menu = Some(FileMenu {
            project_id,
            target,
            path,
            is_dir,
            position,
        });
        cx.notify();
    }

    /// "Open in File Explorer" on a Files path: a file is shown selected in its folder, a
    /// folder is opened.
    fn open_path_in_explorer(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
        is_dir: bool,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        if let Some(full) = self.target_file(project_id, target, path) {
            if is_dir {
                cx.open_with_system(&full);
            } else {
                cx.reveal_path(&full);
            }
        }
        cx.notify();
    }

    /// "Open in Zed" on a Files path (a file or a folder); a failure shows in the sidebar,
    /// like the workspace menu's.
    fn open_path_in_zed(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: &str,
        cx: &mut Context<'_, Self>,
    ) {
        self.file_menu = None;
        if let Some(full) = self.target_file(project_id, target, path) {
            match zed::open(&full, None) {
                Ok(()) => self.sidebar_error = None,
                Err(error) => {
                    tracing::warn!(path = %full.display(), "open in Zed failed: {error}");
                    self.sidebar_error = Some(error.to_string());
                }
            }
        }
        cx.notify();
    }
```

After `render_workspace_menu` add the menu:

```rust
    /// The Files tree's right-click menu at the click position; a click outside closes it.
    fn render_file_menu(
        &self,
        menu: &FileMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let (project_id, target, is_dir) = (menu.project_id, menu.target, menu.is_dir);
        let (explorer_path, zed_path) = (menu.path.clone(), menu.path.clone());
        let item = |id: &'static str, label: &'static str| {
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
        };
        let panel = div()
            .id("file-menu")
            .w(px(theme::MENU_WIDTH))
            .flex()
            .flex_col()
            .p(px(space::XS))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.file_menu = None;
                cx.notify();
            }))
            .child(
                item("file-menu-explorer", "Open in File Explorer").on_click(cx.listener(
                    move |shell, _, _, cx| {
                        shell.open_path_in_explorer(project_id, target, &explorer_path, is_dir, cx);
                    },
                )),
            )
            .child(
                item("file-menu-zed", "Open in Zed").on_click(cx.listener(
                    move |shell, _, _, cx| {
                        shell.open_path_in_zed(project_id, target, &zed_path, cx);
                    },
                )),
            );
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }
```

`render_workspace_menu`, after the `open-in-zed` item and before its `.child(separator())` (:8180-8181), insert:

```rust
            .child(
                menu_item("open-in-explorer".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Open in File Explorer"),
                    )
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.open_workspace_in_explorer(project_id, cx);
                    })),
            )
```

`render_files`, on the `div().id("files-tree")` (:9590), after `.on_key_down(..)` add the root menu:

```rust
                    // Right click on the header or the blank part of the tree = the folder
                    // itself (rows stop the event first).
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |shell, event: &gpui::MouseDownEvent, _, cx| {
                            shell.open_file_menu(project_id, String::new(), true, event.position, cx);
                        }),
                    )
```

and after `.child(self.render_file(project_id, view, window, theme, cx))` (:9606):

```rust
            .children(
                (self.file_menu.as_ref())
                    .filter(|menu| menu.project_id == project_id)
                    .map(|menu| self.render_file_menu(menu, theme, cx)),
            )
```

`file_row`, before `div()` (after `let path = entry.path.clone();` :9588):

```rust
    let (menu_path, is_dir) = (entry.path.clone(), entry.is_dir);
```

and after the `.on_click(..)` (:15656-15658):

```rust
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |shell, event: &gpui::MouseDownEvent, _, cx| {
                shell.open_file_menu(project_id, menu_path.clone(), is_dir, event.position, cx);
                cx.stop_propagation();
            }),
        )
```

Run: `cargo check -p mkrew-ui -j 4`
Expected: compiles (the UI has no headless test; the pure part is covered in Step 1).

- [ ] **Step 4: SPEC**

§10, replace the **Open in Zed** bullet (SPEC.md:728-729) with:

```markdown
- **Open in Zed:** on task cards, the workspace menu, the Files tree menu and Docs; runs
  `zed <path>[:line]`; clear error with install hint if `zed` is missing.
- **Open in File Explorer:** in the workspace menu and in the Files tree's right-click menu
  (§12A). It opens the OS file manager (Explorer, Finder, the Linux file manager): a folder is
  opened, a file is shown selected in its folder.
```

§12A, after `- "Open in Zed" opens the file at the caret's line.` (:848):

```markdown
- **Right-click menu** on tree rows (files and folders) and on the blank part of the tree (the
  folder the view shows: the project, or the task worktree): "Open in File Explorer" and
  "Open in Zed" (§10). A click outside closes it.
```

- [ ] **Step 5: HUMAN_CHECKS**

Append to the `2026-10-10 inbox: redesign` section:

```markdown
- [ ] **Open in File Explorer / Zed:** right-click a file in the Files tree → "Open in File
  Explorer" opens Explorer with that file selected; on a folder row it opens the folder; on the
  blank part below the rows it opens the workspace folder (with a task worktree chosen: the
  worktree). "Open in Zed" opens Zed on the file or folder (an error shows in the sidebar if `zed`
  is missing). A click outside closes the menu; right-clicking another row moves it. Right-click
  a workspace in the sidebar → "Open in File Explorer" opens its folder.
```

- [ ] **Step 6: Verify and tick**

Run the full verification. Do not tick inbox line 101 yet (Task 21 does).
Suggested message: `feat(ui): Open in File Explorer and Open in Zed in the Files tree and workspace menus`

---

### Task 20: Ctrl+Click on paths and URLs in terminals

Inbox: line 101 `File navigation & Markdown preview` (second and fourth bullets; do not tick yet). Spec: §10 (SPEC.md:717-729), §12A (:825-855).

**Current state (investigated):**
- `TerminalPane` (`terminal/view.rs:42`) has `mouse_down` (:303: focus, `start_selection`), `mouse_move` (:320, returns early unless a drag selects), `cell_at` (:291: window position → `(row, col, Side)` through `Geometry` set in `prepaint` :728-738) and `.cursor(CursorStyle::IBeam)` (:648). It knows neither project nor folder, and emits no events.
- The visible rows are `Emulator::shared_grid()` (`emulator.rs:333`): an `Arc<Grid>` rebuilt only after a change, `Grid.rows: Vec<Vec<GridCell>>` (one cell per column; a wide char's second cell has `Flags::WIDE_CHAR_SPACER`). The scrollback view is already in it (`display_offset`). Reading one row of it adds no parsing.
- GPUI: `Modifiers::secondary()` (`platform/keystroke.rs:483`: Cmd on macOS, Ctrl elsewhere); `InteractiveElement::on_modifiers_changed` (`elements/div.rs:1176`, a window-wide listener, so it fires for every pane); `Window::mouse_position()` (`window.rs:3196`); `CursorStyle::PointingHand`; `Context::subscribe_in` (`app/context.rs:341`).
- Pane creation: `Shell::attach` (`main.rs:4244`) builds `TerminalPane::new` inside `this.update_in` (:4278). The history dialog's replay pane (`TerminalPane::replay`, :5603) is read-only and never attached.
- A session knows `project_id` and `task_id` (`Session`, `message.rs:473`); a task's `worktree_path` is its worktree. The Files view of that session's folder is `FilesView` with `worktree_task_id == task_id` (SPEC §20E.1); `worktree_target::project_dir(project_path, worktree_root)` is the folder inside the worktree.
- Files view: `reveal_file` (:3894) → `FilesView::reveal(path)` (`files_view.rs:323`), `open_revealed` (:3874) asks about unsaved text (`leave_file`), `read_file` (:1753) completes with `FilesView::read` (:355). There is no line: `Shown.editor.set_cursor(Pos, false)` + `file_code_scroll.scroll_to_item(line, ScrollStrategy::Center)` (:1954) is how find puts a match in view. `UniformListScrollHandle::scroll_to_item` is deferred to the next prepaint, so it holds until the list is drawn.
- `set_files_target` (:1656) starts the view over on another worktree (asks `leave_file` first when something is unsaved).
- `tempfile` is a dev-dependency of `mkrew-ui`; no new crate.

**Decisions** (defaults; see open questions):
- Detected: `http://` / `https://` URLs; file paths (relative or absolute, `/` or `\`, drive `C:\…`) with optional `:line` or `:line:col`. Not detected: `file(12)` (MSVC style: the path before `(` is a link without a line), links wrapped over two rows, `~/…`, `file://` URLs.
- A path is a link only if it names an existing **file inside the pane's folder** (the project folder, or the worktree folder of the pane's task). The check runs when the pointer moves to another cell with Ctrl/Cmd held, never otherwise. Other paths get no underline and Ctrl+Click behaves like a plain click.
- The column of `:line:col` is parsed (so it is not part of the path) and dropped: the Files editor puts the caret at column 0 of the line.
- A `.md` file opened at a line shows the editor (Task 21 adds the preview and the rule).

**Files:**
- Create: `crates/mkrew-ui/src/terminal/links.rs` (pure detector + `resolve`, with tests)
- Modify: `crates/mkrew-ui/src/terminal.rs` (`pub mod links;`), `crates/mkrew-ui/src/terminal/emulator.rs` (`Grid::row_chars`, test), `crates/mkrew-ui/src/terminal/view.rs`, `crates/mkrew-ui/src/files_view.rs` (`reveal(path, line)`, `pending_line`, `apply_pending_line`, tests), `crates/mkrew-ui/src/main.rs` (`attach` :4278, `read_file` :1784, `open_revealed` :3874, `reveal_file` :3894, `open_quick_pick` :3868, new `link_scope`, `open_terminal_link`, `open_file_at`, `show_pending_line`), `docs/SPEC.md` §10, §12A, `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces (`terminal::links`): `pub const SPACER: char = '\0'`; `pub enum Target { Url(String), Path { path: String, line: Option<u32> } }`; `pub struct Link { pub start: usize, pub end: usize /* exclusive */, pub target: Target }`; `pub fn link_at(cells: &[char], col: usize) -> Option<Link>`; `pub fn resolve(base: &Path, path: &str) -> Option<String>` (the Files path of an existing file inside `base`, `/` separators).
- Produces: `Grid::row_chars(&self, row: usize) -> Option<Vec<char>>`; `terminal::view::OpenLink { Url(String), File { path: String, line: Option<u32> } }` (an event of `TerminalPane`); `TerminalPane::set_links(&mut self, base: Option<PathBuf>)`.
- Produces: `FilesView::reveal(&mut self, path: &str, line: Option<u32>) -> Requests`; `FilesView::apply_pending_line(&mut self) -> Option<usize>` (the 0-based line to scroll to); `Shell::open_file_at(project_id: Ulid, target: Option<Ulid>, path: String, line: Option<u32>, window, cx)`. Task 21 consumes `reveal(.., line)`.
- Consumes: `Shell::files_at`, `set_files_target`, `leave_file`, `open_revealed`, `worktree_target::project_dir`.

- [ ] **Step 1: Failing tests for the detector**

Create `crates/mkrew-ui/src/terminal/links.rs` with only this (the implementation follows in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn chars(row: &str) -> Vec<char> {
        row.chars().collect()
    }

    /// The link under the first char of `needle` in the ASCII `row`.
    fn target(row: &str, needle: &str) -> Option<Target> {
        let col = row[..row.find(needle)?].chars().count();
        link_at(&chars(row), col).map(|link| link.target)
    }

    fn url(text: &str) -> Target {
        Target::Url(text.into())
    }

    fn path(text: &str, line: Option<u32>) -> Target {
        Target::Path {
            path: text.into(),
            line,
        }
    }

    #[test]
    fn urls_keep_their_query_and_lose_trailing_punctuation() {
        assert_eq!(
            target("see https://example.com/a?b=1&c=2#top, then", "https"),
            Some(url("https://example.com/a?b=1&c=2#top"))
        );
        assert_eq!(
            target("(https://example.com/x).", "https"),
            Some(url("https://example.com/x"))
        );
        assert_eq!(
            target("http://localhost:5173/", "http"),
            Some(url("http://localhost:5173/"))
        );
        // A bracket pair inside the URL stays.
        let wiki = "https://en.wikipedia.org/wiki/Rust_(language)";
        assert_eq!(target(wiki, "https"), Some(url(wiki)));
        // Markdown: the link is the URL, not its label.
        let row = "[docs](https://x.dev/a)";
        assert_eq!(target(row, "https"), Some(url("https://x.dev/a")));
        assert_eq!(target(row, "docs"), None);
        // The scheme alone is nothing.
        assert_eq!(target("https:// x", "https"), None);
    }

    #[test]
    fn paths_carry_an_optional_line_and_column() {
        assert_eq!(
            target("at ./src/a.rs:3:5 now", "./src"),
            Some(path("./src/a.rs", Some(3)))
        );
        assert_eq!(
            target("crates/x/src/main.rs:123", "crates"),
            Some(path("crates/x/src/main.rs", Some(123)))
        );
        assert_eq!(
            target("open Cargo.toml please", "Cargo"),
            Some(path("Cargo.toml", None))
        );
        assert_eq!(
            target("/home/u/a.rs:7", "/home"),
            Some(path("/home/u/a.rs", Some(7)))
        );
    }

    #[test]
    fn windows_paths_keep_their_drive_colon() {
        assert_eq!(
            target(r"C:\x\y.rs:12", "C:"),
            Some(path(r"C:\x\y.rs", Some(12)))
        );
        assert_eq!(
            target(r"error in C:\x\y.rs:12:5: bad", "C:"),
            Some(path(r"C:\x\y.rs", Some(12)))
        );
        assert_eq!(target("C:/x/y.rs", "C:"), Some(path("C:/x/y.rs", None)));
        // Anywhere in the token is the link.
        assert_eq!(target(r"C:\x\y.rs", "y.rs"), Some(path(r"C:\x\y.rs", None)));
    }

    #[test]
    fn quotes_brackets_and_punctuation_are_not_part_of_the_path() {
        assert_eq!(
            target(r#"read "src/a.rs" and"#, "src"),
            Some(path("src/a.rs", None))
        );
        assert_eq!(
            target("(src/a.rs:3)", "src"),
            Some(path("src/a.rs", Some(3)))
        );
        // `file(12)` is not detected as a line: the path before the bracket is the link.
        assert_eq!(
            target("src/a.rs(12): error", "src"),
            Some(path("src/a.rs", None))
        );
        assert_eq!(target("see `./a.rs`, or", "./a"), Some(path("./a.rs", None)));
        assert_eq!(target("[src/a.rs]", "src"), Some(path("src/a.rs", None)));
        assert_eq!(target("in src/a.rs.", "src"), Some(path("src/a.rs", None)));
    }

    #[test]
    fn a_link_covers_its_cells_wherever_the_pointer_is() {
        let row = chars("see src/a.rs:3 now");
        let expected = Link {
            start: 4,
            end: 14,
            target: path("src/a.rs", Some(3)),
        };
        for col in 4..14 {
            assert_eq!(link_at(&row, col), Some(expected.clone()), "col {col}");
        }
        for col in [0, 2, 3, 14, 15, 17, 40] {
            assert_eq!(link_at(&row, col), None, "col {col}");
        }
    }

    #[test]
    fn words_numbers_and_flags_are_not_links() {
        for row in [
            "hello", "v1.2", "1.5", "10:30:45", "--flag", "key:value", "a-b", "...", ".",
        ] {
            assert_eq!(link_at(&chars(row), 0), None, "{row}");
        }
    }

    #[test]
    fn wide_chars_take_two_cells_and_none_of_the_text() {
        // "日本 src/a.rs": every wide char is followed by its spacer cell.
        let row = [
            '日', SPACER, '本', SPACER, ' ', 's', 'r', 'c', '/', 'a', '.', 'r', 's',
        ];
        assert_eq!(
            link_at(&row, 11),
            Some(Link {
                start: 5,
                end: 13,
                target: path("src/a.rs", None)
            })
        );
        let name = ['日', SPACER, '.', 'r', 's'];
        assert_eq!(
            link_at(&name, 0),
            Some(Link {
                start: 0,
                end: 5,
                target: path("日.rs", None)
            })
        );
        // The spacer of a wide char is the char.
        assert_eq!(link_at(&name, 1), link_at(&name, 0));
    }

    #[test]
    fn only_existing_files_inside_the_folder_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("proj");
        std::fs::create_dir_all(base.join("src")).unwrap();
        std::fs::write(base.join("src").join("a.rs"), "x").unwrap();
        std::fs::write(dir.path().join("outside.txt"), "x").unwrap();
        assert_eq!(resolve(&base, "src/a.rs").as_deref(), Some("src/a.rs"));
        assert_eq!(
            resolve(&base, "./src/../src/a.rs").as_deref(),
            Some("src/a.rs")
        );
        let absolute = base.join("src").join("a.rs");
        assert_eq!(
            resolve(&base, absolute.to_str().unwrap()).as_deref(),
            Some("src/a.rs")
        );
        // A folder, a missing file and a file outside the folder are not links.
        assert_eq!(resolve(&base, "src"), None);
        assert_eq!(resolve(&base, "src/missing.rs"), None);
        assert_eq!(resolve(&base, "../outside.txt"), None);
        let outside = dir.path().join("outside.txt");
        assert_eq!(resolve(&base, outside.to_str().unwrap()), None);
    }
}
```

`terminal.rs`: add `pub mod links;` after `pub mod layout;` (the module docs line "the GPUI view" stays).

Run: `cargo test -p mkrew-ui --lib terminal::links -j 4`
Expected: FAIL to compile (`cannot find function link_at`, `Target`, `Link`, `SPACER`, `resolve`).

- [ ] **Step 2: Implement the detector**

At the top of `links.rs`, above `mod tests`:

```rust
//! Links in a terminal row (SPEC §10): `http(s)://` URLs and file paths (relative or absolute,
//! with an optional `:line` or `:line:col`) under the pointer. Pure and tested headlessly. The
//! pane reads ONE row of its cached grid while Ctrl / Cmd is held and calls [`link_at`], then
//! [`resolve`] checks that a path is a file of the pane's folder; nothing runs while output is
//! parsed or painted.

use std::path::Path;

/// Stands for the second cell of a wide char in a row of chars (one char per cell), so that a
/// column is an index.
pub const SPACER: char = '\0';

/// What a link points to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Url(String),
    /// A path as typed (not checked yet) and its 1-based line.
    Path { path: String, line: Option<u32> },
}

/// A link and the cells it covers: `start..end`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub target: Target,
}

/// Ends a link on both sides: blanks and the quotes and angle brackets that wrap one.
fn breaks(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '<' | '>' | '|')
}

/// The link under cell `col` of `cells`, if any.
pub fn link_at(cells: &[char], col: usize) -> Option<Link> {
    if breaks(*cells.get(col)?) {
        return None;
    }
    let mut start = col;
    while start > 0 && !breaks(cells[start - 1]) {
        start -= 1;
    }
    let mut end = col + 1;
    while end < cells.len() && !breaks(cells[end]) {
        end += 1;
    }
    let link = url(cells, start, end).or_else(|| path(cells, start, end))?;
    (link.start <= col && col < link.end).then_some(link)
}

fn text(cells: &[char]) -> String {
    cells.iter().filter(|c| **c != SPACER).collect()
}

fn starts_with_scheme(cells: &[char]) -> bool {
    ["http://", "https://"].iter().any(|scheme| {
        cells.len() >= scheme.len()
            && (cells.iter().zip(scheme.chars())).all(|(cell, expected)| {
                cell.eq_ignore_ascii_case(&expected)
            })
    })
}

/// The URL inside the token `start..end` (a Markdown `[label](https://…)` token holds one).
fn url(cells: &[char], start: usize, end: usize) -> Option<Link> {
    let scheme = (0..end - start).find(|&i| starts_with_scheme(&cells[start + i..end]))?;
    let start = start + scheme;
    let end = trim_end(cells, start, end);
    let text = text(&cells[start..end]);
    // Something after "://".
    (text.len() > text.find("://")? + 3).then(|| Link {
        start,
        end,
        target: Target::Url(text),
    })
}

/// The path inside the token `start..end`.
fn path(cells: &[char], start: usize, end: usize) -> Option<Link> {
    let mut start = start;
    while start < end && matches!(cells[start], '(' | '[' | '{') {
        start += 1;
    }
    // "src/a.rs(12)": a bracket ends the path.
    let end = (start..end)
        .find(|&i| matches!(cells[i], '(' | '[' | '{'))
        .unwrap_or(end);
    let end = trim_end(cells, start, end);
    let typed = text(&cells[start..end]);
    let (path, line) = split_line(&typed);
    looks_like_path(path).then(|| Link {
        start,
        end,
        target: Target::Path {
            path: path.to_owned(),
            line,
        },
    })
}

/// `end` moved left over what ends a sentence rather than a link: `.,;:!?` and a closing
/// bracket without its opening one inside the link.
fn trim_end(cells: &[char], start: usize, mut end: usize) -> usize {
    let unbalanced = |open: char, close: char| {
        let token = &cells[start..end];
        token.iter().filter(|c| **c == close).count() > token.iter().filter(|c| **c == open).count()
    };
    while end > start {
        let drop = match cells[end - 1] {
            '.' | ',' | ';' | ':' | '!' | '?' => true,
            ')' => unbalanced('(', ')'),
            ']' => unbalanced('[', ']'),
            '}' => unbalanced('{', '}'),
            _ => false,
        };
        if !drop {
            break;
        }
        end -= 1;
    }
    end
}

/// `text` without a trailing `:line` or `:line:col`, and the line.
fn split_line(text: &str) -> (&str, Option<u32>) {
    let Some((rest, last)) = number_suffix(text) else {
        return (text, None);
    };
    match number_suffix(rest) {
        Some((path, line)) => (path, Some(line)),
        None => (rest, Some(last)),
    }
}

fn number_suffix(text: &str) -> Option<(&str, u32)> {
    let (head, digits) = text.rsplit_once(':')?;
    if head.is_empty() || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((head, digits.parse().ok()?))
}

/// A path has a separator, or a name with an extension (`Cargo.toml`) and no more colons
/// than a drive's; `v1.2`, `1.5` and `key:value` are not paths.
fn looks_like_path(path: &str) -> bool {
    let body = match path.as_bytes() {
        [drive, b':', sep, ..] if drive.is_ascii_alphabetic() && matches!(sep, b'\\' | b'/') => {
            &path[2..]
        }
        _ => path,
    };
    if body.is_empty() || body.contains(':') {
        return false;
    }
    if body.contains(['/', '\\']) {
        return body.chars().any(char::is_alphanumeric);
    }
    body.rsplit_once('.').is_some_and(|(_, ext)| {
        (1..=8).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
            && ext.chars().any(|c| c.is_ascii_alphabetic())
    })
}

/// The Files path (`/` separators, relative to `base`) of the file that `path` names, typed
/// in a terminal running in `base`; `None` unless it exists, is a file, and lies inside
/// `base` (a folder, a missing file or a file elsewhere is no link). Reads the disk: call it
/// when the pointer moves to another cell, never per frame.
pub fn resolve(base: &Path, path: &str) -> Option<String> {
    let typed = Path::new(path);
    let full = if typed.is_absolute() {
        typed.to_path_buf()
    } else {
        base.join(typed)
    };
    let full = full.canonicalize().ok()?;
    if !full.is_file() {
        return None;
    }
    let rel = full.strip_prefix(base.canonicalize().ok()?).ok()?;
    let parts: Vec<_> = rel.components().map(|part| part.as_os_str().to_string_lossy()).collect();
    Some(parts.join("/"))
}
```

- [ ] **Step 3: Run the detector tests**

Run: `cargo test -p mkrew-ui --lib terminal::links -j 4`
Expected: PASS (7 tests). If `a_link_covers_its_cells…` fails on `end`, re-check `trim_end` (the `:3` stays, the space ends the token).

- [ ] **Step 4: `Grid::row_chars` (test first)**

`terminal/emulator.rs` `mod tests`, after `wide_chars_dump_once`:

```rust
    /// Link detection indexes cells by column: one char per cell, a wide char's second cell
    /// is the spacer.
    #[test]
    fn a_row_has_one_char_per_cell() {
        let mut emu = Emulator::new(8, 1);
        emu.advance("a日b".as_bytes());
        let grid = emu.grid();
        assert_eq!(
            grid.row_chars(0),
            Some(vec!['a', '日', SPACER, 'b', ' ', ' ', ' ', ' '])
        );
        assert_eq!(grid.row_chars(1), None);
    }
```

Run: `cargo test -p mkrew-ui --lib terminal::emulator -j 4`
Expected: FAIL to compile (`no method row_chars`, `SPACER`).

In `emulator.rs` add `use crate::terminal::links::SPACER;` next to `use crate::terminal::input::InputMode;`, and in `impl Grid` after `to_text`:

```rust
    /// The chars of screen row `row`, one per cell (a wide char's second cell is
    /// [`SPACER`]), for link detection; `None` past the last row.
    pub fn row_chars(&self, row: usize) -> Option<Vec<char>> {
        let spacers = Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER;
        let cells = self.rows.get(row)?;
        Some(
            (cells.iter())
                .map(|cell| if cell.flags.intersects(spacers) { SPACER } else { cell.c })
                .collect(),
        )
    }
```

Run: `cargo test -p mkrew-ui --lib terminal::emulator -j 4`
Expected: PASS.

- [ ] **Step 5: Failing tests for "open at a line" in `FilesView`**

`files_view.rs` `mod tests`: the three existing `view.reveal("…")` calls (`revealing_a_file_opens_its_folders_and_shows_it`, :890, :904, :906) become `view.reveal("…", None)`. Add after that test:

```rust
    /// Ctrl+Click on `README.md:3`: the caret goes to the line once the read answers.
    #[test]
    fn a_link_line_waits_for_the_read_and_puts_the_caret_there() {
        let mut view = listed();
        let requests = view.reveal("README.md", Some(3));
        assert_eq!(requests.read.as_deref(), Some("README.md"));
        assert_eq!(view.apply_pending_line(), None, "still loading");
        view.begin_read();
        view.read("README.md", file_text("a\nb\nc\nd\n"));
        assert_eq!(view.apply_pending_line(), Some(2));
        assert_eq!(view.line(), Some(3));
        assert_eq!(view.apply_pending_line(), None, "applied once");
    }

    #[test]
    fn a_line_past_the_end_lands_on_the_last_line() {
        let mut view = opened("a\nb");
        view.reveal("README.md", Some(99));
        assert_eq!(view.apply_pending_line(), Some(1));
        assert_eq!(view.line(), Some(2));
    }

    /// The file is already shown: nothing is read, the line applies at once, and unsaved text
    /// stays.
    #[test]
    fn a_line_in_the_shown_file_applies_at_once_and_keeps_unsaved_text() {
        let mut view = opened("a\nb\nc");
        view.editor_mut().unwrap().insert("x");
        assert_eq!(view.reveal("README.md", Some(2)), Requests::default());
        assert_eq!(view.apply_pending_line(), Some(1));
        assert_eq!(view.line(), Some(2));
        assert_eq!(text(&view), "xa\nb\nc");
        assert!(view.unsaved().is_some());
    }

    #[test]
    fn a_failed_read_drops_the_line() {
        let mut view = listed();
        view.reveal("README.md", Some(5));
        view.begin_read();
        view.read("README.md", Err("gone".into()));
        assert_eq!(view.apply_pending_line(), None);
        // Retry shows the file from the top.
        assert_eq!(view.open("README.md").as_deref(), Some("README.md"));
        view.begin_read();
        view.read("README.md", file_text("a\nb\n"));
        assert_eq!(view.apply_pending_line(), None);
        assert_eq!(view.line(), Some(1));
    }
```

Run: `cargo test -p mkrew-ui --lib files_view -j 4`
Expected: FAIL to compile (`reveal` takes 1 argument, `apply_pending_line` missing).

- [ ] **Step 6: Implement it in `FilesView`**

Field, after `read_again: bool,` (:128):

```rust
    /// A line (1-based) that a link asked for, put under the caret once the file is shown.
    pending_line: Option<u32>,
```

`new`: `pending_line: None,` after `read_again: false,`. `open` (:311): after `self.content = Content::Loading;` add `self.pending_line = None;`. Replace `reveal`:

```rust
    /// Opens file `path` from outside the tree (quick open, a terminal link): its folders open
    /// (listed as needed), the cursor moves to it and it shows in the viewer; with `line`, the
    /// caret goes there once the file is shown ([`Self::apply_pending_line`]).
    pub fn reveal(&mut self, path: &str, line: Option<u32>) -> Requests {
        let mut list = Vec::new();
        let mut dir = String::new();
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        for part in parts {
            if !dir.is_empty() {
                dir.push('/');
            }
            dir.push_str(part);
            list.extend(self.expand(&dir).list);
        }
        self.cursor = Some(path.to_owned());
        let read = self.open(path);
        self.pending_line = line;
        Requests { list, read }
    }

    /// Puts the caret on the line [`Self::reveal`] was given, once the file is shown (call it
    /// after every read and after `reveal`). Returns the 0-based line to scroll to, clamped to
    /// the text; `None` while nothing is shown or nothing is pending.
    pub fn apply_pending_line(&mut self) -> Option<usize> {
        let Content::Shown(shown) = &mut self.content else {
            return None;
        };
        let line = usize::try_from(self.pending_line.take()?)
            .ok()?
            .saturating_sub(1)
            .min(shown.editor.lines().len().saturating_sub(1));
        shown.editor.set_cursor(Pos::new(line, 0), false);
        Some(line)
    }
```

Run: `cargo test -p mkrew-ui --lib files_view -j 4`
Expected: PASS (all files_view tests).

- [ ] **Step 7: The pane (`terminal/view.rs`)**

Imports: add `EventEmitter, ModifiersChangedEvent` to the `gpui::{..}` list, `use std::path::PathBuf;`, `use crate::terminal::links::{self, Target};`.

After `struct Geometry` add:

```rust
/// Ctrl+Click (Cmd+Click on macOS) on a link of the terminal: the shell opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenLink {
    Url(String),
    /// A file of the pane's folder: its Files path (`/` separators) and the line to show.
    File { path: String, line: Option<u32> },
}

impl EventEmitter<OpenLink> for TerminalPane {}

/// The link under the pointer while Ctrl / Cmd is held: its row and cells, for the underline.
#[derive(Clone, Debug, PartialEq, Eq)]
struct HoverLink {
    row: usize,
    start: usize,
    end: usize,
    open: OpenLink,
}
```

`TerminalPane` fields, after `ime_marked`:

```rust
    /// `None` = no links (the history's replay pane). `Some(base)`: URLs, and with a `base`
    /// (the project folder, or the task worktree of the pane's agent) file paths.
    links: Option<Option<PathBuf>>,
    /// The link under the pointer while Ctrl / Cmd is held, and the cell it was looked up for
    /// (the row is read again only when the pointer moves to another cell).
    hover_link: Option<HoverLink>,
    probed: Option<(usize, usize)>,
```

`with_emulator`: `links: None, hover_link: None, probed: None,` after `ime_marked: Rc::default(),`. After `session_id()`:

```rust
    /// Turns on Ctrl+Click links; `base` is where typed relative paths start (`None` = URLs only).
    pub fn set_links(&mut self, base: Option<PathBuf>) {
        self.links = Some(base);
    }
```

Before `mouse_down` add:

```rust
    /// The cell under `position` when it is inside the painted grid.
    fn cell_inside(&self, position: Point<Pixels>) -> Option<(usize, usize)> {
        let geometry = self.geometry?;
        let at = position - geometry.origin;
        let (cols, rows) = self.emulator.size();
        let inside = at.x >= px(0.)
            && at.y >= px(0.)
            && at.x < geometry.cell_width * cols as f32
            && at.y < geometry.line_height * rows as f32;
        let (row, col, _) = self.cell_at(position).filter(|_| inside)?;
        Some((row, col))
    }

    /// The link at a cell, read from that row of the cached grid only.
    fn link_in(&self, row: usize, col: usize) -> Option<HoverLink> {
        let base = self.links.as_ref()?;
        let cells = self.emulator.shared_grid().row_chars(row)?;
        let link = links::link_at(&cells, col)?;
        let open = match link.target {
            Target::Url(url) => OpenLink::Url(url),
            Target::Path { path, line } => OpenLink::File {
                path: links::resolve(base.as_deref()?, &path)?,
                line,
            },
        };
        Some(HoverLink {
            row,
            start: link.start,
            end: link.end,
            open,
        })
    }

    /// Ctrl / Cmd held (or not) with the pointer at `position`: finds the link under it, or
    /// drops the one shown. One row is read, and only when the pointer moved to another cell,
    /// so holding the key costs nothing while output streams; without the key nothing runs.
    fn update_link(&mut self, held: bool, position: Point<Pixels>, cx: &mut Context<'_, Self>) {
        if self.links.is_none() {
            return;
        }
        let cell = if held { self.cell_inside(position) } else { None };
        if cell == self.probed {
            return;
        }
        self.probed = cell;
        let found = cell.and_then(|(row, col)| self.link_in(row, col));
        if found != self.hover_link {
            self.hover_link = found;
            cx.notify();
        }
    }
```

`mouse_down`: after the `find_clicked` check, before `window.focus(..)`:

```rust
        // Ctrl+Click (Cmd+Click) on a link opens it: no selection starts, and the pane does
        // not take the keyboard from the view the link opens.
        if event.modifiers.secondary()
            && let Some(link) = &self.hover_link
        {
            cx.emit(link.open.clone());
            return;
        }
```

`mouse_move` first statement:

```rust
        self.update_link(event.modifiers.secondary(), event.position, cx);
```

`render`: replace `.cursor(CursorStyle::IBeam)` and add the modifier listener right after it:

```rust
            .cursor(if self.hover_link.is_some() {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            })
            // Pressing or releasing Ctrl / Cmd with the pointer at rest (every pane gets this).
            .on_modifiers_changed(cx.listener(
                |pane, event: &ModifiersChangedEvent, window, cx| {
                    pane.update_link(event.modifiers.secondary(), window.mouse_position(), cx);
                },
            ))
```

`TerminalElement::paint`, right after the `for (index, cells)` loop (before `let pane = self.pane.read(cx);`):

```rust
        // The link under the pointer: one underline in the accent color, outside the row cache.
        if let Some(link) = &self.pane.read(cx).hover_link
            && link.row < grid.rows.len()
        {
            let thickness = px(theme.terminal_underline_thickness);
            let underline = Bounds::new(
                point(
                    origin.x + cell_width * link.start as f32,
                    origin.y + line_height * (link.row + 1) as f32 - thickness,
                ),
                size(cell_width * (link.end - link.start) as f32, thickness),
            );
            window.paint_quad(fill(underline, hsla(theme.accent)));
        }
```

Run: `cargo check -p mkrew-ui -j 4`
Expected: compiles.

- [ ] **Step 8: The shell opens links (`main.rs`)**

`use mkrew_ui::terminal::view::{OpenLink, TerminalPane};` (replace the import at :80).

New methods near `open_revealed`:

```rust
    /// Where links typed in `session`'s terminal point: its project, the task worktree its
    /// agent runs in (`None` = the project folder) and that folder.
    fn link_scope(&self, session: Ulid) -> Option<(Ulid, Option<Ulid>, PathBuf)> {
        let session = self.sidebar.sessions().iter().find(|s| s.id == session)?;
        let project = (self.sidebar.projects().iter()).find(|p| p.id == session.project_id)?;
        let project_dir = PathBuf::from(&project.path);
        let worktree = session
            .task_id
            .and_then(|id| self.board.task(id))
            .and_then(|task| Some((task.id, task.worktree_path.as_deref()?)));
        Some(match worktree {
            Some((task, root)) => (
                project.id,
                Some(task),
                worktree_target::project_dir(&project_dir, Path::new(root)),
            ),
            None => (project.id, None, project_dir),
        })
    }

    /// Ctrl+Click on a terminal link: a URL opens in the browser, a file in the Files view.
    fn open_terminal_link(
        &mut self,
        session: Ulid,
        link: &OpenLink,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match link {
            OpenLink::Url(url) => cx.open_url(url),
            OpenLink::File { path, line } => {
                let Some((project_id, target, _)) = self.link_scope(session) else {
                    return;
                };
                self.open_file_at(project_id, target, path.clone(), *line, window, cx);
            }
        }
    }

    /// Shows `path` of the Files view on `target` (the project, or a task worktree) with the
    /// caret on `line`. Another target starts the view over, so unsaved text is asked about
    /// first (as the chooser does).
    fn open_file_at(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).worktree_task_id != target {
            if self.files_view(project_id).unsaved().is_some() {
                let again: AfterSave = Box::new(move |shell, window, cx| {
                    shell.open_file_at(project_id, target, path, line, window, cx);
                });
                return self.leave_file(project_id, again, window, cx);
            }
            self.set_files_target(project_id, target, window, cx);
        }
        self.open_revealed(project_id, target, path, line, window, cx);
    }

    /// Puts the caret on the line a link asked for and scrolls there, once the file is shown
    /// (after `reveal` when it already was, after the read otherwise).
    fn show_pending_line(&mut self, project_id: Ulid, target: Option<Ulid>) {
        if let Some(line) = (self.files_at(project_id, target)).and_then(FilesView::apply_pending_line) {
            self.file_code_scroll
                .scroll_to_item(line, ScrollStrategy::Center);
        }
    }
```

`open_revealed` (:3874) gets `line: Option<u32>` after `path: String`; its continuation and the final call pass it:

```rust
        let leaves =
            (self.files_at(project_id, target)).is_some_and(|view| view.leaves_unsaved(&path));
        if leaves {
            let reveal: AfterSave = Box::new(move |shell, window, cx| {
                shell.open_revealed(project_id, target, path, line, window, cx);
            });
            return self.leave_file(project_id, reveal, window, cx);
        }
        self.reveal_file(project_id, target, path, line, window, cx);
```

`reveal_file` (:3894) gets `line: Option<u32>` after `path`; `let requests = view.reveal(&path, line);` and, after `self.files_requests(project_id, requests, window, cx);`:

```rust
        self.show_pending_line(project_id, target);
```

`open_quick_pick` (:3868): `self.open_revealed(project_id, open.worktree_task_id, path, None, window, cx);`.

`read_file` (:1784-1795), inside the `update_in` closure after the `again` handling:

```rust
                shell.show_pending_line(project_id, target);
```

(place it after the `if let Some(path) = again { .. }` block and before `cx.notify();`).

`attach` (:4276-4287): the pane gets its folder and the shell listens:

```rust
                let snapshot = buffered.snapshot.unwrap_or(attached.snapshot);
                let painted = shell.painted.clone();
                let base = shell.link_scope(session).map(|(_, _, dir)| dir);
                let pane = cx.new(|cx| {
                    let mut pane =
                        TerminalPane::new(session, &snapshot, connection, painted, window, cx);
                    pane.set_links(base);
                    pane.feed(&buffered.output);
                    if let Some(exit_code) = buffered.exited {
                        pane.set_exited(exit_code);
                    }
                    pane
                });
                cx.subscribe_in(&pane, window, move |shell, _, link: &OpenLink, window, cx| {
                    shell.open_terminal_link(session, link, window, cx);
                })
                .detach();
                shell.panes.insert(session, pane);
```

Run: `cargo check -p mkrew-ui -j 4`
Expected: compiles; then `cargo test -p mkrew-ui --lib -j 4` Expected: PASS.

- [ ] **Step 9: SPEC**

§10, after the **Open in File Explorer** bullet added by Task 19:

```markdown
- **Terminal links:** while Ctrl (Cmd on macOS) is held over a terminal pane (agents and
  shells) the link under the pointer is underlined and the pointer is a hand; Ctrl+Click
  (Cmd+Click) opens it. Links are `http://` / `https://` URLs (opened in the browser) and file
  paths, relative (`./src/a.rs`, `crates/x/main.rs`) or absolute (`/home/u/a.rs`,
  `C:\x\a.rs`), with an optional `:line` or `:line:col`. A path is a link only if it names an
  existing file inside the pane's folder (the project folder, or the worktree of the pane's
  task); it opens in the Files view (§12A) with the caret on that line. Trailing `.,;:!?`,
  unbalanced closing brackets and quotes are not part of a link. `file(12)` (the path before
  the bracket is the link, without the line), `~/…` paths and links wrapped over two rows are
  not detected. Perf: the row text comes from the cached grid, for the hovered row only, while
  the key is held and the pointer moves to another cell; nothing runs while output is parsed
  or painted.
```

§12A, after the **Right-click menu** bullet (Task 19):

```markdown
- **Open at a line:** Ctrl+Click on a file path in a terminal (§10) shows that file with the
  caret on the line, scrolled into view. A path in another task's worktree switches the view to
  that worktree (asking about unsaved text first).
```

- [ ] **Step 10: HUMAN_CHECKS**

```markdown
- [ ] **Ctrl+Click links:** in an agent pane or a shell, `echo https://example.com/a?b=1.` then
  hold Ctrl over the URL: underline + hand pointer, only on the URL (not the final `.`); Ctrl+Click
  opens the browser. `echo crates/mkrew-ui/src/main.rs:120:5` (and `.\Cargo.toml`, and an absolute
  `C:\…\main.rs:12`): underline, Ctrl+Click opens the Files view on that file with the caret on
  line 120, scrolled into view; a path that does not exist, a folder, or a file outside the
  workspace gets no underline and Ctrl+Click selects as usual. Pressing/releasing Ctrl with the
  pointer at rest toggles the underline. A pane of a worktree task resolves paths in its
  worktree and switches Files to it. With unsaved text in Files, Ctrl+Click on another file asks
  Save / Don't save / Cancel. On macOS use Cmd. Terminal CPU stays flat while a flood runs with
  Ctrl held.
```

- [ ] **Step 11: Verify and tick**

Run the full verification. Do not tick inbox line 101 yet (Task 21 does).
Suggested message: `feat(ui): Ctrl+Click opens URLs and file paths from terminal panes`

---

### Task 21: Markdown preview in Files (mermaid, Source toggle)

Inbox: line 101 `File navigation & Markdown preview` (third bullet; last task of the item: tick). Spec: §12A (SPEC.md:825-855), §12 (:782-823), §14.

**Current state (investigated):**
- Docs already has everything: `docs::Shown { hash, blocks, lines, code, diagrams }` built by `Shown::new(&DocContent)` (`docs.rs:54`: parse, highlight fences, draw ```` ```mermaid ```` with `diagram::render`) on the background executor (`read_doc`, `main.rs:1579`), `preview::blocks(&[Block], &Shown, &Theme) -> Vec<AnyElement>` (`preview.rs:40`), typography from `theme::doc` and the theme tokens (so light/dark follow the theme), and `docs::Mode { Preview, Source }`. `render_doc` draws the page as a scroll column of `theme::DOC_PREVIEW_MAX_WIDTH` (`main.rs:9402-9427`); `doc_mode_toggle` (:11651) draws the segmented Preview / Source control but writes to `Docs`.
- Files: `render_file` (:9612) shows `render_editor` for `Content::Shown` (:9916). `markdown` is computed from the path (:9662) and only gates "Review in Docs". `FilesView` holds the text only as `Shown.editor` (`files_view.rs:40`); `FsContent` has no `DocContent`, so `Shown::new` cannot be reused as is. `Editor::text()` (`editor.rs:150`) is the text with the file's line endings; `Editor::version()` (:167) bumps on every text change (and restarts at 0 for a new editor).
- Unsaved text is never replaced (SPEC §2.5, ADR 0028): `FilesView::show` (:364) replaces the editor only when it is clean and the hash changed. Edits happen only in the editor, i.e. in Source mode.
- ADR check: ADR 0028 (the Files view edits files) is unchanged, because Source mode is that editor; ADR 0021 (Docs preview rendering, Mermaid addendum) is reused as is. No new crate, protocol or token. SPEC §12A changes; no ADR.
- `open_find` (:414) works on the editor's text, which is hidden in Preview; Task 20's `reveal(path, Some(line))` puts a caret in the editor.

**Decisions** (defaults; see open questions):
- A `.md` file opens as **Preview**; the header shows `Preview | Source` (only for `.md`). Another file starts over in Preview.
- The preview is built from the editor's text (not from the disk), so unsaved edits show in it and toggling never loses them. It is parsed once per text (off the UI thread), when the file is read, and when switching to Preview after edits; never per keystroke.
- Find (Ctrl+F / Ctrl+H) and "Open at a line" (Task 20) switch to Source, where their caret and matches are visible. No new key binding for the toggle (the header control only).
- A re-read that replaces the text shows "Rendering the preview…" until the new page is built; the previous page is kept while an edited text is re-parsed.

**Files:**
- Modify: `crates/mkrew-ui/src/docs.rs` (`Shown::from_text`, `Shown::new`), `crates/mkrew-ui/src/files_view.rs` (mode, preview state, tests), `crates/mkrew-ui/src/main.rs` (`render_file` :9645-9916, `render_doc` :9402-9427, `doc_mode_toggle` :11651 → `mode_toggle`, `read_file` :1784, new `build_preview`, new `render_md_preview`, new `preview_page`), `docs/SPEC.md` §12A, `docs/HUMAN_CHECKS.md`, `docs/INBOX.md` (tick line 101)

**Interfaces:**
- Consumes: Task 20's `FilesView::reveal(path, line)`; `docs::{Shown, Mode}`, `preview::blocks`.
- Produces: `docs::Shown::from_text(text: &str) -> Shown` (`hash` empty); `files_view::Stamp = (u64, u64)`; on `FilesView`: `pub mode: docs::Mode`, `is_markdown(&self) -> bool`, `previewing(&self) -> bool`, `toggle_mode(&mut self)`, `preview(&self) -> Option<&Rc<docs::Shown>>`, `preview_request(&mut self) -> Option<(Stamp, String)>`, `preview_ready(&mut self, stamp: Stamp, shown: docs::Shown)`; `Shell::build_preview(project_id, window, cx)`; `Shell::mode_toggle(..)` (generic Preview / Source control, used by Docs and Files).

- [ ] **Step 1: Failing tests (`files_view.rs`)**

At the top of `mod tests` add `use crate::docs::Mode;` (next to the other `use`). After `a_failed_read_drops_the_line` (Task 20):

```rust
    /// A page for tests: what the background build would return.
    fn page(text: &str) -> docs::Shown {
        docs::Shown::from_text(text)
    }

    #[test]
    fn markdown_opens_as_a_preview_that_is_parsed_once_per_text() {
        let mut view = opened("# T\n");
        assert!(view.is_markdown());
        assert_eq!(view.mode, Mode::Preview);
        assert!(view.previewing());
        let (stamp, text) = view.preview_request().expect("a new text is parsed");
        assert_eq!(text, "# T\n");
        // Asked once while it is being built.
        assert_eq!(view.preview_request(), None);
        view.preview_ready(stamp, page(&text));
        assert_eq!(view.preview().map(|shown| shown.lines.clone()), Some(vec!["# T".to_owned()]));
        assert_eq!(view.preview_request(), None, "the text did not change");
    }

    #[test]
    fn a_late_preview_never_replaces_a_newer_one() {
        let mut view = opened("a\n");
        let (old, _) = view.preview_request().unwrap();
        view.toggle_mode();
        view.editor_mut().unwrap().insert("b");
        view.toggle_mode();
        let (new, _) = view.preview_request().unwrap();
        view.preview_ready(new, page("ba\n"));
        view.preview_ready(old, page("a\n"));
        assert_eq!(view.preview().map(|shown| shown.lines.clone()), Some(vec!["ba".to_owned()]));
        assert_eq!(view.preview_request(), None);
    }

    #[test]
    fn another_file_starts_in_preview_without_the_old_page() {
        let mut view = opened("a\n");
        let (stamp, _) = view.preview_request().unwrap();
        view.preview_ready(stamp, page("a\n"));
        view.toggle_mode();
        view.open("src/main.rs");
        assert_eq!(view.mode, Mode::Preview);
        assert!(view.preview().is_none());
    }

    #[test]
    fn a_reread_that_changes_the_text_builds_a_new_page() {
        let mut view = opened("a\n");
        let (stamp, _) = view.preview_request().unwrap();
        view.preview_ready(stamp, page("a\n"));
        view.begin_read();
        view.read("README.md", file_text("b\n"));
        assert!(view.preview().is_none(), "no page of the old text");
        assert_eq!(view.preview_request().map(|(_, text)| text).as_deref(), Some("b\n"));
    }

    #[test]
    fn only_markdown_has_a_preview() {
        let mut view = listed();
        view.open("src/main.rs");
        view.begin_read();
        view.read("src/main.rs", file_text_at("src/main.rs", "fn a() {}"));
        assert!(!view.is_markdown());
        assert!(!view.previewing());
        assert_eq!(view.preview_request(), None);
    }

    /// Find and a link's line need the editor on screen.
    #[test]
    fn find_and_a_link_line_switch_to_source() {
        let mut view = opened("a\nb\n");
        assert!(view.open_find(false));
        assert_eq!(view.mode, Mode::Source);
        let mut view = opened("a\nb\n");
        view.reveal("README.md", Some(2));
        assert_eq!(view.mode, Mode::Source);
        // A plain reveal (quick open) of the same file keeps the mode.
        let mut view = opened("a\n");
        view.reveal("README.md", None);
        assert_eq!(view.mode, Mode::Preview);
    }
```

Second test, as it must read (it uses the module's `text(&view)` helper, so its local is `typed`):

```rust
    #[test]
    fn unsaved_edits_survive_the_toggle_and_the_preview_shows_them() {
        let mut view = opened("a\n");
        let (stamp, _) = view.preview_request().unwrap();
        view.preview_ready(stamp, page("a\n"));
        view.toggle_mode();
        assert_eq!(view.mode, Mode::Source);
        assert_eq!(view.preview_request(), None, "nothing to build in Source");
        view.editor_mut().unwrap().insert("x");
        view.toggle_mode();
        let (_, typed) = view.preview_request().expect("the edited text is parsed");
        assert_eq!(typed, "xa\n");
        assert_eq!(text(&view), "xa\n");
        assert!(view.unsaved().is_some());
    }
```

Run: `cargo test -p mkrew-ui --lib files_view -j 4`
Expected: FAIL to compile (`docs::Shown::from_text`, `mode`, `toggle_mode`, … missing).

- [ ] **Step 2: `docs::Shown::from_text`**

`docs.rs`, replace `Shown::new` (:54-76):

```rust
impl Shown {
    pub fn new(content: &DocContent) -> Shown {
        Shown {
            hash: content.hash.clone(),
            ..Shown::from_text(&content.content)
        }
    }

    /// A text parsed for the preview, with `hash` empty. The Files view parses its editor's
    /// text, which is not a `DocContent`.
    pub fn from_text(text: &str) -> Shown {
        let blocks = markdown::parse(text);
        let mut code = HashMap::new();
        let mut diagrams = HashMap::new();
        markdown::walk(&blocks, &mut |block| {
            if let BlockKind::Code { lang, text } = &block.kind {
                code.insert(block.lines.start, highlight::fenced(lang.as_deref(), text));
                if lang
                    .as_deref()
                    .is_some_and(|lang| lang.eq_ignore_ascii_case("mermaid"))
                {
                    diagrams.insert(block.lines.start, diagram::render(text));
                }
            }
        });
        Shown {
            hash: String::new(),
            lines: source_lines(text),
            blocks,
            code,
            diagrams,
        }
    }
}
```

- [ ] **Step 3: The state in `FilesView`**

Imports at the top of `files_view.rs`:

```rust
use std::sync::atomic::{AtomicU64, Ordering};

use crate::docs::{self, Mode};
```

Before `pub struct FilesView`:

```rust
/// Numbers every text the view shows (across views), so that a page built for one text is
/// never taken for another's.
static NEXT_TEXT: AtomicU64 = AtomicU64::new(1);

/// What a preview was built from: the number of the text in the editor and the editor's
/// version of it. Later texts and later edits have larger stamps.
pub type Stamp = (u64, u64);

/// The rendered page of a `.md` file (ADR 0021) and what it was built from.
#[derive(Debug)]
struct MdPreview {
    stamp: Stamp,
    shown: Rc<docs::Shown>,
}
```

Fields, after `pending_line` (Task 20):

```rust
    /// `.md` files: the rendered preview or the editor (SPEC §12A). Another file starts in
    /// Preview.
    pub mode: Mode,
    /// Number of the text in the editor: new for every file and every re-read that replaced it.
    text_id: u64,
    /// The last page built, and the stamp last asked for (one build per text).
    preview: Option<MdPreview>,
    preview_asked: Option<Stamp>,
```

`new`: `mode: Mode::Preview, text_id: 0, preview: None, preview_asked: None,` after `pending_line: None,`.

`open`: inside the new-file path (after `self.pending_line = None;`):

```rust
        self.mode = Mode::Preview;
        self.preview = None;
        self.preview_asked = None;
```

`reveal`: after `self.pending_line = line;`:

```rust
        // The caret of a link's line shows in the editor, not in the preview.
        if line.is_some() {
            self.mode = Mode::Source;
        }
```

`open_find`: after the `if self.file.is_none() { return false; }` check add:

```rust
        // Matches and the replace row act on the editor's text.
        self.mode = Mode::Source;
```

(`open_find` borrows `shown` from `self.content` before; move the assignment to just before `let editor = &shown.editor;`? It cannot: `shown` is borrowed. Put it at the very top instead, after the two early returns are evaluated: restructure as

```rust
        if !matches!(self.content, Content::Shown(_)) || self.file.is_none() {
            return false;
        }
        self.mode = Mode::Source;
        let Content::Shown(shown) = &self.content else {
            return false;
        };
```

and delete the old `let Content::Shown(shown) = ... else` and the `self.file.is_none()` check.)

`show` (:386), first line of the `Ok((content, code)) =>` replace arm:

```rust
                self.text_id = NEXT_TEXT.fetch_add(1, Ordering::Relaxed);
```

New methods, after `shown()`:

```rust
    /// Whether the opened file is Markdown (`.md`).
    pub fn is_markdown(&self) -> bool {
        (self.file.as_deref())
            .and_then(|file| file.rsplit_once('.'))
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("md"))
    }

    /// The `.md` file is shown as its rendered page, not as the editor.
    pub fn previewing(&self) -> bool {
        self.mode == Mode::Preview && self.is_markdown() && self.stamp().is_some()
    }

    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            Mode::Preview => Mode::Source,
            Mode::Source => Mode::Preview,
        };
    }

    /// What a page of the editor's text now would be built from (`None`: nothing editable
    /// shown).
    fn stamp(&self) -> Option<Stamp> {
        let shown = (self.shown()).filter(|shown| !shown.code.binary && !shown.code.too_large)?;
        Some((self.text_id, shown.editor.version()))
    }

    /// The last page of this text; while an edited text is parsed again it is the page of the
    /// text before the edit. `None` after another text replaced it.
    pub fn preview(&self) -> Option<&Rc<docs::Shown>> {
        (self.preview.as_ref())
            .filter(|preview| preview.stamp.0 == self.text_id)
            .map(|preview| &preview.shown)
    }

    /// The text to parse for the preview, once per text: `None` when no preview is shown, the
    /// page is current, or one is being built for this text. The shell parses it off the UI
    /// thread and answers with [`Self::preview_ready`].
    pub fn preview_request(&mut self) -> Option<(Stamp, String)> {
        if !self.previewing() {
            return None;
        }
        let stamp = self.stamp()?;
        let current = (self.preview.as_ref()).is_some_and(|preview| preview.stamp == stamp);
        if current || self.preview_asked == Some(stamp) {
            return None;
        }
        self.preview_asked = Some(stamp);
        Some((stamp, self.shown()?.editor.text()))
    }

    /// The page built for `stamp`. Dropped when another text replaced the one it was built
    /// from, or when a newer page is already here (builds can finish out of order).
    pub fn preview_ready(&mut self, stamp: Stamp, shown: docs::Shown) {
        let newer = (self.preview.as_ref()).is_some_and(|preview| preview.stamp >= stamp);
        if stamp.0 != self.text_id || newer {
            return;
        }
        self.preview = Some(MdPreview {
            stamp,
            shown: Rc::new(shown),
        });
    }
```

Run: `cargo test -p mkrew-ui --lib files_view -j 4` and `cargo test -p mkrew-ui --lib docs -j 4`
Expected: PASS (the Docs tests cover `Shown::new` through `from_text`).

- [ ] **Step 4: The generic mode toggle, the page and the build (`main.rs`)**

Replace `doc_mode_toggle` (:11650-11694) with:

```rust
    /// Preview / Source segments (Docs, and `.md` files in Files); the inactive one runs
    /// `switch`. `id` prefixes the segments' element ids.
    fn mode_toggle(
        &self,
        id: &'static str,
        mode: Mode,
        keys: SharedString,
        switch: impl Fn(&mut Self, &mut Window, &mut Context<'_, Self>) + Clone + 'static,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::Div {
        let hover = theme.surface_raised;
        div().flex_none().flex().gap(px(space::XS)).children(
            [(Mode::Preview, "Preview"), (Mode::Source, "Source")].map(|(target, label)| {
                let active = target == mode;
                let switch = switch.clone();
                div()
                    .id((id, usize::from(target == Mode::Source)))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .tooltip(tooltip(theme, "Toggle preview / source", keys.clone()))
                    .when(active, |el| {
                        el.bg(rgb(theme.surface_raised))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!active, |el| {
                        el.cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                switch(shell, window, cx);
                            }))
                    })
                    .child(label)
            }),
        )
    }
```

`render_doc` (:9317): replace `.child(self.doc_mode_toggle(project_id, docs.mode, theme, cx))` with:

```rust
            .child(self.mode_toggle(
                "doc-mode",
                docs.mode,
                self.keymap.label(Command::ToggleDocSource).into(),
                move |shell, _, cx| {
                    if let Some(docs) = shell.docs.get_mut(&project_id) {
                        docs.toggle_mode();
                        cx.notify();
                    }
                },
                theme,
                cx,
            ))
```

and the `Mode::Preview =>` arm (:9402-9427) with the shared page:

```rust
                Mode::Preview => preview_page(
                    "doc-preview",
                    preview_blocks(project_id, shown, docs.comments.selection, theme, cx),
                ),
```

Free function next to `preview_blocks` (:15447):

```rust
/// The preview's page: a scrolling, centered column of `blocks` (ADR 0021), shared by Docs
/// and the Files view's `.md` files.
fn preview_page(
    id: &'static str,
    blocks: Vec<gpui::AnyElement>,
) -> gpui::AnyElement {
    div()
        .id(id)
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .px(px(space::XL))
        .py(px(space::L))
        .child(
            div()
                .w_full()
                .max_w(px(theme::DOC_PREVIEW_MAX_WIDTH))
                .mx_auto()
                .flex()
                .flex_col()
                .gap(px(space::M))
                .text_size(px(theme::doc::BODY))
                .line_height(relative(theme::doc::LINE_HEIGHT))
                .children(blocks),
        )
        .into_any_element()
}
```

`render_file`:
- `let markdown = path.rsplit_once('.')…;` (:9662-9664) → `let markdown = view.is_markdown();`
- before the "Review in Docs" `.when(markdown && view.worktree_task_id.is_none(), …)` (:9699) insert:

```rust
            .when(markdown && editable, |el| {
                el.child(self.mode_toggle(
                    "file-mode",
                    view.mode,
                    "".into(),
                    move |shell, window, cx| {
                        shell.files_view(project_id).toggle_mode();
                        shell.build_preview(project_id, window, cx);
                        cx.notify();
                    },
                    theme,
                    cx,
                ))
            })
```

- the find bar: `let find_bar = view.find.as_ref().filter(|_| !view.previewing()).map(|find| {` (:9736; matches are only visible in the editor).
- the body match, before the last arm (:9916):

```rust
            Content::Shown(_) if view.previewing() => self.render_md_preview(view, theme),
            Content::Shown(shown) => self.render_editor(project_id, shown, window, theme, cx),
```

After `render_file` add:

```rust
    /// A `.md` file of the Files view as the Docs preview: the page of the editor's text
    /// (unsaved edits included).
    fn render_md_preview(&self, view: &FilesView, theme: &Theme) -> gpui::AnyElement {
        match view.preview() {
            None => message(theme, None, "Rendering the preview…".into(), None)
                .flex_1()
                .into_any_element(),
            Some(shown) if shown.blocks.is_empty() => {
                message(theme, None, "This file is empty.".into(), None)
                    .flex_1()
                    .into_any_element()
            }
            Some(shown) => preview_page("file-preview", preview::blocks(&shown.blocks, shown, theme)),
        }
    }
```

Next to `read_file`:

```rust
    /// Parses the `.md` file of the Files view of `project_id` for its preview when it is
    /// shown as one and its text moved on, off the UI thread (highlighting and Mermaid are
    /// slow). Asks again when the text changed while it ran.
    fn build_preview(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let target = view.worktree_task_id;
        let Some((stamp, text)) = view.preview_request() else {
            return;
        };
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let page = background
                .spawn(async move { Shown::from_text(&text) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(view) = shell.files_at(project_id, target) {
                    view.preview_ready(stamp, page);
                }
                shell.build_preview(project_id, window, cx);
                cx.notify();
            });
        })
        .detach();
    }
```

`read_file` closure (:1784-1795), after `shell.show_pending_line(project_id, target);` (Task 20):

```rust
                shell.build_preview(project_id, window, cx);
```

Run: `cargo check -p mkrew-ui -j 4`, then `cargo test -p mkrew-ui --lib -j 4`
Expected: compiles, PASS. (Headless limit: the page itself is checked by the human step below.)

- [ ] **Step 5: SPEC §12A**

Replace `- `.md` files offer "Review in Docs".` (SPEC.md:844) with:

````markdown
- **Markdown preview:** a `.md` file opens as a rendered preview, by the Docs renderer (§12,
  §14: the same typography tokens for light and dark, highlighted code blocks, ```` ```mermaid ````
  blocks drawn as diagrams, built off the UI thread). The header has a `Preview | Source`
  toggle; Source is the editor above, and editing only happens there. The preview is built from
  the editor's text, so unsaved changes show in it and toggling never loses them; it is parsed
  when the file is read and when switching to Preview after an edit, never per keystroke. Find
  and "Open at a line" switch to Source. Another file starts in Preview. Links in the preview
  are not clickable (ADR 0021). `.md` files also offer "Review in Docs".
````

- [ ] **Step 6: HUMAN_CHECKS**

```markdown
- [ ] **Markdown preview in Files:** open a `.md` file in Files → rendered page (headings with
  rules, lists, tables, highlighted code blocks, a ```` ```mermaid ```` block drawn as a diagram),
  `Preview | Source` in the header. Source = the editor: type a few words (● shows), switch to
  Preview → the words are in the page, switch back → caret and undo intact, nothing lost; Ctrl+S
  saves from either mode. Ctrl+F in Preview switches to Source with the bar open. Edit the file in
  Zed (no unsaved text) → the page follows. Ctrl+Click `README.md:12` in a terminal → Source,
  caret on line 12. Toggle the theme: the page and the diagram follow (light + dark). A 2 MB
  `.md` does not freeze the window while it renders.
```

- [ ] **Step 7: Verify and tick**

Run the full verification. Tick inbox line 101 `[x]` (it is `[~]` by now) in `docs/INBOX.md`.
Suggested message: `feat(ui): rendered Markdown preview with a Source toggle in the Files view`

---

### Task 22: Git diff: Unified / Split modes, remembered

Inbox: line 107 `**10. Git module: diff view modes**`. Spec: §12B (SPEC.md:885-886 Diff viewer, :907-908 Extra tokens), §13 (:972-973 Git keys), §20.0 (:1374 `[ui]`), backlog list (:1328 "side-by-side diff").

No ADR: SPEC lists side-by-side only as backlog (not forbidden), so this is a SPEC update. No protocol change (`FileDiff` is already structured), no migration.

**Current state (investigated):**
- The diff viewer is one `uniform_list("git-diff", …)` in `Shell::render_git_diff` (`main.rs`, the `git_view::Diff::Shown(shown) =>` arm of `let body`), tracked by `Shell.git_diff_scroll: UniformListScrollHandle`. Rows come from `git_view::Shown.rows: Vec<DiffRow>` (`Gap(u32) | Hunk(usize) | Line(DiffLine)`), built by `git_view::rows(&FileDiff)` inside `Shown::new` (off the UI thread). `Shown.spans[i]` are the syntax spans of `rows[i]` (same index).
- `DiffLine { kind: Context|Add|Del, text, old_line, new_line, no_newline, crlf }`. Git emits a run of `Del` lines then the run of `Add` lines of the same change, so pairing is deterministic. There is no word-level highlight anywhere (`highlight::diff_spans` is syntax only).
- One row is drawn by the free fn `git_diff_row(shown, index, current, picked, line_height, theme)` (`main.rs`); it also draws the first rows of the Discard dialog (`git_view::DISCARD_PREVIEW_ROWS`, call `git_diff_row(shown, index, None, None, line_height, theme)`). The frame has `border_l(theme::DIFF_PICK_BAR_WIDTH)` (accent when the line is picked for a comment).
- Comment picking: in the list closure a click on a `Line` row with `new_line == Some(n)` (and `view.can_comment()`) calls `Shell::pick_diff_line(project_id, n, shift, window, cx)`; comments anchor on new-side line numbers; `Shown::quote` / `new_side_text` read `rows`, not the layout, so they need no change.
- Hunk navigation: `Shell::git_key` (`NextHunk | PreviousHunk`) scrolls `git_diff_scroll` to `Shown::hunk_row(hunk)` (row index of `DiffRow::Hunk(hunk)`). Only `git_key` (main.rs) and the test `rows_collapse_unchanged_lines_between_hunks` (git_view.rs) call `hunk_row`.
- Colors: `Theme.diff_add_bg/_text`, `diff_del_bg/_text`, `diff_hunk_header` (theme.rs: struct, `Theme::dark()`, `Theme::light()` and the zero-filled literal block). Test `highlighted_diff_lines_stay_readable` checks syntax / text / muted on the add and del backgrounds (≥ 4.5).
- Settings: `mkrew_core::settings::UiSettings { font_family, font_size, theme, … }` (`deny_unknown_fields`, enums parse `rename_all = "lowercase"` so an unknown value is a parse error with `file:line:col`, listed in the palette; test `ui_theme_parses_and_defaults_to_system`). After Task 15 the byte-preserving line editor is `set_ui_value(src, key, value)` / `write_ui_value(path, key, value)` (value = a TOML literal; it refuses to write a file that does not parse or would not parse after the edit). `Shell::toggle_theme` is the persistence pattern: apply in memory, call the writer, on `Err` set `self.settings_error`.
- Hot reload: the 500 ms status tick in `Shell::new` compares the mtime of `settings.toml` and calls `theme::reload(...)`, which only returns fonts and theme mode.
- Keys: Git-context commands `GitToggleMode b`, `GitToggleHistory h`, `GitNextHunk n`, `GitPreviousHunk p`, `GitStage s`, `GitUnstage u`, `GitFocusCommit c`. `d` is unbound everywhere in `keymap.rs`, so the optional toggle key is free.
- The segmented-control look to copy is the Working / Against base / History control in `git_header` (`segment` closure: `surface_raised` active, `text_muted` inactive, hover, `opacity::ACTIVE`).

**Design (decided):**
- Unified = today's list, unchanged. Split = ONE `uniform_list` of paired rows, so the single `git_diff_scroll` moves both sides.
- Pairing rule (inside one hunk): a run of `Del` lines followed by the run of `Add` lines is paired in order (i-th `Del` with i-th `Add`); the shorter side is padded with an empty cell; a `Context` line is on both sides (left `old_line`, right `new_line`); gap and hunk-header rows span both halves (a run never pairs across them).
- Colors: paired `Del`+`Add` rows are *modified* and use two new tokens `diff_mod_bg` / `diff_mod_text` (amber) on BOTH halves; an unpaired `Del` / `Add` row keeps `diff_del_*` / `diff_add_*`; the padding cell has no fill. Unified cannot know a pair without the same layout, so it stays red / green with `−` / `+` signs (SPEC says so).
- Comments: only the right half of a row with a new-side line is clickable (accent bar and number on the right half); left-only deletions are not.
- Mode: `Shell.diff_mode: DiffMode` (global, every repo of the workspace, History included), `[ui] diff_mode = "unified" | "split"`, default unified; unknown value = parse error, like `theme`. Written by the header toggle, read at start and on settings hot reload.

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`DiffMode`, `UiSettings`, `template()`, tests)
- Modify: `crates/mkrew-ui/src/theme.rs` (`diff_mod_bg`, `diff_mod_text`, tests)
- Modify: `crates/mkrew-ui/src/git_view.rs` (`SplitRow`, `split_rows`, `Shown.split`, `Shown::list_len`, `Shown::hunk_row(hunk, mode)`, tests)
- Modify: `crates/mkrew-ui/src/keymap.rs`, `crates/mkrew-ui/src/palette.rs` (`Command::GitToggleDiffMode`, key `d`)
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell.diff_mode`, start + hot reload, `set_diff_mode`, dispatch, header toggle, split rendering, helpers)
- Modify: `docs/SPEC.md` §12B, §13, §20.0, backlog; `docs/HUMAN_CHECKS.md`; `docs/INBOX.md` (tick line 107)

**Interfaces:**
- Consumes (Task 15): `mkrew_core::settings::set_ui_value(src: &str, key: &str, value: &str) -> String` and `write_ui_value(path: &Path, key: &str, value: &str) -> Result<(), SettingsError>`.
- Produces:
  - `mkrew_core::settings::DiffMode { Unified, Split }` (`Default` = `Unified`; `name(self) -> &'static str`; `other(self) -> DiffMode`), `UiSettings.diff_mode: DiffMode`.
  - `git_view::SplitRow { Span(usize), Same(usize), Change { old: Option<usize>, new: Option<usize> } }` (indices into `Shown.rows`), `git_view::split_rows(rows: &[DiffRow]) -> Vec<SplitRow>`, `Shown.split: Vec<SplitRow>`, `Shown::list_len(&self, mode: DiffMode) -> usize`, `Shown::hunk_row(&self, hunk: usize, mode: DiffMode) -> Option<usize>` (signature change).
  - `Theme.diff_mod_bg`, `Theme.diff_mod_text`.
  - `keymap::Command::GitToggleDiffMode` (name `git_toggle_diff_mode`, default `d`, context `Git`).

- [ ] **Step 1: Failing tests for the setting**

`crates/mkrew-core/src/settings.rs`, in `mod tests`, after `ui_theme_parses_and_defaults_to_system`:

```rust
    #[test]
    fn ui_diff_mode_parses_and_defaults_to_unified() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert_eq!(ui("").diff_mode, DiffMode::Unified);
        assert_eq!(ui("[ui]\ndiff_mode = \"split\"").diff_mode, DiffMode::Split);
        // Another [ui] key does not change it.
        assert_eq!(ui("[ui]\ntheme = \"dark\"").diff_mode, DiffMode::Unified);
        assert!(parse_err("[ui]\ndiff_mode = \"side\"").contains("unknown variant `side`"));
        // [terminal] has no diff_mode.
        assert!(parse_err("[terminal]\ndiff_mode = \"split\"").contains("unknown field `diff_mode`"));
    }

    #[test]
    fn writing_diff_mode_keeps_theme_and_comments() {
        let src = "[ui]\n# diff_mode = \"unified\"\ntheme = \"dark\"\n";
        let out = set_ui_value(src, "diff_mode", "\"split\"");
        let ui = parse(Path::new("s.toml"), &out).expect(&out).ui;
        assert_eq!((ui.diff_mode, ui.theme), (DiffMode::Split, ThemeMode::Dark));
        assert!(out.contains("# diff_mode = \"unified\""), "the user's comment stays: {out}");
        // A second write replaces the line instead of adding another.
        let again = set_ui_value(&out, "diff_mode", "\"unified\"");
        assert_eq!(
            parse(Path::new("s.toml"), &again).expect(&again).ui.diff_mode,
            DiffMode::Unified
        );
        assert_eq!(again.matches("diff_mode =").count(), 2, "comment + one key: {again}");
    }

    #[test]
    fn write_ui_value_saves_diff_mode_into_the_template() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        write_ui_value(&path, "diff_mode", "\"split\"").unwrap();
        assert_eq!(load(&path).unwrap().ui.diff_mode, DiffMode::Split);
        // A file that does not parse is left alone.
        std::fs::write(&path, "[ui\n").unwrap();
        assert!(write_ui_value(&path, "diff_mode", "\"unified\"").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "[ui\n");
    }
```

Run: `cargo test -p mkrew-core settings -j 4`
Expected: FAIL to compile (`cannot find type DiffMode`).

- [ ] **Step 2: Implement the setting**

`settings.rs`, after `impl ThemeMode`:

```rust
/// `[ui] diff_mode` (SPEC §12B): how the Git diff viewer lays out a file's changes. The
/// diff header toggle writes it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffMode {
    /// One column: removed lines, then the added ones.
    #[default]
    Unified,
    /// Two columns: the old file on the left, the new one on the right.
    Split,
}

impl DiffMode {
    pub fn name(self) -> &'static str {
        match self {
            DiffMode::Unified => "unified",
            DiffMode::Split => "split",
        }
    }

    /// The other mode (what the toggle switches to).
    pub fn other(self) -> DiffMode {
        match self {
            DiffMode::Unified => DiffMode::Split,
            DiffMode::Split => DiffMode::Unified,
        }
    }
}
```

`UiSettings`: add `pub diff_mode: DiffMode,` after `pub theme: ThemeMode,` (and, after Task 15's `inspector_width`, keep both); in `impl Default for UiSettings` add `diff_mode: DiffMode::Unified,`. Update the doc line to `/// `[ui]`: the app font, theme, diff layout (and panel width) (SPEC §14, §12B, §20.0).`

`template()`: the comment above `[ui]` gets one more line and the block one more key, after the `# theme = "system"` line:

```rust
     # Diff view: unified (one column) or split (old | new); the Git diff header toggle writes it.\n\
```
placed as the last line of the comment block above `[ui]`, and

```rust
     # diff_mode = \"unified\"\n\
```
right after `# theme = \"system\"\n\`.

Run: `cargo test -p mkrew-core settings -j 4`
Expected: PASS (`the_template_parses_to_the_defaults_commented_or_not` uncomments the new key and still equals the defaults).

- [ ] **Step 3: Failing test for the modified-line tokens**

`crates/mkrew-ui/src/theme.rs`, in `mod tests`, add after `highlighted_diff_lines_stay_readable`:

```rust
    /// Added, removed and modified (a line changed in place, Split diff) lines read on
    /// their own backgrounds, in both appearances.
    #[test]
    fn diff_line_colors_read_on_their_backgrounds() {
        for theme in [Theme::dark(), Theme::light()] {
            for (fg, bg) in [
                (theme.diff_add_text, theme.diff_add_bg),
                (theme.diff_del_text, theme.diff_del_bg),
                (theme.diff_mod_text, theme.diff_mod_bg),
            ] {
                let ratio = contrast(fg, bg);
                assert!(ratio >= 4.5, "{fg:06X} on {bg:06X}: {ratio:.2}");
            }
        }
    }
```

and in `highlighted_diff_lines_stay_readable` change the inner loop to `for bg in [theme.diff_add_bg, theme.diff_del_bg, theme.diff_mod_bg] {`.

Run: `cargo test -p mkrew-ui --lib theme -j 4`
Expected: FAIL to compile (`no field diff_mod_bg`).

- [ ] **Step 4: Add the tokens**

theme.rs, struct `Theme`, after `diff_del_text`:

```rust
    /// A line changed in place: the paired removed + added rows of the Split diff (both halves).
    pub diff_mod_bg: u32,
    pub diff_mod_text: u32,
```

`Theme::dark()` after `diff_del_text: 0xF2A0A4,`: `diff_mod_bg: 0x3A3320,` and `diff_mod_text: 0xE6C77A,`.
`Theme::light()` after `diff_del_text: 0xA61B1B,`: `diff_mod_bg: 0xFFF6DB,` and `diff_mod_text: 0x7A5200,`.
Zero-filled literal block, after `diff_del_text: 0,`: `diff_mod_bg: 0,` and `diff_mod_text: 0,`.

(Hand-computed ratios: dark text on bg ≈ 7.7, light ≈ 6.4; the backgrounds sit at the same luminance as the add / del ones, so the syntax colors keep their margin. If the extended test reports < 4.5, move the failing color one step and re-run; do not lower the 4.5.)

Run: `cargo test -p mkrew-ui --lib theme -j 4`
Expected: PASS.

- [ ] **Step 5: Failing tests for the split model**

`crates/mkrew-ui/src/git_view.rs`, in `mod tests`, next to `laid_out`, add helpers:

```rust
    fn del(old: u32) -> DiffLine {
        line(DiffLineKind::Del, Some(old), None)
    }

    fn add(new: u32) -> DiffLine {
        line(DiffLineKind::Add, None, Some(new))
    }

    fn ctx(old: u32, new: u32) -> DiffLine {
        line(DiffLineKind::Context, Some(old), Some(new))
    }

    /// A hunk holding `lines` (its counts only matter for the gaps between hunks).
    fn hunk_of(lines: Vec<DiffLine>, base: Hunk) -> Hunk {
        Hunk { lines, ..base }
    }
```

Tests, after `line_rows_carry_their_syntax_spans`:

```rust
    /// Rows of `hunk_of(…)` below: 0 = hunk header, then the lines in order.
    #[test]
    fn split_pairs_a_change_in_order_and_pads_the_shorter_side() {
        // ctx, 3 removed, 2 added, ctx.
        let shown = Shown::new(diff(vec![hunk_of(
            vec![ctx(1, 1), del(2), del(3), del(4), add(2), add(3), ctx(5, 4)],
            hunk(1, 1, 1, 1),
        )]));
        assert_eq!(
            shown.split,
            [
                SplitRow::Span(0),
                SplitRow::Same(1),
                SplitRow::Change { old: Some(2), new: Some(5) },
                SplitRow::Change { old: Some(3), new: Some(6) },
                SplitRow::Change { old: Some(4), new: None },
                SplitRow::Same(7),
            ]
        );
        // More added than removed: the left side is padded.
        let shown = Shown::new(diff(vec![hunk_of(
            vec![del(2), add(2), add(3), add(4)],
            hunk(1, 1, 1, 1),
        )]));
        assert_eq!(
            shown.split,
            [
                SplitRow::Span(0),
                SplitRow::Change { old: Some(1), new: Some(2) },
                SplitRow::Change { old: None, new: Some(3) },
                SplitRow::Change { old: None, new: Some(4) },
            ]
        );
        // A pure insertion and a pure deletion are one-sided (base `hunk(1, 1, 1, 1)` keeps
        // the first hunk free of a leading gap row).
        let shown = Shown::new(diff(vec![hunk_of(vec![add(1), add(2)], hunk(1, 1, 1, 1))]));
        assert_eq!(
            shown.split[1..],
            [
                SplitRow::Change { old: None, new: Some(1) },
                SplitRow::Change { old: None, new: Some(2) },
            ]
        );
        let shown = Shown::new(diff(vec![hunk_of(vec![del(1)], hunk(1, 1, 1, 0))]));
        assert_eq!(shown.split[1..], [SplitRow::Change { old: Some(1), new: None }]);
    }

    #[test]
    fn split_never_pairs_across_a_gap_or_a_hunk_header() {
        // Hunk 0 ends with a removed line, hunk 1 (far below) starts with an added one.
        let shown = Shown::new(diff(vec![
            hunk_of(vec![del(1)], hunk(1, 1, 1, 0)),
            hunk_of(vec![add(10)], hunk(10, 1, 10, 1)),
        ]));
        // rows: 0 hunk, 1 del, 2 gap, 3 hunk, 4 add.
        assert_eq!(
            shown.split,
            [
                SplitRow::Span(0),
                SplitRow::Change { old: Some(1), new: None },
                SplitRow::Span(2),
                SplitRow::Span(3),
                SplitRow::Change { old: None, new: Some(4) },
            ]
        );
    }

    /// Comment picking and the quote read new-side numbers: the right side must hold every
    /// new-side line, in order, and only added / unchanged lines; the left side the old ones.
    #[test]
    fn split_sides_keep_every_old_and_new_line_in_order() {
        let shown = Shown::new(diff(vec![hunk_of(
            vec![ctx(1, 1), del(2), add(2), add(3), ctx(3, 4), del(4), ctx(5, 5)],
            hunk(1, 5, 1, 5),
        )]));
        let line_at = |at: usize| match &shown.rows[at] {
            DiffRow::Line(line) => line.clone(),
            other => panic!("not a line: {other:?}"),
        };
        let (mut olds, mut news) = (Vec::new(), Vec::new());
        for row in &shown.split {
            let (left, right) = match row {
                SplitRow::Span(_) => continue,
                SplitRow::Same(at) => (Some(*at), Some(*at)),
                SplitRow::Change { old, new } => {
                    assert!(old.iter().all(|&at| matches!(line_at(at).kind, DiffLineKind::Del)));
                    assert!(new.iter().all(|&at| matches!(line_at(at).kind, DiffLineKind::Add)));
                    (*old, *new)
                }
            };
            olds.extend(left.and_then(|at| line_at(at).old_line));
            news.extend(right.and_then(|at| line_at(at).new_line));
        }
        assert_eq!(olds, [1, 2, 3, 4, 5]);
        assert_eq!(news, [1, 2, 3, 4, 5]);
    }

    /// Hunk navigation (n / p) scrolls to the header's row of the mode on screen.
    #[test]
    fn hunk_rows_follow_the_diff_mode() {
        let shown = Shown::new(diff(vec![
            hunk_of(vec![ctx(1, 1), del(2), add(2), add(3)], hunk(1, 2, 1, 3)),
            hunk(10, 1, 10, 1),
        ]));
        // Unified: hunk, ctx, del, add, add, gap, hunk, ctx. Split pairs del+add: one row less.
        assert_eq!(shown.hunk_row(1, DiffMode::Unified), Some(6));
        assert_eq!(shown.hunk_row(1, DiffMode::Split), Some(5));
        assert_eq!(shown.hunk_row(0, DiffMode::Split), Some(0));
        assert_eq!(shown.hunk_row(2, DiffMode::Split), None);
        assert_eq!(shown.list_len(DiffMode::Unified), 8);
        assert_eq!(shown.list_len(DiffMode::Split), 7);
    }
```

In `rows_collapse_unchanged_lines_between_hunks`, change `shown.hunk_row(1)` to `shown.hunk_row(1, DiffMode::Unified)` and `shown.hunk_row(3)` to `shown.hunk_row(3, DiffMode::Unified)`.

Run: `cargo test -p mkrew-ui --lib git_view -j 4`
Expected: FAIL to compile (`cannot find type SplitRow`, `hunk_row` takes 1 argument).

- [ ] **Step 6: Implement the split model**

git_view.rs imports: add `DiffLineKind` to the `use mkrew_protocol::{…}` list and below `use gpui::Keystroke;` add `use mkrew_core::settings::DiffMode;`.

After `pub enum DiffRow { … }`:

```rust
/// One row of the Split diff viewer; indices point into [`Shown::rows`] (its `spans` and line
/// data are reused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitRow {
    /// A gap or hunk-header row (`rows[.0]`), drawn across both halves.
    Span(usize),
    /// An unchanged line (`rows[.0]`): old number on the left, new number on the right.
    Same(usize),
    /// Removed line `old` on the left, added line `new` on the right. Both present = the line
    /// was modified; one `None` = the other half is empty padding.
    Change { old: Option<usize>, new: Option<usize> },
}

/// The Split rows of `rows`. Inside a hunk a run of removed lines and the run of added lines
/// after it are paired in order and the shorter side is padded; unchanged lines are on both
/// sides; gap and hunk rows end a run and span both halves.
pub fn split_rows(rows: &[DiffRow]) -> Vec<SplitRow> {
    fn flush(out: &mut Vec<SplitRow>, removed: &mut Vec<usize>, added: &mut Vec<usize>) {
        for i in 0..removed.len().max(added.len()) {
            out.push(SplitRow::Change {
                old: removed.get(i).copied(),
                new: added.get(i).copied(),
            });
        }
        removed.clear();
        added.clear();
    }
    let mut out = Vec::with_capacity(rows.len());
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    for (at, row) in rows.iter().enumerate() {
        match row {
            DiffRow::Line(line) if matches!(line.kind, DiffLineKind::Del) => {
                // A removed line after added ones starts the next change.
                if !added.is_empty() {
                    flush(&mut out, &mut removed, &mut added);
                }
                removed.push(at);
            }
            DiffRow::Line(line) if matches!(line.kind, DiffLineKind::Add) => added.push(at),
            DiffRow::Line(_) => {
                flush(&mut out, &mut removed, &mut added);
                out.push(SplitRow::Same(at));
            }
            DiffRow::Gap(_) | DiffRow::Hunk(_) => {
                flush(&mut out, &mut removed, &mut added);
                out.push(SplitRow::Span(at));
            }
        }
    }
    flush(&mut out, &mut removed, &mut added);
    out
}
```

`Shown`: add the field after `rows`:

```rust
    /// The Split layout of `rows` ([`split_rows`]); never longer than `rows`.
    pub split: Vec<SplitRow>,
```

In `Shown::new`, after `let rows = rows(&diff);` nothing changes; in the final struct literal add `split: split_rows(&rows),` BEFORE `rows,` moves it (put `split: split_rows(&rows),` as the first field of the literal, above `number_width`). Replace `hunk_row` and add `list_len`:

```rust
    /// The number of rows the list draws in `mode`.
    pub fn list_len(&self, mode: DiffMode) -> usize {
        match mode {
            DiffMode::Unified => self.rows.len(),
            DiffMode::Split => self.split.len(),
        }
    }

    /// The list row of hunk `hunk`'s header in `mode` (what n / p scroll to).
    pub fn hunk_row(&self, hunk: usize, mode: DiffMode) -> Option<usize> {
        let at = self.rows.iter().position(|row| *row == DiffRow::Hunk(hunk))?;
        match mode {
            DiffMode::Unified => Some(at),
            DiffMode::Split => self.split.iter().position(|row| *row == SplitRow::Span(at)),
        }
    }
```

Run: `cargo test -p mkrew-ui --lib git_view -j 4`
Expected: PASS (the four new tests and `rows_collapse_unchanged_lines_between_hunks`). `main.rs` does not compile yet (`hunk_row` arity); that is fixed in Step 9, so run only `--lib` tests here if the binary target is built separately, otherwise continue to Step 7 first.

- [ ] **Step 7: Failing test for the key**

`keymap.rs`, test `git_keys…` table (the loop with `("c", Command::GitFocusCommit),`): add `("d", Command::GitToggleDiffMode),` after it.

Run: `cargo test -p mkrew-ui --lib keymap -j 4`
Expected: FAIL to compile (`no variant GitToggleDiffMode`).

- [ ] **Step 8: Implement the command**

keymap.rs: enum, after `GitFocusCommit,`:

```rust
    /// Git only: switch the diff viewer between Unified and Split.
    GitToggleDiffMode,
```
`all()`: `Command::GitToggleDiffMode,` after `Command::GitFocusCommit,`. `name()`: `Command::GitToggleDiffMode => "git_toggle_diff_mode".into(),`. `default_keys()`: `Command::GitToggleDiffMode => "d".into(),`. `context()`: add `| Command::GitToggleDiffMode` to the arm ending `Command::GitFocusCommit => Some(GIT_CONTEXT)` (before it).

palette.rs `title()`: `Command::GitToggleDiffMode => "Git: Toggle Unified / Split diff".into(),`. `available()`: add `| Command::GitToggleDiffMode` to the arm ending `Command::GitFocusCommit => in_view(View::Git)`.

Run: `cargo test -p mkrew-ui --lib keymap -j 4` and `cargo test -p mkrew-ui --lib palette -j 4`
Expected: PASS (`every_default_parses_and_is_unique`, `names_round_trip`, `template_parses_to_the_defaults_commented_or_not` cover the entry through `all()`). If `main.rs` blocks the build, finish Step 9 first and run both then.

- [ ] **Step 9: Wire the shell (`main.rs`; read ranges, the file is ~18k lines)**

9a. Imports: after `use mkrew_core::prompt::PromptVars;` add `use mkrew_core::settings::DiffMode;`.

9b. Field: in `struct Shell`, after `git_diff_scroll: UniformListScrollHandle,` add

```rust
    /// How the Git diff viewer lays out a file (`[ui] diff_mode`, §12B); global, every repo.
    diff_mode: DiffMode,
```
In `Shell::new`, after `let settings_file = paths.settings_file();` add (a missing or broken file is the default; the daemon reports it, the tick below re-reads on a fix):

```rust
        let diff_mode = mkrew_core::settings::load(&settings_file)
            .unwrap_or_default()
            .ui
            .diff_mode;
```
and in the struct literal, after `git_diff_scroll: UniformListScrollHandle::new(),` add `diff_mode,`.

9c. Hot reload (the `Ok(changed) => {` arm of the 500 ms tick, after its `if let Some((fonts, mode)) = changed { … }` block, still inside `Ok(changed)`):

```rust
                                // `theme::reload` only returns fonts and theme; a hand edit of
                                // `diff_mode` applies too (the file just parsed, so this
                                // second read only happens when it changed).
                                if let Ok(settings) =
                                    mkrew_core::settings::load(&shell.settings_file)
                                {
                                    shell.diff_mode = settings.ui.diff_mode;
                                }
```

9d. New method next to `toggle_theme`:

```rust
    /// Switches the Git diff viewer to `mode` for every file and repo, keeps the current hunk
    /// in view (the two layouts have different row counts), and saves `[ui] diff_mode`. A
    /// settings file that does not parse keeps the choice for this run; the palette says why
    /// (§20.0).
    fn set_diff_mode(&mut self, mode: DiffMode, cx: &mut Context<'_, Self>) {
        if self.diff_mode == mode {
            return;
        }
        self.diff_mode = mode;
        let hunk_row = self
            .workspace
            .and_then(|project_id| self.git.get(&project_id))
            .and_then(|view| match (&view.diff, view.hunk) {
                (git_view::Diff::Shown(shown), Some(hunk)) => shown.hunk_row(hunk, mode),
                _ => None,
            });
        self.git_diff_scroll
            .scroll_to_item(hunk_row.unwrap_or(0), ScrollStrategy::Top);
        if let Err(error) = mkrew_core::settings::write_ui_value(
            &self.settings_file,
            "diff_mode",
            &format!("\"{}\"", mode.name()),
        ) {
            self.settings_error = Some(error.to_string());
        }
        cx.notify();
    }
```

9e. Dispatch (`Shell::run`, after the `Command::GitFocusCommit => { … }` arm):

```rust
            Command::GitToggleDiffMode => self.set_diff_mode(self.diff_mode.other(), cx),
```

9f. `git_key`: the hunk row lookup becomes `(git_view::Diff::Shown(shown), Some(hunk)) => shown.hunk_row(hunk, self.diff_mode),`.

9g. Shared pieces of the diff rows. Replace the `git_view::DiffRow::Line(line) => { … }` arm of `git_diff_row` with (the sign / text part moves into `diff_code`, so Split reuses it):

```rust
        git_view::DiffRow::Line(line) => {
            let (bg, fg, sign) = match line.kind {
                DiffLineKind::Add => (Some(theme.diff_add_bg), theme.diff_add_text, "+"),
                DiffLineKind::Del => (Some(theme.diff_del_bg), theme.diff_del_text, "−"),
                DiffLineKind::Context => (None, theme.text, " "),
            };
            let is_picked = line
                .new_line
                .zip(picked)
                .is_some_and(|(n, picked)| picked.contains(n, n));
            frame
                .when_some(bg, |el, bg| el.bg(rgb(bg)))
                .when(is_picked, |el| el.border_color(rgb(theme.accent)))
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .gap(px(space::S))
                        .text_color(rgb(theme.text_muted))
                        .child(number(line.old_line))
                        .child(
                            div()
                                .when(is_picked, |el| el.text_color(rgb(theme.accent)))
                                .child(number(line.new_line)),
                        ),
                )
                .child(diff_code(shown, index, line, sign, fg, theme))
                .when(line.no_newline, |el| {
                    el.child(
                        div()
                            .text_color(rgb(theme.text_muted))
                            .child("(no newline at end of file)"),
                    )
                })
        }
```

Add after `git_diff_row`:

```rust
/// A diff line's `sign` and text (tabs expanded, syntax colors of `shown.spans[index]`): the
/// part of a row after the numbers, in Unified and in each Split half. In a highlighted diff,
/// text outside tokens is plain text (the sign and background still tell added from removed);
/// unhighlighted diffs keep the diff text color `fg`.
fn diff_code(
    shown: &git_view::Shown,
    index: usize,
    line: &mkrew_protocol::DiffLine,
    sign: &'static str,
    fg: u32,
    theme: &Theme,
) -> gpui::Div {
    let spans = shown.spans.get(index).map(Vec::as_slice).unwrap_or(&[]);
    let text_color = if shown.highlighted { theme.text } else { fg };
    div()
        .flex()
        .gap(px(space::S))
        .child(div().text_color(rgb(fg)).child(sign))
        .child(div().text_color(rgb(text_color)).child(preview::code_line(
            line.text.replace('\t', highlight::TAB),
            spans,
            &[],
            theme,
        )))
}

/// Which half of a Split row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DiffSide {
    Old,
    New,
}

/// One half of a Split row: the old (left) or new (right) line `index` of `shown.rows`, or an
/// empty padding cell. `modified` = the row pairs a removed with an added line (amber on both
/// halves); otherwise removed / added lines keep the delete / add colors. Both halves have the
/// pick bar's width, so the columns line up; only the new side shows the picked bar.
fn split_half(
    shown: &git_view::Shown,
    index: Option<usize>,
    side: DiffSide,
    modified: bool,
    picked: Option<Selection>,
    theme: &Theme,
) -> gpui::Div {
    let half = div()
        .flex_1()
        .min_w_0()
        .h_full()
        .overflow_hidden()
        .flex()
        .items_center()
        .gap(px(space::M))
        .px(px(space::M))
        .border_l(px(theme::DIFF_PICK_BAR_WIDTH))
        .border_color(gpui::transparent_black())
        .whitespace_nowrap();
    let Some((at, git_view::DiffRow::Line(line))) =
        index.and_then(|at| Some((at, shown.rows.get(at)?)))
    else {
        return half;
    };
    let (bg, fg, sign) = match (line.kind, modified) {
        (DiffLineKind::Add, false) => (Some(theme.diff_add_bg), theme.diff_add_text, "+"),
        (DiffLineKind::Del, false) => (Some(theme.diff_del_bg), theme.diff_del_text, "−"),
        (DiffLineKind::Add, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "+"),
        (DiffLineKind::Del, true) => (Some(theme.diff_mod_bg), theme.diff_mod_text, "−"),
        (DiffLineKind::Context, _) => (None, theme.text, " "),
    };
    let number = match side {
        DiffSide::Old => line.old_line,
        DiffSide::New => line.new_line,
    };
    let is_picked = side == DiffSide::New
        && line
            .new_line
            .zip(picked)
            .is_some_and(|(n, picked)| picked.contains(n, n));
    half.when_some(bg, |el, bg| el.bg(rgb(bg)))
        .when(is_picked, |el| el.border_color(rgb(theme.accent)))
        .child(
            div()
                .flex_none()
                .text_color(rgb(if is_picked {
                    theme.accent
                } else {
                    theme.text_muted
                }))
                .child(match number {
                    Some(n) => format!("{n:>width$}", width = shown.number_width),
                    None => " ".repeat(shown.number_width),
                }),
        )
        .child(diff_code(shown, at, line, sign, fg, theme))
        .when(line.no_newline, |el| {
            el.child(
                div()
                    .text_color(rgb(theme.text_muted))
                    .child("(no newline at end of file)"),
            )
        })
}

/// A Split row that pairs lines (`Same` / `Change`; spans are drawn by [`git_diff_row`]):
/// old half, a 1 px divider, new half. `on_new_half` gets the new half and its new-side line
/// number (`None` for padding) so the caller can make it pickable for a comment.
fn git_split_row(
    shown: &git_view::Shown,
    row: &git_view::SplitRow,
    picked: Option<Selection>,
    line_height: f32,
    theme: &Theme,
    on_new_half: impl FnOnce(gpui::Div, Option<u32>) -> gpui::Div,
) -> gpui::Div {
    let (old, new, modified) = match row {
        git_view::SplitRow::Same(at) => (Some(*at), Some(*at), false),
        git_view::SplitRow::Change { old, new } => (*old, *new, old.is_some() && new.is_some()),
        git_view::SplitRow::Span(_) => (None, None, false),
    };
    let new_number = new.and_then(|at| match shown.rows.get(at) {
        Some(git_view::DiffRow::Line(line)) => line.new_line,
        _ => None,
    });
    div()
        .w_full()
        .h(px(line_height))
        .flex()
        .items_center()
        .child(split_half(shown, old, DiffSide::Old, modified, picked, theme))
        .child(
            div()
                .flex_none()
                .w(px(theme::BORDER_WIDTH))
                .h_full()
                .bg(rgb(theme.border)),
        )
        .child(on_new_half(
            split_half(shown, new, DiffSide::New, modified, picked, theme),
            new_number,
        ))
}

/// Makes `el` (a diff row, or the new half of a Split row) pick new-side line `number` for a
/// comment on click; hover previews the pick bar unless the line is already picked.
fn pick_on_click(
    el: gpui::Div,
    project_id: Ulid,
    number: u32,
    picked: Option<Selection>,
    border: u32,
    cx: &Context<'_, Shell>,
) -> gpui::Div {
    el.cursor_pointer()
        .when(!picked.is_some_and(|p| p.contains(number, number)), |el| {
            el.hover(move |style| style.border_color(rgb(border)))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |shell, event: &gpui::MouseDownEvent, window, cx| {
                shell.pick_diff_line(project_id, number, event.modifiers.shift, window, cx);
            }),
        )
}
```

9h. Body of the diff. In `render_git_diff`, replace the whole `git_view::Diff::Shown(shown) => { … }` arm that follows the `rows.is_empty()` arm (the one building `uniform_list("git-diff", rows, …)` to `.into_any_element()`) with:

```rust
            git_view::Diff::Shown(shown) => {
                let shown = Rc::clone(shown);
                let mode = self.diff_mode;
                let current = view.hunk;
                // Hunk headers get a stage / unstage action when the file can go by hunk.
                let hunk_stage = view.hunk_change(0).map(|change| change.stage);
                let picked = view.comments.selection;
                let can_comment = view.can_comment();
                let line_height = theme.terminal_font_size * theme.terminal_line_height;
                let row_theme = theme.clone();
                let rows = shown.list_len(mode);
                uniform_list(
                    "git-diff",
                    rows,
                    cx.processor(move |_, range: std::ops::Range<usize>, _, cx| {
                        range
                            .filter_map(|index| {
                                Some(match mode {
                                    DiffMode::Unified => {
                                        let row = shown.rows.get(index)?;
                                        let frame = git_diff_row(
                                            &shown,
                                            index,
                                            current,
                                            picked,
                                            line_height,
                                            &row_theme,
                                        );
                                        match (row, hunk_stage) {
                                            (git_view::DiffRow::Hunk(hunk), Some(stage)) => frame
                                                .child(hunk_stage_button(
                                                    project_id, *hunk, stage, &row_theme, cx,
                                                ))
                                                .into_any_element(),
                                            // New-side lines can be picked for a comment; hover
                                            // previews the pick bar.
                                            (git_view::DiffRow::Line(line), _) => {
                                                match line.new_line.filter(|_| can_comment) {
                                                    Some(number) => pick_on_click(
                                                        frame,
                                                        project_id,
                                                        number,
                                                        picked,
                                                        row_theme.border,
                                                        cx,
                                                    )
                                                    .into_any_element(),
                                                    None => frame.into_any_element(),
                                                }
                                            }
                                            _ => frame.into_any_element(),
                                        }
                                    }
                                    DiffMode::Split => match shown.split.get(index)? {
                                        // Gap and hunk rows span both halves.
                                        git_view::SplitRow::Span(at) => {
                                            let frame = git_diff_row(
                                                &shown,
                                                *at,
                                                current,
                                                picked,
                                                line_height,
                                                &row_theme,
                                            );
                                            match (shown.rows.get(*at)?, hunk_stage) {
                                                (git_view::DiffRow::Hunk(hunk), Some(stage)) => {
                                                    frame
                                                        .child(hunk_stage_button(
                                                            project_id, *hunk, stage, &row_theme,
                                                            cx,
                                                        ))
                                                        .into_any_element()
                                                }
                                                _ => frame.into_any_element(),
                                            }
                                        }
                                        // Only the new (right) half is pickable: comments
                                        // anchor on new-side lines.
                                        row => git_split_row(
                                            &shown,
                                            row,
                                            picked,
                                            line_height,
                                            &row_theme,
                                            |half, number| match number.filter(|_| can_comment) {
                                                Some(number) => pick_on_click(
                                                    half,
                                                    project_id,
                                                    number,
                                                    picked,
                                                    row_theme.border,
                                                    cx,
                                                ),
                                                None => half,
                                            },
                                        )
                                        .into_any_element(),
                                    },
                                })
                            })
                            .collect()
                    }),
                )
                .track_scroll(&self.git_diff_scroll)
                .flex_1()
                .min_h_0()
                .pb(px(space::L))
                .font_family(theme.terminal_font_family.clone())
                .text_size(px(theme.terminal_font_size))
                .into_any_element()
            }
```

9i. Header toggle. In `render_git_diff`, directly before `let header = div()`:

```rust
        // Unified | Split (global, remembered): only for a diff that has lines to lay out.
        let has_lines = shown
            .as_ref()
            .is_some_and(|shown| !shown.diff.binary && !shown.diff.too_large && !shown.rows.is_empty());
        let diff_mode = self.diff_mode;
        let mode_keys = self.keymap.label(Command::GitToggleDiffMode);
        // One step lighter than the control's `bg`, so hovering never looks selected.
        let mode_hover = theme.surface;
        let mode_segment =
            |id: &'static str, label: &'static str, tip: &'static str, this: DiffMode| {
                let active = diff_mode == this;
                div()
                    .id(id)
                    .flex_none()
                    .whitespace_nowrap()
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .when(active, |el| {
                        el.bg(rgb(theme.surface_raised))
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.text))
                    })
                    .when(!active, |el| {
                        el.cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .hover(move |style| style.bg(rgb(mode_hover)))
                            .active(|style| style.opacity(opacity::ACTIVE))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.git_focus, cx);
                                shell.set_diff_mode(this, cx);
                            }))
                    })
                    .tooltip(tooltip(theme, tip, mode_keys.clone()))
                    .child(label)
            };
        let mode_toggle = div()
            .flex_none()
            .flex()
            .rounded(px(radius::SMALL))
            .bg(rgb(theme.bg))
            .child(mode_segment(
                "git-diff-unified",
                "Unified",
                "One column: removed lines, then added lines",
                DiffMode::Unified,
            ))
            .child(mode_segment(
                "git-diff-split",
                "Split",
                "Two columns: old on the left, new on the right",
                DiffMode::Split,
            ));
```

and in the `header` chain, right after `.children(hunk_label.map(|label| { … }))`:

```rust
            .when(has_lines, |header| header.child(mode_toggle))
```

Run: `cargo check -p mkrew-ui` (expected: no errors; fix only what the compiler names, keep the structure), then `cargo test -p mkrew-ui --lib -j 4`
Expected: PASS.

- [ ] **Step 10: SPEC**

§12B, replace the Diff viewer bullet (SPEC.md:885-886):

```markdown
- **Diff viewer:** unified or split (side-by-side), hunk navigation, collapsed unchanged regions, syntax
  highlighting. Diffs > 1 MB show a summary and "Open in Zed".
- **Diff modes:** a Unified | Split toggle in the diff header (key `d`, §13) switches between one
  column (removed lines, then added lines) and two columns (old on the left, new on the right). Split
  is one virtualized list of paired rows, so one scroll handle moves both sides. In a hunk the i-th
  removed line is paired with the i-th added line and the shorter side is padded with an empty cell;
  unchanged lines show on both sides; gap and hunk-header rows span both columns. Paired rows (a line
  changed in place) use the *modified* colors, an unpaired removed / added line the delete / add
  colors; Unified keeps `−` then `+` in the delete / add colors. Hunk navigation (`n` / `p`) and
  diff comments work in both modes (comments pick new-side lines: the right column in Split). The
  mode is global (every file, every repo of the workspace, History too) and is remembered as
  `[ui] diff_mode` (§20.0).
```

§12B Extra tokens bullet (:907-908) becomes:

```markdown
- **Extra tokens:** `diff-add-bg`, `diff-add-text`, `diff-del-bg`, `diff-del-text`,
  `diff-mod-bg`, `diff-mod-text` (a line changed in place, Split), `diff-hunk-header` (dark + light)
  in `theme.rs`. Diff backgrounds stay very subtle.
```

§13 Git keys, after the `| Git | Toggle History | h |` row (:973):

```markdown
| Git | Toggle Unified / Split diff | d |
```

Backlog (:1327-1328): remove `side-by-side diff; ` so the line reads

```markdown
over SSH; compact density; `gix` git backend; pull/rebase/branch switching and
```

§20.0, after the `theme = "system"` line of the `[ui]` example (:1374):

```markdown
diff_mode = "unified"       # §12B; "unified" | "split"; the Git diff header toggle writes it
```

- [ ] **Step 11: HUMAN_CHECKS**

Append to the section `### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)` in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Git diff Unified / Split:** in Git, select a changed text file. The header shows
  `Unified | Split` (not for a binary or too-large file). Click Split: two columns, old on the
  left and new on the right, one scroll bar moving both; a line edited in place is amber on both
  sides, a new line is green on the right with an empty left cell, a removed line is red on the
  left with an empty right cell, unchanged lines show on both sides, `⋯ N unchanged lines` and the
  hunk header span the full width (Stage hunk still on it). Both themes: text readable on all
  colors. Narrow window: long lines clip inside their half, nothing overlaps.
- [ ] **Diff mode is remembered and global:** pick Split, select other files, switch to History and a
  commit's file, switch the repo chip of a workspace with several repos: still Split. Quit and
  restart: still Split; `settings.toml` has `diff_mode = "split"`. Edit it to `"unified"` while the
  app runs: the diff changes within a second. `diff_mode = "side"` shows a settings error in the
  palette and keeps the current mode.
- [ ] **Diff keys and comments in both modes:** with the Git list focused, `d` toggles the mode (the
  tooltip and the palette entry "Git: Toggle Unified / Split diff" show it); `n` / `p` bring the
  hunk header to the top in both; switching mode keeps the current hunk in view. In Split, click a
  right-column line: accent bar and number on the right half; Shift+click extends; Ctrl+Alt+M adds
  a comment that quotes those lines; clicking a left-only (removed) line does nothing.
```

- [ ] **Step 12: Verify and tick**

Run the full verification (`"C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`). Run the HUMAN_CHECKS lines above in the real app (headless tests cover the pairing, settings and tokens only). Tick inbox line 107 `- [x] **10. Git module: diff view modes**` (last task of the item).
Suggested message: `feat(ui): Git diff Unified / Split modes, remembered in [ui] diff_mode`

---

## Open questions (defaults used above)

| # | Area | Question | Options | Default in this plan |
| --- | --- | --- | --- | --- |
| 1 | Title bar (T1) | Idle Run button style | (a) green fill for ▶ Run and ▾, amber ↻, red ■, tinted hovers (new `danger_bg`) · (b) keep the accent fill, color only the glyphs | **(a)** |
| 2 | Title bar (T3) | Linux | (a) keep the OS title bar, no drawn controls · (b) client-side decorations with drawn controls and resize edges (~150 untestable lines) | **(a)** |
| 3 | Title bar (T3) | Pressed state of ─ □ ✕ on Windows | hover only (the OS owns the click) · custom pressed visuals (not exposed by GPUI) | hover only |
| 4 | Title bar (T2) | A repo whose `git.status` fails / many child repos | left out and logged; no cap on repos · cap at ~12 | left out; no cap |
| 5 | Title bar (T3) | Sidebar header once the logo moves to the title bar | keep "Workspaces +" (30 px), drop the logo · keep both | drop the logo |
| 6 | Usage (T5) | omp `context_window` (not in omp's file) | (a) 200000 in `agents/omp.toml`, user edits it · (b) per-model table | **(a)** |
| 7 | Usage (T4) | Meaning of `input` on the wire | (a) fresh input only, cache in its own fields (ADR 0035) · (b) keep the total, cache as subsets | **(a)**, as the inbox asks |
| 8 | Usage (T5) | omp context size | input + cacheRead + cacheWrite of the newest call (= Claude) · `totalTokens` | the first |
| 9 | Status bar (T6) | Toggle-sidebar key | **Ctrl+Shift+B** · Ctrl+Alt+B · Ctrl+B (steals ^B from terminals, against SPEC §13) | Ctrl+Shift+B |
| 10 | Status bar (T6) | Remember a hidden sidebar across restarts | in memory only · `[ui] sidebar` key | in memory |
| 11 | Status bar (T6) | Elapsed time | shown for working / idle only, hidden otherwise · always | working / idle |
| 12 | Sidebar (T8) | Palette size and protocol | (a) add `lime` (9 hues), amend ADR 0020, no protocol bump (a new UI on a not-yet-restarted old daemon cannot set `lime`) · (b) 10 hues · (c) also bump the protocol | **(a)** |
| 13 | Sidebar (T9) | Filter / chip behaviour | in memory; while filtering, workspaces without a match hide and the rest open · persisted | in memory |
| 14 | Sidebar (T7) | Test for the Change-color fix | HUMAN_CHECKS line (mkrew-ui has no GPUI test harness) · add a harness | HUMAN_CHECKS |
| 15 | Tabs + panes (T11) | Breadcrumb "folder" | (a) a task pane's worktree branch, omitted for plain / run panes · (b) persist `Session.cwd` (migration) | **(a)** |
| 16 | Tabs + panes (T11) | Status in the pane (inbox item's own question) | (a) one pill on the left, Done keeps the turn summary · (b) no status in the pane | **(a)** |
| 17 | Tabs + panes (T12) | Where a pane color shows | (a) tinted agent chip + 2 px line under the header · (b) whole header tint · (c) pane border (clashes with the state border) | **(a)** |
| 18 | Tabs + panes (T10) | Tab icon with several panes; "Close pane" button | icon of the focused pane; the "Close pane" button stays · fold close into ⋯ | as stated |
| 19 | Pin (T14) | Pin glyph | 📌 (color emoji via Segoe UI Emoji, ignores text color) · a monochrome symbol | 📌, one HUMAN_CHECKS line |
| 20 | Pin (T14) | Right-click on a sidebar session row | open the session, then its pane menu (switches workspace) · menu without switching (Rename needs the visible header) | open first |
| 21 | Pin (T13) | Store / `mk` order | unchanged (only the UI sorts pinned first) · `ORDER BY pinned DESC` | unchanged |
| 22 | Session panel (T15) | Open transcript | Zed (existing helper) · OS default app (`.jsonl` often has none) · copy the path | Zed |
| 23 | Session panel (T15) | Key and behaviour outside Terminals | **Ctrl+Shift+Y**; switches to Terminals and opens · no-op elsewhere | as stated |
| 24 | Session panel (T15) | What is remembered | width only (`[ui] inspector_width`); closed at every start · also the open state | width only |
| 25 | Session panel (T15) | Stop | asks only for a running agent; a shell stops at once · always ask | as stated |
| 26 | Plan checklist (T17) | Link scope and path form | live sessions only; project-relative when inside the workspace, else absolute | as stated |
| 27 | Plan checklist (T17) | Wire caps / bad file | 200 items × 120 chars, exact counts; an unreadable or deleted file keeps the link and shows an error | as stated |
| 28 | Plan checklist (T18) | Which sessions show the Plan card | agent sessions only · every session | agents only |
| 29 | Files (T19) | "Files module" menu and folders | right-click on the empty tree area = root folder menu; files are selected in Explorer, folders opened | as stated |
| 30 | Terminal links (T20) | Link syntax and base folder | `path:line[:col]` (col dropped), no `file(line)`; relative to the project folder or the task worktree | as stated |
| 31 | Markdown preview (T21) | Toggle key | header control only; Find and open-at-line switch to Source · a key | header only |
| 32 | Diff modes (T22) | Modified lines in Split | new amber `diff_mod_bg` / `diff_mod_text` on paired rows · keep red / green | new tokens |
| 33 | Diff modes (T22) | Toggle key | `d` in the Git context · palette and header only | `d` |
| 34 | Diff modes (T22) | Intra-line highlight / horizontal scroll per side | neither (whole-line colors, long lines clip) · add them | neither |
