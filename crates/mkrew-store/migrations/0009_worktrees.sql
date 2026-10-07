-- M13 (SPEC §20E): task worktrees and fan-out.
ALTER TABLE task ADD COLUMN worktree INTEGER NOT NULL DEFAULT 0;
ALTER TABLE task ADD COLUMN worktree_path TEXT;
ALTER TABLE task ADD COLUMN branch TEXT;
ALTER TABLE task ADD COLUMN fanout_of TEXT REFERENCES task(id);
