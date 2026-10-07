# 0017 — Resume omp, Copilot and agent panes without a task

- Date: 2026-10-07
- Status: accepted
- Milestone: none (outside the milestone loop)

## Context

The user works mostly with Oh My Pi (omp), Claude Code, Codex and GitHub Copilot CLI. They want
herdr-like persistence: after `mk daemon stop` or a reboot, every agent pane comes back with its
conversation. Before this change only task sessions of Claude and Codex resumed. omp and Copilot
reported no session id, and agent panes without a task were never resumed.

## Options

1. Hook agents `omp` and `copilot` in `mkrew_core::hook`, Mkrew-owned integration files
   (an omp extension, a Copilot hooks file), and resume of interrupted free panes that a tab still
   shows — pros: exact session ids from the agent, same path as Claude/Codex; cons: two more
   agents in the core hook mapping, one more migration.
2. Screen-scrape agent session ids — cons: fragile, one regex per agent, breaks on UI changes.
3. `--continue` without an id — cons: resumes the newest conversation of the folder, which is
   wrong with two panes in one project.

## Decision

Option 1. The agents report their own session id through `mk hook`, so resume uses the exact
conversation. A session is marked `interrupted` when it exits while the daemon stops, or when it
is still live at startup after a crash or reboot. At startup, interrupted agent sessions without a
task that are still a leaf of a tab layout are resumed in the same pane, once.

## Consequences

- SPEC §3, §5, §6, §8, §9 updated. Two more hook agents in `mkrew_core::hook`; migration
  `0010_session_interrupted.sql` (store column only, protocol stays v24).
- User profiles written before this change are not updated (seeding never overwrites). They need
  the `resume` line by hand; `mk integration install omp|copilot` prints it.
- Known limit: named omp profiles (`omp --profile x`) load `~/.omp/profiles/x/agent/extensions`,
  which the install does not cover.
- Known limit: a session that ends by itself, or is killed by the user, in the second before or
  while a daemon stop lands can be marked interrupted and come back.
- Manual checks (real agents, reboot) are in HUMAN_CHECKS "2026-10-07 resume omp/Copilot/free panes".
