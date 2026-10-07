# 0018 — Run commands replace the dev server

- Date: 2026-10-07
- Status: accepted (supersedes 0008)
- Milestone: none (outside the milestone loop; spec docs/superpowers/specs/2026-10-07-run-button-design.md)

## Context

A workspace often holds several servers (EA: a Python backend in `EA-BE`, a Vite frontend in
`EA-FE`). ADR 0008 allowed one `dev_command`, run in the project folder, so users wrote wrapper
scripts (`run.ps1`) to set the folder and load `.env`.

## Options

1. Several dev commands, still one string each. Cons: no folder or env; wrapper scripts stay.
2. Run commands with name, folder, command, env file and env, typed into a shell as in 0008.
3. Launch programs directly (no shell). Cons: shell syntax (`&&`, `npm` shims) breaks.

## Decision

Option 2. Table `run_command` (migration 0011; the old `dev_command` becomes a command named
`dev`, `dev_autostart` → `run_autostart`); `Project.run_commands`; requests
`project.run_start | run_restart | run_stop` (`name?`) replace `project.dev_start | dev_stop`;
a run session is a live shell titled `run:<name>`; the live ones lie side by side in the `run`
tab. The typed line ends with an exit for the shell family, so a session ends with its command.
Ports come from the session's screen text (`session.ports` event and request).

## Consequences

Protocol v25. `mk project dev` → `mk project run | restart | stop <p> [name]`; `--dev*` flags
removed, `--run-autostart` added. Ports that are never printed are not shown (reading the OS
socket table is the upgrade). Commands are edited in the UI only.
