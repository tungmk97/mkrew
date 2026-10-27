# Jira on the Board Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Board gets a `Tasks | Jira` switch; the Jira side lists the workspace's open Jira Cloud issues assigned to the user, and clicking one opens the task dialog pre-filled from the ticket (or the existing linked task).

**Architecture:** Each workspace stores Jira project keys (`project.jira_keys`, migration 0020, protocol 44, set from the UI or `mk project update --jira`). The UI reads `[jira]` from `settings.toml`, calls Jira REST v3 itself with `ureq` on the background executor (pure helpers + two blocking calls in `crates/mkrew-ui/src/jira.rs`, shaped like `stt.rs`), keeps per-workspace Jira state in a pure model (`crates/mkrew-ui/src/jira_board.rs`), and renders it in `crates/mkrew-ui/src/shell/jira.rs`. Issue ↔ task link = task title starts with `KEY `.

**Tech Stack:** Rust, gpui (gpui-pre 0.3.7), ureq 3 + rustls, serde_json, base64 0.22 (already in `Cargo.lock`), rusqlite, clap.

**Spec:** `docs/superpowers/specs/2026-10-19-jira-board-design.md`

## Global Constraints

- Jira Cloud only; Basic auth `email:api_token`; token read from the env var `[jira] api_token_env` names, never from the file; the token never appears in messages or logs.
- `[jira]` defaults: `site = ""` (Jira off, no switch), `email = ""`, `api_token_env = "JIRA_API_TOKEN"`.
- Search: `POST /rest/api/3/search/jql`, fields `summary,status,issuetype`, `maxResults` 50, one page. Issue: `GET /rest/api/3/issue/<key>?fields=summary,description`. Timeout 20 s.
- JQL: `project in (K1, K2) AND assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC`.
- Key rule (daemon): trim, upper-case, drop empty, de-duplicate keeping first-seen order; each must match `^[A-Z][A-Z0-9_]+$` else `invalid_request` naming the key. Stored as JSON text; empty list stored as `NULL`.
- Protocol 43 → 44. Migration `0020_project_jira_keys.sql`. ADR 0062. SPEC §20I.
- Link: task belongs to issue `K` iff `task.project_id` matches and `task.title` starts with `K` followed by a space.
- Draft title `"<KEY> <summary>"`; prompt `"<KEY>: <summary>\n<base>/browse/<KEY>"` then, when the description is not empty, `"\n\n<description text>"`.
- UI: every color/spacing/radius/font size from `crates/mkrew-ui/src/theme.rs` (SPEC §14); dark and light.
- No test calls the network: HTTP tests use a loopback `TcpListener` like `stt.rs`.
- Clippy `-D warnings`; no `unwrap/expect` outside tests unless provably infallible (comment why).
- Commits: only when the human asked for commits in this run; explicit paths only; `git -c user.email=tungnv975@gmail.com commit -m "<conventional message>"`. Never commit Cargo.lock, workflows, `docs/superpowers/specs/`, or other people's WIP.

## Review Focus

1. **Keys typed loosely** (`" proj , Bill,proj,"`) → saved as `["PROJ", "BILL"]`; `"PR-1"` or `"1AB"` refused with the key named. Pinned in Task 1 (`normalize_keys` tests + daemon e2e).
2. **Prefix collisions** — task `PROJ-123 x` must not link to issue `PROJ-12`, and `PROJ-123x` links to nothing. Pinned in Task 6 (`linked_task` test).
3. **Late replies** — Refresh twice, or switch workspace while loading: an older search answer must not overwrite a newer state. Pinned in Task 6 (`finish_load` serial test).
4. **Rich descriptions** — nested lists, code blocks, mentions, link cards, `null` description must give readable text, not JSON or nothing. Pinned in Task 4 (`adf_to_text` fixture test).
5. **Bad login / unknown project** — 401/403 and 400 with `errorMessages` must show the spec's messages without the token. Pinned in Task 4 (`answer_error` table test) and Task 5 (token-not-in-error test).

---

## File Structure

| File | Change | Responsibility |
|---|---|---|
| `crates/mkrew-core/src/jira.rs` | Create | `normalize_keys`, the daemon's key rule (spec B can reuse it) |
| `crates/mkrew-core/src/lib.rs` | Modify | `pub mod jira;` |
| `crates/mkrew-core/src/settings.rs` | Modify | `JiraSettings`, `Settings.jira`, template |
| `crates/mkrew-store/migrations/0020_project_jira_keys.sql` | Create | column |
| `crates/mkrew-store/src/lib.rs` | Modify | migration list, columns, `ProjectChanges.jira_keys`, read/write |
| `crates/mkrew-store/src/tests.rs` | Modify | persistence test |
| `crates/mkrew-protocol/src/lib.rs` | Modify | `PROTOCOL_VERSION = 44` |
| `crates/mkrew-protocol/src/message.rs` | Modify | `Project.jira_keys`, `ProjectUpdateParams.jira_keys` |
| `crates/mkrew-protocol/src/message/tests.rs` | Modify | serde tests + literals |
| `crates/mkrew-daemon/src/server.rs` | Modify | `project_update` normalises keys |
| `crates/mkrew-daemon/tests/daemon/*.rs` | Modify | literals + e2e test |
| `crates/mkrew-cli/src/main.rs`, `crates/mkrew-cli/src/project.rs` | Modify | `--jira` |
| `crates/mkrew-cli/tests/cli.rs` | Modify | CLI test |
| `crates/mkrew-ui/Cargo.toml` | Modify | `base64 = "0.22"` |
| `crates/mkrew-ui/src/jira.rs` | Create | Jira REST: pure helpers + `search`, `issue_draft` |
| `crates/mkrew-ui/src/jira_board.rs` | Create | pure per-workspace Jira view state + `linked_task` |
| `crates/mkrew-ui/src/lib.rs` | Modify | `pub mod jira; pub mod jira_board;` |
| `crates/mkrew-ui/src/task_dialog.rs` | Modify | `TaskDialog::from_draft` |
| `crates/mkrew-ui/src/workspace_menu.rs` | Modify | `set_jira_keys` params builder + literals |
| `crates/mkrew-ui/src/shell/jira.rs` | Create | render + actions of the Jira side |
| `crates/mkrew-ui/src/shell.rs` | Modify | `mod jira;` |
| `crates/mkrew-ui/src/shell/board.rs` | Modify | header with switch; Jira body |
| `crates/mkrew-ui/src/main.rs`, `crates/mkrew-ui/src/shell/connection.rs` | Modify | Shell fields, init, settings reload, tick |
| other `Project {..}` / `ProjectUpdateParams {..}` literals | Modify | add the new field |
| `docs/decisions/0062-jira-on-the-board.md`, `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md` | Create/Modify | docs |

---

### Task 1: `jira_keys` on the workspace (core rule, protocol 44, store, daemon)

**Files:**
- Create: `crates/mkrew-core/src/jira.rs`, `crates/mkrew-store/migrations/0020_project_jira_keys.sql`
- Modify: `crates/mkrew-core/src/lib.rs`, `crates/mkrew-protocol/src/lib.rs:16`, `crates/mkrew-protocol/src/message.rs:462-492,741-777`, `crates/mkrew-store/src/lib.rs:17-40,79-93,144-166,287-347,1421-1477`, `crates/mkrew-daemon/src/server.rs:962-1008`
- Modify (literals only, add `jira_keys: None` / `jira_keys: Vec::new()`): `crates/mkrew-cli/src/project.rs:58`; `crates/mkrew-protocol/src/message/tests.rs` (~101, ~449, 660); `crates/mkrew-daemon/tests/daemon/main.rs` (668, 681, 872); `crates/mkrew-daemon/tests/daemon/runs.rs` (20, 48); `crates/mkrew-daemon/tests/daemon/workspaces.rs` (~422, ~613); `crates/mkrew-ui/src/run_setup.rs` (~357, ~451); `crates/mkrew-ui/src/workspace_menu.rs` (~81, 95, ~109, ~123, ~137); `crates/mkrew-ui/tests/connect.rs` (~91); `crates/mkrew-ui/tests/lifecycle.rs` (~126); `crates/mkrew-ui/src/inbox.rs` (~168); `crates/mkrew-ui/src/run.rs` (~293); `crates/mkrew-ui/src/palette/tests.rs` (~52); `crates/mkrew-ui/src/sidebar/tests.rs` (~7). Re-grep `ProjectUpdateParams {` and `Project {` after the change: the compiler lists any missed one.
- Test: `crates/mkrew-core/src/jira.rs` (unit), `crates/mkrew-protocol/src/message/tests.rs`, `crates/mkrew-store/src/tests.rs`, `crates/mkrew-daemon/tests/daemon/workspaces.rs`

**Interfaces:**
- Produces: `mkrew_core::jira::normalize_keys(keys: Vec<String>) -> Result<Vec<String>, String>`; `Project.jira_keys: Vec<String>`; `ProjectUpdateParams.jira_keys: Option<Vec<String>>` (absent = unchanged, `[]` = clear); `mkrew_store::ProjectChanges.jira_keys: Option<&[String]>`.

- [ ] **Step 1: Write the failing core test** — create `crates/mkrew-core/src/jira.rs`:

```rust
//! Jira project keys of a workspace (SPEC §20I, ADR 0062).

/// Trims, upper-cases, drops empty entries and duplicates (first one wins), then checks each
/// key is a Jira project key (`^[A-Z][A-Z0-9_]+$`). The error names the first bad key.
pub fn normalize_keys(keys: Vec<String>) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for key in keys {
        let key = key.trim().to_ascii_uppercase();
        if key.is_empty() || out.contains(&key) {
            continue;
        }
        let mut chars = key.chars();
        let first_ok = chars.next().is_some_and(|c| c.is_ascii_uppercase());
        let rest_ok = key.len() >= 2
            && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if !(first_ok && rest_ok) {
            return Err(format!(
                "{key:?} is not a Jira project key (letters, digits and _, starting with a letter)"
            ));
        }
        out.push(key);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn keys_are_trimmed_upper_cased_and_deduplicated_in_order() {
        assert_eq!(
            normalize_keys(keys(&[" proj ", "Bill", "PROJ", "", "  "])).unwrap(),
            keys(&["PROJ", "BILL"])
        );
        assert_eq!(normalize_keys(Vec::new()).unwrap(), Vec::<String>::new());
        assert_eq!(normalize_keys(keys(&["A1_B"])).unwrap(), keys(&["A1_B"]));
    }

    #[test]
    fn non_keys_are_refused_by_name() {
        for bad in ["PR-1", "1AB", "P", "PRÓJ", "PR OJ"] {
            let error = normalize_keys(keys(&["OK", bad])).unwrap_err();
            assert!(error.contains(&bad.to_ascii_uppercase()), "{bad}: {error}");
        }
    }
}
```

Add `pub mod jira;` to `crates/mkrew-core/src/lib.rs` in the alphabetical `pub mod` list.

- [ ] **Step 2: Run it** — `cargo test -p mkrew-core jira -j 4`. Expected: PASS (pure function written with its tests; if a case fails, fix the function, not the test).

- [ ] **Step 3: Protocol** — in `crates/mkrew-protocol/src/message.rs`, `Project` (after `default_shell`):

```rust
    /// Jira project keys linked to this workspace (SPEC §20I); empty = none.
    #[serde(default)]
    pub jira_keys: Vec<String>,
```

`ProjectUpdateParams` (after `run_commands`):

```rust
    /// Replaces the Jira project keys; `[]` clears them. Normalised by the daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira_keys: Option<Vec<String>>,
```

`crates/mkrew-protocol/src/lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 44;`

Add to `project_update_tells_missing_from_null` in `message/tests.rs` (next to the `run_commands` asserts; the `parse` helper and `id` already exist there):

```rust
    assert_eq!(parse(json!({"project_id": id})).jira_keys, None);
    assert_eq!(
        parse(json!({"project_id": id, "jira_keys": []})).jira_keys,
        Some(Vec::new())
    );
    assert_eq!(
        parse(json!({"project_id": id, "jira_keys": ["PROJ"]})).jira_keys,
        Some(vec!["PROJ".to_owned()])
    );
```

In the `Event::ProjectUpdated(Project { .. })` round trip (~660) set `jira_keys: vec!["PROJ".into()]`. Add `jira_keys: None` to every other `ProjectUpdateParams` literal and `jira_keys: Vec::new()` to every other `Project` literal listed under **Files**.

- [ ] **Step 4: Store** — create `crates/mkrew-store/migrations/0020_project_jira_keys.sql`:

```sql
-- SPEC §5, §20I, ADR 0062. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN jira_keys TEXT; -- JSON array of Jira project keys, NULL = none
```

In `crates/mkrew-store/src/lib.rs`:
- `MIGRATIONS`: append `include_str!("../migrations/0020_project_jira_keys.sql"),` after the 0019 line.
- `PROJECT_COLUMNS`: append `, jira_keys` (row index 12).
- `ProjectChanges`: add `pub jira_keys: Option<&'a [String]>,`.
- `add_project`'s `Project { .. }` literal: `jira_keys: Vec::new(),`.
- `update_project`: after the `pinned` block

```rust
        if let Some(keys) = changes.jira_keys {
            project.jira_keys = keys.to_vec();
        }
```

  and replace the UPDATE with

```rust
        // A Vec of strings always serializes.
        #[allow(clippy::expect_used)]
        let jira_keys = (!project.jira_keys.is_empty())
            .then(|| serde_json::to_string(&project.jira_keys).expect("strings serialize"));
        tx.execute(
            "UPDATE project SET name = ?2, default_agent = ?3, run_autostart = ?4, color = ?5, default_shell = ?6, pinned = ?7, jira_keys = ?8 WHERE id = ?1",
            params![
                id.to_string(),
                project.name,
                project.default_agent,
                project.run_autostart,
                project.color,
                project.default_shell,
                project.pinned,
                jira_keys
            ],
        )?;
```

- `RawProject`: add `jira_keys: Option<String>,`; `from_row`: `jira_keys: row.get(12)?,`; `into_project`:

```rust
        let jira_keys = match self.jira_keys {
            Some(json) => serde_json::from_str(&json).map_err(|_| corrupt("project.jira_keys", json))?,
            None => Vec::new(),
        };
```

  and `jira_keys,` in the `Project` literal.

Add to `crates/mkrew-store/src/tests.rs` next to `project_default_shell_persists_and_clears` (copy its `dir`/`store`/`open` setup lines verbatim from that test):

```rust
#[test]
fn project_jira_keys_persist_and_clear() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert!(p.jira_keys.is_empty());
    let keys = ["PROJ".to_owned(), "BILL".to_owned()];
    let set = ProjectChanges { jira_keys: Some(&keys), ..ProjectChanges::default() };
    assert_eq!(store.update_project(p.id, set).unwrap().jira_keys, keys);
    drop(store);
    let store = open(&dir);
    assert_eq!(store.get_project(p.id).unwrap().jira_keys, keys);
    let clear = ProjectChanges { jira_keys: Some(&[]), ..ProjectChanges::default() };
    store.update_project(p.id, clear).unwrap();
    drop(store);
    assert!(open(&dir).get_project(p.id).unwrap().jira_keys.is_empty());
}
```

- [ ] **Step 5: Daemon** — in `project_update` (`server.rs`), after the `run_commands` block:

```rust
        let jira_keys = match params.jira_keys.map(mkrew_core::jira::normalize_keys).transpose() {
            Ok(keys) => keys,
            Err(message) => return Outcome::error(ErrorCode::InvalidRequest, message),
        };
```

and `jira_keys: jira_keys.as_deref(),` in the `ProjectChanges` literal. Update the doc comment: "…; Jira keys are normalised (`mkrew_core::jira::normalize_keys`)."

Add to `crates/mkrew-daemon/tests/daemon/workspaces.rs`:

```rust
#[test]
fn jira_keys_are_normalised_saved_and_cleared() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    assert!(project.jira_keys.is_empty());
    let update = |keys: Vec<&str>| ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
        jira_keys: Some(keys.into_iter().map(str::to_owned).collect()),
    };
    let saved: Project = client
        .call(Request::ProjectUpdate(update(vec![" proj ", "Bill", "PROJ"])))
        .into_result()
        .unwrap();
    assert_eq!(saved.jira_keys, ["PROJ", "BILL"]);
    client.wait_event(&Event::ProjectUpdated(saved.clone()));
    let refused = client.call(Request::ProjectUpdate(update(vec!["PR-1"])));
    assert_eq!(error_code(refused), ErrorCode::InvalidRequest);
    let cleared: Project = client
        .call(Request::ProjectUpdate(update(Vec::new())))
        .into_result()
        .unwrap();
    assert!(cleared.jira_keys.is_empty());
    client.stop();
}
```

- [ ] **Step 6: Run** — `cargo test -p mkrew-protocol -j 4`, `cargo test -p mkrew-store jira -j 4`, then `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` and `cargo test -p mkrew-daemon --test daemon jira -j 2`, then `cargo check --workspace --all-targets -j 4` (catches missed literals). Expected: all PASS / clean.

- [ ] **Step 7: Commit** (only if commits were asked for) — explicit paths of every file touched in this task; message `feat(daemon): workspace Jira keys (protocol 44, migration 0020)`.

---

### Task 2: `mk project update --jira`

**Files:**
- Modify: `crates/mkrew-cli/src/main.rs:213-227,288-301`, `crates/mkrew-cli/src/project.rs:44-72`
- Test: `crates/mkrew-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `ProjectUpdateParams.jira_keys` (Task 1).

- [ ] **Step 1: Write the failing test** — after `project_run_autostart_is_switched_on_update` in `cli.rs`:

```rust
#[test]
fn project_jira_keys_are_set_and_cleared_on_update() {
    let env = Env::new();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let set = env.ok(&["project", "update", "demo", "--jira", "proj, BILL"]);
    assert_eq!(set.trim(), "Jira keys of demo: PROJ, BILL");
    let cleared = env.ok(&["project", "update", "demo", "--jira", ""]);
    assert_eq!(cleared.trim(), "Jira keys of demo: none");
    let bad = env.mk(&["project", "update", "demo", "--jira", "PR-1"]);
    assert!(!bad.status.success());
    env.ok(&["daemon", "stop"]);
}
```

- [ ] **Step 2: Run it** — `cargo test -p mkrew-cli --test cli project_jira -j 4`. Expected: FAIL (`unexpected argument '--jira'`).

- [ ] **Step 3: Implement** — `main.rs`, in `ProjectCommand::Update` after `run_autostart`:

```rust
        /// Jira project keys of the workspace, comma-separated (`PROJ,BILL`); "" clears them.
        #[arg(long, group = "change")]
        jira: Option<String>,
```

Destructure `jira` in the match arm and pass `jira: jira.map(|list| list.split(',').map(str::to_owned).collect()),` in `project::Changes`.

`project.rs`: add `pub jira: Option<Vec<String>>,` to `Changes` (doc: "`Some(vec![])` clears"), set `jira_keys: None` in the existing literal, and append to `update` before `Ok(())`:

```rust
    if let Some(keys) = changes.jira {
        let updated: Project = client.call(Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: None,
            run_autostart: None,
            color: None,
            pinned: None,
            jira_keys: Some(keys),
        }))?;
        let keys = if updated.jira_keys.is_empty() {
            "none".to_owned()
        } else {
            updated.jira_keys.join(", ")
        };
        println!("Jira keys of {}: {keys}", updated.name);
    }
```

Update the `update` doc comment to list `[--jira <keys>]`.

- [ ] **Step 4: Run** — `cargo build -p mkrew-cli --bins -j 4 && cargo test -p mkrew-cli --test cli project_ -j 4`. Expected: PASS.

- [ ] **Step 5: Commit** (if asked) — `crates/mkrew-cli/src/main.rs crates/mkrew-cli/src/project.rs crates/mkrew-cli/tests/cli.rs`; `feat(cli): mk project update --jira`.

---

### Task 3: `[jira]` settings

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs` (`Settings` 14-31, near `VoiceSettings` ~114-158, `template()` ~367-435, tests ~437-717)

**Interfaces:**
- Produces: `mkrew_core::settings::JiraSettings { site: String, email: String, api_token_env: String }` (`Default`: `""`, `""`, `"JIRA_API_TOKEN"`); `Settings.jira`.

- [ ] **Step 1: Write the failing test** — in the `settings.rs` tests module, next to the voice one:

```rust
    #[test]
    fn jira_section_parses_and_rejects_a_token() {
        let jira = parse(
            Path::new("settings.toml"),
            "[jira]\nsite = \"acme.atlassian.net\"\nemail = \"an@acme.com\"\napi_token_env = \"ACME_JIRA\"\n",
        )
        .unwrap()
        .jira;
        assert_eq!(
            jira,
            JiraSettings {
                site: "acme.atlassian.net".into(),
                email: "an@acme.com".into(),
                api_token_env: "ACME_JIRA".into(),
            }
        );
        // The token itself never lives in the file (SPEC §20I).
        let message = parse_err("[jira]\napi_token = \"x\"\n");
        assert!(message.contains("unknown field `api_token`"), "{message}");
    }
```

and in `missing_file_is_defaults` add:

```rust
        assert_eq!(settings.jira.site, "");
        assert_eq!(settings.jira.email, "");
        assert_eq!(settings.jira.api_token_env, "JIRA_API_TOKEN");
```

- [ ] **Step 2: Run** — `cargo test -p mkrew-core settings -j 4`. Expected: FAIL (`JiraSettings` not found).

- [ ] **Step 3: Implement** — after `VoiceSettings`'s `Default` impl:

```rust
/// `[jira]` (SPEC §20I, ADR 0062): the Jira Cloud site the Board reads issues from.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JiraSettings {
    /// `acme.atlassian.net` (with or without `https://`); empty = Jira off.
    pub site: String,
    /// The Atlassian account email used with the API token.
    pub email: String,
    /// Name of the environment variable holding the API token; the token is never read
    /// from this file.
    pub api_token_env: String,
}

impl Default for JiraSettings {
    fn default() -> Self {
        Self {
            site: String::new(),
            email: String::new(),
            api_token_env: "JIRA_API_TOKEN".into(),
        }
    }
}
```

`Settings`: after `web`, add

```rust
    /// `[jira]`: Jira issues on the Board (SPEC §20I, ADR 0062).
    pub jira: JiraSettings,
```

`template()`: append after the `[web]` block, keeping the file's existing style (`# key = value` lines must equal the defaults; explanatory comment lines must not contain ` = `):

```text
\n\
# [jira]: Jira issues on the Board (Tasks | Jira). The token comes from the environment\n\
# variable named below, never from this file.\n\
# [jira]\n\
# site = \"\"\n\
# email = \"\"\n\
# api_token_env = \"JIRA_API_TOKEN\"\n\
```

Copy how the `[web]` block is written in `template()` exactly (header comment line, `# [web]` line, escapes); `the_template_parses_to_the_defaults_commented_or_not` checks it.

- [ ] **Step 4: Run** — `cargo test -p mkrew-core settings -j 4`. Expected: PASS, including the template round-trip test.

- [ ] **Step 5: Commit** (if asked) — `crates/mkrew-core/src/settings.rs`; `feat(core): [jira] settings`.

---

### Task 4: Jira REST helpers — pure part (`crates/mkrew-ui/src/jira.rs`)

**Files:**
- Create: `crates/mkrew-ui/src/jira.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod jira;` between `pub mod status;`… keep alphabetical), `crates/mkrew-ui/Cargo.toml` (add after `anyhow`: `# Basic auth for Jira (SPEC §20I); 0.22 is already in the tree.` + `base64 = "0.22"`)

**Interfaces:**
- Consumes: `mkrew_core::settings::JiraSettings` (Task 3).
- Produces:
  - `pub struct Issue { pub key: String, pub summary: String, pub status: String, pub kind: String }` (`Debug, Clone, PartialEq, Eq`)
  - `pub struct Draft { pub title: String, pub prompt: String }` (`Debug, Clone, PartialEq, Eq`)
  - `pub fn api_token(settings: &JiraSettings, var: impl Fn(&str) -> Option<String>) -> anyhow::Result<String>`
  - `pub fn base_url(site: &str) -> String`
  - `pub fn auth_header(email: &str, token: &str) -> String`
  - `pub fn jql(keys: &[String]) -> String`
  - `pub fn search_body(keys: &[String]) -> String`
  - `pub fn parse_search(status: u16, body: &str) -> anyhow::Result<Vec<Issue>>`
  - `pub fn parse_issue(site: &str, key: &str, status: u16, body: &str) -> anyhow::Result<Draft>`
  - `pub fn adf_to_text(doc: &serde_json::Value) -> String`
  - `pub fn draft(site: &str, key: &str, summary: &str, description: &str) -> Draft`

- [ ] **Step 1: Write the failing tests** — create `crates/mkrew-ui/src/jira.rs` with the module doc, `use` lines, and this test module (functions not written yet):

```rust
//! Jira Cloud REST v3 for the Board's Jira side (SPEC §20I, ADR 0062): pure helpers and two
//! blocking calls, run on the background executor. Nothing here touches gpui, so it can move
//! to the daemon unchanged.

use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use base64::Engine as _;
use mkrew_core::settings::JiraSettings;
use serde_json::{Value, json};

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> JiraSettings {
        JiraSettings {
            site: "acme.atlassian.net".into(),
            email: "an@acme.com".into(),
            ..JiraSettings::default()
        }
    }

    #[test]
    fn token_comes_from_the_named_variable() {
        let token = api_token(&settings(), |name| {
            (name == "JIRA_API_TOKEN").then(|| " tok \n".to_owned())
        });
        assert_eq!(token.unwrap(), "tok");
        for var in [None, Some("  ".to_owned())] {
            let error = api_token(&settings(), |_| var.clone()).unwrap_err().to_string();
            assert_eq!(
                error,
                "Set the environment variable JIRA_API_TOKEN to your Jira API token"
            );
        }
        let no_email = JiraSettings { email: " ".into(), ..settings() };
        let error = api_token(&no_email, |_| Some("tok".into())).unwrap_err().to_string();
        assert_eq!(error, "Set [jira] email in settings.toml");
    }

    #[test]
    fn site_becomes_an_https_base() {
        for site in [
            "acme.atlassian.net",
            "https://acme.atlassian.net",
            " https://acme.atlassian.net/ ",
            "http://acme.atlassian.net",
        ] {
            assert_eq!(base_url(site), "https://acme.atlassian.net", "{site}");
        }
    }

    #[test]
    fn basic_auth_encodes_email_and_token() {
        // base64("an@acme.com:tok")
        assert_eq!(auth_header("an@acme.com", "tok"), "Basic YW5AYWNtZS5jb206dG9r");
    }

    #[test]
    fn jql_lists_open_issues_of_the_keys_assigned_to_me() {
        let keys = ["PROJ".to_owned(), "BILL".to_owned()];
        assert_eq!(
            jql(&keys),
            "project in (PROJ, BILL) AND assignee = currentUser() AND statusCategory != Done \
             ORDER BY updated DESC"
        );
        let body: Value = serde_json::from_str(&search_body(&keys[..1])).unwrap();
        assert_eq!(
            body,
            json!({
                "jql": "project in (PROJ) AND assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC",
                "fields": ["summary", "status", "issuetype"],
                "maxResults": 50
            })
        );
    }

    #[test]
    fn search_answer_becomes_rows() {
        let body = r#"{"issues":[
            {"id":"1","key":"PROJ-7","fields":{"summary":"Fix login","status":{"name":"In Progress"},"issuetype":{"name":"Bug"}}},
            {"id":"2","key":"BILL-2","fields":{"summary":"Invoice PDF","status":{"name":"To Do"},"issuetype":{"name":"Story"}}}
        ],"isLast":true}"#;
        assert_eq!(
            parse_search(200, body).unwrap(),
            [
                Issue { key: "PROJ-7".into(), summary: "Fix login".into(), status: "In Progress".into(), kind: "Bug".into() },
                Issue { key: "BILL-2".into(), summary: "Invoice PDF".into(), status: "To Do".into(), kind: "Story".into() },
            ]
        );
        assert_eq!(parse_search(200, r#"{"issues":[]}"#).unwrap(), []);
        let odd = parse_search(200, "{}").unwrap_err().to_string();
        assert!(odd.starts_with("unexpected answer from Jira"), "{odd}");
    }

    #[test]
    fn error_answers_become_the_spec_messages() {
        let login = "Jira rejected the login: check [jira] email and the API token";
        assert_eq!(parse_search(401, "").unwrap_err().to_string(), login);
        assert_eq!(parse_search(403, "<html/>").unwrap_err().to_string(), login);
        let unknown = r#"{"errorMessages":["The value 'NOPE' does not exist for the field 'project'."],"errors":{}}"#;
        assert_eq!(
            parse_search(400, unknown).unwrap_err().to_string(),
            "The value 'NOPE' does not exist for the field 'project'."
        );
        assert_eq!(
            parse_issue("acme.atlassian.net", "PROJ-9", 404, "{}").unwrap_err().to_string(),
            "PROJ-9 no longer exists or you cannot see it"
        );
        let long = parse_search(500, &"x".repeat(500)).unwrap_err().to_string();
        assert_eq!(long, format!("Jira answered 500: {}…", "x".repeat(200)));
    }

    #[test]
    fn rich_descriptions_become_plain_text() {
        let doc = json!({"type": "doc", "version": 1, "content": [
            {"type": "heading", "attrs": {"level": 2}, "content": [{"type": "text", "text": "Steps"}]},
            {"type": "paragraph", "content": [
                {"type": "text", "text": "Login fails for "},
                {"type": "mention", "attrs": {"id": "1", "text": "@An"}},
                {"type": "text", "text": " see "},
                {"type": "inlineCard", "attrs": {"url": "https://x.test/y"}},
                {"type": "hardBreak"},
                {"type": "text", "text": "second line", "marks": [{"type": "strong"}]}
            ]},
            {"type": "bulletList", "content": [
                {"type": "listItem", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "a"}]},
                    {"type": "bulletList", "content": [
                        {"type": "listItem", "content": [
                            {"type": "paragraph", "content": [{"type": "text", "text": "b"}]}
                        ]}
                    ]}
                ]},
                {"type": "listItem", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "c"}]}
                ]}
            ]},
            {"type": "codeBlock", "attrs": {"language": "sh"}, "content": [{"type": "text", "text": "cargo test"}]},
            {"type": "orderedList", "content": [
                {"type": "listItem", "content": [
                    {"type": "paragraph", "content": [{"type": "text", "text": "one"}]}
                ]}
            ]},
            {"type": "rule"},
            {"type": "paragraph"}
        ]});
        assert_eq!(
            adf_to_text(&doc),
            "Steps\n\nLogin fails for @An see https://x.test/y\nsecond line\n\n- a\n  - b\n- c\n\n\
             ```\ncargo test\n```\n\n1. one"
        );
        assert_eq!(adf_to_text(&Value::Null), "");
    }

    #[test]
    fn issue_answer_becomes_the_task_draft() {
        let body = r#"{"key":"PROJ-7","fields":{"summary":"Fix login","description":
            {"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"It breaks."}]}]}}}"#;
        assert_eq!(
            parse_issue("https://acme.atlassian.net/", "PROJ-7", 200, body).unwrap(),
            Draft {
                title: "PROJ-7 Fix login".into(),
                prompt: "PROJ-7: Fix login\nhttps://acme.atlassian.net/browse/PROJ-7\n\nIt breaks.".into(),
            }
        );
        let empty = r#"{"key":"PROJ-8","fields":{"summary":"No text","description":null}}"#;
        assert_eq!(
            parse_issue("acme.atlassian.net", "PROJ-8", 200, empty).unwrap().prompt,
            "PROJ-8: No text\nhttps://acme.atlassian.net/browse/PROJ-8"
        );
    }
}
```

Add `pub mod jira;` to `crates/mkrew-ui/src/lib.rs` and the `base64` line to `crates/mkrew-ui/Cargo.toml`.

- [ ] **Step 2: Run** — `cargo test -p mkrew-ui --lib jira -j 4`. Expected: FAIL to compile (`api_token`, `Issue`, … not found).

- [ ] **Step 3: Implement** — above the test module:

```rust
/// Each call (search or one issue) gives up after this.
const TIMEOUT: Duration = Duration::from_secs(20);

/// Issues per search; one page only.
// ponytail: first 50 issues only; follow nextPageToken if someone has more open issues
const MAX_RESULTS: u32 = 50;

/// One row of the Jira list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub key: String,
    pub summary: String,
    pub status: String,
    /// The issue type (Bug, Story, Task…).
    pub kind: String,
}

/// What the task dialog opens with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub title: String,
    pub prompt: String,
}

/// The API token from the variable `api_token_env` names (`var` reads it); also checks that an
/// email is set, since both go into the login.
pub fn api_token(settings: &JiraSettings, var: impl Fn(&str) -> Option<String>) -> Result<String> {
    if settings.email.trim().is_empty() {
        bail!("Set [jira] email in settings.toml");
    }
    let name = settings.api_token_env.trim();
    match (!name.is_empty()).then(|| var(name)).flatten().map(|t| t.trim().to_owned()) {
        Some(token) if !token.is_empty() => Ok(token),
        _ => bail!("Set the environment variable {name} to your Jira API token"),
    }
}

/// `https://<host>` for a site given as a bare host or a URL.
pub fn base_url(site: &str) -> String {
    let host = site.trim();
    let host = host
        .strip_prefix("https://")
        .or_else(|| host.strip_prefix("http://"))
        .unwrap_or(host);
    format!("https://{}", host.trim_end_matches('/'))
}

pub fn auth_header(email: &str, token: &str) -> String {
    let pair = format!("{}:{token}", email.trim());
    format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(pair))
}

/// Keys are checked by the daemon (`mkrew_core::jira::normalize_keys`), so no quoting.
pub fn jql(keys: &[String]) -> String {
    format!(
        "project in ({}) AND assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC",
        keys.join(", ")
    )
}

pub fn search_body(keys: &[String]) -> String {
    json!({
        "jql": jql(keys),
        "fields": ["summary", "status", "issuetype"],
        "maxResults": MAX_RESULTS,
    })
    .to_string()
}

pub fn parse_search(status: u16, body: &str) -> Result<Vec<Issue>> {
    let json = answer(status, body, None)?;
    let issues = json
        .get("issues")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("unexpected answer from Jira: {}", short(body)))?;
    Ok(issues
        .iter()
        .map(|issue| Issue {
            key: text(issue, "/key"),
            summary: text(issue, "/fields/summary"),
            status: text(issue, "/fields/status/name"),
            kind: text(issue, "/fields/issuetype/name"),
        })
        .collect())
}

pub fn parse_issue(site: &str, key: &str, status: u16, body: &str) -> Result<Draft> {
    let json = answer(status, body, Some(key))?;
    let summary = text(&json, "/fields/summary");
    let description = adf_to_text(json.pointer("/fields/description").unwrap_or(&Value::Null));
    Ok(draft(site, key, &summary, &description))
}

pub fn draft(site: &str, key: &str, summary: &str, description: &str) -> Draft {
    let mut prompt = format!("{key}: {summary}\n{}/browse/{key}", base_url(site));
    if !description.is_empty() {
        prompt.push_str("\n\n");
        prompt.push_str(description);
    }
    Draft { title: format!("{key} {summary}"), prompt }
}

/// The JSON of a 2xx answer, or the spec's message for any other status (`key` = the issue
/// asked for, for the 404 message).
fn answer(status: u16, body: &str, key: Option<&str>) -> Result<Value> {
    let json: Option<Value> = serde_json::from_str(body).ok();
    if (200..300).contains(&status) {
        return json.ok_or_else(|| anyhow!("unexpected answer from Jira: {}", short(body)));
    }
    match (status, key) {
        (401 | 403, _) => bail!("Jira rejected the login: check [jira] email and the API token"),
        (404, Some(key)) => bail!("{key} no longer exists or you cannot see it"),
        _ => {}
    }
    let messages: Vec<&str> = json
        .as_ref()
        .and_then(|json| json.get("errorMessages"))
        .and_then(Value::as_array)
        .map(|list| list.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if status == 400 && !messages.is_empty() {
        bail!("{}", messages.join(" "));
    }
    bail!("Jira answered {status}: {}", short(body))
}

fn text(json: &Value, pointer: &str) -> String {
    json.pointer(pointer).and_then(Value::as_str).unwrap_or_default().to_owned()
}

/// The first 200 chars of `body`, trimmed.
fn short(body: &str) -> String {
    let body = body.trim();
    match body.char_indices().nth(200) {
        Some((end, _)) => format!("{}…", &body[..end]),
        None => body.to_owned(),
    }
}

/// Atlassian Document Format as plain text: blocks separated by a blank line, lists as
/// `- ` / `1. ` lines (nested ones indented), code blocks fenced, mentions and link cards as
/// their text and URL. Unknown nodes give their children's text.
pub fn adf_to_text(doc: &Value) -> String {
    let mut out = String::new();
    walk(doc, &mut out);
    out.trim().to_owned()
}

fn walk(node: &Value, out: &mut String) {
    let children = node.get("content").and_then(Value::as_array).into_iter().flatten();
    let attr = |name: &str| node.pointer(&format!("/attrs/{name}")).and_then(Value::as_str);
    match node.get("type").and_then(Value::as_str).unwrap_or_default() {
        "text" => out.push_str(node.get("text").and_then(Value::as_str).unwrap_or_default()),
        "hardBreak" => out.push('\n'),
        "mention" => out.push_str(attr("text").unwrap_or_default()),
        "inlineCard" | "blockCard" => out.push_str(attr("url").unwrap_or_default()),
        "paragraph" | "heading" => {
            children.for_each(|child| walk(child, out));
            out.push_str("\n\n");
        }
        "codeBlock" => {
            out.push_str("```\n");
            children.for_each(|child| walk(child, out));
            out.push_str("\n```\n\n");
        }
        kind @ ("bulletList" | "orderedList") => {
            for (index, item) in children.enumerate() {
                let marker = if kind == "orderedList" {
                    format!("{}. ", index + 1)
                } else {
                    "- ".to_owned()
                };
                let mut inner = String::new();
                item.get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .for_each(|child| walk(child, &mut inner));
                for (line_no, line) in inner.lines().filter(|l| !l.is_empty()).enumerate() {
                    if line_no == 0 {
                        out.push_str(&marker);
                    } else {
                        out.push_str(&" ".repeat(marker.len()));
                    }
                    out.push_str(line);
                    out.push('\n');
                }
            }
            out.push('\n');
        }
        _ => children.for_each(|child| walk(child, out)),
    }
}
```

Note on the fixture: an empty `paragraph` and `rule` at the end add only blank lines, which `trim` removes. If `rustfmt` reflows the long test literals, keep their values.

- [ ] **Step 4: Run** — `cargo test -p mkrew-ui --lib jira -j 4`. Expected: PASS. Then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`. (`use std::time::Duration`, `Context as _` and `TIMEOUT` are used in Task 5; until then add nothing — run clippy at the end of Task 5 instead if it warns about them.)

- [ ] **Step 5: Commit** (if asked) — `crates/mkrew-ui/src/jira.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/Cargo.toml` (NOT Cargo.lock unless the human says so); `feat(ui): Jira REST helpers`.

---

### Task 5: Jira REST blocking calls

**Files:**
- Modify: `crates/mkrew-ui/src/jira.rs`

**Interfaces:**
- Produces: `pub fn search(settings: &JiraSettings, token: &str, keys: &[String]) -> Result<Vec<Issue>>`; `pub fn issue_draft(settings: &JiraSettings, token: &str, key: &str) -> Result<Draft>`. Both blocking; run on the background executor. A test-only `base` override is NOT added: tests pass `site = "http://127.0.0.1:<port>"` and the functions use `origin(site)` (below) which keeps an explicit `http://` loopback origin.

- [ ] **Step 1: Write the failing test** — add to the tests module (copy the loopback-server shape from `stt.rs` `transcribe_posts_multipart_with_the_key_and_reads_the_text`):

```rust
    use std::io::{BufRead, BufReader, Read, Write};

    /// One request on a loopback port (SPEC §16): returns the request head and body.
    fn serve_once(status: &str, answer: &'static str) -> (u16, std::thread::JoinHandle<(String, String)>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let status = status.to_owned();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                head.push_str(&line);
            }
            let length: usize = head
                .lines()
                .find_map(|line| {
                    let line = line.to_ascii_lowercase();
                    line.strip_prefix("content-length:")?.trim().parse().ok()
                })
                .unwrap_or(0);
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            write!(
                reader.get_mut(),
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
                 Connection: close\r\n\r\n{answer}",
                answer.len()
            )
            .unwrap();
            (head, String::from_utf8(body).unwrap())
        });
        (port, server)
    }

    #[test]
    fn search_posts_the_jql_with_basic_auth() {
        let (port, server) = serve_once(
            "200 OK",
            r#"{"issues":[{"key":"PROJ-7","fields":{"summary":"Fix login","status":{"name":"To Do"},"issuetype":{"name":"Bug"}}}]}"#,
        );
        let settings = JiraSettings { site: format!("http://127.0.0.1:{port}"), ..settings() };
        let issues = search(&settings, "tok", &["PROJ".to_owned()]).unwrap();
        assert_eq!(issues[0].key, "PROJ-7");
        let (head, body) = server.join().unwrap();
        let head = head.to_ascii_lowercase();
        assert!(head.starts_with("post /rest/api/3/search/jql http/1.1\r\n"), "{head}");
        assert!(head.contains("authorization: basic ywnaywntzs5jb206dg9r\r\n"), "{head}");
        assert!(head.contains("content-type: application/json\r\n"), "{head}");
        assert_eq!(body, search_body(&["PROJ".to_owned()]));
    }

    #[test]
    fn issue_draft_gets_the_issue_and_keeps_the_token_out_of_errors() {
        let (port, server) = serve_once(
            "200 OK",
            r#"{"key":"PROJ-7","fields":{"summary":"Fix login","description":null}}"#,
        );
        let settings = JiraSettings { site: format!("http://127.0.0.1:{port}"), ..settings() };
        let draft = issue_draft(&settings, "tok", "PROJ-7").unwrap();
        assert_eq!(draft.title, "PROJ-7 Fix login");
        let (head, _) = server.join().unwrap();
        assert!(
            head.to_ascii_lowercase()
                .starts_with("get /rest/api/3/issue/proj-7?fields=summary,description http/1.1\r\n"),
            "{head}"
        );
        let (port, server) = serve_once("401 Unauthorized", "");
        let settings = JiraSettings { site: format!("http://127.0.0.1:{port}"), ..settings() };
        let error = format!("{:#}", issue_draft(&settings, "s3cret", "PROJ-7").unwrap_err());
        server.join().unwrap();
        assert!(!error.contains("s3cret"), "{error}");
        assert!(error.starts_with("Jira rejected the login"), "{error}");
        let closed = JiraSettings { site: "http://127.0.0.1:9".into(), ..settings() };
        let error = format!("{:#}", search(&closed, "s3cret", &["PROJ".to_owned()]).unwrap_err());
        assert!(error.starts_with("Cannot reach http://127.0.0.1:9"), "{error}");
        assert!(!error.contains("s3cret"), "{error}");
    }
```

- [ ] **Step 2: Run** — `cargo test -p mkrew-ui --lib jira -j 4`. Expected: FAIL to compile (`search`, `issue_draft` not found).

- [ ] **Step 3: Implement** — add next to `base_url`:

```rust
/// Where requests go: `base_url(site)`, except an explicit `http://` origin is kept as is
/// (loopback tests; Jira Cloud is always https).
fn origin(site: &str) -> String {
    let site = site.trim();
    match site.strip_prefix("http://") {
        Some(_) => site.trim_end_matches('/').to_owned(),
        None => base_url(site),
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        // 4xx bodies carry Jira's reason (unknown project): read them.
        .http_status_as_error(false)
        .build()
        .into()
}

/// The open issues of `keys` assigned to the user. Blocking: run it off the UI thread.
pub fn search(settings: &JiraSettings, token: &str, keys: &[String]) -> Result<Vec<Issue>> {
    let origin = origin(&settings.site);
    let mut response = agent()
        .post(format!("{origin}/rest/api/3/search/jql").as_str())
        .header("Authorization", auth_header(&settings.email, token))
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .send(search_body(keys).as_bytes())
        .with_context(|| format!("Cannot reach {origin}"))?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().context("cannot read Jira's answer")?;
    parse_search(status, &body)
}

/// The task draft of one issue. Blocking: run it off the UI thread.
pub fn issue_draft(settings: &JiraSettings, token: &str, key: &str) -> Result<Draft> {
    let origin = origin(&settings.site);
    let mut response = agent()
        .get(format!("{origin}/rest/api/3/issue/{key}?fields=summary,description").as_str())
        .header("Authorization", auth_header(&settings.email, token))
        .header("Accept", "application/json")
        .call()
        .with_context(|| format!("Cannot reach {origin}"))?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_string().context("cannot read Jira's answer")?;
    parse_issue(&settings.site, key, status, &body)
}
```

The Basic auth vector in the test (`ywnaywntzs5jb206dg9r`) is the lower-cased `YW5AYWNtZS5jb206dG9r` from Task 4. `key` is a Jira key from Jira's own answer (`[A-Z0-9_-]`), so it needs no URL encoding. The `{:#}` format shows the context chain: `Cannot reach <origin>: <io error>`, matching the spec's `Cannot reach <site>: <error>`.

- [ ] **Step 4: Run** — `cargo test -p mkrew-ui --lib jira -j 4` → PASS; `cargo clippy -p mkrew-ui --all-targets -- -D warnings` → clean.

- [ ] **Step 5: Commit** (if asked) — `crates/mkrew-ui/src/jira.rs`; `feat(ui): Jira search and issue calls`.

---

### Task 6: Pure Jira view state + task dialog draft

**Files:**
- Create: `crates/mkrew-ui/src/jira_board.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod jira_board;`), `crates/mkrew-ui/src/task_dialog.rs:133-165`, `crates/mkrew-ui/src/workspace_menu.rs` (new builder)

**Interfaces:**
- Consumes: `jira::{Issue, Draft}` (Task 4); `mkrew_protocol::{Task, TaskStatus, ProjectUpdateParams}`; `crate::text_box::Input`.
- Produces:
  - `pub enum Source { #[default] Tasks, Jira }` (`Debug, Clone, Copy, PartialEq, Eq, Default`)
  - `pub enum Load { NotLoaded, Loading, Loaded(Vec<Issue>), Failed(String) }` (`Debug, Clone, PartialEq, Eq, Default` with `#[default] NotLoaded`)
  - `pub struct JiraView { pub source: Source, pub load: Load, serial: u64, pub keys_field: Option<Input>, pub keys_error: Option<String>, pub saving_keys: bool, pub opening: Option<String>, pub row_error: Option<(String, String)> }` (`Debug, Default`)
  - `impl JiraView { pub fn start_load(&mut self) -> u64; pub fn finish_load(&mut self, serial: u64, result: Result<Vec<Issue>, String>) -> bool; pub fn busy(&self) -> bool; pub fn edit_keys(&mut self, keys: &[String]); pub fn typed_keys(&self) -> Vec<String> }`
  - `pub fn linked_task<'a>(tasks: &'a [Task], project_id: Ulid, key: &str) -> Option<&'a Task>`
  - `TaskDialog::from_draft(project_id: Ulid, draft: jira::Draft) -> TaskDialog`
  - `workspace_menu::set_jira_keys(project_id: Ulid, keys: Vec<String>) -> ProjectUpdateParams`

- [ ] **Step 1: Write the failing tests** — create `crates/mkrew-ui/src/jira_board.rs`:

```rust
//! The Board's Jira side of one workspace (SPEC §20I): which side shows, the issue list being
//! loaded, the keys field, and the link from an issue to its task. Pure; `shell/jira.rs`
//! renders it.

use mkrew_protocol::Task;
use ulid::Ulid;

use crate::jira::Issue;
use crate::text_box::Input;

#[cfg(test)]
mod tests {
    use mkrew_protocol::TaskStatus;

    use super::*;

    fn task(project_id: Ulid, title: &str) -> Task {
        Task {
            id: Ulid::new(),
            project_id,
            title: title.to_owned(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::Todo,
            position: 1.0,
            checkpoint: None,
            error_flag: false,
            worktree: false,
            worktree_path: None,
            branch: None,
            fanout_of: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn issue(key: &str) -> Issue {
        Issue { key: key.into(), summary: "s".into(), status: "To Do".into(), kind: "Bug".into() }
    }

    #[test]
    fn a_task_links_to_the_issue_its_title_starts_with() {
        let (p, other) = (Ulid::new(), Ulid::new());
        let tasks = vec![
            task(p, "PROJ-123 Fix login"),
            task(p, "PROJ-123x odd"),
            task(other, "PROJ-9 elsewhere"),
            task(p, "PROJ-1"),
        ];
        assert_eq!(linked_task(&tasks, p, "PROJ-123").map(|t| t.title.as_str()), Some("PROJ-123 Fix login"));
        assert!(linked_task(&tasks, p, "PROJ-12").is_none(), "prefix of a longer key");
        assert!(linked_task(&tasks, p, "PROJ-9").is_none(), "other workspace");
        assert!(linked_task(&tasks, p, "PROJ-1").is_none(), "title is only the key, no space");
    }

    #[test]
    fn only_the_newest_load_lands() {
        let mut view = JiraView::default();
        assert_eq!(view.load, Load::NotLoaded);
        let first = view.start_load();
        let second = view.start_load();
        assert!(view.busy());
        assert!(!view.finish_load(first, Ok(vec![issue("OLD-1")])), "stale answer dropped");
        assert_eq!(view.load, Load::Loading);
        assert!(view.finish_load(second, Err("Jira answered 500: x".into())));
        assert_eq!(view.load, Load::Failed("Jira answered 500: x".into()));
        let third = view.start_load();
        assert!(view.finish_load(third, Ok(vec![issue("PROJ-1")])));
        assert_eq!(view.load, Load::Loaded(vec![issue("PROJ-1")]));
        assert!(!view.busy());
    }

    #[test]
    fn the_keys_field_starts_from_the_saved_keys_and_splits_on_commas() {
        let mut view = JiraView::default();
        view.edit_keys(&["PROJ".into(), "BILL".into()]);
        assert_eq!(view.keys_field.as_ref().map(Input::as_str), Some("PROJ, BILL"));
        view.keys_field = Some(Input::from(" proj ,, bill "));
        assert_eq!(view.typed_keys(), ["proj", "bill"]);
    }
}
```

(`Input::as_str` and `From<&str>`/`From<String>` for `Input` exist in `crates/mkrew-ui/src/text_box.rs:79,287,293`; the `task` helper copies the field list of `crates/mkrew-ui/src/board/tests.rs:21-39`.)

In `crates/mkrew-ui/src/task_dialog.rs` tests module add:

```rust
    #[test]
    fn a_jira_draft_opens_as_a_new_task_without_a_worktree() {
        let project_id = Ulid::new();
        let dialog = TaskDialog::from_draft(
            project_id,
            crate::jira::Draft { title: "PROJ-7 Fix login".into(), prompt: "PROJ-7: Fix login\nhttps://x/browse/PROJ-7".into() },
        );
        assert_eq!(dialog.mode, Mode::New { project_id });
        assert_eq!(dialog.title.as_str(), "PROJ-7 Fix login");
        assert_eq!(dialog.prompt.text(), "PROJ-7: Fix login\nhttps://x/browse/PROJ-7");
        assert_eq!(dialog.agent, None);
        assert!(!dialog.worktree);
    }
```

- [ ] **Step 2: Run** — `cargo test -p mkrew-ui --lib jira_board task_dialog -j 4` (two runs if the filter takes one). Expected: FAIL to compile.

- [ ] **Step 3: Implement** — in `jira_board.rs` above the tests:

```rust
/// Which side of the Board shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Source {
    #[default]
    Tasks,
    Jira,
}

/// The issue list of a workspace; kept until Refresh or app restart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Load {
    #[default]
    NotLoaded,
    Loading,
    Loaded(Vec<Issue>),
    Failed(String),
}

#[derive(Debug, Default)]
pub struct JiraView {
    pub source: Source,
    pub load: Load,
    /// Bumped per search; an answer with an older serial is dropped.
    serial: u64,
    /// `Some` while the keys field shows (no keys yet, or Edit).
    pub keys_field: Option<Input>,
    /// Why the last keys Save failed (shown under the field).
    pub keys_error: Option<String>,
    pub saving_keys: bool,
    /// The issue whose draft is being fetched (its row shows the dots).
    pub opening: Option<String>,
    /// `(issue key, reason)` of the last failed draft fetch, shown under that row.
    pub row_error: Option<(String, String)>,
}

impl JiraView {
    /// A search was sent; returns its serial for [`JiraView::finish_load`].
    pub fn start_load(&mut self) -> u64 {
        self.serial += 1;
        self.load = Load::Loading;
        self.serial
    }

    /// Applies a search answer unless a newer search started since; returns whether it did.
    pub fn finish_load(&mut self, serial: u64, result: Result<Vec<Issue>, String>) -> bool {
        if serial != self.serial {
            return false;
        }
        self.load = match result {
            Ok(issues) => Load::Loaded(issues),
            Err(reason) => Load::Failed(reason),
        };
        true
    }

    /// Something is in flight (the status tick keeps the dots moving).
    pub fn busy(&self) -> bool {
        self.load == Load::Loading || self.opening.is_some() || self.saving_keys
    }

    /// Shows the keys field, filled with the saved keys.
    pub fn edit_keys(&mut self, keys: &[String]) {
        self.keys_field = Some(Input::from(keys.join(", ")));
        self.keys_error = None;
    }

    /// The field's text split on commas; the daemon trims, upper-cases and checks them.
    pub fn typed_keys(&self) -> Vec<String> {
        let text = self.keys_field.as_ref().map(Input::as_str).unwrap_or_default();
        text.split(',')
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(str::to_owned)
            .collect()
    }
}

/// The task of `project_id` whose title is `key` followed by a space (SPEC §20I).
pub fn linked_task<'a>(tasks: &'a [Task], project_id: Ulid, key: &str) -> Option<&'a Task> {
    tasks.iter().find(|task| {
        task.project_id == project_id
            && task.title.strip_prefix(key).is_some_and(|rest| rest.starts_with(' '))
    })
}
```

`task_dialog.rs`, after `edit`:

```rust
    /// A new task pre-filled from a Jira issue (SPEC §20I): default agent, no worktree.
    pub fn from_draft(project_id: Ulid, draft: crate::jira::Draft) -> Self {
        Self::open(Mode::New { project_id }, draft.title, draft.prompt, None)
    }
```

`workspace_menu.rs`, after `set_run_commands` (and add `jira_keys: None` to the other builders if Task 1 did not):

```rust
/// `project.update` replacing the workspace's Jira project keys (`[]` clears them).
pub fn set_jira_keys(project_id: Ulid, keys: Vec<String>) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
        jira_keys: Some(keys),
    }
}
```

Add `pub mod jira_board;` to `crates/mkrew-ui/src/lib.rs`.

- [ ] **Step 4: Run** — `cargo test -p mkrew-ui --lib jira_board -j 4` and `cargo test -p mkrew-ui --lib task_dialog -j 4`. Expected: PASS.

- [ ] **Step 5: Commit** (if asked) — `crates/mkrew-ui/src/jira_board.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/task_dialog.rs crates/mkrew-ui/src/workspace_menu.rs`; `feat(ui): Jira view state and task draft`.

---

### Task 7: Board UI — switch, keys field, issue list, open draft

Read `skill://mkrew-ui-design` first. Only theme tokens; dark and light.

**Files:**
- Create: `crates/mkrew-ui/src/shell/jira.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (module list: `mod jira;`), `crates/mkrew-ui/src/main.rs` (`struct Shell` fields near `board`/`board_error` ~427-434 and `features` ~546), `crates/mkrew-ui/src/shell/connection.rs` (init ~264-380 incl. `board: Board::default()` ~333; settings load ~17; reload ~57-90; tick `due` ~101), `crates/mkrew-ui/src/shell/board.rs` (`render_board` ~482)

**Interfaces:**
- Consumes: `jira::{api_token, search, issue_draft, Issue}`, `jira_board::{JiraView, Source, Load, linked_task}`, `TaskDialog::{from_draft, edit}`, `workspace_menu::set_jira_keys`, `Shell::mode_toggle` (`shell/docs.rs:559`), `button`, `icon_text`, `message`, `notice`, `working_dots`, `text_field`, `self.typing`, `quick_open::key` (as in `shell/git_menus.rs:221-310`), `board::column_title`.
- Produces: `Shell.jira: HashMap<Ulid, JiraView>`, `Shell.jira_on: bool`, `Shell.jira_keys_focus: FocusHandle`; `Shell::render_board_header`, `Shell::render_jira`, `Shell::load_jira`, `Shell::open_issue`, `Shell::save_jira_keys`, `Shell::jira_keys_key_down`.

- [ ] **Step 1: Shell state** — `main.rs` `struct Shell`, after `card_error`:

```rust
    /// The Board's Jira side per workspace (SPEC §20I); kept across reconnects.
    jira: HashMap<Ulid, crate::jira_board::JiraView>,
    /// `[jira] site` is set: the Board shows the `Tasks | Jira` switch.
    jira_on: bool,
    /// The Jira keys field.
    jira_keys_focus: FocusHandle,
```

`connection.rs` init: `jira: HashMap::new(), jira_on: <settings>.jira.site.trim() != "", jira_keys_focus: cx.focus_handle(),` — use the settings value already loaded at ~17 (if that code keeps only some fields, keep `settings.jira.site` the same way). In the reload block (~57-90), next to `shell.diff_wrap = …`: `shell.jira_on = !settings.jira.site.trim().is_empty();`. In the tick (~101) extend `due` with `|| shell.jira.values().any(crate::jira_board::JiraView::busy)`. Do not clear `jira` on reconnect.

- [ ] **Step 2: Actions** — create `crates/mkrew-ui/src/shell/jira.rs` (follow the imports of `shell/voice_input.rs` and `shell/git_menus.rs`):

```rust
//! The Board's Jira side (SPEC §20I, ADR 0062): the `Tasks | Jira` switch, the workspace's
//! Jira keys field, the issue list, and opening an issue as a task draft.

impl Shell {
    /// The `[jira]` section, read per call so a hand edit applies at once (like voice).
    fn jira_settings(&self) -> Result<mkrew_core::settings::JiraSettings, String> {
        mkrew_core::settings::load(&self.settings_file)
            .map(|settings| settings.jira)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn set_board_source(&mut self, project_id: Ulid, source: Source, window: &mut Window, cx: &mut Context<'_, Self>) {
        let view = self.jira.entry(project_id).or_default();
        view.source = source;
        if source == Source::Jira && view.load == Load::NotLoaded {
            self.load_jira(project_id, window, cx);
        }
        cx.notify();
    }

    /// Searches the workspace's open issues; a no-op without keys (the field shows instead).
    pub(crate) fn load_jira(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(keys) = self.sidebar.projects().iter().find(|p| p.id == project_id).map(|p| p.jira_keys.clone()) else { return };
        if keys.is_empty() {
            return;
        }
        let setup = self.jira_settings().and_then(|settings| {
            crate::jira::api_token(&settings, |name| std::env::var(name).ok())
                .map(|token| (settings, token))
                .map_err(|error| error.to_string())
        });
        let view = self.jira.entry(project_id).or_default();
        let serial = view.start_load();
        let (settings, token) = match setup {
            Ok(setup) => setup,
            Err(reason) => {
                view.finish_load(serial, Err(reason));
                cx.notify();
                return;
            }
        };
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { crate::jira::search(&settings, &token, &keys) })
                .await
                .map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                if let Some(view) = shell.jira.get_mut(&project_id)
                    && view.finish_load(serial, result)
                {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// A click on an issue row: the linked task's edit dialog, else a new task from the ticket.
    pub(crate) fn open_issue(&mut self, project_id: Ulid, key: String, window: &mut Window, cx: &mut Context<'_, Self>) {
        if let Some(task) = crate::jira_board::linked_task(self.board.tasks(), project_id, &key).cloned() {
            self.jira.entry(project_id).or_default().source = Source::Tasks;
            self.open_task_dialog(TaskDialog::edit(task), window, cx);
            return;
        }
        let setup = self.jira_settings().and_then(|settings| {
            crate::jira::api_token(&settings, |name| std::env::var(name).ok())
                .map(|token| (settings, token))
                .map_err(|error| error.to_string())
        });
        let view = self.jira.entry(project_id).or_default();
        view.row_error = None;
        let (settings, token) = match setup {
            Ok(setup) => setup,
            Err(reason) => {
                view.row_error = Some((key, reason));
                cx.notify();
                return;
            }
        };
        view.opening = Some(key.clone());
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let fetched_key = key.clone();
            let draft = cx
                .background_executor()
                .spawn(async move { crate::jira::issue_draft(&settings, &token, &fetched_key) })
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(view) = shell.jira.get_mut(&project_id) else { return };
                if view.opening.as_deref() != Some(key.as_str()) {
                    return; // another row was clicked since
                }
                view.opening = None;
                match draft {
                    Ok(draft) => shell.open_task_dialog(TaskDialog::from_draft(project_id, draft), window, cx),
                    Err(error) => view.row_error = Some((key, format!("{error:#}"))),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Saves the keys field (`project.update`); errors show under the field.
    pub(crate) fn save_jira_keys(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else { return };
        let view = self.jira.entry(project_id).or_default();
        let params = crate::workspace_menu::set_jira_keys(project_id, view.typed_keys());
        view.saving_keys = true;
        view.keys_error = None;
        let saved = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let saved = saved.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                let Some(view) = shell.jira.get_mut(&project_id) else { return };
                view.saving_keys = false;
                match saved {
                    Ok(project) => {
                        view.keys_field = None;
                        view.load = Load::NotLoaded;
                        if !project.jira_keys.is_empty() {
                            shell.load_jira(project_id, window, cx);
                        }
                    }
                    Err(error) => view.keys_error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }
}
```

Name lookups to confirm while writing (do not invent; grep): how `Shell` finds a `Project` by id (the scout saw `self.sidebar.projects()`; use the existing accessor, e.g. `self.sidebar.projects().iter().find(|p| p.id == project_id)`); `self.connection` / `connection.call::<Project>` as in `shell/workspace_actions.rs:554-613`; `this.update` vs `this.update_in` exactly as `voice_input.rs:42-99`. `set_board_source` takes `window` because `load_jira` spawns with `spawn_in(window, …)`.

The keys field key handler: copy `git_repo_query_key_down` (`shell/git_menus.rs:266-310`) as `jira_keys_key_down`, operating on `self.jira[project_id].keys_field` (project = `self.workspace`), with `Key::Open` (Enter) → `save_jira_keys`, `Key::Close` with text → clear, `Key::Close` empty → set `keys_field = None` when the workspace already has keys (else keep it), `Key::Paste` replacing newlines with spaces, `Key::Move(_)` ignored.

- [ ] **Step 3: Render** — in `shell/jira.rs`:

- `render_board_header(&self, project_id, theme, cx) -> Option<gpui::Div>`: `None` unless `self.jira_on`; else a row `div().flex().items_center().gap(px(space::S)).px(px(space::L)).pt(px(space::S))` with
  `self.mode_toggle("board-source", source, [(Source::Tasks, "Tasks"), (Source::Jira, "Jira")], "Tasks / Jira", SharedString::default(), move |shell, source, window, cx| shell.set_board_source(project_id, source, window, cx), theme, cx)`.
  In Jira mode with keys, append the label `notice(theme.text_muted, keys.join(", "))`, an `Edit` button (`button("jira-edit", "Edit", theme, false)` → `view.edit_keys(&keys)`, focus `jira_keys_focus`) and a Refresh button (`button("jira-refresh", icon_text(Icon::RotateCw, "Refresh", button_fg(theme, false)), theme, false)` → `load_jira`).
- `render_jira(&self, project_id, now, theme, window, cx) -> gpui::AnyElement`, in this order:
  1. `keys_field` is `Some`, or the workspace has no keys → a column (`gap(px(space::S))`, `px(px(space::L))`, `pt(px(space::M))`) with the line "Link this workspace to Jira projects" (`font_size::BODY`, `theme.text_muted`), `text_field("jira-keys", field, "PROJ, BILL", focused, all_selected, theme)` wired like `git_menus.rs:221-243` (`.track_focus(&self.jira_keys_focus)`, `.on_key_down(cx.listener(Self::jira_keys_key_down))`, `.child(self.typing(&self.jira_keys_focus, Self::jira_keys_key_down, cx))`), a primary `button("jira-save", "Save", theme, true)` → `save_jira_keys` (show `working_dots(now, rgb(theme.text_muted))` beside it while `saving_keys`), and `keys_error` as `notice(theme.text, format!("✕ {error}"))` — the same style `render_board` uses for its `✕` notices.
  2. `Load::Loading` → `message(theme, None, "Loading Jira issues…".into(), None)` plus `working_dots`.
  3. `Load::Failed(reason)` → `message(theme, None, "Cannot load Jira issues.".into(), Some(reason.into()))` + `.child(button("jira-retry", "Retry", theme, false))` → `load_jira`.
  4. `Load::Loaded(issues)` empty → `message(theme, None, format!("No open issues assigned to you in {}", keys.join(", ")).into(), None)`.
  5. `Load::Loaded(issues)` → `div().id("jira-issues").flex_1().min_h_0().overflow_y_scroll().px(px(space::L)).pb(px(space::L))` with one row per issue: `div().id(("jira-issue", index))` `.flex().items_center().gap(px(space::S)).px(px(space::S)).py(px(space::XS)).rounded(px(radius::SMALL)).cursor_pointer().hover(|s| s.bg(rgb(theme.hover)))` children: kind (`font_size::META`, `text_muted`), key (`weight::MEDIUM`), summary (`flex_1().min_w_0().text_ellipsis().whitespace_nowrap()`), status (`font_size::META`, `text_muted`), and when `linked_task(..)` is `Some(task)` a chip `div().px(px(space::S)).rounded(px(radius::SMALL)).bg(rgb(theme.surface_raised)).text_size(font_size::META).child(board::column_title(task.status))`; `working_dots` when `opening == Some(key)`; on click → `open_issue(project_id, key, window, cx)`. Under a row whose key matches `row_error`, `notice(theme.text, format!("✕ {reason}"))`.
  6. `Load::NotLoaded` with keys (render never starts a load) → `message(theme, None, "Press Refresh to load Jira issues.".into(), None)`.

- [ ] **Step 4: Wire into `render_board`** (`shell/board.rs` ~482): compute `let header = self.render_board_header(project_id, theme, cx);` and `let source = self.jira.get(&project_id).map_or(Source::Tasks, |v| v.source);` at the top. If `self.jira_on && source == Source::Jira`, return `div().flex_1().min_h_0().flex().flex_col().font_family(theme.ui_font_family.clone()).children(header).child(self.render_jira(project_id, now, theme, window, cx)).into_any_element()` before the task early returns. Otherwise keep the existing flow, adding `.children(header)` as the first child of the main column and wrapping the two early-return elements (`empty(…)`, `message(… "No tasks yet." …)`) in the same column with `.children(header)` first, so the switch shows while tasks load or when there are none.

- [ ] **Step 5: Build and smoke** — `cargo build -p mkrew-ui -j 4`; `cargo clippy -p mkrew-ui --all-targets -- -D warnings`; `git diff -- crates/mkrew-ui | grep -nE '#[0-9a-fA-F]{6}|px\([0-9]'` must print nothing new (theme tokens only). Smoke without a real Jira: set `[jira] site = "acme.invalid"`, `email = "a@b.c"`, `setx JIRA_API_TOKEN x` is NOT needed — leave the variable unset, launch the app (`target/debug/mkrew.exe`; rename a locked exe to `*.locked-old.exe`, never kill the running app), open a workspace's Board: the `Tasks | Jira` switch shows; Jira shows the keys field; Save `proj` → header `PROJ · Edit · Refresh` and `Cannot load Jira issues.` with `Set the environment variable JIRA_API_TOKEN to your Jira API token`; Retry repeats it; switch back to Tasks shows the columns. Check dark and light (`[ui] theme`). Then set `site = ""`: within a second the switch disappears.

- [ ] **Step 6: Commit** (if asked) — `crates/mkrew-ui/src/shell/jira.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/board.rs`; `feat(ui): Jira side of the Board`.

---

### Task 8: Docs and full verify

**Files:**
- Create: `docs/decisions/0062-jira-on-the-board.md`
- Modify: `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`

- [ ] **Step 1: ADR** — `docs/decisions/0062-jira-on-the-board.md` (check `docs/decisions/` that 0062 is still free):

```markdown
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
  (ADR 0045). The daemon does not talk to Jira.
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
- Only the first 50 issues show; no paging, no polling (Refresh button).
- Renaming a task away from its `KEY ` prefix unlinks it from the issue.
```

- [ ] **Step 2: SPEC** —
  - §5: in the `project` DDL (~213-221) add `jira_keys TEXT, -- JSON array of Jira project keys, NULL = none (§20I)`; in the migration prose (~323-329) add "`0020_project_jira_keys.sql` adds `project.jira_keys` (ADR 0062)." (also add the missing 0018/0019 lines only if their facts are in their ADRs; otherwise leave as is).
  - §7 table row `project.list/add/update/archive` (~526): add `jira_keys?` to params and "`jira_keys` replaces the list (`[]` clears); each key is trimmed, upper-cased, de-duplicated and must match `^[A-Z][A-Z0-9_]+$` (`invalid_request` naming it)". After "Protocol 41: …" (~632) add "Protocol 44: `Project.jira_keys`, `project.update` `jira_keys?` (ADR 0062)."
  - §9 CLI block: `mk project update <p> [--base <ref> | --auto-base] [--run-autostart on|off] [--jira <KEYS>]` (match the existing line's format).
  - §10 Board: one sentence — "With `[jira] site` set, a `Tasks | Jira` switch heads the Board (§20I)."
  - §20.0 TOML block, after `[web]` lines:

```toml
[jira]                      # §20I, ADR 0062; "" site = off
site = ""                   # "acme.atlassian.net"
email = ""
api_token_env = "JIRA_API_TOKEN"  # env var holding the API token; never the token itself
```

  - New `### §20I Jira on the Board (outside the milestones, ADR 0062)` after §20H, numbered items like §20G/§20H: 1 **Switch** (shown when `[jira] site` set; per workspace; kept until restart); 2 **Keys** (field when none; `KEYS · Edit`; Save = `project.update`); 3 **List** (JQL, fields, 50, Refresh, empty text); 4 **Open** (linked → edit dialog; else `GET issue` → new-task dialog with title/prompt formats; worktree is the dialog's option); 5 **Text** (ADF rules from the design spec §2); 6 **Errors** (the table from the design spec §4); 7 **Token** (env var; never in messages or logs). Copy the exact strings from the design spec.

- [ ] **Step 3: HUMAN_CHECKS** — insert after the italic intro line (~17):

```markdown
### To verify — 2026-10-19 Jira on the Board (ADR 0062, plan docs/superpowers/plans/2026-10-19-jira-board.md)

- [ ] **Restart the daemon first** (protocol 44).
- [ ] **Setup:** create an API token at id.atlassian.com → Security → API tokens; run
  `setx JIRA_API_TOKEN <token>`, then quit and restart Mkrew. In `settings.toml` set
  `[jira] site = "<you>.atlassian.net"` and `email = "<your Atlassian email>"`.
- [ ] **Switch (dark + light):** the Board shows `Tasks | Jira`; Jira asks for keys; type your
  project key in lower case and Save → `KEY · Edit · Refresh` and your open issues, newest first.
- [ ] **Open:** click an issue → New task dialog with `KEY summary` and the ticket text (lists,
  code, links readable). Tick "Run in a worktree", Save, Run the card → branch `mkrew/key-…`.
  Back on Jira the row shows `Running`; clicking it opens the task, no second task.
- [ ] **Errors:** `setx JIRA_API_TOKEN ""` + restart → `Set the environment variable
  JIRA_API_TOKEN to your Jira API token`; a wrong token → `Jira rejected the login…`; key `NOPE`
  → Jira's message; Wi-Fi off → `Cannot reach https://…`. The token never shows.
- [ ] **CLI:** `mk project update <workspace> --jira PROJ,BILL` prints `Jira keys of …: PROJ, BILL`
  and the Board header follows.
```

- [ ] **Step 4: PROGRESS** — add under "Notes for the next session", newest first:
  "- 2026-10-19 Jira on the Board (plan docs/superpowers/plans/2026-10-19-jira-board.md, ADR 0062, protocol 43→44, migration 0020): `project.jira_keys` + `mkrew_core::jira::normalize_keys`, `mk project update --jira`, `[jira]` settings, UI `jira.rs` (REST, ADF→text), `jira_board.rs` (pure state, title-prefix link), `shell/jira.rs` (switch, keys field, list, open draft). Manual checks in HUMAN_CHECKS "2026-10-19 Jira on the Board". Spec B (Jira MCP for agents) not started. Outside the milestone loop."
  Also fix the stale "Protocol is v37" note to "Protocol is v44".

- [ ] **Step 5: Full verify** — `cargo fmt --all`; then `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN, see PROGRESS notes), run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4`, `cargo test --workspace -j 4` separately. Expected: all green.

- [ ] **Step 6: Commit** (if asked) — `docs/decisions/0062-jira-on-the-board.md docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md`; `docs: Jira on the Board (ADR 0062, SPEC §20I)`.
