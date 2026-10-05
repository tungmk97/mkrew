-- SPEC §5, M1. Never edit: schema changes go in a new numbered migration.
CREATE TABLE project (
  id          TEXT PRIMARY KEY,
  name        TEXT NOT NULL,
  path        TEXT NOT NULL UNIQUE,      -- absolute, normalized
  dev_command TEXT,
  default_agent TEXT,                    -- profile id used for new tasks/panes
  review_globs TEXT,                     -- JSON array, NULL = defaults (§12)
  created_at  INTEGER NOT NULL,
  archived    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE task (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES project(id),
  title         TEXT NOT NULL,
  prompt        TEXT NOT NULL DEFAULT '',
  agent_profile TEXT NOT NULL,
  status        TEXT NOT NULL,           -- todo|running|needs_input|review|done
  position      REAL NOT NULL,
  checkpoint    TEXT,                    -- git HEAD before run (M5)
  error_flag    INTEGER NOT NULL DEFAULT 0,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL
);

CREATE TABLE session (
  id                TEXT PRIMARY KEY,
  task_id           TEXT REFERENCES task(id),     -- NULL = free terminal
  project_id        TEXT NOT NULL REFERENCES project(id),
  agent_profile     TEXT NOT NULL,
  kind              TEXT NOT NULL DEFAULT 'agent', -- agent|shell
  title             TEXT,
  native_session_id TEXT,
  state             TEXT NOT NULL,       -- starting|working|needs_input|idle|done|exited|unknown
  exit_code         INTEGER,
  cols              INTEGER NOT NULL,
  rows              INTEGER NOT NULL,
  started_at        INTEGER NOT NULL,
  ended_at          INTEGER
);

CREATE TABLE session_event (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id TEXT NOT NULL REFERENCES session(id),
  kind       TEXT NOT NULL,              -- state_changed|hook|files_changed|...
  payload    TEXT NOT NULL,              -- JSON
  at         INTEGER NOT NULL
);
