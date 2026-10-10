-- SPEC §5. Never edit: schema changes go in a new numbered migration.
-- Pinned workspaces, sessions and tabs lead their lists (SPEC §10).
ALTER TABLE project ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE session ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE terminal_tab ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
