# 0114 - Cached pane views and a virtual omp transcript

- Date: 2026-11-25
- Status: accepted (amends SPEC §11 omp native surface and §15; amended by ADR 0118: an omp frame or a pane animation notifies that session's pane views, not `Shell`)
- Spec: `docs/superpowers/specs/2026-11-25-pane-render-perf-design.md`

## Context

The user reported that Mkrew felt choppy, typing included.
The UI thread of 0.9.0 used 46-47% of one core with about 20 live omp sessions.
In a sandbox with one printing pane and one omp pane holding a real 14 MB session, a frame took 52 ms on average (65 ms at most) and the UI used 70% of a core; 40 ms of each frame was layout of the whole transcript.
Those numbers come from the first diagnosis build, which timed every element; the table below compares two builds with only the frame timer, so its "before" is lower.

Two causes:

- GPUI marks every ancestor of a notifying view dirty, and nothing below `Shell` was cached, so every output frame, omp frame, status tick and key press laid out every pane again.
- The omp surface built every row of `main` inside one scrolling div.

## Decision

- Each pane is a `PaneView` (`shell/pane_view.rs`) embedded `cached`, keyed by window, workspace, tab and leaf.
  Like `SidebarView` it observes `Shell` and renders again on every `Shell` notify, on a window refresh, on new bounds and on a scroll inside it.
- The terminal grid is embedded `cached`: output renders that pane (its frame and grid) and the window root; the other panes reuse their paint.
- The omp transcript is a `gpui::list`.
  `Mirror::apply` records the rows each frame's ops name (before and after the ops run), `tsp::row_changes` turns them into splices and remeasures, and the list lays out only the rows on screen plus 600 px around them.
  It follows the end with `FollowMode::Tail`.
- State recorded while painting survives a reused frame: pane menu anchors and the voice typing target are cleared by the pane view that records them; text hit areas are cleared at every omp pane render.
- A browser pane's page is built from a task that borrows nothing, not during the render (`browser::Parent`).
  Building a WebView2 page runs the message loop, and that loop runs GPUI's other tasks; during a render they cannot update the window, and the daemon connection's loop stopped for good on that error (the UI froze: no output, no new sessions).
  The faster frames of this change made that race common; it was there before.

## Measurements

| Scenario | Before | After |
|---|---|---|
| Frame time, printing pane + omp pane with a 14 MB session (mean / max) | 34.9 / 62.3 ms | 1.8 / 5.3 ms |
| UI CPU, same scenario | 45-49% of a core | 6.6-6.9% of a core |
| UI CPU, `measure.rs` 2,000-row transcript beside a printing pane | 101.2% of a core | 5.0% of a core |

## Consequences

- Typing in a long omp session or next to busy agents no longer waits for a whole-transcript layout.
- A state change that does not notify `Shell` (or the `TerminalPane` it belongs to) is not drawn until something else renders that pane; code that changes pane state must notify, as before.
- `ponytail:` on `PaneView`: every `Shell` notify still renders every pane view; notifying only the pane of the session that changed is the next step if a profile shows `Shell` notifies dominate.
- The Chat list is not virtualized (it holds at most 50 turns).
- A browser page shows one frame later than before (it is built after the render that first draws its pane).
