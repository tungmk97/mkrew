# Idle UI cost and omp 19 surfaces - Design

- Date: 2026-11-25
- Status: approved by the user (2026-11-25: "làm plan để fix cả 1 và 2", then full permission to implement and test).
- Branch and worktree: `fix/idle-perf` in `.worktrees/idle-perf`, from `master` at `c775413f`.
- ADR: `docs/decisions/0118-idle-perf-and-omp-19-kinds.md` (0117 is taken by the Tern polish spec).
- Protocol: no change (63). Migration: none.
- Builds on ADR 0114 (cached `PaneView`, virtual omp transcript) and ADR 0107 (omp native surface).

## 1. Problem

Measured on the installed 0.10.1 with 14 live omp sessions, `window_mode = "per_workspace"`, nobody typing:

- The UI process used 42% of one core over 10 s (25% on average over 26 minutes); ADR 0114 measured 6-7% after its change.
- In 30 s the daemon sent 567 `tsp.update` events: one working session 404 (13.5 a second, 10.9 MB of JSON), two others about 80 each.
- Each `tsp.update` for a pane on screen ends in `cx.notify()` on `Shell` (`shell/connection.rs`, `Event::TspUpdate` arm).
  Every `PaneView`, every `SpaceWindow`, every detached window and the sidebar observe `Shell`, so one frame of one omp renders every pane in every window (the `ponytail:` note on `PaneView` names this as the next step).
- The 500 ms status tick also notifies `Shell` while `tsp::animating` finds a live node in any shown omp document.
  omp keeps its todo reminder as a hidden `checklist` (`"hidden": true`, `role: omp.notice.todo`) with an `active` item; the pane does not draw hidden nodes, but `animating` still counts it, so the whole app renders twice a second for as long as such a todo exists.
- `TspUi::sync_rows` clones the ids of every row of `main` on every frame, even for frames that only touch the dock or the hud.

Separately, omp 19.1.1 (installed today) sends node kinds Mkrew does not know:

- omp 19 has 50 kinds (`packages/wire/src/tsp.ts:42-93`); new: `web json button buttons menu doc shell`.
- It sends `button` (ask card pills, `omp.ask.pill`) and `menu` (jobs panel sources, `omp.jobs.sources`) without checking the hello's `kinds`.
- `Document::check` rejects a whole `add` subtree when one node has an unknown kind, so the ask card (`fh`, `1o`) and the jobs split (`di.split/...`) never enter the mirror.
  Every later op that names those ids or places a row `before` them fails, forever: 1,478 WARN lines in today's daemon log (0 on the days before), one `error` event to omp per frame, and missing content in the pane.

## 2. Goal and success criteria

- UI CPU with the same live sessions, idle, under 15% of one core (SPEC §15 target from ADR 0114).
- An omp frame renders only the panes that draw that session (and the window root that holds them), not every window and pane.
- A hidden node never keeps the status tick redrawing.
- omp 19 ask cards and the jobs panel reach the mirror and draw (generically for the new kinds); no `unknown kind` or follow-on `unknown id` / `is not a child of` storm in the daemon log.
- Nothing else looks or behaves differently.

## 3. What the user said, and what was assumed

- Said: research performance; fix (1) the omp 19 TSP desync and (2) the redraw fan-out; implement and test without asking; a worktree is fine.
- Assumed (recorded in HUMAN_CHECKS): no new drawing for `button` and `menu` beyond the generic one (label plus click); the ask card's renamed roles (`omp.ask.card*`) draw with the generic layout until a design pass; the PTY output of TSP panes is left alone (idle omp writes 0 bytes to the PTY in native mode, measured: session logs did not grow in 20 s).

## 4. Design

### 4.1 Unknown kinds degrade (core `tsp::doc`)

- `Document::check` no longer validates the kind; the `KINDS` list goes.
  An unknown kind enters the tree like any node; the UI already draws a kind it does not know with `tsp_generic` (its text-like prop, then its children).
  omp's own wire format ignores unknown fields and verbs in both directions; one new kind must not drop a subtree.
- `doc` joins `TEXT_KINDS` (omp 19's `TSP_TEXT_KINDS`), so `text` and `splice` ops on it apply.
- The hello's `P1_KINDS` does not change: omp keeps using its fallbacks for kinds Mkrew does not advertise.

### 4.2 One warning per surface (core `tsp::engine`)

- `Engine::frame` still sends omp one `error` event per frame with failing ops (omp's contract).
- It logs `Step::Warn` only for the first such frame of a surface; later ones are `Step::Debug` (logged at `debug`).
  A surface opened again starts a new document and may warn again.

### 4.3 `button` clicks (UI `tsp`)

- omp sends `act: "click"` for a `button` without `actions.click` (`tsp.ts:637-656`); `click_event` and `has_click` do the same for a `button` that is not `disabled` or `busy`.

### 4.4 Hidden nodes do not animate (UI `tsp::animating`)

- The walk skips a node whose `hidden` prop is `true`, with its subtree, exactly as `tsp_node` draws nothing for it.

### 4.5 Notify the panes of a session, not `Shell` (UI shell)

- New `Shell::notify_session_panes(session, cx)`: every `PaneView` in `pane_views` whose slot shows `session` renders again (in any window); the window root renders with it (GPUI marks ancestors dirty), other panes and other windows reuse their last frame.
- `Event::TspUpdate`: when `event_shown` says the session is on a pane, notify that session's panes instead of `Shell`.
  The peek popover never draws an omp surface (it shows "omp draws this session natively"), so it needs no notify.
- Status tick: `tsp_animating` and `chat_animating` return the sessions that animate; the tick notifies their panes.
  It notifies `Shell` only for what other surfaces draw (Board, starting pane, voice, Jira, time labels), as today.
- Everything else that notifies `Shell` stays as it is (layout, theme, focus, dialogs, sidebar state).

### 4.6 Frames that leave `main` alone (UI `TspUi::sync_rows`)

- Pure `tsp::rows_unchanged(doc, known, dirty)`: `dirty` is empty and `main`'s children are exactly `known`, compared without allocating.
- `sync_rows` returns at once for a frame of the shown surface when it holds; nothing to splice, remeasure or regroup.

## 5. Non-goals

- A drawn `button`/`menu`/`buttons` look, `doc`/`json`/`shell`/`web` builders, and the omp 19 ask card restyle (separate design work; generic drawing until then).
- Daemon-side dropping of no-op frames (omp re-sends identical `set`/`add` ops while working): the cost measured is the UI's fan-out, not the JSON.
- Storage findings of the same audit (turn files, backups, session logs).

## 6. Testing

- Core: an `add` with an unknown kind applies and later ops on its ids work; `text` on a `doc` node applies; two failing frames give two `error` replies and one `Warn`.
- UI pure: a hidden checklist with an active item and a spinner inside a hidden `col` do not animate, a visible one does; `rows_unchanged` true only for the same ids with nothing dirty; a `button` without actions clicks `click`, a disabled or busy one does not.
- Shell wiring cannot be unit-tested headless: a CPU measurement of the installed build against the branch build with the same live sessions, plus HUMAN_CHECKS lines.
