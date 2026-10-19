-- SPEC §5, §20I, ADR 0066. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN jira_keys TEXT; -- JSON array of Jira project keys, NULL = none
