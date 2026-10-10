-- SPEC §5, ADR 0039. Never edit: schema changes go in a new numbered migration.
ALTER TABLE session ADD COLUMN plan_path TEXT; -- linked plan .md: project-relative with '/', or absolute; NULL = none (session.link_plan)
