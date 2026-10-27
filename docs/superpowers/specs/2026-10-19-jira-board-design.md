# Jira on the Board — design (spec A)

Date: 2026-10-19. Status: draft, waiting for the human's review.

## Goal

The Board of a workspace can show its open Jira issues next to its own tasks. Picking an issue
opens the existing task dialog pre-filled from the ticket; the human reviews the prompt, ticks
"Run in a worktree" if wanted, and saves. From there the normal task flow (Run, checkpoint, undo,
merge back, resume) does the rest.

Decided with the human (brainstorming 2026-10-19):

| Question | Decision |
|---|---|
| Who talks to Jira | Mkrew itself, over Jira Cloud REST v3 (not MCP) |
| Jira kind and login | Jira Cloud (`*.atlassian.net`), email + API token |
| Which issues | Per workspace: one or more Jira project keys; issues assigned to the current user, not Done |
| What picking an issue does | Opens the task dialog pre-filled; the human saves; worktree is the dialog's existing option |
| Where it lives | Board gets a `Tasks | Jira` switch |
| Token storage | `[jira] api_token_env` names an environment variable, like `[voice] api_key_env` |
| Issue ↔ task link | A task belongs to `PROJ-123` when its title starts with `PROJ-123 ` |
| Project keys storage | A workspace column (`project.jira_keys`), like `base_ref` / `default_shell` |
| Where the HTTP call runs | The UI, on the background executor, like `stt.rs` |

Out of scope: spec B (agents get a Jira MCP server through `mk integration install`), writing
to Jira (comments, transitions), Jira Server/Data Center, OAuth, JQL typed by the user,
`mk web` and CLI access to issues, polling.

## 1. Data and settings

**Workspace (database, protocol 43 → 44):**

- Migration `0020_project_jira_keys.sql`: `ALTER TABLE project ADD COLUMN jira_keys TEXT;`
  (JSON array of strings; `NULL` = none).
- `Project` gains `jira_keys: Vec<String>` (`#[serde(default)]`).
- `project.update` accepts `jira_keys?: string[]`: absent = unchanged, `[]` = clear. The daemon
  trims, upper-cases and de-duplicates each key, keeping the first-seen order; a key not matching
  `^[A-Z][A-Z0-9_]+$` after that is `invalid_request` naming the key. Broadcasts
  `project.updated` as today.
- CLI: `mk project update <p> --jira PROJ,BILL` (comma-separated; `--jira ""` clears).

**Global (`settings.toml`, read by the UI only):**

```toml
[jira]
site = ""                        # "acme.atlassian.net"; "" = Jira off: the Board shows no switch
email = ""
api_token_env = "JIRA_API_TOKEN" # environment variable holding the API token; never the token itself
```

`JiraSettings` joins `mkrew_core::settings::Settings` (it has `deny_unknown_fields`, so the
section must be declared) and SPEC §20.0. `site` accepts a bare host or `https://host`, with or
without a trailing `/`.

**Issue ↔ task link:** a task belongs to issue `K` when `task.title` starts with `K ` (`K`
followed by one space). No column. If renaming tasks loses links in practice, add
`task.jira_key` then.

## 2. Talking to Jira — `crates/mkrew-ui/src/jira.rs`

Pure functions plus two blocking calls, shaped like `stt.rs`. No gpui types in this file, so it
can move to the daemon later unchanged.

- `api_token(settings, var) -> Result<String>`: reads the variable `api_token_env` names; empty
  name or unset/blank variable = error "Set the environment variable JIRA_API_TOKEN to your
  Jira API token" (with the configured name).
- `base_url(site) -> String`: `https://<host>`.
- `auth_header(email, token) -> String`: `Basic base64(email:token)`. Uses the `base64` crate
  (0.22, already in `Cargo.lock`; added to `mkrew-ui/Cargo.toml`).
- `jql(keys) -> String`:
  `project in (PROJ, BILL) AND assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC`.
  Keys are already validated by the daemon, so no quoting is needed.
- `search(settings, token, keys) -> Result<Vec<Issue>>`: `POST /rest/api/3/search/jql` with JSON
  `{"jql": …, "fields": ["summary","status","issuetype"], "maxResults": 50}`
  ([Atlassian docs](https://developer.atlassian.com/cloud/jira/platform/rest/v3/api-group-issue-search/#api-rest-api-3-search-jql-post)).
  POST so the JQL needs no URL encoding. One page only:
  `// ponytail: first 50 issues only; follow nextPageToken if someone has more open issues`.
- `issue_prompt(settings, token, key) -> Result<Draft>`:
  `GET /rest/api/3/issue/<key>?fields=summary,description`; returns `Draft { title, prompt }`.
- `Issue { key, summary, status, kind }` (only what a row shows; order comes from the JQL) —
  parsed by `parse_search(status, body)`.
- `adf_to_text(&serde_json::Value) -> String`: Atlassian Document Format to plain text.
  `text` → its text; `hardBreak` → `\n`; `paragraph`, `heading` → block followed by a blank
  line; `bulletList`/`orderedList` items → `- ` / `1. ` prefixes; `codeBlock` → fenced block;
  `mention` → `attrs.text`; `inlineCard` → `attrs.url`; any other node → its children; no
  `content` → nothing. `null` description → empty text.
- Timeout 20 s per call; `ureq` with rustls as in `stt.rs`.

**Draft** (what the task dialog receives):

- title: `PROJ-123 <summary>`
- prompt:

  ```text
  PROJ-123: <summary>
  https://acme.atlassian.net/browse/PROJ-123

  <description as plain text>
  ```

## 3. Board UI

- `board::View::Board` stays one view. The Board model gains `source: Source { Tasks, Jira }`
  (default `Tasks`) and the Jira state of the shown workspace:
  `Jira { NotLoaded, Loading, Loaded(Vec<Issue>), Failed(String) }`, kept per project id in memory
  until Refresh or app restart.
- The Board header shows a two-option segmented switch `Tasks | Jira` only when `[jira] site`
  is set. Switching to Jira loads once if `NotLoaded`.
- **Jira tab, workspace without keys:** a short line "Link this workspace to Jira projects", a
  text field (`PROJ, BILL`) and Save → `project.update { jira_keys }`. Daemon errors show under
  the field.
- **Jira tab with keys:** header shows `PROJ, BILL · Edit` (Edit shows the same field, pre-filled)
  and a Refresh button. Below, one row per issue: type, key, summary, status, and, when a task
  is linked (§1), that task's column as a chip (`Todo`, `Running`, `Review`, `Done`).
- **Click a row without a linked task:** row shows a spinner; `issue_prompt` runs on the
  background executor; then `TaskDialog::from_draft(project_id, draft)` opens (new constructor;
  title and prompt filled, agent = workspace default, worktree off). Save creates the Todo task
  as today; the row then shows `Todo`. Run stays on the card, as today.
- **Click a row with a linked task:** switches the Board to `Tasks` and opens that task in the
  existing edit dialog (`TaskDialog::edit`). No second task is created.
- Loading: a spinner row. Empty result: "No open issues assigned to you in PROJ, BILL".
- All colors, spacing, radii and font sizes from `theme.rs` (SPEC §14); dark and light.

## 4. Errors

All shown inline in the Jira tab with a Retry button (`Failed(message)`), or under the row for
a failed issue fetch. Nothing is retried automatically.

| Cause | Message |
|---|---|
| Token variable unset or blank | `Set the environment variable <NAME> to your Jira API token` |
| `email` empty | `Set [jira] email in settings.toml` |
| 401 / 403 | `Jira rejected the login: check [jira] email and the API token` |
| 400 | Jira's `errorMessages` joined (e.g. an unknown project key) |
| 404 on an issue | `PROJ-123 no longer exists or you cannot see it` |
| Other status | `Jira answered <status>: <first 200 chars>` (like `stt::short`) |
| Network / timeout | `Cannot reach <site>: <error>` |

The token never appears in messages or logs.

## 5. Testing

No test touches the network.

- `jira.rs` unit tests: `jql` for one and several keys; `auth_header` known vector; `base_url`
  for `acme.atlassian.net`, `https://acme.atlassian.net/`; `parse_search` on a fixture search
  answer; error mapping per status table row; `adf_to_text` on a fixture with paragraph, heading,
  nested bullet list, code block, mention, link card and hard break; `null` description;
  draft title and prompt.
- Board model: linking by title prefix (`PROJ-12` does not match `PROJ-123 x`; `PROJ-123x`
  does not match), source switch, Jira state transitions.
- `TaskDialog::from_draft` fills title and prompt, worktree off.
- Store: migration 0020 round trip of `jira_keys`.
- Daemon: `project.update` normalisation (`" proj ,Proj"` → `["PROJ"]`), invalid key →
  `invalid_request`, `[]` clears.
- Protocol: `jira_keys` absent / `[]` / list serde.
- CLI: `--jira PROJ,BILL` and `--jira ""` build the right request.
- Manual (HUMAN_CHECKS "2026-10-19 Jira on the Board"): with a real site, list loads, click
  pre-fills the dialog, worktree run works, linked chip shows, bad token message, light and dark.

## 6. Docs

- ADR 0062 "Board reads Jira from the UI": REST not MCP, UI not daemon, token by env var, title
  prefix link; §1 "no cloud" exception like voice input (ADR 0045).
- SPEC: new §20I "Jira on the Board"; §10 Board switch; §20.0 `[jira]`; §7 protocol table
  (`project.update`, `Project`), protocol version 44; §5 migration list; §9 CLI `--jira`.
- PROGRESS note; HUMAN_CHECKS entry.
