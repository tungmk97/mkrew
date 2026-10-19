# 0062 — Jira issues on the Board, read by the UI over REST

- Date: 2026-10-19
- Status: accepted (SPEC §5, §7, §9, §10, §20.0, §20I)
- Milestone: none (user request 2026-10-19)

## Context

The user wants to start agent work from Jira tickets: see the open issues of a workspace on
the Board, pick one, and get a task whose prompt is the ticket. The user's Jira is Jira Cloud
with an email + API token. The first ask was "Jira via MCP".

## Decision

- The Board gets a `Tasks | Jira` switch, shown when `[jira] site` is set. The Jira side lists
  the open issues assigned to the user in the workspace's Jira project keys; a click opens the
  task dialog pre-filled (title `KEY summary`, prompt = key, summary, link, description as
  plain text). "Run in a worktree" stays the dialog's own choice.
- The UI calls Jira Cloud REST v3 itself (`POST /rest/api/3/search/jql`,
  `GET /rest/api/3/issue/<key>`) with `ureq` on the background executor, like voice input
  (ADR 0045). The daemon does not talk to Jira. Requests always use https, except to a loopback
  host (tests), so the token never travels in plain text.
- `[jira] site`, `email`, `api_token_env`: the token comes from the named environment
  variable, never from `settings.toml`.
- Workspaces store `jira_keys` (migration 0020, protocol 44); the daemon normalises them.
- A task belongs to issue `KEY` when its title starts with `KEY `; no column.

## Alternatives

- Mkrew as an MCP client of a Jira MCP server: one more process (Python or Node) and a JSON-RPC
  client for the same list. Rejected; agents get Jira MCP separately (spec B).
- The daemon calls Jira (`jira.issues` request): `mk web` and the CLI could list issues too, but
  nobody asked; `jira.rs` has no gpui types so it can move there later.
- A `task.jira_key` column: survives renaming the task, costs a migration and protocol field.
  Deferred until the title link proves too weak.
- The token in the OS keychain: a new crate and a secret dialog. Rejected for the env-var rule
  voice input already uses.

## Consequences

- §1 "no cloud" gets a second user-configured exception (after speech to text): only when
  `[jira] site` is set, and only to that site.
- Only the first 50 issues show; no paging, no polling (Refresh button). The loaded list stays
  until Refresh or an app restart.
- Renaming a task away from its `KEY ` prefix unlinks it from the issue.
