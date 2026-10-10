# 0039 — Plan checklist from a linked Markdown file

- Date: 2026-10-10
- Status: accepted
- Milestone: outside the milestone loop (inbox 2026-10-10, item 8)

## Context

The redesign (B4) wants a session's plan progress in the right panel and on the selected
sidebar row: `Plan <name> 6/20`, a bar, the checklist. The first idea, a "Setup / Tasks /
Finish" phase bar, needs each agent's output parsed: per-agent code, which SPEC §6 forbids
(agents differ only in `agents/*.toml`), and it breaks with every agent release. Agents that
plan do write a Markdown checklist (`- [ ]` / `- [x]`) somewhere, in the project or outside.
The daemon already reads Markdown (`docs::read`, 2 MiB cap) and has a one-file watcher shape
(`settings.rs`).

## Options

1. Phase bar parsed from agent output — per-agent, fragile. Rejected.
2. Reuse the project watcher (`watch.rs`) and read plans from its batches — no new watcher,
   but it only lives while the project has a live session, the plan may lie outside the
   project, and every unrelated change of a big tree would be filtered per session.
3. One watcher per linked live session on that file only, parse in the daemon — one tiny
   watcher each, works for any path, the UI never parses Markdown.
4. Parse in the UI after `fs.read` — a round trip per change and a second parser; the file
   may be outside the project (`fs.read` is project-relative).

## Decision

Option 3. The user links a `.md` file to a live session (`session.link_plan`; stored as
`session.plan_path`, relative to the project folder when inside, else absolute; kept by
resume). The daemon watches the file's directory non-recursively with a name filter
(editors save through rename), debounces 300 ms, re-reads the file, scans it line by line for
task-list items outside code fences (`mkrew_core::plan::parse`) and announces `session.plan`
only when the checklist changed. The event carries exact `done`/`total` and at most 200 items
of ≤ 120 characters. A file that cannot be read keeps the link and reports an `error`.
Exited sessions are neither watched nor shown (ADR 0033). No phase bar.

## Consequences

SPEC §5 (`session.plan_path`, migration `0017`), §7 (`session.link_plan`,
`session.plan` request and event, protocol 37), §10 (Plan card, sidebar bar; Task 18).
Ceiling: N sessions linked to the same file run N small watchers (one directory handle
each); share them per path if that ever matters. Nested items count like top-level ones;
ordered lists (`1. [ ]`) are not counted. A linked file edited by an agent updates the counts
within about a second without any polling.
