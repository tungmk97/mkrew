-- SPEC §5, ADR 0019. Never edit: schema changes go in a new numbered migration.
ALTER TABLE session ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0; -- removed from the sidebar (session.remove); history kept
