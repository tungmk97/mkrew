-- SPEC §5, §12B, ADR 0065. Never edit: schema changes go in a new numbered migration.
CREATE TABLE viewed_file (
  project_id       TEXT NOT NULL REFERENCES project(id),
  worktree_task_id TEXT NOT NULL DEFAULT '', -- '' = the project folder, else the task whose worktree is shown
  repo             TEXT NOT NULL DEFAULT '', -- '' = the workspace folder's own repository, else a child repository folder
  list             TEXT NOT NULL,            -- 'working' | 'base' | 'since:<checkpoint>'
  path             TEXT NOT NULL,            -- relative to the repository
  hash             TEXT NOT NULL,            -- FNV-1a 64 of the file's diff text when it was marked
  marked_at        INTEGER NOT NULL,         -- Unix ms
  PRIMARY KEY (project_id, worktree_task_id, repo, list, path, hash)
);
