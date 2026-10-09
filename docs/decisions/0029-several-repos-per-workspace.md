# 0029 — Several git repositories in one workspace

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

The inbox asks that a workspace folder holding a front end and a back end, each its own git
repository (`fe/.git`, `be/.git`) with no repository at the root, works in the Git view. Every
git request resolved one folder (`Daemon::target`) and git finds repositories upward only, so
such a root showed "Cannot read the git status", no Files markers, and a commit made in `fe/`
from a terminal never refreshed anything (the watcher only knew the root's `.git`).

## Options

1. Add `fe/` and `be/` as two workspaces — works today, but two sidebar entries and agents
   cannot span both folders; the inbox asks for one root.
2. A `repo` parameter on git requests plus discovery of the direct child repositories, and a
   repository chooser in the Git view — small; every git function already takes a folder.
3. A workspace setting listing repositories (migration, CLI, settings UI) — replaces a cheap
   directory scan and goes stale when folders change.
4. All repositories stacked in one Git view — commit, push, history and base are per
   repository anyway, so most of option 2 is still needed underneath; much larger.

## Decision

Option 2. `git.repos` lists `""` (the project folder, when it is in a repository) and each
direct child folder holding `.git` (folder or file), sorted, skipping hidden folders,
`node_modules` and `target`, not filtered by `.gitignore`, read on every call. `git.status`,
`git.diff`, `git.stage`/`unstage`, `git.discard`, `git.commit`, `git.log`, `git.show`,
`git.fetch`/`push` and `ai.generate` accept `repo?` (one folder name). Paths in requests and
replies stay project-relative (`fe/src/a.ts`): the daemon strips and adds `repo/`, so Open in
Zed, diff comments and "Changed by" work unchanged. The watcher also treats a direct child's
`.git/{HEAD,index,packed-refs,refs/…}` as git state. Files markers merge the child
repositories' status. The Git view shows a "Repo" chooser with two or more repositories and
picks the first one when the project folder is no repository.

## Consequences

- SPEC §7, §12A, §12B, M5, §20C.1 and §20E.1 updated; protocol version bumped to 28.
- Not covered (unchanged behaviour): turn snapshots and turn restore (§20C), task checkpoints
  and Undo (M5), task worktrees, merge back and fan-out (§20E) still work on the project
  folder's repository only and are skipped or refused when it is none; `repo` together with
  `worktree_task_id` is `invalid_request`. `git.set_base` stays per project and applies to the
  shown repository (auto-detection works per repository). Repositories deeper than one level
  (`apps/web`) are not found. A root repository that also has nested ones lists them as
  untracked folders in its own status (no submodule awareness). A child repository's own
  `.gitignore` is not read by the watcher, so churn in its ignored folders still counts as
  file changes.
