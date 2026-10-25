# 0077 — Jira side: List as a parent tree, Kanban by status

- Date: 2026-10-25
- Status: accepted (SPEC §20.0, §20I)
- Milestone: none (user request 2026-10-25)

## Context

The Board's Jira side grouped the issues by status in one list. The user wants two layouts, as
in Jira: a Kanban board, and a list that shows each issue under its parent (epic > task >
sub-task).

## Decision

- A `List | Kanban` switch in the Jira toolbar; the choice is `[jira] layout` (`list` default,
  `kanban`), one for every workspace, saved at once like `[jira] mine`.
- The search also asks for `parent`. Jira sends the parent's key, summary and type with each
  issue, so no second request is made.
- List: a tree, depth first. A parent outside the results (done, or not the user's under
  `Mine`) shows as a quiet header row with no status. A row with children has a chevron that
  folds them; folds are kept per workspace in memory. A typed filter shows every match with
  its parents and opens folded rows.
- Kanban: one column per status, in the old section order (In Progress, To Do, then others).
  Read only: no drag between columns, so nothing is written to Jira.

## Alternatives

- Drag a card to another column to run a Jira transition: needs a write call, the issue's
  allowed transitions and their errors. Not asked for now.
- A second search for parents outside the results: complete data (status), one more request.
  The fields Jira sends with the child are enough for a header.
- Keep the status sections in List: Kanban now shows the grouping by status.

## Consequences

- The tree knows only one parent level per issue: a sub-task whose task is outside the results
  shows under that task's header, without the epic above it.
