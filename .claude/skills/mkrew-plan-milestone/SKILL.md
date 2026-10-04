---
name: mkrew-plan-milestone
description: Break the current Mkrew milestone from docs/SPEC.md §18 into a queue of small, ordered, independently verifiable tasks in docs/PROGRESS.md. Use when the current milestone has no task queue, when the user asks to "plan M3" or "replan", or when the build loop reaches a milestone with an empty queue.
---

# Plan a milestone

## Inputs

- `docs/PROGRESS.md` (current milestone, notes)
- `docs/SPEC.md` §18 for the milestone's acceptance criteria, plus the sections they reference
- Current code layout (`ls crates/`, grep for existing modules) — plan from what exists

## Rules for good tasks

- Each task fits ONE session: roughly ≤ 400 changed lines, one coherent concern, ends green.
- Order by dependency; foundations (types, protocol, store) before features before UI.
- Every acceptance criterion maps to at least one task; say which in the task text.
- Each task line names the SPEC sections to read: `(spec: §7, §8)`.
- Prefer tasks that are verifiable automatically (unit, integration with `mkrew-fakeagent`,
  snapshot). For criteria that need a human, add a final task "add manual checks to HUMAN_CHECKS".
- 5–12 tasks per milestone. More than 12 means tasks are too small; fewer than 5 too big.
- IDs: `T<milestone number>.<n>` (e.g. `T3.4`). Splits later use letters (`T3.4a`).

## Output

Replace the milestone section of `docs/PROGRESS.md` with:

```
## Current milestone: Mx — <title> (SPEC §18)

### Acceptance criteria
- [ ] <copied from SPEC §18, verbatim>

### Task queue
- [ ] Tx.1 <imperative summary>; covers: <criterion keywords>. (spec: §a, §b)
...

### Done (last 10)
```

Commit: `chore(progress): plan Mx`. Then stop — implementation starts in the next session.
