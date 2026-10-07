-- SPEC §5, Run button (ADR 0018). Never edit: schema changes go in a new numbered migration.
CREATE TABLE run_command (
  project_id TEXT NOT NULL REFERENCES project(id),
  position   INTEGER NOT NULL,           -- order in the menu and in the run tab
  name       TEXT NOT NULL,              -- unique per project, e.g. "BE"
  dir        TEXT NOT NULL DEFAULT '.',  -- relative to the project folder
  command    TEXT NOT NULL,              -- typed into the shell, e.g. "npm run dev"
  env_file   TEXT,                       -- relative to dir, e.g. ".env"
  env        TEXT NOT NULL DEFAULT '[]', -- JSON [[key, value], …], applied after env_file
  PRIMARY KEY (project_id, name)
);
INSERT INTO run_command (project_id, position, name, command)
  SELECT id, 0, 'dev', dev_command FROM project WHERE dev_command IS NOT NULL;
ALTER TABLE project DROP COLUMN dev_command;
ALTER TABLE project RENAME COLUMN dev_autostart TO run_autostart;
