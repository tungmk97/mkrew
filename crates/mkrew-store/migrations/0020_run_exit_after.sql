-- ADR 0063: a run command types an exit after it only when asked (off by default, so existing
-- commands stay open). Never edit: schema changes go in a new numbered migration.
ALTER TABLE run_command ADD COLUMN exit_after INTEGER NOT NULL DEFAULT 0;
