# 0061 — Ask AI: one question to an agent, outside every project

- Date: 2026-10-15
- Status: accepted (extends ADR 0016; SPEC §6, §7, §13, §20H)
- Milestone: none (user request 2026-10-15)

## Context

The user wants to ask an agent any question that is not about a project ("random questions"),
like a chat. Every session belongs to a project (`session.project_id NOT NULL`), and
`ai.generate` ran in a project folder. `claude -p` is Claude Code, not the claude.ai chat: it
loads the user's CLAUDE.md, plugins, hooks and MCP and may read or change files. With
`--safe-mode --tools "" --system-prompt "…"` it answers like a chat on the user's subscription
(probe 2026-10-15: 4.6 s without tools, 26.7 s with `--tools WebSearch`).

## Decision

- `ai.generate` gets `kind: ask` and `web` (default false); `project_id` becomes optional
  (required for the other kinds), `ai.generated.project_id` echoes it. Protocol 43.
- `ask` needs `agent_profile`, runs in `<data>/mkrew/scratch` (created on demand) with the
  question as the prompt, and keeps a code fence in the answer (it is shown as Markdown).
- Two optional profile keys, split like `oneshot`: `ask` (falls back to `oneshot`) and
  `ask_web`. `agent.profiles` reports `ask` / `ask_web` so the UI offers only what works.
  The sample `claude.toml` carries chat-like commands.
- UI: "Ask AI…" dialog (Ctrl+Shift+G, palette): agent chips, Web search Off/On, a multi-line
  question, the answer as Markdown, Copy answer. One question at a time, no history.

## Alternatives

- A built-in "Scratch" workspace with an interactive agent tab: full conversations, but a
  terminal, not a quick answer. Not chosen by the user; still possible with no code change.
- Sessions without a project (`project_id` NULL): migration plus every project-scoped path.
  Rejected.
- Calling the Anthropic API directly: against ADR 0016 and billed apart from the subscription.
  Rejected.
- One `ask` command with a `{tools}` placeholder: one line to keep, but less freedom per mode.
  Rejected for two plain keys.

## Consequences

- Existing profiles in the config dir are not rewritten: `ask` uses `oneshot` (plain `claude -p`,
  Claude Code behaviour) until the user adds the lines, and Web search stays off.
- No follow-up questions; a conversation needs an agent tab.
