# 0069 — Drag to reorder workspaces

- Date: 2026-10-20
- Status: accepted (extends ADR 0062; SPEC §5, §7, §10)
- Milestone: none (user request 2026-10-20)

## Context

Workspaces were listed pinned first, then by age (`created_at`). Sessions, run commands and tabs
can already be dragged into a new order; workspaces could not, so the only way to bring one up was
to pin it.

## Decision

- `project.position INTEGER NOT NULL DEFAULT 0` (migration 0023); `Project.position` and
  `project.update` `position?` (protocol 47). Workspaces sort by `(pinned first, position,
  created_at)`, so ones never moved (0) keep their age order.
- A field of `project.update`, not a new `project.move` request: one more optional field next to
  `pinned`, same broadcast (`project.updated`), no new handler. An integer, not a float like
  `Session.position`: the UI always numbers the group `1..=n`, and `Project` stays `Eq`.
- In the full sidebar the whole workspace card is dragged with the shared reorder look
  (`shell/reorder.rs`, `ReorderList::Workspaces(pinned)`): within the pinned or the unpinned
  group only (pin / unpin moves between them, like tabs). A drag that starts on a session or RUN
  row moves that row (the innermost drag source starts first and stops propagation).
- On drop the group is numbered `1..=n`, shown at once and saved with one `project.update` per
  workspace whose position changed. Workspaces on the "N empty workspaces" line keep their index,
  so the hidden ones do not shift.
- Off while filtering, and not in the compact rail.

## Consequences

- An unpinned workspace returns to its dragged place, not its age place.
- A drop saves up to n `project.update`s; fine for a handful of workspaces.
- Not done: dragging in the compact rail or from the "N empty workspaces" line; add if missed.
