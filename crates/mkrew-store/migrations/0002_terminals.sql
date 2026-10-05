-- SPEC §5, M2. Never edit: schema changes go in a new numbered migration.
CREATE TABLE terminal_tab (
  id         TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES project(id),
  title      TEXT NOT NULL,
  position   REAL NOT NULL,
  layout     TEXT NOT NULL               -- JSON split tree, leaves are session ids
);
