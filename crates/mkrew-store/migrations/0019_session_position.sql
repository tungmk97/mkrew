-- SPEC §5, §7 `session.move`. Never edit: schema changes go in a new numbered migration.
ALTER TABLE session ADD COLUMN position REAL NOT NULL DEFAULT 0; -- sidebar order inside the workspace group, ascending; 0 = never moved (session.move)
-- The run tab is titled `Run` now (mkrew_core::run::TAB_TITLE).
UPDATE terminal_tab SET title = 'Run' WHERE title = 'run';
