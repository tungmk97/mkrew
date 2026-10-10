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
| `smol` | anthropic/claude-sonnet-5-5:low | agents `sonic`, `scout` |
| `slow` | anthropic/claude-opus-5-5:high | agents `reviewer`, `final-reviewer`; `advisor` falls back to it |
| `tiny` | anthropic/claude-haiku-4-5-20251001 | session titles, recap; `memory` falls back to it |

Fallback chain (`retry.fallbackChains`): `{"default":["anthropic/claude-sonnet-5-5"]}`. When the
main session's Opus keeps failing (429, quota, overload), omp switches to Sonnet and returns to
Opus when its cooldown ends (`retry.fallbackRevertPolicy: cooldown-expiry`). It does not help when
the proxy itself is down: both models go through it.

Haiku needs the dated id. The proxy (`clawd.lillya.dev`) rejects `anthropic/claude-haiku-4-5`
with `400 unknown provider for model claude-haiku-4-5`, but accepts
`anthropic/claude-haiku-4-5-20251001`.

Agents, in two copies with the same content:

- project: `.omp/agents/` (in git, wins inside this repo);
- user: `C:\Users\NguyenTung\.omp\agent\agents\` (every other project; not in git).

After editing the project copy, copy it to the user folder again.

The agents:

- `final-reviewer.md`: `model: "@slow"`, read-only whole-branch review.
- `plan-executor.md`: `model: "@task"`, autoloads the `executing-plans` skill, dispatches
  `final-reviewer` for the final review.

writing-plans stays on the main session (Opus). To run executing-plans on Sonnet, the main
session hands the plan to `plan-executor` instead of running executing-plans inline: at the
writing-plans handoff, choose inline execution and say "dispatch plan-executor with
`<plan path>`". The subagent cannot ask the human; on a stop condition it reports the blocker
back to the main session.

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
| executing-plans | implement the plan (via `agent: "plan-executor"`) | `task` | sonnet-5-5 (medium) |
| ↳ | final whole-branch review (`final-reviewer`) | `slow` | opus-5-5 (high) |
| ↳ | inline in the main session (not dispatched) | `default` | **opus-5-5 (high)** |
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
| background | session titles, recap, memory | `tiny` | haiku-4-5 |
| main session | Opus fails repeatedly (429, quota) | fallback `default` | sonnet-5-5 until Opus cooldown ends |

Verified on 2026-10-09 by dispatching each agent and reading back its model: `task`, `sonic` and
`plan-executor` → `anthropic/claude-sonnet-5-5` (`plan-executor` also confirmed the
executing-plans skill was preloaded); `reviewer` and `final-reviewer` → `anthropic/claude-opus-5-5`;
a throwaway `model: "@tiny"` agent → `anthropic/claude-haiku-4-5-20251001`. The fallback chain
is configured but not exercised (it needs a real Opus failure).
The thinking levels in the table come from the configuration; the agents report only the model name.

## Change a role

```sh
omp config get modelRoles
omp config set modelRoles '{"default":"…","task":"…","smol":"…","slow":"…","tiny":"…"}'
```

`omp config set` replaces the whole `modelRoles` record, so always pass every role. You can also
use `/model` → **Roles** inside a session. List usable models with `omp models --kind chat`.
New subagents pick up the change at once. The running main session keeps its model until you
restart it or switch with `/model`.

## Rollback

Before 2026-10-09, `modelRoles` was `{"default":"anthropic/claude-opus-5-5"}` and
`retry.fallbackChains` was `{}`: every subagent and background job ran on the session model
(Opus), with no model fallback.

1. Restore the roles (global config):

   ```sh
   omp config set modelRoles '{"default":"anthropic/claude-opus-5-5"}'
   omp config set retry.fallbackChains '{}'
   omp config get modelRoles
   ```

   From PowerShell or cmd, escape the inner quotes:
   `omp config set modelRoles "{\"default\":\"anthropic/claude-opus-5-5\"}"`.

   Rollback only the Haiku change: set `modelRoles` again without the `tiny` key (titles, recap
   and memory then fall back to `smol`). Rollback only the fallback chain:
   `omp config set retry.fallbackChains '{}'`.

2. Remove the project agent:

   ```sh
   rm .omp/agents/final-reviewer.md .omp/agents/plan-executor.md
   rm ~/.omp/agent/agents/final-reviewer.md ~/.omp/agent/agents/plan-executor.md
   ```

   Or use `git revert <commit>` if the agent and this doc were committed together.

3. Check: dispatch `task` and `sonic`. Both must report `anthropic/claude-opus-5-5`.

Snapshot: `C:\Users\NguyenTung\.omp\agent\config.yml.bak-2026-10-09` is a copy of `config.yml`
**with** the configuration above (taken after the change, not before). Copy it back over
`config.yml` to restore this setup if later edits break it. It is not a rollback to the
original; use the steps above for that.

Rollback only the executing-plans change: delete `plan-executor.md` from both folders.
executing-plans then runs inline on the main session (Opus) again.

Partial rollback: change one role only by passing the full record with that one value changed.
For example, to put `smol` back on the session model, drop the `smol` key from the record.
