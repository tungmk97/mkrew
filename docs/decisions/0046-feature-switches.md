# 0046 — Feature switches, Docs and Turns off by default

- Date: 2026-10-11
- Status: accepted
- Milestone: none (outside the milestone loop; user request: on/off switches for the Docs
  module, the session panel and the agent's turns, all off by default)

## Context

- There was no on/off switch for any module. `settings.toml` (SPEC §20.0) is the place for user
  settings; the daemon and the UI both re-read it live.
- Docs (SPEC §12) is a whole view; its daemon side only hashes changed `.md` files of watched
  projects, and `doc.list` / `doc.read` run only when the view is shown.
- Turns (SPEC §20C.1–§20C.2) cost git work on every turn: a start and an end snapshot (`git add`
  into a temp index + `write-tree`) and a line count between them, whether or not the "Turns…"
  dialog is ever opened.
- The session panel (SPEC §10) is closed at every start and is not rendered while closed: no cost
  when unused.

## Options

1. Hide the UI entry points only. Turn snapshots would still run on every turn.
2. A `[features]` table: `docs` gates the UI; `turns` gates the UI and the daemon's snapshots.
   (Chosen.)
3. Cargo features. Needs a rebuild to switch; the user wants a config switch.

## Decision

- `[features] docs = false`, `turns = false` (defaults). The session panel gets no switch.
- `docs` off: the Docs rail tab, `View: Docs` (palette and key) and "Review in Docs" are gone; a
  workspace left on Docs shows Terminals. Applies live.
- `turns` off: the pane's "Turns…" button and `Agent: Turns…` are gone. The daemon reads the
  switch when a session starts: off records turn rows (times, files) without snapshots, like a
  folder outside git, so no line counts and no restore for that session.

## Consequences

- SPEC §20.0 lists `[features]`; §12 and §20C note the switch.
- Turning `turns` on applies to sessions started afterwards; earlier sessions keep no snapshots.
- The `done · 3 files · 4m` summary keeps working with `turns` off (files come from the watcher).
