# 0005 — Undo task: what is discarded and how

- Date: 2026-10-06
- Status: accepted
- Milestone: M5

## Context

M5 wants "Undo task": show the diff that would be discarded, then `git restore` / `git clean`
only after explicit confirmation. SPEC §7 has no request for it, and `task.checkpoint` only
records HEAD before the run. Tracked files can be reset to that commit, but git keeps no record
of which untracked files existed before the run, so a plain `git clean` would also delete the
user's own untracked work.

## Options

1. Which untracked files to delete: none / all untracked (loses the user's files) / snapshot the
   untracked list at run time (new table, large in big repos) / those modified at or after the
   checkpoint time (needs only a timestamp).
2. API: one request with a `dry_run` flag / separate `task.undo_preview` + `task.undo{confirm}`.

## Decision

- Migration 0004 adds `task.checkpoint_at` (Unix ms, taken just before HEAD is read in
  `task.run`; `NULL` without a checkpoint). Not part of the protocol `Task`.
- `task.undo_preview {task_id}` → `UndoPreview {checkpoint, fingerprint, changed[], added[],
  created[], diff, diff_truncated}` (`fingerprint` = opaque hash of the changed files and the
  whole, uncut diff): `git diff --name-status <checkpoint> -- .` split into `changed`
  (restored) and `added` (status `A`: tracked now, absent from the checkpoint, so the restore
  deletes them whatever their age), `diff` = `git diff <checkpoint> -- .` cut at 1 MiB,
  `created` = `git ls-files --others --exclude-standard` filtered to mtime ≥ `checkpoint_at`
  (link mtime, not target). All paths relative to the project folder; git runs with
  `-C <project>` so a project inside a bigger repo only touches its own folder. Allowed while
  the task runs.
- `task.undo {task_id, confirm, created[], fingerprint}`: `invalid_request` without
  `confirm: true`, `invalid_state` while a session of the task is live, without a checkpoint,
  or when the recomputed fingerprint differs from the previewed one (tracked changes made
  after the preview are never restored away unseen); then `git
  restore --source=<checkpoint> --staged --worktree -- .` and `git --literal-pathspecs clean
  -f -- <paths>` (chunked) where paths = recomputed created files ∩ the previewed `created`
  the client sends back, so files that appear after the preview was shown survive. Ignored
  files are never deleted. HEAD and commits are not touched (commits made by the agent stay;
  their file changes are reverted in the tree).
- The checkpoint must be a full hex commit id before it is passed to git.
- `mk task undo <task> [--yes]` prints the preview; only `--yes` undoes. `PROTOCOL_VERSION` = 6.

## Consequences

An untracked file that existed before the run but was edited by it counts as created and is
deleted (listed in the preview; HUMAN_CHECKS). Checkpoints from before migration 0004 delete
no untracked files. Only the task's own sessions block undo: another agent working in the
same folder has its edits reverted too, and a `task.run` racing the undo is not locked out
(both listed in HUMAN_CHECKS). SPEC §5, §7 and §9 updated. T5.4 builds the UI dialog on the
preview and must send its `created` and `fingerprint` back.
