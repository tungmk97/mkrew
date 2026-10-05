-- SPEC §5, M3 (ADR 0003). Never edit: schema changes go in a new numbered migration.
ALTER TABLE session ADD COLUMN state_source TEXT NOT NULL DEFAULT 'daemon'; -- hook|screen|daemon
ALTER TABLE session ADD COLUMN state_changed_at INTEGER NOT NULL DEFAULT 0;
UPDATE session SET state_changed_at = COALESCE(ended_at, started_at);
CREATE INDEX session_event_session ON session_event(session_id, id);
