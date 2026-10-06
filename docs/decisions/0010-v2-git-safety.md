# 0010 — v2: turn snapshots and task worktrees under principle 5

- Date: 2026-10-06
- Status: accepted
- Milestone: M11, M13

## Context

v2 (SPEC §20) adds per-turn undo (§20C) and opt-in task worktrees (§20E). SPEC §2.5 says Mkrew
never auto-commits, never runs branch-changing git commands on its own and never loses user data.
Both features need git writes that are not a user-triggered stage/commit/push.

## Options

1. Per-turn undo via `git stash create` — touches nothing visible, but skips untracked files and
   stores a stash-shaped commit; restoring is awkward.
2. Per-turn undo via a temporary index (`GIT_INDEX_FILE` + `git add -A` + `git write-tree`) and a
   ref under `refs/mkrew/turns/` — includes untracked (non-ignored) files, leaves the user's index,
   HEAD and branches untouched; only adds objects and a non-branch ref.
3. Copy changed files into `<data>/mkrew/` ourselves — no git needed, but reinvents a content store
   and gets large on big edits.
4. Worktrees: none / always / opt-in per task.

## Decision

Snapshots use option 2: objects + refs outside `refs/heads` are not commits on the user's
branches and change nothing the user sees in `git status`, `git log` or their files. Refs of
sessions ended more than 7 days ago are deleted. Restoring uses a temporary index too and always
goes through a preview + fingerprint + explicit confirmation like `task.undo` (ADR 0005).

Worktrees are opt-in per task (§1 still says "no forced git worktrees"). `git worktree add -b
mkrew/…` creates a new branch but never switches the user's checkout, so it is not a
"branch-changing command" in the §2.5 sense. Merge back and worktree removal are user actions
with confirmation; merge only runs when both sides are clean and aborts on conflict; branches are
deleted with `-d` only, never `-D`.

## Consequences

- SPEC §2.5 names these two exceptions explicitly.
- Screen-detected turn starts (no hook) may snapshot after the agent's first edit; restore then
  misses those edits. Hook-driven agents (Claude Code) wait for the snapshot before the hook
  returns. Documented in §20C.1.
- Snapshot cost on huge repos is bounded by a perf criterion (< 300 ms on 10,000 files, M11);
  if it fails, the fix is to skip snapshots above a size limit, recorded in HUMAN_CHECKS.
- Real-git tests must assert HEAD, current branch, index and untouched files are unchanged.
