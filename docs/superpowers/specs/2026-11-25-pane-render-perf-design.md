# Pane render performance - Design

- Date: 2026-11-25
- Status: design approved in chat by the user (2026-11-25), section by section; this file waits for the user's review.
- Branch and worktree: `feat/pane-perf` in `.worktrees/pane-perf`, from `master` at `a2033e7`.
- ADR: `docs/decisions/0114-pane-render-perf.md`, written with the code (repo rule: ADR and SPEC change in the same commit as the code).
  `master` has 0112 (omp surface full width); the unmerged `fix/omp-composer-input` takes 0113, so this one takes 0114.
  Re-check when this branch merges; take the next free number if it moved.
- Protocol: no change.
- Migration: none.
- Depends on: `fix/omp-composer-input` (ADR 0113) rewrites large parts of `shell/tsp_view.rs` (its dock and composer).
  Implementation starts after that branch merges, or rebases onto it, so the two never conflict.
- Restores SPEC §15 "Input latency: key press to glyph on screen < 16 ms", which the app no longer meets with long omp sessions.

## 1. Goal

The user reports that Mkrew feels choppy, typing included.

Measured on 2026-11-25:

- The installed 0.9.0 UI thread uses 46-47% of one core continuously, with about 20 live omp sessions.
- A sandbox with a release build and a frame timer, one pane printing 12 times a second and one omp pane:

| Scenario | UI CPU (% of one core) | Frame time, mean / max |
|---|---|---|
| Empty sandbox | 2.7% | about 2.5 ms |
| One terminal pane printing | 7% | 3.2 / 4.5 ms |
| Plus one omp pane, short transcript | 10.5% | 4.5 / 7 ms |
| Plus one omp pane, long transcript (a copy of a real 14 MB session) | 69.7% | 52 / 65 ms |

Inside one 52 ms frame: about 4 ms builds the element tree, about 3 ms prepaint, about 4.5 ms paint.
The other 40 ms is layout (taffy and text measuring) of the whole omp transcript.

Two causes:

1. Nothing below the `Shell` view is cached.
   GPUI marks every ancestor of a notifying view dirty (`Window::mark_view_dirty`), so every terminal output, every omp frame, every 500 ms status tick and every key press renders `Shell` again, and with it every pane.
   Only the sidebar is a cached child view today (`shell/sidebar_view.rs`).
2. The omp surface (TSP) is not virtualized.
   `render_tsp` builds an element for every child of `main` inside one `overflow_y_scroll` div, so its layout cost grows with the session length.

Success means, measured with the sandbox scenario above (one printing pane plus one omp pane with the long transcript):

- Frame time under 8 ms (today 52 ms).
- UI CPU under 15% of one core (today 70%).
- Frame time about the same with a 100-row and a 2,000-row transcript.
- Nothing looks or behaves differently.

## 2. What the user said, and what was assumed

Said by the user:

- Mkrew is choppy, typing included; research why and propose a fix.
- Chose: virtualize the omp surface, cache every pane, and leave the Chat list as it is (it is capped at 50 turns, `mkrew_core::thread::CHAT_TURNS`).
- Chose approach A for invalidation: a cached pane view that renders again whenever `Shell` notifies.
- Build it in its own worktree.

Assumed (accepted in chat):

- The success numbers in section 1.
- No change to the wire protocol, the daemon or the look.

## 3. Cached pane views

### 3.1 `PaneView` (`crates/mkrew-ui/src/shell/pane_view.rs`)

```rust
pub(crate) struct PaneView {
    shell: WeakEntity<Shell>,
    host: HostId,
    project_id: Ulid,
    key: PaneKey,
}
```

- It follows `SidebarView`: `new` calls `cx.observe(&shell, |_, _, cx| cx.notify())`.
- `render` calls `Shell::render_pane(host, project_id, &key, window, cx)` through `shell.update`; a dropped `Shell` renders `gpui::Empty`.

### 3.2 `Shell::render_pane`

- It works out what `render_tab` passes to `pane_frame` today: `several`, `zoomed`, the pane's `width` (the host's recorded content size times the pane's rect width), `is_focused`, `composer_focused`, `voice_ring`, `now` (`status::now_ms()`) and the theme.
  A cached view has no way to receive per-frame arguments from its parent, so it reads them from `Shell` and the window itself.
- Then it calls `pane_frame` unchanged.

### 3.3 Where pane views live

- `Shell.pane_views: HashMap<(HostId, PaneKey), Entity<PaneView>>`.
- `render_tab` makes a view on first use and puts `view.cached(StyleRefinement::default().size_full())` inside each pane's absolutely positioned slot, in the main window and in detached windows alike.
- `sync_panes` drops the views whose key is in no tab layout of their host any more.

### 3.4 The terminal grid

- `pane_frame` embeds a `TerminalPane` as `pane.clone().cached(StyleRefinement::default().size_full())`.
- Output notifies only the `TerminalPane`; it renders and paints the grid alone.
- A `PaneView` that renders again reuses the grid's last frame.

### 3.5 When a pane renders again

| Trigger | What renders |
|---|---|
| `Shell` calls `cx.notify()` (its state changed, a status tick, a shown event) | every `PaneView` (through `observe`) |
| The window refreshes (focus moved, theme, font, window resize) | everything (GPUI drops every view cache) |
| A pane's bounds change (divider drag, zoom, resize, layout change) | that pane (bounds are part of the cache key) |
| Scrolling inside the pane | that pane (GPUI notifies the view that rendered the scrolled element, `div.rs` and `list.rs`) |
| Terminal output | that pane's grid only |

A `Shell` notify still renders every pane.
`ponytail:` comment on `PaneView`: invalidation follows every `Shell` notify; per-session notifies (only the pane whose session changed) are the upgrade if a profile shows `Shell` notifies dominate.

### 3.6 State recorded while painting

A cached view that is reused does not run its paint callbacks again, so anything a pane records while it paints must survive a reused frame.

- `MenuAnchors`: `Anchor::in_sidebar` becomes `in_cached_view` and also covers `Anchor::Pane(_)`.
  `Shell` clears only the other anchors at its frame start; each `PaneView` clears its own pane's anchor when it renders, as `SidebarView` does with `clear_sidebar`.
- `typing_target` (where voice input types): it records which pane registered the field (`None` for one `Shell` draws itself).
  `Shell` clears it at its frame start only when `Shell` owns it; a `PaneView` clears it when the field is its own.
- The web pane places its native child window from bounds read while painting.
  Bounds changes always miss the cache, so it should stay right; it is checked by hand (section 7).
- Every other `canvas(...)` recorder inside a pane is listed and checked in the plan.

## 4. Virtual omp transcript

### 4.1 State (`TspUi`, `shell/tsp_view.rs`)

- Removed: `scroll: ScrollHandle`, `reveal_pending`.
- Added:
  - `rows: gpui::ListState` (`ListAlignment::Top`, `FollowMode::Tail`, overdraw `theme::TSP_LIST_OVERDRAW`),
  - `row_ids: Vec<String>`: the ids of `main`'s children the list was last told about,
  - `rows_surface: Option<String>`: the shown surface those rows belong to.
- Item 0 is the spacer under the pane's drag strip (`space::L`), as today; item `i + 1` is row `i` of `main`.

### 4.2 Which rows changed (`crates/mkrew-ui/src/tsp.rs`)

- `Mirror::apply` returns `MirrorResult::Applied { dirty_rows: HashSet<String> }` (empty for `Open` and `Closed`, which `sync_rows` treats as a possible change of the shown surface).
  `dirty_rows` holds the ids of the frame surface's rows: every id an op names (`add` and `move` also name a parent) is mapped up to its ancestor that is a direct child of `main`.
  `sync_rows` uses them only when the frame's surface is the shown one.
  The mapping runs **before** the ops apply (so `del` and `move` find where the node was) and **after** (so `add`, `set`, `text`, `splice` and `move` find where it is).
  An op that names `main` itself, or a node outside `main` (the dock, a layer, the hud), dirties no row.
- A pure function computes what the list must hear:

```rust
pub enum RowChange {
    Reset(usize),
    Splice { range: Range<usize>, count: usize },
    Remeasure(usize),
}

pub fn row_changes(old: &[String], new: &[String], dirty: &HashSet<String>) -> Vec<RowChange>;
```

- It trims the common prefix and suffix of the two id lists; the middle is one `Splice`.
  Each kept row in `dirty` is a `Remeasure`.
  Indexes are row indexes; the caller adds 1 for the spacer.
- `TspUi::sync_rows` runs after every mirror change (a snapshot, an update, a surface opened or closed).
  A new snapshot or a different shown surface is a `Reset`.

### 4.3 UI-only changes that change a row's height

| Change | List call |
|---|---|
| A checklist phase folded or unfolded (`todo_toggled`) | `remeasure_items` for the row holding that checklist |
| A frame carrying `blobs` (an image arrived) | `remeasure_items` for each row holding an `image` node that names one of them |
| Font family or size changed (settings) | `rows.remeasure()` |
| Pane width changed | none: `ListState` measures again on a new width |

### 4.4 Scrolling

| Today | After |
|---|---|
| `at_bottom` then `scroll_to_bottom` on every frame | `FollowMode::Tail`: follows the end, stops when the user scrolls up, follows again at the end |
| `reveal`: `scroll_to_item(ix + 1)` and `reveal_pending` | `pause_following_tail()` then `scroll_to_reveal_item(ix + 1)` |
| `ListScroll::Top` | `scroll_to(ListOffset::default())` |
| `ListScroll::Bottom` | `scroll_to_end()` then `set_follow_mode(FollowMode::Tail)` |
| `ListScroll::Page { up }` and PageUp / PageDown | `scroll_by(-height)` / `scroll_by(height)`, `height` from `viewport_bounds()` |
| "Scroll to the latest message" shows while `!at_end` | shows while `!rows.is_following_tail()`; a press calls `scroll_to_end()` and `set_follow_mode(FollowMode::Tail)` |

### 4.5 Rendering

- `render_tsp` puts `gpui::list(ui.rows.clone(), render_row)` where the scroll div is today.
- `render_row` holds a `WeakEntity<Shell>` and the session id; for item 0 it returns the spacer, else it calls `Shell::tsp_row(session, ix - 1, window, cx)` through `shell.update`.
- `tsp_row` finds the shown document, builds the row's `Paint` and calls `tsp_node` as today.
  The Markdown cache (`md`, keyed by node revision) stays.
- The list keeps the mouse handlers of today's scroll div: a drag extends the selection, a press off any text clears it.

### 4.6 What stays as it is

- A bounded `screen` surface (one card, the Agent Hub, not scrolled; `tsp::screen_bounded`) keeps its flex column.
- The dock, the layers (modal) and the `list` nodes inside a row (pickers, with their own `ScrollHandle` in `ui.lists`).
- The Chat list.

## 5. Selection and hit areas

| Recorded while painting | Risk | Handling |
|---|---|---|
| `TspUi.hits` (where each `md` / `code` node's text was painted) | a row scrolled out keeps old positions; a drag over a node out of view could jump | each `PaneView` render clears its session's `hits`; only rows painted in that frame record again; `tsp_drag` without a hit keeps the selection (as today) |
| `selecting` (node id and range) | none: kept by id | unchanged; Ctrl+C works on a node scrolled out of view |
| `todo_chip`, the scroll of nested `list` nodes | in the dock or inside one row; a reused frame keeps the same bounds | unchanged |
| `ChatUi.hits`, Chat Tab stops | the Chat list is not virtualized; a reused frame keeps the whole paint | unchanged |

## 6. Code layout

| File | Change |
|---|---|
| `crates/mkrew-ui/src/shell/pane_view.rs` | new: `PaneView` |
| `crates/mkrew-ui/src/shell.rs` | `mod pane_view` |
| `crates/mkrew-ui/src/main.rs` | `Shell.pane_views`; frame start keeps cached-view anchors and pane-owned typing targets |
| `crates/mkrew-ui/src/shell/pane_render.rs` | `render_tab` embeds pane views; `render_pane`; the grid embedded `cached` |
| `crates/mkrew-ui/src/shell/menu_render.rs` | `in_cached_view`, a per-pane clear |
| `crates/mkrew-ui/src/ime.rs` | `TypingTarget` records the pane that registered its field |
| `crates/mkrew-ui/src/tsp.rs` | `Mirror::apply` dirty rows, `RowChange`, `row_changes` |
| `crates/mkrew-ui/src/shell/tsp_view.rs` | `TspUi` list state, `sync_rows`, `tsp_row`, scrolling, `hits` clearing |
| `crates/mkrew-ui/src/theme.rs` | `TSP_LIST_OVERDRAW` |
| `crates/mkrew-ui/tests/measure.rs` | the new measurement (section 7) |
| `docs/decisions/0114-pane-render-perf.md`, `docs/SPEC.md` (§10, §15), `docs/PROGRESS.md`, `docs/HUMAN_CHECKS.md` | records |

## 7. Testing

Unit tests (`cargo test`, no window):

- `tsp::row_changes`: rows added and removed at the start, in the middle and at the end; a common prefix and suffix; a dirty row that was kept is remeasured, a dirty row that left is not; identical lists give nothing.
- `Mirror::apply` dirty rows, one case per op: `add` into a row and as a new row, `set`, `text`, `splice`, `move` inside one row and from one row to another, `del` of a leaf and of a whole row, an op on `main` itself, an op on a dock node.

Measurement (`crates/mkrew-ui/tests/measure.rs`, `#[ignore]`, a real window, like the other SPEC §15 measurements):

- One fake pane printing without pause and one `mkrew-fakeagent --tsp` pane playing a fixture the test writes, 2,000 transcript rows.
- It reports the UI's CPU; the before and after numbers go into ADR 0009 (non-functional measurements).
- No real agent and no network.

Frame time:

- The throwaway frame timer on `perf/probe` (never merged) measures before and after with the section 1 scenario.
- The table goes into ADR 0114.

End to end in the smoke sandbox (`scripts/smoke.ps1`), screenshots in dark and light, nothing may look different:

- The transcript follows the end while it is there and stops when the user scrolls up.
- An omp `reveal`, PageUp, PageDown, Home and End, the "Scroll to the latest message" button.
- A drag selection and Ctrl+C in an omp pane.
- Voice input types into a field of a pane that reused its frame.
- A pane's menu opens at its anchor.
- A detached window shows and updates its panes.
- A web pane stays in place while the window resizes, a divider is dragged and a pane is zoomed.

`docs/HUMAN_CHECKS.md`: the user types in a long omp session and in a pane next to busy agents on their own machine.

## 8. Risks

- A pane whose state changes without a `Shell` notify shows the old state.
  Approach A keeps this as rare as today: every `Shell` notify still renders every pane.
- `gpui::ListState` needs to hear about every height change of an off-screen row (section 4.3); a missed one puts the scroll position off until that row is measured again.
- The `fix/omp-composer-input` branch changes the same file; this work waits for it (see the header).
