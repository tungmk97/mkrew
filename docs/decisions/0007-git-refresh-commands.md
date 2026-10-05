# 0007 — A Git view refresh runs status, base lookup and line counts in parallel

- Date: 2026-10-06
- Status: accepted
- Milestone: M7

## Context

SPEC §12B said one refresh = ONE `git status --porcelain=v2 --branch -z` call (plus a cached
`merge-base` in Against-base mode). But the same refresh must also return the +/− counts of
the changed-file list and the base shown in the header (`origin/HEAD` → `main` → `master`,
or the configured ref). `git status` reports neither line counts nor other refs, so a single
call cannot fill the `git.status` result the spec asks for (SPEC §7).

## Options

1. Status only; counts `None` and base resolved only in Against-base mode. Pros: one process.
   Cons: no +/− counts in the list, no base in the header in Working mode.
2. Status + `git for-each-ref` / `git rev-parse` (base) + `git diff [--cached] --raw
   --numstat -z` (counts), run in parallel threads. Pros: full result; latency ≈ the slowest
   call, not the sum. Cons: up to 4 processes per refresh.
3. Compute counts in the UI from per-file diffs. Cons: one diff per file, far worse.

## Decision

Option 2. Working mode runs status ∥ base lookup ∥ staged numstat ∥ unstaged numstat;
Against-base mode runs status ∥ base lookup, then the cached `merge-base`, then one
`git diff --raw --numstat -z <merge-base>`. All Git view reads set `GIT_OPTIONAL_LOCKS=0` so
they never take the index lock from the user's own git commands.

## Consequences

- SPEC §12B "One refresh" bullet updated.
- §15 (status < 150 ms on 10,000 files) is measured on the whole `git.status` (T7.5 perf test).
- Untracked files have no counts (git has none for them); base mode lists them too, since
  they are part of the working tree.
