-- SPEC §5, M6. Never edit: schema changes go in a new numbered migration.
CREATE TABLE doc_comment (
  id          TEXT PRIMARY KEY,             -- 'c_' + ULID
  project_id  TEXT NOT NULL REFERENCES project(id),
  file_path   TEXT NOT NULL,                -- relative to project root, '/' separators
  parent_id   TEXT REFERENCES doc_comment(id), -- NULL = thread root, else a reply
  author      TEXT NOT NULL,                -- 'user' or agent profile id
  body        TEXT NOT NULL,
  start_line  INTEGER,
  end_line    INTEGER,
  quote       TEXT,
  status      TEXT NOT NULL,                -- open|sent|addressed|resolved|outdated
  sent_to     TEXT REFERENCES session(id),
  sent_snapshot TEXT,                       -- file content when sent, for diff
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
);
CREATE INDEX doc_comment_file ON doc_comment(project_id, file_path);
