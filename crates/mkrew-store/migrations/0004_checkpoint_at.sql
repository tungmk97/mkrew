-- SPEC §5, M5 (ADR 0005). Never edit: schema changes go in a new numbered migration.
ALTER TABLE task ADD COLUMN checkpoint_at INTEGER; -- Unix ms the checkpoint was taken (undo: untracked files newer than this were created by the run)
