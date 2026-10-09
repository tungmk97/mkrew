# Model roles for superpowers on omp

How omp picks a model for each superpowers step, what is configured now, and how to undo it.

## How omp picks a model

- A skill has no model of its own. A skill runs in the session that loaded it.
- The main session uses the `default` role.
- A subagent uses, in order: `task.agentModelOverrides[agent]`, then the agent's frontmatter
  `model`, then the session model.
- Superpowers dispatches `Subagent (general-purpose)`. omp has no `general-purpose` agent, so an
  omitted agent becomes `task` (role `@task`).
- The omp `task` tool has no per-call model field. The superpowers "Model Selection" tiers work
  only through the agent you pick: `sonic` (`@smol`), `task` (`@task`), `reviewer` /
  `final-reviewer` (`@slow`).
- The `plan` role is omp `/plan` mode only. It is not the writing-plans skill.

## Current configuration

Global, in `C:\Users\NguyenTung\.omp\agent\config.yml` (`omp config path`):

| Role | Model | Used by |
|---|---|---|
| `default` | **anthropic/claude-opus-5-5** (thinking `high` from `defaultThinkingLevel`) | main session |
| `task` | anthropic/claude-sonnet-5-5:medium | agent `task` (all `general-purpose` dispatches) |
| `smol` | anthropic/claude-sonnet-5-5:low | agents `sonic`, `scout`; `tiny`/`memory` fall back to it |
| `slow` | anthropic/claude-opus-5-5:high | agents `reviewer`, `final-reviewer`; `advisor` falls back to it |

`anthropic/claude-haiku-4-5` is not used: the proxy (`clawd.lillya.dev`) returns
`400 unknown provider for model claude-haiku-4-5`.

Project agent: `.omp/agents/final-reviewer.md` (`model: "@slow"`).

## Superpowers step → role → model

| Skill | Step | Role | Model |
|---|---|---|---|
| using-superpowers | pick and load skills | `default` | **opus-5-5 (high)** |
| brainstorming | questions, approaches, spec | `default` | **opus-5-5 (high)** |
| ↳ | spec document reviewer | `task` | sonnet-5-5 (medium) |
| writing-plans | write plan, self-check against spec | `default` | **opus-5-5 (high)** |
| ↳ | plan document reviewer | `task` | sonnet-5-5 (medium) |
| subagent-driven-development | controller | `default` | **opus-5-5 (high)** |
| ↳ | implementer per task | `task` | sonnet-5-5 (medium) |
| ↳ | task reviewer, re-review | `task` | sonnet-5-5 (medium) |
| ↳ | fix loop rounds 1–5 | `task` | sonnet-5-5 (medium) — rounds 4–5 get no stronger model |
| ↳ | final whole-branch review | `slow` | opus-5-5 (high) — **only if dispatched with `agent: "final-reviewer"`**; as `general-purpose` it runs on `task` |
| executing-plans | implement inline | `default` | **opus-5-5 (high)** |
| requesting-code-review | code reviewer | `task` | sonnet-5-5 (medium) |
| receiving-code-review | handle feedback | `default` | **opus-5-5 (high)** |
| dispatching-parallel-agents | parallel workers | `task` | sonnet-5-5 (medium) |
| systematic-debugging | debug | `default` | **opus-5-5 (high)** |
| test-driven-development | tests, implementation | `default` | **opus-5-5 (high)** |
| verification-before-completion | run checks | `default` | **opus-5-5 (high)** |
| finishing-a-development-branch | merge, PR, cleanup | `default` | **opus-5-5 (high)** |
| using-git-worktrees | create worktree | `default` | **opus-5-5 (high)** |
| writing-skills | write skill | `default` | **opus-5-5 (high)** |
| ↳ | pressure-test subagents | `task` | sonnet-5-5 (medium) |
| diagnosing-superpowers | coordinate | `default` | **opus-5-5 (high)** |
| ↳ | analyst subagents | `task` | sonnet-5-5 (medium) |
| any skill | `scout` / `sonic` | `smol` | sonnet-5-5 (low) |
| background | session titles, recap, memory | `smol` | sonnet-5-5 (low) |

Verified on 2026-10-09 by dispatching each agent and reading back its model: `task` and `sonic` →
`anthropic/claude-sonnet-5-5`; `reviewer` and `final-reviewer` → `anthropic/claude-opus-5-5`.
The thinking levels in the table come from the configuration; the agents report only the model name.

## Change a role

```sh
omp config get modelRoles
omp config set modelRoles '{"default":"…","task":"…","smol":"…","slow":"…"}'
```

`omp config set` replaces the whole `modelRoles` record, so always pass every role. You can also
use `/model` → **Roles** inside a session. List usable models with `omp models --kind chat`.
New subagents pick up the change at once. The running main session keeps its model until you
restart it or switch with `/model`.

## Rollback

Before 2026-10-09, `modelRoles` was `{"default":"anthropic/claude-opus-5-5"}`: every subagent
ran on the session model (Opus).

1. Restore the roles (global config):

   ```sh
   omp config set modelRoles '{"default":"anthropic/claude-opus-5-5"}'
   omp config get modelRoles
   ```

   From PowerShell or cmd, escape the inner quotes:
   `omp config set modelRoles "{\"default\":\"anthropic/claude-opus-5-5\"}"`.

2. Remove the project agent:

   ```sh
   rm .omp/agents/final-reviewer.md
   ```

   Or use `git revert <commit>` if the agent and this doc were committed together.

3. Check: dispatch `task` and `sonic`. Both must report `anthropic/claude-opus-5-5`.

Partial rollback: change one role only by passing the full record with that one value changed.
For example, to put `smol` back on the session model, drop the `smol` key from the record.
