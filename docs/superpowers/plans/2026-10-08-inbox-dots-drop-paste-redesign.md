# Inbox 2026-10-08 (3): Working Dots, File Drop, Ctrl+V, Agent Icons, Logo, Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Handle the seven `[ ]` items in `docs/INBOX.md` (lines 18–23 and 27): status icons, file drop, Ctrl+V paste, agent icons, a dot animation for `working`, the new logo everywhere, and the calm visual redesign.

**Architecture:**
- Items 18 and 22 touch the same code (`status::icon` and its 12 render sites). Task 1 replaces the ◐◓◑◒ spinner with three dots that take turns on the existing 500 ms status tick. It adds one `status_mark` element used everywhere. The missing `done` / `needs input` states have a setup cause: no hook integration is installed (checked 2026-10-08). That is a user step.
- Item 19: the drop code exists, but only the terminal grid accepts drops. The composer ("terminal chat" box), the header and the placeholder ignore them. GPUI 0.3.7 also loses drops after keyboard input (verified: `window.rs:5415` keeps keyboard modality for `FileDrop`, `div.rs:2903` then skips `on_drop`). Task 2 adds one pane-level handler that does not depend on the hover check.
- Item 20: Ctrl+V sends a raw `^V` today. Only Ctrl+Shift+V pastes, and Windows bitmaps (PrintScreen, `CF_DIB` → `ImageFormat::Bmp`) are silently dropped. Task 3 makes plain Ctrl+V a smart paste (ADR 0026) and converts BMP to PNG.
- Item 21: badges exist (ADR 0025), but the user's profile files predate the `icon` key. The UI shows C/C/O/C, and three of those letters are the same. Task 4 makes the daemon fall back to the shipped sample's `icon`. It also draws the badge as a chip and shows it in more places.
- Item 27: the redesign (Tasks 5–10). Theme becomes a GPUI global with System/Light/Dark mode. The right view rail is replaced by a 44 px toolbar (reverses ADR 0023, see ADR 0027). The tab strip gets a ✕ per tab. The sidebar, inbox card and empty state get the new look. Colors stay `u32` tokens in `theme.rs` (SPEC §14); there is no Hsla rewrite.
- Item 23: the new logo. The `mkrew.exe` icon is already done (uncommitted user work: `crates/mkrew-ui/build.rs`, `resources/windows/`). Task 11 adds what is left: the installer icons (Inno + WiX), the glyph in the sidebar header, the README lockup, and how to regenerate the `.ico`. It runs after the redesign, so the glyph lands in the new header.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), serde/toml, insta. One new direct dependency, `image` (already in `Cargo.lock` through gpui, Task 3, Open question 6).

**Spec:** `docs/SPEC.md` §6, §10, §11, §13, §14, §20.0, §20B.5, cited per task. The redesign brief is the INBOX item itself (`docs/INBOX.md:27-100`). Its reference files are at `design/theme.rs`, `design/sidebar.rs` and `design/app_shell.rs` (the brief says `design/reference/…`; that folder does not exist). Open questions are at the end, each with the default this plan uses.

**Inbox mapping:**

| Inbox line | Task |
| --- | --- |
| 18 `Should have icon the the agent that showing when agent inprogress, iddle, done, or ask me` | 1 |
| 19 `Check if we don't have feature to drag drop a file into the terminal chat …` | 2 |
| 20 `Check if we can Ctrl+V to paste text or image into the terminal chat …` | 3 |
| 21 `For the agent in the workspace do we have icon for each agent …` | 4 |
| 22 `In case agent running, we're shoing circle loop icon … multiple dot aninamtion` | 1 |
| 23 `[feat] Use the new Mkrew logo everywhere …` | 11 |
| 27 `I want to redesign the look of this GPUI desktop app …` | 5, 6, 7, 8, 9, 10 |

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1). Every agent difference lives in its TOML profile — no `match "claude"` anywhere in Rust code.
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change. Next free ADR number: 0026.
- Few dependencies: add no new crates, except `image` in Task 3 (Open question 6).
- GPUI rule: set `.hover()` at most once per element (`button()`/`quiet_button()`/`row_action()`/`menu_item()` already set it). `group_hover` is a separate slot and is safe.
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use; do not run `git commit`.
- Leave the existing uncommitted human edits alone (`.github/workflows/*`, `Cargo.lock`, `crates/mkrew-ui/Cargo.toml`, `crates/mkrew-ui/build.rs`, `crates/mkrew-ui/resources/`, `packaging/mkrew.iss`, `docs/BUILD_VI.md`, `design/`, `icon/`). Exceptions: Task 3 adds one line to `crates/mkrew-ui/Cargo.toml` and Cargo updates `Cargo.lock` (Open question 6); Task 11 adds one `SetupIconFile=` line to `packaging/mkrew.iss` and two doc-comment lines to `build.rs`. Change nothing else in those files.
- Never write files outside this repo. Hook installs (`mk integration install …`) and edits to the user's own `<config>/mkrew/*` files are **user steps**, listed in HUMAN_CHECKS.
- **Inbox:** when a task passes verification, change its `docs/INBOX.md` line from `[~]` to `[x]` (see the mapping above). Line 27 is ticked after Task 10.
- Verify on Windows with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these three in order:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace -j 4`
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.
- `crates/mkrew-ui/src/main.rs` is ~16.4k lines: read ranges, never the whole file. Line numbers below are from 2026-10-08 and drift as tasks land. Re-find each function by name (`grep -n "fn session_row"`).

## Review Focus

1. **Fixed heights vs. `[ui] font_size`** (ADR 0024: 8–32 px). The design's 44/38/30/28 px boxes clip text at large font sizes. Expected: they scale. Pinned by Task 5 `layout_heights_scale_with_the_ui_font` (the heights are rems: `44/13 rem`, …) and the manual check at `font_size = 20`.
2. **A broken or hand-edited `settings.toml` when the theme toggle writes it.** Expected: comments and other keys survive. A file that does not parse is never written; the toggle still switches for the session and the palette shows the error. Pinned by Task 5 `set_ui_theme_keeps_comments_and_other_keys` and `set_ui_theme_inserts_ui_section`.
3. **Light-mode contrast on the new surfaces** (sidebar, hover, selected, segment). Expected: body/muted text ≥ 4.5:1 on each. Pinned by Task 5 extending `body_text_contrast_is_at_least_4_5` to the new tokens.
4. **Ctrl+V in a shell or vim with an empty clipboard.** Expected: `^V` still reaches the program (quoted insert / block visual). Pinned by Task 3 `plain_ctrl_v_is_paste_only_while_live` (key level) plus the fallback in `TerminalPane::paste` (manual check).
5. **Clicking a tab's ✕.** Expected: it closes that tab without activating it first; closing the last tab behaves like `CloseTab` today. Pinned by Task 9 `tabs::tests::remove_inactive_keeps_active` (pure `Tabs`) and the manual check.

---

## File Structure

| File | Change | Task |
| --- | --- | --- |
| `crates/mkrew-ui/src/status.rs` | `working_dot`, `WORKING_DOTS`; `icon(state)` without `now` | 1 |
| `crates/mkrew-ui/src/sidebar.rs` | Working glyph `•••`; `agent_badge` unchanged | 1 |
| `crates/mkrew-ui/src/main.rs` | `status_mark`; drop handler; agent chip; theme global use; toolbar; sidebar; tab ✕; empty state | 1, 2, 4–10 |
| `crates/mkrew-ui/src/terminal/view.rs` | drop handler removed; `type_text` pub; `paste()`; theme from global | 2, 3, 5 |
| `crates/mkrew-ui/src/terminal/input.rs` | `KeyAction::Paste`, `is_paste_key` | 3 |
| `crates/mkrew-ui/src/composer.rs` | `attachment()` (BMP → PNG) | 3 |
| `crates/mkrew-ui/Cargo.toml` | `image` | 3 |
| `crates/mkrew-daemon/src/lib.rs`, `server.rs` | `sample_icon` fallback | 4 |
| `crates/mkrew-ui/src/workspace_menu.rs` | `AgentChoice.icon` | 4 |
| `crates/mkrew-ui/src/theme.rs` | Global `Theme`, `ThemeMode`, new tokens, `ActiveTheme`, sizes | 5–10 |
| `crates/mkrew-core/src/settings.rs` | `UiSettings.theme`, `set_ui_theme` | 5 |
| `crates/mkrew-ui/src/keymap.rs`, `palette.rs` | `Command::ToggleTheme` | 5 |
| `crates/mkrew-ui/src/tabs.rs` | test for removing an inactive tab | 9 |
| `docs/decisions/0026-ctrl-v-pastes.md`, `0027-calm-redesign.md`, `0025-agent-badges.md` | ADRs | 3, 4, 5 |
| `docs/SPEC.md` | §6, §10, §11, §13, §14, §20.0, §20B.5 | 1–10 |
| `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`, `docs/INBOX.md` | checks, notes, ticks | 1–11 |
| `packaging/mkrew.iss`, `packaging/mkrew.wxs` | installer icon | 11 |
| `crates/mkrew-ui/src/main.rs` (sidebar header), `README.md`, `crates/mkrew-ui/build.rs` (doc) | logo glyph, lockup, regen note | 11 |

---

### Task 1: Working dots everywhere; hook setup for done / needs input

Inbox: line 18 `Should have icon the the agent that showing when agent inprogress, iddle, done, or ask me` and line 22 `In case agent running, we're shoing circle loop icon …`. Spec: §6 (state sources, :436-476), §11 (:723-744), §14 (:930 "Small, slow spinner").

**Current state (investigated):**
- **Root cause of item 18:** every state already has a glyph and a color (`sidebar::status_display` sidebar.rs:19-30, `status_color` :33-42). `done` comes only from a hook (SPEC §6:441), and omp/copilot have `needs_input_patterns = []`. On this machine no Mkrew hook is installed. Checked 2026-10-08:
  - `~/.claude/settings.json` has only Orca/herdr hooks.
  - `~/.codex/config.toml` has no `notify` line.
  - `~/.omp/agent/extensions/` has no `mkrew.ts`.
  - `~/.copilot/hooks/` has no `mkrew.json`.
  - So sessions only show working ↔ idle. The fix is `mk integration install claude|codex|omp|copilot` plus restarting the agents. It writes outside the repo, so it is a user step (Step 7).
- **Item 22:** `status::icon` (status.rs:25-32) cycles `SPINNER = ["◐","◓","◑","◒"]` per `theme::STATUS_TICK` (500 ms). The tick loop (main.rs:396-438) notifies `Shell` every tick while anything works. Glyph sites in main.rs (re-find by function name):
  - `render_peek` 7525
  - `render_send_dialog` 12161
  - `render_history` 12895 (static Exited)
  - `render_tab_bar` 13118
  - `workspace_row` 14955 (count) and 14965 (`?`)
  - `session_row` 15033
  - `inbox_row` 15129
  - `task_card` 15495 and 15517 (static Exited)
  - `fan_out_child_row` 15880 (fixed 12 px cell)
  - `pane_header` ~16104
- `with_animation` is not used: it requests a frame every vsync, and with no `.cached()` views every frame re-renders the whole Shell (gpui animation.rs:444-470). Three dots stepping on the existing tick cost what the spinner costs today: 2 renders/s while something works. A cycle is 1.5 s, close to the mock's 1.4 s.
- Other states keep their glyphs (○ ring, `?`, ✓, ✕). The tab label shows no text, so a colored dot there would be status by color alone (§11:725, §14:986). Open question 2.

**Files:**
- Modify: `crates/mkrew-ui/src/status.rs:13-32`, test :172-180
- Modify: `crates/mkrew-ui/src/sidebar.rs:22`
- Modify: `crates/mkrew-ui/src/theme.rs` (after `STATUS_TICK` :66)
- Modify: `crates/mkrew-ui/src/main.rs` (new fn near `status_tooltip`; the 9 animated sites above)
- Modify: `docs/SPEC.md` §11 :731, :741; `docs/HUMAN_CHECKS.md:327`

**Interfaces:**
- Produces: `status::WORKING_DOTS: usize = 3`; `status::working_dot(now_ms: i64) -> usize`; `status::icon(state: SessionState) -> &'static str` (no `now`); `fn status_mark(state: SessionState, now: i64, theme: &Theme) -> gpui::AnyElement` in main.rs (Tasks 7 and 8 reuse it); tokens `theme::STATUS_DOT: f32 = 4.0`, `theme::STATUS_DOT_GAP: f32 = 2.0`, `opacity::STATUS_DOT_DIM: f32 = 0.35`, `theme::STATUS_MARK_WIDTH: f32 = space::L`.

- [ ] **Step 1: Rewrite the failing test**

In `crates/mkrew-ui/src/status.rs` tests, replace `spinner_turns_only_while_working` with:

```rust
    #[test]
    fn working_dots_take_turns_one_per_tick() {
        let step = theme::STATUS_TICK.as_millis() as i64;
        let lit: Vec<usize> = (0..4).map(|i| working_dot(i * step)).collect();
        assert_eq!(lit, [0, 1, 2, 0]);
        // Mid-tick stays on the same dot; a clock before 1970 is dot 0.
        assert_eq!(working_dot(step + step / 2), 1);
        assert_eq!(working_dot(-5), 0);
        assert_eq!(icon(SessionState::Idle), "○");
        assert_eq!(icon(SessionState::NeedsInput), "?");
    }
```

Run: `cargo test -p mkrew-ui --lib status::tests::working_dots -j 4`
Expected: FAIL to compile (`cannot find function working_dot`).

- [ ] **Step 2: Implement in status.rs**

Replace `SPINNER` and `icon` (status.rs:13-32) with:

```rust
/// Dots of the `working` mark (SPEC §11); one is lit per [`theme::STATUS_TICK`].
pub const WORKING_DOTS: usize = 3;

/// The lit dot of the `working` mark at `now_ms`: steps on the status tick, so the mark costs
/// no more redraws than the old spinner.
pub fn working_dot(now_ms: i64) -> usize {
    let step = theme::STATUS_TICK.as_millis().max(1) as i64;
    (now_ms.max(0) / step) as usize % WORKING_DOTS
}

/// The state's glyph; `working` is drawn as dots (see [`working_dot`]) wherever there is room.
pub fn icon(state: SessionState) -> &'static str {
    status_display(state).icon
}
```

Update the module doc (status.rs:1) `spinner` → `working dots`. In `sidebar.rs:22`, change `("◐", "working")` to `("•••", "working")`; `every_state_has_an_icon_and_text` still passes (icons stay distinct).

- [ ] **Step 3: Tokens**

`theme.rs`, after `STATUS_TICK`:

```rust
/// Diameter of one dot of the `working` mark (SPEC §11).
pub const STATUS_DOT: f32 = 4.0;
/// Gap between the dots of the `working` mark.
pub const STATUS_DOT_GAP: f32 = 2.0;
/// Width of every status mark, so glyph and dot rows line up (3 × 4 + 2 × 2 = 16).
pub const STATUS_MARK_WIDTH: f32 = space::L;
```

In `pub mod opacity`, after `DISABLED`:

```rust
    /// The unlit dots of the `working` mark.
    pub const STATUS_DOT_DIM: f32 = 0.35;
```

- [ ] **Step 4: One element for every status mark**

In main.rs, next to `status_tooltip`:

```rust
/// A session's status mark (SPEC §11): three dots taking turns while working, else the glyph.
/// Fixed width, so rows with different states line up.
fn status_mark(state: SessionState, now: i64, theme: &Theme) -> gpui::AnyElement {
    let color = rgb(sidebar::status_color(theme, state));
    let mark = div()
        .flex_none()
        .w(px(theme::STATUS_MARK_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    if state != SessionState::Working {
        return mark.text_color(color).child(status::icon(state)).into_any_element();
    }
    let lit = status::working_dot(now);
    mark.gap(px(theme::STATUS_DOT_GAP))
        .children((0..status::WORKING_DOTS).map(|i| {
            div()
                .size(px(theme::STATUS_DOT))
                .rounded_full()
                .bg(color)
                .opacity(if i == lit { opacity::FULL } else { opacity::STATUS_DOT_DIM })
        }))
        .into_any_element()
}
```

At each animated site, replace the `div().….text_color(status color).child(status::icon(state, now))` child with `status_mark(state, now, theme)`. Keep any tooltip or id the old div had: wrap the mark (`div().id(…).tooltip(…).child(status_mark(…))`):
- `render_peek`
- `render_send_dialog`
- `render_tab_bar`
- `session_row`
- `inbox_row`
- `task_card`
- `fan_out_child_row`: delete its `w(px(space::M))` cell; the mark has its own width
- `pane_header`

The static sites (`render_history`, the `task_card` error row, the workspace row `?`) call `status::icon(state)` without `now`.

Workspace row count (`workspace_row`, the `format!("{} {}", status::icon(Working, now), row.working)` child):

```rust
        .when(row.working > 0, |el| {
            el.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_working))
                    .child(status_mark(SessionState::Working, now, theme))
                    .child(row.working.to_string()),
            )
        })
```

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`. The compiler lists every remaining `status::icon(_, now)` call. Expected: clean.

- [ ] **Step 5: Tests**

Run: `cargo test -p mkrew-ui --lib status sidebar -j 4`
Expected: PASS (`working_dots_take_turns_one_per_tick`, `redraws_only_when_the_shown_status_changes` unchanged, `every_state_has_an_icon_and_text`).

- [ ] **Step 6: SPEC**

- §11 :731: replace "small spinner" with `three dots taking turns, one step per status tick (500 ms)`.
- §11 :741: replace "with the spinner" with `with the working dots`.
- Status table glyph for working: `•••`.
- HUMAN_CHECKS.md:327: replace "slowly turning spinner" with "three dots taking turns".

- [ ] **Step 7: Manual checks and setup (Task 10 records them)**

The user setup for item 18 goes into the Task 10 HUMAN_CHECKS block.

- [ ] **Step 8: Verify and tick**

Run the full verification.
- Tick line 22 `[x]`.
- Tick line 18 only after the user confirms the hook check. Until then leave it `[~]` and say so in the report.

Suggested message: `feat(ui): working state shows three dots taking turns instead of a spinner`

---

### Task 2: File drop — the whole pane accepts it, the composer gets the paths, keyboard focus no longer loses drops

Inbox: line 19 `Check if we don't have feature to drag drop a file into the terminal chat if not need to add in`. Spec: §20B.5 (SPEC.md:1371-1375).

**Current state (investigated):**
- The only `ExternalPaths` drop target is `TerminalPane`'s root div (`terminal/view.rs:627`, `.on_drop(cx.listener(Self::drop_paths))`, `drop_paths` :541-555). The composer (`composer_panel` main.rs:13468, "Message the agent…"), the pane header and the placeholder are siblings in `pane_frame` (main.rs:13320-13463) and have no handler. Windows still shows the Copy cursor everywhere (gpui-pre-windows window.rs:1215, 1259 always return `DROPEFFECT_COPY`). So a drop on the chat box silently does nothing.
- With the composer open, a drop on the grid types into the agent's own input line and takes focus from the composer (`self.focus(window, cx)` view.rs:554).
- **GPUI bug (verified in source):** `Window::dispatch_event` (gpui-pre window.rs:5415) sets the input modality from the raw event, and `PlatformInput::FileDrop` keeps the old one. During an OLE drag Mkrew gets no `WM_MOUSEMOVE`. If the last input was a key press, `Hitbox::is_hovered` returns false (window.rs:778). Then `on_drop` (div.rs:2903, `hitbox.is_hovered(window)`) never fires, and the drop is lost without any message.
- Ruled out:
  - OLE init: `OleInitialize` is at platform.rs:166.
  - Overlays: no `occlude` covers panes.
  - DPI handling.
  - Elevation: `PrivilegesRequired=lowest`; an admin launch would still block drops (UIPI).

**Fix:** one handler on the `pane_frame` outer div that does not use the hover check:
- `on_drag_move::<ExternalPaths>` (Capture phase, no hover gate, div.rs:364-385) remembers which pane is under the cursor.
- `capture_any_mouse_up` (no hover gate) delivers the drop while `cx.has_active_drag()`.
- The pane-level `on_drop` goes away, so there is one path.

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/view.rs:541-569` (remove `drop_paths`, make `type_text` pub), :627 (remove `.on_drop`), imports
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell` field + init, `Shell::drop_paths` next to `composer_typed` :3226, `pane_frame` :13341)
- Modify: `docs/SPEC.md` §20B.5 :1371-1375

**Interfaces:**
- Produces: `TerminalPane::type_text(&mut self, text: &str, cx: &mut Context<'_, Self>)` (pub; no-op for an exited pane or empty text); `Shell.drop_hover: Option<(Ulid, ExternalPaths)>`; `Shell::drop_paths(&mut self, session: Ulid, paths: &ExternalPaths, window, cx)`.

- [ ] **Step 1: Confirm the routing helper is covered**

Run: `cargo test -p mkrew-ui --lib composer::tests::typed_paths -j 4`
Expected: PASS. `typed_paths` stays the only formatting logic. The routing is a two-way branch on GPUI state and has no headless harness (mkrew-ui does not enable gpui `test-support`), so it is covered by the manual checks in Task 10.

- [ ] **Step 2: TerminalPane — one typing entry point**

In `terminal/view.rs`, delete `drop_paths` (:541-555) and the `.on_drop(cx.listener(Self::drop_paths))` line (:627). Remove `ExternalPaths` from the imports if it becomes unused. Replace `type_paths` (:557-569) with:

```rust
    /// Types `text` as one paste into a live session (dropped or pasted paths, §20B.5).
    pub fn type_text(&mut self, text: &str, cx: &mut Context<'_, Self>) {
        if self.exited.is_some() || text.is_empty() {
            return;
        }
        let data = paste_bytes(text, self.emulator.input_mode());
        self.input(data, cx);
    }
```

In `paste_image` (:530-533), change the callback to `pane.type_text(&typed_paths([saved.path.as_str()]), cx)`.

- [ ] **Step 3: Shell — route a drop to the composer or the terminal**

`Shell` field (next to `composers`) and init:

```rust
    /// The pane under an OS file drag (§20B.5), tracked without GPUI's hover check: gpui-pre
    /// 0.3.7 keeps keyboard modality during OLE drags, so `on_drop` misses drops after typing.
    drop_hover: Option<(Ulid, ExternalPaths)>,
```

```rust
            drop_hover: None,
```

Method (after `composer_typed`):

```rust
    /// Files dropped from the OS on `session`'s pane (§20B.5): into its composer draft while
    /// the composer is shown, else typed into the terminal. No Enter either way.
    fn drop_paths(
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
        if text.is_empty() {
            return;
        }
        if let Some(tabs) = self.active_tabs_mut() {
            tabs.focus(session);
        }
        if self.composers.shown(session).is_some() {
            self.composer_typed(session, &text, window, cx);
        } else if let Some(pane) = self.panes.get(&session).cloned() {
            pane.update(cx, |pane, cx| pane.type_text(&text, cx));
            self.focus_pane(window, cx);
        }
        cx.notify();
    }
```

- [ ] **Step 4: pane_frame — the handler**

On the outer div of `pane_frame` (main.rs:13341), after the `.on_mouse_down(…)` block:

```rust
            .on_drag_move::<ExternalPaths>(cx.listener(
                move |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    let inside = event.bounds.contains(&event.event.position);
                    match &shell.drop_hover {
                        _ if inside => {
                            shell.drop_hover = Some((session, event.drag(cx).clone()));
                        }
                        Some((hovered, _)) if *hovered == session => shell.drop_hover = None,
                        _ => {}
                    }
                },
            ))
            // ponytail: works around gpui-pre 0.3.7 keeping keyboard modality during OLE drags
            // (window.rs:5415 skips FileDrop), which makes `on_drop` miss drops after typing.
            // Use plain `.on_drop` when upstream treats FileDrop as mouse input.
            .capture_any_mouse_up(cx.listener(move |shell, _: &MouseUpEvent, window, cx| {
                let hovered = shell.drop_hover.take_if(|(hovered, _)| *hovered == session);
                // A plain click has no active drag: stale paths are never typed.
                if let (Some((_, paths)), true) = (hovered, cx.has_active_drag()) {
                    cx.stop_propagation();
                    shell.drop_paths(session, &paths, window, cx);
                }
            }))
```

Add `DragMoveEvent`, `ExternalPaths` and `MouseUpEvent` to the gpui import in main.rs if they are missing. Verify `DragMoveEvent::drag` and `App::has_active_drag` in gpui-pre 0.3.7 (div.rs:79, app.rs:2630) before relying on them; look them up, do not guess.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean.

- [ ] **Step 5: SPEC §20B.5**

After the drop sentence (SPEC.md:1371-1375) add: `The whole pane accepts the drop (header, terminal, composer). While the pane's composer is shown, the paths go into its draft instead of the terminal.`

- [ ] **Step 6: Verify and tick**

Run the full verification. Tick line 19 `[x]` after the Task 10 drop check passes on Windows. Until then leave it `[~]`.
Suggested message: `fix(ui): file drops land anywhere on a pane and go into the open composer`

---

### Task 3: Ctrl+V pastes text, images and copied files in terminal panes (ADR 0026)

Inbox: line 20 `Check if we can Ctrl+V to paste text or image into the terminal chat, if not need to add in`. Spec: §13 (:859-863, table :886), §20B.5 (:1379-1383).

**Current state (investigated):**
- Plain Ctrl+V in a terminal pane is not bound. `key_down` (view.rs:218) → `key_action` → `key_to_bytes` sends `0x16` to the PTY.
- Only Ctrl+Shift+V (`Command::Paste`, keymap.rs:289) pastes (view.rs:352-371):
  - text → `paste_bytes`
  - a png/jpg/gif/webp image → `paste_image` → `attachment.save` → typed path
- Windows bitmaps (PrintScreen, `CF_DIB`) arrive as `ImageFormat::Bmp` (gpui-pre-windows clipboard.rs:247). `attachment_ext(Bmp)` is `None`, so they are dropped silently.
- Explorer-copied files (`ClipboardEntry::ExternalPaths`) are ignored.
- The composer's Ctrl+V (`DialogKey::Paste`, main.rs:5550) pastes text only.
- Agents and ^V on Windows:
  - Claude Code's image paste key is Alt+V on Windows.
  - Codex treats ^V as image-only paste and errors with text on the clipboard (openai/codex#17103).
  - Shells and vim use ^V as quoted insert / block visual.
- Windows Terminal binds Ctrl+V to paste and never forwards ^V.
- Decision (Open question 5): plain Ctrl+V = the same smart paste as Ctrl+Shift+V. It also handles BMP → PNG and copied files. It falls back to `^V` only when the clipboard has nothing usable. This is a second plain-Ctrl exception after ADR 0022 → ADR 0026.

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/input.rs:44-72`, tests after :300
- Modify: `crates/mkrew-ui/src/terminal/view.rs:218-232` (key_down), :352-371 (`Command::Paste`)
- Modify: `crates/mkrew-ui/src/composer.rs:222-232`, tests after :445
- Modify: `crates/mkrew-ui/src/main.rs:5550` (composer image paste)
- Modify: `crates/mkrew-ui/Cargo.toml` (`[dependencies]`)
- Create: `docs/decisions/0026-ctrl-v-pastes.md`
- Modify: `docs/SPEC.md` §13 :859-863, :886; §20B.5 :1381

**Interfaces:**
- Produces: `input::KeyAction::Paste`; `input::is_paste_key(&Keystroke) -> bool`; `composer::attachment(image: gpui::Image) -> Option<(&'static str, Vec<u8>)>`; `TerminalPane::paste(&mut self, cx) -> bool` (false = nothing usable on the clipboard).

- [ ] **Step 1: Failing key tests**

In `terminal/input.rs` tests (use the module's existing `key()` helper and `InputMode::default()` as the neighbouring `key_action` tests do):

```rust
    #[test]
    fn plain_ctrl_v_is_paste_only_while_live() {
        let mode = InputMode::default();
        assert_eq!(key_action(&key("ctrl-v"), false, false, mode), KeyAction::Paste);
        // A selection does not change it (Ctrl+C copies, Ctrl+V pastes).
        assert_eq!(key_action(&key("ctrl-v"), true, false, mode), KeyAction::Paste);
        assert_eq!(key_action(&key("ctrl-v"), false, true, mode), KeyAction::Ignore);
        // Ctrl+Shift+V stays the keymap's; AltGr (ctrl-alt) and Win+V are not paste.
        for other in ["ctrl-shift-v", "ctrl-alt-v", "cmd-v", "v"] {
            assert_ne!(key_action(&key(other), false, false, mode), KeyAction::Paste, "{other}");
        }
    }
```

Run: `cargo test -p mkrew-ui --lib terminal::input::tests::plain_ctrl_v -j 4`
Expected: FAIL to compile (`no variant Paste`).

- [ ] **Step 2: Implement in input.rs**

After `is_copy_key`:

```rust
/// Plain Ctrl+V: the pane pastes the clipboard; ^V goes to the program only when there is
/// nothing to paste (ADR 0026).
pub fn is_paste_key(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    keystroke.key == "v" && m.control && !m.shift && !m.alt && !m.platform
}
```

Add the variant to `KeyAction`:

```rust
    /// Paste the clipboard (plain Ctrl+V in a live pane, ADR 0026).
    Paste,
```

In `key_action`, after the copy check:

```rust
    if !exited && is_paste_key(keystroke) {
        return KeyAction::Paste;
    }
```

Run the test → PASS.

- [ ] **Step 3: Failing BMP test**

`composer.rs` tests:

```rust
    #[test]
    fn a_bmp_clipboard_image_is_stored_as_png() {
        let mut bmp = Vec::new();
        image::RgbImage::new(2, 2)
            .write_to(&mut std::io::Cursor::new(&mut bmp), image::ImageFormat::Bmp)
            .unwrap();
        let (ext, bytes) = attachment(gpui::Image::from_bytes(ImageFormat::Bmp, bmp)).unwrap();
        assert_eq!(ext, "png");
        assert!(bytes.starts_with(b"\x89PNG"));
        let png = gpui::Image::from_bytes(ImageFormat::Png, b"png".to_vec());
        assert_eq!(attachment(png), Some(("png", b"png".to_vec())));
        assert_eq!(attachment(gpui::Image::from_bytes(ImageFormat::Svg, b"<svg/>".to_vec())), None);
    }
```

`crates/mkrew-ui/Cargo.toml` `[dependencies]` (the crate is already in `Cargo.lock` through gpui):

```toml
image = { version = "0.25", default-features = false, features = ["png", "bmp"] }
```

Run: `cargo test -p mkrew-ui --lib composer::tests::a_bmp -j 4` → FAIL (`cannot find function attachment`).

- [ ] **Step 4: Implement `attachment`**

After `attachment_ext` in composer.rs:

```rust
/// A clipboard image as `attachment.save` input (extension, bytes). Windows bitmaps
/// (`CF_DIB`, e.g. PrintScreen) arrive as BMP, which agents do not read: re-encoded as PNG.
pub fn attachment(image: gpui::Image) -> Option<(&'static str, Vec<u8>)> {
    if image.format == ImageFormat::Bmp {
        let decoded =
            image::load_from_memory_with_format(&image.bytes, image::ImageFormat::Bmp).ok()?;
        let mut png = Vec::new();
        decoded
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .ok()?;
        return Some(("png", png));
    }
    attachment_ext(image.format).map(|ext| (ext, image.bytes))
}
```

Check the `gpui::Image` field names (`format`, `bytes`) in gpui-pre 0.3.7 before relying on them. Run the test → PASS.

- [ ] **Step 5: TerminalPane::paste and the Ctrl+V branch**

In view.rs, replace the `Command::Paste` arm body (:352-371) with `self.paste(cx);` and add:

```rust
    /// Pastes the clipboard (§20B.5): text (bracketed when asked), else an image saved through
    /// `attachment.save` and typed as its path, else copied files typed as paths. `false` when
    /// the clipboard holds none of these (Ctrl+V then sends ^V, ADR 0026).
    fn paste(&mut self, cx: &mut Context<'_, Self>) -> bool {
        let Some(item) = cx.read_from_clipboard() else {
            return false;
        };
        if self.exited.is_some() {
            return true;
        }
        if let Some(text) = item.text() {
            let data = paste_bytes(&text, self.emulator.input_mode());
            self.input(data, cx);
            return true;
        }
        for entry in item.into_entries() {
            match entry {
                ClipboardEntry::Image(image) => {
                    if let Some((ext, bytes)) = attachment(image) {
                        self.paste_image(ext, bytes, cx);
                        return true;
                    }
                }
                ClipboardEntry::ExternalPaths(paths) => {
                    let paths: Vec<String> = paths
                        .paths()
                        .iter()
                        .map(|path| path.to_string_lossy().into_owned())
                        .collect();
                    self.type_text(&typed_paths(paths.iter().map(String::as_str)), cx);
                    return true;
                }
                _ => {}
            }
        }
        false
    }
```

Decoding a large BMP takes tens of ms on the UI thread. Move `attachment(image)` into `cx.background_executor().spawn(…)` inside `paste_image` if the manual check shows a visible stall. Do not do it up front.

In `key_down`, add the arm:

```rust
            KeyAction::Paste => {
                cx.stop_propagation();
                if !self.paste(cx) {
                    self.input(vec![0x16], cx);
                }
            }
```

Update the `key_down` doc comment: "Plain Ctrl+V pastes (ADR 0026)". Replace the `attachment_ext` import in view.rs with `attachment`.

- [ ] **Step 6: Composer — image paste**

In `composer_key_down` (main.rs:5550), the `DialogKey::Paste` arm inserts `item.text()` only. When there is no text, take the first `ClipboardEntry::Image` through `composer::attachment`. Send it with the same `Request::AttachmentSave` call `paste_image` uses (`self.connection`). On success, run `this.update(cx, |shell, cx| shell.composers.insert(session, &composer::typed_paths([saved.path.as_str()])))` and notify. A failure only logs, like `paste_image`.

- [ ] **Step 7: ADR 0026 and SPEC**

Create `docs/decisions/0026-ctrl-v-pastes.md`:

```markdown
# 0026 — Plain Ctrl+V pastes in terminal panes

- Date: 2026-10-08
- Status: accepted
- Milestone: inbox 2026-10-08

## Context

The user expects Ctrl+V to paste text and images into an agent pane, as in Windows Terminal.
Mkrew only pasted on Ctrl+Shift+V; plain Ctrl+V sent ^V, which Codex reads as "paste an image"
(and errors on text) and Claude Code ignores on Windows (its image key is Alt+V). Windows
bitmaps (PrintScreen) were dropped because GPUI reads them as BMP.

## Options

1. Plain Ctrl+V = the Ctrl+Shift+V paste; ^V only when the clipboard has nothing usable.
2. A per-profile key to forward for images (`image_paste_key`), so agents read the clipboard.
3. Keep Ctrl+Shift+V only; fix BMP.

## Decision

Option 1, decided in `terminal::input::key_action` like Ctrl+C (ADR 0022). Text is pasted
(bracketed when asked); an image (BMP re-encoded as PNG) is saved through `attachment.save` and
its path typed; Explorer-copied files type their paths. It works for every agent and shell
without per-agent code (§2.1). Cost: a program that wants a literal ^V gets it only with an
empty clipboard — the same trade Windows Terminal makes. Not rebindable (outside the keymap,
like Ctrl+C).
```

SPEC §13 :859-863: after the Ctrl+C exception add `Plain Ctrl+V pastes in a terminal pane; ^V reaches the program only when the clipboard holds no text, image or files (ADR 0026).` Table :886: `Ctrl+Shift+V` → `Ctrl+V or Ctrl+Shift+V`. §20B.5 :1381: `Ctrl+Shift+V` → `Ctrl+V / Ctrl+Shift+V`, and add `(a BMP image is re-encoded as PNG; copied files type their paths)`.

- [ ] **Step 8: Verify and tick**

Run the full verification. Tick line 20 `[x]`.
Suggested message: `feat(ui): Ctrl+V pastes text, images and files in terminal panes (ADR 0026)`

---

### Task 4: Agent icons the user actually sees — bundled fallback and a chip

Inbox: line 21 `For the agent in the workspace do we have icon for each agent, if not yet then need to add in for better view`. Spec: §6 (:347-350, :424-428), §11 (:725-741). ADR 0025.

**Current state (investigated):**
- **Root cause:** commit 2e807bf added the `icon` key, but `seed_profiles` (daemon lib.rs:91-101) never overwrites existing files. The user's `%APPDATA%\mkrew\agents\*.toml` (all six) have no `icon` line (checked 2026-10-08).
  - `Server::agent_profiles` (server.rs:1308-1331) therefore sends `icon: None`.
  - `sidebar::agent_badge` falls back to letters: claude `C`, codex `C`, copilot `C`, omp `O`.
- The badge is plain muted text without a background (main.rs `session_row` badge child, `pane_header` badge child). It reads as part of the name.
- It is shown only in the session row and the pane header. Not in:
  - `inbox_row`
  - the board card agent line (`card.agent`)
  - the agent menus (`agent_rows`, `AgentChoice` drops `icon`)
- Appending `icon = …` to the user's files is unsafe: they end in a `[state]` table, and the key would land inside it and become a parse error (`deny_unknown_fields`).
- There is no SVG pipeline; brand logos were already rejected in ADR 0025.

**Files:**
- Modify: `crates/mkrew-daemon/src/lib.rs:80-88` (+ `sample_icon`, test)
- Modify: `crates/mkrew-daemon/src/server.rs:1318`
- Modify: `crates/mkrew-ui/src/theme.rs` (chip size)
- Modify: `crates/mkrew-ui/src/main.rs` (`agent_chip`; `session_row`, `pane_header`, `inbox_row` + caller, board card agent line, `agent_rows`)
- Modify: `crates/mkrew-ui/src/workspace_menu.rs:24-62` (`AgentChoice.icon`)
- Modify: `docs/decisions/0025-agent-badges.md` (amendment), `docs/SPEC.md` §6 :424-428, §11 :731

**Interfaces:**
- Produces: `mkrew_daemon::sample_icon(id: &str) -> Option<String>` (crate-private is fine); `fn agent_chip(badge: String, theme: &Theme) -> gpui::Div` in main.rs; `theme::AGENT_CHIP_SIZE: f32 = 18.0`; `AgentChoice.icon: Option<String>`.
- Consumes: `Shell::agent_badge(&self, agent: &str) -> String` (exists).

- [ ] **Step 1: Failing daemon test**

In `crates/mkrew-daemon/src/lib.rs` tests (create `#[cfg(test)] mod tests` at the end if none exists):

```rust
    #[test]
    fn sample_icon_comes_from_the_shipped_profile_of_the_same_id() {
        assert_eq!(sample_icon("claude").as_deref(), Some("✻"));
        assert_eq!(sample_icon("omp").as_deref(), Some("π"));
        // A user's own profile id has no shipped sample: the UI shows its first letter.
        assert_eq!(sample_icon("mybot"), None);
    }
```

Run: `cargo test -p mkrew-daemon --lib sample_icon -j 4` → FAIL (`cannot find function sample_icon`).

- [ ] **Step 2: Implement the fallback**

After `seed_profiles` in lib.rs:

```rust
/// The shipped sample's `icon` for profile `id` (ADR 0025): profile files copied before the key
/// existed are never overwritten, so they borrow it. Data from the TOML samples, not per-agent
/// code (§2.1).
pub(crate) fn sample_icon(id: &str) -> Option<String> {
    let (file, src) = SAMPLE_PROFILES
        .iter()
        .find(|(file, _)| file.strip_suffix(".toml") == Some(id))?;
    mkrew_core::parse_profile(id, std::path::Path::new(file), src)
        .ok()?
        .icon
}
```

`server.rs:1318`: `icon: p.icon.or_else(|| crate::sample_icon(&p.id)),`

Run the test → PASS. A user `icon = "Z"` still wins because it is `Some`.

- [ ] **Step 3: The chip**

`theme.rs`, next to `WORKSPACE_DOT_SIZE`:

```rust
/// Side of an agent badge chip (ADR 0025), like the design's workspace avatar.
pub const AGENT_CHIP_SIZE: f32 = 18.0;
```

main.rs, next to `status_mark`:

```rust
/// An agent badge (ADR 0025) as a small square chip, so glyphs and letters line up and never
/// read as part of the name.
fn agent_chip(badge: String, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .size(px(theme::AGENT_CHIP_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .text_color(rgb(theme.text))
        .child(badge)
}
```

No background: a selected or hovered row is already `surface_raised`, and a border-only chip reads on both.

- [ ] **Step 4: Use it**

- `session_row` and `pane_header`: replace the muted badge `div()` children with `agent_chip(badge, theme)`.
- `inbox_row`:
  - Add a `badge: String` parameter after the entry.
  - Insert `agent_chip(badge, theme)` after the status mark.
  - Caller (`render_inbox`, where `inbox_row(` is called): pass `self.agent_badge(&entry.agent)`.
- Board card agent line (`task_card`, the `card.agent` child):
  - Prefix it with `agent_chip(…)` when `card.agent_profile()` is `Some(id)`.
  - The badge string is computed in the caller with `self.agent_badge(id)` and passed in the card's context struct as `agent_badge: Option<String>`.
- `workspace_menu.rs`:
  - `AgentChoice { id, label, selected }` gains `icon: Option<String>`.
  - `agent_choices` copies `profile.icon.clone()`.
  - `agent_rows` (main.rs) renders `agent_chip(sidebar::agent_badge(choice.icon.as_deref(), &choice.id), theme)` after the ✓ column.
  - Update the test helpers that build `AgentChoice`/`AgentProfileInfo` literally.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` → clean. `cargo test -p mkrew-ui --lib workspace_menu sidebar -j 4` → PASS.

- [ ] **Step 5: Docs**

ADR 0025: append

```markdown
## Amendment 2026-10-08

Users' profile files are never overwritten, so files copied before `icon` existed showed
letters (claude, codex and copilot all `C`). A profile without `icon` now borrows the shipped
sample's `icon` for the same id (`sample_icon`, daemon); other ids show the first letter. The
badge is a bordered square chip and also shows in the Inbox row, board card and agent menus.
```

SPEC §6 :424-428: replace "a missing or blank one shows the id's first letter" with `a missing one uses the shipped sample's icon for the same id, else (or when blank) the id's first letter`. §11 :731: append `The agent chip also shows in the Inbox row, on board cards and in agent menus.`

- [ ] **Step 6: Verify and tick**

Run the full verification. The user must restart the daemon (new binary) and reconnect the UI. Task 10 lists this. Tick line 21 `[x]`.
Suggested message: `fix: agent badges fall back to the shipped icon and render as chips (ADR 0025)`

---

### Task 5: Redesign 1 — Theme global, System/Light/Dark mode, new tokens, ToggleTheme (ADR 0027)

Inbox: line 27 (redesign), section "Theme (dark + light mode)". Spec: §13, §14 (:916-988), §20.0.

**Current state (investigated):**
- `Theme` (theme.rs:329-384) holds `u32` tokens and already has `dark()` / `light()`.
  - It is not a Global and has no mode.
  - `Shell::render` builds `Theme::for_appearance(dark).with_fonts(&fonts)` every frame (main.rs:16289-16295) and passes `&Theme` to every helper.
  - `TerminalPane` builds its own (view.rs:120, 588-601) and clears its row cache when `Theme != old`.
- View code has **no** hex literals. The only non-token color is `theme::DIALOG_SCRIM` (const, theme.rs:245).
- `settings.toml` is user-edited; the app never writes it except the template. `Settings` (mkrew-core settings.rs:14-25, `deny_unknown_fields`) is shared with the daemon. `[ui]` uses `FontSettings`, which `[terminal]` shares, so `theme` needs its own `UiSettings`.
- `Keymap::label(command)` (keymap.rs:553) formats real bindings ("Ctrl+Shift+A").
  - Every `Command` needs a default key: the tests `every_default_parses_and_is_unique` and `every_binding_becomes_a_gpui_key_binding` require one.
  - `ctrl-shift-j` is unused (checked).
- The brief says "don't change persistence / action definitions" but also asks to persist the mode and add `ToggleTheme`. This plan treats those two as named exceptions (Open question 9).
- Decision: keep `u32` tokens and the `rgb(theme.x)` call style (a Hsla rewrite would touch every view for no user-visible gain). The design's tokens map onto existing fields or become new fields. Colors come from `design/theme.rs`, with the contrast fixes below.

**Token mapping (design → `Theme`):**

| Design | Theme field | Dark | Light |
| --- | --- | --- | --- |
| `sidebar` | `sidebar` (new) | 0x131418 | 0xF4F5F7 |
| `bg` | `bg` | 0x17181C | 0xFFFFFF |
| `surface` | `surface` | 0x1F2127 | 0xF6F7F9 |
| `card` | `card` (new) | 0x1A1B20 | 0xFFFFFF |
| `hover` | `hover` (new) | 0x22242B | 0xEEF0F3 |
| `selected` | `selected` (new) | 0x262938 | 0xE9EAFF |
| `segment_on` | `surface_raised` | 0x2C2E37 | 0xFFFFFF |
| `border` | `border` | 0x2A2C33 | 0xE3E5EA |
| `divider` | `divider` (new) | 0x24262D | 0xECEEF1 |
| `text` | `text` | 0xE6E7EB | 0x1B1D22 |
| `text_dim` | `text_dim` (new) | 0xB4B7C0 | 0x474B55 |
| `muted` | `text_muted` | 0x9A9CA6 (kept: 0x8B8F9A fails 4.5:1 on `selected`) | 0x5F6473 (0x6B7080 fails on `hover`) |
| `subtle` | `subtle` (new, graphics only: chevrons, idle ring, guide line — never text) | 0x5D616C | 0xA0A4AF |
| `accent` | `accent` | 0x7C83FF | 0x5B63F0 |
| `accent_text` | `accent_text` (new) | 0x9A9FFF | 0x4A52D9 |
| `accent_bg` | `accent_bg` (new) | 0x25264A | 0xECEEFF |
| `on_accent` | `on_accent` (new) | 0xFFFFFF | 0xFFFFFF |
| `on_accent_divider` | `on_accent_divider` (new, RGBA → `rgba()`) | 0xFFFFFF40 | 0xFFFFFF59 |
| `warning` / `warning_bg` | `status_needs_input` / `warning_bg` (new) | 0xE3A63E / 0x3A3020 | 0xB7791F / 0xFDF3E1 |
| `success` / `success_bg` | `status_done` / `success_bg` (new) | 0x4CB782 / 0x1D3A31 | 0x2F855A / 0xE1F5EC |
| `avatar_tint_alpha` | `tint_alpha` (new, u8) | 0x2E | 0x24 |
| — | `scrim` (new, RGBA; replaces `DIALOG_SCRIM`) | value of `DIALOG_SCRIM` | same |

`status_working` follows `accent`. Derived tokens (`pane_focus_border`, `terminal_selection_bg`, match bgs) are re-derived as SPEC §14 says (accent mixed into `bg`). The light `bg` becomes pure white, so the light ANSI "white" and syntax colors are rechecked by the existing contrast tests.

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (struct :329-384, `for_appearance` :387, palettes :399-521, `DIALOG_SCRIM` :245, `SIDEBAR_WIDTH` :101, tests :578-760)
- Modify: `crates/mkrew-core/src/settings.rs` (`UiSettings`, `ThemeMode`, `set_ui_theme`, template, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (window open :149, `Shell::new` :383, settings poll :404-420, `Shell::render` :16289-16295, `Tooltip`/`CardPreview` :16229-16285, `run()` dispatch :6058-6190, `DIALOG_SCRIM` use)
- Modify: `crates/mkrew-ui/src/terminal/view.rs:120, 588-601`
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum, `all`, `name`, `default_keys`), `crates/mkrew-ui/src/palette.rs` (`title`, `available`)
- Create: `docs/decisions/0027-calm-redesign.md`
- Modify: `docs/SPEC.md` §13 table, §14 tokens :936-954 and :981, §20.0

**Interfaces:**
- Produces:
  - `mkrew_core::settings::ThemeMode { System, Light, Dark }` (serde lowercase, default `System`)
  - `Settings.ui: UiSettings { font_family: String, font_size: FontSize, theme: ThemeMode }`
  - `settings::set_ui_theme(src: &str, mode: ThemeMode) -> String`
  - `impl gpui::Global for Theme`; `Theme.mode: ThemeMode`
  - `theme::ActiveTheme` trait with `fn theme(&self) -> &Theme` for `App`
  - `theme::refresh(mode: ThemeMode, window: &Window, cx: &mut App)` (sets the global from mode + `window.appearance()` + the `Fonts` global)
  - `theme::next_mode(ThemeMode) -> ThemeMode`; `theme::mode_glyph(ThemeMode) -> &'static str`
  - `Theme::tint(&self, color: u32) -> gpui::Rgba`
  - new fields listed above
  - `theme::rems_px(px: f32) -> Rems` (px at the default 13 px body → rems)
  - `Command::ToggleTheme`

- [ ] **Step 1: Failing settings tests**

`mkrew-core/src/settings.rs` tests:

```rust
    #[test]
    fn ui_theme_parses_and_defaults_to_system() {
        let ui = |src: &str| parse(Path::new("s.toml"), src).unwrap().ui;
        assert_eq!(ui("").theme, ThemeMode::System);
        assert_eq!(ui("[ui]\ntheme = \"dark\"").theme, ThemeMode::Dark);
        assert_eq!(ui("[ui]\ntheme = \"light\"\nfont_size = 14").font_size, FontSize(14));
        assert!(parse_err("[ui]\ntheme = \"blue\"").contains("unknown variant `blue`"));
        // [terminal] has no theme.
        assert!(parse_err("[terminal]\ntheme = \"dark\"").contains("unknown field `theme`"));
    }

    #[test]
    fn set_ui_theme_keeps_comments_and_other_keys() {
        let src = "# mine\n[ui]\n# theme = \"system\"\nfont_size = 15\ntheme = \"light\"\n\n[terminal]\nfont_size = 12\n";
        let out = set_ui_theme(src, ThemeMode::Dark);
        assert_eq!(out, src.replace("theme = \"light\"", "theme = \"dark\""));
        assert_eq!(parse(Path::new("s.toml"), &out).unwrap().ui.theme, ThemeMode::Dark);
    }

    #[test]
    fn set_ui_theme_inserts_ui_section() {
        let with_ui = set_ui_theme("[ui]\nfont_size = 15\n", ThemeMode::Light);
        assert_eq!(with_ui, "[ui]\ntheme = \"light\"\nfont_size = 15\n");
        let without = set_ui_theme("[zed]\nfollow_debounce_ms = 1\n", ThemeMode::Dark);
        assert_eq!(without, "[zed]\nfollow_debounce_ms = 1\n\n[ui]\ntheme = \"dark\"\n");
        assert_eq!(set_ui_theme("", ThemeMode::System), "[ui]\ntheme = \"system\"\n");
    }
```

Run: `cargo test -p mkrew-core --lib settings::tests::ui_theme settings::tests::set_ui_theme -j 4` → FAIL to compile.

- [ ] **Step 2: Implement settings**

```rust
/// `[ui] theme` (SPEC §14, ADR 0027): `system` follows the OS live.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub fn name(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        }
    }
}

/// `[ui]`: the app font and theme (SPEC §14, §20.0).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSettings {
    pub font_family: String,
    pub font_size: FontSize,
    pub theme: ThemeMode,
}

impl Default for UiSettings {
    fn default() -> Self {
        let font = FontSettings::default();
        Self { font_family: font.font_family, font_size: font.font_size, theme: ThemeMode::System }
    }
}

/// `src` with `[ui] theme = "<mode>"`: replaces an uncommented `theme =` line inside `[ui]`,
/// else inserts one right after the `[ui]` header, else appends a `[ui]` section. Everything
/// else (comments, order, other keys) is kept byte for byte.
pub fn set_ui_theme(src: &str, mode: ThemeMode) -> String {
    let line = format!("theme = \"{}\"", mode.name());
    let mut lines: Vec<&str> = src.lines().collect();
    let header = lines.iter().position(|l| l.trim() == "[ui]");
    let Some(header) = header else {
        let sep = match src { "" => "", s if s.ends_with("\n\n") => "", s if s.ends_with('\n') => "\n", _ => "\n\n" };
        return format!("{src}{sep}[ui]\n{line}\n");
    };
    let end = lines[header + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| header + 1 + i);
    let is_theme = |l: &&str| {
        let l = l.trim_start();
        l.strip_prefix("theme").is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    match lines[header + 1..end].iter().position(is_theme) {
        Some(i) => lines[header + 1 + i] = &line,
        None => lines.insert(header + 1, &line),
    }
    let mut out = lines.join("\n");
    if src.ends_with('\n') || src.is_empty() {
        out.push('\n');
    }
    out
}
```

Change `Settings.ui` to `UiSettings`. The `Fonts::from_settings` field names are unchanged. In `template()`, under `[ui]`, add `# theme = "system"      # system | light | dark`. Run the tests → PASS. Run `cargo test -p mkrew-core -j 4` → PASS (the template test parses it).

- [ ] **Step 3: Failing theme tests**

In `theme.rs` tests:
- Extend `body_text_contrast_is_at_least_4_5` so `text` and `text_muted` are checked on `sidebar`, `hover`, `selected`, `surface_raised` and `card`, in both palettes.
- Add:

```rust
    #[test]
    fn mode_cycles_system_light_dark() {
        assert_eq!(next_mode(ThemeMode::System), ThemeMode::Light);
        assert_eq!(next_mode(ThemeMode::Light), ThemeMode::Dark);
        assert_eq!(next_mode(ThemeMode::Dark), ThemeMode::System);
    }

    #[test]
    fn mode_resolves_against_the_os() {
        assert!(Theme::resolve(ThemeMode::System, true).dark);
        assert!(!Theme::resolve(ThemeMode::System, false).dark);
        assert!(!Theme::resolve(ThemeMode::Light, true).dark);
        assert!(Theme::resolve(ThemeMode::Dark, false).dark);
        assert_eq!(Theme::resolve(ThemeMode::Dark, false).mode, ThemeMode::Dark);
    }

    #[test]
    fn layout_heights_scale_with_the_ui_font() {
        // 44 px at the default 13 px body; at 20 px the toolbar is 44 × 20 / 13.
        assert_eq!(rems_px(44.0), rems(44.0 / 13.0));
        assert_eq!(rems_px(TOOLBAR_HEIGHT).0 * 20.0, 44.0 * 20.0 / 13.0);
    }

    #[test]
    fn on_accent_text_is_readable_on_the_accent() {
        for t in [Theme::dark(), Theme::light()] {
            assert!(contrast(t.on_accent, t.accent) >= 3.0, "{:06X}", t.accent);
        }
    }
```

Run: `cargo test -p mkrew-ui --lib theme -j 4` → FAIL to compile.

- [ ] **Step 4: Implement theme.rs**

- Add the new fields to `Theme` (with doc comments: what each surface is for), and set them in `dark()`, `light()` and `shared()` from the mapping table. Add `pub mode: ThemeMode` (re-export `mkrew_core::settings::ThemeMode` as `theme::ThemeMode`).
- Replace `for_appearance(dark: bool)` with `resolve(mode, os_dark: bool) -> Self`. `System` → OS, else forced; the result carries `mode`. Update the callers the compiler lists.
- `impl gpui::Global for Theme {}`.
- Delete `DIALOG_SCRIM`; use `rgba(theme.scrim)` at its use site.

```rust
/// Views read the theme with `cx.theme()` (ADR 0027).
pub trait ActiveTheme {
    fn theme(&self) -> &Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Theme {
        self.global::<Theme>()
    }
}

/// Rebuilds the theme global from `mode`, the window's OS appearance and the `Fonts` global.
/// Called at startup, on OS appearance changes, on a settings reload and by the toggle.
pub fn refresh(mode: ThemeMode, window: &Window, cx: &mut App) {
    let os_dark = matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    );
    let fonts = cx.global::<Fonts>().clone();
    cx.set_global(Theme::resolve(mode, os_dark).with_fonts(&fonts));
}

/// System → Light → Dark → System (the toolbar toggle, `ToggleTheme`).
pub fn next_mode(mode: ThemeMode) -> ThemeMode {
    match mode {
        ThemeMode::System => ThemeMode::Light,
        ThemeMode::Light => ThemeMode::Dark,
        ThemeMode::Dark => ThemeMode::System,
    }
}

/// The toggle's glyph: monitor / sun / moon.
pub fn mode_glyph(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::System => "🖵",
        ThemeMode::Light => "☀",
        ThemeMode::Dark => "☾",
    }
}

/// A design size in px at the default 13 px body, as rems: it grows with `[ui] font_size`
/// (ADR 0024) instead of clipping text.
pub fn rems_px(px: f32) -> Rems {
    rems(px / 13.0)
}

/// Layout heights from the redesign (ADR 0027), in px at the default font size.
pub const TOOLBAR_HEIGHT: f32 = 44.0;
pub const TAB_STRIP_HEIGHT: f32 = 38.0;
pub const WORKSPACE_ROW_HEIGHT: f32 = 30.0;
pub const SESSION_ROW_HEIGHT: f32 = 28.0;
pub const WORKSPACE_AVATAR_SIZE: f32 = 18.0;
pub const EMPTY_STATE_MAX_WIDTH: f32 = 320.0;
pub const EMPTY_STATE_TILE: f32 = 44.0;
```

`Theme::tint`:

```rust
    /// `color` at the palette's avatar alpha (workspace avatars, ADR 0027).
    pub fn tint(&self, color: u32) -> gpui::Rgba {
        gpui::rgba((color << 8) | u32::from(self.tint_alpha))
    }
```

Set `SIDEBAR_WIDTH` to 280.0 (brief). Check that `🖵` renders in Segoe UI. If it does not, use `◐` for System and record that in HUMAN_CHECKS.

Run: `cargo test -p mkrew-ui --lib theme -j 4`. Adjust light/dark values until the contrast tests pass, and list every adjusted value in ADR 0027 (the brief asks to "tell me what you changed").

- [ ] **Step 5: Wire the global**

- Window open (main.rs:149, inside the `open_window` closure, before `Shell::new`): `theme::refresh(settings.ui.theme, window, cx);` using the settings already loaded at :131-135.
- `Shell::new` appearance observer (:383): `theme::refresh(cx.theme().mode, window, cx); cx.notify();`.
- Settings poll (:404-420): when the reload yields new fonts **or** a different `ui.theme`, set `Fonts`, then `theme::refresh(new_mode, window, cx)`. Extend `reload_fonts` to return the mode alongside the fonts, or call `settings::load` once and derive both. Keep the "last good settings on error" rule.
- `Shell::render` (:16289-16295): `let theme = cx.theme().clone();` (keep `window.set_rem_size`).
- `Tooltip` / `CardPreview` (:16229-16285): read `cx.theme().clone()` in their render instead of a stored copy.
- `terminal/view.rs:588-601`: `theme_for` → `cx.theme().clone()`. Keep the `Theme != old` row-cache check. Its own appearance observer (:120) only needs `cx.notify()`.

- [ ] **Step 6: ToggleTheme**

`keymap.rs`:
- `Command::ToggleTheme` in the enum and `all()`
- `name()` → `"toggle_theme"`
- `default_keys()` → `"ctrl-shift-j"`
- `context()` → `None`

`palette.rs`:
- `title` → `"Global: Toggle theme (System → Light → Dark)"`
- `available` → `true`

`run()` in main.rs:

```rust
            Command::ToggleTheme => self.toggle_theme(window, cx),
```

```rust
    /// Cycles System → Light → Dark (ADR 0027) and saves it as `[ui] theme`. A settings file
    /// that does not parse is not written; the mode still changes for this session.
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let mode = theme::next_mode(cx.theme().mode);
        theme::refresh(mode, window, cx);
        let path = self.paths.settings_file();
        let src = match std::fs::read_to_string(&path) {
            Ok(src) => src,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => settings::template().to_owned(),
            Err(error) => return self.settings_error(error.to_string(), cx),
        };
        if let Err(error) = settings::parse(&path, &src) {
            return self.settings_error(error.to_string(), cx);
        }
        if let Err(error) = std::fs::write(&path, settings::set_ui_theme(&src, mode)) {
            self.settings_error(format!("{}: {error}", path.display()), cx);
        }
        cx.notify();
    }
```

Use the existing settings-error path the font reload uses (palette notice). If it is not a method yet, reuse the code at :404-420 instead of `settings_error`; do not add a second error UI. Use the existing `Paths` handle the "Open settings" action uses (main.rs:3845-3856).

Run: `cargo test -p mkrew-ui --lib keymap palette -j 4` → PASS (every default parses and is unique).

- [ ] **Step 7: ADR 0027 (started here, completed in Task 10) and SPEC**

Create `docs/decisions/0027-calm-redesign.md` with Context (the brief: modern, simple, calm; more content room), Options (keep the rail of ADR 0023 / top toolbar per the brief), Decision (the toolbar; Theme global + System/Light/Dark `[ui] theme` written by the toggle; `ToggleTheme` default `ctrl-shift-j` because every command needs a default; u32 tokens kept; token values from `design/theme.rs` with the contrast changes listed). Add `Supersedes: 0023` to its header. In `0023-view-rail.md`, set `Status: superseded by 0027`.

SPEC:
- §14 token table :936-954: add the new tokens (both columns) and update the changed values.
- §14 :981: `Theme follows OS light/dark.` → `Theme: System (follows the OS live), Light or Dark — [ui] theme, cycled by the toolbar toggle (ADR 0027).`
- §13 table: `Global | Toggle theme | Ctrl+Shift+J`.
- §20.0 key list: `[ui] theme = "system" | "light" | "dark"`.

- [ ] **Step 8: Verify**

Run the full verification. The app must look the same as before, apart from the new token values and the 280 px sidebar. Line 23 stays `[~]`.
Suggested message: `feat(ui): theme global with System/Light/Dark and ToggleTheme (ADR 0027)`

---

### Task 6: Redesign 2 — shared components

Inbox: line 27, "Constraints: factor out reusable pieces". Spec: §14.

**Current state (investigated):** helpers live in main.rs:
- `button_frame` :13774, `button(id,label,theme,primary)` :13789, `quiet_button` :13807, `disabled_button` :13820, `danger_button` :13845
- `new_pane_button` :13738, `row_action` :14458, `menu_item` :14729, `message()` :13689 (~20 callers: board, docs, files, git, main)

Each sets `.hover` once already. The primary button uses `theme.surface` as its text color, which only works by accident.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs:13689-13860`

**Interfaces:**
- Consumes: Task 5 tokens (`hover`, `on_accent`, `card`, `accent_bg`), `theme::rems_px`, `EMPTY_STATE_*`.
- Produces:
  - `button(id, label, theme, primary)` (primary = accent bg + `on_accent` text)
  - `quiet_button(id, label, theme, hover)` (= ghost: `text_muted`, `hover` bg)
  - `secondary_button(id, label, theme) -> Stateful<Div>` (surface bg + border)
  - `kbd(text: String, theme: &Theme, on_accent: bool) -> Div`
  - `icon_button(id, glyph, theme) -> Stateful<Div>`
  - `empty_state(theme, glyph: Option<&'static str>, title: Option<&'static str>, sentence, detail) -> Div`; `message()` becomes a thin call to it, so its ~20 callers get the new look unchanged

- [ ] **Step 1: Restyle and add**

```rust
/// A small text button: primary (accent, `on_accent` text) or ghost (muted, hover bg).
fn button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
    primary: bool,
) -> gpui::Stateful<gpui::Div> {
    if primary {
        button_frame(id, label)
            .bg(rgb(theme.accent))
            .text_color(rgb(theme.on_accent))
            .font_weight(FontWeight(weight::MEDIUM))
            .hover(|style| style.opacity(opacity::HOVER))
            .active(|style| style.opacity(opacity::ACTIVE))
    } else {
        quiet_button(id, label, theme, theme.hover)
    }
}

/// A secondary button: surface background with a faint border (the empty state's New shell).
fn secondary_button(
    id: &'static str,
    label: impl Into<SharedString>,
    theme: &Theme,
) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    button_frame(id, label)
        .bg(rgb(theme.surface))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .text_color(rgb(theme.text))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
}

/// A keyboard shortcut chip (`Keymap::label` text). On an accent button it is drawn in
/// `on_accent` at reduced opacity.
fn kbd(text: String, theme: &Theme, on_accent: bool) -> gpui::Div {
    let base = div()
        .px(px(space::XS))
        .rounded(px(radius::SMALL))
        .text_size(font_size::META);
    if on_accent {
        base.text_color(rgb(theme.on_accent)).opacity(opacity::SECONDARY)
    } else {
        base.bg(rgb(theme.surface_raised))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .text_color(rgb(theme.text_muted))
    }
}

/// A square ghost button holding one glyph (theme toggle, sidebar +, ⋯).
fn icon_button(id: impl Into<ElementId>, glyph: &'static str, theme: &Theme) -> gpui::Stateful<gpui::Div> {
    let hover = theme.hover;
    div()
        .id(id)
        .size(theme::rems_px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .cursor_pointer()
        .text_color(rgb(theme.text_muted))
        .hover(move |style| style.bg(rgb(hover)))
        .active(|style| style.opacity(opacity::ACTIVE))
        .child(glyph)
}
```

Change `disabled_button`'s primary text from `theme.surface` to `theme.on_accent`. `empty_state`: take the body of `message()`, constrain it with `.max_w(px(theme::EMPTY_STATE_MAX_WIDTH))`, and put an optional tile in front:

```rust
        .when_some(glyph, |el, glyph| {
            el.child(
                div()
                    .size(theme::rems_px(theme::EMPTY_STATE_TILE))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(radius::MEDIUM))
                    .bg(rgb(theme.surface))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .text_color(rgb(theme.text_muted))
                    .child(glyph),
            )
        })
```

The title is `font_size::CARD_TITLE` (15 px) + `weight::MEDIUM`; the sentence is `text_muted`. `message(theme, title, sentence, detail)` becomes `empty_state(theme, None, title, sentence, detail)`. Read `message()` (:13689-13723) first and keep its detail and centering behavior.

Never chain `.hover` on the result of these helpers.

- [ ] **Step 2: Verify**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` → clean (unused helpers get `#[allow(dead_code)]` only until Task 8 uses them; remove the allow there). These are visual, so there are no unit tests (wiring-only).
Suggested message: `refactor(ui): shared ghost/primary/secondary buttons, kbd chip and empty state`

---

### Task 7: Redesign 3 — sidebar

Inbox: line 27, section "Sidebar". Spec: §10 (:681-682 workspace color), §11 (:741), §14 principle 2 (:926-929).

**Current state (investigated):**
- `render_sidebar` (main.rs:7028-7172): header "Workspaces" + `button("add-workspace","+")`, errors, empty state, flat rows (workspace_row / session_row / peek), inbox (:7143-7150). Width `SIDEBAR_WIDTH`, bg `surface`.
- `workspace_row` (:14862-14992): chevron, 8 px color dot, name, ▶ run indicator, token count, working count, `?`, ⋯ only on the active row. Click activates, or toggles when already active; right-click opens the menu.
- `session_row` (:14994-15093): indent XL, status mark, agent chip (Task 4), "agent · title" (accent + medium when selected), status text with tooltip, ✕ on group_hover, hover peek.
- `render_inbox` (:7175-7229): header "Inbox (n)", empty text, list with max height 50 %.
- Reference: `design/sidebar.rs` (header :120-137, workspace row :139-245, sessions under a guide :226-296, Inbox card :298-350).

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (`render_sidebar`, `workspace_row`, `session_row`, `render_inbox`)
- Modify: `crates/mkrew-ui/src/theme.rs` (delete `WORKSPACE_DOT_SIZE` when unused)
- Modify: `docs/SPEC.md` §10 :681-682, §11 :725-744, §14 :926-929; `docs/decisions/0020-workspace-colors.md` (one-line amendment: dot → tinted letter avatar)

**Interfaces:**
- Consumes: `status_mark` (Task 1), `agent_chip` (Task 4), `icon_button` / `empty_state` (Task 6), tokens and sizes (Task 5), `status::time`, `sidebar::status_display(state).label`.

- [ ] **Step 1: Header and container**

- `render_sidebar`: bg `theme.sidebar`, `border_r` 1 px `theme.divider`.
- Header: `h(rems_px(TOOLBAR_HEIGHT))`, px `space::M`, "Workspaces" at `font_size::META`, `weight::MEDIUM`, `text_muted`; on the right, `icon_button("add-workspace", "+", theme)` with the same click handler and tooltip as today.
- The empty sidebar ("No workspaces yet.") uses `empty_state`.

- [ ] **Step 2: Workspace row**

Restyle `workspace_row`:
- `h(rems_px(WORKSPACE_ROW_HEIGHT))`, `.group(format!("ws-{id}"))`.
- Chevron in `theme.subtle`.
- Avatar instead of the dot:

```rust
fn workspace_avatar(name: &str, color: Option<u32>, theme: &Theme) -> gpui::Div {
    let color = color.unwrap_or(theme.text_muted);
    div()
        .flex_none()
        .size(theme::rems_px(theme::WORKSPACE_AVATAR_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .bg(theme.tint(color))
        .text_color(rgb(color))
        .text_size(font_size::META)
        .font_weight(FontWeight(weight::SEMIBOLD))
        .child(name.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default())
}
```

- The name is truncated (`.truncate()` / `text_ellipsis`).
- Working count as an accent pill: `bg(theme.accent_bg)`, `text_color(theme.accent_text)`, `rounded_full`, children `status_mark(Working, now, theme)` + count.
- Keep ▶ run, tokens and `?`.
- The ⋯ menu button shows on every row on group hover: `.opacity(opacity::HIDDEN).group_hover(group, |s| s.opacity(opacity::FULL))`. It stays visible on the active row. No `.hover` on `row_action`'s result.

Click, right-click and tooltip handlers are unchanged.

- [ ] **Step 3: Sessions under a guide line**

In `render_sidebar`, wrap the session rows of an expanded workspace in:

```rust
div()
    .ml(px(space::L))
    .pl(px(space::XS))
    .border_l(px(theme::BORDER_WIDTH))
    .border_color(rgb(theme.subtle))
    .flex()
    .flex_col()
```

The row list (:7095-7142) is flat today. Group consecutive session rows that follow a workspace row into this container; keep the peek and `NoSessions` rows in order. Drop `session_row`'s own `pl(XL)` indent.

- [ ] **Step 4: Session row**

`session_row`:
- `h(rems_px(SESSION_ROW_HEIGHT))`.
- Children: `status_mark`, `agent_chip`, name ("agent · title", truncated, `text` color; `weight::MEDIUM` when selected; no accent color), status label (`status_display(state).label` plus the exit code as `status::text` gives it today, `text_muted`, META), and elapsed time right-aligned (`status::time(…)`, `text_muted`, META).
- Selected bg = `theme.selected`; hover bg = `theme.hover` (one `.hover`).
- Keep the status tooltip, ✕ on group hover and the peek.

Keep the lowercase labels ("working", "idle"): `sidebar::tests` pins them.

- [ ] **Step 5: Inbox card**

`render_inbox`: a card pinned at the bottom of the sidebar.
- Card styling: margin `space::S`, padding `space::M`, `bg(theme.card)`, 1 px `theme.border`, `radius::MEDIUM`.
- Header row: an icon tile (`rems_px(28.0)`, `radius::SMALL`).
  - Empty: ✓ in `status_done` on `success_bg`, with "Nothing needs you right now.".
  - Otherwise: `!` in `status_needs_input` on `warning_bg`, with `"{n} waiting for you"`.
- Keep the NextInbox tooltip on the header and the `inbox_row` list inside the card (scroll, max 50 %).

- [ ] **Step 6: Verify, SPEC**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` and `cargo test -p mkrew-ui -j 4` → clean / PASS.
SPEC:
- §10 :681-682: "a dot before its name" → "a letter avatar tinted with the color".
- §14 :926-929: the exception is the workspace avatar.
- §11: the session row shows mark, agent chip, name, label and time; the workspace row shows a working pill.
Suggested message: `feat(ui): calm sidebar — avatars, guide line, working pill, inbox card`

---

### Task 8: Redesign 4 — toolbar replaces the view rail

Inbox: line 27, sections "Layout" and "Toolbar". Spec: §10 (:656-721: diagram :660-671, rail bullet :684-685, Run :703-719). Reverses ADR 0023 (inbox line 15).

**Current state (investigated):**
- `render_view_rail` (main.rs:7629-7684): five view buttons (`view-{title}`, tooltip `keymap.label(view.command())`, `show_view`), plus a compact Run at the foot outside Terminals. It is mounted at :16330-16334 and sized by `VIEW_RAIL_WIDTH` (theme.rs:103-105).
- `render_tab_bar` (:13055-13208) holds Run (full, :13156), History (:13157-13167), + Agent / + Shell (:13168-13181) and Close pane.
- `render_run_button(compact)` (:7689-7768) + `render_run_menu` (:7771-7898).
- `render_main` (:7588-7624) picks the view.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (delete `render_view_rail` and its mount; new `render_toolbar`; `render_main`; `render_run_button`; `render_tab_bar` :13156-13181)
- Modify: `crates/mkrew-ui/src/theme.rs` (delete `VIEW_RAIL_WIDTH`)
- Modify: `docs/SPEC.md` §10; `docs/HUMAN_CHECKS.md:36, :509` (rail mentions)

**Interfaces:**
- Consumes: `icon_button`, `button`, `quiet_button` (Task 6), `theme::mode_glyph`, `Command::ToggleTheme`, `Shell::toggle_theme` (Task 5).
- Produces: `Shell::render_toolbar(&self, theme: &Theme, cx) -> impl IntoElement`.

- [ ] **Step 1: Toolbar**

`render_main` puts `render_toolbar` on top of every view (only with a workspace and a connection, the same condition the rail had):
- `h(rems_px(TOOLBAR_HEIGHT))`, px `space::M`, `border_b` 1 px `theme.divider`, `bg(theme.bg)`.
- **Left — segmented control:** a container with `bg(theme.surface)`, `radius::MEDIUM`, padding 2 px. For each view, take the item from the old rail with its id, tooltip and `show_view` click. Active item: `bg(theme.surface_raised)`, 1 px `theme.border`, `text`, `weight::MEDIUM`. Inactive item: `text_muted` with a `theme.hover` hover.
- **Right**, in order:
  - `icon_button("toggle-theme", theme::mode_glyph(theme.mode), theme)`, tooltip `"Theme: System|Light|Dark"` + `keymap.label(Command::ToggleTheme)`, click → `toggle_theme`.
  - History, `+ Agent`, `+ Shell` as ghost buttons, moved from `render_tab_bar` with the same ids and handlers. Outside Terminals, `+ Agent` / `+ Shell` first call `show_view(Terminals)` (Open question 12).
  - Run.

- [ ] **Step 2: Split Run**

`render_run_button`:
- Drop the `compact` parameter.
- Main half: accent bg, `on_accent` text, "▶ Run".
- ▾ half: same accent bg, `border_l(px(BORDER_WIDTH)).border_color(rgba(theme.on_accent_divider))`. It opens `render_run_menu` as today.
- While live, keep ↻ ■ and the warning color the current code uses.

- [ ] **Step 3: Delete the rail**

- Delete `render_view_rail` and its mount in `Shell::render`.
- Delete `VIEW_RAIL_WIDTH`.
- Remove Run / History / + Agent / + Shell from `render_tab_bar`.
- Keep the tab bar's notices, zoom and Close pane.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` → clean.

- [ ] **Step 4: SPEC and checks**

SPEC §10:
- Diagram :660-671: rows `sidebar | toolbar / tab strip (Terminals) / content`, no rail column.
- :684-685: replace the rail bullet with `Toolbar (44 px, ADR 0027): segmented views Board / Terminals / Docs / Files / Git on the left; theme toggle, History, + Agent, + Shell and the split Run button on the right, in every view.`
- :703-705: Run lives in the toolbar.

HUMAN_CHECKS :36 and :509: mark the rail checks `superseded by ADR 0027` (do not delete them).
Suggested message: `feat(ui): toolbar with segmented views replaces the view rail (ADR 0027)`

---

### Task 9: Redesign 5 — tab strip with a close button per tab

Inbox: line 27, section "Tab strip". Spec: §10 (Terminals).

**Current state (investigated):**
- `render_tab_bar` (:13055-13208): tabs (rename field, urgent status mark + tooltip, click activates, double-click renames), `+` new tab, notices.
- There is no per-tab ✕: `close_tab` (:6387-6427) closes the active tab only.
- `Tabs::remove(tab_id)` (tabs.rs:207) already handles any tab.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (`close_tab` :6387-6427, `run()` `Command::CloseTab`, `render_tab_bar`)
- Modify: `crates/mkrew-ui/src/tabs.rs` (test)
- Modify: `docs/SPEC.md` §10 Terminals

**Interfaces:**
- Produces: `Shell::close_tab(&mut self, tab: TabId, window, cx)` (use the tab id type `tabs.rs` already uses); `Command::CloseTab` passes the active tab's id.

- [ ] **Step 1: Pin `Tabs::remove` on an inactive tab**

Read `tabs.rs:180-240` and the existing tests first. If no test removes an inactive tab, add one with the module's existing constructors:

```rust
    #[test]
    fn remove_inactive_keeps_active() {
        // Three tabs, the third active; removing the first keeps the third active.
        let (mut tabs, ids) = three_tabs_with_last_active(); // build with the helpers this module's tests already use
        tabs.remove(ids[0]);
        assert_eq!(tabs.active_id(), Some(ids[2]));
        assert_eq!(tabs.len(), 2);
    }
```

Replace `three_tabs_with_last_active`, `active_id` and `len` with the real constructor and accessor names in `tabs.rs`. If such a test already exists, skip this step. Run: `cargo test -p mkrew-ui --lib tabs -j 4` → PASS.

- [ ] **Step 2: `close_tab(tab)`**

Split `close_tab`: the body works on the given tab id instead of the active one. Keep the running-sessions confirmation and the last-tab behavior exactly as they are. `Command::CloseTab` calls it with the active id.

- [ ] **Step 3: Restyle the strip**

`render_tab_bar`:
- `h(rems_px(TAB_STRIP_HEIGHT))`, `border_b` `theme.divider`.
- Each tab gets `.group(format!("tab-{id}"))`.
- Active tab: `bg(theme.surface_raised)`, 1 px `theme.border`, `radius::MEDIUM`, `text`. Inactive tab: `text_muted` and a `theme.hover` hover (one `.hover`).
- After the label, add a ✕ via `row_action`. It is always shown on the active tab; on the others it uses `.opacity(opacity::HIDDEN).group_hover(…, |s| s.opacity(opacity::FULL))`. Its click handler:

```rust
cx.listener(move |shell, _, window, cx| {
    cx.stop_propagation(); // never activate the tab being closed
    shell.close_tab(tab_id, window, cx);
})
```

Use `on_mouse_down` + `stop_propagation` if the tab activates on mouse down rather than click; match the tab's own handler.
- Keep rename, double-click, the urgent mark, `+` and the notices.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` → clean.

- [ ] **Step 4: SPEC**

§10 Terminals: `Each tab has a ✕ (always on the active tab, on hover for the others); closing never activates the tab first.`
Suggested message: `feat(ui): raised active tab and a close button per tab`

---

### Task 10: Redesign 6 — empty state, sweep, docs, manual checks

Inbox: line 27, sections "Empty state" and "Verify". Spec: §14 (:932 helpful empty states, :983-986 design check).

**Current state (investigated):**
- `render_tab`'s empty case (:13250-13284) shows `message(None, "This tab has no panes.", detail)` + `button("empty-new-agent", …, primary)` → `new_pane(Agent)` + `new_pane_button("empty-new-shell")`. There are no kbd chips.
- The real shortcuts are `NewAgentTab` = Ctrl+Shift+A and `NewShellTab` = Ctrl+Shift+T. Those open a **new tab**, while the buttons add a pane to this tab (Open question 11).

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (`render_tab` empty case)
- Modify: `docs/decisions/0027-calm-redesign.md`, `docs/SPEC.md` §14, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`, `docs/INBOX.md`

- [ ] **Step 1: Empty tab**

```rust
empty_state(
    theme,
    Some(">_"),
    Some("No panes in this tab"),
    "Start an agent or a shell, or pick a session from the sidebar.".into(),
    None,
)
.child(
    div()
        .flex()
        .gap(px(space::S))
        .child(
            button("empty-new-agent", "New agent", theme, true)
                .child(kbd(self.keymap.label(Command::NewAgentTab), theme, true))
                .on_click(/* unchanged: new_pane(Agent) */),
        )
        .child(
            secondary_button("empty-new-shell", "New shell", theme)
                .child(kbd(self.keymap.label(Command::NewShellTab), theme, false))
                .on_click(/* unchanged: new_pane(Shell) */),
        ),
)
```

Copy the two existing `on_click` closures verbatim from the current `render_tab` empty case; do not change what they do. `Keymap::label` shows `Ctrl` on Windows. Other views already get the look through `message()` → `empty_state` (Task 6).

- [ ] **Step 2: Sweep**

- grep `crates/mkrew-ui/src` for `rgb(0x`, `rgba(0x`, `hsla(` and `DIALOG_SCRIM` outside `theme.rs` and tests → expect none.
- `gpui::transparent_black()` may stay; it is not a palette color.
- grep for the old text `This tab has no panes` → only the new copy.

- [ ] **Step 3: Docs**

- ADR 0027: complete Consequences:
  - the light/dark values changed for contrast (list them)
  - the ⋯ on every row
  - selected names are no longer accent-colored
  - + Agent / + Shell switch to Terminals
  - status glyphs stay where no text label sits beside them (§11)
  - things not matched from the reference, and why: the single pulsing dot became Task 1's three dots; SVG icons became glyphs (no asset pipeline); the design's `Ctrl+Shift+S` chip became the real `NewShellTab` key
- SPEC §14 design check: screenshots in light and dark, or a HUMAN_CHECKS entry if capture is not possible.
- `docs/PROGRESS.md` "Notes for the next session":

```markdown
- 2026-10-08 inbox 3 (plan docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md): working dots (status_mark); pane-wide file drop + composer routing + gpui modality workaround; Ctrl+V smart paste + BMP→PNG (ADR 0026); agent icons fall back to the shipped sample + chips (ADR 0025 amendment); calm redesign — Theme global System/Light/Dark `[ui] theme`, toolbar replaces the view rail, per-tab ✕, new sidebar/inbox/empty state (ADR 0027). Outside the milestone loop.
```

- [ ] **Step 4: HUMAN_CHECKS**

Add `### To verify — 2026-10-08 inbox 3: dots, drop, paste, icons, redesign (plan docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md)`:

```markdown
- [ ] **Hooks (user setup, item 18):** run `mk integration install claude`, `… codex`, `… omp`,
  `… copilot`, restart the agents and the daemon. Claude: prompt → three dots; a permission
  prompt → amber `?` + amber pane border; turn ends → `✓ done` + Inbox entry; later `○ idle`.
  Hover a status: source `hook`. Without the install `done` never appears (SPEC §6).
- [ ] **Working dots:** a working session shows three dots taking turns (~1.5 s cycle) in the
  sidebar row, the workspace pill, the tab, the pane header, the Inbox and board cards; rows with
  other states line up with it.
- [ ] **File drop:** drag 2 files (one path with a space) from Explorer onto (1) the terminal,
  (2) the open composer (Ctrl+Shift+Enter), (3) the pane header → paths typed, the spaced one
  quoted, no Enter; with the composer open they go into the draft and it keeps the keyboard.
  (4) Type a letter in a pane, Alt+Tab to Explorer overlapping Mkrew, drop without moving over
  Mkrew first → still typed. (5) Drag over a pane, release outside the window, click the pane →
  nothing typed. Mkrew must not run as administrator.
- [ ] **Ctrl+V:** in Claude, omp, codex and pwsh panes: text → pasted (bracketed); Win+Shift+S
  then Ctrl+V → `…attachments\<id>\<ulid>.png` typed; PrintScreen then Ctrl+V → a `.png` path;
  Explorer copy of 2 files → 2 paths; empty clipboard in vim → Ctrl+V starts block visual. In the
  composer, an image clipboard + Ctrl+V → path in the draft. No visible stall on a 4K screenshot.
- [ ] **Agent icons:** after a daemon restart + UI reconnect, your existing profiles show ✻ claude,
  ⌬ codex, π omp, ⊙ copilot, >_ shell as bordered chips in the sidebar rows, pane headers, Inbox
  rows, board cards and the Run with… / default agent menus (glyphs, not boxes; light + dark).
- [ ] **Theme:** the toolbar toggle cycles System → Light → Dark; every surface updates at once,
  no dark pieces left in light or the reverse; `settings.toml` gets `[ui] theme = "…"` with your
  comments kept; Ctrl+Shift+J does the same. In System mode, change Windows' app mode → the app
  follows live. A broken settings.toml → the toggle still switches, the palette shows the error,
  the file is untouched.
- [ ] **Both themes, these states visible:** hover, selected session, active tab, working dots,
  the Inbox card empty and non-empty, the empty-state buttons with their Ctrl+Shift+A / T chips.
  Muted text on the sidebar and status labels is readable in light mode.
- [ ] **Layout:** no right rail; toolbar 44 px with the segmented views; switching views, Run and
  its ▾ menu, History, + Agent, + Shell (also from Board/Docs/Files/Git), creating / closing /
  switching tabs, ✕ on an inactive tab closes it without activating it, expanding and collapsing
  workspaces, ⋯ on hover, selecting sessions. At `[ui] font_size = 20` nothing clips.
```

- [ ] **Step 5: Full verification and ticks**

Run the full verification, then run the app (`cargo run -p mkrew-ui`) and walk through the HUMAN_CHECKS "Layout" list once in each theme. Tick line 27 `[x]`. Report file by file what changed (the brief asks for this), plus anything not matched from the reference.
Suggested message: `feat(ui): calm empty state, docs and checks for the redesign (ADR 0027)`

---

### Task 11: The new logo — installer icons, sidebar glyph, README lockup

Inbox: line 23 `[feat] Use the new Mkrew logo everywhere (source: icon/mkrew-1-lanes/, picked tile-dark.svg)`. Spec: §4 (packaging), §14.

**Current state (investigated):**
- Done, uncommitted (user work): `crates/mkrew-ui/build.rs` embeds `resources/windows/mkrew.rc` → `mkrew.ico` (icon resource 1) through `embed-resource = "3"` (`[build-dependencies]` in the user's `crates/mkrew-ui/Cargo.toml` diff). So `mkrew.exe`, the window, the taskbar, Explorer and Alt+Tab use the logo.
- Left:
  - `packaging/mkrew.iss` has no `SetupIconFile=`. Its `UninstallDisplayIcon={app}\bin\mkrew.exe` (:26) already picks up the embedded icon.
  - `packaging/mkrew.wxs` has no `<Icon>`, no `ARPPRODUCTICON`, and the Start Menu `<Shortcut>` (:17) has no `Icon`. [INFERENCE] The shortcut already shows the exe's icon 1, but Add/Remove Programs shows a generic one.
  - There is no logo in the app.
  - `README.md` has no lockup.
  - There is no regen step written down; no rasterizer (`magick`, `resvg`, `rsvg-convert`, `inkscape`) is installed here.
- Assets: `icon/mkrew-1-lanes/glyph-{dark,light,mono}.svg` (256×256 viewBox, one path; dark `#7C8CFF`, light `#4F5BD5` = the SPEC §14 accents), `lockup-*.svg`, `tile-*.svg`.
- The UI already renders SVG without an asset pipeline: `diagram.rs:50-53` → `Image::from_bytes(ImageFormat::Svg, bytes)` + `img(Arc<Image>)`.
- `mk.exe` (CLI) and `mkrew-daemon.exe` (no window) icons: the item says "if wanted". Default: skip them (Open question 13).

**Files:**
- Modify: `packaging/mkrew.iss` (`[Setup]`), `packaging/mkrew.wxs`
- Modify: `crates/mkrew-ui/src/main.rs` (sidebar header from Task 7), `crates/mkrew-ui/src/theme.rs` (logo size)
- Modify: `README.md` (top), `crates/mkrew-ui/build.rs` (doc comment: regen command)

**Interfaces:**
- Consumes: the Task 7 sidebar header; `Theme.dark`.
- Produces: `fn logo(theme: &Theme) -> gpui::Img` in main.rs; `theme::LOGO_SIZE: f32 = 18.0`.

- [ ] **Step 1: Installer icons**

`packaging/mkrew.iss`, `[Setup]` after `WizardStyle=modern` (the path is relative to the `.iss` folder):

```ini
SetupIconFile=..\crates\mkrew-ui\resources\windows\mkrew.ico
```

`packaging/mkrew.wxs`, inside `<Package>` before `<StandardDirectory Id="ProgramFiles64Folder">`:

```xml
    <Icon Id="MkrewIcon" SourceFile="$(sys.SOURCEFILEDIR)..\crates\mkrew-ui\resources\windows\mkrew.ico" />
    <Property Id="ARPPRODUCTICON" Value="MkrewIcon" />
```

and add `Icon="MkrewIcon"` to `<Shortcut Id="MkrewStartMenu" …>`. Look up the WiX 7 `sys.SOURCEFILEDIR` variable before relying on it; if it is not available, pass the path as a define from `scripts/package.ps1`, as `BinDir` already is.

Run: build the installer with the `mkrew-build-setup` skill (`dist\mkrew-<version>-x64-setup.exe`). Expected: the setup .exe shows the logo in Explorer; after installing, Add/Remove Programs and the Start Menu entry show it.

- [ ] **Step 2: Glyph in the app**

`theme.rs`, next to `WORKSPACE_AVATAR_SIZE`:

```rust
/// Side of the Mkrew logo glyph in the sidebar header.
pub const LOGO_SIZE: f32 = 18.0;
```

main.rs:

```rust
/// The Mkrew logo glyph for the current appearance (`icon/mkrew-1-lanes/glyph-*.svg`): accent
/// on dark, the darker light accent on light.
fn logo(theme: &Theme) -> gpui::Img {
    static DARK: LazyLock<Arc<Image>> = LazyLock::new(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            include_bytes!("../../../icon/mkrew-1-lanes/glyph-dark.svg").to_vec(),
        ))
    });
    static LIGHT: LazyLock<Arc<Image>> = LazyLock::new(|| {
        Arc::new(Image::from_bytes(
            ImageFormat::Svg,
            include_bytes!("../../../icon/mkrew-1-lanes/glyph-light.svg").to_vec(),
        ))
    });
    let image = if theme.dark { DARK.clone() } else { LIGHT.clone() };
    img(image).flex_none().size(theme::rems_px(theme::LOGO_SIZE))
}
```

In the sidebar header (Task 7 Step 1), put `logo(theme)` left of "Workspaces" with `gap(px(space::S))`. Use the same `img` / `Image` imports `diagram.rs` uses.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` → clean. The `include_bytes!` path is checked at compile time.

- [ ] **Step 3: README lockup**

At the top of `README.md`, replace the `# Mkrew` line with:

```markdown
<picture>
  <source media="(prefers-color-scheme: dark)" srcset="icon/mkrew-1-lanes/lockup-dark.svg">
  <img alt="Mkrew" src="icon/mkrew-1-lanes/lockup-light.svg" height="48">
</picture>
```

Keep a `# Mkrew` heading below it only if the README's other links point at it (grep for `#mkrew`).

- [ ] **Step 4: Regen note**

Extend the `build.rs` doc comment (one line):

```rust
//! Regenerate the .ico after the SVG changes (16, 20, 24, 32, 40, 48, 64, 256 px), e.g. with ImageMagick:
//! `magick -background none -density 384 icon/mkrew-1-lanes/tile-dark.svg -define icon:auto-resize=256,64,48,40,32,24,20,16 crates/mkrew-ui/resources/windows/mkrew.ico`
```

- [ ] **Step 5: Checks and tick**

Add to the Task 10 HUMAN_CHECKS block:

```markdown
- [ ] **Logo:** the setup .exe, Add/Remove Programs and the Start Menu entry show the logo
  (Inno and MSI builds); the sidebar header shows the glyph, crisp at `[ui] font_size` 13 and 20,
  in light and dark; the README on GitHub shows the lockup in both color schemes.
```

Run the full verification. Tick line 23 `[x]` after the user confirms the installer check.
Suggested message: `feat: Mkrew logo in the installers, sidebar header and README`

---

## Open questions (defaults used above)

| # | Question | Options | Default in this plan |
| --- | --- | --- | --- |
| 1 | Item 18: done / needs-input never show | a) user runs `mk integration install …` (no code) · b) a + a palette command "Install agent hooks" | **a** |
| 2 | Item 22 vs the brief's single pulsing dot | a) 3 dots taking turns on the 500 ms tick, everywhere · b) one dot pulsing 0.35↔1 over 1.4 s (`with_animation`, repaints the whole window every frame while working) · c) 3 dots with smooth `with_animation` | **a** |
| 3 | Sidebar marks for idle / needs input | a) keep ○ and `?` glyphs (the brief's ring and amber dot are close; glyphs never rely on color alone) · b) ring / amber dot in the session row only, glyphs elsewhere | **a** |
| 4 | Item 19: where did the drop fail? | a) build all three fixes (whole pane, composer routing, modality workaround) · b) tell me where you dropped and I trim the plan | **a** |
| 5 | Ctrl+V with text when a program wants ^V | a) Ctrl+V always pastes when the clipboard has something (like Windows Terminal) · b) only in agent panes, shells keep ^V | **a** |
| 6 | `image` crate for BMP → PNG (touches `crates/mkrew-ui/Cargo.toml` + `Cargo.lock`, which hold your uncommitted edits) | a) add it (already in the lock via gpui, no download) · b) skip; PrintScreen images stay unsupported | **a** |
| 7 | Item 21: icons for your existing profiles | a) daemon falls back to the shipped icon · b) you add `icon = "…"` lines by hand · c) a + chips of running agents on the workspace row | **a** |
| 8 | Redesign removes the view rail built for inbox line 15 (ADR 0023) | a) yes, ADR 0027 supersedes 0023; line 15 stays as you left it · b) keep the rail and skip the toolbar | **a** |
| 9 | Theme persistence (the brief says "don't change persistence" and also "persist the mode") | a) `[ui] theme` in settings.toml, written by the toggle (comments kept, never writes a broken file) · b) setting read only; the toggle lasts for the session | **a** |
| 10 | `ToggleTheme` key (brief: none needed; every command needs one today) | a) `Ctrl+Shift+J` · b) allow commands without a default (keymap + template + tests change) | **a** |
| 11 | Empty-state chips: the buttons add a pane here, the shortcuts open a new tab | a) show the real tab shortcuts anyway · b) no chips · c) buttons open a new tab too | **a** |
| 12 | + Agent / + Shell outside Terminals | a) shown in every view, switch to Terminals first · b) only in Terminals | **a** |
| 13 | Item 23: icons for `mk.exe` / `mkrew-daemon.exe` | a) skip (CLI and windowless daemon; Explorer only) · b) embed the same `.ico` (a `build.rs` per crate + `embed-resource`) | **a** |
| 14 | Item 23: where the logo shows in the app | a) sidebar header only · b) a + an About entry in the palette | **a** |
| 15 | Item 23: `.ico` regeneration | a) a documented ImageMagick command · b) a script in `scripts/` (needs a rasterizer installed) | **a** |
