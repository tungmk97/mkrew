# Inbox 2026-10-10: Perf — UI Re-renders, Daemon Watcher and SQLite, Opt-in Turn Snapshots and Conflicts, Terminal Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Handle the four new perf items in `docs/INBOX.md` (lines 115–141). Each one carries out the choices the user ticked in `docs/PERF_REVIEW.md`:
- The UI stops re-rendering the whole window for data nobody sees, and stops recomputing the sidebar, the previews and the git summary on every frame or event.
- The daemon stops sending empty or too-frequent `git.changed`, releases watchers nobody uses, and writes less to SQLite (less fsync, nothing that is never read, pruning).
- Turn snapshots and conflict detection become opt-in (`[features]`, both off by default).
- The terminal path does less work: a rate-limited screen scan, a smaller scrollback, a reusable grid buffer, and a shorter log replay.

**Architecture:**
- Item 11 (Tasks 1–6, UI): a new pure `redraw` module decides from what the window draws (view, visible panes, focused session, inspector, sidebar mode, dialogs) whether a changed model needs a root render. Models always update; hidden data no longer re-renders the app. The sidebar view, the status bar's Inbox count and an id index are cached in `Sidebar` behind a revision counter (bumped on every mutating path) plus `Viewed::version`; time labels stay out of the cache. `Docs::apply(event, visible)` only marks a hidden doc stale. `FilesView::preview_step/preview_due` debounce the `.md` preview by 300 ms. `git_summary::Throttle` allows one summary refresh per 2 s, with a trailing refresh and an immediate one on a workspace switch. The 500 ms `settings.toml` poll becomes a check on window activation (ADR 0044). `Editor` keeps the widest-line index per text change.
- Item 12 (Tasks 7–9, daemon + store): no raw `hook` rows; `synchronous=NORMAL`; the backup runs on its own connection instead of under the store mutex; `files_changed` / `edited` / `hook` rows older than 7 days are pruned (`usage` stays). `git.changed` is sent only for batches with real changes. A batch waits up to 5 s while a session is working (tracked in a new in-memory `Hub::working` map that also replaces the per-batch `list_sessions`). Kept watchers are released through a new `watch.release` request (protocol 38, ADR 0042). `files_changed` is written only for Markdown batches.
- Item 13 (Tasks 10–12, settings + daemon + UI): `[features] turn_snapshots / conflict_detection` (both off) in the shared settings. The daemon reads them through `Hub::features()`, so a toggle applies from the next turn start or watcher batch. When off: no git snapshot, the turn-start hook is not held, and `record_edits` returns at once. The UI reads the same file where it already reloads settings and hides the Turns, Since-turn and Changed-by entries and conflict warnings. ADR 0041 and the SPEC record the new default.
- Item 14 (Tasks 13–15, terminal): the daemon scans a session's screen at most 4×/s (`scan::ScanRate`, pure, injected clock, with a trailing scan so the last screen is always classified; a queued `session.send` bypasses it). It builds only the 4 rows the state machine reads. The UI scrollback drops to 5,000 lines and the grid is rebuilt into the previous frame's buffer with `Option<Box<[char]>>` zero-width chars. History replays the newest 1 MiB of a log from a pure, tested safe cut (`replay_start`). ADR 0043 records the limits.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), alacritty_terminal 0.26, rusqlite, notify, serde, pulldown-cmark, mermaid-rs-renderer, insta. No new dependencies.

**Spec:** `docs/SPEC.md`, cited per task (mainly §5, §7, §10, §12A, §12B, §20.0 and the turn/undo/conflict sections). Source of the choices: `docs/PERF_REVIEW.md` (IDs A2–A6, B1–B12; A1 and A5 are "Giữ", so they are not in this plan). Open questions are at the end, each with the default this plan uses.

**Order:** tasks run in number order. Tasks 3–5 edit `Shell::dispatch` arms after Task 1. Tasks 10–12 come after Tasks 7–9: they all touch `crates/mkrew-daemon/src/session.rs`, Task 12's off-test relies on Task 9's Markdown-only `files_changed`, and Task 10 reuses Task 8's `Hub` fields. Task 11 re-reads `[features]` where Task 6 moved the UI settings reload (window activation). Every task names what it consumes in its **Interfaces** block.

**Inbox mapping:**

| Inbox line | Task |
| --- | --- |
| 115 `11. Perf: stop whole-app re-renders in the UI` | 1, 2, 3, 4, 5, 6 |
| 126 `12. Perf: daemon watcher, git.changed and SQLite` | 7, 8, 9 |
| 132 `13. Perf: turn snapshots and conflict detection become opt-in` | 10, 11, 12 |
| 138 `14. Perf: terminal path (daemon scan, scrollback, replay)` | 13, 14, 15 |

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1). Every agent difference lives in its TOML profile. There is no `match "claude"` anywhere in Rust code.
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change. ADR numbers in this plan: 0044 (Task 6, also amends ADR 0024), 0042 (Task 8), 0041 (Task 10; Task 12 amends it), 0043 (Task 13; Tasks 14–15 extend it). The numbers do not follow task order; that is intended. Before creating one, list `docs/decisions/` and take the next free number if the plan's number is taken (renumber the task's references).
- **Protocol version:** only Task 8 changes the wire format (`watch.release`). It bumps `PROTOCOL_VERSION` (`crates/mkrew-protocol/src/lib.rs:16`, 37 today) to 38. No other task changes the protocol.
- **Migrations:** none. Never delete a migration or a table.
- Few dependencies: add no new crates.
- GPUI rule: set `.hover()` at most once per element (`button()`/`quiet_button()`/`row_action()`/`menu_item()` already set it). `group_hover` is a separate slot and is safe.
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use; do not run `git commit`.
- Leave uncommitted human edits alone. Baseline is commit `e281607` plus the uncommitted `docs/INBOX.md`, `docs/PERF_REVIEW.md` and `docs/superpowers/specs/2026-10-10-ui-redesign-options.md`. Never touch `.github/workflows/*`, `packaging/`, `docs/BUILD_VI.md`, `design/`, `icon/`, `docs/superpowers/specs/`, `docs/PERF_REVIEW.md`, or the x11/wayland lines in `crates/mkrew-ui/Cargo.toml`.
- Never write files outside this repo. Edits to the user's own `<config>/mkrew/*` files (e.g. turning on `[features]`) are **user steps**, listed in HUMAN_CHECKS.
- **Inbox:** the four lines already read `[~] … → plan: docs/superpowers/plans/2026-10-10-inbox-perf.md Task …`. When the last task of an item passes verification, change its line from `[~]` to `[x]` (see the mapping above) and leave the plan suffix as it is. A task's tick step that says "add the plan suffix" is already done.
- **HUMAN_CHECKS:** Task 1 creates the section `### To verify — 2026-10-10 inbox: perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)` directly above the first existing `### To verify —` heading. Every later task appends its bullets to the end of that section.
- Verify on Windows with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these three in order:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace -j 4`
- The C: drive is nearly full: no `cargo clean`, no extra target dirs, no downloads. Check free space before a full build.
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions or the daemon.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.
- `crates/mkrew-ui/src/main.rs` is ~18k lines: read ranges, never the whole file. Line numbers below are from 2026-10-10 and drift as tasks land. Re-find each function by name (`grep -n "fn dispatch"`).
- UI work cannot be unit-tested headless: pure logic gets tests; every visible change gets a HUMAN_CHECKS smoke line that the user runs with `cargo run -p mkrew-ui`.
- Nothing in this plan was compiled while it was written. Where a task names an unverified name (it says so), check it against the code and adjust; do not invent a second convention.

## Review Focus

1. **A `done` or `needs_input` of a session you are not looking at (other workspace, Docs open, sidebar hidden).** Expected: the status bar `N need you`, the sidebar dots and the OS notification still update at once. Only usage/turn/plan/ports/queue/board data of hidden surfaces skips the redraw, and it is current when shown. Pinned by Task 1 `state_changes_redraw_even_when_nothing_else_is_shown`, `usage_redraws_for_a_visible_pane_only`, `plan_needs_the_inspector_or_the_full_sidebar_and_the_focused_session`.
2. **A prompt printed just after a screen scan, then silence.** Expected: the session still becomes `needs_input`, at the trailing scan ≤ 250 ms later, and a queued send never goes into a busy agent. Pinned by Task 13 `a_prompt_that_lands_inside_the_gap_is_classified_by_the_trailing_scan` and `a_flood_scans_at_most_four_times_a_second_and_the_last_batch_is_scanned`.
3. **A session that worked for a week, then idles.** Expected: its token totals survive pruning (`usage` rows are never pruned), and a non-Markdown write by one agent still attributes the turn's files to the working agent. Pinned by Task 7 `events_past_a_week_are_pruned_but_recent_ones_and_usage_stay`, Task 9 `the_only_working_session_gets_the_batch_for_its_turn`, `a_batch_without_markdown_writes_no_event`.
4. **`[features]` toggled while an agent is mid-turn, and a restore asked for while snapshots are off.** Expected: only turns that start afterwards get snapshots (no end snapshot without a start tree). A turn restore or "since turn" diff answers a clear `invalid_state` naming `turn_snapshots`, while a commit `since` still works. Conflict warnings clear at once when detection is switched off. Pinned by Task 10 `a_toggle_applies_from_the_next_turn`, `turn_snapshots_off_keeps_turns_but_takes_no_snapshot`, Task 12 `off_ignores_events_and_clears_warnings`, `conflict_detection_off_raises_no_conflict`.
5. **Pin, rename, color, archive, collapse, filter typing or viewing a `done` pane while the sidebar cache is warm.** Expected: the next frame shows it, the status bar count ignores the filter, and `17m` labels keep moving without dirtying. Pinned by Task 2 `the_cached_view_equals_a_fresh_one_after_every_kind_of_mutation`, `a_filter_or_chip_change_invalidates_the_cache`, `viewing_a_done_session_updates_the_cached_counts`, `time_labels_come_from_the_clock_not_from_the_cache`.

Also pinned inside their tasks: a doc changed while Docs is hidden (Task 3), a Files `.md` typing burst (Task 4), a git change burst and a workspace switch (Task 5), a released watcher with a live session (Task 8), stale grid cells and zero-width chars (Task 14), and a log cut inside a char or an escape sequence (Task 15). How smooth the app feels, and CPU with 2–3 working agents before and after, cannot be unit-tested; Task 1's HUMAN_CHECKS line covers it.

---

### Task 1: Daemon events stop re-rendering the whole app (perf B1)

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullet B1 and the CPU check; do not tick yet, Task 6 ticks it). Spec: §10 (SPEC.md:685-859), §11 (:861-905), §15 (:1233-1246, Idle CPU row :1239). No SPEC change and no ADR: no observable behavior changes, only when the root re-renders.

**Current state (investigated):**
- `Shell::dispatch` (`main.rs:8421`) feeds each `Incoming::Event` to `board`, `turns`, `usages`, `plans`, `ports`, `composers`, `follow`, `sidebar`, `conflicts`, every workspace's `tabs`, then the docs/files/git handlers. Six of them end in an unconditional `cx.notify()` when their data changed: `board.apply` (also calls `drop_gone_targets`), `turns.apply`, `usages.apply`, `plans.apply`, `ports.apply`, `composers.apply`; and `conflicts.prune(...) || detected`. Each `apply` already returns "something shown changed", but nothing asks whether the changed thing is on screen. The root `Shell::render` then rebuilds title bar, sidebar, tabs, view, dialogs and status bar.
- What each drawn surface reads (verified by grepping the field reads in `main.rs`):
  - `usages`: pane header `usage::context_share` (`render_pane`, ~16912), status bar `status_agent_fields` for `focused_session()` (~9105), inspector `inspector::session_card` (~16290, focused only), Board cards via `BoardCtx.usages` (~10721, `Usages::task_total`).
  - `turns`: pane header pill `pane_text::pill_text(.., self.turns.get(session))` (~16900). Nothing else (the Inbox card is gone, ADR from redesign Task 9).
  - `composers` queue (`session.queue`): pane header `⧗ N queued` and the composer panel (~16789, ~17001), per pane.
  - `plans`: sidebar progress bar for the selected row only (`session_row(.. progress ..)`, ~8765, full sidebar only: the compact rail draws no bar) and the inspector Plan card (`render_plan_card`, ~16191). Both are for the focused session.
  - `ports`: pane title `run::pane_title(info, self.ports.get(..))` (~16875), the title-bar Run button label through `run_summary(project_id)` for the active workspace (`render_run_button`, ~10235), and the hover tooltip of every workspace row in the full sidebar (`run_summary` + `summary.tooltip()`, ~8756).
  - `board` (`TaskUpdated` / `TaskDeleted`): Board view, the pane header breadcrumb folder (`pane_text::folder(.. self.board.task(..))`, ~16892) and the Files/Git worktree chooser (`target_chooser`, ~12243). The title bar, sidebar and status bar do not read it.
  - `conflicts`: pane header badge (~16779) and Board cards (`BoardCtx.conflicts`). Nowhere else.
  - `CommentUpdated` unconditionally notifies after updating `git` views' comments (~8490-8495); only the Git view of that project draws them (Docs re-lists in `doc_event`, Task 3's area). `AiGenerated` notifies unconditionally (~8506); it feeds the Git commit box and the pane composer.
- Events the sidebar handles (`SessionState`, `SessionExited`, `SessionUpdated`, `SessionTerminalTitle`, `ProjectUpdated`; `Sidebar::apply` returns `Change::Redraw`) feed the sidebar dots, the status bar Inbox count, tab urgency and `Viewed`: `Shell::render` marks the focused pane viewed only because every state change re-renders (`self.viewed.view(session)`, ~20513). These MUST keep re-rendering; this task does not touch them. OS notifications are sent by the daemon, not the UI, so they cannot go stale here.
- Every UI action that changes what is on screen (switch workspace / view / tab, toggle inspector, open a dialog) already calls `cx.notify()`. So a model that was updated silently while hidden is read fresh by the next render: skipping the notify for hidden data never shows stale values later.
- `focused_session()` is `None` unless the view is Terminals. `tabs::on_screen(window_visible, Option<&Tabs>)` (`tabs.rs:444`) lists the visible panes (zoomed pane alone). `sidebar::Layout` has `hidden()` / `compact()`. `modal_open()` (`main.rs:4470`) is true for every dialog.
- Tests: `mkrew-ui` unit tests live in `mod tests` of each module with a local `session(..)` fixture (`usage.rs:207`). `Event` structs: `SessionUsageEvent`, `SessionTurnEvent`, `SessionPlanEvent`, `SessionPortsEvent`, `SessionQueueEvent`, `TaskDeletedEvent`, `ConflictDetectedEvent`, `SessionStateEvent`, `SessionExitedEvent` (all in `mkrew-protocol/src/message.rs`).
- Name clash: `main.rs` already imports `mkrew_ui::docs::Shown`. The new type is only used as `redraw::Shown` there.

**Files:**
- Create: `crates/mkrew-ui/src/redraw.rs` (`SidebarMode`, `Shown`, `event_needs_redraw`, `conflicts_need_redraw`, tests)
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod redraw;` after `pub mod quick_open;`)
- Modify: `crates/mkrew-ui/src/main.rs` (`use mkrew_ui::redraw::{self, SidebarMode};`, new `Shell::shown`, `Shell::dispatch` model block and conflicts block)
- Modify: `docs/HUMAN_CHECKS.md` (creates the shared section)

**Interfaces:**
- Produces:
  ```rust
  pub enum SidebarMode { Hidden, Compact, Full }
  pub struct Shown {
      pub workspace: Option<Ulid>,   // active workspace
      pub view: View,                // board::View of the active workspace
      pub panes: Vec<Ulid>,          // sessions whose pane is drawn (Terminals view; zoom respected)
      pub focused: Option<Ulid>,     // Shell::focused_session()
      pub inspector: bool,           // the session panel is drawn
      pub sidebar: SidebarMode,
      pub modal: bool,               // any dialog is open
  }
  pub fn event_needs_redraw(event: &Event, shown: &Shown, sessions: &[Session]) -> bool
  pub fn conflicts_need_redraw(shown: &Shown) -> bool
  ```
- Consumes: `board::View`, `mkrew_protocol::{Event, Session}`, `tabs::on_screen`, `Shell::focused_session`, `Shell::modal_open`, `sidebar::Layout`.
- Contract: `event_needs_redraw` is asked only after a model reported that its data changed. It answers "does the window draw this data now"; events owned by another handler (state, exit, tabs, docs, git) answer `true` so a future reader of the match sees they are not decided here.

- [ ] **Step 1: Failing tests**

Create `crates/mkrew-ui/src/redraw.rs` containing only the test module below, and in `lib.rs` add `pub mod redraw;` after `pub mod quick_open;`.

```rust
#[cfg(test)]
mod tests {
    use mkrew_protocol::{
        ConflictDetectedEvent, SessionExitedEvent, SessionKind, SessionPlanEvent,
        SessionPortsEvent, SessionQueueEvent, SessionState, SessionStateEvent, SessionTurnEvent,
        SessionUsageEvent, StateSource, TaskDeletedEvent,
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

    /// Terminals view of `ws`, `a` focused, inspector open, compact rail.
    fn shown(w: &World) -> Shown {
        Shown {
            workspace: Some(w.ws),
            view: View::Terminals,
            panes: vec![w.a()],
            focused: Some(w.a()),
            inspector: true,
            sidebar: SidebarMode::Compact,
            modal: false,
        }
    }

    /// Nothing of any session is drawn except the sidebar's own rows.
    fn nothing_shown(w: &World) -> Shown {
        Shown {
            workspace: Some(w.ws),
            view: View::Docs,
            panes: Vec::new(),
            focused: None,
            inspector: false,
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

    fn redraws(w: &World, shown: &Shown, event: &Event) -> bool {
        event_needs_redraw(event, shown, &w.sessions)
    }

    /// The focused pane's usage feeds the status bar, the inspector and its header; a session
    /// of another tab or workspace feeds nothing on screen.
    #[test]
    fn usage_redraws_for_a_visible_pane_only() {
        let w = world();
        let mut s = shown(&w);
        assert!(redraws(&w, &s, &usage(w.a())), "focused pane");
        assert!(!redraws(&w, &s, &usage(w.b())), "hidden tab, inspector on another session");
        assert!(!redraws(&w, &s, &usage(w.c())), "other workspace");
        // A split pane shows its own context share in its header.
        s.panes.push(w.b());
        assert!(redraws(&w, &s, &usage(w.b())));
    }

    /// Board cards show the tokens of their task's session; the Docs view shows none.
    #[test]
    fn usage_redraws_the_board_of_the_shown_workspace_only() {
        let w = world();
        let mut s = nothing_shown(&w);
        s.view = View::Board;
        assert!(redraws(&w, &s, &usage(w.b())));
        assert!(!redraws(&w, &s, &usage(w.c())), "another workspace's board is not drawn");
        s.view = View::Docs;
        assert!(!redraws(&w, &s, &usage(w.b())));
    }

    #[test]
    fn turns_and_queues_need_a_visible_pane() {
        let w = world();
        let s = shown(&w);
        for event in [turn(w.a()), queue(w.a())] {
            assert!(redraws(&w, &s, &event), "{event:?}");
        }
        for event in [turn(w.b()), queue(w.b()), turn(w.c()), queue(w.c())] {
            assert!(!redraws(&w, &s, &event), "{event:?}");
        }
    }

    /// The plan is drawn for the focused session only, in the inspector or on the selected
    /// row of the full sidebar.
    #[test]
    fn plan_needs_the_inspector_or_the_full_sidebar_and_the_focused_session() {
        let w = world();
        let mut s = shown(&w);
        assert!(redraws(&w, &s, &plan(w.a())));
        assert!(!redraws(&w, &s, &plan(w.b())), "not the focused session");
        s.inspector = false;
        assert!(!redraws(&w, &s, &plan(w.a())), "inspector closed, compact rail has no bar");
        s.sidebar = SidebarMode::Full;
        assert!(redraws(&w, &s, &plan(w.a())), "the selected row's progress bar");
    }

    /// A run's ports are in its pane title, in the Run button of the shown workspace and in
    /// the hover tip of every workspace row of the full sidebar.
    #[test]
    fn ports_show_in_the_run_button_and_the_full_sidebars_tips() {
        let w = world();
        let mut s = nothing_shown(&w);
        assert!(redraws(&w, &s, &ports(w.b())), "Run button of the shown workspace");
        assert!(!redraws(&w, &s, &ports(w.c())), "other workspace, no sidebar rows");
        s.sidebar = SidebarMode::Compact;
        assert!(!redraws(&w, &s, &ports(w.c())), "the rail has no run tip");
        s.sidebar = SidebarMode::Full;
        assert!(redraws(&w, &s, &ports(w.c())), "its workspace row's tooltip");
    }

    /// The Inbox count, the sidebar dots and `Viewed` depend on these: never skipped.
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
    }

    #[test]
    fn task_changes_redraw_the_views_that_read_tasks() {
        let w = world();
        let event = |project_id| {
            Event::TaskDeleted(TaskDeletedEvent {
                task_id: Ulid::new(),
                project_id,
            })
        };
        let mut s = shown(&w);
        for view in [View::Board, View::Terminals, View::Files, View::Git] {
            s.view = view;
            assert!(redraws(&w, &s, &event(w.ws)), "{view:?}");
            assert!(!redraws(&w, &s, &event(w.other)), "{view:?}: another workspace");
        }
        s.view = View::Docs;
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
        let mut s = shown(&w);
        for view in [View::Board, View::Terminals] {
            s.view = view;
            assert!(conflicts_need_redraw(&s) && redraws(&w, &s, &detected), "{view:?}");
        }
        for view in [View::Docs, View::Files, View::Git] {
            s.view = view;
            assert!(!conflicts_need_redraw(&s) && !redraws(&w, &s, &detected), "{view:?}");
        }
    }
}
```

Run: `cargo test -p mkrew-ui --lib redraw -j 4`
Expected: FAIL to compile (`Shown`, `SidebarMode`, `event_needs_redraw`, `View` are not defined).

- [ ] **Step 2: The pure decision**

Put this above the test module in `redraw.rs`:

```rust
//! Which daemon events make the root `Shell` render again (perf review B1). A model (usage,
//! turns, plan, ports, queue, board, conflicts) always keeps its data; the root only needs to
//! redraw when the window draws that data now. Anything that changes what is on screen (a view,
//! workspace, tab or inspector switch, a dialog) re-renders on its own, so data skipped while
//! hidden is read fresh when it is shown. Pure and tested headlessly; `main.rs` builds
//! [`Shown`] once per batch.

use mkrew_protocol::{Event, Session};
use ulid::Ulid;

use crate::board::View;

/// How the sidebar is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    Hidden,
    /// The rail: no progress bar, no run tooltips.
    Compact,
    Full,
}

/// What the window draws now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub workspace: Option<Ulid>,
    pub view: View,
    /// Sessions whose pane is drawn: the active tab's visible panes in the Terminals view.
    pub panes: Vec<Ulid>,
    pub focused: Option<Ulid>,
    pub inspector: bool,
    pub sidebar: SidebarMode,
    /// Some dialog is open.
    pub modal: bool,
}

/// Whether an event whose model reported "my data changed" must re-render the root. Events
/// owned by another handler (sidebar state, tabs, docs, git) answer `true`: they are never
/// decided here, and their handlers keep their own redraw rules.
pub fn event_needs_redraw(event: &Event, shown: &Shown, sessions: &[Session]) -> bool {
    // Dialogs read models of their own (compare, history, task editor): keep today's behavior.
    if shown.modal {
        return true;
    }
    let in_workspace = |project_id: Ulid| shown.workspace == Some(project_id);
    let project_of = |session_id: Ulid| {
        sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(|s| s.project_id)
    };
    // The status bar and the inspector read the focused pane, the headers every visible one.
    let on_pane = |id: Ulid| shown.focused == Some(id) || shown.panes.contains(&id);
    match event {
        // Pane header, status bar, inspector; Board cards show their task's tokens.
        Event::SessionUsage(e) => {
            on_pane(e.session_id)
                || (shown.view == View::Board && project_of(e.session_id).is_some_and(in_workspace))
        }
        // Pane header pill and `⧗ N queued`.
        Event::SessionTurn(e) => on_pane(e.session_id),
        Event::SessionQueue(e) => on_pane(e.session_id),
        // Inspector Plan card, or the selected row's bar in the full sidebar.
        Event::SessionPlan(e) => {
            shown.focused == Some(e.session_id)
                && (shown.inspector || shown.sidebar == SidebarMode::Full)
        }
        // Pane title, the Run button of the shown workspace, every row tip of the full sidebar.
        Event::SessionPorts(e) => {
            on_pane(e.session_id)
                || shown.sidebar == SidebarMode::Full
                || project_of(e.session_id).is_some_and(in_workspace)
        }
        Event::TaskUpdated(task) => tasks_drawn(shown, task.project_id),
        Event::TaskDeleted(deleted) => tasks_drawn(shown, deleted.project_id),
        Event::ConflictDetected(_) => conflicts_need_redraw(shown),
        // Review comments of the Git view; Docs re-lists in its own handler.
        Event::CommentUpdated(comment) => {
            shown.view == View::Git && in_workspace(comment.project_id)
        }
        // The commit message box (Git) and the pane composer (Terminals).
        Event::AiGenerated(_) => matches!(shown.view, View::Git | View::Terminals),
        Event::SessionState(_)
        | Event::SessionExited(_)
        | Event::SessionUpdated(_)
        | Event::SessionTerminalTitle(_)
        | Event::ProjectUpdated(_)
        | Event::TabUpdated(_)
        | Event::SessionSnapshot(_)
        | Event::NotificationClicked(_)
        | Event::SessionEdited(_)
        | Event::DocChanged(_)
        | Event::GitChanged(_) => true,
    }
}

/// Conflict warnings are drawn on Board cards and in pane headers.
pub fn conflicts_need_redraw(shown: &Shown) -> bool {
    shown.modal || matches!(shown.view, View::Board | View::Terminals)
}

/// Tasks are read by the Board, the pane header breadcrumb and the Files / Git worktree chooser
/// of the shown workspace.
fn tasks_drawn(shown: &Shown, project_id: Ulid) -> bool {
    shown.workspace == Some(project_id)
        && matches!(
            shown.view,
            View::Board | View::Terminals | View::Files | View::Git
        )
}
```

Run: `cargo test -p mkrew-ui --lib redraw -j 4`
Expected: PASS (9 tests).

- [ ] **Step 3: Use it in `dispatch`**

`main.rs`: next to the other `use mkrew_ui::…` lines (find with `grep -n "^use mkrew_ui::quick_open" crates/mkrew-ui/src/main.rs`), add in alphabetical place `use mkrew_ui::redraw::{self, SidebarMode};`.

Add this method right before `fn dispatch(` (line ~8420, after `send_peek`):

```rust
    /// What the window draws now, for [`redraw::event_needs_redraw`].
    fn shown(&self, window: &Window) -> redraw::Shown {
        let terminals = self.view() == View::Terminals;
        let tabs = self
            .workspace
            .filter(|_| terminals)
            .and_then(|id| self.tabs.get(&id));
        redraw::Shown {
            workspace: self.workspace,
            view: self.view(),
            panes: on_screen(window.is_visible(), tabs),
            focused: self.focused_session(),
            inspector: self.inspector_open && terminals,
            sidebar: if self.sidebar_layout.hidden() {
                SidebarMode::Hidden
            } else if self.sidebar_layout.compact() {
                SidebarMode::Compact
            } else {
                SidebarMode::Full
            },
            modal: self.modal_open(),
        }
    }
```

In `dispatch`, replace

```rust
        let mut peek_changed = false;
        for incoming in batch {
```
with
```rust
        let mut peek_changed = false;
        // One reading per batch. A batch that switches workspace or view (a notification click)
        // notifies on its own, and the render after it reads every model, so a stale `shown`
        // can only cost one redraw, never a stale screen.
        let shown = self.shown(window);
        for incoming in batch {
```

Replace

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
                if data_changed
                    && redraw::event_needs_redraw(event, &shown, self.sidebar.sessions())
                {
                    cx.notify();
                }
```

Replace

```rust
                let detected = self.conflicts.apply(event);
                if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected {
                    cx.notify();
                }
```
with
```rust
                let detected = self.conflicts.apply(event);
                // `prune` first: it must run on every event.
                let pruned = self.conflicts.prune(&self.board, self.sidebar.sessions());
                if (pruned || detected) && redraw::conflicts_need_redraw(&shown) {
                    cx.notify();
                }
```

Replace the two unconditional notifies of the comment and AI arms:

```rust
                    for view in self.git.values_mut() {
                        view.comments.apply_event(view.project_id, comment);
                    }
                    cx.notify();
```
with
```rust
                    for view in self.git.values_mut() {
                        view.comments.apply_event(view.project_id, comment);
                    }
                    if redraw::event_needs_redraw(event, &shown, self.sidebar.sessions()) {
                        cx.notify();
                    }
```
and
```rust
                    self.composers.enhanced(done.generation_id, result);
                    cx.notify();
```
with
```rust
                    self.composers.enhanced(done.generation_id, result);
                    if redraw::event_needs_redraw(event, &shown, self.sidebar.sessions()) {
                        cx.notify();
                    }
```
Do not touch the `sidebar.apply` match (`Change::Redraw` / `Refresh`), the `layout_changed` / `peek` notifies, or the docs/files/git handlers (Task 3).

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors.

- [ ] **Step 4: HUMAN_CHECKS (creates the shared section)**

In `docs/HUMAN_CHECKS.md`, directly above the first `### To verify —` heading (find it with `grep -n "^### To verify" docs/HUMAN_CHECKS.md`; line 18 today, and above any section another plan of 2026-10-10 already put there), add:

```markdown
### To verify — 2026-10-10 inbox: perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)

- [ ] **Events only redraw what is shown (Task 1):** 2 workspaces, an agent in each, inspector open (`◨`) on workspace A's agent.
  - Let workspace B's agent finish turns: A's status bar, ctx %, inspector and panes do not flicker, and still update live when A's own agent ends a turn (tokens and `ctx` change at once).
  - Open the Board: a card's token total updates live when its agent ends a turn. Switch to Docs, let the agent end another turn, come back to Board: the total is current (nothing stale after a view switch).
  - Link a plan (`.md` checklist) to the focused agent: its Plan card and the selected sidebar row's bar update live while you tick items; close the inspector, tick more, reopen it: current.
  - Start a run command that prints a port: the title bar Run label shows `:PORT`; in the full sidebar, hover another workspace's row (a run live there) and the tip lists its ports.
  - The `N need you` count, the sidebar state dots and the OS notifications still follow every agent, also with the window on another workspace, on Docs, and with the sidebar hidden. Focus a `done` pane: it leaves the count at once.
- [ ] **CPU (Task 1 / item 11):** open Task Manager → Details → `mkrew.exe`, run 3 agents in `working` (long tasks) with the window visible on Terminals, and read CPU over 60 s. Before: build of the commit before this plan (copy its `target\release\mkrew.exe` aside first). After: the build after Task 6. Write the numbers here: before ___ % / after ___ % of one core. (The 500 ms working-dot tick, A1, stays, so the drop is the avoided full renders from other sessions' events, not the dots.)
```

- [ ] **Step 5: Verify and tick**

Run: `cargo test -p mkrew-ui --lib redraw -j 4` (expect PASS), then the full verification (`bash scripts/verify.sh`, or fmt, clippy, `cargo test -j 2`). Do not tick inbox line 115 (Tasks 2–6 follow).
Suggested message: `perf(ui): daemon events redraw the root only when the data is on screen`

---

### Task 2: Sidebar, Inbox count and tab bar stop recomputing every render (perf B2)

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullet B2; do not tick yet). Spec: §10 (SPEC.md:685-859), §11 (:861-905, the Inbox / "Needs you" definition). No SPEC change and no ADR: the rows, counts and texts are identical.

**Current state (investigated):**
- `Sidebar::view` (`sidebar.rs:452`) runs on every render: `grouped()` lowercases the filter, builds a `HashMap<Ulid, Vec<&Session>>`, filters by query (`matches` lowercases three fields per session) and sorts each workspace; then counts, `workspace_summary` `format!`s, and every row is built with cloned `String`s. `render_sidebar` (`main.rs:8577`) calls it once per render; `render_compact_sidebar` (`:8880`) calls it instead of it (never both); the pane-lifecycle check (`:8244`) calls it again per call.
- `inbox_entries()` (`main.rs:8078`) = `inbox::entries(..)`: clones every needing session's strings and sorts, and the status bar (`render_status_bar`, `:8976`) only takes `.len()`. `inbox::entries` also leaves out sessions whose workspace is not listed (archived), so `len()` is not "sessions with `needs_you`".
- `SessionRow` holds `state_changed_at`, not text; the right-column time (`17m`) is produced at draw time by `SessionRow::time_text(now_ms)`. `WorkspaceRow.summary` has counts, no time. So the cacheable part is already time-free, and `status::redraw_due` keeps repainting on the 500 ms tick without any cache dirtying.
- `Viewed` (`inbox.rs:29`) changes during render: `Shell::render` calls `self.viewed.view(session)` for the focused pane before drawing, which flips that session's `needs_you`. It is an input of both the rows' chip count and the Inbox count, so the cache must key on it.
- Inputs of the view and every mutation path in `Sidebar` (all are `&mut self` methods of `Sidebar`; fields are private): `load` (also replays events), `add_project`, `toggle`, `filter_mut` (hands out `&mut Input`), `set_chip`, `toggle_all`, `reveal`, `clear_filter`, `remove_session`, `set_title`, `set_color`, `update_session` (reached from `apply` for `SessionState`, `SessionExited`, `SessionUpdated` (pin, rename, plan link, color), `SessionTerminalTitle`), `upsert_project` (from `apply(ProjectUpdated)` and `add_project`: color, pin, rename, archive/hidden), `sort_projects`. `apply` for `SessionUsage` / `Turn` / `Plan` / `Ports` / `Queue` / `Edited` / `Tab*` / `Task*` returns `Change::None` and changes nothing the view reads: those must not dirty the cache.
- Tab bar (`render_tab_bar`, `main.rs:16442-16444`): per tab `status::most_urgent(sessions, &tab.sessions())` does `sessions.iter().find` per pane, and `tabs::tab_kind(tab, sessions)` another `find`: O(panes × sessions) per frame. `Shell::render` (`:20514`), `render_status_bar` (`:9017`), `render_inspector` (`:16056`) and `render_pane` (`:16746`) each do one more `sidebar.sessions().iter().find(..)` per frame.
- Tests: `sidebar.rs` `mod tests` has `project(name)`, `session(project, started_at, state)`, `workspace(row)`, `session_ids(rows)`; `inbox.rs` has `project`, `session(project, state, changed_at)`; `status.rs:210` and `tabs.rs:1037` pin `most_urgent` / `tab_kind` with slices. `crates/mkrew-ui/tests/lifecycle.rs:168` calls `sidebar.rows()`.

**Files:**
- Modify: `crates/mkrew-ui/src/inbox.rs` (`Viewed.version`, `Viewed::version`, `inbox::count`, tests)
- Modify: `crates/mkrew-ui/src/sidebar.rs` (`rev`, caches, `view` → `Rc<View>`, `waiting`, `session`, `build`, `touch`, tests)
- Modify: `crates/mkrew-ui/src/status.rs` (`most_urgent` takes a lookup, test call sites)
- Modify: `crates/mkrew-ui/src/tabs.rs` (`tab_kind` takes a lookup, test call sites)
- Modify: `crates/mkrew-ui/src/main.rs` (`render_sidebar`, `render_compact_sidebar`, `render_status_bar`, `render_tab_bar`, `render_inspector`, `render_pane`, `Shell::render`)
- Modify: `docs/HUMAN_CHECKS.md` (append to the Task 1 section)

**Interfaces:**
- Produces:
  ```rust
  // inbox.rs
  impl Viewed { pub fn version(&self) -> u64 }            // changes only when a viewed `done` changes
  pub fn count(projects: &[Project], sessions: &[Session], viewed: &Viewed) -> usize
  // sidebar.rs
  impl Sidebar {
      pub fn view(&self, viewed: &Viewed) -> Rc<View>     // was: -> View
      pub fn waiting(&self, viewed: &Viewed) -> usize     // the status bar's Inbox count
      pub fn session(&self, id: Ulid) -> Option<&Session> // indexed, replaces per-frame `.iter().find`
  }
  // status.rs
  pub fn most_urgent<'a>(ids: &[Ulid], find: impl Fn(Ulid) -> Option<&'a Session>) -> Option<&'a Session>
  // tabs.rs
  pub fn tab_kind<'a>(tab: &Tab, find: impl Fn(Ulid) -> Option<&'a Session>) -> TabKind
  ```
- `View` gains `PartialEq, Eq` (tests compare a cached view with a fresh one).
- How time stays correct: the cache never holds a time. `SessionRow` keeps `state_changed_at`; the draw calls `time_text(now)` every frame, and `status::redraw_due` on the 500 ms tick still decides when to repaint. Nothing about time dirties the cache.

- [ ] **Step 1: Failing tests**

`inbox.rs` `mod tests`, at the end:

```rust
    /// The status bar's number is the Inbox's length, whatever the sidebar filter says:
    /// needs input, unseen done; not seen done, working, exited, nor a workspace that is not
    /// listed (archived).
    #[test]
    fn the_count_is_the_inbox_length() {
        let (api, web, archived) = (project("api"), project("web"), project("gone"));
        let seen = session(&api, SessionState::Done, 5);
        let mut viewed = Viewed::default();
        viewed.view(&seen);
        let sessions = vec![
            session(&api, SessionState::NeedsInput, 1),
            session(&web, SessionState::Done, 2),
            seen,
            session(&api, SessionState::Working, 3),
            session(&api, SessionState::Exited, 4),
            session(&archived, SessionState::NeedsInput, 6),
        ];
        let projects = [api, web];
        assert_eq!(count(&projects, &sessions, &viewed), 2);
        assert_eq!(
            count(&projects, &sessions, &viewed),
            entries(&projects, &sessions, &viewed).len()
        );
    }

    /// The sidebar caches on this number: it moves when a `done` becomes viewed (or a later
    /// `done` of the same session is viewed), not when the same view is repeated every frame.
    #[test]
    fn viewing_moves_the_version_only_when_a_viewed_done_changes() {
        let api = project("api");
        let mut done = session(&api, SessionState::Done, 5);
        let mut viewed = Viewed::default();
        let start = viewed.version();
        viewed.view(&session(&api, SessionState::Working, 1));
        assert_eq!(viewed.version(), start, "only a done is recorded");
        viewed.view(&done);
        let first = viewed.version();
        assert_ne!(first, start);
        viewed.view(&done);
        assert_eq!(viewed.version(), first, "the same done again changes nothing");
        done.state_changed_at = 9;
        viewed.view(&done);
        assert_ne!(viewed.version(), first, "a later done is viewed anew");
    }
```

`sidebar.rs`: in `mod tests` change the `use mkrew_protocol::{SessionExitedEvent, SessionStateEvent};` line to

```rust
    use mkrew_protocol::{
        SessionExitedEvent, SessionPortsEvent, SessionStateEvent, SessionTerminalTitleEvent,
    };
```
and add at the end of `mod tests`:

```rust
    /// Two workspaces (api: a working, b idle; web: c done, titled "login"), all expanded.
    fn fixture() -> (Sidebar, Vec<Project>, Vec<Session>) {
        let (api, web) = (project("api"), project("web"));
        let a = session(&api, 1_000, SessionState::Working);
        let b = session(&api, 2_000, SessionState::Idle);
        let mut c = session(&web, 3_000, SessionState::Done);
        c.title = Some("login".into());
        let mut sidebar = Sidebar::default();
        sidebar.load(
            vec![api.clone(), web.clone()],
            vec![a.clone(), b.clone(), c.clone()],
        );
        (sidebar, vec![api, web], vec![a, b, c])
    }

    type Mutation = fn(&mut Sidebar, &[Project], &[Session]);

    /// Whatever changes what the sidebar shows (every `&mut` path of `Sidebar`, plus the
    /// events it applies) is seen on the next `view`: the cached view always equals a fresh
    /// compute, and so does the status bar's count.
    #[test]
    fn the_cached_view_equals_a_fresh_one_after_every_kind_of_mutation() {
        // (name, whether the rows or counts visibly change, mutation)
        let cases: Vec<(&str, bool, Mutation)> = vec![
            ("state event", true, |s, _, ss| {
                s.apply(&Event::SessionState(SessionStateEvent {
                    session_id: ss[0].id,
                    state: SessionState::NeedsInput,
                    source: StateSource::Hook,
                    changed_at: 9_000,
                }));
            }),
            ("exit", true, |s, _, ss| {
                s.apply(&Event::SessionExited(SessionExitedEvent {
                    session_id: ss[0].id,
                    exit_code: Some(0),
                }));
            }),
            ("pin a session", true, |s, _, ss| {
                s.apply(&Event::SessionUpdated(Box::new(Session {
                    pinned: true,
                    ..ss[1].clone()
                })));
            }),
            ("rename by event", true, |s, _, ss| {
                s.apply(&Event::SessionUpdated(Box::new(Session {
                    title: Some("renamed".into()),
                    ..ss[0].clone()
                })));
            }),
            ("rename by reply", true, |s, _, ss| {
                s.set_title(ss[0].id, Some("fix".into()));
            }),
            ("terminal title", true, |s, _, ss| {
                s.apply(&Event::SessionTerminalTitle(SessionTerminalTitleEvent {
                    session_id: ss[0].id,
                    title: Some("vim".into()),
                }));
            }),
            ("session color (not drawn in rows)", false, |s, _, ss| {
                s.set_color(ss[0].id, Some("teal".into()));
            }),
            ("pin a workspace", true, |s, ps, _| {
                s.apply(&Event::ProjectUpdated(Project {
                    pinned: true,
                    ..ps[1].clone()
                }));
            }),
            ("workspace color", true, |s, ps, _| {
                s.apply(&Event::ProjectUpdated(Project {
                    color: Some("teal".into()),
                    ..ps[0].clone()
                }));
            }),
            ("hide (archive) a workspace", true, |s, ps, _| {
                s.apply(&Event::ProjectUpdated(Project {
                    archived: true,
                    ..ps[1].clone()
                }));
            }),
            ("collapse one", true, |s, ps, _| s.toggle(ps[0].id)),
            ("collapse all", true, |s, _, _| s.toggle_all()),
            ("chip", true, |s, _, _| s.set_chip(Chip::Working)),
            ("filter text", true, |s, _, _| s.filter_mut().insert("login")),
            ("remove a session", true, |s, _, ss| s.remove_session(ss[0].id)),
            ("add a workspace", true, |s, _, _| s.add_project(project("new"))),
            ("reload", true, |s, ps, ss| {
                s.load(vec![ps[0].clone()], vec![ss[1].clone()]);
            }),
        ];
        let viewed = Viewed::default();
        for (name, changes, mutate) in cases {
            let (mut sidebar, projects, sessions) = fixture();
            let before = sidebar.view(&viewed);
            mutate(&mut sidebar, &projects, &sessions);
            let after = sidebar.view(&viewed);
            assert_eq!(*after, sidebar.build(&viewed), "{name}: stale cached view");
            assert_eq!(*before != *after, changes, "{name}: visible change");
            assert_eq!(
                sidebar.waiting(&viewed),
                inbox::count(sidebar.projects(), sidebar.sessions(), &viewed),
                "{name}: stale Inbox count"
            );
        }
    }

    /// A filter or chip change must not serve the old rows; an unchanged sidebar (and events it
    /// ignores) reuses the very same view.
    #[test]
    fn a_filter_or_chip_change_invalidates_the_cache() {
        let (mut sidebar, projects, sessions) = fixture();
        let viewed = Viewed::default();
        let all = sidebar.view(&viewed);
        assert!(Rc::ptr_eq(&all, &sidebar.view(&viewed)), "no change: cache hit");
        // Events the sidebar ignores (ports) leave the cache alone.
        let ignored = Event::SessionPorts(SessionPortsEvent {
            session_id: sessions[0].id,
            ports: vec![3000],
        });
        assert_eq!(sidebar.apply(&ignored), Change::None);
        assert!(Rc::ptr_eq(&all, &sidebar.view(&viewed)), "ignored event: cache hit");

        sidebar.filter_mut().insert("login");
        let narrowed = sidebar.view(&viewed);
        assert!(!Rc::ptr_eq(&all, &narrowed));
        assert_eq!(session_ids(&narrowed.rows), [sessions[2].id]);

        sidebar.clear_filter();
        assert_eq!(session_ids(&sidebar.view(&viewed).rows).len(), 3);
        sidebar.set_chip(Chip::Working);
        assert_eq!(session_ids(&sidebar.view(&viewed).rows), [sessions[0].id]);
        // Revealing a workspace drops the chip and the query too.
        sidebar.filter_mut().insert("zzz");
        sidebar.reveal(projects[1].id);
        assert_eq!(session_ids(&sidebar.view(&viewed).rows).len(), 3);
    }

    /// `17m` moves with the clock, the cached rows do not: the time is read per draw.
    #[test]
    fn time_labels_come_from_the_clock_not_from_the_cache() {
        let (sidebar, _, _) = fixture();
        let viewed = Viewed::default();
        let first = sidebar.view(&viewed);
        let Row::Session(row) = &first.rows[1] else {
            panic!("expected the working session row");
        };
        assert_eq!(row.state, SessionState::Working);
        // `state_changed_at` is 1_000 ms.
        assert_eq!(row.time_text(91_000).as_deref(), Some("1m"));
        assert_eq!(row.time_text(3_901_000).as_deref(), Some("1h 5m"));
        assert!(Rc::ptr_eq(&first, &sidebar.view(&viewed)), "time did not dirty the cache");
    }

    /// Focusing a `done` pane (`Viewed::view` during render) takes it out of the chip and of the
    /// status bar count at once: the cache keys on `Viewed`, not only on the sidebar.
    #[test]
    fn viewing_a_done_session_updates_the_cached_counts() {
        let (sidebar, _, sessions) = fixture();
        let mut viewed = Viewed::default();
        assert_eq!(
            (sidebar.view(&viewed).counts.needs_you, sidebar.waiting(&viewed)),
            (1, 1)
        );
        viewed.view(&sessions[2]);
        assert_eq!(
            (sidebar.view(&viewed).counts.needs_you, sidebar.waiting(&viewed)),
            (0, 0)
        );
    }

    /// The status bar counts the whole Inbox, not the sessions the filter text leaves.
    #[test]
    fn the_inbox_count_ignores_the_filter_text() {
        let (mut sidebar, _, _) = fixture();
        let viewed = Viewed::default();
        sidebar.filter_mut().insert("zzz");
        assert!(sidebar.view(&viewed).rows.is_empty());
        assert_eq!(sidebar.view(&viewed).counts.needs_you, 0, "the chip follows the filter");
        assert_eq!(sidebar.waiting(&viewed), 1, "the status bar does not");
    }

    /// The id index follows every change of the session list.
    #[test]
    fn the_session_lookup_follows_the_list() {
        let (mut sidebar, projects, sessions) = fixture();
        let id = sessions[0].id;
        assert_eq!(sidebar.session(id).map(|s| s.state), Some(SessionState::Working));
        sidebar.apply(&Event::SessionExited(SessionExitedEvent {
            session_id: id,
            exit_code: None,
        }));
        assert_eq!(sidebar.session(id).map(|s| s.state), Some(SessionState::Exited));
        sidebar.remove_session(sessions[1].id);
        assert!(sidebar.session(sessions[1].id).is_none());
        assert_eq!(sidebar.session(id).map(|s| s.id), Some(id), "indices shifted");
        sidebar.load(vec![projects[0].clone()], vec![sessions[1].clone()]);
        assert!(sidebar.session(id).is_none());
        assert!(sidebar.session(sessions[1].id).is_some());
    }
```

Run: `cargo test -p mkrew-ui --lib -j 4 -- sidebar:: inbox::`
Expected: FAIL to compile (`Viewed::version`, `inbox::count`, `Sidebar::build` / `waiting` / `session`, `View: PartialEq` do not exist).

- [ ] **Step 2: Inbox: `count` and the `Viewed` version**

`inbox.rs`: replace

```rust
pub struct Viewed {
    /// Session → `state_changed_at` of its viewed `done`.
    done: HashMap<Ulid, i64>,
}

impl Viewed {
    /// `session` is on screen and focused.
    pub fn view(&mut self, session: &Session) {
        if session.state == SessionState::Done {
            self.done.insert(session.id, session.state_changed_at);
        }
    }
```
with
```rust
pub struct Viewed {
    /// Session → `state_changed_at` of its viewed `done`.
    done: HashMap<Ulid, i64>,
    /// Counts the changes of `done`; what the sidebar's cached view is keyed on.
    version: u64,
}

impl Viewed {
    /// `session` is on screen and focused. Called every frame: only a new or later `done` is a
    /// change.
    pub fn view(&mut self, session: &Session) {
        if session.state == SessionState::Done
            && self.done.insert(session.id, session.state_changed_at)
                != Some(session.state_changed_at)
        {
            self.version += 1;
        }
    }

    pub fn version(&self) -> u64 {
        self.version
    }
```

After `pub fn entries(..)` (before `/// The session "next Inbox entry"`), add:

```rust
/// `entries(..).len()` without building the entries: the status bar's number.
pub fn count(projects: &[Project], sessions: &[Session], viewed: &Viewed) -> usize {
    sessions
        .iter()
        .filter(|session| needs_you(session, viewed))
        .filter(|session| projects.iter().any(|p| p.id == session.project_id))
        .count()
}
```

- [ ] **Step 3: Sidebar cache, `waiting`, `session`**

`sidebar.rs`, imports: replace `use std::collections::{HashMap, HashSet};` with

```rust
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
```

Replace

```rust
/// What the sidebar draws.
#[derive(Debug)]
pub struct View {
```
with
```rust
/// What the sidebar draws.
#[derive(Debug, PartialEq, Eq)]
pub struct View {
```

In `Sidebar`, replace

```rust
    since_load: Option<Vec<Event>>,
}

impl Sidebar {
```
with
```rust
    since_load: Option<Vec<Event>>,
    /// Counts every change of what [`Sidebar::view`] and [`Sidebar::session`] read (lists,
    /// expansion, filter, chip); the caches below are valid for one value. Bumped by
    /// [`Sidebar::touch`] in every `&mut` path that changes those inputs.
    rev: u64,
    view_cache: RefCell<Option<CachedView>>,
    /// Session id → position in `sessions`, for `rev`.
    index: RefCell<Option<(u64, HashMap<Ulid, usize>)>>,
}

/// The rows, chip numbers and Inbox count for one `Sidebar::rev` and one `Viewed::version`.
/// Holds no time text: rows keep `state_changed_at` and the draw formats it.
#[derive(Debug)]
struct CachedView {
    rev: u64,
    viewed: u64,
    view: Rc<View>,
    waiting: usize,
}

impl Sidebar {
    /// What [`Sidebar::view`] and [`Sidebar::session`] read changed: drop their caches.
    fn touch(&mut self) {
        self.rev = self.rev.wrapping_add(1);
    }

```

Add `self.touch();` as the first statement of `toggle`, `filter_mut`, `set_chip`, `toggle_all`, `clear_filter` and `remove_session`, and as the last statement of `load` (after the `if !self.loaded { .. }` block):

```rust
    pub fn toggle(&mut self, project_id: Ulid) {
        self.touch();
        if !self.expanded.remove(&project_id) {
```
```rust
    pub fn filter_mut(&mut self) -> &mut Input {
        // The caller edits the text: assume it changed.
        self.touch();
        &mut self.filter
    }
```
```rust
    pub fn set_chip(&mut self, chip: Chip) {
        self.touch();
        self.chip = chip;
    }
```
```rust
    pub fn toggle_all(&mut self) {
        self.touch();
        if self.any_expanded() {
```
```rust
    pub fn clear_filter(&mut self) {
        self.touch();
        self.filter.clear();
        self.chip = Chip::All;
    }
```
```rust
    pub fn remove_session(&mut self, session_id: Ulid) {
        self.touch();
        self.sessions.retain(|session| session.id != session_id);
    }
```
and in `load`, replace

```rust
                .map(|session| session.project_id)
                .collect();
        }
    }

    /// A workspace the user just added: shown and expanded.
```
with
```rust
                .map(|session| session.project_id)
                .collect();
        }
        self.touch();
    }

    /// A workspace the user just added: shown and expanded.
```
(`reveal` and `add_project` reach `clear_filter` / `upsert_project`, which touch; `set_title` / `set_color` / `apply` reach `update_session`.)

In `update_session`, replace

```rust
            Some(session) => {
                update(session);
                Change::Redraw
            }
            None => Change::Refresh,
        }
    }

    fn upsert_project(&mut self, project: Project) {
```
with
```rust
            Some(session) => {
                update(session);
                self.touch();
                Change::Redraw
            }
            None => Change::Refresh,
        }
    }

    fn upsert_project(&mut self, project: Project) {
        self.touch();
```
Careful with the borrow: `update_session` holds `session` from `self.sessions.iter_mut()`; write it as

```rust
    fn update_session(&mut self, session_id: Ulid, update: impl FnOnce(&mut Session)) -> Change {
        match self.sessions.iter_mut().find(|s| s.id == session_id) {
            Some(session) => {
                update(session);
            }
            None => return Change::Refresh,
        }
        self.touch();
        Change::Redraw
    }
```
(use this form instead of the snippet above; `session` is dropped before `self.touch()`).

Replace the whole `view` fn and `rows` (the block from `/// Rows to show and the chip numbers, in one pass` through the end of `pub fn rows`) so that the old body becomes `build` and `view` is the cache:

```rust
    /// Rows to show and the chip numbers, cached until the sidebar or `viewed` changes (so the
    /// two or three callers of one frame share one compute). Each workspace, then (if open, or
    /// while a filter is active) its live sessions that pass the filter and the chip. While
    /// filtering, workspaces without such a session are left out.
    pub fn view(&self, viewed: &Viewed) -> Rc<View> {
        self.cached(viewed).0
    }

    /// The status bar's Inbox count: [`inbox::count`], cached with the view. Not narrowed by the
    /// filter text or the chip.
    pub fn waiting(&self, viewed: &Viewed) -> usize {
        self.cached(viewed).1
    }

    fn cached(&self, viewed: &Viewed) -> (Rc<View>, usize) {
        let mut cache = self.view_cache.borrow_mut();
        if let Some(hit) = cache
            .as_ref()
            .filter(|hit| hit.rev == self.rev && hit.viewed == viewed.version())
        {
            return (Rc::clone(&hit.view), hit.waiting);
        }
        let view = Rc::new(self.build(viewed));
        let waiting = inbox::count(&self.projects, &self.sessions, viewed);
        *cache = Some(CachedView {
            rev: self.rev,
            viewed: viewed.version(),
            view: Rc::clone(&view),
            waiting,
        });
        (view, waiting)
    }

    /// The session `id` (live or exited), found through an index that is rebuilt after any change
    /// of the list: O(1) per frame instead of a scan per pane.
    pub fn session(&self, id: Ulid) -> Option<&Session> {
        let mut index = self.index.borrow_mut();
        if index.as_ref().is_none_or(|(rev, _)| *rev != self.rev) {
            let map = self
                .sessions
                .iter()
                .enumerate()
                .map(|(at, session)| (session.id, at))
                .collect();
            *index = Some((self.rev, map));
        }
        let at = *index.as_ref()?.1.get(&id)?;
        self.sessions.get(at)
    }

    /// [`Sidebar::view`]'s rows with no `done` session counted as viewed: what the tests and the
    /// pane-lifecycle check read (only the `Needs you` chip looks at `viewed`). Not cached.
    pub fn rows(&self) -> Vec<Row> {
        self.build(&Viewed::default()).rows
    }

    /// One pass over the sessions (O(sessions) plus the newest-first sort inside each workspace).
    fn build(&self, viewed: &Viewed) -> View {
        let filtering = self.filtering();
        // ... the body of the old `view` fn, unchanged ...
    }
```
For `build`, keep the old `view` body exactly (from `let filtering = self.filtering();` to `View { counts, rows }`), only moving it. Delete the old `pub fn view` header and the old `rows` fn.

Run: `cargo test -p mkrew-ui --lib -j 4 -- sidebar:: inbox::`
Expected: PASS (the new tests and all existing sidebar / inbox tests).

- [ ] **Step 4: Tab bar lookups**

`status.rs`: replace

```rust
pub fn most_urgent<'a>(sessions: &'a [Session], ids: &[Ulid]) -> Option<&'a Session> {
    ids.iter()
        .filter_map(|id| sessions.iter().find(|s| s.id == *id))
        .rev()
        .max_by_key(|s| urgency(s.state))
}
```
with
```rust
pub fn most_urgent<'a>(
    ids: &[Ulid],
    find: impl Fn(Ulid) -> Option<&'a Session>,
) -> Option<&'a Session> {
    ids.iter()
        .filter_map(|&id| find(id))
        .rev()
        .max_by_key(|s| urgency(s.state))
}
```
In its test `tab_shows_the_most_urgent_state` replace `most_urgent(&sessions, &ids).map(|s| s.state)` with
```rust
            most_urgent(&ids, |id| sessions.iter().find(|s| s.id == id)).map(|s| s.state)
```
and replace
```rust
        assert_eq!(
            most_urgent(&[a.clone(), b.clone()], &[b.id, a.id]),
            Some(&b)
        );
```
with
```rust
        let pair = [a.clone(), b.clone()];
        assert_eq!(
            most_urgent(&[b.id, a.id], |id| pair.iter().find(|s| s.id == id)),
            Some(&b)
        );
```

`tabs.rs`: replace

```rust
pub fn tab_kind(tab: &Tab, sessions: &[Session]) -> TabKind {
    let focused = tab
        .focused()
        .and_then(|id| sessions.iter().find(|s| s.id == id));
    match focused {
```
with
```rust
pub fn tab_kind<'a>(tab: &Tab, find: impl Fn(Ulid) -> Option<&'a Session>) -> TabKind {
    match tab.focused().and_then(find) {
```
In `a_tab_shows_the_kind_of_its_focused_pane` replace the four calls: `tab_kind(tabs.active().unwrap(), &sessions)` → `tab_kind(tabs.active().unwrap(), |id| sessions.iter().find(|s| s.id == id))` (three times: the `Shell`, the `Agent("fake")` and the `empty` one), and `tab_kind(tabs.active().unwrap(), &[])` → `tab_kind(tabs.active().unwrap(), |_| None)`.

Run: `cargo test -p mkrew-ui --lib -j 4 -- status:: tabs::`
Expected: PASS (unchanged behavior, new call shapes).

- [ ] **Step 5: Wire `main.rs`**

`render_sidebar`: `let view = self.sidebar.view(&self.viewed);` stays (now an `Rc<View>`; `view.counts.*` reads work). Replace `for row in view.rows {` with

```rust
            // The cached rows are shared; the row builders take owned rows (their text moves into
            // the elements), so each frame still clones the visible rows, not the sort, the
            // HashMap, the lowercase and the summaries.
            for row in view.rows.iter().cloned() {
```
`render_compact_sidebar`: replace `for row in self.sidebar.view(&self.viewed).rows {` with `for row in self.sidebar.view(&self.viewed).rows.iter().cloned() {`.

The check at `:8244` (`.view(&self.viewed).rows.iter().any(..)`) compiles unchanged.

`render_status_bar`: replace `let waiting = self.inbox_entries().len();` with `let waiting = self.sidebar.waiting(&self.viewed);`. Keep `inbox_entries()` for `jump_next_inbox` and the peek cycle (user actions, not per frame).

`render_tab_bar`: replace

```rust
                let urgent = status::most_urgent(self.sidebar.sessions(), &tab.sessions());
```
with
```rust
                let urgent = status::most_urgent(&tab.sessions(), |id| self.sidebar.session(id));
```
and `let icon = match tab_kind(tab, self.sidebar.sessions()) {` with `let icon = match tab_kind(tab, |id| self.sidebar.session(id)) {`.

The per-frame single lookups: replace each of
- `render_status_bar`: `.and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id))` (after `.focused_session()`) → `.and_then(|id| self.sidebar.session(id))`
- `render_inspector`: `.and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));` → `.and_then(|id| self.sidebar.session(id));`
- `render_pane`: `let info = self.sidebar.sessions().iter().find(|s| s.id == session);` → `let info = self.sidebar.session(session);`
- `Shell::render`: `&& let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == id)` → `&& let Some(session) = self.sidebar.session(id)`

Check nothing else used the old shapes: `grep -n "most_urgent(\|tab_kind(" crates/mkrew-ui/src/main.rs` must show only the two lines above.

Compile check: `cargo check -p mkrew-ui --all-targets -j 4`
Expected: no errors (`tests/lifecycle.rs` still uses `sidebar.rows()`).

- [ ] **Step 6: HUMAN_CHECKS**

Append to the end of the `### To verify — 2026-10-10 inbox: perf (...)` section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Sidebar and Inbox count are current (Task 2):** with several workspaces and agents, in full and in compact sidebar:
  - A session going `needs input` / `done` (also in another workspace, collapsed) changes `N need you` in the status bar and the `Needs you` chip at once; focusing a `done` pane takes it out of both at once.
  - Pin / rename / change color of a session or a workspace, hide (archive) a workspace, collapse and expand, `Collapse all`: the rows change immediately.
  - Type in the filter: rows narrow per keystroke, the chips narrow with it, but the status bar `N need you` does not change. Clear the filter: all rows are back.
  - The `17m` times keep advancing (working and idle rows, once per minute after the first minute) without any other change.
  - Several tabs with many panes: each tab's status dot and icon (agent badge / `>_`) follow its panes.
```

- [ ] **Step 7: Verify and tick**

Run: `cargo test -p mkrew-ui --lib -j 4 -- sidebar:: inbox:: status:: tabs:: redraw::` (expect PASS), then `cargo test -p mkrew-ui --test lifecycle -j 2` (expect PASS), then the full verification (`bash scripts/verify.sh`, or fmt, clippy, `cargo test -j 2`). Do not tick inbox line 115 (Tasks 3–6 follow).
Suggested message: `perf(ui): cache the sidebar rows, the Inbox count and session lookups until their inputs change`

---

### Task 3: Docs is not re-read while it is hidden

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullet B3; do not tick yet). Spec: §12 (SPEC.md:924, re-anchoring on `doc.changed`), §7 event `doc.changed` (SPEC.md:574).

**Current state (investigated):**
- `main.rs` `Shell::doc_event` (~1736) runs for every `doc.changed` / `comment.updated`. It calls `docs.apply(event)` on EVERY workspace's `Docs` (`self.docs.values_mut()`), and for each `Reaction { reread: Some(file) }` it calls `read_doc` at once, whatever view is shown (the `shown` value is only used to gate `relist`). `read_doc` sends `doc.read`, then `Shown::new(&content)` on the background executor: markdown parse + syntect + mermaid ×2. An agent that writes a plan every few seconds therefore costs a full parse per write in a view nobody looks at.
- `docs.rs` `Docs::apply` (~216) decides `reread` and already sets `self.doc = Doc::Loading` when the shown file is not the announced hash. `Docs::select` (~179) resets `changed_to`. `Docs::read` (~192) drops an answer whose hash differs from `changed_to` (so a late old read is ignored). `changed_to` stays set, so one later read of the newest hash is enough.
- The pattern in Files/Git: `files_event` (~1969) and `git_event` (~2961) return early unless `self.workspace.filter(|_| self.view() == View::Files/Git)`. Hidden views are refreshed by their `show` path: `show_view` (~1559) calls `refresh_files` / `refresh_git` when `shown != view`, `activate_workspace` (~1247) calls them when `switched && self.view() == …`. Docs does the same for the LIST: `show_view` and `activate_workspace` call `load_docs`, and `doc_event` only relists `if reaction.relist && shown == Some(project_id)`. Only the re-read of the selected file is missing from that pattern.
- Existing tests in `docs.rs` `mod tests` (helpers `changed(project, path, Some(hash))`, `shown(path, text, hash)`, `shown_hash(&docs)`) call `docs.apply(&event)` at lines ~455, 463, 467, 470, 486, 516, 517, 519.

**Files:**
- Modify: `crates/mkrew-ui/src/docs.rs` (`Docs` field `stale`, `new`, `select`, `apply`, new `take_stale`, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (`doc_event`, new `show_docs`, `show_view`, `activate_workspace`)
- Modify: `docs/HUMAN_CHECKS.md` (append to the shared perf section)

**Interfaces:**
- Changes: `Docs::apply(&mut self, event: &Event, visible: bool) -> Reaction` (`visible` = this workspace's Docs is on screen). With `visible == false` a re-read is never returned; the file is remembered instead.
- Produces: `Docs::take_stale(&mut self) -> Option<String>` (the selected file to read now that Docs is shown again; `None` after it was taken).
- Produces: `Shell::show_docs(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>)` = `load_docs` + the stale re-read.

- [ ] **Step 1: Failing tests**

In `docs.rs` `mod tests`, change the existing calls to the new signature (they all test a shown Docs): `docs.apply(&changed(project, "a.md", Some("h1")))` → `docs.apply(&changed(project, "a.md", Some("h1")), true)`; the same `, true` at the lines of `Some("h2")`, `b.md`, `Ulid::new()`, `None`, `&comment(project)`, `&comment(Ulid::new())` and `&event(project, CommentContext::Diff)`. Then add at the end of the module (before the closing `}`):

```rust
    /// Hidden Docs: a change only marks the file stale (a read parses, highlights and draws
    /// diagrams); showing Docs again reads it once, however many changes came.
    #[test]
    fn a_change_while_docs_is_hidden_is_read_once_when_it_shows() {
        let project = Ulid::new();
        let mut docs = Docs::new(project);
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        for hash in ["h2", "h3", "h4"] {
            let hidden = docs.apply(&changed(project, "a.md", Some(hash)), false);
            assert_eq!((hidden.relist, hidden.reread), (true, None), "{hash}");
        }
        assert_eq!(docs.doc, Doc::Loading);
        assert_eq!(docs.take_stale().as_deref(), Some("a.md"));
        assert_eq!(docs.take_stale(), None, "asked once");
        // A read sent before the last change answers late: dropped; the newest one shows.
        docs.read("a.md", Ok(shown("a.md", "a2", "h2")));
        assert_eq!(docs.doc, Doc::Loading);
        docs.read("a.md", Ok(shown("a.md", "a4", "h4")));
        assert_eq!(shown_hash(&docs), Some("h4"));
    }

    /// Nothing is stale when there is nothing to read later: the same hash, another file,
    /// a deleted file, or another file picked meanwhile (its own read replaces the mark).
    #[test]
    fn only_a_changed_selected_file_is_stale() {
        let project = Ulid::new();
        let mut docs = Docs::new(project);
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        docs.apply(&changed(project, "a.md", Some("h1")), false);
        docs.apply(&changed(project, "b.md", Some("x")), false);
        assert_eq!(docs.take_stale(), None, "same hash, other file");
        docs.apply(&changed(project, "a.md", Some("h2")), false);
        assert_eq!(docs.select("b.md").as_deref(), Some("b.md"));
        assert_eq!(docs.take_stale(), None, "another file got selected");
        docs.select("a.md");
        docs.read("a.md", Ok(shown("a.md", "a", "h1")));
        docs.apply(&changed(project, "a.md", Some("h5")), false);
        let gone = docs.apply(&changed(project, "a.md", None), false);
        assert_eq!((gone.relist, gone.reread), (true, None));
        assert_eq!(docs.doc, Doc::Deleted);
        assert_eq!(docs.take_stale(), None, "a deleted file is not read");
    }
```

Run: `cargo test -p mkrew-ui --lib docs::tests -j 4`
Expected: FAIL to compile (`apply` takes one argument, `take_stale` does not exist).

- [ ] **Step 2: Logic in `docs.rs`**

`Docs`, after the `changed_to` field (end of the struct):

```rust
    /// A `doc.changed` of the selected file came while Docs was hidden: it is read when Docs
    /// is shown again ([`Self::take_stale`]).
    stale: Option<String>,
```
`Docs::new`, after `changed_to: None,`: `stale: None,`

`Docs::select`, replace
```rust
        self.doc = Doc::Loading;
        self.changed_to = None;
        Some(file_path.to_owned())
```
with
```rust
        self.doc = Doc::Loading;
        self.changed_to = None;
        self.stale = None;
        Some(file_path.to_owned())
```

`Docs::apply`: replace the signature line
```rust
    pub fn apply(&mut self, event: &Event) -> Reaction {
```
with
```rust
    pub fn apply(&mut self, event: &Event, visible: bool) -> Reaction {
```
and replace
```rust
                self.changed_to.clone_from(&changed.hash);
                match &changed.hash {
                    None => self.doc = Doc::Deleted,
                    Some(hash) => {
                        let current = matches!(&self.doc, Doc::Shown(shown) if shown.hash == *hash);
                        // A read in flight may predate the change: read again after it.
                        if !current {
                            self.doc = Doc::Loading;
                            reaction.reread = Some(changed.file_path.clone());
                        }
                    }
                }
```
with
```rust
                self.changed_to.clone_from(&changed.hash);
                self.stale = None;
                match &changed.hash {
                    None => self.doc = Doc::Deleted,
                    Some(hash) => {
                        let current = matches!(&self.doc, Doc::Shown(shown) if shown.hash == *hash);
                        // A read in flight may predate the change: read again after it. While
                        // Docs is hidden nobody sees the page, so it is only remembered.
                        if !current {
                            self.doc = Doc::Loading;
                            if visible {
                                reaction.reread = Some(changed.file_path.clone());
                            } else {
                                self.stale = Some(changed.file_path.clone());
                            }
                        }
                    }
                }
```
Add after `apply`:
```rust
    /// The selected file to read now that Docs is shown again, when it changed while hidden.
    pub fn take_stale(&mut self) -> Option<String> {
        self.stale.take()
    }
```
Update the module doc (line 3) wording: `` `doc.changed` re-reads the shown file and re-lists`` → `` `doc.changed` re-reads the shown file (when Docs is hidden: when it is shown again) and re-lists``.

Run: `cargo test -p mkrew-ui --lib docs::tests -j 4`
Expected: PASS (new tests and the old ones with `true`).

- [ ] **Step 3: `main.rs`**

`doc_event`, replace
```rust
            .map(|docs| (docs.project_id, docs.apply(event)))
            .collect();
```
with
```rust
            .map(|docs| (docs.project_id, docs.apply(event, shown == Some(docs.project_id))))
            .collect();
```
Update its doc comment (`/// Follows \`doc.changed\` …`): `re-reads a shown file that changed` → `re-reads a changed file when its Docs is on screen (a hidden one reads it when shown again)`.

Add after `doc_event`:
```rust
    /// Docs of `project_id` came on screen: re-lists its files and reads the selected one
    /// again when it changed while Docs was hidden.
    fn show_docs(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.load_docs(project_id, window, cx);
        let stale = self.docs.get_mut(&project_id).and_then(Docs::take_stale);
        if let Some(file_path) = stale {
            self.read_doc(project_id, file_path, window, cx);
        }
    }
```
`show_view`, replace
```rust
        if view == View::Docs && shown != View::Docs {
            self.load_docs(project_id, window, cx);
        }
```
with
```rust
        if view == View::Docs && shown != View::Docs {
            self.show_docs(project_id, window, cx);
        }
```
`activate_workspace`, replace
```rust
        if switched && self.view() == View::Docs {
            self.load_docs(project_id, window, cx);
        }
```
with
```rust
        if switched && self.view() == View::Docs {
            self.show_docs(project_id, window, cx);
        }
```
(`load_docs` keeps its other callers: the `doc_event` relist and its own `again`.)

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors.

- [ ] **Step 4: HUMAN_CHECKS**

Append to the end of the section `### To verify — 2026-10-10 inbox: perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)` in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Docs while hidden (B3):** open a workspace's Docs on a plan file, switch to Terminals. Let an agent (or Zed) save that file several times → `mkrew-ui` CPU stays flat in Task Manager. Back to Docs → the page shows the latest content (a brief "Loading…" is fine) and comments still work. Also: change the file while another workspace is shown, then switch to the workspace and open Docs → latest content. A file that was deleted meanwhile shows the deleted notice.
```

- [ ] **Step 5: Verify and tick**

Run: `cargo test -p mkrew-ui --lib docs -j 4` (expect PASS), then the full verification. Do not tick inbox line 115 (Task 6 does).
Suggested message: `perf(ui): Docs reads a changed file only when it is shown`

---

### Task 4: Markdown live preview in Files is debounced 300 ms

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullet A6; do not tick yet). Spec: §12A (SPEC.md:954-958, Files editor), ADR 0021 (preview).

**Current state (investigated):**
- `files_view.rs`: `FilesView::preview_request` (~595) returns `Some((stamp, editor.text()))` as soon as the text moved on, once per stamp (`preview_asked`). `Stamp = (text_id, editor version)`; `stamp()` (~579). `preview_ready` (~610) already drops a page whose `text_id` is another or that is older than the page already stored (`preview.stamp >= stamp`), so a late build never replaces a newer one. This is the generation logic from redesign Task 21; it is reused as is. `open()` (~343) resets `preview` and `preview_asked`.
- `main.rs` `Shell::build_preview` (~1944): calls `preview_request` and parses `Shown::from_text(&text)` on the background executor (syntect + mermaid ×2), then in `update_in` calls `preview_ready` and `build_preview` again (asks once more when the text changed meanwhile). It is called from `read_file` (~1934) after each read and from the Preview/Source toggle (~11287). Nothing calls it per keystroke while the editor shows: it runs on switching back to Preview. While Source is shown `previewing()` is false so no build. The cost lands when the user types in Source, toggles to Preview, and when Preview is shown and the file is re-read (agent edit, `git.changed` → `read_file`): each read that changes the text builds at once with no delay, and `preview_request` copies the whole text each time.
- `file_edited` (~2407) already has a debounce for highlighting: `background.timer(theme::REHIGHLIGHT_DELAY)` (`theme.rs:166`, 150 ms) then re-checks the version in `highlight_request`. The same shape is reused for the preview.
- Tests: `files_view.rs` `mod tests` has `opened(text)`, `page(text)`, `file_text(text)`, existing preview tests at ~1088-1156 (`markdown_opens_as_a_preview_that_is_parsed_once_per_text`, `a_late_preview_never_replaces_a_newer_one`).

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`PREVIEW_DELAY`, after `REHIGHLIGHT_DELAY`)
- Modify: `crates/mkrew-ui/src/files_view.rs` (`PreviewStep`, field `preview_wait`, `preview_step`, `preview_due`, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (`build_preview`, new `parse_preview`)
- Modify: `docs/HUMAN_CHECKS.md` (append)

**Interfaces:**
- Produces: `theme::PREVIEW_DELAY: Duration` = 300 ms.
- Produces: `files_view::PreviewStep { Idle, Build(Stamp, String), Wait(u64) }`.
- Produces: `FilesView::preview_step(&mut self) -> PreviewStep` and `FilesView::preview_due(&mut self, wait: u64) -> Option<(Stamp, String)>`.
- Consumes: `FilesView::preview_request`, `preview_ready` (unchanged). `preview_request` stays pub (existing tests use it).

Decision (pure rule): the FIRST page of a text (no page stored, e.g. a file just opened) is built at once, because the pane would otherwise sit empty. Every later change waits: each change takes a new wait number (`preview_wait`), and when the 300 ms timer ends only the newest number builds. A timer of an older number does nothing. Typing 20 characters in a row therefore parses once, 300 ms after the last one.

- [ ] **Step 1: Failing tests**

In `files_view.rs` `mod tests`, after `a_late_preview_never_replaces_a_newer_one`, add:

```rust
    /// A file just opened has no page: it is built at once, once.
    #[test]
    fn the_first_page_of_a_file_is_built_at_once() {
        let mut view = opened("# T\n");
        let PreviewStep::Build(stamp, text) = view.preview_step() else {
            panic!("expected a build");
        };
        assert_eq!(text, "# T\n");
        assert_eq!(view.preview_step(), PreviewStep::Idle, "being built");
        view.preview_ready(stamp, page(&text));
        assert_eq!(view.preview_step(), PreviewStep::Idle, "current");
    }

    /// Edits after the first page wait; every later edit restarts the wait, and only the
    /// last wait builds (one parse for a burst of typing).
    #[test]
    fn edits_after_the_first_page_build_once_after_the_last_change() {
        let mut view = opened("a\n");
        let PreviewStep::Build(first, _) = view.preview_step() else {
            panic!("expected a build");
        };
        view.preview_ready(first, page("a\n"));
        view.toggle_mode();
        view.editor_mut().unwrap().insert("b");
        view.toggle_mode();
        let PreviewStep::Wait(one) = view.preview_step() else {
            panic!("expected a wait");
        };
        view.toggle_mode();
        view.editor_mut().unwrap().insert("c");
        view.toggle_mode();
        let PreviewStep::Wait(two) = view.preview_step() else {
            panic!("expected a wait");
        };
        assert_ne!(one, two);
        assert_eq!(view.preview_due(one), None, "superseded by a later edit");
        let (stamp, text) = view.preview_due(two).expect("the last wait builds");
        assert_eq!(text, "bca\n");
        assert_eq!(view.preview_due(two), None, "asked once");
        view.preview_ready(stamp, page(&text));
        assert_eq!(view.preview_step(), PreviewStep::Idle);
    }

    /// A build that finishes after a newer page exists never replaces it, and a wait that
    /// ends where nothing shows a preview builds nothing.
    #[test]
    fn a_wait_ends_without_a_build_when_the_preview_is_gone() {
        let mut view = opened("a\n");
        let PreviewStep::Build(first, _) = view.preview_step() else {
            panic!("expected a build");
        };
        view.preview_ready(first, page("a\n"));
        view.editor_mut().unwrap().insert("b");
        let PreviewStep::Wait(wait) = view.preview_step() else {
            panic!("expected a wait");
        };
        view.toggle_mode();
        assert_eq!(view.mode, Mode::Source);
        assert_eq!(view.preview_due(wait), None, "the editor is shown");
        view.toggle_mode();
        let PreviewStep::Wait(next) = view.preview_step() else {
            panic!("expected a wait");
        };
        assert_eq!(view.preview_due(wait), None, "the old timer is superseded");
        assert!(view.preview_due(next).is_some());
        view.open("src/main.rs");
        assert_eq!(view.preview_due(next), None, "another file replaced it");
    }
```

Run: `cargo test -p mkrew-ui --lib files_view::tests::the_first_page files_view::tests::edits_after files_view::tests::a_wait_ends -j 4` (run each filter, or `cargo test -p mkrew-ui --lib files_view::tests::preview -j 4` after naming; use the three commands)
Expected: FAIL to compile (`PreviewStep` does not exist).

- [ ] **Step 2: `theme.rs` constant**

After
```rust
pub const REHIGHLIGHT_DELAY: Duration = Duration::from_millis(150);
```
add:
```rust

/// A `.md` preview in Files is parsed again this long after its text last changed (the
/// first page of a file is built at once).
pub const PREVIEW_DELAY: Duration = Duration::from_millis(300);
```

- [ ] **Step 3: Logic in `files_view.rs`**

After `pub type Stamp = (u64, u64);` add:

```rust

/// What the shell does about a `.md` preview that may be out of date ([`FilesView::preview_step`]).
#[derive(Debug, PartialEq, Eq)]
pub enum PreviewStep {
    /// Nothing: no preview shown, the page is current, or one is being built for this text.
    Idle,
    /// Parse this text now: the file has no page yet, the pane would stay empty.
    Build(Stamp, String),
    /// A page of an older text is shown: wait [`crate::theme::PREVIEW_DELAY`], then call
    /// [`FilesView::preview_due`] with this number. A later `Wait` makes it stale.
    Wait(u64),
}
```
In `FilesView` after `preview_asked: Option<Stamp>,` add:
```rust
    /// Number of the last wait handed out by [`Self::preview_step`]: only its timer builds.
    preview_wait: u64,
```
In `FilesView::new` after `preview_asked: None,` add `preview_wait: 0,` (`open()` does not reset it: the numbers only grow, so an old timer never matches).

After `preview_request` add:
```rust
    /// Decides what the shell does now about the preview (called after every read, mode
    /// switch and page): the first page of a text is built at once, later ones after
    /// [`crate::theme::PREVIEW_DELAY`] of quiet, so a burst of edits parses once.
    pub fn preview_step(&mut self) -> PreviewStep {
        if !self.previewing() {
            return PreviewStep::Idle;
        }
        let Some(stamp) = self.stamp() else {
            return PreviewStep::Idle;
        };
        let current = (self.preview.as_ref()).is_some_and(|preview| preview.stamp == stamp);
        if current || self.preview_asked == Some(stamp) {
            return PreviewStep::Idle;
        }
        if self.preview.is_none() {
            return self
                .preview_request()
                .map_or(PreviewStep::Idle, |(stamp, text)| PreviewStep::Build(stamp, text));
        }
        self.preview_wait += 1;
        PreviewStep::Wait(self.preview_wait)
    }

    /// The wait `wait` ended: the text to parse, unless a later change took a newer wait or
    /// nothing shows a preview any more. Answer with [`Self::preview_ready`].
    pub fn preview_due(&mut self, wait: u64) -> Option<(Stamp, String)> {
        if wait != self.preview_wait {
            return None;
        }
        self.preview_request()
    }
```
Note: `preview()` returns `None` for a page of another text (a re-read replaced the text) while `self.preview` is still `Some`, so a re-read after the first page also waits 300 ms (shows "Rendering the preview…" meanwhile); this is the same burst case (agent saving a file over and over) and is intended.

Run: `cargo test -p mkrew-ui --lib files_view::tests -j 4`
Expected: PASS (new and existing preview tests).

- [ ] **Step 4: `main.rs`**

Replace the whole `build_preview` fn (from the doc comment `/// Parses the \`.md\` file of the Files view` to its closing `}`) with:

```rust
    /// Asks the Files view of `project_id` what its `.md` preview needs ([`files_view::PreviewStep`]):
    /// the first page of a text is parsed at once, a change after it waits
    /// [`theme::PREVIEW_DELAY`] of quiet. Parsing runs off the UI thread (highlighting and
    /// Mermaid are slow) and asks again when the text changed while it ran.
    fn build_preview(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let target = view.worktree_task_id;
        match view.preview_step() {
            files_view::PreviewStep::Idle => {}
            files_view::PreviewStep::Build(stamp, text) => {
                self.parse_preview(project_id, target, stamp, text, window, cx);
            }
            files_view::PreviewStep::Wait(wait) => {
                let background = cx.background_executor().clone();
                cx.spawn_in(window, async move |this, cx| {
                    background.timer(theme::PREVIEW_DELAY).await;
                    let _ = this.update_in(cx, |shell, window, cx| {
                        let due = (shell.files_at(project_id, target))
                            .and_then(|view| view.preview_due(wait));
                        if let Some((stamp, text)) = due {
                            shell.parse_preview(project_id, target, stamp, text, window, cx);
                        }
                    });
                })
                .detach();
            }
        }
    }

    /// Parses `text` for the preview of `stamp` on the background executor and stores the page
    /// (a page of an older text or older than the stored one is dropped by `preview_ready`).
    fn parse_preview(
        &mut self,
        project_id: Ulid,
        target: Option<Ulid>,
        stamp: files_view::Stamp,
        text: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
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
No other caller changes (`read_file` and the Preview toggle call `build_preview` as before).

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors.

- [ ] **Step 5: HUMAN_CHECKS**

Append to the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Files `.md` preview debounce (A6):** open a long `.md` file in Files → its preview shows at once (no extra wait). Switch to Source, type quickly for a few seconds, switch back to Preview → the old page shows first and updates about 0.3 s after you stopped (not once per key, `mkrew-ui` CPU does not stay high while typing). Open a file with a mermaid block and edit it in Source → the diagram appears once after you stop. An agent saving the open file repeatedly → the preview follows without the window stalling.
```

- [ ] **Step 6: Verify and tick**

Run: `cargo test -p mkrew-ui --lib files_view theme -j 4` (expect PASS), then the full verification. Do not tick inbox line 115 (Task 6 does).
Suggested message: `perf(ui): Files markdown preview waits 300 ms after the last change`

---

### Task 5: Title/status bar git summary refresh is throttled to once per 2 s

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullet B4; do not tick yet). Spec: §10 (SPEC.md:746-750, title bar git chip and `Git N` badge), §7 event `git.changed` (SPEC.md:577).

**Current state (investigated):**
- `main.rs` event loop (~8496): for every `Event::GitChanged` it runs `files_event`, `git_event`, and when `changed.worktree_task_id.is_none() && self.workspace == Some(changed.project_id)` calls `load_git_summary(changed.project_id, …)` (~8503), whatever view is shown.
- `Shell::load_git_summary` (~2669) is the single-flight loader from redesign Task 2: `git_summary_busy` / `git_summary_again`; its request is `git_summary_of` (~17277): `git.repos` + one `git.status` per repository. A change that arrives while one runs sets `git_summary_again` and reloads once when it answers. It is not rate limited: with `git.changed` every ~300 ms (SPEC.md:577) a busy agent keeps it running back to back.
- Other callers of `load_git_summary`: `activate_workspace` (~1263, `switched || self.git_summary.is_none()`) and its own `shown != project_id` retry. Fields `git_summary`, `git_summary_busy`, `git_summary_again` at ~408-411, initialised at ~779-781.
- `git_summary.rs` is the pure module (`RepoSummary`, `GitSummary`, `mod tests`), imported in `main.rs:60` as `use mkrew_ui::git_summary::{GitSummary, RepoSummary};`. The throttle goes there so it is tested headlessly. `status::now_ms() -> i64` (Unix ms) is the clock the shell already uses.

**Files:**
- Modify: `crates/mkrew-ui/src/git_summary.rs` (`REFRESH_EVERY_MS`, `Due`, `Throttle`, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (import, field, init, `activate_workspace`, event loop, new `git_summary_due`)
- Modify: `docs/HUMAN_CHECKS.md` (append)

**Interfaces:**
- Produces: `git_summary::REFRESH_EVERY_MS: i64 = 2_000`.
- Produces: `git_summary::Due { Now, After(i64), Skip }`.
- Produces: `git_summary::Throttle { new via Default }` with `changed(&mut self, now_ms: i64) -> Due` (a `git.changed` arrived), `timer(&mut self, now_ms: i64) -> Due` (the trailing timer fired), `refreshed(&mut self, now_ms: i64)` (a refresh was started by something else: starts a new window and drops a pending trailing one).
- Produces: `Shell::git_summary_due(&mut self, project_id: Ulid, due: Due, window, cx)`.
- Consumes: `Shell::load_git_summary` unchanged.

Rules: first change after `refreshed` or after a quiet 2 s: `Now`. Changes inside the 2 s window: one `After(remaining)` for the first (the trailing timer), `Skip` for the rest, so the last change always ends with one refresh at the window end. `timer` at the window end: `Now` (and a new window opens), `After(rest)` if it fired early, `Skip` if nothing is pending. A workspace switch calls `refreshed` and loads at once. A clock that steps back opens a new window (no frozen refresh).

- [ ] **Step 1: Failing tests**

In `git_summary.rs` `mod tests` (end of the module), add:

```rust
    /// The first change refreshes at once; a burst inside 2 s gets exactly one trailing
    /// refresh at the end of the window, so the last change always shows.
    #[test]
    fn a_burst_gets_one_immediate_and_one_trailing_refresh() {
        let mut throttle = Throttle::default();
        assert_eq!(throttle.changed(10_000), Due::Now);
        assert_eq!(throttle.changed(10_100), Due::After(1_900));
        assert_eq!(throttle.changed(10_500), Due::Skip, "the timer is already set");
        assert_eq!(throttle.changed(11_999), Due::Skip);
        assert_eq!(throttle.timer(11_999), Due::After(1), "fired a ms early");
        assert_eq!(throttle.timer(12_000), Due::Now);
        assert_eq!(throttle.timer(12_001), Due::Skip, "nothing pending");
        // A change after that opens the next window from the trailing refresh.
        assert_eq!(throttle.changed(12_500), Due::After(1_500));
    }

    /// Boundaries: exactly 2 s after a refresh is free, 1 ms earlier waits 1 ms; a quiet gap
    /// longer than 2 s refreshes at once.
    #[test]
    fn the_window_is_exactly_two_seconds() {
        let mut throttle = Throttle::default();
        assert_eq!(throttle.changed(0), Due::Now);
        assert_eq!(throttle.changed(2_000), Due::Now);
        assert_eq!(throttle.changed(3_999), Due::After(1));
        assert_eq!(throttle.timer(4_000), Due::Now);
        assert_eq!(throttle.changed(60_000), Due::Now);
        assert_eq!(REFRESH_EVERY_MS, 2_000);
    }

    /// A workspace switch refreshes at once (the caller loads) and its window replaces the
    /// old one: the old trailing timer has nothing left to do, and the next change waits for
    /// the new window.
    #[test]
    fn a_switch_refreshes_at_once_and_restarts_the_window() {
        let mut throttle = Throttle::default();
        assert_eq!(throttle.changed(0), Due::Now);
        assert_eq!(throttle.changed(500), Due::After(1_500));
        throttle.refreshed(1_000);
        assert_eq!(throttle.timer(2_000), Due::Skip, "the switch refreshed already");
        assert_eq!(throttle.changed(1_500), Due::After(1_500), "2 s after the switch, not 0");
        assert_eq!(throttle.timer(2_000), Due::After(1_000), "an old timer asks again");
        assert_eq!(throttle.timer(3_000), Due::Now);
    }

    /// A clock that stepped back (NTP, manual change) must not freeze the refresh for hours.
    #[test]
    fn a_clock_that_stepped_back_starts_a_new_window() {
        let mut throttle = Throttle::default();
        assert_eq!(throttle.changed(10_000_000), Due::Now);
        assert_eq!(throttle.changed(5_000), Due::After(REFRESH_EVERY_MS));
        assert_eq!(throttle.timer(7_000), Due::Now);
    }
```

Run: `cargo test -p mkrew-ui --lib git_summary::tests -j 4`
Expected: FAIL to compile (`Throttle`, `Due`, `REFRESH_EVERY_MS` do not exist).

- [ ] **Step 2: Logic in `git_summary.rs`**

After the `impl GitSummary { … }` block and before `#[cfg(test)]` add:

```rust
/// The least time between two refreshes of the title bar git summary (perf B4).
pub const REFRESH_EVERY_MS: i64 = 2_000;

/// What to do about a `git.changed` (or the timer it set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Due {
    /// Refresh now.
    Now,
    /// Call [`Throttle::timer`] again in this many ms (the trailing refresh).
    After(i64),
    /// Nothing: a timer is already set, or nothing is pending.
    Skip,
}

/// Trailing throttle of the summary refresh: at most one per [`REFRESH_EVERY_MS`] while
/// changes keep coming, and the last change always ends with a refresh. Pure; the shell
/// passes `status::now_ms()`.
#[derive(Debug, Default)]
pub struct Throttle {
    /// When the last refresh started.
    last: Option<i64>,
    /// A trailing timer is set (one at a time).
    trailing: bool,
}

impl Throttle {
    /// A `git.changed` of the shown workspace arrived.
    pub fn changed(&mut self, now_ms: i64) -> Due {
        let wait = self.wait(now_ms);
        if wait == 0 {
            self.refreshed(now_ms);
            return Due::Now;
        }
        if std::mem::replace(&mut self.trailing, true) {
            Due::Skip
        } else {
            Due::After(wait)
        }
    }

    /// The trailing timer fired.
    pub fn timer(&mut self, now_ms: i64) -> Due {
        if !self.trailing {
            return Due::Skip;
        }
        let wait = self.wait(now_ms);
        if wait > 0 {
            return Due::After(wait);
        }
        self.refreshed(now_ms);
        Due::Now
    }

    /// A refresh starts for another reason (a workspace switch): a new window opens and a
    /// pending trailing refresh is no longer needed.
    pub fn refreshed(&mut self, now_ms: i64) {
        self.last = Some(now_ms);
        self.trailing = false;
    }

    /// Ms left in the window (0 = a refresh is allowed).
    fn wait(&mut self, now_ms: i64) -> i64 {
        let Some(last) = self.last else {
            return 0;
        };
        // A clock that stepped back opens a new window instead of freezing the refresh.
        let last = if now_ms < last {
            self.last = Some(now_ms);
            now_ms
        } else {
            last
        };
        (last + REFRESH_EVERY_MS - now_ms).max(0)
    }
}
```

Run: `cargo test -p mkrew-ui --lib git_summary::tests -j 4`
Expected: PASS.

- [ ] **Step 3: `main.rs`**

Import (line ~60): replace `use mkrew_ui::git_summary::{GitSummary, RepoSummary};` with `use mkrew_ui::git_summary::{Due, GitSummary, RepoSummary, Throttle};`.

Field, after `git_summary_again: bool,`:
```rust
    /// At most one summary refresh per 2 s while `git.changed` keeps coming; a trailing one
    /// always follows the last change (perf B4).
    git_summary_throttle: Throttle,
```
Init, after `git_summary_again: false,`: `git_summary_throttle: Throttle::default(),`

`activate_workspace`, replace
```rust
        if switched || self.git_summary.is_none() {
            self.load_git_summary(project_id, window, cx);
        }
```
with
```rust
        if switched || self.git_summary.is_none() {
            // A switch refreshes at once and opens a new 2 s window.
            self.git_summary_throttle.refreshed(status::now_ms());
            self.load_git_summary(project_id, window, cx);
        }
```
Event loop, replace
```rust
                        self.load_git_summary(changed.project_id, window, cx);
```
with
```rust
                        let due = self.git_summary_throttle.changed(status::now_ms());
                        self.git_summary_due(changed.project_id, due, window, cx);
```
Add after `load_git_summary` (before `fn git_requests`):
```rust
    /// Does what the throttle decided for a `git.changed` of `project_id`: refresh the title
    /// bar summary now, or set the one trailing timer that refreshes it at the end of the
    /// 2 s window (only if that workspace is still the shown one).
    fn git_summary_due(
        &mut self,
        project_id: Ulid,
        due: Due,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match due {
            Due::Now => self.load_git_summary(project_id, window, cx),
            Due::Skip => {}
            Due::After(ms) => {
                let background = cx.background_executor().clone();
                let wait = Duration::from_millis(ms.unsigned_abs());
                cx.spawn_in(window, async move |this, cx| {
                    background.timer(wait).await;
                    let _ = this.update_in(cx, |shell, window, cx| {
                        let due = shell.git_summary_throttle.timer(status::now_ms());
                        // A switch meanwhile refreshed the new workspace and cleared the
                        // pending timer, so `Skip`; otherwise this is still the shown one.
                        if let Some(shown) = shell.workspace.filter(|_| due != Due::Skip) {
                            shell.git_summary_due(shown, due, window, cx);
                        }
                    });
                })
                .detach();
            }
        }
    }
```
Update the doc of `load_git_summary`: `Asked on a workspace switch and on its \`git.changed\`` → `Asked on a workspace switch and (throttled, \`git_summary_due\`) on its \`git.changed\``.

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors.

- [ ] **Step 4: HUMAN_CHECKS**

Append to the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Git summary throttle (B4):** in Terminals, with a shell in a repo run `1..30 | % { Add-Content t.txt $_; Start-Sleep -Milliseconds 300 }` → the title bar `Git N` badge and the branch chip update about every 2 s (not on every write), and the final count shows within about 2 s after the loop ends; `git.exe` processes in Task Manager no longer spawn continuously. Switch to another workspace → its branch and badge show at once. In the Git view, commit → the badge goes to the new count within about 2 s (it used to be immediate; the Git view's own list still updates at once).
```

- [ ] **Step 5: Verify and tick**

Run: `cargo test -p mkrew-ui --lib git_summary -j 4` (expect PASS), then the full verification. Do not tick inbox line 115 (Task 6 does).
Suggested message: `perf(ui): refresh the title bar git summary at most once per 2 s`

---

### Task 6: No settings.toml poll; the Files editor width is cached

Inbox: line 115 `11. Perf: stop whole-app re-renders in the UI` (bullets B8 and B11, the last task of item 11; TICK it `[~]`/`[ ]` → `[x]`). Spec: §20.0 (SPEC.md:1503-1509), ADR 0024 (font settings, live reload), §12A (SPEC.md:954-958).

> **B11 is done** by `docs/superpowers/plans/2026-10-10-file-viewer-scroll.md` Task 3 (`Editor::longest_line`, `render_editor` borrows the theme). Skip the B11 steps and the B11 HUMAN_CHECKS line here; do only B8.

**Current state (investigated):**
- B8: `main.rs` `Shell::new` (~522-575) spawns the status loop `cx.spawn_in(window, async move |this, cx| { let mut last = status::now_ms(); loop { background timer(theme::STATUS_TICK) …` and inside every 500 ms tick runs `std::fs::metadata(&shell.settings_file)…modified()` on the UI thread and compares with `shell.settings_modified` (field ~297, doc says "until the first status tick reads it"). On a change it runs `theme::reload(&shell.settings_file, &installed, &last, mode)`: `Ok(changed)` clears `settings_error`, sets the `theme::Fonts` global, `theme::refresh`, `cx.refresh_windows()`, and applies `diff_mode` from a second `mkrew_core::settings::load`; `Err(error)` sets `settings_error` and keeps the last good fonts/theme (palette lists it, §20.0). The first tick is also the first read (mtime `None` → `Some`), which reloads the already-loaded file once.
- `observe_window_activation` exists (~513): `cx.observe_window_activation(window, |_, _, cx| cx.notify()).detach();` (the Inbox re-render on return). `window.is_window_active()` is used at ~20512.
- Our own writes (`write_ui_value` for theme / inspector width / diff mode, ~4941-4993) change the mtime too; today the next tick re-loads the file and applies what we just wrote (no visible change). After this task the next activation does (also no visible change).
- `theme::reload` has the test `a_broken_settings_file_keeps_the_last_settings_and_says_why` (theme.rs ~848): it already pins "broken file keeps the last good settings and reports `file:line:col`" and "unchanged: no redraw". That logic does not move.
- SPEC §20.0 (SPEC.md:1506-1507) says: "The UI reads it the same way and re-reads it within 500 ms of a change (fonts apply live, ADR 0024)". ADR 0024 (`docs/decisions/0024-font-settings.md:26-27`) says it is "checked on the existing 500 ms status tick".
- B11: `render_editor` (`main.rs` ~11641) computes `longest` per render: `lines.iter().enumerate().max_by_key(|(_, line)| editor::display_col(line, line.len()))` over the WHOLE file (a 2 MB file = hundreds of thousands of chars per frame), and `let row_theme = theme.clone();` (~11658) clones the whole `Theme` per render to move into the `uniform_list` closure. `editor.rs` `Editor::splice` (~482) is "the only text change" and bumps `version` (~492); there is no other place that replaces `lines`. `editor::display_col` (~529) and `TAB` (highlight.rs:25, `"    "`).
- `mkrew-ui` `Editor` tests live in `editor.rs` `mod tests` (~625, `use super::*`, helper `type_chars`).

**Files:**
- Modify: `crates/mkrew-ui/src/editor.rs` (`longest` field, `new`, `splice`, `widest_line`, `longest_line`, test)
- Modify: `crates/mkrew-ui/src/main.rs` (`render_editor`, `Shell::new` activation observer and status loop, new `reload_settings_if_changed`, field docs)
- Create: `docs/decisions/0044-settings-reload-on-activation.md`
- Modify: `docs/SPEC.md` §20.0 (:1506-1507)
- Modify: `docs/decisions/0024-font-settings.md` (:26-27)
- Modify: `docs/HUMAN_CHECKS.md` (append)
- Modify: `docs/INBOX.md` (tick line 115)

**Interfaces:**
- Produces: `Editor::longest_line(&self) -> usize` (index of the widest line in screen cells, kept per text version; lines are never empty so it is always a valid index).
- Produces: `Shell::reload_settings_if_changed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>)` (the former poll body, run at start and when the window is activated).
- Consumes: `theme::reload`, `mkrew_core::settings::load` (unchanged).

- [ ] **Step 1: Failing test (B11)**

In `editor.rs` `mod tests`, add at the end:

```rust
    /// The widest row in screen cells (a tab is `TAB` cells, more than its one byte) follows
    /// every edit and undo, so the render never scans the file again.
    #[test]
    fn longest_line_is_the_widest_row_and_follows_edits() {
        let widest = |editor: &Editor| {
            let line = &editor.lines()[editor.longest_line()];
            display_col(line, line.len())
        };
        let tabs = 2 * TAB.len() + 1;
        let mut editor = Editor::new("short\n\t\tx\n12345678\n");
        assert_eq!(editor.longest_line(), 1, "two tabs and x are wider than 8 bytes");
        assert_eq!(widest(&editor), tabs);
        // A longer line typed at the end of the last row wins…
        editor.set_cursor(Pos::new(2, 8), false);
        editor.insert("9012345678");
        assert_eq!((editor.longest_line(), widest(&editor)), (2, 18));
        // …and undo gives the old widest row back.
        assert!(editor.undo());
        assert_eq!(widest(&editor), tabs);
        // Cutting the widest row's text leaves the next widest.
        editor.set_cursor(Pos::new(1, 0), false);
        editor.set_cursor(Pos::new(1, 3), true);
        editor.cut();
        assert_eq!((editor.longest_line(), widest(&editor)), (2, 8));
        // An empty file still has a line 0.
        assert_eq!(Editor::new("").longest_line(), 0);
    }
```

Run: `cargo test -p mkrew-ui --lib editor::tests::longest_line -j 4`
Expected: FAIL to compile (`longest_line` does not exist).

- [ ] **Step 2: Cache in `editor.rs`**

`Editor` struct, after `version: u64,` add:
```rust
    /// Index of the widest line in screen cells ([`Self::longest_line`]), found when the text
    /// changes instead of on every frame.
    longest: usize,
```
`Editor::new`, replace
```rust
            .collect();
        Editor {
            lines,
```
with
```rust
            .collect::<Vec<String>>();
        let longest = widest_line(&lines);
        Editor {
            lines,
```
and after `version: 0,` (inside the struct literal) add `longest,`.

`Editor::splice`, after `self.version += 1;` add:
```rust
        // ponytail: one scan per text change (it was one per frame); update from the replaced
        // lines only if typing in a 2 MB file ever feels slow.
        self.longest = widest_line(&self.lines);
```
After `pub fn version(&self) -> u64 { self.version }` add:
```rust

    /// Index of the widest line in screen cells (tabs and multibyte chars differ from the
    /// byte length), for the editor's horizontal scroll width.
    pub fn longest_line(&self) -> usize {
        self.longest
    }
```
After `pub fn display_col` (a free fn, before `display_byte`) add:
```rust
/// Index of the widest of `lines` in screen cells ([`display_col`] of its end); ties go to the
/// last one, an empty slice to 0.
fn widest_line(lines: &[String]) -> usize {
    lines
        .iter()
        .enumerate()
        .max_by_key(|(_, line)| display_col(line, line.len()))
        .map_or(0, |(index, _)| index)
}
```

Run: `cargo test -p mkrew-ui --lib editor -j 4`
Expected: PASS.

- [ ] **Step 3: `render_editor` uses the cache and borrows the theme**

In `main.rs` `render_editor`, replace
```rust
        // Widest row in screen cells (tabs and multibyte chars differ from the byte length).
        let longest = (lines.iter().enumerate())
            .max_by_key(|(_, line)| editor::display_col(line, line.len()))
            .map(|(index, _)| index);
        let text_left = Rc::clone(&self.file_text_left);
        let row_theme = theme.clone();
```
with
```rust
        // Widest row in screen cells, kept by the editor per text version.
        let longest = Some(shown.editor.longest_line());
        let text_left = Rc::clone(&self.file_text_left);
```
In the `cx.processor(move |shell, range: std::ops::Range<usize>, _, cx| {` closure, after
```rust
                let Some(shown) = view.shown() else {
                    return Vec::new();
                };
```
add `let theme = cx.theme();` and in the same closure replace the argument line `&row_theme,` (the last argument of `preview::code_row(`) with `theme,`.

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors. If the borrow checker rejects `cx.theme()` held across `cx.listener(...)` in that closure, keep the clone but make it once per closure call, not per render: `let theme = cx.theme().clone();` at the same spot (still no clone per frame of the whole list build when the list is not repainted); report which one was used.

- [ ] **Step 4: B8: settings are checked when the window is activated**

`main.rs`, new method (put it next to `open_settings`, before it):

```rust
    /// Re-reads `settings.toml` when its mtime moved since the last look (ADR 0044). Run at
    /// start and whenever the window is activated, never on a timer: a hand edit of fonts,
    /// theme or `diff_mode` applies when the user comes back to the window. A broken file keeps
    /// the last good settings and lists why in the palette (§20.0).
    fn reload_settings_if_changed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let modified = std::fs::metadata(&self.settings_file)
            .and_then(|m| m.modified())
            .ok();
        if modified == self.settings_modified {
            return;
        }
        self.settings_modified = modified;
        let installed = cx.text_system().all_font_names();
        let last = cx.global::<theme::Fonts>().clone();
        let mode = cx.theme().mode;
        match theme::reload(&self.settings_file, &installed, &last, mode) {
            Ok(changed) => {
                self.settings_error = None;
                if let Some((fonts, mode)) = changed {
                    cx.set_global(fonts);
                    theme::refresh(mode, window, cx);
                    cx.refresh_windows();
                }
                // `theme::reload` only returns fonts and theme; a hand edit of `diff_mode`
                // applies too (the file just parsed, so this second read only happens when
                // it changed).
                if let Ok(settings) = mkrew_core::settings::load(&self.settings_file) {
                    self.apply_diff_mode(settings.ui.diff_mode);
                }
            }
            // The last good settings stay; the palette lists why (§20.0).
            Err(error) => self.settings_error = Some(error),
        }
        cx.notify();
    }
```
`Shell::new`: replace
```rust
        cx.observe_window_activation(window, |_, _, cx| cx.notify())
            .detach();
```
with
```rust
        cx.observe_window_activation(window, |shell, window, cx| {
            // Back from Zed after a hand edit of settings.toml: fonts, theme and diff mode
            // apply now (ADR 0044).
            if window.is_window_active() {
                shell.reload_settings_if_changed(window, cx);
            }
            cx.notify();
        })
        .detach();
```
Status loop: replace
```rust
        cx.spawn_in(window, async move |this, cx| {
            let mut last = status::now_ms();
            loop {
```
with
```rust
        cx.spawn_in(window, async move |this, cx| {
            let mut last = status::now_ms();
            // The first look at settings.toml (the activation observer does the later ones).
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.reload_settings_if_changed(window, cx);
            });
            loop {
```
and delete, inside the loop's `this.update_in(cx, |shell, window, cx| {` closure, the whole poll block from the comment `// ponytail: mtime poll on the 500 ms tick; a file watcher if it ever shows` through the `}` that closes `if modified != shell.settings_modified {` (the `cx.notify();` + `}` before `let sessions = shell.sidebar.sessions();`, about 32 lines). Keep `let sessions = shell.sidebar.sessions();` and everything after it unchanged. (If the closure then does not use `window`, keep it: `window.is_visible()` below uses it.)

Field docs (~293-297): replace
```rust
    /// `<config>/mkrew/settings.toml`, read when a composer `@` pick is typed and polled for
    /// font changes (§20.0).
    settings_file: PathBuf,
    /// Its mtime at the last font check (ADR 0024); `None` until the first status tick reads it.
```
with
```rust
    /// `<config>/mkrew/settings.toml`, read when a composer `@` pick is typed and checked for
    /// changes when the window is activated (§20.0, ADR 0044).
    settings_file: PathBuf,
    /// Its mtime at the last check (ADR 0024); `None` until the first check reads it.
```

Compile check: `cargo check -p mkrew-ui -j 4`
Expected: no errors (no unused variable warnings from the status loop).
Run: `cargo test -p mkrew-ui --lib theme::tests::a_broken_settings_file -j 4`
Expected: PASS (the keep-the-last-good-settings behavior is unchanged; no new failing test for B8: the window activation callback is GPUI-only, and `theme::reload` carries the decision logic and its test).

- [ ] **Step 5: ADR 0044, SPEC and ADR 0024**

Create `docs/decisions/0044-settings-reload-on-activation.md`:

```markdown
# 0044 — The UI checks settings.toml when its window is activated, not every 500 ms

- Date: 2026-10-10
- Status: accepted
- Milestone: perf review (inbox item 11, B8)

## Context

ADR 0024 re-reads `settings.toml` when its mtime changes, checked on the 500 ms status tick.
That is a `std::fs::metadata` call on the UI thread twice a second for the whole life of the
app (`docs/PERF_REVIEW.md` B8). The file is hand-edited config: it changes while the user is in
Zed or another editor, so the window is not in front when it happens.

## Options

1. Keep the poll, check less often — still a timer on the UI thread; delays live feedback.
2. A file watcher in the UI — a new crate or thread for one small file; rejected in ADR 0024.
3. Check when the window is activated (and once at start) — no timer; the edit applies the
   moment the user comes back.

## Decision

Option 3. `Shell::reload_settings_if_changed` runs from `observe_window_activation` and once at
start. It keeps the mtime compare, `theme::reload` and the rule that a broken file keeps the
last good settings and shows the error in the palette (§20.0). The daemon is unchanged (it
watches its own config dir, 300 ms debounce).

## Consequences

- SPEC §20.0: "re-reads it within 500 ms of a change" becomes "re-reads it when its window is
  activated". ADR 0024 points here for the check.
- A settings edit made while the Mkrew window stays in front (for example by a script) applies
  at the next activation. The UI's own writes (theme toggle, inspector width, diff mode) need no
  reload: the state is already applied.
- HUMAN_CHECKS lists the check.
```

`docs/SPEC.md` §20.0, replace
```
`daemon.status` (`settings_error`) and by the palette like keymap errors. The UI reads it the
same way and re-reads it within 500 ms of a change (fonts apply live, ADR 0024). Palette
```
with
```
`daemon.status` (`settings_error`) and by the palette like keymap errors. The UI reads it the
same way at start and again each time its window is activated, if the file changed (no
polling, ADR 0044): fonts, theme and `diff_mode` apply when you come back to the window. Palette
```
`docs/decisions/0024-font-settings.md`, replace
```
`settings.toml` is re-read when its mtime changes, checked on the existing 500 ms status tick —
```
with
```
`settings.toml` is re-read when its mtime changes, checked when the window is activated (ADR 0044; first version: the 500 ms status tick) —
```

- [ ] **Step 6: HUMAN_CHECKS**

Append to the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Settings reload on activation (B8):** with Mkrew open, edit `settings.toml` in Zed (`[terminal] font_size = 16`, or `[ui] theme = "dark"`, or `diff_mode`), save, click back into the Mkrew window → the font size / theme / diff layout changes at once (it no longer changes while Zed is in front). Make the file invalid (`font_size = 4`), save, come back → nothing changes, the palette (Ctrl+Shift+P) lists `Settings error: settings.toml:…` first; fix it, come back → the error is gone. Start Mkrew with a hand-edited file → its fonts apply at start.
- [ ] **Files editor (B11):** open a large file (≥ 1 MB) in Files and type, hold an arrow key, scroll → typing and scrolling feel smooth (no per-frame stall); the horizontal scroll width still fits the widest line (a very long line scrolls fully; after deleting it the width shrinks; a line with tabs scrolls as wide as it looks). Dark and light look the same as before.
- [ ] **CPU before / after (item 11):** with 2–3 agents `working` and the window visible, read `mkrew-ui` CPU in Task Manager over one minute: before this plan (note the number from your last build) vs now. Write both numbers under this item in `docs/PERF_REVIEW.md` (end of the file) or here.
```

- [ ] **Step 7: Verify and tick**

Run: `cargo test -p mkrew-ui --lib editor theme docs files_view git_summary -j 4` (expect PASS; or run each module filter separately), then the full verification.
Tick `docs/INBOX.md` line 115: change `- [~] **11. Perf: stop whole-app re-renders in the UI**` (or `- [ ]`) to `- [x] **11. Perf: stop whole-app re-renders in the UI**` and, if it has no plan suffix, add ` → plan: docs/superpowers/plans/2026-10-10-inbox-perf.md Tasks 1–6` after `(perf B1, B2, B3, B4, B8, B11, A6)` like line 107.
Suggested message: `perf(ui): check settings.toml on window activation and cache the editor width`

---

### Task 7: SQLite writes: no `hook` events, `synchronous=NORMAL`, backup on its own connection, pruning

Inbox: line 126 `12. Perf: daemon watcher, git.changed and SQLite` (bullets A4 and B7; do not tick yet). Spec: §5 (SPEC.md:251-257 table comment, :348 WAL line), §8 (SPEC.md:646 backups), §16 NFR table (:1242).

**Current state (investigated):**
- A4. `Actor::on_hook` (`crates/mkrew-daemon/src/session.rs:1219`) builds `record = {agent, event, payload}` and calls `self.hub.store().insert_session_event(self.session.id, "hook", &record, at)` on EVERY `hook.report` (lines 1224-1235). `let at = crate::server::now_ms();` (1220) is only used by that insert.
- Every reader of `session_event` (grep `session_events(` and `project_session_events(` over `crates`):
  - `server/doc.rs:29` kind `files_changed` (`doc.list`, last 24 h, `docs::RECENT_MS`).
  - `server/turn.rs:112` kind `edited` (`session.files`, `since_ms` = UI sends now − 24 h, `git_view.rs:750`).
  - `server/turn.rs:143` kind `usage` with `since = 0` (`session.usage`: the newest usage of each session; totals survive restarts, SPEC §20E.3 :1775-1776).
  - Tests only: `mkrew-store/src/tests.rs:345` and `:999`, `mkrew-daemon/tests/daemon.rs:959`, `:1002` (kind `hook`), `:1266` (`files_changed`), `:2184` writes a `usage` row.
  - **No production code reads kind `hook`.** `state_changed` is never written either (only named in the migration comment `0001_init.sql:46` and SPEC :254).
- Pruning: the two 24 h readers need rows at most 1 day old, so `files_changed`, `edited` and the old `hook` rows are safe to drop after 7 days. **`usage` is NOT pruned**: `session.usage` reads it with `since = 0` and the UI totals (inspector, status bar) need the newest usage row of every session, including sessions older than a week.
- B7. `Store::open` (`crates/mkrew-store/src/lib.rs:98-105`) sets `journal_mode=WAL` (100) and `foreign_keys` (101); `synchronous` stays at the default FULL. `Store::backup_to` (:114) runs `VACUUM INTO` on the Store's own connection. `backup::run(store: &Store, …)` (`backup.rs:70`) is called from `backup::start` (:35-37) as `run(&hub.store(), &dir, now)`, so the global `Hub::store` mutex is held for the whole copy and every other request that needs the store waits. The check runs hourly (`EVERY`) in `spawn_blocking`; it also calls `attachment::cleanup`, which takes `hub.store()` per session dir.
- `backup::start` is called from `server.rs:116-120` with `self.paths.backups_dir()` and `self.paths.attachments_dir()`; the database path is `Paths::db_file()` (`mkrew-core/src/paths.rs:57`).
- Tests: `backup.rs` `mod tests` (`use super::*`, `NOON`, `names(dir)`); `mkrew-store/src/tests.rs` (helpers `open(&dir)`, `session(project_id, started_at)`; `backup_to_writes_a_readable_copy` at :61-74; `open_runs_migrations_and_enables_wal` at :36-47).
- SPEC: :254 comment `state_changed|hook|files_changed|...`; :348 `SQLite runs in WAL mode.`; :646 `Backups: daily copy of mkrew.db …, keep 7`; :1242 `Data | SQLite WAL; daily DB backup, keep 7`. No statement about durability or `synchronous`. No ADR needed: no behavior in the SPEC is contradicted (the table stays, no migration).

**Files:**
- Modify: `crates/mkrew-store/src/lib.rs` (`Store::open`, replace `Store::backup_to` with free fn `backup`, add `Store::prune_session_events`)
- Modify: `crates/mkrew-store/src/tests.rs`
- Modify: `crates/mkrew-daemon/src/backup.rs` (`start`, `run`, new `prune_events`, tests)
- Modify: `crates/mkrew-daemon/src/server.rs` (call of `backup::start`, :116)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Actor::on_hook`)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (`hook_reports_drive_the_state_of_live_sessions_only`, `screen_only_profiles_record_hooks_but_ignore_them`)
- Modify: `docs/SPEC.md` (:254, :348, :646, :1242), `docs/HUMAN_CHECKS.md` (append to the shared section)

**Interfaces:**
- Produces: `mkrew_store::backup(db: &Path, to: &Path) -> Result<()>` (own connection; replaces `Store::backup_to`, whose only callers are `backup.rs:86` and the store tests).
- Produces: `Store::prune_session_events(&self, kind: &str, before: i64, limit: usize) -> Result<usize>` (rows of `kind` with `at < before`, at most `limit`, returns how many were deleted).
- Produces: `backup::run(db: &Path, dir: &Path, now_ms: i64) -> anyhow::Result<Option<PathBuf>>`, `backup::prune_events(hub: &Hub, now_ms: i64, chunk: usize) -> Result<usize, mkrew_store::StoreError>`, `backup::start(hub: Arc<Hub>, db: PathBuf, dir: PathBuf, attachments: PathBuf) -> JoinHandle<()>`.

- [ ] **Step 1: Failing tests**

`crates/mkrew-store/src/tests.rs`: add after `open_runs_migrations_and_enables_wal`:

```rust
/// WAL + `synchronous=NORMAL`: a commit does not fsync the log (perf B7).
#[test]
fn commits_use_synchronous_normal() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let level: i64 = store
        .conn
        .pragma_query_value(None, "synchronous", |r| r.get(0))
        .unwrap();
    assert_eq!(level, 1, "0 = OFF, 1 = NORMAL, 2 = FULL");
}
```

Replace the test `backup_to_writes_a_readable_copy` (:61-74): rename it `backup_writes_a_readable_copy`, replace `store.backup_to(&copy).unwrap();` with `backup(&dir.path().join("mkrew.db"), &copy).unwrap();` and `assert!(store.backup_to(&copy).is_err());` with `assert!(backup(&dir.path().join("mkrew.db"), &copy).is_err());`. Then add after it:

```rust
/// The copy is made on its own connection: a writer in the middle of a transaction neither
/// blocks it (WAL) nor leaks its uncommitted row into the copy.
#[test]
fn backup_does_not_wait_for_a_writer_and_skips_its_uncommitted_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("mkrew.db");
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    store
        .insert_session_event(s.id, "usage", &serde_json::json!({}), 1)
        .unwrap();

    let writer = Connection::open(&db).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    writer
        .execute(
            "INSERT INTO session_event (session_id, kind, payload, at) VALUES (?1, 'files_changed', '{}', 2)",
            [s.id.to_string()],
        )
        .unwrap();

    let copy = dir.path().join("copy.db");
    backup(&db, &copy).unwrap();
    writer.execute_batch("COMMIT").unwrap();

    let restored = Store::open(&copy).unwrap();
    assert_eq!(restored.session_events(s.id, "usage").unwrap().len(), 1);
    assert!(restored.session_events(s.id, "files_changed").unwrap().is_empty());
    assert_eq!(store.session_events(s.id, "files_changed").unwrap().len(), 1);
}
```

`crates/mkrew-daemon/src/backup.rs` `mod tests`: add the imports and tests at the end of the module (before its closing `}`):

```rust
    use std::sync::Arc;

    use mkrew_protocol::{Session, SessionKind, SessionState, StateSource};
    use ulid::Ulid;

    /// A hub whose store holds one session with a `session_event` row per `(kind, at)`.
    fn hub_with_events(events: &[(&str, i64)]) -> (tempfile::TempDir, Arc<Hub>, Ulid) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project = store.add_project(dir.path(), Some("repo")).unwrap();
        let id = Ulid::new();
        store
            .insert_session(&Session {
                id,
                task_id: None,
                project_id: project.id,
                agent_profile: "fake".into(),
                kind: SessionKind::Agent,
                title: None,
                terminal_title: None,
                color: None,
                native_session_id: None,
                state: SessionState::Idle,
                state_source: StateSource::Daemon,
                state_changed_at: 0,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 0,
                ended_at: None,
                pinned: false,
                plan_path: None,
            })
            .unwrap();
        for &(kind, at) in events {
            store
                .insert_session_event(id, kind, &serde_json::json!({}), at)
                .unwrap();
        }
        (dir, Arc::new(Hub::new(store)), id)
    }

    /// The `at` of the session's remaining events of `kind`, oldest first.
    fn left(hub: &Hub, session: Ulid, kind: &str) -> Vec<i64> {
        let events = hub.store().session_events(session, kind).unwrap();
        events.iter().map(|e| e.at).collect()
    }

    /// Rows of the pruned kinds go once they are older than a week (several chunks);
    /// exactly 7 days old and newer rows stay, and so does every `usage` row: `session.usage`
    /// totals read them with `since = 0`.
    #[test]
    fn events_past_a_week_are_pruned_but_recent_ones_and_usage_stay() {
        let now = 100 * DAY_MS;
        let old = now - EVENT_KEEP_MS - 1;
        let edge = now - EVENT_KEEP_MS;
        let recent = now - DAY_MS;
        let mut events = vec![("hook", old); 5];
        events.extend([
            ("hook", recent),
            ("files_changed", old),
            ("files_changed", edge),
            ("edited", old),
            ("usage", old),
            ("usage", recent),
        ]);
        let (_dir, hub, session) = hub_with_events(&events);

        assert_eq!(prune_events(&hub, now, 2).unwrap(), 7);

        assert_eq!(left(&hub, session, "hook"), [recent]);
        assert_eq!(left(&hub, session, "files_changed"), [edge]);
        assert!(left(&hub, session, "edited").is_empty());
        assert_eq!(left(&hub, session, "usage"), [old, recent]);
    }

    /// The hourly check writes today's backup without taking the store mutex: it is made
    /// while another thread holds the lock (a long write), and only the pruning after it
    /// waits for the lock.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_backup_is_written_while_the_store_mutex_is_held() {
        let data = tempfile::tempdir().unwrap();
        let db = data.path().join("mkrew.db");
        let hub = Arc::new(Hub::new(Store::open(&db).unwrap()));
        let dir = data.path().join("backups");
        let guard = hub.store();

        let task = start(Arc::clone(&hub), db, dir.clone(), data.path().join("attachments"));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !(dir.exists() && names(&dir).iter().any(|n| is_backup(n))) {
            assert!(
                std::time::Instant::now() < deadline,
                "no backup while the store mutex is held"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        drop(guard);
        task.abort();
    }
```

If `use super::*` no longer brings `Store` into scope after Step 3, add `use mkrew_store::Store;` to the same `mod tests`.

`crates/mkrew-daemon/tests/daemon.rs`:

In `hook_reports_drive_the_state_of_live_sessions_only`, replace

```rust
    let db = env.data_dir().join("mkrew.db");
    let events = Store::open(&db)
        .unwrap()
        .session_events(id, "hook")
        .unwrap();
    let names: Vec<&str> = events
        .iter()
        .map(|e| e.payload["event"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["needs_input", "busy", "thinking", "done"]);
    assert_eq!(events[0].payload["agent"], "fake");
    assert_eq!(events[0].payload["payload"]["session_id"], "native-1");
```

with

```rust
    // Raw hook payloads are not stored any more (perf A4); the state and the native id above
    // are what a report changes.
    let db = env.data_dir().join("mkrew.db");
    let events = Store::open(&db)
        .unwrap()
        .session_events(id, "hook")
        .unwrap();
    assert!(events.is_empty(), "{events:?}");
```

Rename `screen_only_profiles_record_hooks_but_ignore_them` to `screen_only_profiles_take_the_native_id_but_ignore_hook_states` and delete its last block (the `let db = …; let events = Store::open(&db)….session_events(id, "hook")….unwrap(); assert_eq!(events.len(), 1);`), keeping the two state assertions and the `native_session_id` assertion before it.

Run: `cargo test -p mkrew-store -j 4 commits_use_synchronous_normal` (left 2, right 1) and `cargo test -p mkrew-store -p mkrew-daemon --no-run -j 4`
Expected: FAIL: `commits_use_synchronous_normal` asserts 2 vs 1; the backup and prune tests fail to compile (`backup`, `prune_session_events`, `prune_events`, `EVENT_KEEP_MS`, the new `start` signature do not exist).

- [ ] **Step 2: Store: `synchronous`, own-connection backup, prune**

`crates/mkrew-store/src/lib.rs`, `Store::open`: replace

```rust
        conn.pragma_update(None, "foreign_keys", true)?;
```

with

```rust
        conn.pragma_update(None, "foreign_keys", true)?;
        // WAL + NORMAL: a commit no longer fsyncs the log. A crash of the app never loses a
        // commit; only an OS crash or power cut can lose the last few and the file stays
        // consistent (SPEC §5).
        conn.pragma_update(None, "synchronous", "NORMAL")?;
```

and in its doc comment replace `enables WAL and runs migrations.` with `enables WAL (synchronous NORMAL) and runs migrations.`.

Delete `Store::backup_to` (the doc comment `/// Writes a consistent copy of the database to `path` (`VACUUM INTO`, safe while in use).` through its closing `}`, lines 112-121) and add this free function after the `impl Store { … }` block that contains it ends, next to the other free fns (e.g. directly above `fn user_version`; find it with `grep -n "^fn user_version" crates/mkrew-store/src/lib.rs`):

```rust
/// Writes a consistent copy of the database file `db` to `to` (`VACUUM INTO`). It opens a
/// connection of its own, so the caller needs no [`Store`] (and no lock around one): in WAL
/// mode the copy reads one snapshot while writers go on, and never contains an uncommitted
/// row. SQLite refuses to overwrite an existing non-empty file.
pub fn backup(db: &Path, to: &Path) -> Result<()> {
    let target = to.to_str().ok_or_else(|| StoreError::Path {
        path: to.to_path_buf(),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is not UTF-8"),
    })?;
    let conn = Connection::open(db)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute("VACUUM INTO ?1", [target])?;
    Ok(())
}
```

In `impl Store`, after `project_session_events` (ends near :914, `.collect()` `}`), add:

```rust
    /// Deletes up to `limit` `session_event` rows of `kind` older than `before` (ms), oldest
    /// first, and returns how many went. Callers loop until it returns less than `limit`, so
    /// one call holds the connection only briefly.
    pub fn prune_session_events(&self, kind: &str, before: i64, limit: usize) -> Result<usize> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        Ok(self.conn.execute(
            "DELETE FROM session_event WHERE id IN (
                SELECT id FROM session_event WHERE kind = ?1 AND at < ?2 ORDER BY id LIMIT ?3)",
            params![kind, before, limit],
        )?)
    }
```

Run: `cargo test -p mkrew-store -j 4`
Expected: PASS (all store tests).

- [ ] **Step 3: Daemon: backup without the mutex, pruning in the hourly check**

`crates/mkrew-daemon/src/backup.rs`:

Replace `use mkrew_store::Store;` with nothing (delete the line; production code no longer uses it). After `const DAY_MS: i64 = 24 * 60 * 60 * 1000;` add:

```rust
/// `session_event` kinds nobody reads once they are a day old (`doc.list` and `session.files`
/// look back 24 h; `hook` is not written any more). `usage` is not here: `session.usage` reads
/// the newest usage row of every session, however old (SPEC §20E.3).
const PRUNED_KINDS: [&str; 3] = ["hook", "files_changed", "edited"];
/// Pruned events older than this are deleted.
const EVENT_KEEP_MS: i64 = 7 * DAY_MS;
/// Rows deleted per lock of the store, so a big backlog never stalls the daemon.
const PRUNE_CHUNK: usize = 5_000;
```

Replace the doc comment and signature of `start`:

```rust
/// Checks for today's backup now and then every [`EVERY`]; each check also deletes old
/// attachments in `attachments` ([`attachment::cleanup`], SPEC §20B.5). Runs until aborted.
pub fn start(hub: Arc<Hub>, dir: PathBuf, attachments: PathBuf) -> JoinHandle<()> {
```

with

```rust
/// Checks for today's backup now and then every [`EVERY`]; each check also deletes old
/// attachments in `attachments` ([`attachment::cleanup`], SPEC §20B.5) and old session events
/// ([`prune_events`]). The backup copies the database file `db` on its own connection, never
/// under the store mutex. Runs until aborted.
pub fn start(hub: Arc<Hub>, db: PathBuf, dir: PathBuf, attachments: PathBuf) -> JoinHandle<()> {
```

Replace

```rust
            let (hub, dir, attachments) = (Arc::clone(&hub), dir.clone(), attachments.clone());
            let done = tokio::task::spawn_blocking(move || {
                let now = crate::server::now_ms();
                let backed_up = run(&hub.store(), &dir, now);
                let cleaned = attachment::cleanup(&attachments, now, |id| {
                    hub.store().get_session(id).ok().and_then(|s| s.ended_at)
                });
                (backed_up, cleaned)
            })
            .await;
            let (backed_up, cleaned) = match done {
```

with

```rust
            let (hub, db, dir, attachments) = (
                Arc::clone(&hub),
                db.clone(),
                dir.clone(),
                attachments.clone(),
            );
            let done = tokio::task::spawn_blocking(move || {
                let now = crate::server::now_ms();
                let backed_up = run(&db, &dir, now);
                let pruned = prune_events(&hub, now, PRUNE_CHUNK);
                let cleaned = attachment::cleanup(&attachments, now, |id| {
                    hub.store().get_session(id).ok().and_then(|s| s.ended_at)
                });
                (backed_up, pruned, cleaned)
            })
            .await;
            let (backed_up, pruned, cleaned) = match done {
```

and after the `match backed_up { … }` block add:

```rust
            match pruned {
                Ok(0) => {}
                Ok(removed) => tracing::info!(removed, "old session events deleted"),
                Err(e) => tracing::warn!("session events not pruned: {e}"),
            }
```

Replace `run`'s doc and signature and its `backup_to` call:

```rust
/// Copies the database to today's file unless it already exists, then prunes to [`KEEP`].
/// Returns the file it created, if any.
pub fn run(store: &Store, dir: &Path, now_ms: i64) -> anyhow::Result<Option<PathBuf>> {
```
→
```rust
/// Copies the database file `db` to today's file unless it already exists, then prunes to
/// [`KEEP`]. Returns the file it created, if any.
pub fn run(db: &Path, dir: &Path, now_ms: i64) -> anyhow::Result<Option<PathBuf>> {
```

```rust
        store
            .backup_to(&partial)
            .with_context(|| format!("write {}", partial.display()))?;
```
→
```rust
        mkrew_store::backup(db, &partial)
            .with_context(|| format!("write {}", partial.display()))?;
```

Add after `run` (before `/// Deletes all but the newest `keep` backups`):

```rust
/// Deletes the events of [`PRUNED_KINDS`] older than [`EVENT_KEEP_MS`], `chunk` rows per lock
/// of the store. Returns how many were deleted.
pub fn prune_events(
    hub: &Hub,
    now_ms: i64,
    chunk: usize,
) -> Result<usize, mkrew_store::StoreError> {
    let before = now_ms - EVENT_KEEP_MS;
    let mut total = 0;
    for kind in PRUNED_KINDS {
        loop {
            // The guard is a temporary: the lock is free again between chunks.
            let removed = hub.store().prune_session_events(kind, before, chunk)?;
            total += removed;
            if removed < chunk {
                break;
            }
        }
    }
    Ok(total)
}
```

Existing tests in this module: in `one_backup_per_day_and_the_newest_seven_are_kept` and `a_leftover_partial_file_is_replaced` replace `let store = Store::open(&data.path().join("mkrew.db")).unwrap();` with

```rust
        let db = data.path().join("mkrew.db");
        Store::open(&db).unwrap();
```

and every `run(&store, ` with `run(&db, ` (`grep -n "run(&store" crates/mkrew-daemon/src/backup.rs`). Add `use mkrew_store::Store;` at the top of `mod tests` if it is not in scope.

`crates/mkrew-daemon/src/server.rs`: replace

```rust
        let backups = backup::start(
            Arc::clone(&self.hub),
            self.paths.backups_dir(),
```

with

```rust
        let backups = backup::start(
            Arc::clone(&self.hub),
            self.paths.db_file(),
            self.paths.backups_dir(),
```

Run: `cargo test -p mkrew-daemon --lib backup -j 4`
Expected: PASS (`events_past_a_week_are_pruned_but_recent_ones_and_usage_stay`, `the_backup_is_written_while_the_store_mutex_is_held` and the older backup tests).

- [ ] **Step 4: Stop writing `hook` events (A4)**

`crates/mkrew-daemon/src/session.rs`, `Actor::on_hook`: replace the doc comment

```rust
    /// Records the report as a `hook` session event, takes the agent's session id and lets
    /// the state machine decide (it ignores hooks for screen-only profiles).
    fn on_hook(&mut self, agent: HookAgent, report: HookReportParams) {
        let at = crate::server::now_ms();
        let native = agent.native_session_id(&report.payload);
```

with

```rust
    /// Takes the agent's session id, records the files the report names as edited and lets the
    /// state machine decide (it ignores hooks for screen-only profiles). The raw report is not
    /// stored: nothing reads it (perf A4).
    fn on_hook(&mut self, agent: HookAgent, report: HookReportParams) {
        let native = agent.native_session_id(&report.payload);
```

and delete this block:

```rust
        let record = serde_json::json!({
            "agent": report.agent,
            "event": report.event,
            "payload": report.payload,
        });
        if let Err(e) = self
            .hub
            .store()
            .insert_session_event(self.session.id, "hook", &record, at)
        {
            tracing::error!(id = %self.session.id, "saving hook event failed: {e}");
        }
```

Verify before deleting: `grep -rn '"hook"' crates --include=*.rs` shows only the profile `StateMode` serde name, `source_str`/`parse_source` (`StateSource::Hook`), the fake-agent scenario `action = "hook"` and the tests edited in Step 1.

Run: `cargo test -p mkrew-daemon --test daemon -j 4 hook_reports_drive screen_only_profiles_take`
Expected: PASS.

- [ ] **Step 5: SPEC**

- `docs/SPEC.md:254`: replace `-- state_changed|hook|files_changed|...` with `-- files_changed|edited|usage|... (hook reports are not stored)`.
- `:348`: replace `SQLite runs in WAL mode. Schema changes` with `SQLite runs in WAL mode with `synchronous=NORMAL` (a commit is not fsynced: an OS crash or power cut can lose the last few commits, never corrupt the file). Events of kinds `files_changed` and `edited` older than 7 days are deleted by the hourly maintenance check (§8); `usage` events are kept, `session.usage` needs them. Schema changes`.
- `:646`: replace `daily copy of `mkrew.db` to `<data>/mkrew/backups/`, keep 7.` with `daily copy of `mkrew.db` to `<data>/mkrew/backups/` (`VACUUM INTO` on its own connection, not under the daemon's store lock), keep 7. The same hourly check deletes old session events (§5).`
- `:1242`: replace `SQLite WAL; daily DB backup, keep 7` with `SQLite WAL (`synchronous=NORMAL`); daily DB backup, keep 7`.

- [ ] **Step 6: HUMAN_CHECKS**

Append to the end of the section `### To verify — 2026-10-10 inbox: perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)` (Task 1 created it):

```markdown
- [ ] **SQLite after the perf change:** start the app with a database that has a few weeks of use. After the first hour (or a restart: the check runs at startup) open `mkrew.db` with `sqlite3`: `select kind, count(*) from session_event group by kind;` has no `hook` rows older than 7 days, and no `files_changed` / `edited` rows older than 7 days; `usage` rows of old sessions are still there and the inspector / status bar still show their token totals. While an agent works, `select count(*) from session_event where kind='hook';` does not grow. Today's file in `backups/` opens in `sqlite3` and lists your sessions; during the backup (start the app, immediately click around) the UI does not freeze.
```

- [ ] **Step 7: Verify and tick**

Run: `cargo test -p mkrew-store -p mkrew-daemon -j 4` (expect PASS), then the full verification. Do not tick inbox line 126 yet (Tasks 8 and 9 follow).
Suggested message: `perf(daemon): no hook events, synchronous=NORMAL, backup off the store lock, prune old events`

---

### Task 8: Watcher: `git.changed` only for real changes, 5 s cap while working, release kept watchers

Inbox: line 126 (bullet B5; do not tick yet). Spec: §7 (SPEC.md:509 `session.ports` row, :535-536 `doc.*` rows, :577 `git.changed`, :605), §8, §12B (SPEC.md:1005), ADR 0042 (new).

**Current state (investigated):**
- `watch::watch` (`crates/mkrew-daemon/src/watch.rs:135-149`) starts one `RecursiveMode::Recursive` watcher over the project root. Every event path goes to `debounce` (:153), which filters afterwards with `Filter::relative_with` (`.git`, `node_modules`, `target`, `.gitignore`) and `Filter::is_git_state` (`HEAD`, `index`, `packed-refs`, `refs/…` of the root or of a direct child `.git`). The recursion itself cannot be narrowed without losing events of nested `.gitignore`d trees that `notify` has no exclude API for (`notify` 8 has no per-directory exclusion); the filtering stays, and a `.git`-only storm already costs no `stat` (`relative_with` skips `is_dir` for ignored dirs).
- `debounce` already sends nothing for an empty batch: `if !batch.is_empty() || git { on_batch(…) }` (:183). The review's "ALWAYS sends `GitChanged`" is the closure in `Hub::watch` (`session.rs:659-672`): `on_batch` runs only for a non-empty batch, but it broadcasts `Event::GitChanged` for any batch, including one with `files` empty and `git == false`. That cannot happen today, so this is not the leak; the real waste is a batch that reports only files the UI cannot care about is still non-empty. It is: each non-empty batch (a changed file, or a git-state file) sends `git.changed`, at most every `MAX_DELAY` = 2 s while files keep changing (an agent writing all the time ⇒ a `git status` per UI client every 2 s). The send condition is therefore made explicit (`files` non-empty or `git`), and the cap is raised while an agent works.
- `MAX_DELAY` (`watch.rs:18`, 2 s) is read in `debounce` (`let deadline = tokio::time::Instant::now() + MAX_DELAY;`). The only user of the constant outside `watch.rs` is a comment in `tests/daemon.rs:1483` (`two_sessions_editing_one_file_raise_a_conflict`).
- Where `working` is known: `Actor::set_state` (`session.rs:1326`) is the single place a session's state is assigned (`self.session.state = state;`). The Hub keeps no state per session (only the store row, which `files_changed` reads with `list_sessions`). This task adds `Hub::working: Mutex<HashMap<Ulid, Ulid>>` (session → project), updated in `set_state`, used by the watcher's delay and by Task 9.
- Kept watchers: `Hub::watch` (`session.rs:645`) stores `ProjectWatch { _watch, kept }` in `Hub::watches`; `kept |= true` by `keep_watched` (project; callers `server/doc.rs:86` `watched_project` for `doc.list`/`doc.read`, `server/target.rs:131-137` `watched_target` for `fs.list`/`fs.read`/`fs.find`/`git.status`…) and `keep_worktree_watched` (task worktree, key = task id; same `watched_target`). Nothing clears `kept`: only `unwatch_worktree` (task worktree removal, `server/task_worktree.rs:90`) and `unwatch_idle_project` (last session exit, only when not kept) remove a watch.
- How the UI shows a view: `Shell::show_view` (`crates/mkrew-ui/src/main.rs:1559`) sets `self.views[project]`, and for Docs / Files / Git calls `load_docs` (`doc.list`), `refresh_files` (`fs.list`), `refresh_git` (`git.status`) when it was not already shown; `activate_workspace` (:1246) does the same on a workspace switch; `set_files_target` (:1795) and `retarget_git` (:2589, followed by `refresh_git`) change the task-worktree target. The title bar summary (`load_git_summary` :2669 → `git.status` through `git_summary_of`, :17277) also calls `git.status` for the shown workspace, so a shown workspace always keeps its project watcher alive (needed: the title bar follows `git.changed`).
- Existing protocol has no request that says a view closed (`Request` variants: no `view.*`, `watch.*`; `session.ports` / `session.usage` take `ProjectRef`). `ProjectRef { project_id }` (`mkrew-protocol/src/files.rs:14`) cannot name a worktree target, so a new request is needed: `watch.release`. That is the protocol bump 37 → 38 (`PROTOCOL_VERSION`, `mkrew-protocol/src/lib.rs:16`).
- Existing tests: `watch.rs` `mod tests` (`debounce_batches_until_quiet_and_caps_the_delay`, `debounce_flags_git_state_changes_without_files`, `watcher_reports_project_changes_but_not_ignored_ones`, `start_paused` virtual time); `session.rs` `mod tests` (`use super::*`, only pacer / flood tests, no Hub fixture); `message.rs` `every_m1_request_round_trips` (:1729) and `sid()` (:1671).

**Files:**
- Modify: `crates/mkrew-daemon/src/watch.rs` (`MAX_DELAY_WORKING`, `max_delay`, `watch`, `debounce`, tests)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Hub` fields + `new`, `set_working`, `has_working`, `release_watch`, `Hub::watch`, `Actor::set_state`, tests)
- Modify: `crates/mkrew-daemon/src/server.rs` (dispatch arm), `crates/mkrew-daemon/src/server/target.rs` (`watch_release`)
- Modify: `crates/mkrew-protocol/src/files.rs` (`WatchReleaseParams`), `crates/mkrew-protocol/src/message.rs` (`Request::WatchRelease`, `GitChanged` doc, tests), `crates/mkrew-protocol/src/lib.rs` (`PROTOCOL_VERSION`)
- Create: `crates/mkrew-ui/src/watches.rs`; Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod watches;`), `crates/mkrew-ui/src/main.rs` (`Shell.watch_needs`, `Shell::sync_watches`, four call sites)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (comment at :1483)
- Create: `docs/decisions/0042-release-kept-watchers.md`
- Modify: `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces: `watch::MAX_DELAY_WORKING: Duration` (5 s), `watch::max_delay(working: bool) -> Duration`, `watch::watch(root: &Path, max_delay: impl Fn() -> Duration + Send + 'static, on_batch: impl Fn(Batch) + Send + 'static) -> notify::Result<Watch>`.
- Produces: `Hub::set_working(&self, id: Ulid, project_id: Ulid, working: bool)`, `Hub::has_working(&self, project_id: Ulid) -> bool`, `Hub::working_sessions(&self, project_id: Ulid) -> Vec<Ulid>`, `Hub::release_watch(&self, project_id: Ulid, worktree_task_id: Option<Ulid>)`.
- Produces: `mkrew_protocol::WatchReleaseParams { project_id: Ulid, worktree_task_id: Option<Ulid> }` and `Request::WatchRelease(WatchReleaseParams)` (wire `watch.release`, result `null`). `PROTOCOL_VERSION` = 38.
- Produces (UI): `watches::Needs::of(workspace: Option<Ulid>, view: View, files: Option<Ulid>, git: Option<Ulid>) -> Needs`, `Needs::released(&self, now: &Needs) -> Vec<WatchReleaseParams>`.

- [ ] **Step 1: Failing tests (watch.rs)**

In `crates/mkrew-daemon/src/watch.rs` `mod tests`, add at the end (before the closing `}`):

```rust
    /// Sends one change every 100 ms from now on (the stream never goes quiet) and returns
    /// how long after the first change the batch was cut.
    async fn time_to_cut(
        tx: &mpsc::UnboundedSender<Vec<PathBuf>>,
        root: &Path,
        out: &mut mpsc::UnboundedReceiver<tokio::time::Instant>,
    ) -> Duration {
        let start = tokio::time::Instant::now();
        for i in 0..100 {
            tx.send(vec![root.join(format!("f{i}"))]).unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(at) = out.try_recv() {
                return at - start;
            }
        }
        panic!("no batch after 10 s of changes");
    }

    /// A stream of changes is cut after 5 s while a session is working and after 2 s when it
    /// is not; the flag is read when each batch starts (perf B5).
    #[tokio::test(start_paused = true)]
    async fn a_working_session_stretches_the_cap_to_five_seconds() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let working = Arc::new(AtomicBool::new(true));
        let flag = Arc::clone(&working);
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, mut out) = mpsc::unbounded_channel();
        tokio::spawn(debounce(
            Filter::new(&root),
            rx,
            move || max_delay(flag.load(Ordering::SeqCst)),
            move |_: Batch| {
                let _ = out_tx.send(tokio::time::Instant::now());
            },
        ));

        assert_eq!(
            time_to_cut(&tx, &root, &mut out).await,
            Duration::from_secs(5)
        );
        // Let the change sent at the deadline go out in its own batch, then start clean.
        tokio::time::sleep(Duration::from_secs(1)).await;
        while out.try_recv().is_ok() {}

        working.store(false, Ordering::SeqCst);
        assert_eq!(
            time_to_cut(&tx, &root, &mut out).await,
            Duration::from_secs(2)
        );
    }

    /// Events that only touch git internals (objects, lock files, logs) or ignored folders
    /// deliver nothing: no batch, so no `git.changed`.
    #[tokio::test(start_paused = true)]
    async fn git_internals_and_ignored_folders_alone_deliver_no_batch() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, mut out) = mpsc::unbounded_channel();
        let task = tokio::spawn(debounce(Filter::new(&root), rx, || MAX_DELAY, move |batch| {
            let _ = out_tx.send(batch);
        }));
        tx.send(vec![
            root.join(".git/objects/ab/cd"),
            root.join(".git/index.lock"),
            root.join(".git/logs/HEAD"),
            root.join("target/debug/x.o"),
            root.join("node_modules/a/b.js"),
        ])
        .unwrap();
        tokio::time::sleep(QUIET * 3).await;
        assert!(out.try_recv().is_err(), "an empty batch was delivered");

        tx.send(vec![root.join("a.rs")]).unwrap();
        drop(tx);
        task.await.unwrap();
        assert_eq!(out.recv().await.unwrap().files, ["a.rs"]);
    }
```

In the same `mod tests`, update the three existing call sites for the new signatures:
- in `debounce_batches_until_quiet_and_caps_the_delay` replace `tokio::spawn(debounce(Filter::new(&root), rx, move |batch: Batch| {` with `tokio::spawn(debounce(Filter::new(&root), rx, || MAX_DELAY, move |batch: Batch| {`;
- in `debounce_flags_git_state_changes_without_files` replace `tokio::spawn(debounce(Filter::new(&root), rx, move |batch| {` with `tokio::spawn(debounce(Filter::new(&root), rx, || MAX_DELAY, move |batch| {`;
- in `watcher_reports_project_changes_but_not_ignored_ones` replace `let _watch = watch(&root, move |batch| {` with `let _watch = watch(&root, || MAX_DELAY, move |batch| {`.

Hub tests: in `crates/mkrew-daemon/src/session.rs` `mod tests` (ends with `a_session_floods_past_the_bytes_of_one_window`), add at the end:

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

    /// A session of the project registered as live (no task behind it).
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

    /// Docs / Git opened → kept; the view closing releases it at once, except while a session
    /// of the project is live (its exit drops the watch then, as it always did).
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

    /// A task worktree's watcher is only kept for its Files / Git view: release drops it, and
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
```

Protocol tests: in `crates/mkrew-protocol/src/message.rs` `every_m1_request_round_trips`, after the line `Request::SessionPorts(ProjectRef { project_id: sid() }),` (:1892) add:

```rust
            Request::WatchRelease(WatchReleaseParams {
                project_id: sid(),
                worktree_task_id: Some(sid()),
            }),
```

and in the same `mod tests`, after `param_less_requests_omit_params`, add:

```rust
    /// `worktree_task_id` may be left out: the project's own watcher.
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

(If `Message::Request` has other field names, mirror the shape used by `request_wire_shape_matches_spec` at :1698; read it first with `read crates/mkrew-protocol/src/message.rs:1698-1716`.)

Run: `cargo test -p mkrew-daemon --lib watch -j 4` and `cargo test -p mkrew-protocol -j 4`
Expected: FAIL to compile (`max_delay`, new `debounce`/`watch` signatures, `release_watch`, `WatchReleaseParams`, `WatchRelease` do not exist).

- [ ] **Step 2: `watch.rs`: delay by working state**

After `pub const MAX_DELAY: Duration = Duration::from_secs(2);` add:

```rust
/// [`MAX_DELAY`] while a session of the project is `working`: an agent writes files all the
/// time, and every batch makes each UI client run `git status` (perf B5).
pub const MAX_DELAY_WORKING: Duration = Duration::from_secs(5);

/// The longest a batch may wait after its first change.
pub fn max_delay(working: bool) -> Duration {
    if working { MAX_DELAY_WORKING } else { MAX_DELAY }
}
```

Replace the doc comment and signature of `watch`:

```rust
/// reported too (T5.6 attributes such paths like files). Non-UTF-8 paths are skipped. Must run
/// inside the tokio runtime.
pub fn watch(root: &Path, on_batch: impl Fn(Batch) + Send + 'static) -> ::notify::Result<Watch> {
```

with

```rust
/// reported too (T5.6 attributes such paths like files). Non-UTF-8 paths are skipped.
/// `max_delay` is asked when a batch starts (see [`max_delay`]); a batch with nothing to report
/// after filtering is never delivered. Must run inside the tokio runtime.
pub fn watch(
    root: &Path,
    max_delay: impl Fn() -> Duration + Send + 'static,
    on_batch: impl Fn(Batch) + Send + 'static,
) -> ::notify::Result<Watch> {
```

Replace `tokio::spawn(debounce(Filter::new(root), rx, on_batch));` with `tokio::spawn(debounce(Filter::new(root), rx, max_delay, on_batch));`.

Replace the `debounce` doc comment and signature:

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
/// Collects paths until [`QUIET`] passes without a change (or `max_delay()` since the first
/// one), then delivers the batch. Ends when the watcher is dropped.
async fn debounce(
    filter: Filter,
    mut rx: mpsc::UnboundedReceiver<Vec<PathBuf>>,
    max_delay: impl Fn() -> Duration,
    on_batch: impl Fn(Batch),
) {
```

and replace `let deadline = tokio::time::Instant::now() + MAX_DELAY;` with `let deadline = tokio::time::Instant::now() + max_delay();`.

Also replace the module doc's first sentence in line 1-2 only if it says "with live sessions": leave.

- [ ] **Step 3: Hub: working map, delay, release**

`crates/mkrew-daemon/src/session.rs`, `Hub` struct: after the `watches` field (the line `watches: Mutex<HashMap<Ulid, ProjectWatch>>,`) add:

```rust
    /// Sessions in `working` (any kind) and their project, kept by `Actor::set_state`: the
    /// watcher's delay and the attribution of a batch read it instead of the store. Never held
    /// with another lock.
    working: Mutex<HashMap<Ulid, Ulid>>,
```

In `Hub::new`, after `watches: Mutex::default(),` add `working: Mutex::default(),`.

After `unwatch_worktree` add:

```rust
    /// A client closed the view that kept the watcher of project `project_id` (or of the
    /// worktree of task `worktree_task_id`): stops it unless it is not kept anymore only
    /// because a session of the project is live (that session's exit drops it then, see
    /// [`Hub::unwatch_idle_project`]).
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

    /// Records whether session `id` of `project_id` is `working`.
    fn set_working(&self, id: Ulid, project_id: Ulid, working: bool) {
        let mut sessions = lock(&self.working);
        if working {
            sessions.insert(id, project_id);
        } else {
            sessions.remove(&id);
        }
    }

    /// Is a session of the project `working`?
    fn has_working(&self, project_id: Ulid) -> bool {
        lock(&self.working).values().any(|&p| p == project_id)
    }

    /// The sessions of the project in `working`, from memory.
    fn working_sessions(&self, project_id: Ulid) -> Vec<Ulid> {
        lock(&self.working)
            .iter()
            .filter(|&(_, &p)| p == project_id)
            .map(|(&id, _)| id)
            .collect()
    }
```

Lock order: `release_watch` takes `watches`, drops it, then `unwatch_idle_project` takes `watches` then `live`: the documented order holds. `unwatch_worktree` needs no `kept` change: the worktree watch is only ever kept.

Update the `ProjectWatch` doc (:186-188) from `so it stays watched after its last session exits, until the daemon stops (HUMAN_CHECKS T6.4).` to `so it stays watched after its last session exits, until the client releases it (`watch.release`, ADR 0042) or the daemon stops (HUMAN_CHECKS T6.4).`; and the docs of `keep_watched` (:615-616) and `keep_worktree_watched` (:621-623): replace `until the daemon stops` with `until released (`Hub::release_watch`) or the daemon stops`.

`Hub::watch`: replace

```rust
        let hub = Arc::downgrade(self);
        let started = watch::watch(root, move |batch| {
```

with

```rust
        let hub = Arc::downgrade(self);
        let delay_hub = Arc::downgrade(self);
        let max_delay = move || {
            watch::max_delay(
                delay_hub
                    .upgrade()
                    .is_some_and(|hub| hub.has_working(project_id)),
            )
        };
        let started = watch::watch(root, max_delay, move |batch| {
```

and make the send condition explicit: replace

```rust
                let _ = hub.events.send(Event::GitChanged(GitChangedEvent {
                    project_id,
                    worktree_task_id,
                }));
```

with

```rust
                // `watch` only delivers a batch that has files or a git-state change.
                if !batch.files.is_empty() || batch.git {
                    let _ = hub.events.send(Event::GitChanged(GitChangedEvent {
                        project_id,
                        worktree_task_id,
                    }));
                }
```

`Actor::set_state`: after `self.session.state = state;` add

```rust
        self.hub
            .set_working(self.session.id, self.session.project_id, state == SessionState::Working);
```

The `Hub::watch` doc (`Every batch also broadcasts `git.changed`: a reported file or the git state changed.`) stays true.

- [ ] **Step 4: Protocol and daemon request**

`crates/mkrew-protocol/src/files.rs`, after `ProjectRef`:

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

`message.rs`, in `Request` after the `SessionPorts(ProjectRef),` variant:

```rust
    /// Returns nothing (`null`): the client closed the view (Docs, Files, Git, title bar) that
    /// kept the project's file watcher — or, with `worktree_task_id`, that task worktree's —
    /// running. The watcher stops unless a session of the project is live (SPEC §8, ADR 0042).
    #[serde(rename = "watch.release")]
    WatchRelease(WatchReleaseParams),
```

Update the `GitChanged` doc (:368) from `(debounced, ≤ once per 300 ms; SPEC §12B)` to `(debounced: 300 ms quiet, at most 2 s after the first change, 5 s while a session of the project is working; never sent for a batch with nothing to report; SPEC §12B)`.

`lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 37;` → `= 38;`. Check nothing else hard-codes 37: `grep -rn "\b37\b" crates --include=*.rs` (only `paint.rs` ANSI `37`, unrelated).

`server.rs`, next to `Request::SessionPorts(params) => {…}` (:433) add the arm:

```rust
            Request::WatchRelease(params) => (self.watch_release(params), After::Continue),
```

`server/target.rs`: change the import `use mkrew_protocol::{ErrorCode, Outcome, Project};` to `use mkrew_protocol::{ErrorCode, Outcome, Project, WatchReleaseParams};` and `use super::{Daemon, store_outcome, task_dir};` to `use super::{Daemon, ok, store_outcome, task_dir};`; at the end of `impl Daemon` (after `watched_target`) add:

```rust
    /// `watch.release`: the client left the view that kept this watcher (ADR 0042).
    pub(super) fn watch_release(&self, params: WatchReleaseParams) -> Outcome {
        self.hub
            .release_watch(params.project_id, params.worktree_task_id);
        ok(&())
    }
```

Run: `cargo test -p mkrew-protocol -j 4` and `cargo test -p mkrew-daemon --lib -j 4`
Expected: PASS (all `watch`, `session`, `backup` unit tests; the round-trip tests).

- [ ] **Step 5: UI: say when a view closes**

Create `crates/mkrew-ui/src/watches.rs`:

```rust
//! Which daemon file watchers this window still needs (SPEC §8, ADR 0042). The daemon keeps a
//! project's watcher after Docs / Files / Git asked for it; [`Needs::released`] names the ones
//! to give back with `watch.release` when the window stops showing what needed them. Pure and
//! tested headlessly; `main.rs` sends the requests.

use std::collections::BTreeSet;

use mkrew_protocol::WatchReleaseParams;
use ulid::Ulid;

use crate::board::View;

/// The watchers in use: `(project, None)` = the project folder, `(project, Some(task))` = a
/// task worktree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Needs(BTreeSet<(Ulid, Option<Ulid>)>);

impl Needs {
    /// What showing `workspace` in `view` uses. The shown workspace always needs its project
    /// watcher (the title bar git summary follows `git.changed`). The Files or Git view adds
    /// its task worktree (`files` / `git` = the target each view has) only while it is shown.
    pub fn of(workspace: Option<Ulid>, view: View, files: Option<Ulid>, git: Option<Ulid>) -> Self {
        let mut needs = BTreeSet::new();
        if let Some(project) = workspace {
            needs.insert((project, None));
            let task = match view {
                View::Files => files,
                View::Git => git,
                View::Board | View::Terminals | View::Docs => None,
            };
            if let Some(task) = task {
                needs.insert((project, Some(task)));
            }
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
        let before = Needs::of(Some(a), View::Git, None, Some(task));
        let after = Needs::of(Some(b), View::Git, None, None);
        assert_eq!(
            before.released(&after),
            [release(a, None), release(a, Some(task))]
        );
    }

    /// Leaving a worktree's Files / Git view releases only that worktree: the shown project
    /// keeps its watcher for the title bar, and the project folder's own views release nothing.
    #[test]
    fn leaving_a_worktree_view_keeps_the_shown_project() {
        let (a, task) = (Ulid(1), Ulid(3));
        let files = Needs::of(Some(a), View::Files, Some(task), None);
        let terminals = Needs::of(Some(a), View::Terminals, Some(task), None);
        assert_eq!(files.released(&terminals), [release(a, Some(task))]);

        let on_project = Needs::of(Some(a), View::Git, None, None);
        assert!(on_project.released(&terminals).is_empty());
        assert!(terminals.released(&on_project).is_empty());
    }

    /// Each view counts only its own target, and picking another worktree gives back the old.
    #[test]
    fn a_view_counts_its_own_target_and_a_retarget_releases_the_old_one() {
        let (a, t1, t2) = (Ulid(1), Ulid(3), Ulid(4));
        // Files points at a worktree, but the Git view is shown on the project folder.
        assert_eq!(
            Needs::of(Some(a), View::Git, Some(t1), None),
            Needs::of(Some(a), View::Git, None, None)
        );
        let before = Needs::of(Some(a), View::Git, None, Some(t1));
        let after = Needs::of(Some(a), View::Git, None, Some(t2));
        assert_eq!(before.released(&after), [release(a, Some(t1))]);
    }
}
```

(`Ulid(n)` constructs from a `u128`, as `git_view.rs` tests do. The `View` enum is `crate::board::View`.)

`crates/mkrew-ui/src/lib.rs`: add `pub mod watches;` in alphabetical position among the `pub mod` lines (after the last one starting before `w`, e.g. after `pub mod tabs;`/`pub mod terminal;`).

`crates/mkrew-ui/src/main.rs`:
- `Shell` struct: after the field `git_summary_busy: bool,` (:410) add `/// The watchers the daemon keeps for what this window shows (`watches`).` and `watch_needs: watches::Needs,`; in `Shell::new`'s initializer, after `git_summary_busy: false,` (:780) add `watch_needs: watches::Needs::default(),`. Add `watches` to the `use mkrew_ui::{…}` list that already imports `run`, `board`, … (`grep -n "^use mkrew_ui" crates/mkrew-ui/src/main.rs`), and `WatchReleaseParams`-free: the requests are built with `Request::WatchRelease(params)` (`Request` is already imported).
- New method, after `show_view`'s closing brace (before `/// `doc.list` for the Docs view`):

```rust
    /// Gives the daemon back the file watchers this window no longer shows anything for
    /// (`watch.release`, [`watches`]). Cheap: it compares two small sets.
    fn sync_watches(&mut self, cx: &mut Context<'_, Self>) {
        let target = |views: Option<Option<Ulid>>| views.flatten();
        let files = target(
            self.workspace
                .and_then(|id| self.files.get(&id))
                .map(|view| view.worktree_task_id),
        );
        let git = target(
            self.workspace
                .and_then(|id| self.git.get(&id))
                .map(|view| view.worktree_task_id),
        );
        let needs = watches::Needs::of(self.workspace, self.view(), files, git);
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

  (Simplify the `target` closure if rustc/clippy prefers: `self.workspace.and_then(|id| self.files.get(&id)).and_then(|view| view.worktree_task_id)` is the same value.)
- Call it where the shown view, workspace or worktree target changes:
  - `show_view`: replace
    ```rust
        if shown != view {
            self.sync_panes(window, cx);
            cx.notify();
        }
    }
    ```
    with
    ```rust
        self.sync_watches(cx);
        if shown != view {
            self.sync_panes(window, cx);
            cx.notify();
        }
    }
    ```
  - `activate_workspace`: replace its last lines `        self.sync_panes(window, cx);\n        cx.notify();\n    }` (the ones after `self.refresh_git(project_id, window, cx);\n        }` inside `if switched && self.view() == View::Git`) by prefixing `self.sync_watches(cx);` before `self.sync_panes(window, cx);`.
  - `refresh_files` (:1828): first line of the body `self.sync_watches(cx);` (covers `set_files_target`, which ends in `refresh_files`).
  - `refresh_git` (:2612): first line of the body `self.sync_watches(cx);` (covers `retarget_git` callers: `set_git_target`, the repo chooser, `open_worktree` path, `show_git_since`).
  All four are ordinary `&mut self` methods that already take `cx`. A release is sent after the request that re-asks (a later `fs.list`/`git.status`/`doc.list` re-keeps it), so the order against the new view's request does not matter: each keep happens after the view shows.

Run: `cargo test -p mkrew-ui --lib watches -j 4` then `cargo check -p mkrew-ui -j 4`
Expected: PASS and no errors.

- [ ] **Step 6: Existing integration test comment, ADR, SPEC**

`crates/mkrew-daemon/tests/daemon.rs:1482-1483` (`two_sessions_editing_one_file_raise_a_conflict`): replace
```
    // The first spinner lets the project watcher start (it starts after the first spawn); the
    // second keeps the session `working` until the batch landed (≤ `watch::MAX_DELAY` = 2 s).
```
with
```
    // The first spinner lets the project watcher start (it starts after the first spawn); the
    // second keeps the session `working` until the batch landed (300 ms quiet; a continuous
    // stream would wait ≤ `watch::MAX_DELAY_WORKING`).
```

Create `docs/decisions/0042-release-kept-watchers.md` (template `0000-template.md`):

```markdown
# 0042 — Release kept file watchers when their view closes

- Date: 2026-10-10
- Status: accepted
- Milestone: none (perf review, inbox item 12)

## Context

- SPEC §7 says `doc.list`, `doc.read`, and (HUMAN_CHECKS T6.4 / T7) `fs.*` and `git.status` "keep the project watched until the daemon stops". A project the user looked at once keeps a recursive watcher for the whole daemon lifetime, and every batch makes each UI client re-run `git status` (perf review B5).
- There is no request that says a view closed; `ProjectRef` cannot name a task worktree.

## Options

1. Keep watchers until the daemon stops (today). Simple; idle CPU, handles and `git status` runs for projects nobody looks at.
2. Daemon-side timeout after the last request. No protocol change, but a view that stays open and idle loses its refresh; needs a heartbeat.
3. New request `watch.release` sent by the UI when it stops showing what needed the watcher. One small request; the UI already knows its shown workspace, view and worktree target.

## Decision

Option 3: `watch.release { project_id, worktree_task_id? }` (protocol 38). The daemon clears the `kept` flag of the project watcher and stops it unless a session of the project is live (its exit then drops it as before); a worktree watcher is stopped at once. The UI keeps the shown workspace's project watcher (the title bar summary follows `git.changed`) and a worktree watcher only while its Files / Git view is shown. There is no reference counting: the app has one UI window.

## Consequences

- SPEC §7: new `watch.release` row, `doc.*` rows say "until released or the daemon stops", `git.changed` debounce text; §8 note. Protocol 37 → 38.
- A second UI client that still shows a released workspace loses `git.changed` for it until its next `fs.*` / `git.status` / `doc.*` request re-keeps it. Acceptable: only one UI exists; revisit with a ref count if multi-window arrives.
- HUMAN_CHECKS T6.4 note updated.
```

SPEC edits:
- Add after the `session.ports` row (:509):
  `| `watch.release` | `project_id`, `worktree_task_id?` | ok (`null`) — the UI no longer shows that workspace (or task worktree) and gives back the file watcher that `doc.*`, `fs.*` and `git.status` keep running; it stops unless a session of the project is live. A later request of those kinds keeps it again (ADR 0042) |`
- `:535` and `:536`: replace `Keeps the project watched for `doc.changed` until the daemon stops` with `Keeps the project watched for `doc.changed` until released (`watch.release`) or the daemon stops`; replace `Also keeps the project watched` with `Also keeps the project watched (until released)`.
- `:577`: replace `workspace git state changed (debounced, ≤ once per 300 ms)` with `workspace git state changed (debounced: 300 ms quiet, at most 2 s after the first change, 5 s while a session of the project is working; a batch with nothing to report after filtering sends nothing)`.
- `:1005`: replace `Refresh is triggered by the file watcher (debounced 300 ms)` with `Refresh is triggered by the file watcher (debounced 300 ms, §7 `git.changed`)`.

- [ ] **Step 7: HUMAN_CHECKS**

Append to the end of the shared perf section:

```markdown
- [ ] **Watchers stop with their view:** start the daemon with `MKREW_LOG=debug`. Add a second workspace B. In B open Docs, then switch to workspace A with no sessions in B: the log shows `watch released` and `stopped watching project files` for B. Open the Git view of a task worktree, then switch to Terminals: `watch released` with that task id. Come back to Git / Docs / Files: they refresh and follow changes again (edit a file outside the app: the Git list updates).
- [ ] **`git.changed` rate:** with one agent `working` and writing files, the Git view and the title bar git summary refresh about every 5 s (not every 2 s); once the agent is idle, the next change shows within about 0.5 s. A `git fetch` / commit from outside still shows within ~0.5 s with no agent working. Touching only `.git/objects` (a background `git gc`) changes nothing on screen.
- [ ] **Old UI, new daemon:** after the protocol bump (38) an old `mkrew-ui` gets `version_mismatch` and asks to restart the daemon (expected, SPEC §7).
```

- [ ] **Step 8: Verify and tick**

Run: `cargo test -p mkrew-protocol -p mkrew-daemon -p mkrew-ui --lib -j 4` (expect PASS), then `cargo test -p mkrew-daemon --test daemon -j 4 two_sessions_editing file_watcher_records` (expect PASS), then the full verification. Do not tick inbox line 126 yet (Task 9 follows).
Suggested message: `perf(daemon): 5 s watcher cap while working, release kept watchers (protocol 38)`

---

### Task 9: `files_changed` writes only for Markdown, and no `list_sessions` per batch

Inbox: line 126 (bullet B6; this is the last task of item 12: tick `[~]`→`[x]`, and append ` → plan: docs/superpowers/plans/2026-10-10-inbox-perf.md Tasks 7–9` to the item title like item 10 does at line 107). Spec: §12 Docs (SPEC.md:535 `doc.list`), §20C item 4 (SPEC.md:1669-1671), HUMAN_CHECKS T5.6.

**Current state (investigated):**
- `Hub::files_changed` (`crates/mkrew-daemon/src/session.rs:707-746`), called by the project watcher for every non-empty batch (`:663-665`, not for worktree watchers): (1) `docs_changed` (:750) reads and hashes each changed `.md` on the watcher task (only when the batch has a `.md` path, `docs::is_markdown`); (2) collects the live sessions of the project from `Hub::live` (in memory); (3) inserts one `files_changed` session event per live session, **for every batch**, with all paths; (4) calls `store.list_sessions(Some(project_id))` and keeps those `Working` and live; (5) if exactly one is working: `record_edits` (conflict tracker) and `Command::TurnFiles(paths)` to it.
- The only reader of `files_changed` is `Daemon::doc_list` (`server/doc.rs:29`) → `docs::doc_files` (`docs.rs:109`), which keeps only `is_markdown` paths (`:122`) for "last changed by" (`DocFile.last_session_id` / `last_changed_at`; UI: `main.rs:3768` for the comment send target). `session.files` reads `edited` events (hook edits), not `files_changed`. Integration tests read it through `changed_files` (`tests/daemon.rs:1262`), used by `file_watcher_records_changed_files_of_live_sessions_except_ignored_ones` (:1276), by `answer_after_watcher_saw_notes` (:2002, which waits for `notes.txt`) and by `docs_are_listed_read_and_external_edits_reanchor_comments` (:1339, `last_session_id` of `notes/todo.md` at :1420).
- Task 8 added `Hub::working: Mutex<HashMap<session, project>>`, `Hub::set_working`, `Hub::working_sessions(project_id)` (kept by `Actor::set_state`), which has the same content as the old `list_sessions` + `state == Working` filter, minus the SQLite read. Nothing else is needed: `Hub::live` already holds the live session ids.
- `docs_changed` reads the changed `.md` files on the watcher task. The review flags it, but its output (`doc.changed` + comment re-anchoring) is a feature with its own HUMAN_CHECKS (T6.4) and runs only for `.md` paths; moving it off the task is not part of this change (the read is bounded: ≤ 2 MiB, only paths of the batch).
- `Command::TurnFiles` and `record_edits` do not depend on the `files_changed` event; a non-`.md` batch must still reach them (HUMAN_CHECKS T5.6, turn files: `a_turn_that_edits_two_files_reports_both` expects the watcher-seen `notes` file in the turn).
- Item 13 (T10–T12) will also touch this function: see "Anchors shared with item 13" after Task 9.

**Files:**
- Modify: `crates/mkrew-daemon/src/session.rs` (`Hub::files_changed`; tests)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (`file_watcher_records_changed_files_of_live_sessions_except_ignored_ones`, `answer_after_watcher_saw_notes`, `a_turn_that_edits_two_files_reports_both`, the second turn test near :2216)
- Modify: `docs/SPEC.md` (:535, :1669-1671), `docs/HUMAN_CHECKS.md` (T5.6 note + shared section), `docs/INBOX.md` (tick line 126)

**Interfaces:**
- Consumes (Task 8): `Hub::set_working(&self, id: Ulid, project_id: Ulid, working: bool)`, `Hub::working_sessions(&self, project_id: Ulid) -> Vec<Ulid>`.
- Produces: no new items; `fn files_changed(&self, project_id: Ulid, paths: &[String])` keeps its signature.

- [ ] **Step 1: Failing tests (session.rs)**

In `crates/mkrew-daemon/src/session.rs` `mod tests`, below the Task 8 helpers (`hub`, `watched`, `live`), add:

```rust
    /// A hub with a project and one live session per entry of `states`, each registered the way
    /// `spawn` and `set_state` do (in the store, in `live`, and in `working` when working).
    struct Fixture {
        _dir: tempfile::TempDir,
        hub: Arc<Hub>,
        project: Ulid,
        sessions: Vec<(Ulid, mpsc::Receiver<Command>)>,
    }

    fn fixture(states: &[SessionState]) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project = store.add_project(dir.path(), Some("repo")).unwrap().id;
        let hub = Arc::new(Hub::new(store));
        let mut sessions = Vec::new();
        for &state in states {
            let id = Ulid::new();
            hub.store()
                .insert_session(&Session {
                    id,
                    task_id: None,
                    project_id: project,
                    agent_profile: "fake".into(),
                    kind: SessionKind::Agent,
                    title: None,
                    terminal_title: None,
                    color: None,
                    native_session_id: None,
                    state,
                    state_source: StateSource::Daemon,
                    state_changed_at: 0,
                    exit_code: None,
                    cols: 80,
                    rows: 24,
                    started_at: 0,
                    ended_at: None,
                    pinned: false,
                    plan_path: None,
                })
                .unwrap();
            let (commands, rx) = mpsc::channel(8);
            let (input, _input_rx) = mpsc::channel(1);
            lock(&hub.live).insert(
                id,
                Live {
                    project_id: project,
                    commands,
                    input: Arc::new(tokio::sync::Mutex::new(input)),
                },
            );
            hub.set_working(id, project, state == SessionState::Working);
            sessions.push((id, rx));
        }
        Fixture {
            _dir: dir,
            hub,
            project,
            sessions,
        }
    }

    fn recorded(f: &Fixture, session: Ulid) -> Vec<serde_json::Value> {
        let events = f.hub.store().session_events(session, "files_changed");
        events.unwrap().into_iter().map(|e| e.payload).collect()
    }

    /// Only Docs reads `files_changed`: a batch of source files writes no row, however many
    /// sessions are live (perf B6).
    #[tokio::test]
    async fn a_batch_without_markdown_writes_no_event() {
        let f = fixture(&[SessionState::Working, SessionState::Idle]);
        f.hub
            .files_changed(f.project, &["src/lib.rs".to_owned(), "notes.txt".to_owned()]);
        for (id, _) in &f.sessions {
            assert!(recorded(&f, *id).is_empty());
        }
    }

    /// A batch with Markdown is recorded on every live session with just its Markdown paths
    /// (any case of the extension), in batch order.
    #[tokio::test]
    async fn a_markdown_batch_is_recorded_on_every_live_session_with_only_its_md_paths() {
        let f = fixture(&[SessionState::Working, SessionState::Idle]);
        let batch = ["README.MD", "docs/a.md", "src/lib.rs"].map(str::to_owned);
        f.hub.files_changed(f.project, &batch);
        for (id, _) in &f.sessions {
            assert_eq!(
                recorded(&f, *id),
                [serde_json::json!({"paths": ["README.MD", "docs/a.md"]})]
            );
        }
    }

    /// The one working session is the editor of every batch (also one with no Markdown); the
    /// others are not told. Two working: nobody is blamed; when one stops working
    /// (`set_state`), the other is the editor.
    #[tokio::test]
    async fn the_only_working_session_gets_the_batch_for_its_turn() {
        let mut f = fixture(&[SessionState::Idle, SessionState::Working]);
        let paths = vec!["src/lib.rs".to_owned()];
        f.hub.files_changed(f.project, &paths);
        assert!(f.sessions[0].1.try_recv().is_err(), "idle session blamed");
        assert!(matches!(
            f.sessions[1].1.try_recv(),
            Ok(Command::TurnFiles(p)) if p == paths
        ));

        let mut f = fixture(&[SessionState::Working, SessionState::Working]);
        f.hub.files_changed(f.project, &paths);
        for (_, rx) in &mut f.sessions {
            assert!(rx.try_recv().is_err(), "two working: the editor is unknown");
        }
        let first = f.sessions[0].0;
        f.hub.set_working(first, f.project, false);
        f.hub.files_changed(f.project, &paths);
        assert!(f.sessions[0].1.try_recv().is_err());
        assert!(matches!(f.sessions[1].1.try_recv(), Ok(Command::TurnFiles(_))));
    }
```

(`Session`, `SessionKind`, `StateSource` come from `use mkrew_protocol::{…}` at the top of `session.rs`; `SessionState` too. If `Session` is not imported in the parent, add it to the test module's `use`.)

`crates/mkrew-daemon/tests/daemon.rs`, `file_watcher_records_changed_files_of_live_sessions_except_ignored_ones` (:1276-1311): rename it `file_watcher_records_only_markdown_files_of_live_sessions_except_ignored_ones`; replace

```rust
    for dir in ["src", "target", "node_modules"] {
```
with
```rust
    for dir in ["src", "target", "node_modules", "notes"] {
```
replace
```rust
    // Ignored writes first: once `src/lib.rs` is recorded, their batch has been handled too.
```
with
```rust
    // Ignored writes first: once `notes/todo.md` is recorded, their batch has been handled too.
    // `src/lib.rs` is watched but not Markdown: Docs is the only reader, so it is not recorded.
```
replace `["target/a.o", "node_modules/b.js", "debug.log", "src/lib.rs"]` with `["target/a.o", "node_modules/b.js", "debug.log", "src/lib.rs", "notes/todo.md"]`, and in the wait and assertion replace the three occurrences of `src/lib.rs` (`.contains("src/lib.rs")`, `"src/lib.rs never recorded"`, `BTreeSet::from(["src/lib.rs".to_owned()])`) with `notes/todo.md`.

Turn tests that waited for the watcher through `files_changed` must now wait on a Markdown file: in `answer_after_watcher_saw_notes` (:2002-2009) and its doc comment replace `notes.txt` with `notes.md` (three places: the comment, `.contains("notes.txt")`, `"notes.txt never recorded"`); in `a_turn_that_edits_two_files_reports_both` replace `path = \"notes.txt\"` with `path = \"notes.md\"` in the scenario string (:2020), and in its assertions replace `// `notes.txt` was written without a hook: only the file watcher saw it.` / `["a.rs", "notes.txt", "src/b.rs"]` with `notes.md` (`["a.rs", "notes.md", "src/b.rs"]`); in the second turn test (:2230, :2265) replace `path = \"notes.txt\"` with `path = \"notes.md\"` and `("notes.txt", Some(1), Some(0))` with `("notes.md", Some(1), Some(0))`. Do not touch the other `notes.txt` uses (:2333-2378 write the file directly; :6286-6293 plan linking).

Run: `cargo test -p mkrew-daemon --lib session::tests::a_batch -j 4 -- --nocapture` and `cargo test -p mkrew-daemon --lib the_only_working -j 4`
Expected: `a_batch_without_markdown_writes_no_event` and `a_markdown_batch_is_recorded…` FAIL (the first finds a row per session; the second finds all three paths), `the_only_working_session_gets_the_batch_for_its_turn` PASSES against today's code only if `list_sessions` reports the same states: the fixture stores `Working` in the row and in the map, so it passes before and after (it pins that the in-memory list replaces the store read; after Step 2 `list_sessions` is gone from this path).

- [ ] **Step 2: `files_changed`: Markdown only, working list from memory**

`crates/mkrew-daemon/src/session.rs`: replace the whole of `Hub::files_changed`, from its doc comment

```rust
    /// Records a debounced batch of changed files on every live session of the project
    /// (session event `files_changed`, payload `{"paths": [...]}`), and attributes the changes
    /// for conflict detection to the project's session in `working`. With several sessions
    /// working the editor is unknown, so the batch is not attributed (HUMAN_CHECKS T5.6).
    fn files_changed(&self, project_id: Ulid, paths: &[String]) {
```

through its closing `}` (the line before `/// [`Hub::doc_seen`] for each changed Markdown file.`), with:

```rust
    /// Handles a debounced batch of changed files of the project. The Markdown ones are
    /// recorded on every live session (session event `files_changed`, payload
    /// `{"paths": [...]}`): Docs "changed by" is the only reader, so a batch without Markdown
    /// writes nothing. The batch is attributed for conflict detection and to the turn of the
    /// project's session in `working` (from memory: [`Hub::working_sessions`]). With several
    /// sessions working the editor is unknown, so the batch is not attributed (HUMAN_CHECKS T5.6).
    fn files_changed(&self, project_id: Ulid, paths: &[String]) {
        self.docs_changed(project_id, paths);
        let sessions: Vec<Ulid> = lock(&self.live)
            .iter()
            .filter(|(_, live)| live.project_id == project_id)
            .map(|(id, _)| *id)
            .collect();
        // A batch flushed after the last session exited.
        if sessions.is_empty() {
            return;
        }
        let at = crate::server::now_ms();
        let docs: Vec<&str> = paths
            .iter()
            .map(String::as_str)
            .filter(|path| docs::is_markdown(path))
            .collect();
        if !docs.is_empty() {
            let payload = serde_json::json!({ "paths": docs });
            let store = self.store();
            for &id in &sessions {
                if let Err(e) = store.insert_session_event(id, "files_changed", &payload, at) {
                    tracing::warn!(%id, "files_changed not recorded: {e}");
                }
            }
        }
        let working: Vec<Ulid> = self
            .working_sessions(project_id)
            .into_iter()
            .filter(|id| sessions.contains(id))
            .collect();
        if let [editor] = working[..] {
            self.record_edits(project_id, paths, editor, at);
            // A batch flushed after the turn ended is dropped by the tracker (HUMAN_CHECKS T9.5).
            if !self.command(editor, Command::TurnFiles(paths.to_vec())) {
                tracing::debug!(%editor, "changed files not added to the turn");
            }
        }
    }
```

`SessionState` stays imported (used elsewhere in the file). `Store::list_sessions` is no longer called here.

Run: `cargo test -p mkrew-daemon --lib -j 4`
Expected: PASS (the three new tests included).

- [ ] **Step 3: Integration tests**

Run: `cargo test -p mkrew-daemon --test daemon -j 4 file_watcher_records_only docs_are_listed a_turn_that_edits two_sessions_editing`
Expected: PASS. `docs_are_listed_read_and_external_edits_reanchor_comments` is the Docs "changed by" proof: the agent writes `notes/todo.md` and `doc.list` still reports `last_session_id == the working session`. `two_sessions_editing_one_file_raise_a_conflict` proves `record_edits` still runs for non-Markdown `shared.txt` without a `files_changed` row.

- [ ] **Step 4: SPEC and HUMAN_CHECKS**

`docs/SPEC.md`:
- `:535` (`doc.list`): replace `md files in `files_changed` session events of the last 24 h` with `md files in `files_changed` session events of the last 24 h (the daemon records a watcher batch there only when it has a Markdown path, on every live session of the project)`.
- `:1669-1671` (§20C item 4): replace `sessions that have `files_changed`/hook edits in this project in the last 24 h` with `sessions that have hook edits (`edited` events) or turn files in this project in the last 24 h`. Check first with `read docs/SPEC.md:1669-1671` that the text still matches; the data source is `session.files` (`edited` events and turn files), not `files_changed`, so this corrects the sentence.

`docs/HUMAN_CHECKS.md` (T5.6 area, the line `/`-separated, relative) on EVERY live session of the project (attribution is T5.6).` at ~:1780-1781): replace `Each batch becomes a `files_changed` session event (`{"paths": [...]}`,` … `on EVERY live session of the project` with `Each batch that has a Markdown file becomes a `files_changed` session event (`{"paths": [...]}` with only its Markdown paths,` … `on EVERY live session of the project`; other batches write no row (Docs is the only reader). Read the three lines (`read docs/HUMAN_CHECKS.md:1778-1782`) and keep the rest of the sentence.

Append to the end of the shared perf section:

```markdown
- [ ] **Docs "changed by" still works:** with one agent working, have it write a new `notes/todo.md` (not matching the review globs). Docs lists it, and its "changed by" / the comment "send to agent" default points at that agent's session. Have the agent edit only `.rs` files: Docs shows nothing new, Git "Changed by" (hook edits) and the turn's file list still show them, and with two agents editing the same file a conflict warning still appears. In `sqlite3 mkrew.db`: `select count(*) from session_event where kind='files_changed' and payload not like '%.md%';` does not grow while the agent edits source files (rows written before this change excepted).
```

- [ ] **Step 5: Verify and tick**

Run: `cargo test -p mkrew-daemon -p mkrew-store -j 4` (expect PASS), then the full verification. Tick inbox line 126 `[~]`→`[x]` (all of A4, B5, B6, B7 done; item 12 title gets ` → plan: docs/superpowers/plans/2026-10-10-inbox-perf.md Tasks 7–9`).
Suggested message: `perf(daemon): record files_changed only for Markdown batches, in-memory working sessions`

---

### Task 10: `[features]` settings and opt-in turn snapshots (daemon, ADR 0041)

Inbox: line 132 `13. Perf: turn snapshots and conflict detection become opt-in` (bullets "New settings", "Off = zero cost" for A2, "Changes default spec behavior → ADR + SPEC"; do not tick yet). Spec: §20.0 (SPEC.md:1501-1536), §20A.3 (:1563-1568), §20C (:1645-1686), M11 checklist (:1442-1443), §10 pane header (:885).

**Current state (investigated; line numbers are from 2026-10-10, re-find every symbol by name; item 12 (T7–T9) already changed `session.rs`, nothing below depends on those edits):**
- `mkrew_core::settings::Settings` (`crates/mkrew-core/src/settings.rs:16`) has `notify, awake, composer, zed, ui, terminal`. Every section is a struct with `#[derive(Debug, Clone, PartialEq, Eq, Deserialize)] #[serde(default, deny_unknown_fields)]` and a hand-written `impl Default`. `Settings` itself derives `Default`. `template()` (:269) is the commented `settings.toml`; the test `the_template_parses_to_the_defaults_commented_or_not` (:505) uncomments every `# key = value` line and expects `Settings::default()`. `full_file_parses` (:403) builds a full `Settings` literal, so a new field must be added there. `missing_file_is_defaults` (:359) asserts defaults per section.
- Daemon: `SettingsHolder` (`crates/mkrew-daemon/src/settings.rs`) holds the last good `Settings`; `reload()` runs from the config-dir watcher (debounce `QUIET` 300 ms). `Daemon::new` (`server.rs:86`) builds `Hub::new(store)` and then `SettingsHolder::load(paths.settings_file())`. `Hub` (`session.rs`, `impl Hub { pub fn new(store: Store) -> Self`) has no access to settings.
- Turn start path: `Shell`-independent. In `session.rs` the actor's `fn track_turn(&mut self, state: SessionState)` turns a state change into `turns::Job::Start { n, at, reply: self.hook_reply.take() }` (the hook's reply sender moves into the job) or `turns::Job::End { turn, snapshot: state != SessionState::NeedsInput }`, and sends it to the session's `turns::record` task. `turns.rs` `Recorder::start` calls `self.snapshot(turn_ref(..))` (copy index, `git add -A -- :/`, `write-tree`, `update-ref`, per-project lock in `snapshot.rs`) and then `drop(reply)` in `record`; so the hook waits for the git work and for every earlier queued job. `Recorder::end` takes the end snapshot only when the turn has a start tree (`(Some(_), true)`) and counts lines with `git diff --numstat` in `counted` (no trees → `added`/`removed` = `None`, which is already the non-git behaviour).
- Requests that need snapshots: `turn.restore_preview` and `turn.restore` (`server/turn.rs` `restore_target`, which already answers `invalid_state` "turn N of session S has no snapshot…" when `start_tree` is missing); `git.status` / `git.diff` with a `since` that is a tree (`server/git.rs` passes `&|| hub.snapshots.tree(..)` to `git_view::status` / `git_view::diff`, called only for `Target::Tree`). `task.undo*` uses `task.checkpoint` (HEAD), not snapshots, and stays as it is. `delete_expired_refs` (startup) only deletes old refs: it stays, so refs left from an earlier "on" period still expire.
- Existing tests that need snapshots on: `turns.rs` tests (5, helper `hub(base, in_git)` and `Running::start`), `tests/daemon.rs` `a_hook_turn_start_is_snapshotted_before_the_agent_edits`, `turns_are_listed_previewed_and_restored`, `hook_edits_are_announced_attributed_and_diffed_since_a_turn`, `worktree_tasks_run_and_resume_in_their_own_worktree` (its `TurnRestorePreview` → `InvalidState` check is about the worktree; keep its meaning by turning snapshots on). `tests/daemon.rs` has `Env` (config dir `<dir>/config`, `MKREW_CONFIG_DIR`), `Env::write_profile`, `client.hook(..)`, `client.wait_event_where(..)`, `error_code(..)`, and a `git(&repo, &[..])` helper.

**Decision (what still happens with snapshots off):**

| Part | Off |
| --- | --- |
| Turn counting (`mkrew_core::turn::TurnTracker`), `turn` rows (`start_turn` / `end_turn`), `session.turn` events with file paths, the pane-header summary `✓ Done · 3 files · 4m` | **kept** (no git needed); `added` / `removed` are `None` |
| Git snapshot at start/end, `refs/mkrew/turns/*`, `git diff --numstat`, per-project snapshot lock | **gone** |
| Hook reply of a turn-start hook | answered at once: the reply is not moved into the recorder job, so the actor drops it when `on_hook` returns |
| `turn.list` | answers (rows without trees) |
| `turn.restore_preview`, `turn.restore`, `git.status` / `git.diff` with a tree `since` | `invalid_state`: `turn snapshots are off: set [features] turn_snapshots = true in settings.toml` |
| `git.status` / `git.diff` with a commit `since`, `task.undo*` | unchanged |
| Toggle | read at each turn start (`Hub::features()`); a turn keeps the decision it started with (a turn that started with a snapshot still takes its end snapshot after a switch to off; one that started without never does); a switch applies from the next turn |
| Known corner | a turn that ended at `needs_input` with a start tree, followed by a start while off, keeps no end tree or line counts |

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`FeatureSettings`, `Settings.features`, `template()`, tests)
- Modify: `crates/mkrew-daemon/src/settings.rs` (`SettingsHolder::features`, test)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Hub.settings`, `Hub::use_settings`, `Hub::features`, `track_turn`)
- Modify: `crates/mkrew-daemon/src/turns.rs` (`Job::Start.snapshot`, `start_job`, `Recorder::start`, tests)
- Modify: `crates/mkrew-daemon/src/server.rs` (`Daemon::new`)
- Modify: `crates/mkrew-daemon/src/server/turn.rs` (`restore_target`), `crates/mkrew-daemon/src/server/git.rs` (`git_status`, `git_diff`, `view_outcome`)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (`Env::write_settings`, four existing tests, one new test)
- Create: `docs/decisions/0041-optin-turn-snapshots-and-conflicts.md`
- Modify: `docs/SPEC.md` (§20.0, §20A.3, §20C, M11 checklist, §10), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces: `mkrew_core::settings::FeatureSettings { pub turn_snapshots: bool, pub conflict_detection: bool }` (`Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize`), `Settings.features: FeatureSettings`.
- Produces: `SettingsHolder::features(&self) -> FeatureSettings`.
- Produces: `Hub::use_settings(&self, settings: Arc<SettingsHolder>)` (set once) and `Hub::features(&self) -> FeatureSettings` (defaults until `use_settings`).
- Produces: `turns::start_job(n: u32, at: i64, snapshot: bool, hook_reply: &mut Option<oneshot::Sender<()>>) -> Job`; `Job::Start { n, at, snapshot, reply }`.
- Protocol: no wire change (`PROTOCOL_VERSION` is whatever T8 left, 38; this task does not bump it). No migration.
- T11 and T12 consume `Settings.features`.

- [ ] **Step 1: Failing tests (core + daemon settings holder)**

`crates/mkrew-core/src/settings.rs`, `mod tests`, after `full_file_parses`:

```rust
    #[test]
    fn features_are_off_until_switched_on() {
        let features = |src: &str| parse(Path::new("s.toml"), src).unwrap().features;
        assert_eq!(
            features(""),
            FeatureSettings {
                turn_snapshots: false,
                conflict_detection: false
            }
        );
        let snapshots = features("[features]\nturn_snapshots = true\n");
        // One switch does not move the other.
        assert!(snapshots.turn_snapshots && !snapshots.conflict_detection);
        let conflicts = features("[features]\nconflict_detection = true");
        assert!(conflicts.conflict_detection && !conflicts.turn_snapshots);
        assert!(
            parse_err("[features]\nturn_snapshot = true\n").contains("unknown field `turn_snapshot`")
        );
        assert!(parse_err("[features]\nturn_snapshots = 3\n").contains("invalid type"));
    }
```

In `missing_file_is_defaults`, after the `assert!(settings.awake.keep_awake);` line add:

```rust
        assert_eq!(settings.features, FeatureSettings::default());
        assert!(!settings.features.turn_snapshots && !settings.features.conflict_detection);
```

In `full_file_parses`, in the TOML text after the `[terminal]` section (before the closing `"#;`) add:

```toml

[features]
turn_snapshots = true
conflict_detection = true
```

and in the expected `Settings { .. }` literal, after the `terminal: FontSettings { .. },` field add:

```rust
                features: FeatureSettings {
                    turn_snapshots: true,
                    conflict_detection: true,
                },
```

`crates/mkrew-daemon/src/settings.rs`, `mod tests`, after `bad_file_at_start_means_defaults_and_an_error`:

```rust
    /// Hot reload (SPEC §20.0): the switches follow the file, a broken file keeps the last good ones.
    #[test]
    fn feature_switches_follow_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let holder = SettingsHolder::load(path.clone());
        assert_eq!(holder.features(), FeatureSettings::default());

        std::fs::write(&path, "[features]\nturn_snapshots = true\n").unwrap();
        holder.reload();
        assert!(holder.features().turn_snapshots);
        assert!(!holder.features().conflict_detection);

        std::fs::write(&path, "[features]\nturn_snapshots = 3\n").unwrap();
        holder.reload();
        assert!(holder.features().turn_snapshots, "last good settings kept");

        std::fs::write(&path, "").unwrap();
        holder.reload();
        assert_eq!(holder.features(), FeatureSettings::default());
    }
```

Run: `cargo test -p mkrew-core --lib settings -j 4` and `cargo test -p mkrew-daemon --lib -j 4 -- settings::`
Expected: FAIL to compile (`FeatureSettings`, `features` do not exist).

- [ ] **Step 2: `FeatureSettings` in core**

`crates/mkrew-core/src/settings.rs`, in `pub struct Settings`, after the field `pub terminal: FontSettings,` add:

```rust
    /// `[features]`: costly extras that are off until asked for (ADR 0041).
    pub features: FeatureSettings,
```

After `impl Default for ZedSettings { … }` (before `/// `[ui]` / `[terminal]` font.`) add:

```rust
/// `[features]` (SPEC §20.0, ADR 0041): parts that cost git or CPU on every turn / file change.
/// Both are off by default; the daemon and the UI read them like the other keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FeatureSettings {
    /// §20C: a git snapshot at every agent turn start and end. Needed by "Turns…", "Restore
    /// before turn n", "Since turn…" and "Changed by".
    pub turn_snapshots: bool,
    /// §7 `conflict.detected`: warn when two sessions edit the same file within 60 s.
    pub conflict_detection: bool,
}
```

`template()`: replace the last two lines

```
     [terminal]\n\
     # font_family = \"\"\n\
     # font_size = 13\n"
```

with

```
     [terminal]\n\
     # font_family = \"\"\n\
     # font_size = 13\n\
     \n\
     # Costly extras, off by default (ADR 0041): git snapshots at every agent turn (Turns,\n\
     # Restore before a turn, Since turn, Changed by) and two-session file conflict warnings.\n\
     [features]\n\
     # turn_snapshots = false\n\
     # conflict_detection = false\n"
```

`crates/mkrew-daemon/src/settings.rs`: change the import to `use mkrew_core::settings::{self, FeatureSettings, Settings, SettingsError};` and in `impl SettingsHolder`, after `pub fn current(&self) -> Settings { … }`:

```rust
    /// The `[features]` switches of the last good settings (`Copy`: no clone of the strings).
    pub fn features(&self) -> FeatureSettings {
        self.lock().settings.features
    }
```

Run: `cargo test -p mkrew-core --lib settings -j 4` and `cargo test -p mkrew-daemon --lib -j 4 -- settings::`
Expected: PASS (the template test still passes: both lines default to `false`).

- [ ] **Step 3: Failing tests (turn recorder)**

`crates/mkrew-daemon/src/turns.rs`, `mod tests`:

(a) imports: add `use crate::settings::SettingsHolder;`.

(b) replace the helper `fn hub(base: &tempfile::TempDir, in_git: bool) -> (PathBuf, Arc<Hub>, Ulid, Ulid) {` with a thin wrapper plus the old body renamed, so the five existing tests keep snapshots on:

```rust
    /// Snapshots on, like the tests below were written for.
    fn hub(base: &tempfile::TempDir, in_git: bool) -> (PathBuf, Arc<Hub>, Ulid, Ulid) {
        let (dir, hub, project, session, _) =
            hub_with(base, in_git, "[features]\nturn_snapshots = true\n");
        (dir, hub, project, session)
    }
```

and rename the old function to `fn hub_with(base: &tempfile::TempDir, in_git: bool, settings: &str) -> (PathBuf, Arc<Hub>, Ulid, Ulid, Arc<SettingsHolder>) {`. Its doc comment stays; its last line `(dir.clone(), Arc::new(Hub::new(store)), project, session)` becomes:

```rust
        let hub = Arc::new(Hub::new(store));
        let file = base.join("settings.toml");
        std::fs::write(&file, settings).unwrap();
        let holder = Arc::new(SettingsHolder::load(file));
        hub.use_settings(Arc::clone(&holder));
        (dir.clone(), hub, project, session, holder)
```

(c) in `impl Running`, replace `async fn start(&self, n: u32, at: i64)` with:

```rust
        /// Starts turn `n` with a start snapshot and waits for it.
        async fn start(&self, n: u32, at: i64) {
            self.start_with(n, at, true).await;
        }

        /// Starts turn `n` and waits until the recorder dropped the reply.
        async fn start_with(&self, n: u32, at: i64, snapshot: bool) {
            let (reply, done) = oneshot::channel();
            let reply = Some(reply);
            let job = Job::Start { n, at, snapshot, reply };
            self.jobs.send(job).unwrap();
            assert!(done.await.is_err(), "the reply is dropped, never sent");
        }
```

(delete the old body of `start`).

(d) new tests at the end of `mod tests`:

```rust
    #[tokio::test]
    async fn with_snapshots_off_a_turn_has_no_trees_refs_or_counts() {
        let dir = tempfile::tempdir().unwrap();
        let (root, hub, project, session, _) = hub_with(&dir, true, "");
        let mut events = hub.events.subscribe();
        let recorder = run(&hub, project, session, &root);

        recorder.start_with(1, 10, false).await;
        std::fs::write(root.join("a.txt"), "1\nx\n").unwrap();
        recorder.end(ended(1, 10, 20, &["a.txt"]), true);
        recorder.finish().await;

        // The turn is still counted, stored and announced; only git is skipped.
        let turn = hub.store().get_turn(session, 1).unwrap();
        assert_eq!((turn.start_tree, turn.end_tree), (None, None));
        assert_eq!((turn.started_at, turn.ended_at), (10, Some(20)));
        assert_eq!(turn.files, [file("a.txt", None, None)]);
        let Ok(Event::SessionTurn(event)) = events.recv().await else {
            panic!("no session.turn");
        };
        assert_eq!((event.n, event.ended_at), (1, 20));
        assert_eq!(event.files, [file("a.txt", None, None)]);
        assert_eq!(git(&root, &["for-each-ref", "refs/mkrew"]), "");
    }

    /// The hook of a turn start is held for the snapshot only when snapshots are on.
    #[test]
    fn the_hook_reply_is_held_for_the_snapshot_only_when_on() {
        let (reply, mut waiting) = oneshot::channel::<()>();
        let mut hook = Some(reply);
        let job = start_job(1, 10, false, &mut hook);
        assert!(matches!(job, Job::Start { snapshot: false, reply: None, .. }));
        // Still with the caller, which drops it when the hook is handled: the hook is answered.
        assert!(hook.is_some());
        drop(hook);
        assert!(waiting.try_recv().is_err());

        let (reply, _waiting) = oneshot::channel::<()>();
        let mut hook = Some(reply);
        let job = start_job(2, 20, true, &mut hook);
        assert!(matches!(job, Job::Start { snapshot: true, reply: Some(_), .. }));
        assert!(hook.is_none(), "the recorder owns the reply until the snapshot exists");
    }

    /// A switch in settings.toml applies from the next turn start, not retroactively.
    #[tokio::test]
    async fn a_toggle_applies_from_the_next_turn() {
        let dir = tempfile::tempdir().unwrap();
        let (root, hub, project, session, holder) = hub_with(&dir, true, "");
        let recorder = run(&hub, project, session, &root);
        let start = |n: u32| {
            let mut none = None;
            start_job(n, i64::from(n) * 10, hub.features().turn_snapshots, &mut none)
        };

        recorder.jobs.send(start(1)).unwrap();
        std::fs::write(dir.path().join("settings.toml"), "[features]\nturn_snapshots = true\n")
            .unwrap();
        holder.reload();
        // Turn 1 started while off; its end never snapshots, even when on by now.
        recorder.end(ended(1, 10, 15, &["a.txt"]), true);
        recorder.jobs.send(start(2)).unwrap();
        recorder.end(ended(2, 20, 25, &[]), true);
        recorder.finish().await;

        let first = hub.store().get_turn(session, 1).unwrap();
        let second = hub.store().get_turn(session, 2).unwrap();
        assert_eq!((first.start_tree, first.end_tree), (None, None));
        assert!(second.start_tree.is_some() && second.end_tree.is_some());
        assert_eq!(
            git(&root, &["for-each-ref", "--format=%(refname)", "refs/mkrew"]),
            format!("{}\n{}", turn_ref(session, 2), turn_end_ref(session, 2))
        );
    }
```

Run: `cargo test -p mkrew-daemon --lib -j 4 -- turns::`
Expected: FAIL to compile (`start_job`, `Job::Start.snapshot`, `use_settings`, `features` do not exist).

- [ ] **Step 4: Gate the recorder and the hook wait**

`crates/mkrew-daemon/src/turns.rs`: replace the `Job::Start` variant

```rust
    /// Turn `n` started at `at`; `reply` is dropped once the start snapshot is taken (a
    /// `hook.report` that started the turn is answered then).
    Start {
        n: u32,
        at: i64,
        reply: Option<oneshot::Sender<()>>,
    },
```

with

```rust
    /// Turn `n` started at `at`; `reply` is dropped once the start snapshot is taken (a
    /// `hook.report` that started the turn is answered then). `snapshot` = `[features]
    /// turn_snapshots` was on when the turn started (ADR 0041): off, nothing touches git and
    /// there is no reply to wait for.
    Start {
        n: u32,
        at: i64,
        snapshot: bool,
        reply: Option<oneshot::Sender<()>>,
    },
```

After `enum Job` add:

```rust
/// The job for turn `n` starting at `at`. With `snapshot` a hook reply in `hook_reply` moves
/// into it (answered once the start snapshot exists); without, it stays with the caller, who
/// drops it when the hook is handled, so the hook never waits for git.
pub fn start_job(
    n: u32,
    at: i64,
    snapshot: bool,
    hook_reply: &mut Option<oneshot::Sender<()>>,
) -> Job {
    Job::Start {
        n,
        at,
        snapshot,
        reply: if snapshot { hook_reply.take() } else { None },
    }
}
```

In `record`, replace

```rust
            Job::Start { n, at, reply } => {
                recorder.start(n, at).await;
                drop(reply);
            }
```

with

```rust
            Job::Start {
                n,
                at,
                snapshot,
                reply,
            } => {
                recorder.start(n, at, snapshot).await;
                drop(reply);
            }
```

In `impl Recorder`, replace

```rust
    async fn start(&mut self, n: u32, at: i64) {
        let tree = self.snapshot(turn_ref(self.session, n)).await;
```

with

```rust
    async fn start(&mut self, n: u32, at: i64, snapshot: bool) {
        let tree = if snapshot {
            self.snapshot(turn_ref(self.session, n)).await
        } else {
            None
        };
```

(`end` needs no change: it snapshots only when the turn has a start tree.)

`crates/mkrew-daemon/src/session.rs`:
- Imports (next to the other `use crate::…` lines, ~34): `use crate::settings::SettingsHolder;` and add `FeatureSettings` to the core imports: `use mkrew_core::settings::FeatureSettings;`.
- In `pub struct Hub`, right after `pub snapshots: Snapshots,` (doc: `/// Turn snapshots (SPEC §20C.1), one at a time per project.`):

```rust
    /// The daemon's settings, for `[features]` (ADR 0041); set once by [`Hub::use_settings`].
    settings: std::sync::OnceLock<Arc<SettingsHolder>>,
```

- In `Hub::new`, after `snapshots: Snapshots::default(),`: `settings: std::sync::OnceLock::new(),`
- In `impl Hub`, after `pub fn store(&self) -> MutexGuard<'_, Store> { … }`:

```rust
    /// Where `[features]` come from; the daemon calls it once. Without it (unit tests of other
    /// parts) every feature is off, the default.
    pub fn use_settings(&self, settings: Arc<SettingsHolder>) {
        let _ = self.settings.set(settings);
    }

    /// The current `[features]`; the settings holder hot-reloads, so each call sees a toggle.
    pub fn features(&self) -> FeatureSettings {
        self.settings
            .get()
            .map_or_else(FeatureSettings::default, |settings| settings.features())
    }
```

- In `fn track_turn`, replace

```rust
            Some(TurnEvent::Started(n)) => turns::Job::Start {
                n,
                at,
                reply: self.hook_reply.take(),
            },
```

with

```rust
            // Read at every start: a toggle in settings.toml applies from the next turn
            // (ADR 0041). Off, the hook reply stays in `hook_reply` and is dropped when the hook
            // is handled, so the agent is not held up.
            Some(TurnEvent::Started(n)) => turns::start_job(
                n,
                at,
                self.hub.features().turn_snapshots,
                &mut self.hook_reply,
            ),
```

`crates/mkrew-daemon/src/server.rs`, `Daemon::new`: replace

```rust
        let hub = Arc::new(Hub::new(store));
        Arc::new(Self {
            git: Arc::clone(&hub.git),
            hub,
            settings: Arc::new(SettingsHolder::load(paths.settings_file())),
```

with

```rust
        let hub = Arc::new(Hub::new(store));
        let settings = Arc::new(SettingsHolder::load(paths.settings_file()));
        hub.use_settings(Arc::clone(&settings));
        Arc::new(Self {
            git: Arc::clone(&hub.git),
            hub,
            settings,
```

Run: `cargo test -p mkrew-daemon --lib -j 4 -- turns:: settings::`
Expected: PASS (the five old recorder tests run with snapshots on; three new ones pass).

- [ ] **Step 5: Requests that need a snapshot answer a clear error**

`crates/mkrew-daemon/src/server/turn.rs`: after `use crate::undo;` add

```rust
/// What the requests that need turn snapshots answer while `[features] turn_snapshots` is off.
pub(super) const SNAPSHOTS_OFF: &str =
    "turn snapshots are off: set [features] turn_snapshots = true in settings.toml";
```

and as the first statement of `fn restore_target(&self, turn: &TurnRef) -> Result<RestoreTarget, Outcome> {` add:

```rust
        if !self.hub.features().turn_snapshots {
            return Err(Outcome::error(ErrorCode::InvalidState, SNAPSHOTS_OFF));
        }
```

(`turn_restore_preview` and `turn_restore` both go through `restore_target`; the `confirm` check of `turn_restore` still answers first.)

`crates/mkrew-daemon/src/server/git.rs`:
- imports: `use super::turn::SNAPSHOTS_OFF;` next to `use super::{Daemon, ok, store_outcome};` (declare `mod turn;` is already in `server.rs`; if it is private to `server`, `pub(super) const` is reachable from the sibling module).
- `git_status`: replace `&|| hub.snapshots.tree(&*hub.git, id, &dir),` with

```rust
                &|| tree_snapshot(&hub, id, &dir),
```

- `git_diff`: replace `let snapshot = || hub.snapshots.tree(&*hub.git, id, &dir);` with `let snapshot = || tree_snapshot(&hub, id, &dir);`
- add the helper next to `view_outcome`:

```rust
/// The snapshot a "since turn" tree is compared with. Off (ADR 0041) it fails with
/// `Unsupported`, which `view_outcome` answers as `invalid_state`; a commit `since` never calls it.
fn tree_snapshot(hub: &crate::session::Hub, project: Ulid, dir: &Path) -> std::io::Result<String> {
    if !hub.features().turn_snapshots {
        return Err(std::io::Error::new(std::io::ErrorKind::Unsupported, SNAPSHOTS_OFF));
    }
    hub.snapshots.tree(&*hub.git, project, dir)
}
```

- `view_outcome`: replace the arm

```rust
        Ok(Err(ViewError::Git(e))) => {
            Outcome::error(ErrorCode::Internal, format!("{method} failed: {e}"))
        }
```

with

```rust
        Ok(Err(ViewError::Git(e))) if e.kind() == std::io::ErrorKind::Unsupported => {
            Outcome::error(ErrorCode::InvalidState, e.to_string())
        }
        Ok(Err(ViewError::Git(e))) => {
            Outcome::error(ErrorCode::Internal, format!("{method} failed: {e}"))
        }
```

Check the actual `view_outcome` match at `server/git.rs` (~:358-376): place the new arm before the existing `Ok(Err(ViewError::Git(e)))` arm. Note: a `since` tree whose diff is already cached (`GitCache.diffs`, keyed by target + stamp) can still answer from the cache after a switch to off; it expires with the next change of the file.

- [ ] **Step 6: Integration test (real git, fake agent) + keep the old snapshot tests on**

`crates/mkrew-daemon/tests/daemon.rs`:
- In `impl Env`, after `fn write_profile(&self, id: &str, toml: &str) { … }` add:

```rust
    /// `settings.toml` of the daemon's config dir. Before `start`, or later for a hot reload.
    fn write_settings(&self, toml: &str) {
        let dir = self.dir.path().join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), toml).unwrap();
    }
```

- Existing tests that need snapshots: directly after `let env = Env::new();` in `a_hook_turn_start_is_snapshotted_before_the_agent_edits`, `turns_are_listed_previewed_and_restored`, `hook_edits_are_announced_attributed_and_diffed_since_a_turn` and `worktree_tasks_run_and_resume_in_their_own_worktree` add:

```rust
    env.write_settings("[features]\nturn_snapshots = true\n");
```

- New test after `turns_are_listed_previewed_and_restored`:

```rust
/// ADR 0041: by default no snapshot, no ref and no wait; turns still exist; what needs a tree
/// answers `invalid_state`; switching the setting on applies from a later turn.
#[test]
fn turn_snapshots_off_keeps_turns_but_takes_no_snapshot() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let scenario = env.dir.path().join("idle.toml");
    std::fs::write(&scenario, "[[step]]\naction = \"sleep\"\nms = 60000\n").unwrap();
    env.write_fake_profile_as(
        "editor",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let path = env.dir.path().join("config/agents/editor.toml");
    let profile = std::fs::read_to_string(&path).unwrap();
    env.write_profile(
        "editor",
        &profile.replace("mode = \"screen\"", "mode = \"hook\""),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "editor").into_result().unwrap();
    let id = spawned.session_id;
    let hook = |client: &mut Client, event: &str| {
        client
            .hook(id, "fake", event, Value::Null)
            .into_result::<()>()
            .unwrap();
    };
    let refs = || git(&repo, &["for-each-ref", "refs/mkrew"]);
    let turns = |client: &mut Client| -> Vec<Turn> {
        client
            .call(Request::TurnList(SessionRef { session_id: id }))
            .into_result()
            .unwrap()
    };

    hook(&mut client, "busy");
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    hook(&mut client, "done");
    client
        .wait_event_where(|e| matches!(e, Event::SessionTurn(t) if t.session_id == id && t.n == 1));
    assert_eq!(refs(), "", "no snapshot ref while off");
    let listed = turns(&mut client);
    assert_eq!(listed.len(), 1);
    assert_eq!((listed[0].start_tree.as_deref(), listed[0].end_tree.as_deref()), (None, None));

    // Restore and "since turn" (a tree) are refused with the reason; a commit `since` works.
    let refused = |client: &mut Client, request: Request| match client
        .call(request)
        .into_result::<serde_json::Value>()
    {
        Err(ResponseError::Daemon(body)) => {
            assert_eq!(body.code, ErrorCode::InvalidState);
            assert!(body.message.contains("turn_snapshots"), "{}", body.message);
        }
        other => panic!("expected a refusal, got {other:?}"),
    };
    refused(&mut client, Request::TurnRestorePreview(TurnRef { session_id: id, n: 1 }));
    let tree = git(&repo, &["rev-parse", "HEAD^{tree}"]);
    let since = |since: &str| {
        Request::GitStatus(GitStatusParams {
            project_id: project.id,
            worktree_task_id: None,
            repo: None,
            mode: GitMode::Base,
            since: Some(since.to_owned()),
        })
    };
    refused(&mut client, since(&tree));
    let head = git(&repo, &["rev-parse", "HEAD"]);
    let status: GitStatus = client.call(since(&head)).into_result().unwrap();
    assert_eq!(status.merge_base.as_deref(), Some(head.as_str()));

    // Hot reload: the setting applies from a later turn start (the watcher is debounced).
    env.write_settings("[features]\nturn_snapshots = true\n");
    let deadline = Instant::now() + TIMEOUT;
    loop {
        hook(&mut client, "busy");
        if !refs().is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "the setting was not picked up");
        hook(&mut client, "done");
        std::thread::sleep(Duration::from_millis(200));
    }
    let listed = turns(&mut client);
    assert!(listed.first().unwrap().start_tree.is_none(), "the first turn stays unsnapshotted");
    assert!(listed.last().unwrap().start_tree.is_some());
    client.stop();
}
```

Run: `cargo test -p mkrew-daemon --test daemon -j 4 -- turn_snapshots_off a_hook_turn_start turns_are_listed hook_edits_are worktree_tasks_run_and_resume`
Expected: PASS (needs the built `mkrew-fakeagent`, like the neighbouring tests).

- [ ] **Step 7: ADR 0041**

Create `docs/decisions/0041-optin-turn-snapshots-and-conflicts.md`:

```markdown
# 0041 — Turn snapshots and conflict detection are opt-in

- Date: 2026-10-10
- Status: accepted
- Milestone: none (outside the milestone loop; perf review `docs/PERF_REVIEW.md` A2, A3, inbox item 13)

## Context

- SPEC §20C.1 snapshots the working tree at the start and the end of every turn of every agent:
  copy the git index, `git add -A -- :/`, `git write-tree`, `git update-ref`, plus `git diff
  --numstat`; about 4–7 git processes per turn (~50 ms each on Windows), one at a time per
  project. A hook that starts a turn waits for the snapshot, so the agent is held up. Refs pile up
  in `refs/mkrew/turns` for 7 days.
- §7 `conflict.detected`: every file-watcher batch runs the editor tracker and a session list
  query, and the UI prunes its warnings on every event.
- The review says most users use neither "Turns… / Restore before turn", "Since turn…" and
  "Changed by", nor the two-agents-one-file warning, but all of them pay for it.
- Deleting both (the review's "Xoá gồm", ~3,100 lines, protocol `turn.*`) loses features the user
  may still want and breaks the wire contract and the `turn` table (migrations are never deleted).

## Options

1. Delete both features. Cheapest to run, but it throws away reviewed, tested code and the
   per-turn undo.
2. Make them cheaper (async queue, throttle). The hook no longer waits, but every turn still
   costs git and refs; conflict tracking still costs a query per batch.
3. Settings, off by default, hot-reloaded: `[features] turn_snapshots`, `conflict_detection`.
   (Chosen.)

## Decision

- `settings.toml` gets `[features]` with `turn_snapshots = false` and `conflict_detection =
  false` (SPEC §20.0). The daemon reads them at every turn start / watcher batch through its
  settings holder, so a toggle needs no restart; the UI reads the same file.
- Turn snapshots off: no snapshot, no `refs/mkrew/turns` ref, no `git diff --numstat`, and the
  hook of a turn start is answered at once. Turns are still counted and stored, `session.turn`
  is still sent with its file paths (no `added` / `removed`), and the pane-header summary stays.
  `turn.restore_preview`, `turn.restore` and `git.status` / `git.diff` with a tree `since` answer
  `invalid_state` naming the setting. The UI hides Turns…, Restore before a turn, Diff since,
  Since turn and Changed by.
- A turn keeps the decision it started with: a toggle applies from the next turn.
- `task.undo` is not affected (it uses the task checkpoint, ADR 0005).

## Consequences

- SPEC §20.0 (settings list and a `[features]` paragraph), §20A.3, §20C.1–§20C.4, the M11
  checklist and §10 (pane header) are updated in the same change. The wire is unchanged
  (`PROTOCOL_VERSION` is not bumped by this change); the `turn` table stays.
- Default behaviour changes for existing users: after the update there is no per-turn undo until
  `turn_snapshots = true`. The first start with the old refs still deletes expired ones.
- A turn that ended at `needs_input` with a start snapshot, followed by a start while the setting
  is off, keeps no end tree or line counts.
- A cached "since turn" diff can still be answered for a short time after a switch to off.
```

- [ ] **Step 8: SPEC**

`docs/SPEC.md`:
- §20.0 toml block: after

```toml
[zed]
follow_debounce_ms = 1500   # §20C.4

```

add

```toml
[features]
turn_snapshots = false      # §20C: a git snapshot at every agent turn (Turns…, Restore before a turn, Since turn, Changed by); ADR 0041
conflict_detection = false  # §7 conflict.detected and the card / pane warning; ADR 0041

```

- §20.0, right after the closing ```` ``` ```` of that block (:1536) add:

```markdown
**`[features]`** (ADR 0041): costly extras, off by default. The daemon re-reads them like the other
keys; a switch applies from the next turn (`turn_snapshots`) or the next watcher batch
(`conflict_detection`). The UI hides what a switched-off feature drives.
```

- §20A.3 (:1565-1566): replace `From M11 each\n   file also carries `added`/`removed` (numstat between the turn's start and end snapshots).` with `From M11, with\n   `[features] turn_snapshots` on (ADR 0041), each file also carries `added`/`removed` (numstat between the turn's start and end snapshots); off, they are absent and the rest is unchanged.`
- §20C.1 (:1647), before `**Turn snapshots.**` text starts, change `1. **Turn snapshots.** At each turn start` to `1. **Turn snapshots.** Only with `[features] turn_snapshots = true` (default off, ADR 0041; a switch applies from the next turn; off = no snapshot, no `refs/mkrew/turns` ref, the hook reply is not held). At each turn start`.
- §20C.2 (after the sentence `` `turn.restore` uses the same fingerprint check as `task.undo`. ``) add: ` With turn snapshots off, "Turns…" is hidden and `turn.restore_preview` / `turn.restore` answer `invalid_state`.`
- §20C.3 (after `Header shows `Since turn 3 of claude #2`.`) add: ` Hidden with turn snapshots off; `git.status` / `git.diff` with a tree `since` answer `invalid_state` (a commit `since` still works).`
- §20C.4 (after `(data: `session.files`, below).`) add: ` Hidden with turn snapshots off.`
- §20C protocol table: after the row `| `git.status` / `git.diff` (changed) | `since` also accepts a tree id | as before |` add the paragraph below the table: `With `[features] turn_snapshots` off: `turn.list` still answers (rows without trees); `turn.restore_preview`, `turn.restore` and a tree `since` answer `invalid_state`.`
- M11 checklist (:1442-1443): replace `- [ ] A turn snapshot is taken at each turn start without changing` with `- [ ] With `[features] turn_snapshots = true` a turn snapshot is taken at each turn start without changing`.
- §10 (:885): replace `and an agent's "Follow in Zed" and "Turns…"; the status` with `and an agent's "Follow in Zed" and "Turns…" (only with `[features] turn_snapshots` on, ADR 0041); the status`.

- [ ] **Step 9: HUMAN_CHECKS**

In `docs/HUMAN_CHECKS.md`, at the end of the section `### To verify — 2026-10-10 inbox: perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)` (the blank line above the next `### To verify —` heading), append:

```markdown
- [ ] **Turn snapshots off (default), item 13:** rebuild and restart the daemon. In a git workspace
  run an agent with hooks (claude) through 2 turns that edit a file. In a terminal:
  `git for-each-ref refs/mkrew` prints nothing; the agent is not delayed at the start of a turn
  (compare with the setting on); the pane header still says `✓ Done · 1 file · 40s`.
- [ ] **Turn snapshots on:** add `[features]` / `turn_snapshots = true` to `settings.toml` and save
  (no restart). The next turn creates `refs/mkrew/turns/<session>/<n>` (`git for-each-ref refs/mkrew`);
  the turn that was running keeps no snapshot. Back to `false`: no new refs from the next turn on.
```

- [ ] **Step 10: Verify and tick**

Run: `cargo test -p mkrew-core --lib settings -j 4`, `cargo test -p mkrew-daemon --lib -j 4 -- settings:: turns::`, `cargo test -p mkrew-daemon --test daemon -j 4 -- turn_snapshots_off a_hook_turn_start turns_are_listed hook_edits_are worktree_tasks_run_and_resume` (expect PASS), then the full verification. Do not tick inbox line 132 yet (Tasks 11 and 12 follow).
Suggested message: `feat(daemon): turn snapshots are opt-in via [features] turn_snapshots (ADR 0041)`

---

### Task 11: The UI hides turn features when snapshots are off

Inbox: line 132 (bullets "Off = UI hides what depends on it": Turns dialog, Undo dialog for turns, Git view "since turn", "Changed by"; do not tick yet). Spec: §20C.2–§20C.4 (SPEC.md:1659-1671), §10 (:885), §20.0 (:1501-1536), ADR 0041.

**Current state (investigated; main.rs line numbers from 2026-10-10, re-find by name):**
- Pane header (`Shell::render_pane`, the `.children(info.map(|info| { … }))` block, ~:16778): the `turns` element is `(info.kind == SessionKind::Agent).then(|| div().id("pane-turns-{session}") … .child(button_frame("pane-follow", …)).child(quiet_button("pane-turns", "Turns…", …)))`. Follow in Zed stays; only the `pane-turns` button depends on snapshots.
- Palette: `Extra::Turns` ("Agent: Turns…", `palette.rs`) is in `Extra::ALL`; `Extra::available` shows it with `InsertPrompt` / `FollowInZed` when `in_view(View::Terminals) && context.tabs > 0 && context.agent_pane`. `palette::Context` (Copy) is built in `Shell::open_palette` (~:4273). There is **no keymap command** for Turns (`keymap.rs` has none), so only the palette entry exists.
- `Shell::open_turns` (~:5918) opens `turns_dialog`; the Turns dialog's "Diff since" runs `diff_since_turn` (→ `show_git_since` with `Since::turn(..)`) and "Restore before" runs `restore_before_turn` (→ `UndoDialog::for_turn`, `undo_dialog.target == undo_dialog::Target::Turn(_)`). The `undo_dialog` is also the "Undo task" dialog (`Target::Task`, checkpoint based): that one stays.
- Git view: `GitView.since: Option<Since>` with `Origin::Turn { session_id, n }` or `Origin::Task(_)`; `GitView.changed_by: ChangedBy { asked, files, session }`; `Shell::load_session_files` (~:2868) calls `session.files` after every status; `Shell::git_changed_by` (~:12389) renders the "Changed by" chips from `view.changers(sessions)`. The inspector (session panel) has no "Changed by"; the only one is this Git file-list filter row.
- The UI reads settings with `mkrew_core::settings::load(&settings_file).unwrap_or_default()` (`Shell::new` ~:494, `open_composer` ~:4101, `toggle_follow` ~:5872). The 500 ms poll in the status tick compares the file mtime and calls `theme::reload` then `shell.apply_diff_mode(settings.ui.diff_mode)`. **T6 (B8) moves this reload from the 500 ms tick to `observe_window_activation`**; the anchor is the call `shell.apply_diff_mode(settings.ui.diff_mode);` wherever T6 left it (inside the reload block that reads `mkrew_core::settings::load(&shell.settings_file)`).

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs` (`Context.turn_snapshots`, `Extra::available`, tests)
- Modify: `crates/mkrew-ui/src/git_view.rs` (`GitView::turn_features_off`, test)
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell.features`, `Shell::new`, `apply_features`, reload call, palette context, `load_session_files`, `git_changed_by`, `open_turns`, pane header)
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `mkrew_core::settings::FeatureSettings` (T10).
- Produces: `palette::Context { …, pub turn_snapshots: bool }`; `GitView::turn_features_off(&mut self) -> Requests`; `Shell::apply_features(&mut self, features: FeatureSettings, window: &mut Window, cx: &mut Context<'_, Self>)`.

- [ ] **Step 1: Failing tests**

`crates/mkrew-ui/src/palette.rs`, `mod tests`: in `fn context(view: View) -> Context {` add `turn_snapshots: true,` after `agent_pane: false,`. After the existing test that asserts `extras(&agent_pane)` add:

```rust
    /// "Agent: Turns…" needs turn snapshots (ADR 0041); the other agent entries do not.
    #[test]
    fn turns_entry_needs_turn_snapshots() {
        let on = Context {
            agent_pane: true,
            ..context(View::Terminals)
        };
        let off = Context {
            turn_snapshots: false,
            ..on
        };
        assert!(Extra::Turns.available(&on));
        assert!(!Extra::Turns.available(&off));
        assert!(Extra::InsertPrompt.available(&off) && Extra::FollowInZed.available(&off));
    }
```

`crates/mkrew-ui/src/git_view.rs`, `mod tests`, after `changed_by_keeps_only_the_sessions_files`:

```rust
    /// Turn snapshots switched off while the view lists "since turn" or filters "Changed by":
    /// both go away (back to the working changes); a task's "since" is not a turn feature.
    #[test]
    fn turn_features_off_leaves_since_turn_and_changed_by() {
        let sessions = [session(1, None, 1, SessionState::Idle)];
        let mut view = loaded();
        view.changed_by.session = Some(Ulid(1));
        view.changed_by.files = SessionFiles::from([(Ulid(1), vec!["a.txt".into()])]);
        view.show_since(Since::turn(&sessions[0], &sessions, 3, "tree".into()));
        assert_eq!(view.section(), Section::Since);

        let requests = view.turn_features_off();
        assert_eq!(view.section(), Section::Working);
        assert!(requests.status, "the working changes are listed again");
        assert_eq!(view.changed_by, ChangedBy::default());

        let task = task_changes("abc");
        view.show_since(task.clone());
        assert_eq!(view.turn_features_off(), Requests::default());
        assert_eq!(view.section(), Section::Since);
        assert_eq!(view.since, Some(task));
    }
```

Run: `cargo test -p mkrew-ui --lib -j 4 -- palette::tests::turns_entry git_view::tests::turn_features_off`
Expected: FAIL to compile (`Context.turn_snapshots`, `turn_features_off` do not exist).

- [ ] **Step 2: Palette and Git view logic**

`palette.rs`: in `pub struct Context`, after `pub agent_pane: bool,` add:

```rust
    /// `[features] turn_snapshots` is on (ADR 0041): "Agent: Turns…" needs it.
    pub turn_snapshots: bool,
```

In `Extra::available`, replace

```rust
            Self::InsertPrompt | Self::Turns | Self::FollowInZed => {
                in_view(View::Terminals) && context.tabs > 0 && context.agent_pane
            }
```

with

```rust
            Self::InsertPrompt | Self::FollowInZed => {
                in_view(View::Terminals) && context.tabs > 0 && context.agent_pane
            }
            Self::Turns => {
                in_view(View::Terminals)
                    && context.tabs > 0
                    && context.agent_pane
                    && context.turn_snapshots
            }
```

(also update the comment above it: `the focused agent pane's "Turns…" (needs turn snapshots) and "Follow in Zed"`.)

`git_view.rs`, in `impl GitView`, after `pub fn filter_changed_by(…)`:

```rust
    /// Turn snapshots were switched off (ADR 0041): "Changed by" no longer filters, and a
    /// "since turn" listing (shown or kept behind the History) goes back to the working changes.
    /// A task's "since" stays: it is a commit, not a turn snapshot.
    pub fn turn_features_off(&mut self) -> Requests {
        self.changed_by = ChangedBy::default();
        let turn = matches!(
            self.since.as_ref().map(|since| since.origin),
            Some(Origin::Turn { .. })
        );
        if turn {
            let shown = self.section() == Section::Since;
            self.since = None;
            if shown {
                self.select_none();
                return self.list(GitMode::Working, None);
            }
        }
        self.filtered()
    }
```

Run: `cargo test -p mkrew-ui --lib -j 4 -- palette::tests git_view::tests::turn_features_off`
Expected: PASS.

- [ ] **Step 3: Shell state and the reads**

`crates/mkrew-ui/src/main.rs`:
- Imports: add `use mkrew_core::settings::FeatureSettings;` to the `use mkrew_core::…` group.
- `Shell` struct: after `settings_error: Option<String>,` (doc `/// Why the last read of `settings_file` failed; listed in the palette (§20.0).`) add:

```rust
    /// `[features]` of the settings file (ADR 0041): read at start and with the other settings.
    features: FeatureSettings,
```

- `Shell::new`: replace

```rust
        // A broken settings file keeps the defaults, like the other reads here.
        let ui = mkrew_core::settings::load(&settings_file)
            .unwrap_or_default()
            .ui;
```

with

```rust
        // A broken settings file keeps the defaults, like the other reads here.
        let settings = mkrew_core::settings::load(&settings_file).unwrap_or_default();
        let features = settings.features;
        let ui = settings.ui;
```

and in the `Self { … }` literal, after `settings_error: None,` add `features,`.
- The reload (where T6 left `shell.apply_diff_mode(settings.ui.diff_mode);`): directly after that line add

```rust
                                    shell.apply_features(settings.features, window, cx);
```

(the closure already has `window` and `cx`; a broken file leaves `Err`, so the last good features stay).
- Next to `fn apply_diff_mode`, add:

```rust
    /// `[features]` was (re)read (ADR 0041). Switching turn snapshots off closes what depends on
    /// them: the Turns dialog, a turn's Restore, "Since turn" and "Changed by" in every Git view.
    /// Switching conflict detection off clears the warnings (T12 reads `features` too).
    fn apply_features(
        &mut self,
        features: FeatureSettings,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.features == features {
            return;
        }
        self.features = features;
        if !features.turn_snapshots {
            let restoring = self
                .undo_dialog
                .as_ref()
                .is_some_and(|dialog| matches!(dialog.target, undo_dialog::Target::Turn(_)));
            let closing = restoring || self.turns_dialog.is_some();
            if restoring {
                self.undo_dialog = None;
            }
            self.turns_dialog = None;
            self.turns_return = None;
            if closing {
                self.focus_pane(window, cx);
            }
            let projects: Vec<Ulid> = self.git.keys().copied().collect();
            for project_id in projects {
                let requests = self.git_view(project_id).turn_features_off();
                self.git_requests(project_id, requests, window, cx);
            }
        }
        cx.notify();
    }
```

- Palette context in `open_palette`: after `agent_pane: self.focused_agent().is_some(),` add `turn_snapshots: self.features.turn_snapshots,`.
- `fn load_session_files(`: first statement of the body (before `let Some(connection) = …`): 

```rust
        // "Changed by" needs turn snapshots (ADR 0041): no `session.files` call without them.
        if !self.features.turn_snapshots {
            return;
        }
```

- `fn git_changed_by(` (it is `&self`): first statement `if !self.features.turn_snapshots { return None; }` (before `let changers = view.changers(self.sidebar.sessions());`).
- `fn open_turns(`: first statement `if !self.features.turn_snapshots { return; }`.
- Pane header: before `.children(info.map(|info| {` is not editable; inside that closure, before `let turns = (info.kind == SessionKind::Agent).then(|| {` add `let turn_snapshots = self.features.turn_snapshots;`, and replace the whole `.child(\n quiet_button("pane-turns", "Turns…", theme, theme.surface_raised) … })),\n )` expression (the second `.child(` of the `pane-turns-{session}` div) with:

```rust
                        .when(turn_snapshots, |el| {
                            el.child(
                                quiet_button("pane-turns", "Turns…", theme, theme.surface_raised)
                                    .px(px(space::XS))
                                    .py_0()
                                    .text_size(font_size::META)
                                    .tooltip(tooltip(
                                        theme,
                                        "The agent's turns: restore the files to before one",
                                        "",
                                    ))
                                    .on_click(cx.listener(move |shell, _, window, cx| {
                                        shell.open_turns(session, window, cx);
                                    })),
                            )
                        })
```

Build check only here (no cargo run): `cargo check -p mkrew-ui -j 4` expected clean; GPUI rendering is not testable headless, the decisions are in `Extra::available` and `GitView::turn_features_off` (tested above).

- [ ] **Step 4: SPEC**

`docs/SPEC.md` was updated for the UI parts in T10 Step 8 (§10, §20C.2–§20C.4). Add in §20C.2, after the sentence added there: ` The palette's "Agent: Turns…" is hidden too; a switch to off (re-read when the window gets focus) closes an open Turns dialog and Restore, and returns a "Since turn" or "Changed by" listing to the working changes.`

- [ ] **Step 5: HUMAN_CHECKS**

At the end of the perf section in `docs/HUMAN_CHECKS.md` append:

```markdown
- [ ] **UI with turn snapshots off (default), item 13:** open a workspace with an agent pane. The pane
  header shows `Follow in Zed` but no `Turns…`; the palette has `Agent: Insert prompt…` and
  `Agent: Follow in Zed` but no `Agent: Turns…`; the Git view has no `Changed by` row. The board's
  `Undo task` and a Review card's `View changes` still work.
- [ ] **UI with turn snapshots on:** set `[features] turn_snapshots = true`, save, click back into the
  window: `Turns…` appears in the pane header and the palette, the Turns dialog lists turns with
  `Diff since` and `Restore before`, `Changed by` shows in the Git view after the next change. Set it
  to `false` with the Turns dialog or a `Since turn` Git listing open, click the window: the dialog
  closes and the Git view goes back to the working changes.
```

- [ ] **Step 6: Verify and tick**

Run: `cargo test -p mkrew-ui --lib -j 4 -- palette:: git_view::` (expect PASS), then the full verification. Do not tick inbox line 132 yet (Task 12 follows).
Suggested message: `feat(ui): hide Turns, Since turn and Changed by while turn snapshots are off`

---

### Task 12: Conflict detection is opt-in (daemon and UI)

Inbox: line 132 (bullet "Off = zero cost" for A3 and "Off = UI hides … conflict warnings"; **tick it `[~]`→`[x]`**, last task of item 13). Spec: §7 events (SPEC.md:573), M5 checklist (:1369), §20.0 (:1501-1536), ADR 0041.

**Current state (investigated; line numbers from 2026-10-10; item 12 (T7–T9) rewrote parts of `files_changed`, so anchor on names and quoted text, not lines):**
- Daemon `session.rs`: `Hub.conflicts: Mutex<HashMap<Ulid, conflict::Tracker<Ulid>>>`. `fn record_edits(&self, project_id: Ulid, paths: &[String], editor: Ulid, at: i64)` has two callers: `files_changed` (watcher batch, when exactly one session of the project is `working`) and `hook_edits` (hook-reported edits). It prunes and updates the tracker; when there is an overlap it reads the sessions of the project from the store (item 12 B6 moves `files_changed` to the in-memory session list; this overlap read in `record_edits` is the one that remains) and broadcasts `Event::ConflictDetected`. `unwatch_idle_project` removes the project's tracker. `files_changed` still needs its `working` list for `Command::TurnFiles` (turn summaries), so only `record_edits` is gated. A single guard at the top of `record_edits` covers both callers.
- UI `main.rs` (`Shell::apply_event`, ~:8466): `let detected = self.conflicts.apply(event); if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected { cx.notify(); }` on **every** event; and after a list refresh (~:898) `shell.conflicts.prune(&shell.board, shell.sidebar.sessions());`. The badge renders (`conflict_badge`, board cards `cards.conflicts.for_task`, pane header `self.conflicts.for_session`) read `Shell.conflicts`, so an empty `Conflicts` is already "no conflict UI".
- `crates/mkrew-ui/src/conflicts.rs`: `Conflicts { list }`, `apply(&Event) -> bool`, `prune(&Board, &[Session]) -> bool`, tests with helpers `task`, `session`, `detected`, `updated`.
- `tests/daemon.rs` `two_sessions_editing_one_file_raise_a_conflict` runs two `writer` agents that write `shared.txt` and then hook edits of `src/hooked.rs`. Helper `changed_files(&env, id)` reads the `files_changed` session events from the DB: **after item 12 (B6) it records only batches with a `.md` file**, so the new off-test writes `shared.md` instead of `shared.txt`. `Env::write_settings` comes from T10; `client.events` (a `Vec<Event>` of everything pushed so far) is readable in this file.
- T10 added `Hub::features()` and `Shell.features` / `apply_features` (T11); T12 uses both.

**Files:**
- Modify: `crates/mkrew-daemon/src/session.rs` (`record_edits`)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (`two_sessions_editing_one_file_raise_a_conflict`, new test)
- Modify: `crates/mkrew-ui/src/conflicts.rs` (`Conflicts::track`, test)
- Modify: `crates/mkrew-ui/src/main.rs` (event handler, list refresh, `apply_features`)
- Modify: `docs/SPEC.md` (§7 row, M5 checklist), `docs/decisions/0041-optin-turn-snapshots-and-conflicts.md` (Consequences), `docs/HUMAN_CHECKS.md`, `docs/INBOX.md`

**Interfaces:**
- Consumes: `Hub::features() -> FeatureSettings` (T10), `Shell.features` (T11).
- Produces: `Conflicts::track(&mut self, enabled: bool, event: Option<&Event>, board: &Board, sessions: &[Session]) -> bool` (on: `apply` the event if any, then `prune`; off: forget every warning; returns whether the shown warnings changed).
- Protocol: no wire change. No migration.

- [ ] **Step 1: Failing tests**

`crates/mkrew-ui/src/conflicts.rs`, `mod tests`, at the end:

```rust
    /// Off (the default, ADR 0041): events are not kept, nothing is pruned, and switching it
    /// off clears what was shown.
    #[test]
    fn off_ignores_events_and_clears_warnings() {
        let project = Ulid::new();
        let mut board = Board::default();
        board.load(Vec::new());
        let (sa, sb) = (session(None, project), session(None, project));
        let sessions = vec![sa.clone(), sb.clone()];
        let found = detected(project, "a.rs", &[&sa, &sb]);
        let mut conflicts = Conflicts::default();

        assert!(!conflicts.track(false, Some(&found), &board, &sessions));
        assert!(conflicts.for_session(sa.id).is_empty());

        assert!(conflicts.track(true, Some(&found), &board, &sessions));
        assert_eq!(conflicts.for_session(sa.id), ["a.rs"]);
        assert_eq!(conflicts.for_session(sb.id), ["a.rs"]);

        // Switched off: the warnings go once; a second call has nothing to report.
        assert!(conflicts.track(false, None, &board, &sessions));
        assert!(conflicts.for_session(sa.id).is_empty());
        assert!(!conflicts.track(false, None, &board, &sessions));
    }
```

`crates/mkrew-daemon/tests/daemon.rs`:

(a) in `two_sessions_editing_one_file_raise_a_conflict`, directly after `let env = Env::new();` add:

```rust
    env.write_settings("[features]\nconflict_detection = true\n");
```

(b) new test after it. Same two-writer scenario, but the file is `shared.md` (so item 12's `files_changed` event proves the batch was processed) and the setting stays off:

```rust
/// ADR 0041: by default two sessions editing one file raise no `conflict.detected`, from the
/// watcher or from hook edits.
#[test]
fn conflict_detection_off_raises_no_conflict() {
    let env = Env::new();
    let repo = env.repo();
    let scenario = env.dir.path().join("writer.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"spinner\"\ntext = \"reading\"\nms = 500\n\n\
         [[step]]\naction = \"write_file\"\npath = \"shared.md\"\ncontent = \"x\"\n\n\
         [[step]]\naction = \"spinner\"\ntext = \"thinking\"\nms = 3000\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "writer",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let run = |client: &mut Client, title: &str| {
        let task: Task = client
            .call(create_task(&project, title, Some("writer")))
            .into_result()
            .unwrap();
        let spawned: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
        client.wait_state(spawned.session_id, SessionState::NeedsInput);
        spawned.session_id
    };
    let (a, b) = (run(&mut client, "A"), run(&mut client, "B"));

    // The watcher batch with `shared.md` was handled: it is recorded for the session.
    let deadline = Instant::now() + TIMEOUT;
    while !changed_files(&env, b).contains("shared.md") {
        assert!(Instant::now() < deadline, "the watcher batch never arrived");
        std::thread::sleep(Duration::from_millis(100));
    }
    // Hook edits of one file by both sessions (the second call returns after the first was
    // handled, so an event would be in `client.events` by now).
    let edit = |path: &str| json!({"tool_input": {"file_path": path}});
    for session in [a, b] {
        client
            .hook(session, "fake", "PostToolUse", edit("src/hooked.rs"))
            .into_result::<()>()
            .unwrap();
    }
    client.call(Request::DaemonStatus).into_result::<serde_json::Value>().unwrap();
    assert!(
        !client.events.iter().any(|e| matches!(e, Event::ConflictDetected(_))),
        "{:?}",
        client.events
    );
    client.stop();
}
```

(if the request is named differently, use any cheap request the file already sends, e.g. the one `wait_error` uses for `daemon.status`; it only has to make the client read pending events.)

Run: `cargo test -p mkrew-ui --lib -j 4 -- conflicts::tests::off_ignores` and `cargo test -p mkrew-daemon --test daemon -j 4 -- conflict`
Expected: the UI test FAILS to compile (`track` does not exist). The daemon `…raise_a_conflict` test (setting on) passes already; `conflict_detection_off_raises_no_conflict` FAILS (an event arrives) until Step 2.

- [ ] **Step 2: Daemon gate**

`crates/mkrew-daemon/src/session.rs`, `fn record_edits(&self, project_id: Ulid, paths: &[String], editor: Ulid, at: i64) {`: before `let found: Vec<(String, Vec<Ulid>)> = {` insert:

```rust
        // Off by default (ADR 0041): no tracker, no session query, no `conflict.detected`.
        if !self.features().conflict_detection {
            return;
        }
```

Run: `cargo test -p mkrew-daemon --test daemon -j 4 -- two_sessions_editing_one_file_raise_a_conflict conflict_detection_off`
Expected: PASS (both).

- [ ] **Step 3: UI `Conflicts::track` and its callers**

`crates/mkrew-ui/src/conflicts.rs`, in `impl Conflicts`, after `pub fn prune(…)`:

```rust
    /// One event (or, with `None`, a list refresh) for the warnings. `enabled` is `[features]
    /// conflict_detection` (ADR 0041): off, no event is kept, nothing is pruned (no work per
    /// event) and warnings left from an earlier "on" are forgotten. Returns whether the shown
    /// warnings changed.
    pub fn track(
        &mut self,
        enabled: bool,
        event: Option<&Event>,
        board: &Board,
        sessions: &[Session],
    ) -> bool {
        if !enabled {
            let shown = !self.list.is_empty();
            self.list.clear();
            return shown;
        }
        let detected = event.is_some_and(|event| self.apply(event));
        self.prune(board, sessions) || detected
    }
```

`crates/mkrew-ui/src/main.rs`:
- In the event handler replace

```rust
                // After the board and sidebar, so a task leaving Running clears its warning.
                let detected = self.conflicts.apply(event);
                if self.conflicts.prune(&self.board, self.sidebar.sessions()) || detected {
                    cx.notify();
                }
```

with

```rust
                // After the board and sidebar, so a task leaving Running clears its warning.
                // Off (the default, ADR 0041): nothing is tracked or pruned per event.
                let enabled = self.features.conflict_detection;
                if self
                    .conflicts
                    .track(enabled, Some(event), &self.board, self.sidebar.sessions())
                {
                    cx.notify();
                }
```

- After the list refresh, replace

```rust
                shell
                    .conflicts
                    .prune(&shell.board, shell.sidebar.sessions());
```

with

```rust
                let enabled = shell.features.conflict_detection;
                shell
                    .conflicts
                    .track(enabled, None, &shell.board, shell.sidebar.sessions());
```

- In `apply_features` (T11), after the `if !features.turn_snapshots { … }` block and before `cx.notify();` add:

```rust
        // Off clears the warnings; on starts again with the next `conflict.detected`.
        self.conflicts.track(
            features.conflict_detection,
            None,
            &self.board,
            self.sidebar.sessions(),
        );
```

Run: `cargo test -p mkrew-ui --lib -j 4 -- conflicts::tests` (expect PASS), `cargo check -p mkrew-ui -j 4` (expect clean).

- [ ] **Step 4: SPEC and ADR**

`docs/SPEC.md`:
- §7 events table, replace the row `| `conflict.detected` | two sessions edited the same file within 60 s (M5): `project_id`, `path`, `session_ids`, `task_ids` |` with `| `conflict.detected` | two sessions edited the same file within 60 s (M5), only with `[features] conflict_detection = true` (default off, ADR 0041): `project_id`, `path`, `session_ids`, `task_ids` |`.
- M5 checklist: replace `- [ ] Two sessions in one project edit the same file within 60 s → `conflict.detected`, UI warns on both cards.` with `- [ ] With `[features] conflict_detection = true`: two sessions in one project edit the same file within 60 s → `conflict.detected`, UI warns on both cards; off (default), neither.`

`docs/decisions/0041-optin-turn-snapshots-and-conflicts.md`, Consequences: append

```markdown
- Conflict detection off (the default): the daemon keeps no editor tracker, runs no session query
  for it and sends no `conflict.detected`; the UI does not prune warnings per event and shows no
  card or pane warning. §7 and the M5 checklist say so. Warnings already shown are cleared when
  the setting is switched off.
```

- [ ] **Step 5: HUMAN_CHECKS and inbox**

At the end of the perf section in `docs/HUMAN_CHECKS.md` append:

```markdown
- [ ] **Conflict warning off (default), item 13:** two agents in one workspace both edit `shared.txt`
  (use two fake or real agents). No `⚠ Conflict` badge on the cards or pane headers, and
  `mk events` (or the daemon log at `MKREW_LOG=info`) has no `conflict detected` line.
- [ ] **Conflict warning on:** add `[features]` / `conflict_detection = true`, save, click back into
  the window, repeat: the `⚠ Conflict: shared.txt` badge shows on both cards and panes, and `Dismiss`
  clears it. Set it back to `false`: the badges disappear at once and no new ones appear.
```

`docs/INBOX.md` line 132: change `- [~] **13. Perf: turn snapshots and conflict detection become opt-in**` to `- [x] **13. Perf: turn snapshots and conflict detection become opt-in**` (if it still reads `- [ ]`, set `[x]`; the plan link is added by the plan owner).

- [ ] **Step 6: Verify and tick**

Run: `cargo test -p mkrew-ui --lib -j 4 -- conflicts:: palette:: git_view::`, `cargo test -p mkrew-daemon --test daemon -j 4 -- conflict two_sessions_editing turn_snapshots_off` (expect PASS), then the full verification. This task ticks inbox line 132 `[~]`→`[x]`.
Suggested message: `feat: conflict detection is opt-in via [features] conflict_detection (ADR 0041)`

---

### Task 13: Daemon screen scan builds 4 lines, at most 4 scans/s per session (perf B9)

Inbox: line 138 `14. Perf: terminal path (daemon scan, scrollback, replay)` (bullet B9; do not tick yet). Spec: §6 (SPEC.md:441-456), §8 (:619-626), ADR 0015 (`docs/decisions/0015-visible-change-is-activity.md`), ADR 0003 (state source). New ADR 0043.

**Current state (investigated):**
- `Actor::on_output` (`crates/mkrew-daemon/src/session.rs`, ~1180-1215) runs for every output batch of every live session, background ones included (≤ 60/s). It calls `Emulator::text_fingerprint()` (SipHash over every cell of the screen). Only when the hash changed does it call `Actor::screen_lines()` = `Emulator::lines_to_cursor()` (`daemon/src/emulator.rs`, ~113): one `String` per row from the top to the cursor row, then `Actor::scan_ports(&lines)` and `StateMachine::on_output(Instant::now(), &lines)`.
- The state machine reads only the last `PATTERN_TAIL_LINES = 4` non-blank rows (`crates/mkrew-core/src/state.rs:21`; `StateMachine::needs_input` trims, drops blanks, takes 4). The `tick` branch of `Actor::run` (`sleep_until(tick_at)`) also builds the full text.
- Other consumers of the full text: only `Actor::scan_ports` (a run session's port list; `self.ports.is_some()` only). Not users: `session.read` uses `Emulator::last_lines` (whole visible screen, own code), the terminal title uses `Emulator::take_title`, peek and attach use `snapshot()`.
- `Actor::deliver()` is called at the end of every `on_output`. `SendQueue::on_output` clears `in_flight`, then `deliver()` pops the next text if `can_deliver(self.session.state)`. Today the state flips to `working` synchronously in the same call before `deliver()`. A throttled scan would leave a stale `idle` there and deliver the second queued text into a busy agent. So a non-empty queue must bypass the throttle (`ScanRate::on_output(now, urgent)` below).
- Hooks do NOT cover the scan (checked, so the scan stays; reasons in Open questions): `mkrew_core::hook` maps Claude `UserPromptSubmit`→busy, `Notification` (permission/elicitation)→needs_input, `Stop`→done; Codex only `agent-turn-complete`→done; omp/copilot map busy/needs_input/done. No hook reports `idle`: `working→idle` is the screen going quiet (`StateMachine::tick`). Profiles `claude.toml` / `codex.toml` carry screen patterns (`'❯ 1\. Yes'`, `'(?i)allow command'`) the hooks do not replace, and the daemon cannot tell whether the user ran `mk integration install` (no daemon code reads it). No shipped profile has `mode = "hook"`.
- `mod` list in `crates/mkrew-daemon/src/lib.rs` is alphabetical (`mod plan; mod server;`). `session.rs` has `mod tests` with a pure-logic style (`OutputRate`, `Pacer` tests with `Instant::now()` plus `ms(n)` offsets).
- Daemon e2e `tests/daemon.rs`: `fake_agent_session_reaches_needs_input_and_is_killed` (the fake agent prints a prompt then goes silent, so it needs the trailing scan), `send_while_working_is_queued_until_the_agent_asks_and_wait_sees_it_finish`, `a_queued_text_removed_while_working_is_never_typed`.
- Line numbers in `session.rs` shift after items 12 and 13 (Tasks 7-12 edit this file). Anchors below are quoted from the code, not line numbers.

**Files:**
- Create: `crates/mkrew-daemon/src/scan.rs` (`ScanRate`, `SCAN_GAP`, tests)
- Create: `docs/decisions/0043-terminal-path-limits.md`
- Modify: `crates/mkrew-daemon/src/lib.rs` (`mod scan;`)
- Modify: `crates/mkrew-daemon/src/emulator.rs` (`lines_to_cursor`, new `tail_to_cursor`, `cursor_row`, `row_chars`, `row_text`; tests)
- Modify: `crates/mkrew-daemon/src/session.rs` (`Actor` field `scan`, `Actor::run` branch, `on_output`, new `scan_screen`, `screen_tail`, `finish`)
- Modify: `crates/mkrew-core/src/state.rs` (doc of `PATTERN_TAIL_LINES` only)
- Modify: `docs/SPEC.md` §6 (:453-456), §8 (:625-626)
- Modify: `docs/HUMAN_CHECKS.md` (append to the shared perf section)

**Interfaces:**
- Produces: `Emulator::tail_to_cursor(&self, count: usize) -> Vec<String>`: the last `count` non-blank rows from the top to the cursor row, top-down, trailing blanks trimmed.
- Produces: `crate::scan::ScanRate` with `new(gap: Duration) -> Self`, `on_output(&mut self, now: Instant, urgent: bool) -> bool`, `deadline(&self) -> Option<Instant>`, `scanned(&mut self, now: Instant)`, `take_pending(&mut self) -> bool`; `pub const SCAN_GAP: Duration` (250 ms).
- Consumes: `mkrew_core::state::PATTERN_TAIL_LINES`, `StateMachine::on_output/tick`.

- [ ] **Step 1: Failing tests**

In `crates/mkrew-daemon/src/emulator.rs` `mod tests`, after `lines_stop_at_the_cursor_row`, add:

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

Create `crates/mkrew-daemon/src/scan.rs` containing only the tests (the type does not exist yet):

```rust
//! Rate limit of the per-session screen scan (ADR 0043). Pure: the caller passes `now`.

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
}
```

In `crates/mkrew-daemon/src/lib.rs`, after `mod plan;` add `mod scan;`.

Run: `cargo test -p mkrew-daemon --lib scan::tests emulator::tests::the_tail -j 4`
Expected: FAIL to compile (`ScanRate`, `SCAN_GAP`, `tail_to_cursor` do not exist).

- [ ] **Step 2: `Emulator::tail_to_cursor`**

`crates/mkrew-daemon/src/emulator.rs`: replace the whole `lines_to_cursor` method (from `/// Visible lines from the top of the screen down to the cursor row, trailing blanks` through its closing `}`, the `.collect()` of the `(0..=last).map(...)` chain) with:

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
    /// builds a `String` only for the rows it keeps, so a screen scan costs 4 allocations, not
    /// one per row.
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

`crates/mkrew-core/src/state.rs`, in the doc of `PATTERN_TAIL_LINES`, replace `The daemon passes the screen up to the cursor row.` with `The daemon builds only these rows (\`Emulator::tail_to_cursor\`) of the screen up to the cursor row.`

Run: `cargo test -p mkrew-daemon --lib emulator::tests -j 4`
Expected: PASS (`the_tail_is_what_the_full_text_fed_the_state_machine`, and the existing `lines_stop_at_the_cursor_row`, `scrolled_output_keeps_the_visible_screen`).

- [ ] **Step 3: `ScanRate`**

`crates/mkrew-daemon/src/scan.rs`: after the `//!` line and before `#[cfg(test)]`, add:

```rust
use std::time::{Duration, Instant};

/// Two scans of one session are at least this far apart: at most 4 a second (ADR 0043).
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
```

Run: `cargo test -p mkrew-daemon --lib scan::tests -j 4`
Expected: PASS (5 tests).

- [ ] **Step 4: Wire it into the actor**

`crates/mkrew-daemon/src/session.rs`:

(a) After `use crate::emulator::Emulator;` add:
```rust
use crate::scan::{SCAN_GAP, ScanRate};
```
and next to the other `mkrew_core` use lines add a separate line (do not edit the existing `use mkrew_core::{…}` line, items 12/13 touch it):
```rust
use mkrew_core::state::PATTERN_TAIL_LINES;
```

(b) `struct Actor`: replace
```rust
    screen_print: Option<u64>,
    /// `session.send` texts waiting for the session to be ready.
```
with
```rust
    screen_print: Option<u64>,
    /// At most one screen scan per [`SCAN_GAP`], plus a trailing one (ADR 0043).
    scan: ScanRate,
    /// `session.send` texts waiting for the session to be ready.
```

(c) In the `Actor { … }` literal in `Hub::start`, replace `            screen_print: None,` with
```rust
            screen_print: None,
            scan: ScanRate::new(SCAN_GAP),
```

(d) `Actor::run`: after the `let tick_at = self.machine.next_deadline().map(tokio::time::Instant::from_std);` statement add:
```rust
            let scan_at = self.scan.deadline().map(tokio::time::Instant::from_std);
```
In the `tokio::select!`, replace the tick branch
```rust
                () = sleep_until(tick_at), if tick_at.is_some() => {
                    let lines = self.screen_lines();
                    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
                    if let Some(state) = self.machine.tick(Instant::now(), &lines) {
```
with
```rust
                () = sleep_until(scan_at), if scan_at.is_some() => {
                    // The scan held back by `ScanRate`: the final screen is always classified.
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
(the rest of the tick branch stays).

(e) `Actor::on_output`: replace
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
        // The screen work runs at most every `SCAN_GAP` (ADR 0043). A waiting `session.send`
        // needs the current state when `deliver()` runs below, so it scans every batch.
        let now = Instant::now();
        if self.scan.on_output(now, !self.queue.is_empty()) {
            self.scan_screen(now);
        }
```

(f) Replace the method `fn screen_lines(&self) -> Vec<String> { … }` (the whole method, 5 lines) with:
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
        // Only visible changes are work: idle TUIs keep sending cursor/title/progress bytes,
        // which would otherwise hold the session in `working` forever (ADR 0015). The first
        // scan always counts, so hook-only profiles still leave `starting`.
        let print = self.emulator().text_fingerprint();
        if self.screen_print.replace(print) == Some(print) {
            return;
        }
        if self.ports.is_some() {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            self.scan_ports(&lines);
        }
        let tail = self.screen_tail();
        let tail: Vec<&str> = tail.iter().map(String::as_str).collect();
        if let Some(state) = self.machine.on_output(now, &tail) {
            self.set_state(state, StateSource::Screen);
        }
    }
```

(g) `Actor::finish`: replace
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
Expected: no errors, no new warnings (`screen_lines` is still used by `scan_screen` and `finish`).

- [ ] **Step 5: Daemon e2e still holds**

Run: `cargo test -p mkrew-daemon --test daemon fake_agent_session_reaches_needs_input -j 4` then `cargo test -p mkrew-daemon --test daemon send_while_working a_queued_text_removed exiting_with_queued -j 4`
Expected: PASS (the prompt is found by the trailing scan; queued texts still go one at a time because a non-empty queue scans every batch).

- [ ] **Step 6: ADR 0043 and SPEC**

Create `docs/decisions/0043-terminal-path-limits.md`:

```markdown
# 0043 — Terminal path limits: screen scan rate, UI scrollback, log replay

- Date: 2026-10-10
- Status: accepted
- Milestone: post-M13 perf (docs/PERF_REVIEW.md B9, B10, B12)

## Context

The perf review found three costs on the terminal path. (B9) The daemon hashes every cell of a
session's screen after each output batch (up to 60/s per session, background ones too) and, when
the hash changed, builds a `String` for every row up to the cursor and runs the state patterns,
although the state machine reads only the last `PATTERN_TAIL_LINES` = 4 rows (ADR 0015 added the
fingerprint, ADR 0003 the state source). (B10) The UI keeps 10,000 scrollback lines per pane and
rebuilds its whole cell grid, each cell with a `Vec<char>`, on every frame with output. (B12)
Opening an exited session in History replays its log into a new emulator.

## Options

1. Cut the cost per scan only (build 4 rows), keep one scan per batch.
2. Also cap the scan rate, with a trailing scan so the final screen is always seen.
3. Skip the scan for agents with hooks. Rejected: no hook reports `idle`, Codex hooks report only
   `done`, and the daemon cannot tell whether the user installed the hooks.

## Decision

- **Screen scan (B9).** Option 2. The state machine gets only the last 4 non-blank rows up to the
  cursor (`Emulator::tail_to_cursor`). A session's screen is scanned at most every 250 ms
  (`scan::ScanRate`); output inside the gap is scanned by a trailing scan when the gap ends, so a
  prompt that is the last thing printed is never missed. A session with a queued `session.send`
  scans every batch (the send queue needs the current state). A state change can therefore show up
  to 250 ms after the output that caused it. The fingerprint (ADR 0015) is compared between two
  scans: a change that is undone inside one gap is not seen.

## Consequences

- SPEC §6 and §8 say so.
- `working` can start up to 250 ms late and `needs_input` up to 250 ms late; `idle` (3 s quiet) is
  unaffected.
```

`docs/SPEC.md` §6: after the sentence ending `…gives garbage.` (:456) add a new paragraph:

```markdown
The daemon scans a session's screen at most 4 times a second (a scan after the last output
closes the gap, so the final screen is always classified) and the state machine reads only the
last 4 non-blank rows up to the cursor; a state shown by the screen can therefore appear up to
250 ms after the output (ADR 0043). A session with a queued `session.send` is scanned on every
output.
```

§8 (:625-626): replace
```markdown
- Output fan-out per chunk: emulator → state detector, session log file (buffered writer),
  attached clients (binary frames, §7).
```
with
```markdown
- Output fan-out per chunk: emulator → state detector (at most 4 scans/s per session, ADR 0043),
  session log file (buffered writer), attached clients (binary frames, §7).
```

- [ ] **Step 7: HUMAN_CHECKS**

Append to the end of the section `### To verify — 2026-10-10 inbox: perf (plan docs/superpowers/plans/2026-10-10-inbox-perf.md)` in `docs/HUMAN_CHECKS.md` (after its last bullet, before the next `### To verify —` heading):

```markdown
- [ ] **Terminal path, screen scan (T13):** restart the daemon. With claude (hooks off, patterns only) and a fake agent or a shell in the other two panes:
  - Ask claude something that needs approval: the pane shows `needs_input` (yellow) and the notification fires within about half a second, also when the prompt is the last thing printed and the pane is in a background tab.
  - Answer it: the state goes back to `working`, then `idle` about 3 s after the output stops.
  - `mk send <id> "one"` and `mk send <id> "two"` while the agent works: both are typed one after the other, "two" only after the agent reacted to "one".
  - A run command that prints `Local: http://localhost:5173` shows `:5173` in the Run button, also when it prints that line and exits at once.
  - Task Manager: the daemon's CPU with three agents `working` is lower than before (note the numbers here).
```

- [ ] **Step 8: Verify and tick**

Run: `cargo test -p mkrew-daemon --lib scan emulator -j 4` and `cargo test -p mkrew-core state -j 4` (expect PASS), then the full verification (`scripts/verify.sh`). Do not tick inbox line 138 yet (Tasks 14 and 15 follow).
Suggested message: `perf(daemon): scan the screen at most 4 times a second and build only the 4 lines the state machine reads`

---

### Task 14: UI scrollback 5,000 lines and a reusable terminal grid (perf B10)

Inbox: line 138 (bullet B10; do not tick yet). Spec: §10 terminal rendering (SPEC.md:808-818), ADR 0030 (`docs/decisions/0030-snapshot-carries-history.md`), ADR 0043.

**Current state (investigated):**
- `crates/mkrew-ui/src/terminal/emulator.rs`: `const SCROLLBACK_LINES: usize = 10_000;` (used in `Emulator::new` as `scrolling_history` and by three tests through the constant, so they follow the new value). The comment in `view_point` says `the history 10k lines`.
- `Emulator::grid()` builds `vec![vec![blank; cols]; screen_lines]` (a new `Vec` per row; every `GridCell` holds `zerowidth: Vec<char>`), then overwrites every cell from `renderable_content().display_iter`. `shared_grid()` calls it inside `last_grid.get_or_insert_with` once per change, and `Emulator::changed()` (called by every `&mut self` method: `advance`, `resize`, scroll, selection, search) sets `last_grid` to `None`.
- Consumers of the grid cell type: `Grid::to_text` and `Grid::row_chars` (same file); `layout.rs` (`layout_row`: `!cell.zerowidth.is_empty()`; `is_blank`; `push_text`: `segment.text.extend(cell.zerowidth.iter())`; `CachedRow { cells: Vec<GridCell> }` + `RowCache::row` compare `self.cells == cells` and `cells.to_vec()`); `view.rs` (`Prepainted.grid: Arc<Grid>`; paint loop over `grid.rows`; `Pane::link_at` uses `shared_grid().row_chars(row)`); `links.rs` takes `&[char]` from `row_chars` (does not see the cell type); `find.rs` tests read `rows[0][0].found`; selection and copy use alacritty's `selection_to_string`, not the grid; tests `tests/terminal_runs.rs` (`grid.rows`, `layout_row`) and `tests/stress.rs` (`pane.grid().to_text()`). `crates/spike-term` has its own copy: not touched.
- Painting holds an `Arc<Grid>` (`Prepainted`) only for the duration of a frame, so by the next change `last_grid` is normally the sole owner and `Arc::try_unwrap` succeeds.
- SPEC §10 (:809) says `scrollback (UI-side, 10,000 lines, only for attached panes)`. `HUMAN_CHECKS.md` mentions the 10k scrollback only in old, historical records (not edited). ADR 0030's option 2 mentions "10,000 lines each" as a rejected option (not edited).
- `Cursor` and `Grid` do not derive `Default` today.
- Chosen type: `Option<Box<[char]>>` (`None` for ~every cell: no allocation, 16 bytes; the field cannot be `Some(empty)`), with an accessor `zerowidth() -> &[char]` so consumers stay one-liners.

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/emulator.rs` (`SCROLLBACK_LINES`, `GridCell`, `Cursor`/`Grid` derives, `Grid::to_text`, `Emulator` field `spare`, `new`, `changed`, `shared_grid`, `grid`, new `fill_grid`, comment in `view_point`, tests)
- Modify: `crates/mkrew-ui/src/terminal/layout.rs` (`layout_row`, `is_blank`, `push_text`, test)
- Modify: `docs/SPEC.md` §10 (:809)
- Modify: `docs/decisions/0043-terminal-path-limits.md` (append bullets)
- Modify: `docs/HUMAN_CHECKS.md` (append to the shared perf section)

**Interfaces:**
- Produces: `GridCell { …, pub zerowidth: Option<Box<[char]>>, … }` with `pub fn zerowidth(&self) -> &[char]`.
- Produces: `Grid: Default`, `Cursor: Default`; `Emulator::grid(&self) -> Grid` unchanged signature (a fresh grid); `Emulator::shared_grid(&self) -> Arc<Grid>` unchanged signature, now rebuilt into the previous frame's buffer when nobody else holds it.

- [ ] **Step 1: Failing tests**

`crates/mkrew-ui/src/terminal/emulator.rs` `mod tests`, after `resize_changes_dimensions`:

```rust
    #[test]
    fn zero_width_chars_survive_in_the_grid() {
        let mut emu = Emulator::new(10, 2);
        // e + combining acute, a space, a wide thumbs-up + VS16, x.
        emu.advance("e\u{301} \u{1F44D}\u{FE0F}x".as_bytes());
        let grid = emu.grid();
        assert_eq!(grid.rows[0][0].c, 'e');
        assert_eq!(grid.rows[0][0].zerowidth().to_vec(), vec!['\u{301}']);
        assert!(grid.rows[0][1].zerowidth().is_empty());
        assert_eq!(grid.rows[0][2].c, '\u{1F44D}');
        assert_eq!(grid.rows[0][2].zerowidth().to_vec(), vec!['\u{FE0F}']);
        assert_eq!(
            grid.to_text().lines().next(),
            Some("e\u{301} \u{1F44D}\u{FE0F}x")
        );
    }

    /// A grid being painted is shared: the next frame is built elsewhere, never over it.
    #[test]
    fn a_grid_still_being_painted_is_never_overwritten() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"old");
        let painting = emu.shared_grid();
        emu.advance(b"\rnew");
        let next = emu.shared_grid();
        assert!(!Arc::ptr_eq(&painting, &next));
        assert_eq!(painting.to_text().lines().next(), Some("old"));
        assert_eq!(next.to_text().lines().next(), Some("new"));
    }

    /// The frame after a released one is built into the same cell buffers (no per-frame
    /// allocation of rows x cols cells).
    #[test]
    fn a_released_grid_buffer_is_reused_for_the_next_frame() {
        let mut emu = Emulator::new(10, 3);
        emu.advance(b"one");
        let first = emu.shared_grid().rows[0].as_ptr();
        emu.advance(b"\rtwo");
        let next = emu.shared_grid();
        assert_eq!(next.rows[0].as_ptr(), first, "same cell buffer");
        assert_eq!(next.to_text().lines().next(), Some("two"));
    }

    /// A rebuilt grid never shows cells of the buffer it reused: after a shrink, a grow and a
    /// cleared selection it equals a grid built from scratch, in every dimension.
    #[test]
    fn a_reused_buffer_shows_no_stale_cells() {
        let mut emu = Emulator::new(12, 4);
        emu.advance("hello e\u{301}\r\nworld\r\nthird\r\nfourth".as_bytes());
        emu.start_selection(0, 0, Side::Left);
        emu.update_selection(1, 3, Side::Right);
        drop(emu.shared_grid());

        emu.resize(5, 2);
        let small = emu.shared_grid();
        assert_eq!((small.cols, small.rows.len()), (5, 2));
        assert!(small.rows.iter().all(|row| row.len() == 5));
        assert_eq!(*small, emu.grid());
        drop(small);

        emu.clear_selection();
        emu.resize(14, 6);
        let big = emu.shared_grid();
        assert_eq!((big.cols, big.rows.len()), (14, 6));
        assert!(big.rows.iter().all(|row| row.len() == 14));
        assert!(
            big.rows
                .iter()
                .flatten()
                .all(|cell| !cell.selected && cell.found == Found::No)
        );
        assert_eq!(*big, emu.grid());
    }
```

`crates/mkrew-ui/src/terminal/layout.rs` `mod tests`, after the helper `fn row(…)`'s first dependent test (any place after `fn style`):

```rust
    /// A base char with zero-width chars is shaped as one cluster (its own segment), so the
    /// accent is not drawn on the next cell.
    #[test]
    fn zero_width_chars_stay_with_their_base_cell() {
        let theme = Theme::dark();
        let cells = row("ae\u{301}b".as_bytes(), 8);
        let layout = layout_row(&cells, None, &theme);
        let segments: Vec<(&str, usize)> = layout
            .segments
            .iter()
            .map(|segment| (segment.text.as_str(), segment.start_col))
            .collect();
        assert_eq!(segments, [("a", 0), ("e\u{301}", 1), ("b", 2)]);
    }
```

Run: `cargo test -p mkrew-ui --lib terminal -j 4`
Expected: FAIL to compile (`GridCell::zerowidth()` does not exist).

- [ ] **Step 2: Cell type without a `Vec` per cell**

`emulator.rs`:

Replace
```rust
const SCROLLBACK_LINES: usize = 10_000;
```
with
```rust
const SCROLLBACK_LINES: usize = 5_000;
```
In `view_point`, replace `the history 10k lines` with `the history 5k lines`.

Replace
```rust
    /// Zero-width chars attached to `c` (combining marks, VS16, ZWJ sequences).
    pub zerowidth: Vec<char>,
```
with
```rust
    /// Zero-width chars attached to `c` (combining marks, VS16, ZWJ sequences). `None` for
    /// nearly every cell, so a grid holds no allocation per cell; never `Some(empty)`.
    pub zerowidth: Option<Box<[char]>>,
```
After the `GridCell` struct, add:
```rust
impl GridCell {
    /// The zero-width chars attached to [`GridCell::c`], in order.
    pub fn zerowidth(&self) -> &[char] {
        self.zerowidth.as_deref().unwrap_or(&[])
    }
}
```
`#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct Cursor` → `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]`; `#[derive(Clone, Debug, PartialEq)] pub struct Grid` → `#[derive(Clone, Debug, Default, PartialEq)]`.

In `Grid::to_text` replace `.chain(cell.zerowidth.iter().copied())` with `.chain(cell.zerowidth().iter().copied())`.

`layout.rs`: in `layout_row` replace `|| !cell.zerowidth.is_empty() {` with `|| !cell.zerowidth().is_empty() {`; in `is_blank` replace `cell.zerowidth.is_empty()` with `cell.zerowidth().is_empty()`; in `push_text` replace `segment.text.extend(cell.zerowidth.iter());` with `segment.text.extend(cell.zerowidth().iter());`.

- [ ] **Step 3: Rebuild into the released buffer**

`emulator.rs`, struct `Emulator`: after the `last_grid` field add
```rust
    /// The previous grid when [`Emulator::changed`] found no one painting it: the next
    /// [`Emulator::shared_grid`] rebuilds into its buffers instead of allocating rows x cols
    /// cells per frame.
    spare: RefCell<Option<Grid>>,
```
In `Emulator::new`, after `last_grid: RefCell::default(),` add `spare: RefCell::default(),`.

Replace
```rust
    fn changed(&mut self) {
        *self.last_grid.get_mut() = None;
    }
```
with
```rust
    fn changed(&mut self) {
        // Sole owner (the painter dropped its clone): keep the buffers for the next frame.
        // A grid still being painted is left alone and freed by its last owner.
        if let Some(last) = self.last_grid.get_mut().take()
            && let Ok(grid) = Arc::try_unwrap(last)
        {
            *self.spare.get_mut() = Some(grid);
        }
    }
```
Replace `shared_grid` (its 3-line body) with
```rust
    pub fn shared_grid(&self) -> Arc<Grid> {
        let mut last = self.last_grid.borrow_mut();
        Arc::clone(last.get_or_insert_with(|| {
            let mut grid = self.spare.borrow_mut().take().unwrap_or_default();
            self.fill_grid(&mut grid);
            Arc::new(grid)
        }))
    }
```
Replace the whole `pub fn grid(&self) -> Grid { … }` (from `pub fn grid(&self) -> Grid {` to its closing `}`, ending with `Grid { cols, rows, cursor }`) with:

```rust
    /// A freshly built snapshot of the visible screen ([`Emulator::shared_grid`] reuses
    /// buffers instead).
    pub fn grid(&self) -> Grid {
        let mut grid = Grid::default();
        self.fill_grid(&mut grid);
        grid
    }

    /// Rewrites `grid` to show the visible screen: every row is cleared and refilled with blank
    /// cells first, so nothing of the previous frame (cells, selection, find marks, zero-width
    /// chars, a different size) survives.
    fn fill_grid(&self, grid: &mut Grid) {
        let cols = self.term.columns();
        let blank = GridCell {
            c: ' ',
            zerowidth: None,
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            selected: false,
            found: Found::No,
        };
        grid.cols = cols;
        grid.rows.resize_with(self.term.screen_lines(), Vec::new);
        for row in &mut grid.rows {
            row.clear();
            row.resize(cols, blank.clone());
        }
        let content = self.term.renderable_content();
        let offset = content.display_offset as i32;
        let selection = content.selection;
        for indexed in content.display_iter {
            let row = (indexed.point.line.0 + offset) as usize;
            let col = indexed.point.column.0;
            if let Some(slot) = grid.rows.get_mut(row).and_then(|r| r.get_mut(col)) {
                *slot = GridCell {
                    selected: selection.is_some_and(|range| range.contains(indexed.point)),
                    found: Found::No,
                    c: indexed.cell.c,
                    zerowidth: indexed
                        .cell
                        .zerowidth()
                        .filter(|chars| !chars.is_empty())
                        .map(Box::from),
                    fg: indexed.cell.fg,
                    bg: indexed.cell.bg,
                    flags: indexed.cell.flags,
                };
            }
        }
        grid.cursor = Cursor {
            row: (content.cursor.point.line.0 + offset).max(0) as usize,
            col: content.cursor.point.column.0,
            visible: content.cursor.shape != CursorShape::Hidden,
        };
        // One pass over the matched cells (the current match last, so it wins), not a lookup
        // per cell: a one-letter query can match thousands of times on screen.
        for found in self.visible_matches() {
            mark(&mut grid.rows, &found, offset, Found::Match);
        }
        if let Some(current) = &self.current_match {
            mark(&mut grid.rows, current, offset, Found::Current);
        }
    }
```
(The body lines of the old `grid()` that this replaces are the same logic: blank cell, `display_iter` overwrite, cursor, `visible_matches` / `current_match` marks. Only the target changed from local `rows` to `grid.rows` and the allocations went away.)

Run: `cargo test -p mkrew-ui --lib terminal -j 4`
Expected: PASS (the four new emulator tests, the layout test, and the existing `shared_grid_is_rebuilt_after_every_change`, `selection_marks_cells_and_copies_text`, `history_is_capped_at_the_scrollback_limit`, find tests). Then `cargo test -p mkrew-ui --test terminal_runs -j 4` (expect PASS, snapshot unchanged: the grid content is identical).

- [ ] **Step 4: SPEC and ADR**

`docs/SPEC.md` :809, replace `scrollback (UI-side, 10,000 lines, only for attached panes),` with `scrollback (UI-side, 5,000 lines, only for attached panes; ADR 0043),`.

`docs/decisions/0043-terminal-path-limits.md`: after the `- **Screen scan (B9).** …` bullet in `## Decision` add:
```markdown
- **UI scrollback (B10).** 10,000 lines per pane become 5,000 (RAM and resize reflow grow with the
  history); the log on disk still keeps everything. A `GridCell` keeps its zero-width chars in an
  `Option<Box<[char]>>` instead of a `Vec<char>`, and `Emulator::shared_grid` rebuilds into the
  buffers of the previous frame when the painter released it.
```
and after the last bullet of `## Consequences` add:
```markdown
- SPEC §10 says 5,000 lines. A pane scrolls back at most 5,000 lines; older output is in History
  (the session log).
```

- [ ] **Step 5: HUMAN_CHECKS**

Append to the end of the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Terminal path, scrollback and grid (T14):**
  - In a pwsh pane run `1..8000 | % { "line $_" }`, then Shift+PgUp until it stops: the oldest line is about `line 3000`-`3100`, and Ctrl+Shift+F finds `line 7000` but not `line 100`. `mk session history <id>` still prints all 8000 lines.
  - In Git Bash run `printf 'e\xcc\x81 \xf0\x9f\x91\x8d\xef\xb8\x8f \xf0\x9f\x91\xa8\xe2\x80\x8d\xf0\x9f\x91\xa9 x\n'`: the accent sits on the `e`, the thumbs-up and the person pair are drawn whole and `x` is on the same line, in light and dark.
  - Drag the pane splitter and the window edge while a pane prints: no leftover glyphs or stripes after the resize, selection highlight and find highlight follow the text.
  - With six panes flooding (the stress setup of T2.9) the UI memory is lower than with 10,000 lines (note the numbers).
```

- [ ] **Step 6: Verify and tick**

Run: `cargo test -p mkrew-ui --lib terminal -j 4` and `cargo test -p mkrew-ui --test terminal_runs --test stress -j 4` (expect PASS), then the full verification. Do not tick inbox line 138 yet (Task 15 follows).
Suggested message: `perf(ui): 5,000-line pane scrollback and a terminal grid that reuses its buffer without a Vec per cell`

---

### Task 15: History replays only the last 1 MiB of a log, from a safe boundary (perf B12)

Inbox: line 138 (bullet B12; ticks `[~]`→`[x]`: the last task of item 14). Spec: §8 session logs (SPEC.md:645), notes (SPEC.md:1825-1829), ADR 0030, ADR 0043.

**Current state (investigated; the review's premise is out of date):**
- The review says opening an exited session replays ~10 MB (`terminal/view.rs:134-135`). `TerminalPane::replay(session, log: &[u8], …)` only does `Emulator::new(cols, rows)` + `emulator.advance(log)` with whatever bytes it is given. Its only caller, `Shell::select_history` (`main.rs`), already reads `history::read_tail(&history.files, history::TAIL_BYTES)` off the UI thread, and `crates/mkrew-ui/src/history.rs` has `pub const TAIL_BYTES: u64 = 2 * 1024 * 1024;`. So today it replays at most **2 MiB**, not 10 MB. This task lowers the cap to 1 MiB (the inbox's ~1 MB).
- The daemon does not serve the log bytes: `Request::SessionHistory` (`daemon/src/server.rs`) returns only the paths (`session_log::files`: `<id>.log.1`, `<id>.log`; rotation at 5 MB, `ROTATE_AT_BYTES`), and `read_tail` opens them in the UI process. No protocol change.
- `read_tail` reads from the end (`seek(len - left)`), so only the cap is read. When cut it drops everything up to the first `\n` of the joined bytes (`bytes.iter().position(|&byte| byte == b'\n')`, `bytes.drain(..=newline)`). `\n` never occurs inside a UTF-8 multibyte char, so the char boundary and the escape sequences are safe. **Bug:** when the cut tail has no `\n` at all (a TUI that repaints with cursor moves), nothing is dropped and the replay starts mid-sequence or mid-char. The fix is a pure fn `replay_start` with fallbacks.
- No SGR reset is needed: every replay is a fresh `Emulator::new`, which starts with default attributes, and the cut drops whole lines before the start. Modes set before the cut (alt screen `ESC[?1049h`) are lost; SPEC :1826-1827 already documents that a session ending on the alternate screen shows its last screen with little scrollback.
- ADR 0030: attach snapshots carry the daemon's ≤ 200 history lines, "older output is in the History dialog (session log)". Consistent: History stays the place for old output, now the newest 1 MiB; the dialog's note (`Older output is left out; mk session history <id> prints all of it.`) already points to the full log.
- Existing tests in `history.rs`: `read_tail_joins_the_parts_and_cuts_at_a_line_break` and `a_replayed_tail_shows_the_sessions_last_screen` (cut inside an escape sequence: stays valid).
- "Log write blocking on the session task" (review): `Actor::on_output` calls `SessionLog::write` (a `BufWriter` with the default 8 KiB buffer) on the session's async task, so each 8 KiB triggers a blocking `write` syscall and rotation does `flush` + `rename` + `open` there. Not cheap and safe to fix (see Open questions): not changed here.
- `SPEC.md` :1826 says `newest 2 MiB only, starting after the first line break past the cut` and :1829 `a 2 MiB log may stall one frame`; `HUMAN_CHECKS.md` :845 says `A log > 2 MB shows the "Older output is left out" note.`

**Files:**
- Modify: `crates/mkrew-ui/src/history.rs` (`TAIL_BYTES`, new `replay_start`, `read_tail`, tests)
- Modify: `docs/SPEC.md` (:1826, :1829 notes)
- Modify: `docs/decisions/0043-terminal-path-limits.md` (append bullets)
- Modify: `docs/HUMAN_CHECKS.md` (:845 wording; append to the shared perf section)
- Modify: `docs/INBOX.md` (tick line 138)

**Interfaces:**
- Produces: `pub fn replay_start(bytes: &[u8]) -> usize` in `crates/mkrew-ui/src/history.rs`.
- Changes: `pub const TAIL_BYTES: u64` = 1 MiB. `read_tail(files: &[PathBuf], limit: u64) -> io::Result<Tail>` keeps its signature.

- [ ] **Step 1: Failing tests**

`crates/mkrew-ui/src/history.rs` `mod tests`, after `read_tail_joins_the_parts_and_cuts_at_a_line_break`:

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
    fn a_big_log_replays_at_most_a_mebibyte_of_whole_lines() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("s.log");
        std::fs::write(&log, b"0123456789abcdef\r\n".repeat(200_000)).unwrap();
        let tail = read_tail(&[log], TAIL_BYTES).unwrap();
        assert!(tail.cut);
        let len = tail.bytes.len() as u64;
        assert!(len <= TAIL_BYTES && len > TAIL_BYTES - 64, "{len} bytes");
        assert!(tail.bytes.starts_with(b"0123456789abcdef"));
    }
```
Byte check: `b"\x82\xac rest\nnext"` is 2 + 5 + 1 = 8 bytes before `next`; `b"x\x1b[1my\nz"`: x, ESC, `[`, `1`, `m`, y, `\n` = 7; `b"1;32mgreen"` = 10 bytes before the ESC.

Run: `cargo test -p mkrew-ui --lib history -j 4`
Expected: FAIL to compile (`replay_start` does not exist; `a_big_log…` would also fail at 2 MiB).

- [ ] **Step 2: Cap and safe boundary**

`history.rs`: replace
```rust
/// Most log bytes replayed (the newest ones); `mk session history <id>` prints all of it.
pub const TAIL_BYTES: u64 = 2 * 1024 * 1024;
```
with
```rust
/// Most log bytes replayed (the newest ones, ADR 0043); `mk session history <id>` prints all of it.
pub const TAIL_BYTES: u64 = 1024 * 1024;
```
Replace the doc comment and head of `read_tail`:
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
// ponytail: a log with no line break and no ESC for the whole tail keeps its first bytes as they
// are (plain text, harmless); an ESC inside an OSC/DCS string would start there. Scan back from
// before the cut if a real log shows it.
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
In the body of `read_tail`, replace
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

Run: `cargo test -p mkrew-ui --lib history -j 4`
Expected: PASS (the four new tests; the existing `read_tail_joins_the_parts_and_cuts_at_a_line_break` and `a_replayed_tail_shows_the_sessions_last_screen` are unchanged and pass).

- [ ] **Step 3: SPEC, ADR, HUMAN_CHECKS wording**

`docs/SPEC.md`: at :1826 replace `newest 2 MiB only, starting after the first line break past the cut;` with `newest 1 MiB only (ADR 0043), starting after the first line break past the cut (at the first escape sequence when there is none);`. At :1829 replace `(a 2 MiB log may stall one frame)` with `(a 1 MiB log may stall one frame)`.

`docs/decisions/0043-terminal-path-limits.md`: after the `- **UI scrollback (B10).** …` bullet add:
```markdown
- **History replay (B12).** Opening an exited session replays the newest 1 MiB of its log (was
  2 MiB; the review's "10 MB" was the rotated log size, the UI already cut at 2 MiB). The cut
  starts after the first line break, else at the first ESC, so the emulator never starts inside a
  char or an escape sequence. Older output stays in the log (`mk session history`), as ADR 0030
  says for output beyond the daemon's 200 history lines.
```
and after the last bullet of `## Consequences` add:
```markdown
- SPEC notes (T5.x History) say 1 MiB. History shows the newest 1 MiB; the dialog's note points
  to `mk session history` for the rest.
```

`docs/HUMAN_CHECKS.md` :845, replace `A log > 2 MB shows the "Older output is left out" note.` with `A log > 1 MB shows the "Older output is left out" note.`

- [ ] **Step 4: HUMAN_CHECKS**

Append to the end of the perf section in `docs/HUMAN_CHECKS.md`:

```markdown
- [ ] **Terminal path, History replay (T15):**
  - In a pwsh pane run `1..60000 | % { "`e[32mline $_`e[0m" }` (a log of more than 1 MB), exit the session, open History and select it: the "Older output is left out" note shows, the replay opens at once, the last lines are `line 60000`, green, and the first visible line after scrolling to the top is a whole `line N` (no stray `[32m`, no `?` box at the start of the first line).
  - A short session (a few KB) replays completely with no note. A full-screen TUI session (claude or omp) shows its last screen without garbage at the top.
```

- [ ] **Step 5: Verify and tick**

Run: `cargo test -p mkrew-ui --lib history -j 4` (expect PASS), then the full verification. This is the last task of item 14: in `docs/INBOX.md` change line 138 `- [ ] **14. Perf: terminal path (daemon scan, scrollback, replay)**` (or `[~]`) to `- [x] **14. Perf: terminal path (daemon scan, scrollback, replay)** → plan: docs/superpowers/plans/2026-10-10-inbox-perf.md Tasks 13-15`.
Suggested message: `perf(ui): History replays the newest 1 MiB of a session log from a safe boundary`

---

## Open questions (defaults used above)

| Area (Tn) | Question | Options | Default used |
| --- | --- | --- | --- |
| T1 | While any dialog is open, should hidden-model events still redraw? (dialogs such as compare / history / task editor read models not audited) | a) yes, keep today's behavior; b) audit each dialog | a) `Shown.modal` forces `true` |
| T1 | Window minimized: `panes` is empty, so pane-only data skips the redraw; the restore re-renders (`observe_window_visibility`). Gate everything on `window.is_visible()`? | a) no, leave; b) also skip all redraws while minimized | a) leave (not in the item) |
| T1 | Terminal-title and other sidebar-owned events (`SessionTerminalTitle`, `SessionUpdated`) still always redraw; narrow them to expanded workspaces? | a) keep (Inbox / dots / `Viewed` depend on them); b) narrow later | a) keep |
| T1 | The HUMAN_CHECKS section is also created by the redesign plan's Task 1 (above the first `### To verify —` heading). | a) insert this one directly above the first heading at execution time; b) merge | a) |
| T2 | Row builders take owned rows, so the draw still clones the visible rows each frame (strings), though the sort / HashMap / lowercase / `format!` are cached. Change `workspace_row` / `session_row` / compact variants to `&Row`? | a) leave (text moves into elements anyway); b) change signatures | a) leave |
| T2 | `most_urgent` / `tab_kind` signatures change (lookup closure instead of a slice); their tests change shape, not behavior. | a) change (clean cutover); b) keep slice overloads | a) change |
| T4 | After a re-read that replaces the text (agent edit) the preview shows "Rendering the preview…" for the 300 ms wait; the first page of a file is built at once | a) wait on re-read too b) build re-reads at once (no debounce for the agent-save burst) | a) |
| T5 | After a commit in the Git view the title badge can lag up to 2 s (Git view's own list is immediate) | a) accept b) also call `git_summary_throttle.refreshed` + load on Git-view actions | a) |
| T6 | B8 changes SPEC §20.0 ("within 500 ms" → on activation); needs the ADR | ADR 0044 (item 11 slot), SPEC and ADR 0024 edited in the same task | ADR 0044 |
| T6 | `render_editor`: `cx.theme()` borrow inside the `uniform_list` processor may not compile | borrow; else clone once per closure call | borrow, fallback documented in Step 3 |
| T7 | Which `session_event` kinds may be pruned? | a) all kinds older than 7 days; b) only `hook`, `files_changed`, `edited` | b: `usage` rows are read with `since = 0` by `session.usage` (UI totals) and must outlive a week |
| T8 | The review says the watcher "ALWAYS" sends `GitChanged`; in code `debounce` already drops empty batches. Is more needed? | a) only make the send condition explicit; b) also stop watching ignored dirs at the OS level (`notify` has no exclude, would need per-directory non-recursive watches) | a (the `.git`/`target` storm already costs no `stat`; b is a bigger design) |
| T8 | "Any agent `working`" for the 5 s cap: only `SessionKind::Agent`, or every kind? | a) agents only; b) any session kind (what `files_changed` attribution always did) | b: one `working` map shared with T9's attribution; a shell printing output counts as working |
| T8 | A closed view needs a new request (`watch.release`) → protocol 38 + ADR 0042; no existing request fits (`ProjectRef` cannot name a worktree). | a) new request; b) daemon idle timeout; c) keep as is | a |
| T8 | A second UI client would lose a watcher the first released. | a) no ref count (one window); b) ref count per client | a |
| T9 | `docs_changed` still reads and hashes each changed `.md` on the watcher task (review B6 second half). | a) leave it (bounded, feature with HUMAN_CHECKS T6.4); b) move to `spawn_blocking` | a: not in the inbox line; separate change if it shows in a profile |
| T9 | Turn tests used `notes.txt` + `files_changed` as their "watcher saw it" barrier. | a) switch the scenarios to `notes.md`; b) add another barrier | a |
| T7 | The `backup::start` signature gains the db path (`Paths::db_file()`); `Store::backup_to` is removed. | a) free fn `mkrew_store::backup`; b) keep `backup_to` and open a second `Store` | a |
| T10 | With snapshots off, is `turn.list` still answered? | answer with rows without trees / refuse | answer (turn rows and `session.turn` are kept; the UI hides the list anyway) |
| T10 | Hook-turn wait: drop the reply in `track_turn` or make the recorder drop it first? | gate in `track_turn` (no wait for queued jobs) / gate only in `Recorder::start` | gate in `track_turn` via `turns::start_job`, so the hook never waits behind a queued end snapshot |
| T10 | `Hub` needs settings access; how? | `Hub::new(store, settings)` (touches 4 test callers) / `OnceLock` set by `Daemon::new` | `OnceLock` + `use_settings`; no features until set (tests of other parts see off) |
| T11 | The Undo dialog is also "Undo task" (checkpoint based, no snapshots). Hide it entirely? | hide only the turn flavour (`Target::Turn`) / hide both | hide only the turn flavour; `task.undo` does not need snapshots |
| T11 | The inbox names an inspector "Changed by"; the inspector has none, only the Git file-list filter row has. | hide that row (done) / add nothing | hide the Git row, nothing in the inspector |
| T11 | When the setting is switched on while the Git view is open, "Changed by" appears only after the next status refresh. | trigger a refresh on switch / leave | leave (next `git.changed` or showing the view again refreshes it) |
| T12 | `files_changed` events are written only for `.md` batches after item 12; the off-test relies on that. | write the file as `shared.md` (done) / poll another signal | `shared.md`; if T9 lands differently, poll any recorded batch of the session |
| Hooks vs scan (T13) | Skip the scan for agents with hooks? | (a) skip when `mode` includes `hook`; (b) keep; (c) skip only for a hypothetical `mode = "hook"` | (b) keep. No hook reports `idle` (it is the screen going quiet), Codex hooks report only `done`, Claude's patterns (`❯ 1\. Yes`) are not replaced by hooks, and the daemon cannot tell whether the user ran `mk integration install`. No shipped profile uses `mode = "hook"`, so (c) saves nothing today. |
| Scan gap (T13) | Is 250 ms (4 scans/s) right? A state can show up to 250 ms late. | 100 / 250 / 500 ms | 250 ms (`scan::SCAN_GAP`) as the inbox says ~4/s. |
| Premise (T15) | The review says History replays ~10 MB; the code already cuts at 2 MiB (`history::TAIL_BYTES`). | Lower to 1 MiB (done); or only document | Lowered to 1 MiB; the review's number was the log rotation size (5 MB × 2). |
| SGR reset (T15) | Prepend `ESC[0m` to a cut replay? | Add / not | Not added: each replay is a fresh `Emulator` with default attributes and whole lines are dropped before the start. |
| Alt screen (T15) | A cut can drop the `ESC[?1049h` of a TUI session, so its replay paints on the main screen. | Scan back for the last `?1049h`/`?1049l` and prepend it / leave | Leave (SPEC :1826-1827 already documents the limitation). |
| Log write blocking (T15) | `SessionLog::write` (`BufWriter`, 8 KiB) writes on the session's async task. | (a) leave; (b) `BufWriter::with_capacity(64 KiB)` in `session_log::open` (one line, 8× fewer syscalls, but the log of a live session lags up to 64 KiB for `session.history`); (c) a writer thread fed by a channel (changes flush/rotate/exit ordering, needs its own task) | (a) leave: (b) changes what a live History shows, (c) is not cheap. Decide with item 12's SQLite/IO work. |
