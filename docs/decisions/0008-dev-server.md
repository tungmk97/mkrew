# 0008 — The dev server is a shell session titled "dev" with the command typed in

- Date: 2026-10-06
- Status: superseded by 0018
- Milestone: M8

## Context

SPEC §18 M8 asks for a dev server per workspace: run `project.dev_command` in its own tab,
optionally auto-started after the daemon starts. SPEC §7 had no request for it, nothing said
how the command is launched (any shell profile, any OS) or how a client tells the dev server
apart from other shells.

## Options

1. Launch the command directly (`cmd /c`, `sh -c`) as the session program. Cons: shell-specific
   argument syntax per profile/OS; when the server stops, the pane is dead (no prompt to rerun).
2. Spawn the `shell` profile and type `dev_command` + Enter once the shell is ready (the
   existing `typed` send queue of `prompt_mode = "type"`). Pros: works for every shell, the
   user keeps a prompt after Ctrl+C. Cons: the command waits for the shell's first quiet
   period (`idle_after_ms`, 3 s by default).
3. Track the dev session in a new DB column. Cons: a migration for state that dies with the
   daemon anyway (shells are not resumed).

## Decision

Option 2. The dev server is the project's live `shell`-kind session titled `dev`
(`DevServer::TITLE`); no new storage. `project.dev_start` (`project_id`, `cols`/`rows` default
120×30) answers with the running one (`started: false`) or spawns it, serialized by a daemon
mutex so a project never gets two; it shows alone in the project's tab titled `dev` whose
layout is empty or a lone pane of an exited session, else in a new `dev` tab. Returns
`DevServer{session_id, tab_id?, started}`. `project.dev_stop` asks it to terminate (like
`session.kill` without force); ok when none runs. After startup (after resume) the daemon
starts it for every non-archived project with `dev_autostart` and a dev command.

## Consequences

SPEC §7 gains `project.dev_start` / `project.dev_stop` rows, §8 the auto-start step, §9
`mk project dev <p> [--stop]`. `PROTOCOL_VERSION = 16`. The UI (T8.3) finds the running dev
server the same way (live shell titled `dev`). A shell spawned with `title: "dev"` counts as the
dev server — accepted.
