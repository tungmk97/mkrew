# 0118 - Per-session pane redraws and omp 19 node kinds

- Date: 2026-11-25
- Status: accepted (amends ADR 0107 and ADR 0114; SPEC §11, §14)
- Spec: `docs/superpowers/specs/2026-11-25-idle-perf-design.md`

## Context

The installed 0.10.1 UI used 42% of one core over 10 s (25% on average over 26 minutes) with 14 live omp sessions in per-workspace windows and nobody typing; ADR 0114 had measured 6-7%.
In 30 s the daemon sent 567 `tsp.update` events, 404 of them (10.9 MB of JSON) from one working omp.

Three causes:

- Every `tsp.update` for a pane on screen notified `Shell`.
  Every `PaneView`, Space window, detached window and the sidebar observe `Shell`, so one frame of one omp rendered every pane of every window (the `ponytail:` note of ADR 0114 named this as the next step).
- omp keeps its todo reminder as a hidden `checklist` with an active item.
  `tsp::animating` counted it although the pane draws nothing for a hidden node, so the status tick notified `Shell` twice a second for as long as such a todo existed.
- `TspUi::sync_rows` cloned every row id of `main` on every frame, also for frames that only touch the dock.

omp 19.1.1 (installed the same day) sends `button` (the ask card's pills) and `menu` (the jobs panel's source picker) whatever the hello lists.
`Document::check` rejected a whole `add` subtree for one unknown kind, so the ask card and the jobs split never reached the mirror and every later op naming them failed: 1,478 WARN lines that day, none on the days before, one `error` event to omp per frame and missing content in the pane.

## Decision

- The TSP document accepts any node kind; `doc` is a text kind (omp 19's `TSP_TEXT_KINDS`).
  The UI already draws a kind it does not know generically, so a new omp kind degrades to its label and children instead of dropping a subtree.
  The hello's kind list does not change: omp keeps its fallbacks for kinds Mkrew does not advertise.
- A `button` click sends `action` with `actions.click`, else `click`, and nothing while it is `disabled` or `busy` (omp 19 `TspButtonProps`).
- The engine still sends one `error` event per failing frame, but logs only the first failing frame of each surface as a warning; later ones are logged at `debug`.
- `Shell::notify_session_panes` renders the `PaneView`s that show one session, in any window; GPUI renders their window root with them and every other pane and window reuses its last frame.
  `tsp.update` and the status tick's omp and Chat animations use it; the peek popover shows a notice for an omp session, never its surface, so it needs no notify.
- `tsp::animating` skips hidden nodes and their subtrees.
- `sync_rows` returns at once for a frame of the shown surface that named no row and left `main`'s children as they were (`tsp::rows_unchanged`), and for a frame of a hidden surface, without collecting the row ids.

## Measurements

`crates/mkrew-ui/tests/measure.rs` `ui_cpu_with_six_omp_panes_two_working`: six omp panes in one tab (fake omp, 300-row transcripts, a hidden active todo in each dock), one sending a dock frame every 75 ms and two every 330 ms (`mkrew-fakeagent --tsp-tick-ms`), release builds, UI CPU over 10 s.

| Build | UI CPU (one 10 s sample) |
|---|---|
| `master` at `c775413f` (before) | 17.6% of a core (over the 15% budget) |
| `fix/idle-perf` (after) | 6.6% of a core |

The installed app had 14 omp panes across per-workspace windows, so each `Shell` notify fanned out further than in this one-window scenario.

## Consequences

- An omp frame or a pane's animation costs one pane and its window root, whatever the number of panes and windows.
- A change that only a pane draws but that reaches the UI some other way still needs a `Shell` notify (or `notify_session_panes`); code that adds such state must pick one.
- A `button`, `menu`, `buttons`, `doc`, `json`, `shell` or `web` node draws generically (label, then children) until it gets a design; omp 19's ask card roles (`omp.ask.card*`, `omp.ask.fold*`) also draw generically, and the dock's `omp.editor.ask*` branches no longer match omp 19.
- The PTY output of an omp pane on its surface is still fed to its hidden terminal; an idle omp in native mode writes nothing there (measured: session logs did not grow in 20 s).

## Not done

- Dropping frames whose ops leave the document unchanged in the daemon (omp re-sends identical `set` and `add` ops while it works).
- The storage findings of the same audit: `turn.files` without a size cap (54 MB of a 91 MB database, mostly old `.worktrees/` paths), seven daily full backups, session logs without a total cap.
