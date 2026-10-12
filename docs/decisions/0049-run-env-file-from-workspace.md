# 0049 — A run command's env file is relative to the workspace, not to its folder

- Date: 2026-10-12
- Status: accepted

## Context

ADR 0018/0048 resolved `env_file` from the command's folder and kept the picker inside it. A
copy of a command pointed at a git worktree of the same repo (`.worktrees/x/EA-BE`) could not
use the main checkout's `EA-BE/.env`: the worktree has none, and `..` is refused.

## Decision

- `env_file` is relative to the workspace folder; `dir` only sets the shell's working folder.
- Migration 0018 rewrites saved rows to `<dir>/<env_file>` (unchanged when `dir` is `.`).
- The Env file picker starts in the card's folder and may go up to the workspace root.
- The command is untouched: it is typed into the shell in `dir`, so an absolute path or a
  `..\` path already reaches a `.venv` elsewhere.

## Alternatives rejected

- Allow `..` and absolute env file paths, still relative to `dir`: `..\..\..\EA-BE\.env` breaks
  as soon as the folder changes; the coupling stays.
- A `MKREW_WORKSPACE` variable for commands: shell-specific syntax (`$env:` / `%…%` / `$`);
  left for when asked.

## Consequences

The protocol shape is unchanged but `RunCommand.env_file` changed meaning, so `PROTOCOL_VERSION`
goes 38 → 39: a new UI on an old daemon gets `version_mismatch` (restart the daemon) instead of
writing `EA-BE/.env` into a database migration 0018 has not rewritten. Saved setups show
`EA-BE/.env` where they showed `.env`.
