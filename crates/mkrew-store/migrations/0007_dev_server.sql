-- SPEC §5, M8. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN dev_autostart INTEGER NOT NULL DEFAULT 0; -- start dev_command after the daemon starts
