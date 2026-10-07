# Run button — design

- Date: 2026-10-07
- Status: draft, waiting for review
- Replaces: the dev server of ADR 0008 (one `dev_command` per project)

## 1. Goal

One click runs a workspace's local servers (e.g. EA: a Python backend in `EA-BE` and a Vite
frontend in `EA-FE`), the next click restarts or stops them, and the user can see which ports
they listen on. Works for any stack: the command is free text typed into a shell, and mkrew sets
the folder and environment, so no hand-written wrapper script (like EA's `run.ps1`) is needed.

Agreed with the user:

- One **Run** button per workspace that runs every configured command; a menu controls each
  command on its own.
- Each command has: **name, folder, command, env file, env** (option A).
- Setup is asked once (dialog), saved in mkrew's DB per project, then Run/Restart/Stop only.
- Run from stopped shows the Run tab; Restart keeps the current view (option B).
- Ports are shown.

Out of scope: git worktrees (choosing which checkout to run in, parallel runs with different
ports), auto-detecting commands from `package.json` etc., opening a port in the browser,
editing run commands from the CLI.

## 2. Data

Migration `0011_run_commands.sql` (0010 is taken):

```sql
CREATE TABLE run_command (
  project_id TEXT NOT NULL REFERENCES project(id),
  position   INTEGER NOT NULL,          -- order in the menu and in the tab
  name       TEXT NOT NULL,             -- unique per project, e.g. "BE"
  dir        TEXT NOT NULL DEFAULT '.', -- relative to the project folder
  command    TEXT NOT NULL,             -- typed into the shell, e.g. "npm run dev"
  env_file   TEXT,                      -- relative to `dir`, e.g. ".env"
  env        TEXT NOT NULL DEFAULT '[]',-- JSON [[key, value], …], applied after env_file
  PRIMARY KEY (project_id, name)
);
INSERT INTO run_command (project_id, position, name, command)
  SELECT id, 0, 'dev', dev_command FROM project WHERE dev_command IS NOT NULL;
ALTER TABLE project DROP COLUMN dev_command;
ALTER TABLE project RENAME COLUMN dev_autostart TO run_autostart;
```

(`rusqlite` is `bundled`, so SQLite ≥ 3.35 provides `DROP COLUMN`.)

Validation on save (`project.update`, `mkrew_core::run::normalize`): fields trimmed (blank
`dir` = `.`, blank `env_file` = none); 0–8 commands; name 1–20 chars, unique, no `/` or `:`;
command not blank and one line; `dir`/`env_file` relative, with no `..` component; env keys
match `[A-Za-z_][A-Za-z0-9_]*`. Existence of folders/files is checked at run time, not on save
(the folder may be created later).

## 3. Protocol (v25)

```rust
pub struct RunCommand {
    pub name: String,
    pub dir: String,                // "." = project folder
    pub command: String,
    pub env_file: Option<String>,
    pub env: Vec<(String, String)>,
}
```

- `Project`: `dev_command`/`dev_autostart` are removed; it carries `run_commands: Vec<RunCommand>`
  and `run_autostart: bool`.
- `project.update`: `dev_command`/`dev_autostart` are removed; it accepts
  `run_commands?: Vec<RunCommand>` (replaces the whole list) and `run_autostart?: bool`.
- `project.add` loses `dev_command`.
- `project.run_start` (`project_id`, `name?`, `cols`/`rows` default 120×30) → `RunStarted
  { tab_id?, sessions: [{name, session_id, started}] }`. Without `name`: every command not
  already running. A running command is reported with `started: false`. Every command is
  validated before anything spawns (missing folder, unreadable env file → `invalid_request`
  naming the command). With no commands: `invalid_state`; unknown `name`: `not_found`.
- `project.run_stop` (`project_id`, `name?`) → ok. Without `name`: every live run session of
  the project (also one whose name left the config). Ok when nothing runs.
- `project.run_restart` (`project_id`, `name?`, `cols`/`rows`) → `RunStarted`. Stops the
  targets like `run_stop` (the session's own 5 s grace, then forced kill), waits until they
  exited (max 10 s, else `timeout`), then starts them like `run_start`.
- Request `session.ports` (`project_id`) → `[{session_id, ports}]` of the project's live
  sessions with ports; event `session.ports` (`session_id`, `ports`). Ports are kept in daemon
  memory only (the `Session` type is unchanged).
- `project.dev_start`/`project.dev_stop`, `DevServer`, `DevStartParams` are removed (clean
  cutover).

A run session = a live `shell`-kind session titled `run:<name>` (the ADR 0008 rule, one
session per command name). The UI shows `<name>`.

## 4. Daemon

New `crates/mkrew-daemon/src/server/run.rs` replaces `server/dev.rs`.

- **Start one command:** cwd = `<project>/<dir>`; env = the shell profile's env, then the env
  file, then `env` (mkrew's own `MKREW_*`/`TERM` still win); the line typed into the shell (as
  ADR 0008, so any shell works) is the command followed by an exit for that shell family, so
  the session ends when the command ends and the button can show ✕: `cmd` → `<c> & exit`,
  `pwsh`/`powershell` → `<c>; exit $LASTEXITCODE`, anything else → `<c>; exit` (by the shell
  program's file stem; `mkrew_core::run::typed_line`). `Start` and `SpawnSpec` gain
  `env: Vec<(String, String)>`, applied to the `CommandBuilder` after the profile's.
  `[INFERENCE]` cmd's bare `exit` may report 0 even when the command failed; the pane keeps
  the output, which is what the user reads.
- **Env file parser** (`mkrew_core::run::parse_env`, pure): `KEY=VALUE` per line; blank lines
  and `#` comments skipped; a leading BOM dropped; optional `export ` prefix; key and value
  trimmed; one pair of matching `'` or `"` around the value removed; no interpolation, no
  multi-line values. A line without `=` or with an invalid key is an error naming the line
  number.
- **Tab:** one tab titled `run` (the first one with that title; created when missing). Every
  start and restart rebuilds its layout from the project's live run sessions: configured ones
  in config order, then any whose name left the config; nested vertical splits with equal
  widths (`1/n` for the first child). Stop does not touch the layout (exited panes keep their
  output until the next start). Panes the user added to that tab leave its layout on the next
  start (their sessions keep running). `start` and `restart` hold the async `run_starting`
  mutex, so two never interleave.
- **Autostart:** after startup and resume, `project.run_start` (all) for every non-archived
  project with `run_autostart` and at least one command.
- **Ports** (`mkrew_core::run::scan_ports`, pure): `fn scan_ports(line: &str) -> Vec<u16>`.
  Matches `http(s)://<host>:<port>` for any host, and `port`/`ports`/`port(s)` followed by
  optional spaces, an optional `:` or `=`, optional spaces and the number (case-insensitive,
  1–65535, whole number not followed by a digit). The text comes from the emulator, not raw
  PTY bytes (Vite colours the port with ANSI escapes): when a run session's screen text
  changes, it scans the visible lines it already reads for the state machine, adds new ports
  to its list (first 4 distinct, in order seen; never removed while the session lives) and
  emits `session.ports` when the list grows.
  `ponytail:` output-based — a port that scrolls off between two output batches, or is never
  printed, is missed; reading the OS socket table per process tree is the upgrade.

## 5. UI

- **Run button** at the right end of the view bar (`render_view_bar`), in every view:
  - no command or all stopped: `[▶ Run │▾]`; ▶ with no command opens Run setup, and saving
    there runs.
  - running: `[● Running 2/2 · :8080 :5180 │↻│■│▾]`.
  - some exited: `[✕ Running 1/2 · :5180 │↻│■│▾]` in the warning color (icon + text, §11).
  - Ports: every distinct port of the live run sessions, in config order; up to 4, then `+N`.
    The tooltip lists `name — :port …` per command.
- **Menu ▾:** one row per command: `name`, state (icon + text), its ports, then ↻ ■ (running)
  or ▶ (stopped). Then `Show Run tab`, `Run setup…`. The menu is mouse-driven; the keyboard
  reaches every whole-workspace action through the palette (below). Per-command keyboard
  control is a known gap, listed in HUMAN_CHECKS like the other mouse-only menus.
- **Pane header** of a run session: `<name> · :8080`.
- **Sidebar** workspace row: `▶` while any run session lives; tooltip = the ports.
- **Ports in the UI** are loaded with `session.ports` per workspace on refresh (as token usage
  is) and kept live by the `session.ports` event; dropped when the session exits.
- **Run tab focus:** Run from all-stopped switches to Terminals and the `run` tab; Restart and
  per-command Run do not change the view.
- **Run setup dialog** (replaces the Dev server dialog in `dev_server.rs`, now
  `run_setup.rs`): a list of command cards, each with Name, Folder, Command, Env file and Env
  (multi-line `KEY=VALUE`, read with the env file parser); `+ Add command`, `Remove` per card;
  `Start with the daemon` checkbox (also a focus stop: Space flips it). Tab moves to the next
  field (the shared `dialog_key` has no Shift+Tab direction). Ctrl+Enter or Enter outside the
  Env box saves; errors (client-side validation first, then the daemon's) show in the dialog.
  Opening it from ▶ with no command runs after a successful save. All values from `theme.rs`.
  The workspace menu's two dev items become one `Run setup…` item; the tab bar's Start/Stop
  dev server buttons and the "Dev server" tab relabel are removed.
- **Palette:** `Run`, `Restart`, `Stop`, `Run setup…`, `Show Run tab` (workspace commands).
  The existing `StartDevServer`/`StopDevServer` entries and the workspace menu's dev items are
  replaced.

## 6. CLI

`mk project run <project> [name]`, `mk project restart <project> [name]`,
`mk project stop <project> [name]` replace `mk project dev [--stop]`. `mk project add/update`
lose `--dev`, `--no-dev`, `--dev-autostart`; they gain `--run-autostart on|off`. Run commands
are edited in the UI only.

## 7. Errors

- Missing folder / env file, bad env file line: start refused before any spawn; the UI shows
  the message in the sidebar notice (as the dev server did) and keeps the state.
- A command exits: its pane keeps the output and exit code; the button shows ✕.
- Restart: each stopped session gets its 5 s grace, then a forced kill; if they are not all
  exited after 10 s the restart answers `timeout` (logged at `warn`) and starts nothing.

## 8. Docs

ADR `0018-run-commands.md` (supersedes 0008). SPEC: §5 schema, §7 rows, §8 autostart, §9 CLI,
§10 UI; the M8 criterion text stays. HUMAN_CHECKS: an EA check (BE and FE run, ports
`:8080 :5180` shown, restart BE only, stop all).

## 9. Testing

- `mkrew-store`: migration moves `dev_command` into one `dev` command and keeps
  `dev_autostart` as `run_autostart`; run command list round trip and replace.
- `mkrew-core`: env file parser (comments, `export`, quotes, `=` in value, bad line);
  port scanner (uvicorn `http://0.0.0.0:8080`, Vite `http://localhost:5180/`, Spring
  `port(s): 8080 (http)`, .NET `Now listening on: http://localhost:5000`; no false match on a
  time `12:34:56`, a version `1.2.3:4`, or `port 99999`).
- `mkrew-daemon` e2e (shell typing into a temp repo, as the current dev server test): two
  commands start in one `run` tab, each in its own folder with env from file + overrides;
  start again reports `started: false`; restart one keeps the other session; stop all; port
  event from a command that echoes a URL; autostart after restart; invalid setups refused
  before spawning.
- `mkrew-ui`: pure state tests of the button (label, ports, warning) and the setup dialog
  (edit, add/remove, the `project.update` it builds, validation errors).
