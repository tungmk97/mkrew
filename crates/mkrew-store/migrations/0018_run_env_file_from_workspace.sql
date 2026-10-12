-- ADR 0049: a run command's env file is relative to the workspace folder, no longer to its
-- folder. Never edit: schema changes go in a new numbered migration.
UPDATE run_command
   SET env_file = rtrim(dir, '/\') || '/' || env_file
 WHERE env_file IS NOT NULL
   AND rtrim(dir, '/\') NOT IN ('', '.');
