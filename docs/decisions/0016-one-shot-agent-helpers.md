# 0016 - One-shot agent helpers (`oneshot` + `ai.generate`)

- Date: 2026-10-07
- Status: accepted
- Milestone: post-M13 fixes

## Context

The user wants a ✦ button that writes a commit message from the current changes (Git view) and
one that improves a prompt in the composer, using the code when needed. SPEC §1: "Mkrew never
calls LLM APIs itself; it only runs CLI agents the user installed." SPEC §2.1: every agent
difference lives in its TOML profile. The daemon serves one connection's requests one at a
time, and the UI uses a single connection, so a 10-60 s run inside a request would block every
later UI request.

## Options

1. A profile `oneshot` command (the agent CLI's non-interactive mode) plus an asynchronous
   request/event pair. No API keys, works with any agent CLI, follows §2.1.
2. Type the request into a running agent's PTY and scrape the reply. Fragile, and it pollutes
   the user's session.
3. Call a provider API directly. Forbidden by §1.

## Decision

Option 1. `ai.generate` checks the profile and the target right away, returns `null`, and runs
the `oneshot` command in the background with a 120 s limit; the answer is broadcast as
`ai.generated`. The prompt goes on stdin (no 32 K command-line limit for diffs, no cmd.exe
escaping). The client chooses `generation_id`, so the event can never arrive before the client
knows the id. The cwd is the project (or worktree) folder, so the agent can read the code for
prompt rewrites.

## Consequences

- Protocol v24 (`ai.generate`, `ai.generated`); SPEC §6 and §7 updated.
- Profiles seeded before this change have no `oneshot` line (seeding never overwrites); ✦
  answers `invalid_state` naming the key and the file to edit.
- The agent could in principle write files, so the shipped commands use read-only modes where
  the CLI has one (`codex exec --sandbox read-only`; `claude -p` denies edits without a
  permission prompt) and the prompts forbid edits.
- Whether `omp -p` reads its prompt from stdin is unverified (HUMAN_CHECKS).
- The answer only fills a text box; nothing is committed or sent automatically (§2.5).
