-- SPEC §5, ADR 0020. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN color TEXT; -- a WORKSPACE_COLORS name, NULL = none
