# Perf 2026-10-28: Whole-Window Redraws, Daemon Watcher, Terminal Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the app from stuttering while agents work: no animation-driven or off-screen whole-window redraws in the UI, fewer `git.changed` and watchers in the daemon, a cheaper screen scan, a smaller UI scrollback and a History replay that does not block the UI thread.

**Architecture:**
- Task 1 (UI): a working session shows a still dot in its own status color; `status::redraw_due` redraws only when a shown time label changes (ADR 0086).
- Task 2 (UI): a pure `redraw` decision skips the root `cx.notify()` for daemon data that is not on screen; models always update.
- Task 3 (daemon + UI + protocol 49): a batch waits up to 5 s while an agent in the project works (`Hub.working`); the UI releases kept watchers with `watch.release` (ADR 0087).
- Tasks 4-6 (terminal path, ADR 0088): rate-limited tail-only screen scan; UI scrollback 1,000 lines like the daemon; History replays 1 MiB from a safe boundary, with the emulator built off the UI thread.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), alacritty_terminal, notify, serde, insta. No new dependencies.

**Spec:** `docs/SPEC.md`, sections cited per task.
Source of the choices: the 2026-10-28 perf audit (items 1-7 in the chat that produced this plan), re-verified against `docs/superpowers/plans/2026-10-10-inbox-perf.md` Tasks 16, 1, 8, 13, 14 and 15, which this plan supersedes.
Those old tasks were written against 2026-10-13 code and are not to be executed; mark their rows `thay bằng 2026-10-28-perf-redraw-terminal.md` when the matching task here lands.

| Task | Audit item | Old plan task | Crates | ADR | Protocol |
| --- | --- | --- | --- | --- | --- |
| 1 | 1 + 2 (pulse, stepping dots) | 16 | mkrew-ui | 0086 | - |
| 2 | 3 (event redraws) | 1 | mkrew-ui | - | - |
| 3 | 4 (watcher) | 8 | mkrew-daemon, mkrew-protocol, mkrew-ui | 0087 | 48 -> 49 |
| 4 | 5 (screen scan) | 13 | mkrew-daemon, mkrew-core | 0088 (creates) | - |
| 5 | 6 (UI scrollback) | 14 | mkrew-ui | 0088 (appends), 0082 amended | - |
| 6 | 7 (History replay) | 15 | mkrew-ui, mkrew-core, mkrew-cli (comment) | 0088 (appends) | - |

**Order:** Tasks run 1 to 6.
Hard dependency: Task 6 uses `Emulator::for_replay` from Task 5 (without Task 5, use `Emulator::new` instead).
Soft links: Tasks 1 and 2 both edit `shell/connection.rs` (Task 1 the `STATUS_TICK` loop, Task 2 the dispatch arms); Task 3 and Task 4 both edit `crates/mkrew-daemon/src/session.rs` (`Actor::set_state` vs `Actor::scan_screen`); re-read the function before editing.
Tasks 4, 5 and 6 share ADR 0088: the first to run creates it, the later ones append their section.

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1). No `match "claude"` in Rust code.
- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14). Status colors always come with an icon or text.
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network. No test that only pins wiring, wording or a constant.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same commit. ADR numbers: 0086 (Task 1), 0087 (Task 3), 0088 (Tasks 4-6). The highest existing ADR on 2026-10-28 is 0085; list `docs/decisions/` before creating one and renumber if taken.
- **Protocol version:** only Task 3 changes the wire format. It bumps `PROTOCOL_VERSION` (`crates/mkrew-protocol/src/lib.rs:16`, 48 today) to 49. Check the number again before the bump. After it, run `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` before the daemon tests.
- **Migrations:** none. Never delete a migration or a table.
- Add no new crates. Do not touch `Cargo.toml`/`Cargo.lock`.
- GPUI rules: `.hover()` at most once per element; an element with `track_focus` grabs focus on any mouse down; never `match self.store().x() { ... self.store() ... }` (bind the result first).
- **Commits:** each task ends with its own Conventional Commit, made with `git -c user.email=tungnv975@gmail.com commit`. Stage only that task's paths, and only that task's hunks (`git add -p`) in shared files (`shell/connection.rs`, `session.rs`, `theme.rs`, `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`). Never `git add -A` / `.`, never push, reset, checkout, stash or clean. Never stage `.github/workflows/*`, `packaging/`, `docs/BUILD_VI.md`, `docs/INBOX.md`, this plan, or the x11/wayland lines in `crates/mkrew-ui/Cargo.toml`.
- Other agents and the user edit the tree at the same time (on 2026-10-28: `settings.rs`, `settings_fields.rs`, `palette/tests.rs`, `shell/focus.rs`, `shell/inspector.rs`, `shell/thread_panel.rs`, `HUMAN_CHECKS.md`, `SPEC.md` were modified by someone else). Re-read a file before editing it and leave hunks you did not write alone.
- **HUMAN_CHECKS:** the first task to run creates the section `### To verify - 2026-10-28 perf (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)` directly above the first existing `### To verify` heading (the file uses `-`, not an em dash). Later tasks append their bullets to the end of that section.
- Verify with `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (in the omp shell plain `bash` is WSL). On OOM, LNK1201/LNK1318 or a rustc STATUS_STACK_BUFFER_OVERRUN, run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 2` in order.
- The C: drive is nearly full: no `cargo clean`, no extra target dirs. If a build fails with "Access is denied" on an `.exe`, rename it to `*.locked-old.exe`. Never kill the user's daemon, UI or agents.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.
- `crates/mkrew-ui/src/main.rs` is large and `impl Shell` lives in `src/shell/*.rs`: read ranges, never the whole file. Line numbers below are from 2026-10-28 and drift as tasks land; re-find each function by name.
- UI behavior cannot be unit-tested headless: pure logic gets tests; every visible change gets a HUMAN_CHECKS line that the user runs with `cargo run -p mkrew-ui`.
- Nothing in this plan was compiled while it was written. Names marked unverified must be checked against the code; adapt names and lines, never the design. If the design no longer fits, stop that task and report why.

## Review Focus

1. **A `done` or `needs_input` of a session you are not looking at (other workspace, right panel closed, sidebar hidden).** Expected: sidebar marks, the waiting count and the OS notification update at once; only usage/turn/plan/ports/queue/board/comment data of hidden surfaces skips the redraw, and it is current when shown. Pinned by Task 2's `redraw` tests.
2. **A working agent with the window in front and no output.** Expected: the root renders once per second in the first minute (seconds label), then once per minute; no animation frames; every status is still told apart by color plus glyph/text. Pinned by Task 1's `status::redraw_due` tests and the color-distinct test.
3. **A prompt printed just after a screen scan, then silence.** Expected: the session still becomes `needs_input` at the trailing scan, at most 250 ms later; a hook-only profile still leaves `Starting`. Pinned by Task 4's `ScanRate` and `scan::changed` tests.
4. **Closing the right panel or switching workspace while an agent edits files, then reopening Files/Git.** Expected: the watcher of a project nobody views is released, a project with a live session keeps its watcher, and reopening Files/Git shows current data. Pinned by Task 3's release tests.
5. **Scrolling back in a long agent pane, and opening a long ended session in History.** Expected: the pane keeps the newest 1,000 lines (all the daemon sends after a re-attach), History opens without freezing the window and never starts mid-escape or mid-character. Pinned by Task 5's scrollback tests and Task 6's `replay_start` tests.

---


### Task 1: A working agent shows one still colored dot: no pulse, no stepping dots, redraw only when the label changes (perf A1 + N1, ADR 0086)

**Covers:** audit item "working dot pulse / stepping dots" (old plan `2026-10-10-inbox-perf.md` Task 16, perf A1 + N1). Today a working agent costs one whole-window render every 500 ms (stepping dots) plus 12 renders per second while a pulsing tab dot or workspace pill is on screen.
**User decision (verbatim, old plan tick 2026-10-14):** "Làm - nhưng làm theo kiểu thay vì nhấp nháy thì mỗi status nên có chấm màu riêng để dễ nhìn - nếu không ảnh hưởng performance". This overrides the earlier `Giữ` tick on `docs/PERF_REVIEW.md:27`.
A still colored dot has no per-frame cost, so the condition "if it does not hurt performance" holds.

**Current state (verified 2026-10-28):**
- Nothing of old Task 16 is done: `git log -S"loading_dots"` finds no commit, and `PULSE_FPS` was added in `07c767c` (ADR 0054) and is still there.
- Pulse: `status_dot(id, color, hollow, pulse) -> AnyElement` at `crates/mkrew-ui/src/shell/widgets.rs:925-951` wraps the dot in `with_animation(Animation::new(theme::PULSE_PERIOD).repeat_synced().with_easing(pulsating_between(..)).with_max_fps(theme::PULSE_FPS), ..)` when `pulse`. Import `widgets.rs:4` `use gpui::{Animation, AnimationExt, pulsating_between};` is used only there.
- `PULSE_PERIOD` and `PULSE_MIN_OPACITY` (`theme.rs:1782-1783`) are ALSO used by `voice::pulse` (`voice.rs:103-107`, the focus ring, and its test `voice.rs:338-341`), so they stay. Only `PULSE_FPS` (`theme.rs:1791-1792`) goes; its single reader is `widgets.rs:947`.
- `pulse = true` callers: tab dot (`shell/pane_render.rs:154-160`, from `Shell::tab_dot` `:895-920`, the `SessionState::Working if urgent.kind == SessionKind::Agent` arm), sidebar workspace pill (`shell/sidebar.rs:1173-1178`), voice recording dot in the sidebar footer (`shell/sidebar.rs:1058-1063`) and in the recording pill (`shell/voice_input.rs:204`). `pulse = false` callers: `shell/sidebar.rs:210` (RUN row), `shell/run.rs:377`, `shell/phone_access.rs:186`, `shell/menu_render.rs:158-163`.
- Stepping dots: `status::WORKING_DOTS` / `status::working_dot(now)` (`status.rs:13-14`, `:24-29`, one lit per `theme::STATUS_TICK` = 500 ms, `theme.rs:85`) and `widgets::working_dots(now, color)` (`widgets.rs:903-921`). Users: `status_mark` Working branch (`widgets.rs:884-901`; callers `cards.rs:349`, `cards.rs:693`, `inspector.rs:569`, `peek.rs:602`, `send_dialog.rs:374`, `pane_render.rs:1045` via `status_pill` `:1020`), `mark_slot` (`shell/sidebar.rs:1626-1656`, one dot dimmed on `working_dot(now) == 0`; caller `:1618`), and the loading marks `starting_button` (`pane_render.rs:968`), `voice_status` Transcribing (`sidebar.rs:1065`), recording pill Transcribing (`voice_input.rs:224`), Jira loading / saving / opening row (`jira.rs:665`, `:674`, `:705`).
- Redraw: the status loop (`shell/connection.rs:75-116`) wakes every 500 ms and calls `cx.notify()` when `due || status::redraw_due(sessions, last, now)`. `due` = `board_due || shell.starting_pane.is_some() || transcribing || jira_busy` (`connection.rs:97-105`) and covers every loading mark, so the loading dots need no other tick. `status::redraw_due` (`status.rs:105-117`) returns `true` every tick for `Working` and only on a changed label for `Idle`.
- The elapsed label of a Working session is `status::time` (`status.rs:49-53`, `format_duration(now - changed_at)`, same cadence as Idle: each second under a minute, then each minute). It is already shown by `status::text` (`working · 2m`) in the pane pill, cards, inspector, peek, send dialog, fan-out and the sidebar row time.
- Status colors already exist per state: `sidebar::status_color` (`sidebar.rs:53-62`) maps Working `status_working` (indigo), NeedsInput `status_needs_input` (amber), Done `status_done` (green), Exited `status_exited` (red), Idle `status_idle` (gray), Starting/Unknown `text_muted`. `status_idle` exists in all palettes (`theme.rs:708`, `:886`, `:980`, `:1073`, `:1144`), so no new token is needed. Idle is drawn as the hollow ring (tab dot) or the `○` glyph, so it differs by shape too.
- `status_display(Working).icon` is `"•••"` (`sidebar.rs:42`). It is reached only through `status::icon` in places that skip Working (`sidebar.rs:1475` `mark.filter(|_| !working)`, `mark_slot`, `status_mark`) and by the palette entry text `"••• working"` (`palette.rs:701`, pinned in `palette/tests.rs:957`).
- The phone web UI has its own mark (`crates/mkrew-cli/web/model.mjs:5`); not touched.
- SPEC already rejects a pulse for the Run pill (`SPEC.md:997-998`, `shell/run.rs`).

**Files:**
- Modify: `crates/mkrew-ui/src/status.rs` (module doc, `LOADING_DOTS`, `loading_dot`, `icon` doc, `redraw_due`, tests)
- Modify: `crates/mkrew-ui/src/sidebar.rs` (`status_display` Working glyph)
- Modify: `crates/mkrew-ui/src/theme.rs` (delete `PULSE_FPS`; doc fixes on `STATUS_TICK`, `STATUS_DOT`, `STATUS_DOT_GAP`, `SIDEBAR_MARK_WIDTH`, `PULSE_PERIOD`, `opacity::STATUS_DOT_DIM`)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`status_mark`, `loading_dots`, `status_dot`, import)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs`, `shell/sidebar.rs`, `shell/run.rs`, `shell/phone_access.rs`, `shell/menu_render.rs`, `shell/voice_input.rs`, `shell/jira.rs`, `shell/cards.rs`, `shell/inspector.rs`, `shell/peek.rs`, `shell/send_dialog.rs`, `shell/connection.rs` (comment only), `board.rs` (comment only)
- Test: `crates/mkrew-ui/src/status.rs` (`mod tests`), `crates/mkrew-ui/src/sidebar/tests.rs`, `crates/mkrew-ui/src/palette/tests.rs`
- Modify: `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/decisions/0054-simplified-terminals-screen.md`, `docs/decisions/0027-calm-redesign.md`
- Create: `docs/decisions/0086-still-working-dot.md`

**Interfaces:**
- Changes: `widgets::status_dot(color: u32, hollow: bool) -> gpui::Div` (no `id`, no `pulse`; always still).
- Changes: `widgets::status_mark(state: SessionState, theme: &Theme) -> gpui::AnyElement` (no `now`).
- Renames: `widgets::working_dots(now: i64, color: gpui::Rgba) -> gpui::Div` becomes `widgets::loading_dots` (same signature); `status::WORKING_DOTS` becomes `status::LOADING_DOTS`; `status::working_dot(now_ms: i64) -> usize` becomes `status::loading_dot`.
- Changes: `Shell::tab_dot(&self, tab: &Tab, urgent: Option<&Session>, theme: &Theme) -> Option<(u32, bool)>`; `pane_render::status_pill(session: &Session, pill: String, theme: &Theme)` (no `now`); `sidebar::mark_slot(mark: Option<SessionState>, theme: &Theme)` (no `now`, private fn).
- Removes: `theme::PULSE_FPS`. Keeps `theme::PULSE_PERIOD`, `theme::PULSE_MIN_OPACITY` (voice ring).
- Behavior: `status::redraw_due(sessions, last_ms, now_ms)` is `true` only when a Working or Idle session's `status::time` label differs between `last_ms` and `now_ms`.
- Consumes (unchanged): `sidebar::status_color`, `theme.status_working|needs_input|done|exited|idle`, `theme::STATUS_DOT_LARGE`, `theme::STATUS_MARK_WIDTH`, `theme::SIDEBAR_MARK_WIDTH`.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/status.rs` `mod tests`, replace `working_dots_take_turns_one_per_tick` (lines 175-185) with:

```rust
    #[test]
    fn loading_dots_take_turns_one_per_tick() {
        let step = theme::STATUS_TICK.as_millis() as i64;
        let lit: Vec<usize> = (0..4).map(|i| loading_dot(i * step)).collect();
        assert_eq!(lit, [0, 1, 2, 0]);
        // Mid-tick stays on the same dot; a clock before 1970 is dot 0.
        assert_eq!(loading_dot(step + step / 2), 1);
        assert_eq!(loading_dot(-5), 0);
        assert_eq!(icon(SessionState::Idle), "○");
        assert_eq!(icon(SessionState::NeedsInput), "?");
        assert_eq!(icon(SessionState::Working), "●");
    }
```

Replace `redraws_only_when_the_shown_status_changes` (lines 270-290) with:

```rust
    #[test]
    fn redraws_only_when_the_shown_status_changes() {
        let tick = theme::STATUS_TICK.as_millis() as i64;
        let still = [
            session(SessionState::Done, 0),
            session(SessionState::NeedsInput, 0),
            session(SessionState::Exited, 0),
        ];
        assert!(!redraw_due(&still, 0, 600 * SEC));

        // Working and idle show a time: each second under a minute, then only when the
        // minute changes. Nothing else moves while an agent works (ADR 0086).
        for state in [SessionState::Working, SessionState::Idle] {
            let sessions = [session(state, 0)];
            assert!(redraw_due(&sessions, 4 * SEC + 600, 5 * SEC + 100), "{state:?}");
            assert!(!redraw_due(&sessions, 5 * SEC + 100, 5 * SEC + 600), "{state:?}");
            assert!(!redraw_due(&sessions, 90 * SEC, 90 * SEC + tick), "{state:?}");
            assert!(redraw_due(&sessions, 119 * SEC + 800, 120 * SEC + 300), "{state:?}");
        }
    }
```

In `crates/mkrew-ui/src/sidebar/tests.rs`, after `every_state_has_an_icon_and_text` (ends line 163), add the guard for the user's "each status has its own color" decision (it passes today and fails if a token is ever re-pointed at another state's color):

```rust
#[test]
fn each_status_has_its_own_dot_color_in_both_themes() {
    use SessionState::*;
    for theme in [Theme::dark(), Theme::light()] {
        let colors: HashSet<u32> = [Working, NeedsInput, Idle, Done, Exited]
            .into_iter()
            .map(|state| status_color(&theme, state))
            .collect();
        assert_eq!(colors.len(), 5, "{}", theme.name);
    }
}
```

In `crates/mkrew-ui/src/palette/tests.rs:957` change the expected text:

```rust
    assert_eq!(working.map(|e| e.shortcut.as_str()), Some("● working"));
```

(`theme.name` is used as a `Display` value in `theme.rs` tests, e.g. `"{}", theme.name`; if its type does not format with `{}`, use `{:?}`. Flagged unverified.)

- [ ] **Step 2: Run them, expect FAIL**

Run: `cargo test -p mkrew-ui --lib -j 4 -- status:: sidebar::tests palette::tests`
Expected: compile error `cannot find function loading_dot in this scope` in `status.rs`. After adding only a `loading_dot` stub the next failures would be `redraws_only_when_the_shown_status_changes` (`Working` is `true` every tick) and the palette test (`"••• working"` != `"● working"`). The color guard test passes already.

- [ ] **Step 3: Implement `status.rs` and the glyph**

`status.rs` lines 1-3, module doc: replace `working dots, elapsed/idle time` with `working/idle time`:

```rust
//! Per-session status display (SPEC §11): working/idle time, tab urgency, hover text
//! and the pane border. Pure; `main.rs` renders the result with [`crate::sidebar::status_display`]
//! icons and [`crate::sidebar::status_color`] colors.
```

Replace lines 13-14 and 24-34 with:

```rust
/// Dots of the loading mark (a pane being started, a voice transcription, a Jira fetch): one
/// is lit per [`theme::STATUS_TICK`]. A working session shows one still dot instead (ADR 0086).
pub const LOADING_DOTS: usize = 3;
```
```rust
/// The lit dot of the loading mark at `now_ms`: steps on the status tick.
pub fn loading_dot(now_ms: i64) -> usize {
    let step = theme::STATUS_TICK.as_millis().max(1) as i64;
    (now_ms.max(0) / step) as usize % LOADING_DOTS
}

/// The state's glyph; `working` is drawn as a still dot (`widgets::status_dot`) wherever there
/// is room for a mark.
pub fn icon(state: SessionState) -> &'static str {
    status_display(state).icon
}
```

Replace `redraw_due` (lines 105-117) with:

```rust
/// Whether the status display changed between `last_ms` and `now_ms` (checked every
/// [`theme::STATUS_TICK`]): a working or idle session's time text changes once per second,
/// then once per minute. Nothing animates (ADR 0086).
pub fn redraw_due(sessions: &[Session], last_ms: i64, now_ms: i64) -> bool {
    sessions.iter().any(|s| {
        time(s.state, s.state_changed_at, last_ms) != time(s.state, s.state_changed_at, now_ms)
    })
}
```

`SessionState` is still used by the other functions in `status.rs`, so the import stays.

`sidebar.rs:42`:

```rust
        SessionState::Working => ("●", "working"),
```

- [ ] **Step 4: Implement the widgets**

`shell/widgets.rs`: delete line 4 (`use gpui::{Animation, AnimationExt, pulsating_between};`). Replace `status_mark`, `working_dots` and `status_dot` (lines 884-951) with:

```rust
/// A session's status mark (SPEC §11): one still dot in the state's color while working, else
/// the state's glyph in its color. Fixed width, so rows with different states line up.
pub(crate) fn status_mark(state: SessionState, theme: &Theme) -> gpui::AnyElement {
    let color = sidebar::status_color(theme, state);
    let mark = div()
        .flex_none()
        .w(theme::rems_px(theme::STATUS_MARK_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    if state == SessionState::Working {
        return mark.child(status_dot(color, false)).into_any_element();
    }
    mark.text_color(rgb(color))
        .child(status::icon(state))
        .into_any_element()
}

/// The loading mark: three dots taking turns, one lit per [`theme::STATUS_TICK`] (a pane being
/// started, a voice transcription, a Jira fetch: all last seconds, ADR 0086). The status loop
/// keeps the root redrawing while one is on screen (`connection.rs` `due`).
pub(crate) fn loading_dots(now: i64, color: gpui::Rgba) -> gpui::Div {
    let lit = status::loading_dot(now);
    div()
        .flex()
        .items_center()
        .gap(theme::rems_px(theme::STATUS_DOT_GAP))
        .children((0..status::LOADING_DOTS).map(move |i| {
            div()
                .size(theme::rems_px(theme::STATUS_DOT))
                .rounded_full()
                .bg(color)
                .opacity(if i == lit {
                    opacity::FULL
                } else {
                    opacity::STATUS_DOT_DIM
                })
        }))
}

/// A round status dot: filled in `color`, or `hollow` (a ring: idle, stopped). Always still: a
/// looping animation renders the whole window for as long as it is on screen (ADR 0086).
pub(crate) fn status_dot(color: u32, hollow: bool) -> gpui::Div {
    div()
        .flex_none()
        .size(theme::rems_px(theme::STATUS_DOT_LARGE))
        .rounded_full()
        .when(hollow, |el| {
            el.border(px(theme::BORDER_WIDTH)).border_color(rgb(color))
        })
        .when(!hollow, |el| el.bg(rgb(color)))
}
```

Rename `working_dots` to `loading_dots` at its other callers: `shell/jira.rs:665`, `:674`, `:705`; `shell/pane_render.rs:968` (and the doc of `starting_button`, line 947: `with the loading dots`); `shell/sidebar.rs:1065`; `shell/voice_input.rs:224`. No other change at those lines.

- [ ] **Step 5: Callers of `status_dot`, `status_mark`, `tab_dot`, `mark_slot`**

`shell/pane_render.rs:154-161` becomes:

```rust
                    .children(dot.map(|(color, hollow)| status_dot(color, hollow)))
```

`tab_dot` (`:892-920`) becomes:

```rust
    /// A tab's status dot from its most urgent pane, always still and one color per state:
    /// amber needs input, red exited, indigo while an agent works, green for done or a live run
    /// process, else a hollow gray ring (idle, starting). `(color, hollow)`; `None` for an
    /// empty tab.
    pub(crate) fn tab_dot(
        &self,
        tab: &Tab,
        urgent: Option<&Session>,
        theme: &Theme,
    ) -> Option<(u32, bool)> {
        let urgent = urgent?;
        let sessions = self.sidebar.sessions();
        let run_live = tab.sessions().iter().any(|id| {
            sessions.iter().any(|s| {
                s.id == *id
                    && s.state != SessionState::Exited
                    && mkrew_core::run::command_name(s).is_some()
            })
        });
        Some(match urgent.state {
            SessionState::NeedsInput => (theme.status_needs_input, false),
            SessionState::Exited => (theme.status_exited, false),
            SessionState::Working if urgent.kind == SessionKind::Agent => {
                (theme.status_working, false)
            }
            _ if run_live => (theme.status_done, false),
            SessionState::Done => (theme.status_done, false),
            _ => (theme.status_idle, true),
        })
    }
```

`status_pill` (`pane_render.rs:1020-1047`): drop the `now: i64,` parameter and make the mark call `.child(status_mark(state, theme))`. Its only caller `pane_render.rs:642` becomes `parts.push(status_pill(info, pill, theme).into_any_element());` (`now` stays in `pane_status` for `pill_text`).

`shell/sidebar.rs`:
- `:210` `.child(status_dot(id.clone(), color, hollow, false))` becomes `.child(status_dot(color, hollow))` (if `id` is then unused only there, rustc says; it is also cloned elsewhere in that closure, check).
- `:1058-1063` (recording dot in the footer) becomes `Some(row().child(status_dot(theme.status_needs_input, false)))`; update the doc at `:1051-1052` to `a steady dot while it records, the loading dots while it transcribes`.
- `:1173-1178` (workspace pill) becomes `.child(status_dot(theme.status_working, false))` (drop the `SharedString::from(format!("workspace-working-{project_id}"))`).
- `mark_slot` (`:1623-1656`) becomes:

```rust
/// A sidebar row's state mark in a narrow fixed slot (empty without a mark), so agent chips
/// stay in one column without pushing the rows in: one still dot in the state's color while
/// working, else the state glyph.
fn mark_slot(mark: Option<SessionState>, theme: &Theme) -> gpui::AnyElement {
    let slot = div()
        .flex_none()
        .w(theme::rems_px(theme::SIDEBAR_MARK_WIDTH))
        .flex()
        .items_center()
        .justify_center();
    let Some(state) = mark else {
        return slot.into_any_element();
    };
    let color = sidebar::status_color(theme, state);
    if state == SessionState::Working {
        return slot.child(status_dot(color, false)).into_any_element();
    }
    slot.text_size(font_size::META)
        .text_color(rgb(color))
        .child(status::icon(state))
        .into_any_element()
}
```

  and its caller `:1618` becomes `.child(mark_slot(row.mark(), theme))` (`now` is still used by `session_tip` in `compact_session_row`).

`shell/run.rs:377` `status_dot("run-pill-dot", dot, false, false)` becomes `status_dot(dot, false)`. `shell/phone_access.rs:186` becomes `status_dot(theme.status_done, false)`. `shell/menu_render.rs:158-164` becomes `status_dot(color, hollow).into_any_element()` (then `id` and `ix` of `lead_element` may be unused: if so remove the parameters and their callers; `-D warnings` names each; flagged unverified).

`shell/voice_input.rs:204` becomes `.child(status_dot(color, false))`. Update its doc (`:189`): `a steady dot, the waveform, ...; the loading dots and Transcribing after`. The waveform (`voice_meter`) and the ring (`voice::pulse`, unchanged) show liveness while recording.

`status_mark` callers drop `now`: `shell/cards.rs:349` and `:693` (`status_mark(session.state, theme)`), `shell/inspector.rs:569`, `shell/peek.rs:602`, `shell/send_dialog.rs:374` (`status_mark(choice.state, theme)`). In each of these functions `now` is still used for other things (`run_time`, `state_text`, `status::text`, `session_card`), so no parameter chain changes; `-D warnings` would name any that became unused.

Comments: `shell/connection.rs:75` `// Spinner and elapsed/idle times move on their own (§11).` becomes `// Elapsed/idle times and the loading marks move on their own (§11).`; `board.rs:664` `(session spinners are covered by [`status::redraw_due`])` becomes `(session times are covered by [`status::redraw_due`])`.

- [ ] **Step 6: Tokens**

`theme.rs`: delete `PULSE_FPS` and its doc line (`:1791-1792`). Keep `PULSE_PERIOD`, `PULSE_MIN_OPACITY` and change their doc (`:1781`) to `/// One breath of the voice input ring ([`crate::voice::pulse`]); its lowest opacity.` Doc edits:
- `:83-84` `/// Status refresh period: one step of the loading dots and the elapsed/idle time update (SPEC §11, §14).`
- `:93` `/// Diameter of one dot of the loading mark (SPEC §11).`
- `:95` `/// Gap between the dots of the loading mark.`
- `:100` `/// The sidebar's narrower mark slot: one still dot while working, else the glyph.`
- `:201` (inside `mod opacity`) `/// The unlit dots of the loading mark.`

Compile check: `cargo check -p mkrew-ui --all-targets -j 4`. Expected: no errors and no unused `now` / `SharedString` / `id` warnings (fix any it names).

- [ ] **Step 7: Run tests, expect PASS**

Run: `cargo test -p mkrew-ui --lib -j 4 -- status:: sidebar::tests palette::tests theme::`
Expected: PASS. The theme snapshots (`mkrew_dark`, `mkrew_light`) must not change: no token value moved.

- [ ] **Step 8: ADR 0086, SPEC, older ADR notes**

Create `docs/decisions/0086-still-working-dot.md`:

```markdown
# 0086 - A working agent shows one still colored dot

- Date: 2026-10-28
- Status: accepted (the user ticked `Làm` for perf A1 on the 2026-10-14 plan tick, with the note below)
- Milestone: perf review (A1, N1)

## Context

ADR 0027 replaced the pulsing dot with three dots stepping on the 500 ms status tick, and ADR 0054 added a 12 fps pulse to the tab dot and the sidebar workspace pill.
Every animation frame renders the whole window (`theme.rs` said so itself).
While any agent works, the UI rendered 2 times a second for the dots and 12 times a second for the pulse, all day, against SPEC §15's idle CPU budget.
The Run pill already has a still dot for this reason (ADR 0018, ADR 0054).
The user asked that each status have its own distinct dot color instead of motion, as long as it costs nothing.

## Options

1. Keep both (before). Most lively; 14 renders per second while an agent works.
2. Drop the pulse, keep the stepping dots. 2 renders per second.
3. One still dot in the state's color; only the elapsed text moves, once per second for the first minute, then once per minute (as for idle time). Nothing animates.

## Decision

Option 3.
`widgets::status_dot` has no pulse; the `working` mark, the sidebar slot, the tab dot, the workspace pill and the voice recording dots are still dots.
Each state has its own token, always beside an icon or text (SPEC §11, §14): working `status_working` (filled dot), needs input `status_needs_input` (`?`), done `status_done` (`✓`), exited `status_exited` (`✕`), idle `status_idle` (`○`, a hollow ring on the tab).
No new token was needed.
`status::redraw_due` redraws when a working or idle session's time text changes.
The loading marks (a pane being started, a voice transcription, a Jira fetch) keep three stepping dots, renamed `loading_dots`: they last seconds and the status loop already redraws for them (`starting_pane`, `transcribing`, `jira_busy`).
The voice ring keeps `voice::pulse` (it redraws on the voice meter tick anyway).

## Consequences

- SPEC §10, §11, §14 and §20G say so; ADR 0054 and ADR 0027 point here.
- A still dot says "working" less loudly than motion; its color and the `working · 12s` text next to it are the live signal (never color alone, §11).
- gpui's missing reduce-motion query no longer matters for these marks.
- Reverting is local: `status_dot`, `status_mark`, `redraw_due`.
```

`docs/SPEC.md` (replace the fragment on each current line):
- `:789` `` `● 1 working` pill (pulsing dot) `` becomes `` `● 1 working` pill (a still dot) ``.
- `:875-876` `(§11: pulsing accent while an\n  agent works,` becomes `(§11: a still accent dot while an\n  agent works,`.
- `:1038` the whole row becomes `| working | ● a still dot in the working color (no animation, ADR 0086) | elapsed time |`.
- After the table (after `:1043`), add the bullet `- Each state's mark has its own color (`status_working`, `status_needs_input`, `status_done`, `status_exited`, `status_idle`; dark and light), drawn beside its icon or text, never alone. Nothing loops while an agent works: the dot is still and the time text moves (ADR 0086). The loading mark (a pane being started, a transcription, a Jira fetch) is three dots taking turns on the status tick.`
- `:1069` `(a pill, pulsing dot)` becomes `(a pill, a still dot)`.
- `:1462-1463` `Small, slow working dots.` becomes `Nothing loops while an agent works: its mark is a still dot and its time text moves (ADR 0086).`
- `:2258` `It holds a breathing dot, a` becomes `It holds a steady dot, a`; `:2262` `shows the `working` dots and` becomes `shows the loading dots and`.
- `:2265-2266` `Under reduce motion, the dots,\n   the ring, and the wave stand still` becomes `Under reduce motion, the ring\n   and the wave stand still`.
- `:2267-2268` `breathing dot or the `working` dots` becomes `steady dot or the loading dots`.
- `:2334` and `:2354` `working dots` becomes `loading dots`.

`docs/decisions/0054-simplified-terminals-screen.md:31-33`: replace `Working dots (sidebar pill, tab dot) do pulse\n  (`with_animation`, which gpui draws still under reduce-motion); an agent that works prints\n  output anyway.` with `Working dots (sidebar pill, tab dot) pulsed in this ADR; ADR 0086 made them still.`

`docs/decisions/0027-calm-redesign.md:61-62`: append to the bullet `; ADR 0086 later made the working mark a still colored dot too`.

- [ ] **Step 9: HUMAN_CHECKS**

Create (or append to, if a sibling task already made it) this section directly above the first existing `### To verify` heading (line 24 today):

```markdown
### To verify - 2026-10-28 perf (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)

- [ ] **Still working dot, check working dot visually (dark + light) (A1, N1, ADR 0086):** start 2 agents that work for a minute or more. In the sidebar session row, the workspace `● 2 working` pill, the tab dot, the pane pill, the Board cards and the inspector, a working agent shows one steady indigo dot (nothing pulses or steps) next to `working · 12s`; the text still counts seconds, then minutes. Needs input is an amber `?`, done a green `✓`, exited a red `✕`, idle a gray `○` (a hollow gray ring on the tab). The five colors are told apart at a glance in dark and in light, and the dot is readable on the selected row and on the workspace pill. The sidebar slot's dot is now 6 px (it was 4 px): it must not touch the agent chip.
- [ ] **Loading marks keep moving:** a pane being started shows `Starting agent` with three dots taking turns; a voice transcription shows the dots and `Transcribing`; a Jira fetch shows the dots. Recording shows a steady amber dot, the waveform and the ring still breathe.
- [ ] **CPU (A1, N1):** with 2-3 agents `working` and the window visible, `mkrew-ui` CPU in Task Manager over one minute is lower than before this task (before: the dots 2 renders per second plus the 12 fps pulse).
```

Edit the old checks that describe motion (current lines):
- `:235` `It shows a breathing dot, 7 bars` becomes `It shows a steady dot, 7 bars`.
- `:237` `shows only a breathing dot, then the working dots` becomes `shows only a steady dot, then the loading dots`.
- `:938-939` `one\n  blinking dot in a narrow slot` becomes `one\n  still dot in a narrow slot`.
- `:1124` `the three working dots taking turns` becomes `the three loading dots taking turns`.
- `:1179` `session rows show the working dots / status glyph` becomes `session rows show a still working dot / status glyph`.
- `:1556` `prompt → three dots;` becomes `prompt → a still dot;`.
- `:1559-1561` replace the whole `**Working dots:**` bullet with `- [ ] **Working dot:** a working session shows one still dot in the working color in the sidebar row, the workspace pill, the tab, the pane header, the Inbox and board cards; rows with other states line up with it.`
- `:1593` `the three dots take turns;` becomes `the still working dot shows;`.
- `:1597` `its `••• N` count steps.` becomes `its `● N working` pill shows the count.`
- `:1900` `three dots taking turns` becomes `a still dot`.
- `:2788-2791` (T3.6) `every tick while a session works (4-frame spinner),\n  for idle only when its time text changes (each second, then each minute)` becomes `for a working or idle session only when its time text changes (each second, then each minute; ADR 0086)`; delete the sentence `gpui has no\n  reduce-motion query, so the spinner always turns (slow enough to pass; revisit if gpui adds\n  one).`

After the edits run Grep `three dots|working dots|blinking|spinner|pulsing|pulse` over `docs/SPEC.md` and `docs/HUMAN_CHECKS.md`; every remaining hit must be about the loading mark, the voice ring, the fake agent's spinner or history prose.

- [ ] **Step 10: Verify and commit**

Run: `sh scripts/verify.sh` (omp shell `bash` is WSL). Fallback if rustc crashes: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`. Expected: PASS.
Grep the diff for `0x` and raw `px(` numbers: none added (the dot uses `theme::STATUS_DOT_LARGE`).

```
git add crates/mkrew-ui/src/status.rs crates/mkrew-ui/src/sidebar.rs crates/mkrew-ui/src/sidebar/tests.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/board.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/phone_access.rs crates/mkrew-ui/src/shell/menu_render.rs crates/mkrew-ui/src/shell/voice_input.rs crates/mkrew-ui/src/shell/jira.rs crates/mkrew-ui/src/shell/cards.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/peek.rs crates/mkrew-ui/src/shell/send_dialog.rs docs/decisions/0054-simplified-terminals-screen.md docs/decisions/0027-calm-redesign.md docs/decisions/0086-still-working-dot.md
git add -p crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/shell/connection.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "perf(ui): a working agent shows one still colored dot, no pulse or stepping dots"
```

`git add -p` only the hunks this task made in the shared files (`theme.rs`, `connection.rs`, `SPEC.md`, `HUMAN_CHECKS.md`); never `-A` or `.`. Do not push.

**Review focus for this task:**
- A working agent's label crosses a boundary (59.6 s to 60.1 s, 119.8 s to 120.3 s) and must redraw exactly once, and must not redraw on the 500 ms ticks between: `redraws_only_when_the_shown_status_changes` (Working and Idle loop).
- "Starting agent", "Transcribing" and the Jira dots must still step: `loading_dots_take_turns_one_per_tick` pins the step; the redraw comes from `starting_pane` / `transcribing` / `jira_busy` in `connection.rs` `due`, which this task does not touch (checked by the HUMAN_CHECKS "Loading marks keep moving" bullet, no unit test because the loop needs a window).
- Two states never share a dot color in dark or light: `each_status_has_its_own_dot_color_in_both_themes`.


### Task 2: Daemon events redraw the root only when the data is on screen

**Covers:** audit item "daemon events (usage, turn, plan, ports, queue, board, comment, ai generated, conflict) always `cx.notify()` the root Shell" (old plan `2026-10-10-inbox-perf.md` Task 1, perf B1).

No ADR and no SPEC change: no observable behavior changes, only when the root re-renders.
State events (`SessionState`, `SessionExited`, `SessionUpdated`, `SessionTerminalTitle`, `ProjectUpdated`) stay on the sidebar's own `Change::Redraw` path and are not touched.

**Current state (verified 2026-10-28):**
- Nothing of the old Task 1 was built: `git log -S event_needs_redraw` finds only the docs commits `ec79d33` and `c818b9d`; there is no `redraw.rs`.
- Every model `apply` already returns "something shown changed": `usage.rs:36`, `turn.rs:22`, `plan.rs:39`, `run.rs:59` (ports), `composer.rs:149` (queue), `board.rs:335`. Nothing asks whether that thing is on screen.
- `Shell::dispatch` is `shell/connection.rs:805`. Unconditional notifies: board `:816-819` (after `drop_gone_targets`), turns `:820`, usages `:823`, plans `:826`, ports `:829`, composers `:832`, conflicts `:862-865` (`prune(..) || detected`), `CommentUpdated` `:885-890`, `AiGenerated` `:901-911`.
- Not touched: `Change::Redraw` / `Refresh` (`:852-860`), `layout_changed` (`:931`), the docs / git handlers, and the async `load_usage` / `load_ports` / `load_plans` notifies (`:726`, `:754`, `:782`: once per workspace on connect, not hot).
- `SidebarView` observes the Shell (`shell/sidebar_view.rs:17`), so one skipped `cx.notify()` also skips the sidebar re-render. The compact rail and the full sidebar read no usage, turn or queue; see the table for plan and ports.
- Status bar is gone (ADR 0054); `Shell::render` draws title bar (`chrome.rs`), `render_main` (`chrome.rs:110`), dialogs.
- What each model feeds, re-derived from the render code:

| Model | Drawn by | On screen when |
|---|---|---|
| `usages` | `pane_status` ctx share `pane_render.rs:668`; Session tab card `inspector.rs:515` (panel session `focus.rs:166`); Board cards `cards.rs:171` (`BoardCtx.usages`, `board.rs:756`) | pane visible, or Session tab on that session, or Board of the session's workspace |
| `turns` | `pane_status` pill `pane_render.rs:640` | pane visible |
| `composers` queue | `pane_status` `⧗ N queued` `pane_render.rs:658`, `composer_panel` queue `composer.rs:662` | pane visible |
| `plans` | Session tab Plan card `inspector.rs:408`; selected sidebar row bar `sidebar.rs:749` (full sidebar, `focused_session`) | Session tab on that session, or full sidebar and session focused |
| `ports` | pane title `pane_render.rs:831`; Run pill of the active workspace in the title bar `chrome.rs:292` (`run_summary`, `run.rs:12`); RUN group of every expanded workspace `sidebar.rs:46-56,654` | session of the shown workspace (pill, every view, pane titles), or full sidebar |
| `board` tasks | Board `shell/board.rs:722`; pane folder `pane_render.rs:624`; worktree chooser `target_chooser` in Files tab `files_render.rs:195` and Git tab `git_render.rs:230` | workspace shown and (view Board or Terminals, or panel tab Files or Git) |
| `conflicts` | pane badge `pane_render.rs:643`; Board cards `cards.rs:186` | view Board or Terminals |
| `CommentUpdated` | Git comments only in the diff view `git_diff.rs:172`; Docs comments in Docs `docs.rs:539` | Diff view with a Diff comment, or Docs view with a Doc comment, of the shown workspace |
| `AiGenerated` | Git commit box (Git tab), pane composer (Terminals), Ask dialog (modal) | panel tab Git, or view Terminals, or any dialog |

- The right panel is `Files | Session | Git` (ADR 0081/0084): `Shell::panel_shown` `focus.rs:129`, `panel_tab` `:135`, `panel_session` `:166` (the active tab's focused pane, also in Editor and Diff views, unlike `focused_session` `:115`). The Session tab only exists while `[features] session` is on; `panel_tab()` already maps a kept Session tab to Files otherwise.
- `board.apply` also feeds `drop_gone_targets` (`files.rs:838`) which stays unconditional (it re-reads hidden views when shown).
- Every UI action that changes the screen already notifies (view, workspace, tab, panel, dialogs; window visibility `connection.rs:42`, activation `:49`), so a model updated silently while hidden is read fresh by the next render.
- `mkrew_core::settings::SidebarMode { Full, Compact, Hidden }` already exists and `sidebar::Layout::mode()` (`sidebar.rs:212`) returns it: reused, no new enum.
- `Event` has 22 variants (`message.rs:357-425`, new since the old plan: `ProjectCloned`). `Session` has a `position: f64` field (the old test fixture lacked it). `PanelTab` is `Copy + PartialEq` (`inspector.rs:28`).
- Name clash: `main.rs:59` imports `mkrew_ui::docs::Shown`; the new type is only used as `redraw::Shown`.

**Files:**
- Create: `crates/mkrew-ui/src/redraw.rs` (`Shown`, `event_needs_redraw`, `conflicts_need_redraw`, tests)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod redraw;` between `quick_open` line 73 and `router` line 74)
- Modify: `crates/mkrew-ui/src/main.rs` (`use mkrew_ui::redraw;` after the `quick_open` use, line 92)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`Shell::shown`, `Shell::batch_shown`, `dispatch` model block, conflicts block, comment and AI arms)
- Modify: `docs/HUMAN_CHECKS.md` (shared section, if this is the first task to run)

**Interfaces:**
- Produces:
  ```rust
  pub struct Shown {
      pub workspace: Option<Ulid>,
      pub view: View,
      pub panes: Vec<Ulid>,                // visible panes (Terminals view, zoom and hidden window respected)
      pub focused: Option<Ulid>,           // Shell::focused_session()
      pub panel: Option<PanelTab>,         // the right panel's tab while it is drawn
      pub panel_session: Option<Ulid>,     // Shell::panel_session()
      pub sidebar: SidebarMode,            // mkrew_core::settings::SidebarMode
      pub modal: bool,                     // any dialog open
  }
  pub fn event_needs_redraw(event: &Event, shown: &Shown, sessions: &[Session]) -> bool
  pub fn conflicts_need_redraw(shown: &Shown) -> bool
  ```
  and in the shell crate `Shell::shown(&self, window: &Window) -> redraw::Shown` and `Shell::batch_shown<'a>(&self, slot: &'a mut Option<redraw::Shown>, window: &Window) -> &'a redraw::Shown`.
- Consumes: `board::View`, `inspector::PanelTab`, `mkrew_protocol::{Event, Session, CommentContext}`, `tabs::on_screen`, `Shell::{view, focused_session, panel_shown, panel_tab, panel_session, modal_open}`, `sidebar::Layout::mode`.
- Contract: `event_needs_redraw` is asked only after a model reported a change. It answers "does the window draw this data now". Events owned by another handler answer `true`. The match is exhaustive on purpose: a new `Event` variant must be decided here. A session unknown to the sidebar list cannot be placed in a workspace, so it answers `true`.

- [ ] **Step 1: Write the failing test**

Create `crates/mkrew-ui/src/redraw.rs` containing only this module, and add `pub mod redraw;` to `lib.rs` after `pub mod quick_open;`.

```rust
#[cfg(test)]
mod tests {
    use mkrew_protocol::{
        AiGeneratedEvent, CommentId, CommentStatus, ConflictDetectedEvent, DocComment,
        SessionExitedEvent, SessionKind, SessionPlanEvent, SessionPortsEvent, SessionQueueEvent,
        SessionState, SessionStateEvent, SessionTurnEvent, SessionUsageEvent, StateSource,
        TaskDeletedEvent,
    };

    use super::*;

    /// `a` is the focused pane of the shown workspace, `b` lives in a hidden tab of it, `c` in
    /// another workspace.
    struct World {
        ws: Ulid,
        other: Ulid,
        sessions: Vec<Session>,
    }

    impl World {
        fn a(&self) -> Ulid {
            self.sessions[0].id
        }
        fn b(&self) -> Ulid {
            self.sessions[1].id
        }
        fn c(&self) -> Ulid {
            self.sessions[2].id
        }
    }

    fn session(project_id: Ulid) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
        }
    }

    fn world() -> World {
        let (ws, other) = (Ulid::new(), Ulid::new());
        World {
            ws,
            other,
            sessions: vec![session(ws), session(ws), session(other)],
        }
    }

    /// Terminals view of `ws`, `a` focused, no right panel, compact rail.
    fn terminals(w: &World) -> Shown {
        Shown {
            workspace: Some(w.ws),
            view: View::Terminals,
            panes: vec![w.a()],
            focused: Some(w.a()),
            panel: None,
            panel_session: Some(w.a()),
            sidebar: SidebarMode::Compact,
            modal: false,
        }
    }

    /// Editor view of `ws`: no pane drawn, no panel, no sidebar rows.
    fn nothing_shown(w: &World) -> Shown {
        Shown {
            workspace: Some(w.ws),
            view: View::Editor,
            panes: Vec::new(),
            focused: None,
            panel: None,
            panel_session: Some(w.a()),
            sidebar: SidebarMode::Hidden,
            modal: false,
        }
    }

    fn usage(id: Ulid) -> Event {
        Event::SessionUsage(SessionUsageEvent {
            session_id: id,
            input: 1,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            context: 0,
            context_window: None,
            model: None,
            transcript: None,
        })
    }

    fn turn(id: Ulid) -> Event {
        Event::SessionTurn(SessionTurnEvent {
            session_id: id,
            n: 1,
            started_at: 0,
            ended_at: 1,
            files: Vec::new(),
        })
    }

    fn queue(id: Ulid) -> Event {
        Event::SessionQueue(SessionQueueEvent {
            session_id: id,
            items: Vec::new(),
        })
    }

    fn plan(id: Ulid) -> Event {
        Event::SessionPlan(SessionPlanEvent {
            session_id: id,
            name: "plan".into(),
            done: 1,
            total: 2,
            items: Vec::new(),
            error: None,
        })
    }

    fn ports(id: Ulid) -> Event {
        Event::SessionPorts(SessionPortsEvent {
            session_id: id,
            ports: vec![8080],
        })
    }

    fn comment(project_id: Ulid, context: CommentContext) -> Event {
        Event::CommentUpdated(Box::new(DocComment {
            id: CommentId(Ulid::new()),
            project_id,
            file_path: "a.rs".into(),
            context,
            parent_id: None,
            author: DocComment::USER.into(),
            body: "x".into(),
            start_line: Some(1),
            end_line: Some(1),
            quote: None,
            status: CommentStatus::Open,
            sent_to: None,
            sent_snapshot: None,
            created_at: 0,
            updated_at: 0,
        }))
    }

    fn redraws(w: &World, shown: &Shown, event: &Event) -> bool {
        event_needs_redraw(event, shown, &w.sessions)
    }

    /// A visible pane shows its context share; a session of a hidden tab or another workspace
    /// shows nothing. The Session tab and the Board are the other two readers.
    #[test]
    fn usage_redraws_for_a_visible_pane_the_session_tab_or_the_board() {
        let w = world();
        let mut s = terminals(&w);
        assert!(redraws(&w, &s, &usage(w.a())), "visible pane");
        assert!(!redraws(&w, &s, &usage(w.b())), "hidden tab");
        assert!(!redraws(&w, &s, &usage(w.c())), "other workspace");
        s.panes.push(w.b());
        assert!(redraws(&w, &s, &usage(w.b())), "split pane");

        let mut s = nothing_shown(&w);
        assert!(!redraws(&w, &s, &usage(w.a())), "Editor draws no usage");
        s.panel = Some(PanelTab::Session);
        assert!(redraws(&w, &s, &usage(w.a())), "Session tab of the panel session");
        assert!(!redraws(&w, &s, &usage(w.b())), "Session tab shows one session");
        s.panel = Some(PanelTab::Git);
        assert!(!redraws(&w, &s, &usage(w.a())), "Git tab draws no usage");

        s.panel = None;
        s.view = View::Board;
        assert!(redraws(&w, &s, &usage(w.b())), "card of the shown workspace");
        assert!(!redraws(&w, &s, &usage(w.c())), "another workspace's board is not drawn");
    }

    #[test]
    fn turns_and_queues_need_a_visible_pane() {
        let w = world();
        let s = terminals(&w);
        for event in [turn(w.a()), queue(w.a())] {
            assert!(redraws(&w, &s, &event), "{event:?}");
        }
        for event in [turn(w.b()), queue(w.b()), turn(w.c()), queue(w.c())] {
            assert!(!redraws(&w, &s, &event), "{event:?}");
        }
    }

    /// The plan is drawn for one session: in the Session tab, or as the focused row's bar of
    /// the full sidebar (the rail has no bar).
    #[test]
    fn plan_needs_the_session_tab_or_the_focused_row_of_the_full_sidebar() {
        let w = world();
        let mut s = terminals(&w);
        assert!(!redraws(&w, &s, &plan(w.a())), "rail, panel closed");
        s.panel = Some(PanelTab::Session);
        assert!(redraws(&w, &s, &plan(w.a())));
        assert!(!redraws(&w, &s, &plan(w.b())), "not the panel session");
        s.panel = Some(PanelTab::Files);
        assert!(!redraws(&w, &s, &plan(w.a())), "another tab");
        s.sidebar = SidebarMode::Full;
        assert!(redraws(&w, &s, &plan(w.a())), "the selected row's bar");
        assert!(!redraws(&w, &s, &plan(w.b())), "an unselected row has no bar");
    }

    /// Ports are in the Run pill of the shown workspace (every view) and in the RUN group of
    /// the full sidebar (every open workspace); the rail draws neither.
    #[test]
    fn ports_show_in_the_run_pill_and_the_full_sidebars_run_group() {
        let w = world();
        let mut s = nothing_shown(&w);
        assert!(redraws(&w, &s, &ports(w.b())), "Run pill of the shown workspace");
        assert!(!redraws(&w, &s, &ports(w.c())), "other workspace, no sidebar rows");
        s.sidebar = SidebarMode::Compact;
        assert!(!redraws(&w, &s, &ports(w.c())), "the rail has no RUN group");
        s.sidebar = SidebarMode::Full;
        assert!(redraws(&w, &s, &ports(w.c())), "its workspace's RUN group");
        assert!(redraws(&w, &nothing_shown(&w), &ports(Ulid::new())), "unknown session");
    }

    /// The footer's Inbox count, the sidebar dots and `Viewed` depend on these: never skipped.
    #[test]
    fn state_changes_redraw_even_when_nothing_else_is_shown() {
        let w = world();
        let s = nothing_shown(&w);
        let state = Event::SessionState(SessionStateEvent {
            session_id: w.c(),
            state: SessionState::NeedsInput,
            source: StateSource::Hook,
            changed_at: 1,
        });
        let exited = Event::SessionExited(SessionExitedEvent {
            session_id: w.c(),
            exit_code: Some(0),
        });
        assert!(redraws(&w, &s, &state));
        assert!(redraws(&w, &s, &exited));
    }

    /// Dialogs read models we did not audit (compare, history, task editor): while one is open
    /// every change redraws.
    #[test]
    fn an_open_dialog_keeps_every_change_redrawing() {
        let w = world();
        let mut s = nothing_shown(&w);
        assert!(!redraws(&w, &s, &usage(w.c())));
        s.modal = true;
        assert!(redraws(&w, &s, &usage(w.c())));
        assert!(conflicts_need_redraw(&s));
    }

    /// Tasks are read by the Board, the pane folder and the worktree chooser of the Files and
    /// Git tabs: not by the Session tab, the Editor or the diff themselves.
    #[test]
    fn task_changes_redraw_the_views_that_read_tasks() {
        let w = world();
        let event = |project_id| {
            Event::TaskDeleted(TaskDeletedEvent {
                task_id: Ulid::new(),
                project_id,
            })
        };
        let mut s = terminals(&w);
        for view in [View::Board, View::Terminals] {
            s.view = view;
            assert!(redraws(&w, &s, &event(w.ws)), "{view:?}");
            assert!(!redraws(&w, &s, &event(w.other)), "{view:?}: another workspace");
        }
        s.view = View::Editor;
        assert!(!redraws(&w, &s, &event(w.ws)), "Editor, no panel");
        s.panel = Some(PanelTab::Session);
        assert!(!redraws(&w, &s, &event(w.ws)), "Session tab");
        for tab in [PanelTab::Files, PanelTab::Git] {
            s.panel = Some(tab);
            assert!(redraws(&w, &s, &event(w.ws)), "{tab:?} tab chooser");
        }
        s.view = View::Docs;
        s.panel = None;
        assert!(!redraws(&w, &s, &event(w.ws)));
    }

    #[test]
    fn conflicts_show_on_the_board_and_in_pane_headers_only() {
        let w = world();
        let detected = Event::ConflictDetected(ConflictDetectedEvent {
            project_id: w.ws,
            path: "src/a.rs".into(),
            session_ids: vec![w.a(), w.b()],
            task_ids: Vec::new(),
        });
        let mut s = terminals(&w);
        for view in [View::Board, View::Terminals] {
            s.view = view;
            assert!(conflicts_need_redraw(&s) && redraws(&w, &s, &detected), "{view:?}");
        }
        for view in [View::Docs, View::Editor, View::Diff] {
            s.view = view;
            assert!(!conflicts_need_redraw(&s) && !redraws(&w, &s, &detected), "{view:?}");
        }
    }

    /// Diff comments are drawn in the diff view, doc comments in Docs, both of the shown
    /// workspace; an AI answer goes to the Git tab's commit box or a pane composer.
    #[test]
    fn comments_and_ai_answers_follow_the_view_that_draws_them() {
        let w = world();
        let mut s = nothing_shown(&w);
        s.view = View::Diff;
        assert!(redraws(&w, &s, &comment(w.ws, CommentContext::Diff)));
        assert!(!redraws(&w, &s, &comment(w.other, CommentContext::Diff)), "other workspace");
        assert!(!redraws(&w, &s, &comment(w.ws, CommentContext::Doc)), "Doc comment in Diff");
        s.view = View::Docs;
        assert!(redraws(&w, &s, &comment(w.ws, CommentContext::Doc)));
        assert!(!redraws(&w, &s, &comment(w.ws, CommentContext::Diff)));
        s.view = View::Terminals;
        assert!(!redraws(&w, &s, &comment(w.ws, CommentContext::Diff)));

        let ai = Event::AiGenerated(AiGeneratedEvent {
            generation_id: Ulid::new(),
            project_id: Some(w.ws),
            text: Some("x".into()),
            error: None,
        });
        assert!(redraws(&w, &s, &ai), "pane composer");
        s.view = View::Editor;
        assert!(!redraws(&w, &s, &ai));
        s.panel = Some(PanelTab::Git);
        assert!(redraws(&w, &s, &ai), "commit box in the Git tab");
    }
}
```

- [ ] **Step 2: Run it, expect FAIL**

Run: `cargo test -p mkrew-ui --lib redraw -j 4`
Expected: FAIL to compile (`Shown`, `SidebarMode`, `PanelTab`, `View`, `Ulid`, `Session`, `CommentContext`, `event_needs_redraw`, `conflicts_need_redraw` are not defined in `redraw`).

- [ ] **Step 3: Implement**

3a. Put this above the test module in `redraw.rs`:

```rust
//! Which daemon events make the root `Shell` render again. A model (usage, turns, plan, ports,
//! queue, tasks, conflicts) always keeps its data; the root only redraws when the window draws
//! that data now. Anything that changes what is on screen (a view, workspace, tab or panel
//! switch, a dialog, window visibility) re-renders on its own, so data skipped while hidden is
//! read fresh when it is shown. The sidebar view observes the Shell, so a skipped notify skips
//! it too. Pure and tested headlessly; `Shell::shown` builds [`Shown`].

use mkrew_core::settings::SidebarMode;
use mkrew_protocol::{CommentContext, Event, Session};
use ulid::Ulid;

use crate::board::View;
use crate::inspector::PanelTab;

/// What the window draws now.
#[derive(Debug, PartialEq, Eq)]
pub struct Shown {
    pub workspace: Option<Ulid>,
    pub view: View,
    /// Sessions whose pane is drawn: the active tab's visible panes in the Terminals view,
    /// none while the window is hidden.
    pub panes: Vec<Ulid>,
    /// The pane keyboard input goes to (the full sidebar draws its plan bar).
    pub focused: Option<Ulid>,
    /// The right panel's tab while the panel is drawn.
    pub panel: Option<PanelTab>,
    /// The session the panel's Session tab describes.
    pub panel_session: Option<Ulid>,
    pub sidebar: SidebarMode,
    /// Some dialog is open.
    pub modal: bool,
}

/// Whether an event whose model reported "my data changed" must re-render the root. Events
/// owned by another handler (sidebar state, tabs, docs, git) answer `true`: they are never
/// decided here, and their handlers keep their own redraw rules.
pub fn event_needs_redraw(event: &Event, shown: &Shown, sessions: &[Session]) -> bool {
    // Dialogs read models of their own (compare, history, task editor, Ask AI).
    if shown.modal {
        return true;
    }
    let in_workspace = |project_id: Ulid| shown.workspace == Some(project_id);
    // A session not listed yet cannot be placed: redraw.
    let in_shown_workspace = |session_id: Ulid| {
        sessions
            .iter()
            .find(|s| s.id == session_id)
            .is_none_or(|s| in_workspace(s.project_id))
    };
    let on_pane = |id: Ulid| shown.panes.contains(&id);
    let in_session_tab =
        |id: Ulid| shown.panel == Some(PanelTab::Session) && shown.panel_session == Some(id);
    match event {
        // Pane status, the Session tab card, Board cards (their task's tokens).
        Event::SessionUsage(e) => {
            on_pane(e.session_id)
                || in_session_tab(e.session_id)
                || (shown.view == View::Board && in_shown_workspace(e.session_id))
        }
        // Pane status pill, `⧗ N queued` and the composer's queue.
        Event::SessionTurn(e) => on_pane(e.session_id),
        Event::SessionQueue(e) => on_pane(e.session_id),
        // Session tab Plan card, or the focused row's bar in the full sidebar.
        Event::SessionPlan(e) => {
            in_session_tab(e.session_id)
                || (shown.sidebar == SidebarMode::Full && shown.focused == Some(e.session_id))
        }
        // Pane titles and the Run pill (the shown workspace, every view), the RUN group of
        // every open workspace in the full sidebar.
        Event::SessionPorts(e) => {
            in_shown_workspace(e.session_id) || shown.sidebar == SidebarMode::Full
        }
        Event::TaskUpdated(task) => tasks_drawn(shown, task.project_id),
        Event::TaskDeleted(deleted) => tasks_drawn(shown, deleted.project_id),
        Event::ConflictDetected(_) => conflicts_need_redraw(shown),
        // Diff comments are drawn in the diff view, doc comments in Docs.
        Event::CommentUpdated(comment) => {
            in_workspace(comment.project_id)
                && matches!(
                    (shown.view, comment.context),
                    (View::Diff, CommentContext::Diff) | (View::Docs, CommentContext::Doc)
                )
        }
        // The Git tab's commit message box and the pane composer.
        Event::AiGenerated(_) => {
            shown.view == View::Terminals || shown.panel == Some(PanelTab::Git)
        }
        Event::SessionState(_)
        | Event::SessionExited(_)
        | Event::SessionUpdated(_)
        | Event::SessionTerminalTitle(_)
        | Event::ProjectUpdated(_)
        | Event::ProjectCloned(_)
        | Event::TabUpdated(_)
        | Event::SessionSnapshot(_)
        | Event::NotificationClicked(_)
        | Event::SessionEdited(_)
        | Event::DocChanged(_)
        | Event::GitChanged(_) => true,
    }
}

/// Conflict warnings are drawn on Board cards and in pane status. Also asked after
/// `Conflicts::prune`, which can clear a warning on any event.
pub fn conflicts_need_redraw(shown: &Shown) -> bool {
    shown.modal || matches!(shown.view, View::Board | View::Terminals)
}

/// Tasks are read by the Board, the pane status folder and the worktree chooser of the
/// Files and Git tabs of the shown workspace.
fn tasks_drawn(shown: &Shown, project_id: Ulid) -> bool {
    shown.workspace == Some(project_id)
        && (matches!(shown.view, View::Board | View::Terminals)
            || matches!(shown.panel, Some(PanelTab::Files | PanelTab::Git)))
}
```

3b. `main.rs`: after `use mkrew_ui::quick_open::{self, QuickOpen, Results};` (line 92) add `use mkrew_ui::redraw;`.

3c. `shell/connection.rs`: add both methods right before `/// Routes a batch of incoming frames; returns whether a pane got new content.` (line 804). The shell modules get `View`, `on_screen` and `redraw` through `use crate::*`.

```rust
    /// What the window draws now, for [`redraw::event_needs_redraw`].
    pub(crate) fn shown(&self, window: &Window) -> redraw::Shown {
        let view = self.view();
        let tabs = (self.workspace)
            .filter(|_| view == View::Terminals)
            .and_then(|id| self.tabs.get(&id));
        redraw::Shown {
            workspace: self.workspace,
            view,
            panes: on_screen(window.is_visible(), tabs),
            focused: self.focused_session(),
            panel: self.panel_shown().then(|| self.panel_tab()),
            panel_session: self.panel_session(),
            sidebar: self.sidebar_layout.mode(),
            modal: self.modal_open(),
        }
    }

    /// [`Self::shown`], built at most once per batch: an output-only batch (up to ~60/s) never
    /// asks, so it allocates nothing. A batch that switches workspace or view (a notification
    /// click) notifies on its own and the render after it reads every model, so a stale value
    /// can only cost one redraw, never a stale screen.
    fn batch_shown<'a>(
        &self,
        slot: &'a mut Option<redraw::Shown>,
        window: &Window,
    ) -> &'a redraw::Shown {
        slot.get_or_insert_with(|| self.shown(window))
    }
```

3d. In `dispatch`, after `let mut peek_changed = false;` (line 813) add `let mut shown_now: Option<redraw::Shown> = None;`, then replace lines 816-834

```rust
                if self.board.apply(event) {
                    self.drop_gone_targets(window, cx);
                    cx.notify();
                }
                if self.turns.apply(event) {
                    cx.notify();
                }
                if self.usages.apply(event) {
                    cx.notify();
                }
                if self.plans.apply(event) {
                    cx.notify();
                }
                if self.ports.apply(event) {
                    cx.notify();
                }
                if self.composers.apply(event) {
                    cx.notify();
                }
```
with
```rust
                // Every model keeps its data; the root only redraws what the window draws.
                // `|=` (not `||`): each model must see the event.
                let mut data_changed = false;
                if self.board.apply(event) {
                    self.drop_gone_targets(window, cx);
                    data_changed = true;
                }
                data_changed |= self.turns.apply(event);
                data_changed |= self.usages.apply(event);
                data_changed |= self.plans.apply(event);
                data_changed |= self.ports.apply(event);
                data_changed |= self.composers.apply(event);
                if data_changed {
                    let shown = self.batch_shown(&mut shown_now, window);
                    if redraw::event_needs_redraw(event, shown, self.sidebar.sessions()) {
                        cx.notify();
                    }
                }
```

Replace lines 862-865
```rust
                let detected = self.conflicts.apply(event);
                if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected {
                    cx.notify();
                }
```
with
```rust
                let detected = self.conflicts.apply(event);
                // `prune` runs on every event; only its notify is gated.
                let pruned = self.conflicts.prune(&self.board, self.sidebar.sessions());
                if (pruned || detected)
                    && redraw::conflicts_need_redraw(self.batch_shown(&mut shown_now, window))
                {
                    cx.notify();
                }
```

In the `CommentUpdated` arm replace line 889 `cx.notify();` (after the `for view in self.git.values_mut()` loop) with
```rust
                    let shown = self.batch_shown(&mut shown_now, window);
                    if redraw::event_needs_redraw(event, shown, self.sidebar.sessions()) {
                        cx.notify();
                    }
```
and in the `AiGenerated` arm replace line 910 `cx.notify();` (after `self.composers.enhanced(..)`) with the same three lines.

Do not touch the `sidebar.apply` match, `layout_changed`, `peek` handling, the docs / git handlers or the `load_*` notifies.

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors. If the borrow checker objects to `shown` (it borrows only the local `shown_now`, so it should not), clone the three-line block into a `let needs = ...;` before `cx.notify()`.

- [ ] **Step 4: Run tests, expect PASS**

Run: `cargo test -p mkrew-ui --lib redraw -j 4`
Expected: PASS (9 tests).

- [ ] **Step 5: SPEC / ADR / HUMAN_CHECKS**

SPEC: no change (no observable behavior: SPEC §10/§11 say what is drawn, not when the root re-renders; §15 Idle CPU row stays). ADR: none.

`docs/HUMAN_CHECKS.md`: if the section does not exist yet (`grep -n "perf (plan docs/superpowers/plans/2026-10-28" docs/HUMAN_CHECKS.md`), create it directly above the first existing `### To verify` heading (line 24 on 2026-10-28; above any other plan's new section) with the heading below; the other tasks append their bullets under it. Heading text is the shared contract's; the file's other headings currently use ` - ` instead of the em dash, so match whichever form the first task to run used.

```markdown
### To verify - 2026-10-28 perf (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)

- [ ] **Events only redraw what is shown (Task 2):** 2 workspaces, an agent in each, right panel on the Session tab (`[features] session` on) for workspace A's focused agent, full sidebar.
  - Let workspace B's agent finish turns: A's panes, `ctx` and the Session tab do not flicker, and still update at once when A's own agent ends a turn (tokens and `ctx` change).
  - Open the Board: a card's token total updates live when its agent ends a turn. Switch to Files, let the agent end another turn, come back to Board: the total is current.
  - Link a plan (`.md` checklist) to the focused agent: the Plan card and the selected sidebar row's bar update while you tick items; switch the panel to Files, tick more, switch back to Session: current.
  - Start a run command that prints a port: the title bar Run pill shows `:PORT` in every view (also Editor); the full sidebar's RUN group of the other expanded workspace shows its ports too.
  - Queue a send to a busy agent (`⧗ 1 queued` in its pane header), then split so the pane is hidden in another tab: switch back, the count is current.
  - Git tab: Generate a commit message while on the Git tab: the text appears without touching the mouse. Diff view: an agent reply to a diff comment shows up at once.
  - The footer `N need you` count, the sidebar dots and the OS notifications still follow every agent, also with the window on another workspace, on the Editor and with the compact rail. Focus a `done` pane: it leaves the count at once.
- [ ] **CPU (Task 2):** Task Manager > Details > `mkrew.exe`, 3 agents in `working` (long tasks), window visible on Terminals, read CPU over 60 s. Before: build of the commit before this plan (copy its `target\release\mkrew.exe` aside first). After: this task's build. Before ___ % / after ___ % of one core. (The drop is the avoided renders from other sessions' and hidden panes' events; the 500 ms working-dot tick and output frames are other tasks.)
```

If the section already exists, append only the two bullets.

- [ ] **Step 6: Verify and commit**

Run: `cargo test -p mkrew-ui --lib redraw -j 4` (expect PASS), then `sh scripts/verify.sh`. If rustc crashes on mkrew-ui test builds, run in order: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`.

Commit (never push; `shell/connection.rs` and `docs/HUMAN_CHECKS.md` are shared with other tasks and agents, stage only this task's hunks with `git add -p`):
```
git add crates/mkrew-ui/src/redraw.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/main.rs
git add -p crates/mkrew-ui/src/shell/connection.rs docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "perf(ui): daemon events redraw the root only when the data is on screen"
```

**Review focus for this task:** (1) a Session-tab user on the Editor or Diff view (no pane drawn, `panel_session` still the focused pane) must still see usage and plan update: `usage_redraws_for_a_visible_pane_the_session_tab_or_the_board` and `plan_needs_the_session_tab_or_the_focused_row_of_the_full_sidebar`; (2) a run's port printed in another workspace must reach the Run pill of the shown workspace in every view and the full sidebar's RUN group, but not the rail: `ports_show_in_the_run_pill_and_the_full_sidebars_run_group`. Known and accepted: a plan change for a session that only has the sidebar keyboard highlight (Alt+0) and not the focus shows its bar on the next key press.


### Task 3: Watcher: 5 s batch cap while an agent works, release kept watchers
**Covers:** audit item "watcher: git.changed only for real changes, 5 s cap while working, release kept watchers" (old plan Task 8, bullet B5).
**Current state (verified 2026-10-28):**
- "git.changed only for real changes" is already true and needs no code.
  `debounce` delivers nothing for an empty batch (`crates/mkrew-daemon/src/watch.rs:272` `if !batch.is_empty() || git`, since c88ef49), after `Filter::relative_with` has dropped `.git`, `node_modules`, `target`, `.worktrees` and git-ignored paths (`watch.rs:147-179`) and `is_git_state` has kept only `HEAD`, `index`, `packed-refs`, `refs/*` (`watch.rs:184-202`).
  The existing test `debounce_batches_until_quiet_and_caps_the_delay` already pins "only ignored paths: no batch" (`watch.rs:560-578`).
  The daemon's own `git status` does not touch the index (`GIT_OPTIONAL_LOCKS=0`, `crates/mkrew-daemon/src/git.rs:905`), so no `git.changed` loop.
  The send in `Hub::watch` (`session.rs:689-692`) stays unconditional on purpose.
- The cadence is the real cost: `MAX_DELAY` is 2 s (`watch.rs:18`), read in `debounce` at `watch.rs:263`.
  An agent that writes files all the time makes one batch every 2 s, and every batch makes the UI run `git.status` for the title-bar summary (`shell/connection.rs:891-899`) plus the Files tree and Git list when on screen (`files_event`, `git_event`).
- `Hub` has NO working-state map today (grep for `working`/`has_working`/`set_working` in `crates/mkrew-daemon/src/session.rs` finds only the `files_changed` local at :742 and `SessionState::Working` uses).
  `Actor::set_state` (`session.rs:1356-1372`) is the single place a session's state is assigned (`finish` goes through it at :1537).
  `awake.rs` keeps its own set from `Event::SessionState`; it is not shared.
- Kept watchers are never cleared: `Hub::watch` (`session.rs:667-706`) stores `ProjectWatch { _watch, kept }` (struct :189) in `Hub::watches`.
  `kept` is only ever OR-ed to true (`watch.kept |= kept`, :677) by `keep_watched` (:639; callers `server/doc.rs:87` `watched_project` for `doc.list`/`doc.read`, `server/target.rs:134` `watched_target` for every `fs.*` / `git.*` request) and `keep_worktree_watched` (:647, `server/target.rs:137`).
  Only `unwatch_worktree` (:653, task worktree removal, `server/task_worktree.rs:90`) and `unwatch_idle_project` (:710, called from `finish` :1513 and a failed spawn :369, only when not kept) drop a watch.
- The UI has no message for "a view closed" (`Request` has no `watch.*`; `ProjectRef { project_id }`, `crates/mkrew-protocol/src/files.rs:14`, cannot name a worktree).
  PROTOCOL_VERSION is 48 (`crates/mkrew-protocol/src/lib.rs:16`).
- Files and Git live in the right panel now (ADR 0081, 0084), so "view closes" has more shapes than in the old plan.
  On screen = `Shell::files_on_screen()` (Editor view or file tree tab, `shell/focus.rs:149`) and `Shell::git_shown()` (Diff view or Git tab, `shell/focus.rs:160`), both of the active workspace only.
  What changes them: `activate_workspace` (`shell/workspace.rs:21`), `show_view` (`workspace.rs:368`), `panel_changed` (`shell/inspector.rs:77`, reached from `toggle_inspector` and `show_panel_tab`), and the worktree target of each view: `set_files_target` (`shell/files.rs:26`, ends in `refresh_files`) and `retarget_git` (`shell/git.rs:140`, every caller then goes through `refresh_git`, `open_git` or `show_view`; `drop_gone_targets` `files.rs:838` too).
  Docs (`doc.list`, `shell/docs.rs:25`) only keeps the project folder, which the shown workspace needs anyway.
- Clients: the phone relay (`mk web`) and `mk` never send `fs.*`, `git.*` or `doc.*` (grep of `crates/mkrew-cli/src` finds none), so the UI is the only client that keeps watchers.
- Requests of one connection are handled one after the other, in call order (`server.rs:203` `self.handle(..).await`; `Connection::request` "requests go out in call order", `crates/mkrew-ui/src/client.rs:166`).
**Files:**
- Modify: `crates/mkrew-daemon/src/watch.rs` (`MAX_DELAY_WORKING`, `watch`, `debounce`, tests at :527, :614, :647)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Hub` field + `new`, `release_watch`, `set_working`, `has_working`, `Hub::watch`, `Actor::set_state`, three doc comments, tests)
- Modify: `crates/mkrew-daemon/src/server.rs` (dispatch arm after :447), `crates/mkrew-daemon/src/server/target.rs` (`watch_release`)
- Modify: `crates/mkrew-daemon/tests/daemon/docs.rs:151-152` (comment only)
- Modify: `crates/mkrew-protocol/src/files.rs` (`WatchReleaseParams`), `crates/mkrew-protocol/src/message.rs` (import :19-24, `Request::WatchRelease`, `GitChanged` doc :389), `crates/mkrew-protocol/src/message/tests.rs`, `crates/mkrew-protocol/src/lib.rs:16`
- Create: `crates/mkrew-ui/src/watches.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod watches;` between `voice` :96 and `workspace_menu` :97), `crates/mkrew-ui/src/main.rs` (`use mkrew_ui::watches;` after :117, `Shell.watch_needs` after :647), `crates/mkrew-ui/src/shell/connection.rs` (initializer after :323), `crates/mkrew-ui/src/shell/workspace.rs` (`sync_watches`, two call sites), `crates/mkrew-ui/src/shell/inspector.rs` (`panel_changed`), `crates/mkrew-ui/src/shell/files.rs` (`refresh_files`), `crates/mkrew-ui/src/shell/git.rs` (`refresh_git`)
- Create: `docs/decisions/0087-release-kept-watchers.md`
- Modify: `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`
**Interfaces:**
- Produces: `watch::MAX_DELAY_WORKING: Duration` (5 s); `watch::watch(root: &Path, working: impl Fn() -> bool + Send + 'static, on_batch: impl Fn(Batch) + Send + 'static) -> notify::Result<Watch>`; `working()` is asked when a batch starts and picks the cap.
- Produces: `Hub::release_watch(&self, project_id: Ulid, worktree_task_id: Option<Ulid>)` (pub), `Hub::set_working(&self, id: Ulid, project_id: Ulid, working: bool)` and `Hub::has_working(&self, project_id: Ulid) -> bool` (private).
  The map holds AGENT sessions only (same rule as `awake.rs`): a `shell`-kind run session such as a dev server is often `working` all day and must not slow the Files tree.
- Produces: `mkrew_protocol::WatchReleaseParams { project_id: Ulid, worktree_task_id: Option<Ulid> }` and `Request::WatchRelease(WatchReleaseParams)` (wire `watch.release`, result `null`). `PROTOCOL_VERSION` = 49.
- Produces (UI): `watches::Needs::of(workspace: Option<Ulid>, files: Option<Ulid>, git: Option<Ulid>) -> Needs`, `Needs::released(&self, now: &Needs) -> Vec<WatchReleaseParams>`, `Shell::sync_watches(&mut self, cx: &mut Context<'_, Self>)`.
- Consumes: nothing from the other tasks of this plan.

- [ ] **Step 1: Write the failing tests**

`crates/mkrew-daemon/src/watch.rs`, in `mod tests` after `debounce_batches_until_quiet_and_caps_the_delay` (ends :579), add:

```rust
    /// A stream of changes (one every 100 ms, never quiet) is cut 5 s after its first change
    /// while a session of the project is working (perf B5).
    #[tokio::test(start_paused = true)]
    async fn a_working_session_stretches_the_cap_to_five_seconds() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, mut out) = mpsc::unbounded_channel();
        tokio::spawn(debounce(Filter::new(&root), rx, || true, move |_: Batch| {
            let _ = out_tx.send(tokio::time::Instant::now());
        }));
        let start = tokio::time::Instant::now();
        for i in 0..100 {
            tx.send(vec![root.join(format!("f{i}"))]).unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(at) = out.try_recv() {
                assert_eq!(at - start, MAX_DELAY_WORKING);
                return;
            }
        }
        panic!("no batch after 10 s of changes");
    }
```

Update the three existing call sites in the same module:
- `debounce_batches_until_quiet_and_caps_the_delay` (:533): `debounce(Filter::new(&root), rx, move |batch: Batch| {` becomes `debounce(Filter::new(&root), rx, || false, move |batch: Batch| {` (the 2 s cap stays pinned by its `assert_eq!(at - start, MAX_DELAY)` at :552).
- `debounce_flags_git_state_changes_without_files` (:620): `debounce(Filter::new(&root), rx, move |batch| {` becomes `debounce(Filter::new(&root), rx, || false, move |batch| {`.
- `watcher_reports_project_changes_but_not_ignored_ones` (:657): `watch(&root, move |batch| {` becomes `watch(&root, || false, move |batch| {`.

`crates/mkrew-daemon/src/session.rs` `mod tests` (starts :1617; last test `a_session_floods_past_the_bytes_of_one_window` at :1692), add at the end:

```rust
    fn hub() -> (tempfile::TempDir, Arc<Hub>, Ulid) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project = store.add_project(dir.path(), Some("repo")).unwrap();
        (dir, Arc::new(Hub::new(store)), project.id)
    }

    fn watched(hub: &Hub, key: Ulid) -> bool {
        lock(&hub.watches).contains_key(&key)
    }

    /// A session of the project registered as live (no process behind it).
    fn live(hub: &Hub, project_id: Ulid) -> Ulid {
        let id = Ulid::new();
        let (commands, _rx) = mpsc::channel(8);
        let (input, _input_rx) = mpsc::channel(1);
        lock(&hub.live).insert(
            id,
            Live {
                project_id,
                commands,
                input: Arc::new(tokio::sync::Mutex::new(input)),
            },
        );
        id
    }

    /// The view that kept a project watched closes: the watch stops at once, except while a
    /// session of the project is live (its exit drops it then, as it always did).
    #[tokio::test]
    async fn releasing_a_kept_project_watch_drops_it_unless_a_session_lives() {
        let (dir, hub, project) = hub();
        hub.keep_watched(project, dir.path());
        assert!(watched(&hub, project));
        hub.release_watch(project, None);
        assert!(!watched(&hub, project), "released and nothing runs");

        hub.keep_watched(project, dir.path());
        let session = live(&hub, project);
        hub.release_watch(project, None);
        assert!(watched(&hub, project), "a live session still needs it");
        lock(&hub.live).remove(&session);
        hub.unwatch_idle_project(project);
        assert!(!watched(&hub, project), "no longer kept: its last exit drops it");
    }

    /// A task worktree's watcher only exists for its Files / Git view: release drops it, and
    /// releasing something never watched is a no-op.
    #[tokio::test]
    async fn releasing_a_worktree_watch_drops_it() {
        let (dir, hub, project) = hub();
        let task = Ulid::new();
        hub.keep_worktree_watched(project, task, dir.path());
        assert!(watched(&hub, task));
        hub.release_watch(project, Some(task));
        assert!(!watched(&hub, task));
        hub.release_watch(project, Some(Ulid::new()));
        hub.release_watch(Ulid::new(), None);
    }

    /// `has_working` is per project; two working sessions of one project count until both stop.
    #[test]
    fn has_working_follows_the_sessions_of_each_project() {
        let (_dir, hub, project) = hub();
        let (a, b, other) = (Ulid::new(), Ulid::new(), Ulid::new());
        assert!(!hub.has_working(project));
        hub.set_working(a, project, true);
        hub.set_working(b, project, true);
        assert!(hub.has_working(project));
        assert!(!hub.has_working(other));
        hub.set_working(a, project, false);
        assert!(hub.has_working(project), "b still works");
        hub.set_working(b, project, false);
        assert!(!hub.has_working(project));
    }
```

(`hub()` is a plain `Hub::new`, no runtime needed; the two tests that start a real watcher use `#[tokio::test]` because `watch::watch` spawns its debounce task. `Live` has exactly the three fields `project_id`, `commands`, `input`, session.rs:195-201.)

`crates/mkrew-protocol/src/message/tests.rs`: in `every_m1_request_round_trips`, after `Request::SessionPorts(ProjectRef { project_id: sid() }),` (:250) add:

```rust
        Request::WatchRelease(WatchReleaseParams {
            project_id: sid(),
            worktree_task_id: Some(sid()),
        }),
```

and after `param_less_requests_omit_params` (ends :76) add:

```rust
/// `worktree_task_id` may be left out: it names the project's own watcher.
#[test]
fn watch_release_without_a_worktree_means_the_project_folder() {
    let wire = json!({
        "type": "request", "id": 3, "method": "watch.release",
        "params": {"project_id": sid().to_string()}
    });
    let Message::Request { request, .. } = serde_json::from_value(wire).unwrap() else {
        panic!("a request");
    };
    assert_eq!(
        request,
        Request::WatchRelease(WatchReleaseParams {
            project_id: sid(),
            worktree_task_id: None,
        })
    );
}
```

(`mod tests` is `use super::*;` of `message.rs`, so `WatchReleaseParams` arrives through the import added in Step 3.)

The UI tests are the `mod tests` block of the new `crates/mkrew-ui/src/watches.rs` (full file in Step 3).

- [ ] **Step 2: Run them, expect FAIL**

Run: `cargo test -p mkrew-daemon --lib watch -j 4` and `cargo test -p mkrew-protocol -j 4`
Expected: FAIL to compile: `debounce` / `watch` take no `working` argument, `MAX_DELAY_WORKING`, `Hub::release_watch`, `set_working`, `has_working`, `WatchReleaseParams`, `Request::WatchRelease` do not exist.

- [ ] **Step 3: Implement**

**3a. `crates/mkrew-daemon/src/watch.rs`.**
After `pub const MAX_DELAY: Duration = Duration::from_secs(2);` (:18) add:

```rust
/// [`MAX_DELAY`] while a session of the project is working: an agent writes files all the
/// time, and every batch makes each UI client run `git status` (perf B5).
pub const MAX_DELAY_WORKING: Duration = Duration::from_secs(5);
```

Replace the tail of the `watch` doc comment and its signature (:222-224):

```rust
/// reported too (T5.6 attributes such paths like files). Non-UTF-8 paths are skipped. Must run
/// inside the tokio runtime.
pub fn watch(root: &Path, on_batch: impl Fn(Batch) + Send + 'static) -> ::notify::Result<Watch> {
```

with

```rust
/// reported too (T5.6 attributes such paths like files). Non-UTF-8 paths are skipped.
/// `working` is asked when a batch starts: while it is true the batch may wait
/// [`MAX_DELAY_WORKING`] instead of [`MAX_DELAY`]. A batch with nothing to report after
/// filtering is never delivered. Must run inside the tokio runtime.
pub fn watch(
    root: &Path,
    working: impl Fn() -> bool + Send + 'static,
    on_batch: impl Fn(Batch) + Send + 'static,
) -> ::notify::Result<Watch> {
```

Replace `tokio::spawn(debounce(Filter::new(root), rx, on_batch));` (:236) with `tokio::spawn(debounce(Filter::new(root), rx, working, on_batch));`.

Replace the `debounce` doc comment and signature (:240-246):

```rust
/// Collects paths until [`QUIET`] passes without a change (or [`MAX_DELAY`] since the first
/// one), then delivers the batch. Ends when the watcher is dropped.
async fn debounce(
    filter: Filter,
    mut rx: mpsc::UnboundedReceiver<Vec<PathBuf>>,
    on_batch: impl Fn(Batch),
) {
```

with

```rust
/// Collects paths until [`QUIET`] passes without a change (or [`MAX_DELAY`], or
/// [`MAX_DELAY_WORKING`] when `working()` says so at the first change, since the first one),
/// then delivers the batch. Ends when the watcher is dropped.
async fn debounce(
    filter: Filter,
    mut rx: mpsc::UnboundedReceiver<Vec<PathBuf>>,
    working: impl Fn() -> bool,
    on_batch: impl Fn(Batch),
) {
```

and replace `let deadline = tokio::time::Instant::now() + MAX_DELAY;` (:263) with:

```rust
        let cap = if working() { MAX_DELAY_WORKING } else { MAX_DELAY };
        let deadline = tokio::time::Instant::now() + cap;
```

**3b. `crates/mkrew-daemon/src/session.rs`.**
`Hub` struct: after `watches: Mutex<HashMap<Ulid, ProjectWatch>>,` (:79) add:

```rust
    /// Agent sessions in `working` and their project, kept by `Actor::set_state`: the
    /// watcher's batch cap reads it instead of the store. Never held with another lock.
    working: Mutex<HashMap<Ulid, Ulid>>,
```

`Hub::new`: after `watches: Mutex::default(),` (:325) add `working: Mutex::default(),`.

After `unwatch_worktree` (ends :656) add:

```rust
    /// A client closed the view that kept the watcher of project `project_id` (or, with
    /// `worktree_task_id`, of that task's worktree). The worktree watch stops at once; the
    /// project's stops unless a session of the project is live, whose exit drops it then (see
    /// [`Hub::unwatch_idle_project`]; ADR 0087).
    pub fn release_watch(&self, project_id: Ulid, worktree_task_id: Option<Ulid>) {
        tracing::debug!(%project_id, ?worktree_task_id, "watch released");
        match worktree_task_id {
            Some(task_id) => self.unwatch_worktree(task_id),
            None => {
                if let Some(watch) = lock(&self.watches).get_mut(&project_id) {
                    watch.kept = false;
                }
                self.unwatch_idle_project(project_id);
            }
        }
    }

    /// Records whether the agent session `id` of `project_id` is `working`.
    fn set_working(&self, id: Ulid, project_id: Ulid, working: bool) {
        let mut sessions = lock(&self.working);
        if working {
            sessions.insert(id, project_id);
        } else {
            sessions.remove(&id);
        }
    }

    /// Is an agent session of the project `working`?
    fn has_working(&self, project_id: Ulid) -> bool {
        lock(&self.working).values().any(|&p| p == project_id)
    }
```

Lock order holds: `release_watch` takes `watches`, drops it at the end of the `if let`, then `unwatch_idle_project` takes `watches` then `live`.
`unwatch_worktree` needs no `kept` change: a worktree watch is only ever kept.

`Hub::watch`: replace (:680-681)

```rust
        let hub = Arc::downgrade(self);
        let started = watch::watch(root, move |batch| {
```

with

```rust
        let hub = Arc::downgrade(self);
        let working_hub = Arc::downgrade(self);
        let working = move || {
            working_hub
                .upgrade()
                .is_some_and(|hub| hub.has_working(project_id))
        };
        let started = watch::watch(root, working, move |batch| {
```

(the closure body, including the unconditional `Event::GitChanged` send, stays as is).

`Actor::set_state` (:1356-1358): after `self.session.state = state;` add

```rust
        self.hub.set_working(
            self.session.id,
            self.session.project_id,
            self.session.kind == SessionKind::Agent && state == SessionState::Working,
        );
```

(A session starts in `Starting`, `server.rs:854`, so the map needs no seeding; `finish` ends in `set_state(Exited)`, which removes the entry.)

Doc comments: `ProjectWatch` (:186-188) `so it stays watched after its last session exits, until the daemon stops (HUMAN_CHECKS T6.4).` becomes `so it stays watched after its last session exits, until a client releases it (`watch.release`, ADR 0087) or the daemon stops (HUMAN_CHECKS T6.4).`
`keep_watched` (:637-638) and `keep_worktree_watched` (:643-646): replace `until the daemon stops` with `until released ([`Hub::release_watch`]) or the daemon stops`.

**3c. Protocol and daemon request.**
`crates/mkrew-protocol/src/files.rs`, after `ProjectRef` (:16) add:

```rust
/// Params of `watch.release`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchReleaseParams {
    pub project_id: Ulid,
    /// The task worktree whose watcher is released; `None` = the project folder.
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
}
```

`crates/mkrew-protocol/src/message.rs`: add `WatchReleaseParams` to the `use crate::files::{..}` list (:23: `GitStatusParams, ProjectRef,` becomes `GitStatusParams, ProjectRef, WatchReleaseParams,`). In `Request`, after the `SessionPorts(ProjectRef),` variant (:277) add:

```rust
    /// Returns nothing (`null`): the client no longer shows what kept the project's file
    /// watcher (or, with `worktree_task_id`, that task worktree's) running. The watcher
    /// stops unless a session of the project is live (SPEC §7, ADR 0087).
    #[serde(rename = "watch.release")]
    WatchRelease(WatchReleaseParams),
```

Change the `GitChanged` doc (:389) from `The git state of a workspace changed (debounced, ≤ once per 300 ms; SPEC §12B).` to `The git state of a workspace changed (debounced: 300 ms quiet, at most 2 s after the first change, 5 s while an agent session of the project is working; SPEC §12B).`

`crates/mkrew-protocol/src/lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 48;` becomes `= 49;`.
Every client and the daemon tests use the constant (`basics.rs:19`, `client.rs`, `relay.rs`); the phone relay greets with the constant of its own build, so it needs no change.

`crates/mkrew-daemon/src/server.rs`, after the `Request::SessionPorts(params) => {..}` arm (:445-447) add:

```rust
            Request::WatchRelease(params) => (self.watch_release(params), After::Continue),
```
`crates/mkrew-daemon/src/server/target.rs`: `use mkrew_protocol::{ErrorCode, Outcome, Project};` (:7) becomes `use mkrew_protocol::{ErrorCode, Outcome, Project, WatchReleaseParams};`; `use super::{Daemon, store_outcome, task_dir};` (:10) becomes `use super::{Daemon, ok, store_outcome, task_dir};`.
At the end of `impl Daemon`, after `watched_target` (:141), add:

```rust
    /// `watch.release`: the client left the view that kept this watcher (ADR 0087).
    pub(super) fn watch_release(&self, params: WatchReleaseParams) -> Outcome {
        self.hub
            .release_watch(params.project_id, params.worktree_task_id);
        ok(&())
    }
```

`crates/mkrew-daemon/tests/daemon/docs.rs:151-152`: replace

```
    // The first spinner lets the project watcher start (it starts after the first spawn); the
    // second keeps the session `working` until the batch landed (≤ `watch::MAX_DELAY` = 2 s).
```

with

```
    // The first spinner lets the project watcher start (it starts after the first spawn); the
    // second keeps the session `working` until the batch landed (300 ms quiet; a stream that
    // never goes quiet would wait ≤ `watch::MAX_DELAY_WORKING` = 5 s).
```

**3d. UI.**
Create `crates/mkrew-ui/src/watches.rs`:

```rust
//! Which daemon file watchers this window still needs (SPEC §7 `watch.release`, ADR 0087).
//! The daemon keeps a project's watcher after Files / Git / Docs asked for it;
//! [`Needs::released`] names the ones to give back when the window stops showing what needed
//! them. Pure and tested headlessly; `shell/workspace.rs` sends the requests.

use std::collections::BTreeSet;

use mkrew_protocol::WatchReleaseParams;
use ulid::Ulid;

/// The watchers in use: `(project, None)` = the project folder, `(project, Some(task))` = a
/// task worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Needs(BTreeSet<(Ulid, Option<Ulid>)>);

impl Needs {
    /// What showing `workspace` uses: always its project watcher (the title bar summary
    /// follows `git.changed`), plus the worktree of the Files view (`files`) and of the Git
    /// view (`git`) while that view is on screen. Callers pass `None` for a view that is
    /// hidden or that shows the project folder.
    pub fn of(workspace: Option<Ulid>, files: Option<Ulid>, git: Option<Ulid>) -> Self {
        let mut needs = BTreeSet::new();
        if let Some(project) = workspace {
            needs.insert((project, None));
            needs.extend([files, git].into_iter().flatten().map(|t| (project, Some(t))));
        }
        Self(needs)
    }

    /// The watchers `self` used and `now` does not: one `watch.release` each.
    pub fn released(&self, now: &Needs) -> Vec<WatchReleaseParams> {
        self.0
            .difference(&now.0)
            .map(|&(project_id, worktree_task_id)| WatchReleaseParams {
                project_id,
                worktree_task_id,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(project_id: Ulid, worktree_task_id: Option<Ulid>) -> WatchReleaseParams {
        WatchReleaseParams {
            project_id,
            worktree_task_id,
        }
    }

    /// Switching workspace gives back the old one's project watcher and its worktree watcher.
    #[test]
    fn switching_workspace_releases_the_old_project_and_its_worktree() {
        let (a, b, task) = (Ulid(1), Ulid(2), Ulid(3));
        let before = Needs::of(Some(a), None, Some(task));
        let after = Needs::of(Some(b), None, None);
        assert_eq!(
            before.released(&after),
            [release(a, None), release(a, Some(task))]
        );
    }

    /// Hiding the panel (or leaving the Diff view) gives back only the worktree: the shown
    /// project keeps its watcher for the title bar. Picking another worktree gives back the
    /// old one, and a view on the project folder releases nothing.
    #[test]
    fn hiding_a_worktree_view_keeps_the_shown_project() {
        let (a, t1, t2) = (Ulid(1), Ulid(3), Ulid(4));
        let shown = Needs::of(Some(a), Some(t1), None);
        let hidden = Needs::of(Some(a), None, None);
        assert_eq!(shown.released(&hidden), [release(a, Some(t1))]);
        assert!(hidden.released(&shown).is_empty(), "showing it again takes, not gives");
        assert_eq!(
            shown.released(&Needs::of(Some(a), Some(t2), None)),
            [release(a, Some(t1))]
        );
        // Files and Git on the same worktree count once.
        assert_eq!(
            Needs::of(Some(a), Some(t1), Some(t1)),
            Needs::of(Some(a), None, Some(t1))
        );
    }
}
```

(`Ulid(n)` constructs from a `u128` as `git_view/tests.rs:1250` does.)

`crates/mkrew-ui/src/lib.rs`: add `pub mod watches;` between `pub mod voice;` (:96) and `pub mod workspace_menu;` (:97).
`crates/mkrew-ui/src/main.rs`: add `use mkrew_ui::watches;` after `use mkrew_ui::voice;` (:117); in `struct Shell`, after `git_summary_again: bool,` (:647) add

```rust
    /// The daemon watchers this window uses for what it shows (`watches`, ADR 0087).
    watch_needs: watches::Needs,
```

`crates/mkrew-ui/src/shell/connection.rs`: in the `Shell` initializer, after `git_summary_again: false,` (:323) add `watch_needs: watches::Needs::default(),`. (`shell/*.rs` start with `use crate::*;`, which brings `watches`, `Request`, `Outcome` in.)
A lost connection leaves `watch_needs` alone (`connection.rs:478`): on reconnect the first `sync_watches` releases what the old connection kept and the new view does not need, a no-op when the daemon restarted.

`crates/mkrew-ui/src/shell/workspace.rs`: new method after `show_view` (ends :415, before the closing `}` of the `impl`):

```rust
    /// Gives the daemon back the file watchers this window no longer shows anything for
    /// (`watch.release`, [`watches`]). Cheap: it compares two small sets.
    pub(crate) fn sync_watches(&mut self, cx: &mut Context<'_, Self>) {
        let files = self
            .files_on_screen()
            .then(|| self.workspace.and_then(|id| self.files.get(&id)))
            .flatten()
            .and_then(|view| view.worktree_task_id);
        let git = self
            .git_shown()
            .then(|| self.workspace.and_then(|id| self.git.get(&id)))
            .flatten()
            .and_then(|view| view.worktree_task_id);
        let needs = watches::Needs::of(self.workspace, files, git);
        let released = self.watch_needs.released(&needs);
        self.watch_needs = needs;
        let Some(connection) = self.connection.clone() else {
            return;
        };
        for params in released {
            let project_id = params.project_id;
            let reply = connection.request(Request::WatchRelease(params));
            cx.spawn(async move |_, _| match reply.await {
                Ok(Outcome::Result(_)) => {}
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%project_id, "watch.release failed: {}", body.message);
                }
                Err(error) => tracing::warn!(%project_id, "watch.release failed: {error:#}"),
            })
            .detach();
        }
    }
```

Same spawn shape as `shell/composer.rs:405`.
Call it wherever the on-screen set can change:
- `show_view` tail (`workspace.rs:401-404`): replace

  ```rust
          if shown != view {
              self.sync_panes(window, cx);
              cx.notify();
          }
  ```

  with

  ```rust
          self.sync_watches(cx);
          if shown != view {
              self.sync_panes(window, cx);
              cx.notify();
          }
  ```

- `activate_workspace` (`workspace.rs:54`): add `self.sync_watches(cx);` before the final `cx.notify();`.
- `panel_changed` (`shell/inspector.rs:126`): add `self.sync_watches(cx);` before the final `cx.notify();` (covers `toggle_inspector`, `show_panel_tab`, `show_file_tree`, `show_git_panel`).
- `refresh_files` (`shell/files.rs:58`): first line of the body `self.sync_watches(cx);` (covers `set_files_target`, which ends in it).
- `refresh_git` (`shell/git.rs:175`): first line of the body `self.sync_watches(cx);` (covers `set_git_target`, `set_git_repo`, `drop_gone_targets`; `review_diff` and the worktree dialog go through `open_git` / `show_view`).

All five are `&mut self` methods that already take `cx`.
A release is sent before any later request, and the daemon handles one connection's requests in order, so a view shown again re-keeps its watcher after the release (`fs.list` / `git.status` / `doc.list` keep it again).
`Needs::released` never names a key the new view needs, so a release and a keep of the same watcher cannot be in flight together.

- [ ] **Step 4: Run tests, expect PASS**

Run: `cargo test -p mkrew-protocol -j 4`, `cargo test -p mkrew-daemon --lib -j 4`, `cargo test -p mkrew-ui --lib watches -j 4`, then `cargo check -p mkrew-ui -j 4`.
Expected: PASS and no errors.

- [ ] **Step 5: SPEC, ADR, HUMAN_CHECKS**

Create `docs/decisions/0087-release-kept-watchers.md`:

```markdown
# 0087 - Release kept file watchers, 5 s batch cap while an agent works

- Date: 2026-10-28
- Status: accepted
- Milestone: none (perf review, B5)

## Context

`doc.*`, `fs.*` and `git.*` requests keep the project's recursive file watcher running until the daemon stops (SPEC §7).
A project looked at once keeps its watcher for the daemon's lifetime, and every batch of changes makes each UI client run `git status`.
An agent that writes files all the time cut a batch every 2 s.
Nothing tells the daemon that a view closed, and `ProjectRef` cannot name a task worktree.
Files and Git are tabs of the right panel (ADR 0081, 0084), so a view closes by hiding the panel, changing its tab, leaving the Diff view or switching workspace.

## Options

1. Keep watchers until the daemon stops (today) - pros: nothing to build / cons: idle handles and `git status` runs for projects nobody looks at.
2. Daemon-side timeout after the last request - pros: no protocol change / cons: a view that stays open and idle loses its refresh; needs a heartbeat.
3. New request `watch.release` sent by the UI when it stops showing what needed the watcher - pros: one small request, the UI already knows its workspace, panel tab and worktree target / cons: protocol bump, one more thing to keep in step with the views.

## Decision

Option 3: `watch.release { project_id, worktree_task_id? }` (protocol 49).
The daemon clears the `kept` flag of the project watcher and stops it unless a session of the project is live (its exit then drops it as before); a worktree watcher stops at once.
The UI keeps the shown workspace's project watcher (the title bar summary follows `git.changed`) and a worktree watcher only while its Files or Git view is on screen.
There is no reference count: the app has one UI window.
While an agent session of the project is `working`, a batch may wait up to 5 s after its first change instead of 2 s (300 ms of quiet still ends it).
Shell and run sessions do not count.
`git.changed` was already sent only for a batch with a reported file or a git-state change; that does not change.

## Consequences

- SPEC §7: new `watch.release` row; `doc.*` rows say "until released or the daemon stops"; `git.changed` debounce text; §12B refresh line. Protocol 48 to 49.
- A watcher the UI never asked about through its shown view is not released: the Quick open `fs.find` on a worktree, the fan-out columns' `git.status`, the worktree dialog's `git.status` / `git.log`.
  They stay until the worktree is removed or the daemon stops, as before.
- Watchers kept by an earlier UI run of the same daemon are not released either; the new UI only releases what it used itself.
- A second UI window that still shows a released workspace would lose `git.changed` for it until its next request re-keeps it; revisit with a reference count if multiple windows arrive.
- An old UI against the new daemon gets `version_mismatch` and asks to restart the daemon (SPEC §7).
```

`docs/SPEC.md` (line numbers of 2026-10-28):
- After the `session.ports` row (:544) add:
  `| `watch.release` | `project_id`, `worktree_task_id?` | ok (`null`) - the UI no longer shows that workspace (or task worktree): it gives back the file watcher that `doc.*`, `fs.*` and `git.status` keep running. The watcher stops unless a session of the project is live; a later request of those kinds keeps it again (ADR 0087) |`
- `:572`: replace `Keeps the project watched for `doc.changed` until the daemon stops` with `Keeps the project watched for `doc.changed` until released (`watch.release`) or the daemon stops`.
- `:573`: replace `Also keeps the project watched` with `Also keeps the project watched (until released)`.
- `:616`: replace `workspace git state changed (debounced, ≤ once per 300 ms)` with `workspace git state changed (debounced: 300 ms quiet, at most 2 s after the first change, 5 s while an agent session of the project is `working`; a batch with nothing to report after filtering sends nothing)`.
- `:1233`: replace `Refresh is triggered by the file watcher (debounced 300 ms)` with `Refresh is triggered by the file watcher (debounced 300 ms, §7 `git.changed`)`.
- After `Protocol 40 (ADR 0053): `daemon.status` gains `web`.` (:647) add a line: `Protocol 49 (ADR 0087): request `watch.release`.`

`docs/HUMAN_CHECKS.md` (re-read each spot first; text of 2026-10-28):
- `:2933-2934` (T5.5): replace `Batches = 300 ms quiet, at\n  most 2 s;` with `Batches = 300 ms quiet, at\n  most 2 s (5 s while an agent session of the project is `working`, ADR 0087);`.
- `:3009-3011` (T6.4): replace `and it then stays until the\n  daemon stops — even after the last session exits (one watcher per project ever opened in\n  the Docs view; no unwatch on client disconnect / archive).` with `and it then stays until the UI gives it back\n  (`watch.release`, ADR 0087: that workspace is no longer shown) or the daemon stops — even after\n  the last session exits (no unwatch on client disconnect / archive).`
- `:3129` (T7.4): replace `` `git.status` keeps the project watched until the daemon stops (like docs). `` with `` `git.status` keeps the project watched until released or the daemon stops (like docs, ADR 0087). ``

HUMAN_CHECKS new bullets: append to the shared section `### To verify - 2026-10-28 perf (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)`, which sits directly above the first existing `### To verify` heading (today `### To verify - 2026-10-28 Session tab behind `[features] session``, line 24; created by the first task of this plan that runs, so create it if it is not there):

```markdown
- [ ] **Restart the daemon first** (protocol 49). An old UI against the new daemon shows the "restart the daemon" message (expected); a running `mk web` must be restarted too.
- [ ] **Watchers stop with their view:** start the daemon with `MKREW_LOG=debug`. Add a second workspace B with no agent running. In B open the Files tab, then switch to workspace A: the log shows `watch released` and `stopped watching project files` for B. In B open the Git tab on a task worktree (worktree chooser), then hide the right panel: `watch released` with that task id. Open the panel again: Files / Git refresh, and an edit made outside the app shows (the Git list updates).
- [ ] **A live agent keeps its watcher:** leave a working agent in workspace B, switch to A: B's `watch released` is logged but `stopped watching` is not; B's file changes still count for its session (the Git list of B, shown again, lists them).
- [ ] **`git.changed` rate:** with one agent `working` and writing files all the time, the Git tab and the title bar git summary refresh about every 5 s (not every 2 s). Once the agent is idle, the next change shows within about 0.5 s. A commit or `git fetch` from outside still shows within about 0.5 s with no agent working. A background `git gc` (only `.git/objects`) changes nothing on screen.
```

- [ ] **Step 6: Verify and commit**

Run: `cargo build -p mkrew-cli -p mkrew-fakeagent -j 4` (the protocol changed; the daemon tests use these bins), `cargo test -p mkrew-protocol -p mkrew-daemon -p mkrew-ui --lib -j 4`, `cargo test -p mkrew-daemon --test daemon -j 4 two_sessions_editing`, then `sh scripts/verify.sh` (omp shell `bash` is WSL).
Commit (stage only these paths; `git add -p` for `session.rs`, `main.rs`, `shell/connection.rs`, `shell/workspace.rs`, `shell/inspector.rs`, `shell/files.rs`, `shell/git.rs`, `SPEC.md`, `HUMAN_CHECKS.md`, which other tasks of this plan may also edit):
`git add crates/mkrew-daemon/src/watch.rs crates/mkrew-daemon/src/session.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/server/target.rs crates/mkrew-daemon/tests/daemon/docs.rs crates/mkrew-protocol/src/files.rs crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/message/tests.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-ui/src/watches.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/workspace.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/files.rs crates/mkrew-ui/src/shell/git.rs docs/decisions/0087-release-kept-watchers.md docs/SPEC.md docs/HUMAN_CHECKS.md`
`git -c user.email=tungnv975@gmail.com commit -m "perf(daemon): 5 s watcher cap while an agent works, release kept watchers (protocol 49)"`

**Review focus for this task:** (1) a project whose agent is still live when the user leaves its workspace must keep its watcher, or `files_changed` attribution and conflict detection go blind (`releasing_a_kept_project_watch_drops_it_unless_a_session_lives`); (2) hiding the right panel or leaving the Diff view on a task worktree must free only that worktree and never the shown project (`hiding_a_worktree_view_keeps_the_shown_project`).


### Task 4: Daemon screen scan builds the last 4 rows, at most 4 scans a second per session
**Covers:** audit items "scan per output batch hashes all cells and builds every row", "scan runs although the profile ignores the screen" (old plan Task 13, perf B9, `docs/superpowers/plans/2026-10-10-inbox-perf.md:4328-4914`)
Nothing of the old task has landed: `git log -S"ScanRate" -- crates` finds no commit, and `Emulator::tail_to_cursor` does not exist.

**Current state (verified 2026-10-28):**
- `Actor::on_output` (`crates/mkrew-daemon/src/session.rs:1210-1245`) runs for every output batch of every live session, background ones included.
  It is fed by the normal path (`:1059`), the flood path after `take_turn` (`:1079`) and `drain_after_exit` (`:1567`).
  The flood pacing (ADR 0002) only limits how often a flooding session reaches `on_output`; a busy agent still gets up to 60 batches a second (95a8a6e).
- After the log write, DSR reply, title and attachments it calls `Emulator::text_fingerprint()` (`emulator.rs:134-145`, SipHash over every cell of the screen).
  Only when the hash changed (`session.rs:1232-1233`) it calls `Actor::screen_lines()` (`:1349-1354`, `Emulator::lines_to_cursor`, `emulator.rs:113-129`): one `String` per row from the top to the cursor row.
  Then `Actor::scan_ports(&lines)` (`:1439`, returns at once unless `self.ports.is_some()`, i.e. a run session) and `StateMachine::on_output(Instant::now(), &lines)`.
- The state machine reads only the last `PATTERN_TAIL_LINES = 4` non-blank rows (`crates/mkrew-core/src/state.rs:21`; `needs_input` at `:144-163` trims, drops blanks, takes 4).
  The idle tick (`session.rs:1182-1189`, only armed while `Working` and `uses_screen`) also builds the full text.
- `StateMachine::on_output` (`state.rs:67-82`) bumps `last_output` first, then returns early when `!screen_active(now)` (`:136-142`: mode is `hook`, or mode includes hooks and a hook fired in the last `HOOK_PRIORITY` = 10 s).
  In that early return a hook-only profile still leaves `Starting` for `Unknown` (`:71-73`).
  `screen_active` is private; the daemon cannot ask it.
- Profiles: `claude`, `codex`, `copilot`, `omp` are `hook+screen`, the rest `screen` (`agents/*.toml`); none is `hook`.
  So the "profile does not use the screen" case is a user's own hook-only profile, and "hook fired recently" is the common one (10 s after every hook event of the four hook agents).
- The only other consumer of the full text is `Actor::scan_ports` (run sessions: need every row up to the cursor, SPEC §10).
  Not consumers: `session.read` (`Emulator::last_lines`), the title (`take_title`), attach snapshots (`snapshot()`).
- `Actor::deliver()` runs at the end of `on_output` (`:1244`) and in the tick branch (`:1188`).
  A throttled scan would leave a stale `idle` there and deliver the second queued text into a busy agent, so a non-empty queue must bypass the throttle.
- ADR 0015 (only output that changes the screen text is activity): `screen_print` (`session.rs:1015`, set `:1556`) holds the fingerprint after the last scan.
  This stays; it now compares two scans, not two batches.
- Daemon e2e that must keep passing (all in the one `daemon` test binary, `crates/mkrew-daemon/tests/daemon/`): `states.rs:6` `fake_agent_session_reaches_needs_input_and_is_killed` (the fake agent prints a prompt then goes silent: needs the trailing scan), `io.rs:211` `send_while_working_is_queued_until_the_agent_asks_and_wait_sees_it_finish`, `io.rs:268` `a_queued_text_removed_while_working_is_never_typed`, `io.rs:354` `exiting_with_queued_texts_announces_an_empty_queue`, `runs.rs:349` `run_sessions_report_the_ports_they_print`.
- `mod` list in `crates/mkrew-daemon/src/lib.rs:6-35` is alphabetical (`mod plan;` `mod retention;` `mod server;`).
  `mod scan;` goes between `mod retention;` (`:20`) and `mod server;` (`:21`).
- Highest ADR is 0085; `docs/decisions/0088-*` does not exist (Task 4 creates it; Tasks 5 and 6 append a section each).

**Files:**
- Create: `crates/mkrew-daemon/src/scan.rs` (`ScanRate`, `SCAN_GAP`, `changed`, tests)
- Create: `docs/decisions/0088-terminal-path-limits.md`
- Modify: `crates/mkrew-daemon/src/lib.rs` (`mod scan;`)
- Modify: `crates/mkrew-daemon/src/emulator.rs` (`lines_to_cursor` split, new `tail_to_cursor`, `cursor_row`, `row_chars`, `row_text`; tests)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Actor.scan`, `Actor::run` branch, `on_output`, new `scan_screen`, `screen_tail`, `finish`; `git add -p`, other tasks edit this file)
- Modify: `crates/mkrew-core/src/state.rs` (`screen_active` becomes `pub`, doc of `PATTERN_TAIL_LINES`), `crates/mkrew-core/src/state/tests.rs` (one test)
- Modify: `docs/SPEC.md` §6 (after line 478), §8 (lines 675-676)
- Modify: `docs/HUMAN_CHECKS.md` (shared perf section)

**Interfaces:**
- Produces: `Emulator::tail_to_cursor(&self, count: usize) -> Vec<String>`: the last `count` non-blank rows from the top to the cursor row, top-down, trailing blanks trimmed.
- Produces: `StateMachine::screen_active(&self, now: Instant) -> bool` (was private): `true` when the machine would read the screen at `now`.
- Produces: `crate::scan::ScanRate` with `new(gap: Duration) -> Self`, `on_output(&mut self, now: Instant, urgent: bool) -> bool`, `deadline(&self) -> Option<Instant>`, `scanned(&mut self, now: Instant)`, `take_pending(&mut self) -> bool`; `pub const SCAN_GAP: Duration` (250 ms).
- Produces: `crate::scan::changed(print: &mut Option<u64>, now_print: u64, starting: bool) -> bool`: the ADR 0015 gate, `true` when the visible text changed since the last scan (the first scan of a `Starting` session counts; the first scan after a skipped stretch only sets the baseline).
- Consumes: `mkrew_core::state::PATTERN_TAIL_LINES` (`pub mod state`, `mkrew-core/src/lib.rs:23`), `StateMachine::on_output/tick`, `mkrew_core::{StateConfig, StateMode}` (crate root, `lib.rs:33-36`).

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-daemon/src/emulator.rs` `mod tests`, after `lines_stop_at_the_cursor_row` (`:315-321`), add:

```rust
    /// What the state machine took of the old full text: the last `count` non-blank rows
    /// (trailing blanks trimmed), top to bottom.
    fn old_feed(emu: &Emulator, count: usize) -> Vec<String> {
        let mut tail: Vec<String> = emu
            .lines_to_cursor()
            .into_iter()
            .rev()
            .filter(|line| !line.trim_end().is_empty())
            .take(count)
            .collect();
        tail.reverse();
        tail
    }

    #[test]
    fn the_tail_is_what_the_full_text_fed_the_state_machine() {
        let scrolled: String = (0..12).map(|n| format!("line{n}\r\n")).collect();
        let screens: [(&str, &[u8]); 9] = [
            ("empty", b""),
            ("one line", b"hello"),
            ("blank rows between", b"a\r\n\r\n   \r\nb\r\n\r\n\r\nc"),
            ("more than four", b"1\r\n2\r\n3\r\n4\r\n5\r\n6"),
            ("wide char", "形x\r\nok".as_bytes()),
            ("cursor above the content", b"1\r\n2\r\n3\r\n4\r\n5\r\n\x1b[2;1H"),
            ("scrolled", scrolled.as_bytes()),
            ("whitespace-only rows", b"x\r\n \t \r\ny"),
            (
                "colored prompt with trailing blanks",
                b"\x1b[32mDo you want to proceed? (y/n)\x1b[0m   \r\n",
            ),
        ];
        for (name, bytes) in screens {
            let mut emu = Emulator::new(20, 8);
            emu.advance(bytes);
            for count in [1, 2, mkrew_core::state::PATTERN_TAIL_LINES] {
                assert_eq!(emu.tail_to_cursor(count), old_feed(&emu, count), "{name}, {count}");
            }
        }
    }
```

Create `crates/mkrew-daemon/src/scan.rs` with only the tests (the items do not exist yet):

```rust
//! Rate limit and change gate of the per-session screen scan (ADR 0088). Pure: the caller
//! passes `now`.

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use mkrew_core::state::PATTERN_TAIL_LINES;
    use mkrew_core::{StateConfig, StateMachine, StateMode};
    use mkrew_protocol::SessionState;

    use super::*;
    use crate::emulator::Emulator;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_first_output_scans_at_once_and_a_close_one_waits_for_the_trailing_scan() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(ms(250));
        assert!(rate.on_output(t0, false));
        assert_eq!(rate.deadline(), None);
        assert!(!rate.on_output(t0 + ms(10), false));
        assert_eq!(rate.deadline(), Some(t0 + ms(250)));
        // The gap runs from the last scan, not from the last output.
        assert!(!rate.on_output(t0 + ms(249), false));
        assert_eq!(rate.deadline(), Some(t0 + ms(250)));
        rate.scanned(t0 + ms(250));
        assert_eq!(rate.deadline(), None);
        assert!(!rate.take_pending());
        // Quiet for a whole gap: the next output scans at once again.
        assert!(rate.on_output(t0 + ms(500), false));
    }

    /// 300 batches at 16 ms (about 60/s): never two scans closer than the gap, and the
    /// last batch is always followed by a scan within one gap.
    #[test]
    fn a_flood_scans_at_most_four_times_a_second_and_the_last_batch_is_scanned() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        let mut scans = Vec::new();
        let mut last_batch = t0;
        for i in 0..300 {
            let now = t0 + ms(i * 16);
            // The trailing scan fires between two batches when it is due.
            while let Some(due) = rate.deadline().filter(|due| *due <= now) {
                rate.scanned(due);
                scans.push(due);
            }
            if rate.on_output(now, false) {
                scans.push(now);
            }
            last_batch = now;
        }
        let due = rate.deadline().expect("the last batch is owed a scan");
        assert!(due <= last_batch + SCAN_GAP);
        rate.scanned(due);
        scans.push(due);
        for pair in scans.windows(2) {
            assert!(pair[1] - pair[0] >= SCAN_GAP, "{pair:?}");
        }
        // 4.8 s of output.
        assert!(scans.len() <= 21, "{} scans", scans.len());
    }

    /// A queued `session.send` is delivered when the state allows it; the state must be
    /// current then, so a waiting send scans every batch.
    #[test]
    fn a_waiting_send_is_never_held_back() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        assert!(rate.on_output(t0, false));
        assert!(rate.on_output(t0 + ms(1), true));
        assert_eq!(rate.deadline(), None);
    }

    #[test]
    fn an_exit_can_claim_the_owed_scan_once() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        rate.on_output(t0, false);
        rate.on_output(t0 + ms(5), false);
        assert!(rate.take_pending());
        assert!(!rate.take_pending());
        assert_eq!(rate.deadline(), None);
    }

    fn machine() -> StateMachine {
        StateMachine::new(&StateConfig {
            mode: StateMode::Screen,
            idle_after_ms: 3000,
            needs_input_patterns: vec![r"(?i)do you want to".into()],
        })
        .unwrap()
    }

    /// What `Actor::scan_screen` feeds the machine: the tail of the real emulator.
    fn scan(emu: &Emulator, machine: &mut StateMachine, now: Instant) -> Option<SessionState> {
        let tail = emu.tail_to_cursor(PATTERN_TAIL_LINES);
        let tail: Vec<&str> = tail.iter().map(String::as_str).collect();
        machine.on_output(now, &tail)
    }

    /// The agent asks 100 ms after the last scan and then stays silent: the prompt must still
    /// become `needs_input` (at the trailing scan), never be missed.
    #[test]
    fn a_prompt_that_lands_inside_the_gap_is_classified_by_the_trailing_scan() {
        let t0 = Instant::now();
        let mut emu = Emulator::new(40, 6);
        let mut machine = machine();
        let mut rate = ScanRate::new(SCAN_GAP);
        emu.advance(b"Compiling...\r\n");
        assert!(rate.on_output(t0, false));
        assert_eq!(scan(&emu, &mut machine, t0), Some(SessionState::Working));

        emu.advance(b"Do you want to proceed?\r\n");
        assert!(!rate.on_output(t0 + ms(100), false));
        assert_eq!(machine.state(), SessionState::Working);

        let due = rate.deadline().expect("a trailing scan is owed");
        assert_eq!(due, t0 + SCAN_GAP);
        rate.scanned(due);
        assert_eq!(scan(&emu, &mut machine, due), Some(SessionState::NeedsInput));
    }

    /// ADR 0015 across a gap: bytes that change nothing visible (cursor, title, identical
    /// repaint) are not activity at the next scan; a changed row is.
    #[test]
    fn the_gate_counts_visible_changes_between_two_scans_only() {
        let mut emu = Emulator::new(20, 4);
        let mut print = None;
        emu.advance(b"hello");
        // First scan of a `Starting` session counts.
        assert!(changed(&mut print, emu.text_fingerprint(), true));
        emu.advance(b"\x1b[?25l\x1b]0;t\x07\x1b[H\x1b[2Jhello\x1b[?25h");
        assert!(!changed(&mut print, emu.text_fingerprint(), false));
        // `x` then back to `hello` inside one gap: not seen (documented in ADR 0088).
        emu.advance(b"\x1b[Hx");
        emu.advance(b"\x1b[Hh");
        assert!(!changed(&mut print, emu.text_fingerprint(), false));
        emu.advance(b"!");
        assert!(changed(&mut print, emu.text_fingerprint(), false));
    }

    /// After a stretch where the scan was skipped (a hook fired, `print` reset to `None`)
    /// the first scan of a session that left `Starting` only sets the baseline: a `done`
    /// session must not turn `working` just because its screen moved while nobody looked.
    #[test]
    fn the_first_scan_after_a_skipped_stretch_is_a_baseline() {
        let mut print = None;
        assert!(!changed(&mut print, 7, false));
        assert_eq!(print, Some(7));
        assert!(!changed(&mut print, 7, false));
        assert!(changed(&mut print, 8, false));
    }
}
```

In `crates/mkrew-core/src/state/tests.rs`, after `patterns_only_match_the_screen_tail` (`:86-94`), add:

```rust
#[test]
fn the_screen_is_read_only_by_screen_modes_and_not_just_after_a_hook() {
    let c = clock();
    assert!(machine(StateMode::Screen).screen_active(c.at(0)));
    assert!(!machine(StateMode::Hook).screen_active(c.at(0)));

    let mut m = machine(StateMode::HookScreen);
    assert!(m.screen_active(c.at(0)));
    m.on_hook(c.at(100), HookEvent::Busy);
    assert!(!m.screen_active(c.at(100)));
    assert!(!m.screen_active(c.at(100) + HOOK_PRIORITY - Duration::from_millis(1)));
    assert!(m.screen_active(c.at(100) + HOOK_PRIORITY));
    // A `screen` profile ignores hooks: its screen is always read.
    let mut m = machine(StateMode::Screen);
    m.on_hook(c.at(100), HookEvent::Busy);
    assert!(m.screen_active(c.at(100)));
    m.on_exit();
    assert!(!m.screen_active(c.at(200)));
}
```

In `crates/mkrew-daemon/src/lib.rs`, between `mod retention;` and `mod server;` add `mod scan;`.

- [ ] **Step 2: Run them, expect FAIL**

Run: `cargo test -p mkrew-daemon --lib -j 4 -- scan::tests emulator::tests::the_tail`
Expected: FAIL to compile (`ScanRate`, `SCAN_GAP`, `changed`, `tail_to_cursor` do not exist).

Run: `cargo test -p mkrew-core --lib -j 4 state::tests::the_screen_is_read`
Expected: FAIL to compile (`screen_active` is private).

- [ ] **Step 3: `Emulator::tail_to_cursor` and the public `screen_active`**

`crates/mkrew-daemon/src/emulator.rs`: replace `lines_to_cursor` with its doc (`:111-129`) by:

```rust
    /// Visible lines from the top of the screen down to the cursor row, trailing blanks
    /// trimmed. Every row: only a run session's port scan needs all of it (SPEC §10); the
    /// state machine reads [`Self::tail_to_cursor`] (SPEC §6).
    pub fn lines_to_cursor(&self) -> Vec<String> {
        (0..=self.cursor_row())
            .map(|row| self.row_text(row))
            .collect()
    }

    /// The last `count` non-blank rows from the top of the screen down to the cursor row, top
    /// to bottom, trailing blanks trimmed: what the state machine matches
    /// `needs_input_patterns` against (`PATTERN_TAIL_LINES` rows). Walks up from the cursor and
    /// builds a `String` only for the rows it keeps.
    pub fn tail_to_cursor(&self, count: usize) -> Vec<String> {
        let mut lines: Vec<String> = (0..=self.cursor_row())
            .rev()
            .filter(|&row| !self.row_chars(row).all(char::is_whitespace))
            .take(count)
            .map(|row| self.row_text(row))
            .collect();
        lines.reverse();
        lines
    }

    /// The screen row the cursor is on (clamped to the screen).
    fn cursor_row(&self) -> usize {
        let row = self.term.grid().cursor.point.line.0.max(0);
        usize::try_from(row)
            .unwrap_or(0)
            .min(self.term.screen_lines().saturating_sub(1))
    }

    /// The characters of screen row `row`, one per column.
    fn row_chars(&self, row: usize) -> impl Iterator<Item = char> + '_ {
        let line = &self.term.grid()[Line(i32::try_from(row).unwrap_or(i32::MAX))];
        (0..self.term.columns()).map(move |column| line[Column(column)].c)
    }

    /// Screen row `row` as text, trailing blanks trimmed.
    fn row_text(&self, row: usize) -> String {
        let mut text: String = self.row_chars(row).collect();
        text.truncate(text.trim_end().len());
        text
    }
```

`text_fingerprint` (`:131-145`) stays as is: the screen text gate must keep seeing spinners and status rows below the cursor (ADR 0015); at 4 scans a second its cost is negligible.
Its doc line `[`Self::lines_to_cursor`]` still resolves.

`crates/mkrew-core/src/state.rs`:
- `:136` `    fn screen_active(&self, now: Instant) -> bool {` becomes
```rust
    /// Whether the machine reads the screen at `now`: the mode includes `screen`, no hook
    /// fired in the last [`HOOK_PRIORITY`] and the process has not exited. When `false`,
    /// building the screen text for [`on_output`](Self::on_output) is wasted work.
    pub fn screen_active(&self, now: Instant) -> bool {
```
- `:19-20`: replace `The daemon passes the screen up to the cursor row.` with `The daemon builds only these rows (\`Emulator::tail_to_cursor\`) of the screen up to the cursor row.`

Run: `cargo test -p mkrew-daemon --lib -j 4 -- emulator::tests` and `cargo test -p mkrew-core --lib -j 4 state::tests`
Expected: PASS (the new tail test, the new `screen_active` test, `lines_stop_at_the_cursor_row`, `scrolled_output_keeps_the_visible_screen`).

- [ ] **Step 4: `ScanRate` and `changed`**

`crates/mkrew-daemon/src/scan.rs`: after the `//!` lines and before `#[cfg(test)]` add:

```rust
use std::time::{Duration, Instant};

/// Two scans of one session are at least this far apart: at most 4 a second (ADR 0088).
pub const SCAN_GAP: Duration = Duration::from_millis(250);

/// When a session's screen is scanned: right away after a quiet gap, else once the gap since
/// the last scan has passed (the trailing scan, so the final screen is always seen).
#[derive(Debug)]
pub struct ScanRate {
    gap: Duration,
    last: Option<Instant>,
    /// Output arrived inside the gap: a scan is owed at `last + gap`.
    pending: bool,
}

impl ScanRate {
    pub const fn new(gap: Duration) -> Self {
        Self {
            gap,
            last: None,
            pending: false,
        }
    }

    /// Output arrived at `now`: `true` = scan the screen now. `urgent` (a `session.send` waits
    /// for the state) always scans.
    pub fn on_output(&mut self, now: Instant, urgent: bool) -> bool {
        let due = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= self.gap);
        if due || urgent {
            self.scanned(now);
            true
        } else {
            self.pending = true;
            false
        }
    }

    /// When the owed trailing scan is due; `None` when nothing is held back.
    pub fn deadline(&self) -> Option<Instant> {
        if self.pending {
            self.last.map(|last| last + self.gap)
        } else {
            None
        }
    }

    /// A scan ran at `now` (the trailing one included).
    pub fn scanned(&mut self, now: Instant) {
        self.last = Some(now);
        self.pending = false;
    }

    /// Takes the owed scan: `true` if there was one (the session ended before it fired).
    pub fn take_pending(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}

/// ADR 0015 at scan time: `true` when the visible text changed since the previous scan
/// (`print` holds that scan's fingerprint; it is updated). Only visible changes are work: idle
/// TUIs keep sending cursor/title/progress bytes, which would hold a session in `working`.
/// `None` is "no baseline": it counts for a `Starting` session (hook-only profiles still leave
/// `starting`), but after a stretch of skipped scans (the caller reset `print`) it only sets
/// the baseline.
pub fn changed(print: &mut Option<u64>, now_print: u64, starting: bool) -> bool {
    let before = print.replace(now_print);
    before != Some(now_print) && (before.is_some() || starting)
}
```

Run: `cargo test -p mkrew-daemon --lib -j 4 -- scan::tests`
Expected: PASS (7 tests).

- [ ] **Step 5: Wire it into the actor**

`crates/mkrew-daemon/src/session.rs` (use `git add -p` later; other tasks edit this file):

(a) Imports.
After `use crate::launch;` (`:33`) add `use crate::scan::{self, SCAN_GAP, ScanRate};` (alphabetical: `launch`, `scan`, `session_log`).
After `use mkrew_core::turn::{TurnEvent, TurnTracker};` (`:14`) add `use mkrew_core::state::PATTERN_TAIL_LINES;` as its own line; do not touch the `use mkrew_core::{AgentProfile, …}` line (`:15`).

(b) `struct Actor` (`:1013-1016`): replace
```rust
    /// [`Emulator::text_fingerprint`] after the last output; output that leaves it unchanged
    /// is not activity (ADR 0015).
    screen_print: Option<u64>,
```
with
```rust
    /// [`Emulator::text_fingerprint`] after the last scan; output that leaves it unchanged
    /// is not activity (ADR 0015). `None` until the first scan and after skipped scans.
    screen_print: Option<u64>,
    /// At most one screen scan per [`SCAN_GAP`], plus a trailing one (ADR 0088).
    scan: ScanRate,
```

(c) The `Actor { … }` literal in `spawn_watched` (`:556`): after `            screen_print: None,` add `            scan: ScanRate::new(SCAN_GAP),`.

(d) `Actor::run` (`:1046-1050`): after the `let tick_at = self.machine.next_deadline().map(tokio::time::Instant::from_std);` statement add
```rust
            let scan_at = self.scan.deadline().map(tokio::time::Instant::from_std);
```
In the `select!`, replace the tick branch head (`:1182-1185`)
```rust
                () = sleep_until(tick_at), if tick_at.is_some() => {
                    let lines = self.screen_lines();
                    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
                    if let Some(state) = self.machine.tick(Instant::now(), &lines) {
```
with
```rust
                () = sleep_until(scan_at), if scan_at.is_some() => {
                    // The scan `ScanRate` held back: the final screen is always classified.
                    let now = Instant::now();
                    self.scan.scanned(now);
                    self.scan_screen(now);
                    self.deliver();
                }
                () = sleep_until(tick_at), if tick_at.is_some() => {
                    let lines = self.screen_tail();
                    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
                    if let Some(state) = self.machine.tick(Instant::now(), &lines) {
```
(the rest of the tick branch, `:1186-1189`, stays).

(e) `Actor::on_output`: replace `:1229-1240`
```rust
        // Only visible changes are work: idle TUIs keep sending cursor/title/progress bytes,
        // which would otherwise hold the session in `working` forever (ADR 0015). The first
        // batch always counts, so hook-only profiles still leave `starting`.
        let print = self.emulator().text_fingerprint();
        if self.screen_print.replace(print) != Some(print) {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            self.scan_ports(&lines);
            if let Some(state) = self.machine.on_output(Instant::now(), &lines) {
                self.set_state(state, StateSource::Screen);
            }
        }
```
with
```rust
        // The screen work runs at most every `SCAN_GAP` (ADR 0088). A waiting `session.send`
        // needs the current state when `deliver()` runs below, so it scans every batch.
        let now = Instant::now();
        if self.scan.on_output(now, !self.queue.is_empty()) {
            self.scan_screen(now);
        }
```

(f) Replace `fn screen_lines` (`:1349-1354`) with:
```rust
    /// Every row up to the cursor: only [`Actor::scan_ports`] of a run session needs it.
    fn screen_lines(&self) -> Vec<String> {
        self.emulator
            .as_ref()
            .map(Emulator::lines_to_cursor)
            .unwrap_or_default()
    }

    /// The rows the state machine reads ([`PATTERN_TAIL_LINES`]), built without the rest.
    fn screen_tail(&self) -> Vec<String> {
        self.emulator
            .as_ref()
            .map(|emulator| emulator.tail_to_cursor(PATTERN_TAIL_LINES))
            .unwrap_or_default()
    }

    /// The screen work of output: the ports a run session printed, then the state machine.
    /// `now` is when the scan runs (the trailing scan fires up to [`SCAN_GAP`] after the output).
    fn scan_screen(&mut self, now: Instant) {
        let active = self.machine.screen_active(now);
        if !active && self.ports.is_none() {
            // The machine ignores the screen (hook-only profile, or a hook fired in the last
            // 10 s): no fingerprint, no rows. It still sees the output, so a hook-only profile
            // leaves `starting`. The next scan starts from a new baseline.
            self.screen_print = None;
            if let Some(state) = self.machine.on_output(now, &[]) {
                self.set_state(state, StateSource::Screen);
            }
            return;
        }
        let print = self.emulator().text_fingerprint();
        let starting = self.machine.state() == SessionState::Starting;
        if !scan::changed(&mut self.screen_print, print, starting) {
            return;
        }
        if self.ports.is_some() {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            self.scan_ports(&lines);
        }
        let tail = if active { self.screen_tail() } else { Vec::new() };
        let tail: Vec<&str> = tail.iter().map(String::as_str).collect();
        if let Some(state) = self.machine.on_output(now, &tail) {
            self.set_state(state, StateSource::Screen);
        }
    }
```
A run session keeps the fingerprint path (ports need all rows); its profile is `screen` (`shell`, `bash`, ...), so `active` is true and it builds the 4-row tail too.

(g) `Actor::finish` (`:1505-1508`): replace
```rust
    fn finish(mut self, exit_code: Option<i32>) {
        if let Err(e) = self.log.flush() {
```
with
```rust
    fn finish(mut self, exit_code: Option<i32>) {
        // An exit cut off the trailing scan: a run session still owes the ports it printed.
        // (Not the state: a process that exited is not waiting for input.) Before the ports
        // map entry is removed below.
        if self.scan.take_pending() && self.ports.is_some() {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            self.scan_ports(&lines);
        }
        if let Err(e) = self.log.flush() {
```

Compile check: `cargo check -p mkrew-daemon -j 4`
Expected: no errors, no new warnings (`screen_lines` is used by `scan_screen` and `finish`).

- [ ] **Step 6: Daemon e2e still holds**

Run: `cargo build -p mkrew-fakeagent -p mkrew-cli -j 4` (the harness needs both), then
`cargo test -p mkrew-daemon --test daemon -j 4 -- fake_agent_session_reaches_needs_input send_while_working a_queued_text_removed exiting_with_queued run_sessions_report_the_ports`
Expected: PASS: the prompt is found by the trailing scan; queued texts still go one at a time because a non-empty queue scans every batch; a run that prints its port and exits at once still reports it (via `finish`).

- [ ] **Step 7: ADR 0088, SPEC, HUMAN_CHECKS**

Create `docs/decisions/0088-terminal-path-limits.md` (Tasks 5 and 6 append their sections below `## Decision: screen scan`):

```markdown
# 0088 - Terminal path limits: screen scan rate, UI scrollback, history replay

- Date: 2026-10-28
- Status: accepted
- Milestone: perf 2026-10-28 (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)

## Context

The daemon hashes every cell of a session's screen after each output batch (up to 60 a second per session, background ones too).
When the hash changed it built a `String` for every row up to the cursor and ran the state patterns, although the state machine reads only the last `PATTERN_TAIL_LINES` = 4 rows (ADR 0015 added the fingerprint, ADR 0003 the state source).
It did so also while the machine ignored the screen: for 10 s after every hook event of `hook+screen` profiles, and for `hook` profiles always.
The UI scrollback and the History replay costs are decided in the sections appended by their own tasks.

## Decision: screen scan

- The state machine gets only the last 4 non-blank rows up to the cursor (`Emulator::tail_to_cursor`).
  A run session still builds every row for its port scan (SPEC §10).
- A session's screen is scanned at most every 250 ms (`scan::ScanRate`).
  Output inside the gap is scanned by a trailing scan when the gap ends, so a prompt that is the last thing printed is never missed.
  A session with a queued `session.send` scans every batch, because the send queue needs the current state.
- When the machine ignores the screen (`StateMachine::screen_active` is false) and the session is not a run session, the scan does no fingerprint and builds no rows; the machine still sees that output happened.
  The first scan after such a stretch only sets the fingerprint baseline, so a `done` session does not turn `working` because its screen moved while nobody looked.
- The fingerprint (ADR 0015) is compared between two scans, not two batches.
  A change that is undone inside one gap is not seen.

## Consequences

- `working` and `needs_input` can show up to 250 ms late; `idle` (3 s quiet) is unaffected.
- During a hook window, invisible output (cursor, title) now moves `last_output`, which the fingerprint gate used to filter: a TUI that keeps blinking through the end of the window goes `idle` up to `idle_after_ms` after the window ends, not after its last visible change.
- SPEC §6 and §8 say so.
```

`docs/SPEC.md` §6: after the sentence ending `…gives garbage.` (line 478) add a new paragraph (blank line before):

```markdown
The daemon scans a session's screen at most 4 times a second (a scan after the last output closes the gap, so the final screen is always classified) and the state machine reads only the last 4 non-blank rows up to the cursor; a state shown by the screen can therefore appear up to 250 ms after the output (ADR 0088).
A session with a queued `session.send` is scanned on every output, and while the machine ignores the screen (hook-only profile, or a hook event in the last 10 s) no scan work is done.
```

`docs/SPEC.md` §8 (lines 675-676): replace
```markdown
- Output fan-out per chunk: emulator → state detector, session log file (buffered writer),
  attached clients (binary frames, §7).
```
with
```markdown
- Output fan-out per chunk: emulator → state detector (at most 4 scans/s per session, ADR 0088),
  session log file (buffered writer), attached clients (binary frames, §7).
```

`docs/HUMAN_CHECKS.md`: the perf section does not exist yet at the time of writing this plan; the first task to run creates it directly above the first existing `### To verify -` heading (now line 24, `### To verify - 2026-10-28 Session tab behind ...`).
Use the file's own hyphen in the heading: `### To verify - 2026-10-28 perf (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)`.
Append at the end of that section:

```markdown
- [ ] **Terminal path, screen scan (ADR 0088):** restart the daemon. With claude (patterns only) and a fake agent or a shell in the other two panes:
  - Ask claude something that needs approval: the pane shows `needs_input` and the notification fires within about half a second, also when the prompt is the last thing printed and the pane is in a background tab.
  - Answer it: the state goes back to `working`, then `idle` about 3 s after the output stops.
  - `mk send <id> "one"` and `mk send <id> "two"` while the agent works: both are typed one after the other, "two" only after the agent reacted to "one".
  - A run command that prints `Local: http://localhost:5173` shows `:5173` in the Run button, also when it prints that line and exits at once.
  - A claude turn that ends (hook `done`) and then sits at its prompt for a minute stays `done`/`idle`, it does not flip back to `working` by itself.
  - Task Manager: the daemon's CPU with three agents `working` is lower than before (note the numbers here).
```

- [ ] **Step 8: Verify and commit**

Run: `cargo test -p mkrew-daemon --lib -j 4 -- scan emulator`, `cargo test -p mkrew-core --lib -j 4 state` (expect PASS), then `sh scripts/verify.sh`.
The old inbox line is not ticked by this task alone (Tasks 5 and 6 share ADR 0088).

```
git add crates/mkrew-daemon/src/scan.rs crates/mkrew-daemon/src/emulator.rs crates/mkrew-core/src/state.rs crates/mkrew-core/src/state/tests.rs docs/decisions/0088-terminal-path-limits.md
git add -p crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/session.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "perf(daemon): scan the screen at most 4 times a second and build only the 4 rows the state machine reads"
```
(`session.rs`, `lib.rs`, `SPEC.md` and `HUMAN_CHECKS.md` are shared with other tasks: stage only this task's hunks.)

**Review focus for this task:** (1) a prompt printed 100 ms after a scan and then silence must still reach `needs_input` (`a_prompt_that_lands_inside_the_gap_is_classified_by_the_trailing_scan`, e2e `fake_agent_session_reaches_needs_input_and_is_killed`); (2) a run command that prints its port and exits at once still reports it (e2e `run_sessions_report_the_ports_they_print`, via the `finish` scan), and invisible bytes after a hook `done` must not flip the session to `working` (`the_first_scan_after_a_skipped_stretch_is_a_baseline`, `the_gate_counts_visible_changes_between_two_scans_only`).


### Task 5: UI scrollback 1,000 lines per attached pane, 10,000 kept for the History replay pane
**Covers:** audit item "UI scrollback 10,000 -> 1,000 to match the daemon" (old plan Task 14 / perf B10, reduced: 5,000 became 1,000, the grid buffer part is dropped, see below).
**Current state (verified 2026-10-28):**
- `crates/mkrew-ui/src/terminal/emulator.rs:29-30`: `const SCROLLBACK_LINES: usize = 10_000;`, used only in `Emulator::new` (`:153`, `scrolling_history`) and by tests (`:860`, `:911`, `:913`).
- The daemon keeps 1,000 lines since 740cc1f (`crates/mkrew-daemon/src/emulator.rs:18`, ADR 0082), and an attach or overflow snapshot carries at most those lines (`crates/mkrew-protocol/src/message.rs:1424`).
  A pane is re-attached after every tab or workspace switch, so the UI's extra 9,000 lines are lost at the first switch and cost RAM until then (about 29 MB at 120 columns per full pane, ADR 0082 cell size of 24 bytes).
- `paint()` (`crates/mkrew-protocol/src/paint.rs:23-31`) writes N history lines with CRLF plus `rows - 1` line feeds, so exactly N lines scroll into the receiving emulator's scrollback: a full daemon history of 1,000 fits a UI scrollback of 1,000 with nothing lost (pinned by a test below).
- `INCREMENTAL_SEARCH_LINES = 1_000` (`emulator.rs:33`, used by `search_nearby` `:263-265`): with a 1,000-line scrollback an attached pane's whole history is inside that window, so only a replay pane still sees a difference between a query edit and Enter.
  It stays (the replay pane needs it).
  The test `nearby_search_stops_short_of_old_history` (`emulator.rs:845-854`) would break: its `needle` line (written, then `INCREMENTAL_SEARCH_LINES + 10` lines) is pushed out of a 1,000-line history, so `search_next(true)` finds nothing.
  It must run on a replay emulator.
- **History replay shares the constant.** `TerminalPane::replay` (`crates/mkrew-ui/src/terminal/view.rs:155-169`) builds `Emulator::new(...)` and feeds the whole log tail (`shell/history.rs:339-362`, `history::TAIL_BYTES`).
  Left alone, the History dialog would show only the last 1,000 lines of a log tail (HUMAN_CHECKS 2026-10 "History": mouse-wheel scrollback, Ctrl+Shift+F find).
  The pane is read-only, one at a time, and dropped on close (`shell/history.rs:327, 388`), so it keeps 10,000 lines (29 MB at 120 columns while the dialog is open).
- Grid cache and row cache are already done: `Emulator::shared_grid` builds `grid()` once per change into `last_grid` (`emulator.rs:524-528`, `changed()` `:176-178`, since 4cbea62), and `RowCache` (`terminal/layout.rs:289-368`) re-lays out only changed rows (introduced in 1b53b53).
  The 60 fps feed cap (95a8a6e) bounds rebuilds to about 60 per second per pane.
- Skipped on purpose, both from old Task 14:
  - `zerowidth: Option<Box<[char]>>` (`GridCell.zerowidth: Vec<char>` at `emulator.rs:46`, filled at `:553-557`): `Vec::new()` and `to_vec()` of an empty slice do not allocate, so a cell allocates only when it really has a combining mark or ZWJ (rare).
    The change would save 8 bytes per cell and touch `layout.rs:83,108,114`, `Grid::to_text`, the `spike-term` copy; no allocation is removed.
    Not measurable, so not done.
  - The spare-grid buffer reuse (`spare` field, `fill_grid`): what is left per rebuild is `rows + 1` small allocations at most 60 times per second per visible pane; no profile shows it.
    Not done.
- `Cursor` now carries `shape: CursorShape` (`emulator.rs:73`), not `visible`; nothing here touches it.
- Web page: `crates/mkrew-cli/web/app.mjs:510` has `scrollback: 10000` with the comment "Like the desktop pane (SPEC §10)".
  That comment becomes false.
  The phone shows one session and its reader renders at most that many lines (`app.mjs:549`), so the value stays; only the comment changes.
- Docs with the old number: `docs/SPEC.md:955` (§10); `crates/mkrew-ui/tests/stress.rs:48-49` (`WARM_LINES = 10_100`); the `view_point` comment (`emulator.rs:519`, "the history 10k lines").
  ADR 0082 context (`:11`, "the UI's 10,000 lines"), ADR 0030 option 2, ADR 0052 and HUMAN_CHECKS records are history and are not rewritten; ADR 0082 gets one "Amended" line.

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/emulator.rs` (`SCROLLBACK_LINES`, new `REPLAY_SCROLLBACK_LINES`, `new`, new `for_replay` and private `with_scrollback`, `INCREMENTAL_SEARCH_LINES` doc, `view_point` comment, tests)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (`TerminalPane::replay`, one line)
- Modify: `crates/mkrew-ui/tests/stress.rs` (`WARM_LINES` and its comment)
- Modify: `crates/mkrew-cli/web/app.mjs` (comment only)
- Modify: `docs/SPEC.md` (§10 `:955`)
- Modify: `docs/decisions/0088-terminal-path-limits.md` (append a section; create if Task 4 has not run), `docs/decisions/0082-daemon-scrollback-1000-lines.md` (one line)
- Modify: `docs/HUMAN_CHECKS.md` (bullet in the shared perf section)
- Test: `crates/mkrew-ui/src/terminal/emulator.rs` `mod tests`

**Interfaces:**
- Consumes: `Emulator::new(cols: usize, rows: usize) -> Emulator` (unchanged signature, now 1,000 lines), `Emulator::from_snapshot(&ScreenSnapshot)`, `scroll_lines`, `display_offset`, the test helper `numbered_lines` (`emulator.rs:700`).
- Produces: `Emulator::for_replay(cols: usize, rows: usize) -> Emulator` (10,000 lines); private `fn with_scrollback(cols: usize, rows: usize, lines: usize) -> Emulator`; private `const REPLAY_SCROLLBACK_LINES: usize = 10_000`.

- [ ] **Step 1: Write the failing tests**

`crates/mkrew-ui/src/terminal/emulator.rs`, `mod tests`, right after `history_is_capped_at_the_scrollback_limit` (`:908-914`):

```rust
    /// ADR 0088: an attached pane keeps what the daemon keeps (1,000 lines), no more.
    #[test]
    fn an_attached_pane_scrolls_back_1000_lines() {
        let mut emu = Emulator::new(10, 2);
        numbered_lines(&mut emu, 1_500);
        emu.scroll_lines(1_000_000);
        assert_eq!(emu.display_offset(), 1_000);
    }

    /// A pane re-attached after the daemon's history is full (1,000 lines) keeps every line of it.
    #[test]
    fn a_full_daemon_history_fits_the_pane_scrollback() {
        let line = |text: String| {
            vec![CellRun {
                text,
                fg: TermColor::Default,
                bg: TermColor::Default,
                flags: 0,
            }]
        };
        let snapshot = ScreenSnapshot {
            cols: 10,
            rows: 3,
            lines: vec![vec![], vec![], vec![]],
            history: (0..1_000).map(|n| line(format!("old{n}"))).collect(),
            cursor: CursorSnapshot {
                row: 0,
                col: 0,
                visible: true,
            },
            modes: TermModes::default(),
        };
        let mut emu = Emulator::from_snapshot(&snapshot);
        emu.scroll_lines(1_000_000);
        assert_eq!(emu.display_offset(), 1_000);
        assert!(
            emu.grid().to_text().starts_with("old0\nold1\nold2\n"),
            "the oldest daemon line is still there"
        );
    }

    /// The History dialog replays a log tail with far more than 1,000 lines and scrolls through it.
    #[test]
    fn a_replay_pane_keeps_10000_lines() {
        let mut emu = Emulator::for_replay(10, 2);
        numbered_lines(&mut emu, 12_000);
        emu.scroll_lines(1_000_000);
        assert_eq!(emu.display_offset(), 10_000);
    }
```

Change the existing test (`:844-854`) so its history is longer than the window (replay pane), replacing its first line:

```rust
    #[test]
    fn nearby_search_stops_short_of_old_history() {
        // Only a replay pane has more history than the incremental window.
        let mut emu = Emulator::for_replay(10, 2);
        emu.advance(b"needle\r\n");
```
(the rest of the test is unchanged).

- [ ] **Step 2: Run it, expect FAIL**

Run: `cargo test -p mkrew-ui --lib -j 4 -- terminal::emulator`
Expected: FAIL to compile, `no function or associated item named for_replay found for struct Emulator`.
After Step 3 but before the constant change, `an_attached_pane_scrolls_back_1000_lines` fails with `left: 1500, right: 1000`; check that once if you implement in two commits, otherwise skip.

- [ ] **Step 3: Implement**

`crates/mkrew-ui/src/terminal/emulator.rs`, replace `:29-33`:

```rust
/// UI-side scrollback for attached panes: what the daemon keeps and a re-attach brings back
/// (ADR 0082, ADR 0088), so a larger number would only last until the next tab switch.
const SCROLLBACK_LINES: usize = 1_000;

/// Scrollback of a History replay pane ([`Emulator::for_replay`]): one pane at a time, dropped when
/// the dialog closes, fed the tail of a log that holds far more than 1,000 lines (ADR 0088).
const REPLAY_SCROLLBACK_LINES: usize = 10_000;

/// How far back a find-bar query edit looks (Enter searches the whole history). An attached pane's
/// history is no longer than this; only a replay pane sees the difference.
const INCREMENTAL_SEARCH_LINES: usize = 1_000;
```

Replace the head of `Emulator::new` (`:151-153`):

```rust
    pub fn new(cols: usize, rows: usize) -> Self {
        Self::with_scrollback(cols, rows, SCROLLBACK_LINES)
    }

    /// An emulator for a History replay pane, which keeps [`REPLAY_SCROLLBACK_LINES`].
    pub fn for_replay(cols: usize, rows: usize) -> Self {
        Self::with_scrollback(cols, rows, REPLAY_SCROLLBACK_LINES)
    }

    fn with_scrollback(cols: usize, rows: usize, lines: usize) -> Self {
        let config = Config {
            scrolling_history: lines,
```
(the rest of the old `new` body, from `default_cursor_style: CursorStyle {` on, is unchanged and now belongs to `with_scrollback`).

`view_point` comment (`:519`): replace `the history 10k lines` with `the history at most 10k lines`.

`crates/mkrew-ui/src/terminal/view.rs:163`, replace
```rust
        let mut emulator = Emulator::new(session.cols.max(1).into(), session.rows.max(1).into());
```
with
```rust
        let mut emulator =
            Emulator::for_replay(session.cols.max(1).into(), session.rows.max(1).into());
```

`crates/mkrew-ui/tests/stress.rs:48-49`, replace
```rust
/// A pane fed this many lines has a full scrollback (the UI emulator keeps 10,000) and screen.
const WARM_LINES: usize = 10_100;
```
with
```rust
/// A pane fed this many lines has a full scrollback (the UI emulator keeps 1,000) and screen.
const WARM_LINES: usize = 1_100;
```

`crates/mkrew-cli/web/app.mjs:509`, replace the comment only:
```js
    // One session on a phone, so more than the desktop pane's 1,000 (ADR 0088); the attach itself brings at most the daemon's 1,000 lines.
```

- [ ] **Step 4: Run tests, expect PASS**

Run: `cargo test -p mkrew-ui --lib -j 4 -- terminal`
Expected: PASS: the three new tests, `nearby_search_stops_short_of_old_history`, `history_is_capped_at_the_scrollback_limit`, `a_screen_full_of_matches_is_marked_cell_by_cell`, `snapshot_history_is_scrollable`, and the History test in `crates/mkrew-ui/src/history.rs` (`:348`, unchanged).
Then `cargo test -p mkrew-ui --test terminal_runs --test stress -j 4`: expect PASS (the memory and timing asserts of `stress` run in release only; in debug the lower `WARM_LINES` only makes the "scrollbacks full" sample earlier).
If `a_full_daemon_history_fits_the_pane_scrollback` fails with the top line `old1`, `paint()` scrolls one more line than the history has; then add the missing line to `SCROLLBACK_LINES` (a short comment) and say so in the ADR.
By the arithmetic of `paint.rs:23-31` (N CRLFs plus `rows - 1` line feeds scroll exactly N lines) it should not happen; [INFERENCE] until the test runs.

- [ ] **Step 5: SPEC / ADR / HUMAN_CHECKS**

`docs/SPEC.md:955` (§10 "Terminal rendering"), replace
```
  selection & copy, paste (bracketed), scrollback (UI-side, 10,000 lines, only for attached panes),
```
with
```
  selection & copy, paste (bracketed), scrollback (UI-side, 1,000 lines like the daemon's, only for
  attached panes; the History replay pane keeps 10,000; ADR 0088),
```

`docs/decisions/0088-terminal-path-limits.md`: Task 4 creates it. If it does not exist yet, start it with the header
```markdown
# 0088 - Terminal path: rate-limited screen scan, UI scrollback 1,000, history replay

- Date: 2026-10-28
- Status: accepted
- Milestone: perf 2026-10-28
```
Append this section at the end (Tasks 4 and 6 append theirs; keep the order Task 4, 5, 6):
```markdown
## UI scrollback

- An attached pane's UI emulator keeps 1,000 lines (was 10,000), the same as the daemon (ADR 0082).
  A pane is re-attached from the daemon snapshot after every tab or workspace switch, so the lines above 1,000 lived only until the next switch.
  A full history costs about 1,000 rows x cols x 24 bytes: about 2.9 MB at 120 columns, so six attached panes use about 17 MB instead of about 173 MB.
- The History replay pane (`TerminalPane::replay`) keeps 10,000 lines through `Emulator::for_replay`.
  It shows the tail of a session log with far more than 1,000 lines, there is one at a time, and it is dropped when the dialog closes.
- `INCREMENTAL_SEARCH_LINES` (1,000) stays: on an attached pane the whole history is now inside it, so only the replay pane sees the difference between a query edit and Enter.
- The mobile web page keeps 10,000 lines of xterm scrollback: one session, and the reader renders from it.
- Not done: a `zerowidth: Option<Box<[char]>>` grid cell and a reused grid buffer (old plan 2026-10-10 Task 14).
  `Vec::new()` does not allocate, so a cell allocates only for a real combining mark, and a rebuild is at most 60 per second per visible pane (`shared_grid`, 4cbea62); neither was measurable.
- Consequence: a pane scrolls back at most 1,000 lines, and Ctrl+Shift+F searches those; older output is in History (the session log).
```

`docs/decisions/0082-daemon-scrollback-1000-lines.md`, after the last bullet of `## Consequences` add:
```markdown
- Amended 2026-10-28 (ADR 0088): the UI scrollback of an attached pane is 1,000 lines too, so a re-attach no longer loses history.
```

`docs/HUMAN_CHECKS.md`: under `### To verify - 2026-10-28 perf (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)` (create it directly above the first `### To verify` heading, now `:24`, if the first task has not), append:
```markdown
- [ ] **Terminal scrollback 1,000 lines (ADR 0088):**
  - In a pwsh pane run `1..3000 | % { "line $_" }`, then Shift+PgUp until it stops: the oldest line is about `line 2000`-`2100`, and Ctrl+Shift+F finds `line 2900` but not `line 100`.
    Switch to another tab and back: the pane scrolls back the same amount, and nothing above the screen changed.
  - Open History (a finished session of more than 3,000 lines): the mouse wheel and Shift+PgUp go back far past 1,000 lines (up to 10,000), and Ctrl+Shift+F finds text from there.
  - With six panes flooding (the stress setup of T2.9) note the UI memory: it is well below the 10,000-line figure of that entry (about 100 MB for six full panes in the headless test process).
  - Phone page (`mk web`): scrolling back in the terminal still works past 1,000 lines.
```

- [ ] **Step 6: Verify and commit**

Run: `cargo test -p mkrew-ui --lib -j 4 -- terminal`, then `sh scripts/verify.sh` (omp shell `bash` is WSL).

```
git add crates/mkrew-ui/src/terminal/emulator.rs crates/mkrew-ui/src/terminal/view.rs crates/mkrew-ui/tests/stress.rs crates/mkrew-cli/web/app.mjs docs/decisions/0082-daemon-scrollback-1000-lines.md
git add -p docs/SPEC.md docs/HUMAN_CHECKS.md docs/decisions/0088-terminal-path-limits.md
git -c user.email=tungnv975@gmail.com commit -m "perf(ui): 1,000-line scrollback per attached pane like the daemon, 10,000 kept for the History replay pane"
```
(`view.rs` is shared with other terminal tasks: `git add -p` it if others' hunks are uncommitted.)

**Review focus for this task:** (1) a re-attach with a full daemon history must keep the oldest line (`a_full_daemon_history_fits_the_pane_scrollback`); (2) the History dialog must still scroll far past 1,000 lines (`a_replay_pane_keeps_10000_lines`), and an attached pane must stop at 1,000 (`an_attached_pane_scrolls_back_1000_lines`).


### Task 6: History replay: 1 MiB from a safe boundary, emulator built off the UI thread
**Covers:** audit item "opening an exited session in History stalls the UI" (old plan Task 15 / perf B12; the off-thread emulator is new)
**Current state (verified 2026-10-28):**
- `read_tail` and `Tail` live in `crates/mkrew-core/src/log_tail.rs:11-46` (moved there by c71282f). It reads from the end, and a cut stream drops only up to the first `\n` (`:42-44`). A cut tail with no `\n` is replayed from mid-sequence or mid-char: the bug is still there. No `replay_start` exists yet (`git log -S replay_start` finds only the old plan).
- `pub const TAIL_BYTES: u64 = 2 * 1024 * 1024;` is at `crates/mkrew-ui/src/history.rs:12`. Only `shell/history.rs:339` uses it.
- The file read is already off the UI thread: `Shell::load_history_log` (`crates/mkrew-ui/src/shell/history.rs:317-384`) calls `read_tail` inside `background.spawn` (`:336-342`).
- The stall is what runs next, on the UI thread inside `this.update_in` (`:345-381`): `cx.new(|cx| TerminalPane::replay(&selected, &tail.bytes, ...))` (`:362-371`). `TerminalPane::replay` (`crates/mkrew-ui/src/terminal/view.rs:155-169`) does `Emulator::new(..)` then `emulator.advance(log)` for the whole tail. 2 MiB of escape-heavy output through `Processor::advance` is the freeze.
- `Emulator` (`crates/mkrew-ui/src/terminal/emulator.rs:134-148`) holds `Term<VoidListener>`, `Processor<StdSyncHandler>`, `Option<RefCell<RegexSearch>>`, `RefCell<Option<Arc<Grid>>>`, `bool`, `Vec<u8>`. `Grid` is plain data. A grep of `alacritty_terminal-0.26.0/src/{term,grid,selection.rs}` and `vte-0.15.0/src/{lib,ansi}.rs` finds no `Rc`, no raw-pointer field, no `PhantomData`. Alacritty itself shares `Term` with its PTY thread. So `Emulator: Send` **[INFERENCE: not compiled]**. The compiler checks it for us: `BackgroundExecutor::spawn` needs a `Send` future and output, so Step 3 fails to compile if it is wrong. Fallback in that case: keep only (a) (see "Decision").
- `TerminalPane::replay` has one caller (`shell/history.rs:363`). Task 5 (PlanScrollback) lowers `SCROLLBACK_LINES` (`emulator.rs:30`) to 1,000 and adds `Emulator::for_replay(cols: usize, rows: usize) -> Self` (scrollback `REPLAY_SCROLLBACK_LINES = 10_000`); it changes the `Emulator::new` line of `replay` to `for_replay`. **Unverified, depends on Task 5 landing first.** This task builds on `for_replay` and adds no constant. The 3c "before" block below shows `Emulator::new` as in today's code; with Task 5 applied it reads `Emulator::for_replay`.
- The web relay reads the same tail with its own cap: `crates/mkrew-cli/src/web/relay.rs:40` `TAIL_BYTES = 512 * 1024`, via `spawn_blocking` (`:139`). Its comment at `:39` says the desktop shows 2 MiB: stale after this task.
- UI test `a_replayed_tail_shows_the_sessions_last_screen` (`crates/mkrew-ui/src/history.rs:339-351`): the 28-byte cut lands in the old file's last escape sequence, a `\n` follows. It stays valid and passes unchanged.
- Existing core test `read_tail_joins_the_parts_and_cuts_at_a_line_break` (`log_tail.rs:53`) stays valid.
- Wording that says 2 MiB / "stall one frame": `docs/HUMAN_CHECKS.md:2001` (`A log > 2 MB shows the "Older output is left out" note.`), `:2982` (`newest 2 MiB only, starting after the first line break past the cut;`), `:2985` (`parse the tail off the UI thread (a 2 MiB log may stall one frame)`). SPEC has no History-replay note except the §15 row `docs/SPEC.md:1638` (`| History | per-session output log, rotated at 5 MB |`).
- The in-dialog note (`shell/history.rs:764`, "Older output is left out; mk session history {} prints all of it.") stays correct.

**Decision (both, because both are small):**
- (a) Cap 1 MiB, cut at `replay_start`: halves the work and fixes the mid-sequence start. It is a pure function shared with `mk web`.
- (b) Build the `Emulator` and run `advance` inside the existing `background.spawn`, and hand the ready emulator to the pane. This removes the UI-thread stall at any size, so it is the part that fixes the bug. `TerminalPane::replay` takes the emulator instead of the bytes. No chunked advance across frames: not needed once (b) works, and it would add a state machine to the pane.
- If `Emulator` turns out not to be `Send` (the compiler says so in Step 3), do only (a) and say so in ADR 0088. Do not build chunked advance without asking.

**Files:**
- Modify: `crates/mkrew-core/src/log_tail.rs` (`replay_start`, `read_tail` cut, tests)
- Modify: `crates/mkrew-ui/src/history.rs` (`TAIL_BYTES`, doc)
- Modify: `crates/mkrew-ui/src/shell/history.rs` (`load_history_log`: build the emulator in the background closure)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (`TerminalPane::replay` takes an `Emulator`)
- Modify: `crates/mkrew-cli/src/web/relay.rs` (comment at `:39` only)
- Modify: `docs/SPEC.md` (§15 History row, `:1638`)
- Modify: `docs/decisions/0088-*.md` (append a section; the file is created by Task 4, its exact name is unverified)
- Modify: `docs/HUMAN_CHECKS.md` (`:2001`, `:2982`, `:2985` wording; bullets in the shared 2026-10-28 perf section)
- Test: `crates/mkrew-core/src/log_tail.rs` (`mod tests`)

**Interfaces:**
- Produces: `pub fn replay_start(bytes: &[u8]) -> usize` (`mkrew_core::log_tail::replay_start`).
- Changes: `pub const TAIL_BYTES: u64` (UI) = `1024 * 1024`.
- Changes: `TerminalPane::replay(session: &Session, emulator: Emulator, connection: Rc<Connection>, painted: async_channel::Sender<()>, window: &mut Window, cx: &mut Context<'_, Self>) -> Self` (was `log: &[u8]` in place of `emulator`).
- Keeps: `read_tail(files: &[PathBuf], limit: u64) -> io::Result<Tail>`.
- Consumes: `Emulator::for_replay(cols: usize, rows: usize)` (Task 5; today's code has `Emulator::new`, same arguments), `Emulator::advance(&mut self, &[u8])`, `Session { id, cols: u16, rows: u16, exit_code }`.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-core/src/log_tail.rs`, `mod tests`, after `read_tail_joins_the_parts_and_cuts_at_a_line_break`:

```rust
    #[test]
    fn replay_start_prefers_a_line_break_then_an_escape_then_a_char_boundary() {
        // The tail of "€" (E2 82 AC) cut after its first byte, then a whole line.
        assert_eq!(replay_start(b"\x82\xac rest\nnext"), 8);
        // A line break wins even when an escape sequence comes first.
        assert_eq!(replay_start(b"x\x1b[1my\nz"), 7);
        // No line break: the first ESC starts a whole sequence ("1;32m" is the end of one).
        assert_eq!(replay_start(b"1;32mgreen\x1b[0m"), 10);
        // Neither: only the continuation bytes of a cut char go.
        assert_eq!(replay_start(b"\x82\xacabc"), 2);
        assert_eq!(replay_start(b"\x82\xac"), 2);
        assert_eq!(replay_start(b"abc"), 0);
        assert_eq!(replay_start(b"abc\n"), 4);
        assert_eq!(replay_start(b""), 0);
    }

    /// Marks the bytes inside a CSI sequence (after its ESC, up to and including its final byte).
    fn inside_csi(stream: &[u8]) -> Vec<bool> {
        let mut inside = vec![false; stream.len()];
        let mut open = false;
        for (at, &byte) in stream.iter().enumerate() {
            if byte == 0x1b {
                open = true;
                continue;
            }
            if open {
                inside[at] = true;
                if (0x40..=0x7e).contains(&byte) && byte != b'[' {
                    open = false;
                }
            }
        }
        inside
    }

    /// SGR, a wide char, a 3-byte char, cursor moves, a last line without a break.
    const STREAM: &str = "\x1b[1;32mDone\x1b[0m \u{5f62} \u{20ac}5\r\n\x1b[2K\x1b[1Ahello\r\n\x1b[38;5;196mred\x1b[0m\r\nlast";

    #[test]
    fn a_replay_never_starts_inside_a_char_or_an_escape_sequence() {
        let stream = STREAM.as_bytes();
        let inside = inside_csi(stream);
        for cut in 0..=stream.len() {
            let start = cut + replay_start(&stream[cut..]);
            assert!(
                std::str::from_utf8(&stream[start..]).is_ok(),
                "cut {cut}: starts inside a char"
            );
            assert!(
                start == stream.len() || !inside[start],
                "cut {cut}: starts inside an escape sequence"
            );
        }
    }

    #[test]
    fn a_cut_log_without_line_breaks_starts_at_an_escape_sequence() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.log");
        std::fs::write(&log, b"\x1b[1mbold\x1b[0m long line no break").unwrap();
        // The last 26 bytes begin inside "bold": that text goes, the next sequence stays.
        let tail = read_tail(&[log], 26).unwrap();
        assert!(tail.cut);
        assert_eq!(tail.bytes, b"\x1b[0m long line no break");
    }

    #[test]
    fn a_big_log_is_read_as_whole_lines_within_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.log");
        std::fs::write(&log, b"0123456789abcdef\r\n".repeat(200_000)).unwrap();
        let limit = 1024 * 1024;
        let tail = read_tail(&[log], limit).unwrap();
        assert!(tail.cut);
        let len = tail.bytes.len() as u64;
        assert!(len <= limit && len > limit - 64, "{len} bytes");
        assert!(tail.bytes.starts_with(b"0123456789abcdef"));
    }
```
Byte check: `b"\x82\xac rest\n"` is 2 + 5 + 1 = 8 bytes before `next`; `b"x\x1b[1my\n"` is 7; `b"1;32mgreen"` is 10 bytes before the ESC; the 31-byte log of the third test minus its first 5 bytes (`ESC [ 1 m b`) is the 26 bytes `old` + `ESC[0m` + ` long line no break`.
There is no test for (b): it moves existing work to another thread. The compiler pins it (`spawn` needs `Send`), and `a_replayed_tail_shows_the_sessions_last_screen` still pins what the replay shows.

- [ ] **Step 2: Run it, expect FAIL**

Run: `cargo test -p mkrew-core --lib -j 4 -- log_tail`
Expected: FAIL to compile, `cannot find function replay_start in this scope`.

- [ ] **Step 3: Implement**

3a. `crates/mkrew-core/src/log_tail.rs`: replace the doc comment of `read_tail` (`:17-19`)
```rust
/// Reads the last `limit` bytes of `files` as one stream. A cut stream starts after its first
/// line break, so the replay does not begin inside an escape sequence. A file that is gone
/// (rotated away meanwhile) is skipped.
```
with
```rust
/// Where the replay of a cut log may start: just after its first line break (a UTF-8 char never
/// contains `\n`, and the partial line before it is dropped whole); with no line break, at the
/// first ESC (a whole escape sequence); with neither, after the continuation bytes of a cut
/// char. A fresh emulator starts with default attributes, so nothing needs resetting.
// ponytail: a tail with no line break and no ESC keeps its first bytes as they are (plain text,
// harmless); an ESC inside an OSC/DCS string would start there. Scan back from before the cut
// if a real log shows it.
pub fn replay_start(bytes: &[u8]) -> usize {
    if let Some(newline) = bytes.iter().position(|&byte| byte == b'\n') {
        return newline + 1;
    }
    if let Some(escape) = bytes.iter().position(|&byte| byte == 0x1b) {
        return escape;
    }
    // 0b10xx_xxxx: the continuation bytes of a UTF-8 char.
    bytes
        .iter()
        .position(|&byte| byte & 0xC0 != 0x80)
        .unwrap_or(bytes.len())
}

/// Reads the last `limit` bytes of `files` as one stream. A cut stream starts at
/// [`replay_start`], so the replay does not begin inside an escape sequence or a char. A file
/// that is gone (rotated away meanwhile) is skipped.
```
and in the body replace (`:42-44`)
```rust
    if cut && let Some(newline) = bytes.iter().position(|&byte| byte == b'\n') {
        bytes.drain(..=newline);
    }
```
with
```rust
    if cut {
        bytes.drain(..replay_start(&bytes));
    }
```

3b. `crates/mkrew-ui/src/history.rs:11-12`: replace
```rust
/// Most log bytes replayed (the newest ones); `mk session history <id>` prints all of it.
pub const TAIL_BYTES: u64 = 2 * 1024 * 1024;
```
with
```rust
/// Most log bytes replayed (the newest ones, ADR 0088); `mk session history <id>` prints all of it.
pub const TAIL_BYTES: u64 = 1024 * 1024;
```

3c. `crates/mkrew-ui/src/terminal/view.rs`: replace `TerminalPane::replay` (`:153-169`)
```rust
    /// A read-only pane replaying `log` (output of the finished `session`) on the session's
    /// screen size: never attached, takes no input and never resizes the session.
    pub fn replay(
        session: &Session,
        log: &[u8],
        connection: Rc<Connection>,
        painted: async_channel::Sender<()>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let mut emulator = Emulator::new(session.cols.max(1).into(), session.rows.max(1).into());
        emulator.advance(log);
        let exited = Some(session.exit_code);
        Self::with_emulator(
            session.id, emulator, exited, connection, painted, window, cx,
        )
    }
```
with
```rust
    /// A read-only pane showing `emulator`, which already replayed the output of the finished
    /// `session` on the session's screen size (built off the UI thread, ADR 0088): never
    /// attached, takes no input and never resizes the session.
    pub fn replay(
        session: &Session,
        emulator: Emulator,
        connection: Rc<Connection>,
        painted: async_channel::Sender<()>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let exited = Some(session.exit_code);
        Self::with_emulator(
            session.id, emulator, exited, connection, painted, window, cx,
        )
    }
```

3d. `crates/mkrew-ui/src/shell/history.rs` `load_history_log` (`:317-384`). After the `connection` guard (`:323-325`) and before `let serial = ...` (`:326`) add the size of the session (the dialog already lists it, `select` / `load` set `selected` first):
```rust
        let Some((cols, rows)) = (self.history.as_ref())
            .and_then(|dialog| dialog.selected_session())
            .filter(|selected| selected.id == session)
            .map(|selected| (selected.cols.max(1), selected.rows.max(1)))
        else {
            return;
        };
```
Replace the `let tail = match files.await { ... };` block (`:336-344`) with:
```rust
            // Read the log and replay it into the emulator on the background executor: the
            // advance over up to 1 MiB of output is what used to freeze the window.
            let replayed = match files.await {
                Ok(history) => background
                    .spawn(async move {
                        let tail = mkrew_core::log_tail::read_tail(&history.files, history::TAIL_BYTES)?;
                        let emulator = (!tail.bytes.is_empty()).then(|| {
                            let mut emulator = crate::terminal::emulator::Emulator::for_replay(
                                cols.into(),
                                rows.into(),
                            );
                            emulator.advance(&tail.bytes);
                            emulator
                        });
                        Ok::<_, std::io::Error>((tail.cut, emulator))
                    })
                    .await
                    .map_err(|error| format!("Cannot read the log: {error}")),
                Err(error) => Err(format!("{error:#}")),
            };
```
Replace the `match tail { ... }` (`:357-379`) with:
```rust
                match replayed {
                    Ok((_, None)) => dialog.log = Log::Empty,
                    Ok((cut, Some(emulator))) => {
                        dialog.log = Log::Shown { cut };
                        let painted = shell.painted.clone();
                        let pane = cx.new(|cx| {
                            TerminalPane::replay(
                                &selected, emulator, connection, painted, window, cx,
                            )
                        });
                        shell.history_pane = Some(pane);
                        shell.focus_pane(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%session, "session history failed: {error}");
                        dialog.log = Log::Failed(error);
                    }
                }
```
(`cols` / `rows` are `u16`, `for_replay` takes `usize`; `.into()` is the same widening `replay` did. `crate::*` is already imported at `shell/history.rs:3`; the full path above works because `view.rs` imports `crate::terminal::emulator::Emulator`.)
If `cargo check` says `Emulator` cannot be sent between threads, the error names the field: stop, drop 3c and 3d, keep 3a, 3b and the relay comment, and record "(b) not possible: `Emulator` is not `Send` because of <field>" in the ADR section instead of the (b) bullet.

3e. `crates/mkrew-cli/src/web/relay.rs:39`: replace `(the desktop History dialog shows 2 MiB)` with `(the desktop History dialog shows 1 MiB)`. The value `512 * 1024` stays (already below the new cap).

- [ ] **Step 4: Run tests, expect PASS**

Run: `cargo test -p mkrew-core --lib -j 4 -- log_tail`
Expected: PASS, the four new tests and `read_tail_joins_the_parts_and_cuts_at_a_line_break`.
Run: `cargo test -p mkrew-ui --lib -j 4 -- history` (this also compiles the `Send` bound of (b)) and `cargo test -p mkrew-cli -j 4 -- web`
Expected: PASS (`a_replayed_tail_shows_the_sessions_last_screen` unchanged).

- [ ] **Step 5: SPEC / ADR / HUMAN_CHECKS**

`docs/SPEC.md:1638`: replace
`| History | per-session output log, rotated at 5 MB |`
with
`| History | per-session output log, rotated at 5 MB; the History dialog replays its newest 1 MiB, built off the UI thread (ADR 0088) |`

`docs/decisions/0088-*.md` (created by Task 4; append after Task 5's section):
```markdown
## History replay (Task 6)

Opening an exited session in History stalled the window.
The log is read off the UI thread, but the replay (`Emulator::advance` over up to 2 MiB) ran inside `update_in` on it.

- The dialog replays the newest 1 MiB of the log (was 2 MiB, `history::TAIL_BYTES`).
  A cut stream starts after its first line break, else at the first ESC, else after the continuation bytes of a cut char (`mkrew_core::log_tail::replay_start`).
  So the emulator never starts inside a char or an escape sequence, also for a TUI log with no line break.
  `mk web` shares the function and keeps its own 512 KiB cap.
- The emulator is built and fed inside the `background_executor` task that reads the file.
  `TerminalPane::replay` takes the ready `Emulator`, so the UI thread only creates the entity.
  `Emulator` is `Send` (no `Rc`; `Term`, `Processor`, `RegexSearch` and `Grid` are plain data), checked by the compiler at the `spawn`.
- Rejected: advancing in chunks across frames.
  It keeps the work on the UI thread and needs a state machine in the pane, for no gain once the work is off it.
- Older output stays in the log (`mk session history`), as ADR 0030 says for output beyond the daemon's scrollback.
```

`docs/HUMAN_CHECKS.md`: at `:2001` replace `A log > 2 MB shows the "Older output is left out" note.` with `A log > 1 MB shows the "Older output is left out" note.`
At `:2982` replace `newest 2 MiB only, starting after the first line break past the cut;` with `newest 1 MiB only (ADR 0088), starting after the first line break past the cut (at the first escape sequence when there is none);`
At `:2985` replace `parse the tail off the UI thread (a 2 MiB log may stall one frame), a` with `a` (the replay now runs off the UI thread, ADR 0088); re-wrap the lines `:2985-2987` as needed.

Add under the shared section `### To verify - 2026-10-28 perf (plan docs/superpowers/plans/2026-10-28-perf-redraw-terminal.md)` (the file's headings use `-`, not an em dash; create the section directly above the first existing `### To verify -` heading, `:24`, if no earlier task did):
```markdown
- [ ] **History replay, no stall (ADR 0088):** in a pwsh pane run `1..60000 | % { "`e[32mline $_`e[0m" }` (a log of more than 1 MB), exit the session, open History and select it.
  The window keeps answering while it loads (move the mouse over the sidebar, press Esc and reopen), the "Older output is left out" note shows, the last line is `line 60000` in green, and after scrolling to the top the first line is a whole `line N` (no stray `[32m`, no `?` box at its start).
  Pick another session while one loads: only the last pick shows.
  A short session (a few KB) replays completely with no note. A full-screen TUI session (claude or omp) shows its last screen without garbage at the top.
```

- [ ] **Step 6: Verify and commit**

Run: `cargo test -p mkrew-core --lib -j 4 -- log_tail`, `cargo test -p mkrew-ui --lib -j 4 -- history`, `cargo test -p mkrew-cli -j 4 -- web` (expect PASS), then `sh scripts/verify.sh`.
```
git add crates/mkrew-core/src/log_tail.rs crates/mkrew-ui/src/history.rs crates/mkrew-ui/src/shell/history.rs crates/mkrew-ui/src/terminal/view.rs crates/mkrew-cli/src/web/relay.rs docs/decisions/0088-*.md
git add -p docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "perf(ui): History replays the newest 1 MiB from a safe boundary, built off the UI thread"
```

**Review focus for this task:** (1) a TUI log with no line break in its last 1 MiB must not start mid-escape or mid-char: `a_cut_log_without_line_breaks_starts_at_an_escape_sequence` and `a_replay_never_starts_inside_a_char_or_an_escape_sequence`. (2) Picking another session while one is still replaying must show only the last pick: the `selected.id == session` guard in `update_in` (`shell/history.rs:349-356`) is unchanged and not covered by a unit test (HUMAN_CHECKS bullet above).
