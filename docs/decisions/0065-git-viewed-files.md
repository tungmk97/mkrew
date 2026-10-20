# 0065 — Git view: mark files as viewed

- Date: 2026-10-19
- Status: accepted (extends SPEC §12B, §13, §7, §5)
- Milestone: none (competitor study: GitHub / Conductor "viewed")

## Context

Reviewing a large agent diff file by file loses track of what was read. GitHub and Conductor
let the reviewer tick a file as viewed; the tick must not outlive a new change to the file.
The Git view lists files from `git.status` and computes each diff on demand (SPEC §12B), so the
UI cannot tell by itself that a file's diff changed. The remembered diff mode lives in
`settings.toml` `[ui]`; that file is a hand-edited config written one scalar at a time, every
write reloads settings in the UI and the daemon, and marks are an unbounded per-workspace list.

## Decision

- A mark is a row `viewed_file(project_id, worktree_task_id, repo, list, path, hash, marked_at)`
  in the daemon's SQLite (migration 0022). `list` = `working` (Staged, Unstaged, Untracked),
  `base`, or `since:<checkpoint>`; `worktree_task_id` and `repo` are `''` for the project
  folder / the workspace's own repository. `hash` = FNV-1a 64 of the text `git diff` prints for
  the file when it was marked (binary changes count: the text holds the blob ids).
- `git.set_viewed { <git.diff params, commit must be none>, viewed }` hashes the diff as it is
  now and stores or removes the mark (`invalid_state` when the file has no diff).
- `git.viewed` (the params of `git.status`) re-hashes the marked files of that list through the
  diff cache and returns `ViewedFile { path, staged }` for each mark that still stands. Marks
  whose hash no longer matches (the diff changed, or the file no longer changes) are deleted
  there. A `working` mark stands on the staged or the unstaged side, whichever hashes the same:
  staging a whole viewed file keeps it viewed. Protocol 46.
- UI: a box on each file row, `v` in the Git view (SPEC §13), a toolbar button, a palette
  command; viewed rows dim, the selected viewed file's diff collapses ("Show diff" opens it),
  the header shows `Viewed N/M`. Not in the History.

## Alternatives

- `settings.toml` `[ui]`, like `diff_mode`: see Context. Rejected.
- The UI hashing diffs it already holds: only the selected file's diff is loaded, so a mark
  could not clear until the file is opened. Rejected.
- Keying marks by the file's mtime / size or `added` / `removed` counts: an edit that keeps
  the counts would keep the mark. Rejected for the diff text.
- No list in the key (a mark valid in any list whose diff hashes the same): pruning in
  Working changes would delete marks made in Against base. Rejected.

## Consequences

- The daemon hashes the diff as it is when the request arrives: an agent edit between the
  screen and the click marks the newer text.
- Each `git.viewed` costs one `git diff` per marked file whose file or index changed (cached
  by the existing diff key, so none otherwise); a refresh with many marks after a big change
  is slower. For a `since:<tree>` list the working snapshot is taken once per call, not once
  per marked file.
- Marks of finished tasks' `since:` lists stay until `delete_task` or never; a retention pass
  can drop them if the table ever grows.
- Marks are per machine (the daemon's database), not in the repository.
- Migration position risk: the store applies migrations by array position (`user_version`),
  not by file number. Master (6240b22, Jira) already has `0020_run_exit_after` and
  `0021_project_jira_keys`, so after merging master `MIGRATIONS` ends
  `… 0019, 0020_run_exit_after, 0021_project_jira_keys, 0022_viewed_files`, and the protocol is 46
  (Jira is 45). Until master is merged this branch's daemon must not run on the real data
  folder (a wrong `user_version` would be recorded); use a sandbox `MKREW_DATA_DIR`.
