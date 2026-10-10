-- SPEC §5, ADR 0037. Never edit: schema changes go in a new numbered migration.
ALTER TABLE session ADD COLUMN color TEXT; -- a WORKSPACE_COLORS name, NULL = none
