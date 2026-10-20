-- SPEC §5, §7 `project.update` `position`. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN position INTEGER NOT NULL DEFAULT 0; -- sidebar order inside the pinned or unpinned group, ascending; 0 = never moved (then by age)
