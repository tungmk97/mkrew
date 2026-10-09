# 0032 — Shells are profiles; each workspace picks its default shell

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

The user wants + Shell to work like + Agent: pick the shell once, then every new shell pane uses
it. Today a shell pane always runs the `shell` profile. That profile has no command, so it gets
`%COMSPEC%` (cmd.exe on Windows). SPEC §10 promised "pwsh on Windows, configurable", but nothing
could be configured. Profiles already allow `kind = "shell"` with any command, and
`session.spawn` with a named shell profile already works.

## Options

1. Shell profiles (TOML files, like agents) plus `project.default_shell`, picked in the workspace
   menu like the default agent. No default = refuse and ask.
2. A global `[terminal] shell = "pwsh"` in settings.toml, with PATH detection in the UI. One value
   for every workspace, new settings and detection code, and not the agent model.
3. Option 1, but fall back to the `shell` profile when nothing is picked.

## Decision

Option 1.

- Mkrew ships `pwsh`, `powershell`, `cmd`, `bash` and `git-bash` profiles next to `shell` (the OS
  default). Seeding adds missing sample files at every daemon start, so existing config dirs get
  them too.
- `agent.profiles` marks each profile `installed` (its program is found the way a spawn would find
  it). The menu lists only installed shells.
- A shell pane without `default_shell` is refused (`invalid_request`), and the UI asks the user to
  pick one, exactly like a missing default agent. The user asked for this instead of option 3's
  silent fallback.
- `bash` is the first bash on PATH, which on Windows is often WSL's. So Git Bash ships as its own
  profile with the default install path.

Run commands keep the `shell` profile. Their typed line and exit syntax depend on that shell's
family (`run::typed_line`), and a run must not change shell when someone picks another default
for interactive panes.

## Consequences

- Migration `0014_project_default_shell.sql` (the next free number when this landed).
- Protocol +1 (v30): `Project.default_shell`, `ProjectUpdateParams.default_shell` (absent / null /
  id), `AgentProfileInfo.installed`.
- SPEC §5, §6, §7 and §10 (the "pwsh on Windows, configurable" sentence) are updated.
- Every existing workspace asks once, on its first new shell pane after the upgrade.
- If Git Bash is installed somewhere other than `C:\Program Files\Git`, the user edits `command`
  in `git-bash.toml`.
