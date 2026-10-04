@AGENTS.md

## Claude Code specifics

- `/mkrew-next` runs one iteration of the build loop (same as `docs/BUILD_LOOP.md`).
- A SessionStart hook injects the loop state and next tasks from `docs/PROGRESS.md`
  (also after `/compact`), so you do not need to re-read everything.
- Keep your main context small: delegate crate/API documentation lookups to the `researcher`
  subagent, diff review to `reviewer`, and UI review to `ui-reviewer`.
- If the context is getting long, use the `mkrew-handoff` skill and stop.
