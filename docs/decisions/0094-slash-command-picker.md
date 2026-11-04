# 0094 - The `/` picker lists an agent's commands from its files and a pinned table

- Date: 2026-11-04
- Status: accepted
- Milestone: none (human request 2026-11-04: typing `/` in the composer or the New task prompt must show what typing `/` shows in the agent's own terminal)

## Context

Every agent CLI shows a popup when `/` is typed: its built-in commands, plus the skills, command files and plugin commands it finds on disk.
Mkrew's prompt boxes (the composer and the New task Prompt, SPEC §20B.1) typed `/` as a plain character, so the user opened a terminal or a file manager to look up a name.
A machine window (ADR 0089) must list the commands of the computer that runs the agent, not of the computer that shows the window.

## Options considered

1. **Ask the CLI.**
   No CLI offers the list its own popup shows.
   Claude Code's SDK `supportedCommands()` needs a running streaming session and leaves out commands that need a terminal (`/theme`, `/terminal-setup`); no `claude` subcommand lists commands.
   Codex's app-server `skills/list` lists skills only, and the Codex TUI keeps skills out of `/`; its built-ins are compiled into the TUI with no API.
   omp's RPC `get_available_commands` omits the TUI-only built-ins and the prompt templates, so it differs from the popup.
   Starting a CLI per `/` would also cost a process start, may need a login, and must be written four times.
2. **Type `/` into the agent's pane and read the screen.**
   It races with the agent's state and the user's own typing, fills the user's input line, and needs a screen parser per TUI.
3. **A static list inside Mkrew only.**
   It misses the user's own commands, project commands, skills and plugins, which are the names people forget.
4. **Scan the files by each CLI's documented rules and add a version-pinned built-in table (chosen).**
   Only this one gives the same rows as the popup without running the CLI, and it works on a remote machine because the daemon does the reading.

## Decision

- A new request `slash.list` (protocol 52) returns the commands of one agent profile for a project folder, or for the project's folder inside a task worktree (`worktree_task_id`, resolved like `fs.*` without keeping a watcher).
  The daemon runs `mkrew_core::slash::list` on the blocking pool; `~` is the daemon user's home; the environment is the daemon's with the profile's `env` on top.
- Which CLI an agent runs comes from a new optional profile key `cli` (`claude`, `codex`, `omp`, `copilot`), else from the file stem of `command`; shell profiles and wrappers such as `npx` infer nothing and answer with no `cli` and no rows (the key is omitted on the wire).
  Inference exists because `seed_profiles` never overwrites a profile the user already has.
- Built-in commands come from one TOML table per CLI under `crates/mkrew-core/slash/`, pinned to a CLI version (Codex: a commit of `slash_command.rs`).
  Skills, command files and plugin commands are scanned by the per-CLI rules in the design spec (`docs/superpowers/specs/2026-11-04-prompt-skills-design.md`, section 5).
- The user fixes drift without a rebuild: `[slash] hide = [...]` and `[[slash.add]]` in the profile.
- Codex skills are not in `/`, as in Codex (its `$` popup).
  MCP prompts are not listed (they exist only while the agent is connected); the footer says so for Claude and `/mcp__server__prompt` still works when typed.
  Usage-based ordering, argument completion, omp extension and TypeScript commands are out of scope; the last are added with `[[slash.add]]`.
- The picker reuses the prompt picker's frame and the palette's fuzzy matcher.
  An empty filter keeps the CLI's own order, a filter ranks by fuzzy score with ties in that order.
  Enter or Tab types `/name `, Esc types `/` and what was typed after it.
- `/` and `@` open their picker only at the start of a line or after whitespace, through one helper (`trigger_at`), so paths, URLs and addresses type as they are.
  This also changes `@`, which used to open its picker anywhere.
- The composer and the task dialog now share one key path (`Shell::prompt_key_down`).

## Consequences

- A CLI release that adds or renames a command is stale in the table until it is updated; the user hides or adds rows in the profile meanwhile.
- The empty-filter order equals the popup's for Codex (enum order), omp (registry order, `skill:` group row) and Copilot (reference order).
  For Claude it is by name, because the docs state no order; HUMAN_CHECKS asks the user to compare it with the TUI.
- Built-ins that depend on plan, login, provider or feature flags are listed; the user hides them.
- A machine window lists the remote machine's commands and its `~`.
- The list is read when the picker opens (cached per window for the connection, refreshed at once); no file watcher.
- Tests: the scanners and tables (mkrew-core), `slash.list` with project, worktree, home and profile env (daemon), the picker model and the trigger rule (mkrew-ui).

## Sources

- Claude Code commands: https://code.claude.com/docs/en/commands.md
- Claude Code skills and command files, precedence: https://code.claude.com/docs/en/skills.md
- Claude Code plugins on disk: https://code.claude.com/docs/en/plugins/loading.md, https://code.claude.com/docs/en/plugins-reference.md
- Claude Code MCP prompts: https://code.claude.com/docs/en/mcp.md#use-mcp-prompts-as-commands
- Claude Code managed settings: https://code.claude.com/docs/en/managed-settings.md
- Claude Code SDK `supportedCommands()`: https://code.claude.com/docs/en/agent-sdk/typescript.md
- Claude Code changelog: https://github.com/anthropics/claude-code/blob/main/CHANGELOG.md
- Codex built-ins and order: https://github.com/openai/codex/blob/main/codex-rs/tui/src/slash_command.rs
- Codex popup filtering: https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/command_popup.rs, https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/slash_commands.rs
- Codex custom prompts removed in 0.117.0: https://github.com/openai/codex/issues/15941
- Codex skills: https://learn.chatgpt.com/docs/build-skills.md
- omp built-in registry: https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/slash-commands/builtin-registry.ts
- omp list assembly: https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/modes/interactive-mode.ts
- omp file commands, priorities: https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/extensibility/slash-commands.ts and `omp://slash-command-internals.md`, `omp://skills.md` (docs embedded in omp 18.13.2)
- omp prompt templates: https://github.com/can1357/oh-my-pi/blob/main/packages/coding-agent/src/config/prompt-templates.ts
- omp popup ranking and skill group: `@oh-my-pi/pi-tui/src/autocomplete.ts` (pi-tui package, installed with omp 18.13.2)
- Copilot CLI commands, skills, command files: https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference
- Copilot CLI changelog: https://github.com/github/copilot-cli/blob/main/changelog.md
