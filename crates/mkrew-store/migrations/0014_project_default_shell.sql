-- SPEC §5, ADR 0032. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN default_shell TEXT; -- shell profile id for new shell panes, NULL = ask
