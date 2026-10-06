-- M11 (SPEC §20C): persisted agent turns with their git snapshot trees.
CREATE TABLE turn (
  session_id TEXT NOT NULL REFERENCES session(id),
  n          INTEGER NOT NULL,
  started_at INTEGER NOT NULL,
  ended_at   INTEGER,
  start_tree TEXT,            -- NULL outside a git repo
  end_tree   TEXT,
  files      TEXT NOT NULL DEFAULT '[]',  -- JSON [{path, added?, removed?}]
  PRIMARY KEY (session_id, n)
);
