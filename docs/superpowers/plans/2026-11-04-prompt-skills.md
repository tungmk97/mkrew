# Prompt inputs: `/` picker, task dialog attachments, prompt manager - Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Typing `/` in the composer or the New task Prompt box lists the commands that agent's own TUI lists; the New task Prompt box takes pasted images and dropped files; a Settings page manages the prompt library.

**Architecture:** The daemon answers `slash.list` (protocol 52) by running `mkrew_core::slash::list`, which merges a pinned built-in table per CLI with that CLI's skill, command and prompt files, so a machine window lists the agent machine's commands.
`attachment.save` takes an owner, session or project (protocol 53), and the hourly check keeps a project image while an open task names it.
The prompt library gets core CRUD functions and a Settings page; every picker re-reads it on open.

**Tech Stack:** Rust 2024, GPUI (`mkrew-ui`), `serde`/`toml`/`serde_json`, `thiserror`, `tempfile` in tests, `insta` for the profile snapshot.

**Spec:** `docs/superpowers/specs/2026-11-04-prompt-skills-design.md`

## Global Constraints

- Work in `.worktrees/prompt-skills` on `feat/prompt-skills`.
- Commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, no co-author line, `git add` with explicit paths only.
- Numbers: protocol 51 to 52 in A1, 52 to 53 in B1; ADR 0094 (A11) and 0095 (B1); no migration.
  Before A11 and B1, check that no other branch took the number: `git branch -a --no-merged master` and `git log --all --oneline -- docs/decisions/0094* docs/decisions/0095*`.
- No `unwrap`/`expect` outside tests unless provably infallible with a comment; `thiserror` in libraries; `tracing` for logs.
- No test starts an agent, reads the real home folder or uses the network; scans run on tempdirs.
- UI: tokens only from `crates/mkrew-ui/src/theme.rs`; spacing 4/8/12/16/24/32; dark and light; every list has an empty state; hover, focus, active, disabled states; keyboard-only use (ADR 0085); tooltips show shortcuts; the palette lists every new action.
- Never the em dash character in code, docs, TOML or UI text.
- Markdown docs: one sentence per line.
- Deviation from SPEC: ADR plus SPEC update in the same commit.
- Fast loop: `cargo test -p <crate> <filter>`.
  Full verify at the end of each part: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL).
  The first build after `target/` was deleted takes more than 10 minutes; check free space on C: first (about 46G on 2026-11-04).
- Clippy in part A runs after A5, A7, A8, A9, A10 and every UI task, not after A4 or A6 (code first used by the next task).
- Line numbers in tasks are from 2026-11-04; anchor on the quoted symbols when an earlier task moved them.

## Run order

| Step | Tasks (numbered for the SDD scripts) | Notes |
| --- | --- | --- |
| 1 | Task 1 (A11) | ADR 0094 and SPEC text first, as the spec asks |
| 2 | Tasks 2-11 (A1-A10) | A1 adds an interim `slash.list` arm answering `invalid_request` so every commit builds; A12 replaces it |
| 3 | Tasks 12-17 (A12-A17) | daemon handler, `trigger_at`, `prompt_key_down`, picker model, wiring and render, HUMAN_CHECKS and README; verify.sh |
| 4 | Tasks 18-21 (B1-B4) | needs `PROTOCOL_VERSION == 52` and `Shell::prompt_key_down`; verify.sh |
| 5 | Tasks 22-26 (C1-C5) | independent of A and B; verify.sh at the end of C5 |

README: A17 and C5 each add a bullet; when both land, merge them into one feature line.

## Decisions taken while planning (the spec was silent)

- `@` and `/` share `trigger_at`, so `@` now also opens only at a line start or after whitespace (`a@b.com` stays text). SPEC §20B.1 says so (A11).
- The dialog's Agent chips sit under the picker's modal shade, so the list follows the agent chosen when the picker opens; `SlashPicker::retarget` still handles a change while open.
- A new task with no agent and no workspace default types a plain `/`.
- File-tree row drags onto the task dialog cannot start while the dialog's modal shade covers the tree (`shell/focus.rs` `modal_shade` occludes); B4 builds the handler, ADR 0095 and HUMAN_CHECKS record that only OS drops are reachable today.
- Project image age comes from the ULID in its file name; a failed task read keeps every file.
- Claude built-in descriptions are the docs "Purpose" text, condensed; the table header says so.
- Prompt CRUD: the old `prompt::PromptError` struct becomes `ReadError`; `read(dir, name)` returns the whole file so the editor keeps `# Title`; reserved names are checked on the part before the first dot.

## Review Focus

1. A task worktree is a linked worktree: Claude reads the main checkout when the worktree has no `.claude/skills`, and no scan climbs past the repo root (A6, A7).
2. One broken file (bad front matter, non-UTF-8, invalid `settings.json`) becomes an error line and every other row still lists (A7, A8).
3. A slow `slash.list` answer for an agent the user just left never replaces the current rows (A15).
4. A project image named by a task that is not Done is never deleted, and an unreadable task list deletes nothing (B2).
5. A prompt rename to another case of its own name works, and no name typed in the page can leave the prompts folder or name a Windows device (C1).

Each line is pinned by the tests named in the owning task.

---

# Part A core: protocol 52, profile keys, `mkrew_core::slash` (A1-A10)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task.
> Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Everything under the daemon handler for the `/` picker: protocol 52 types, the profile keys `cli` and `[slash]`, and `mkrew_core::slash::list(cli, roots, overrides)` with its four built-in tables and its Claude Code, Codex, omp and Copilot scanners.

**Architecture:** One built-in TOML table per CLI is embedded in `mkrew-core`.
A per-CLI scanner reads that CLI's skill, command and prompt files from a `ScanRoots` (folder, home, managed folders, OS, environment) and merges them with the table by the CLI's own precedence rules.
`list` then applies the profile's `[[slash.add]]` and `[slash] hide`.
The CLI itself is never started, nothing here touches the network, and the daemon (another section) only calls `slash::list`.

**Tech Stack:** Rust workspace, `serde`, `serde_json`, `toml` 1.1 (all already in `mkrew-core`), `tempfile` and `insta` in tests.
No new dependency.

**Spec:** `docs/superpowers/specs/2026-11-04-prompt-skills-design.md`, sections 4.2, 4.3, 5 and 8 (core rows).

## Global Constraints

- Work in the worktree `.worktrees/prompt-skills` (branch `feat/prompt-skills`); commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, no co-author line, `git add` with explicit paths only.
- `PROTOCOL_VERSION` goes from 51 to 52 (task A1).
- No `unwrap` or `expect` outside tests (`clippy.toml` allows them in tests); `thiserror` is not needed because the scanners return error lines, not error types.
- No test starts an agent, reads the real home folder or uses the network; every scan runs on a tempdir (`slash/testing.rs`).
- Never the em dash character in code, docs, TOML or UI text; use `-`.
- Markdown docs: one sentence per line.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fmt check, clippy `-D warnings`, tests). Fast loop: `cargo test -p <crate> <filter>`.
- Rust 1.99, edition 2024: let chains (`if let ... && let ...`) are in use in this repo (`git.rs:295`).
- Clippy runs after A5, A7, A8, A9 and A10.
  Do not run it after A4 or A6: the table loader (A4) is first used by `list` in A5 and the scan helpers (A6) are first used by the Claude scanner in A7, so `dead_code` fires in between.
- The code in this plan was compiled and its tests were run in a scratch crate stage by stage (A3 to A10) on 2026-11-04, against the `profile.rs` of this worktree.
  The protocol crate was stubbed there (A1 is not covered by that run), so run each task's tests anyway.

## Review Focus candidates

1. A task worktree is a linked worktree (`.git` is a file).
   Claude Code reads the main checkout when the worktree has no `.claude/skills`, and no scan may climb past the repo root.
   Pinned by `a_linked_worktree_without_claude_skills_reads_the_main_checkout`, `a_linked_worktree_with_its_own_claude_skills_ignores_the_main_checkout`, `the_scan_reads_each_parent_up_to_the_repo_root_and_no_further` and `a_folder_outside_any_repo_scans_only_itself` (A7), and `a_linked_worktree_maps_its_folder_into_the_main_checkout` (A6).
2. A user's hand-edited profile: a wrong `cli`, `/deploy` in `hide`, a two-word `[[slash.add]]` name, `cli` on a shell.
   Each must be a profile error with line and column, or be normalised, and must never silently lose the rest of the profile.
   Pinned by `an_unknown_cli_is_an_error_at_its_value`, `slash_names_must_be_one_word_and_keys_must_be_known`, `slash_section_parses_hide_and_add` and `cli_is_explicit_or_inferred_from_the_command` (A2).
3. One broken file must not hide the other rows: a skill with unparseable front matter, a non-UTF-8 command file, a `settings.json` that is not JSON.
   Pinned by `a_file_with_broken_front_matter_is_listed_with_no_fields_set`, `a_file_that_cannot_be_read_is_an_error_line_and_the_rest_still_lists` (A7) and `a_settings_file_that_does_not_parse_is_an_error_line_and_the_scan_goes_on` (A8).

## Contract for the sections that consume this one

- `mkrew_core::slash::list(cli: Cli, roots: &ScanRoots, overrides: &SlashOverrides) -> SlashList` and `ScanRoots::for_machine(folder, home, env)`.
  `ScanRoots` is `Send`.
  The daemon passes the project folder (or the folder inside the task worktree) as `folder`.
  Core finds the main checkout itself from the `.git` file.
- `AgentProfile.cli` is already resolved: the explicit key, else `infer_cli(&command)`, always `None` for a shell.
  `AgentProfile.slash` carries names without a leading `/`.
- omp returns the group row as an item named `skill:` with `group: Some(n)` and `source: BuiltIn`, directly followed by its `n` rows named `skill:<name>` with `group: None`.
  The picker draws no source chip on a row that has `group`, and shows the `skill:<name>` rows only when the filter starts with `skill:` (or a bare name prefix-matches, spec 5.3).
  Everything is in one `items` vector in popup order.
- Claude rows come back sorted by name, case-insensitively.
  Codex, omp and Copilot rows are in their documented order.
- Task A1 adds a temporary `Request::SlashList` arm to the daemon dispatch (`server.rs`) only because that `match` is exhaustive.
  The daemon section's handler task replaces exactly that arm.
- Task ids of this section are A1 to A10; the daemon and UI sections start at A11.

---
### Task 2 (A1): Protocol 52: `slash.list`, `Cli`, `SlashItem`

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs` (enum `Request`, after `AgentProfiles` at line 179; new types after `ProfileErrorInfo`, which ends at line 1605)
- Modify: `crates/mkrew-protocol/src/lib.rs:16`
- Modify: `crates/mkrew-protocol/src/message/tests.rs` (the `every_m1_request_round_trips` vec after `Request::AgentProfiles` at line 383; new tests at the end of the file)
- Modify: `crates/mkrew-daemon/src/server.rs` (dispatch `match`, after the `Request::AgentProfiles` arm at line 405)
- Modify: `docs/SPEC.md` (§7 request table after the `agent.profiles` row at line 581; protocol history after line 665)

**Interfaces:**
- Produces: `mkrew_protocol::{Cli, SlashSource, SlashItem, SlashListParams, SlashListResult}`, `Request::SlashList(SlashListParams)` (`"slash.list"`), `PROTOCOL_VERSION == 52`.
- `Cli` derives `Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize`; every other new type derives `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`.

- [ ] **Step 1: Write the failing tests** in `message/tests.rs`.

In the `vec![...]` of `every_m1_request_round_trips`, after `Request::AgentProfiles,`:

```rust
        Request::SlashList(SlashListParams {
            profile_id: "claude".into(),
            project_id: sid(),
            worktree_task_id: Some(sid()),
        }),
```

New tests at the end of the file:

```rust
/// Spec 2026-11-04 §4.3: the request carries no worktree key when there is none.
#[test]
fn slash_list_request_wire_shape() {
    let message = Message::Request {
        id: 3,
        request: Request::SlashList(SlashListParams {
            profile_id: "claude".into(),
            project_id: sid(),
            worktree_task_id: None,
        }),
    };
    assert_eq!(
        round_trip(&message),
        json!({
            "type": "request", "id": 3, "method": "slash.list",
            "params": {"profile_id": "claude", "project_id": sid().to_string()}
        })
    );
    let in_worktree = SlashListParams {
        profile_id: "omp".into(),
        project_id: sid(),
        worktree_task_id: Some(sid()),
    };
    let wire = serde_json::to_value(&in_worktree).unwrap();
    assert_eq!(wire["worktree_task_id"], json!(sid().to_string()));
    assert_eq!(
        serde_json::from_value::<SlashListParams>(wire).unwrap(),
        in_worktree
    );
}

/// Spec 2026-11-04 §4.3: a row names its source and leaves out what it does not have.
#[test]
fn slash_rows_name_their_source_and_omit_what_is_missing() {
    let plain = SlashItem {
        name: "compact".into(),
        description: "Summarize the context".into(),
        argument_hint: None,
        source: SlashSource::BuiltIn,
        group: None,
    };
    assert_eq!(
        serde_json::to_value(&plain).unwrap(),
        json!({"name": "compact", "description": "Summarize the context", "source": "built_in"})
    );
    let grouped = SlashItem {
        name: "kit:deploy".into(),
        description: "Ship it".into(),
        argument_hint: Some("<env>".into()),
        source: SlashSource::Plugin("kit".into()),
        group: Some(3),
    };
    let wire = serde_json::to_value(&grouped).unwrap();
    assert_eq!(
        wire,
        json!({
            "name": "kit:deploy", "description": "Ship it", "argument_hint": "<env>",
            "source": {"plugin": "kit"}, "group": 3
        })
    );
    assert_eq!(serde_json::from_value::<SlashItem>(wire).unwrap(), grouped);
    for (source, name) in [
        (SlashSource::BuiltIn, "built_in"),
        (SlashSource::Project, "project"),
        (SlashSource::User, "user"),
        (SlashSource::Managed, "managed"),
        (SlashSource::Profile, "profile"),
    ] {
        assert_eq!(serde_json::to_value(&source).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<SlashSource>(json!(name)).unwrap(),
            source
        );
    }
}

/// Spec 2026-11-04 §4.3: `cli` is absent when the profile's CLI is unknown.
#[test]
fn slash_list_result_leaves_an_unknown_cli_out() {
    let unknown = SlashListResult {
        cli: None,
        items: vec![],
        errors: vec!["a.md: unreadable".into()],
    };
    let wire = serde_json::to_value(&unknown).unwrap();
    assert_eq!(wire, json!({"items": [], "errors": ["a.md: unreadable"]}));
    assert_eq!(
        serde_json::from_value::<SlashListResult>(wire).unwrap(),
        unknown
    );
    for (cli, name) in [
        (Cli::Claude, "claude"),
        (Cli::Codex, "codex"),
        (Cli::Omp, "omp"),
        (Cli::Copilot, "copilot"),
    ] {
        let known = SlashListResult {
            cli: Some(cli),
            items: vec![],
            errors: vec![],
        };
        assert_eq!(serde_json::to_value(&known).unwrap()["cli"], json!(name));
    }
    assert!(serde_json::from_value::<Cli>(json!("gemini")).is_err());
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-protocol slash`
Expected: compile errors, `cannot find type SlashListParams` / `cannot find type Cli in this scope`.

- [ ] **Step 3: Implement**

In `enum Request` of `message.rs`, right after `AgentProfiles,`:

```rust
    /// Returns [`SlashListResult`]: the `/` commands of the profile's agent CLI in the project
    /// folder, or in the task's worktree, read on the daemon's machine (spec 2026-11-04 §4.3).
    #[serde(rename = "slash.list")]
    SlashList(SlashListParams),
```

After `ProfileErrorInfo` (the struct ending at line 1605):

```rust
/// The agent CLIs whose `/` list Mkrew knows (spec 2026-11-04 §4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cli {
    Claude,
    Codex,
    Omp,
    Copilot,
}

/// Where a `/` row comes from; the picker shows it as a chip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlashSource {
    /// The CLI's own table (also its bundled skills).
    BuiltIn,
    /// A file in the project folder or one of its parents up to the repo root.
    Project,
    /// A file under the user's home.
    User,
    /// A file in the organisation's managed folder (Claude Code).
    Managed,
    /// A plugin, by name.
    Plugin(String),
    /// A `[[slash.add]]` row of the agent profile.
    Profile,
}

/// One row of the `/` picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashItem {
    /// Without the leading `/`.
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    pub source: SlashSource,
    /// omp only: the row stands for this many skills and expands to `skill:`; the skill rows
    /// follow it (spec 2026-11-04 §5.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
}

/// `slash.list` (spec 2026-11-04 §4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashListParams {
    /// The agent profile id (its `cli` key, or the CLI inferred from `command`, picks the list).
    pub profile_id: String,
    pub project_id: Ulid,
    /// The task worktree to scan instead of the project folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_task_id: Option<Ulid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlashListResult {
    /// `None`: the profile's CLI is unknown; `items` is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli: Option<Cli>,
    pub items: Vec<SlashItem>,
    /// Files that could not be read or parsed, one line each.
    pub errors: Vec<String>,
}
```

In `crates/mkrew-protocol/src/lib.rs:16`:

```rust
pub const PROTOCOL_VERSION: u32 = 52;
```

In `crates/mkrew-daemon/src/server.rs`, in the dispatch `match`, right after the `Request::AgentProfiles` arm (line 405).
The match is exhaustive, so this keeps the workspace compiling until the daemon section's handler task replaces the arm:

```rust
            Request::SlashList(_) => (
                Outcome::error(ErrorCode::InvalidRequest, "slash.list is not served yet"),
                After::Continue,
            ),
```

In `docs/SPEC.md`, add this row to the §7 request table directly after the `agent.profiles` row (line 581):

```markdown
| `slash.list` | `profile_id`, `project_id`, `worktree_task_id?` | `cli?` (absent when the profile's CLI is unknown, then `items` is empty), `items[]` (`name` without `/`, `description`, `argument_hint?`, `source`: `built_in`, `project`, `user`, `managed`, `{"plugin": name}` or `profile`; `group?` on the omp `skill:` row), `errors[]` (files that could not be read); the `/` commands of the profile's agent CLI in the project folder, or the task's worktree, read on the daemon's machine |
```

And this line after the `Protocol 51 (port manager, spec 2026-11-01)` line (665):

```markdown
Protocol 52 (slash picker, spec 2026-11-04): request `slash.list`.
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-protocol`
Expected: PASS, including `slash_list_request_wire_shape`, `slash_rows_name_their_source_and_omit_what_is_missing`, `slash_list_result_leaves_an_unknown_cli_out` and `every_m1_request_round_trips`.

Run: `cargo check -p mkrew-daemon -p mkrew-cli`
Expected: no errors (the dispatch `match` is exhaustive again).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-protocol/src/message/tests.rs crates/mkrew-daemon/src/server.rs docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(protocol): slash.list request, Cli and SlashItem (protocol 52)"
```

---
### Task 3 (A2): Profile: `cli`, `[slash]`, `infer_cli`

**Files:**
- Modify: `crates/mkrew-core/src/profile.rs` (`AgentProfile` at lines 16-41, `parse_profile` at 287-369, `RawProfile` at 371-392, tests from line 449)
- Modify: `crates/mkrew-core/src/lib.rs:33-36` (re-exports)
- Modify: `agents/claude.toml`, `agents/codex.toml`, `agents/omp.toml`, `agents/copilot.toml` (explicit `cli`)
- Modify: `crates/mkrew-core/src/snapshots/mkrew_core__profile__tests__snapshot_unknown_field.snap` (new key list)

**Interfaces:**
- Consumes: `mkrew_protocol::Cli` (A1).
- Produces: `AgentProfile.cli: Option<Cli>`, `AgentProfile.slash: SlashOverrides`, `pub struct SlashOverrides { pub hide: Vec<String>, pub add: Vec<SlashAdd> }`, `pub struct SlashAdd { pub name: String, pub description: String, pub argument_hint: Option<String> }`, `pub fn infer_cli(command: &str) -> Option<Cli>`.
- Behavior: `cli` is the explicit key, else `infer_cli(&command)`; a shell profile is always `None`, and an explicit `cli` on a shell is a profile error.
  `hide` entries and `[[slash.add]]` names lose a leading `/` and surrounding spaces; an empty `hide` entry or an `add` name with a space is a profile error with line and column.
  `AgentProfile` is built with a struct literal only in `parse_profile`; every other place uses `..` update syntax, so no other file changes.

- [ ] **Step 1: Write the failing tests.**

Append these tests inside `mod tests` of `profile.rs`, directly before its closing `}` (after `usage_format_name_is_the_inverse_of_parse`, line 740):

```rust
    #[test]
    fn cli_is_explicit_or_inferred_from_the_command() {
        let cli = |src: &str| parse(src).unwrap().cli;
        assert_eq!(cli("command = \"claude\""), Some(Cli::Claude));
        assert_eq!(cli("command = \"npx\""), None);
        assert_eq!(cli("command = \"npx\"\ncli = \"omp\""), Some(Cli::Omp));
        assert_eq!(
            cli("command = \"claude\"\ncli = \"codex\""),
            Some(Cli::Codex),
            "the key wins over the program name"
        );
        assert_eq!(cli("kind = \"shell\""), None);
        assert_eq!(
            cli("kind = \"shell\"\ncommand = \"copilot\""),
            None,
            "a shell never infers a CLI"
        );
    }

    #[test]
    fn infer_cli_reads_the_program_name() {
        use Cli::{Claude, Codex, Copilot, Omp};
        for (command, expected) in [
            ("claude", Some(Claude)),
            ("claude.cmd", Some(Claude)),
            (r"C:\x\codex.exe", Some(Codex)),
            (r"C:\Users\me\AppData\Roaming\npm\CODEX.CMD", Some(Codex)),
            ("/usr/local/bin/copilot", Some(Copilot)),
            ("omp.ps1", Some(Omp)),
            ("  Omp.BAT ", Some(Omp)),
            ("npx", None),
            ("claude-code", None),
            ("claude.exe.bak", None),
            ("claude.sh", None),
            ("", None),
        ] {
            assert_eq!(infer_cli(command), expected, "{command:?}");
        }
    }

    #[test]
    fn an_unknown_cli_is_an_error_at_its_value() {
        assert_eq!(
            parse_err("command = \"x\"\ncli = \"gemini\"\n"),
            "agents/broken.toml:2:7: unknown `cli` `gemini`, expected one of `claude`, `codex`, `omp` or `copilot`"
        );
        let shell = parse_err("kind = \"shell\"\ncli = \"claude\"\n");
        assert!(
            shell.ends_with("`cli` is only for agent profiles"),
            "{shell}"
        );
    }

    #[test]
    fn slash_section_parses_hide_and_add() {
        let p = parse(
            r#"command = "claude"
[slash]
hide = ["desktop", "/upgrade"]

[[slash.add]]
name = "deploy"
description = "Deploy to staging"
argument_hint = "<env>"

[[slash.add]]
name = "/ship"
description = " Ship it "
argument_hint = " "
"#,
        )
        .unwrap();
        assert_eq!(p.slash.hide, ["desktop", "upgrade"]);
        assert_eq!(
            p.slash.add,
            [
                SlashAdd {
                    name: "deploy".into(),
                    description: "Deploy to staging".into(),
                    argument_hint: Some("<env>".into()),
                },
                SlashAdd {
                    name: "ship".into(),
                    description: "Ship it".into(),
                    argument_hint: None,
                },
            ]
        );
        assert_eq!(
            parse("command = \"x\"").unwrap().slash,
            SlashOverrides::default()
        );
    }

    #[test]
    fn slash_names_must_be_one_word_and_keys_must_be_known() {
        let hide = parse_err("command = \"x\"\n[slash]\nhide = [\"/\"]\n");
        assert!(hide.starts_with("agents/broken.toml:3:"), "{hide}");
        assert!(hide.ends_with("`[slash] hide` has an empty name"), "{hide}");
        let spaced = parse_err(
            "command = \"x\"\n[[slash.add]]\nname = \"two words\"\ndescription = \"d\"\n",
        );
        assert!(
            spaced.contains("must be one word without spaces"),
            "{spaced}"
        );
        let no_description = parse_err("command = \"x\"\n[[slash.add]]\nname = \"a\"\n");
        assert!(
            no_description.contains("missing field `description`"),
            "{no_description}"
        );
        let typo = parse_err("command = \"x\"\n[slash]\nhidden = [\"a\"]\n");
        assert!(typo.contains("unknown field `hidden`"), "{typo}");
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core profile`
Expected: compile errors, `cannot find type Cli`, `cannot find function infer_cli`, `no field cli on type AgentProfile`.

- [ ] **Step 3: Implement** in `profile.rs` (a unified diff against the current file):

```diff
--- a/crates/mkrew-core/src/profile.rs
+++ b/crates/mkrew-core/src/profile.rs
@@ -9,4 +9,5 @@
 use std::path::{Path, PathBuf};
 
+use mkrew_protocol::Cli;
 use serde::Deserialize;
 use toml::Spanned;
@@ -24,4 +25,7 @@
     /// Program to run. Empty only for [`ProfileKind::Shell`], meaning the OS default shell.
     pub command: String,
+    /// The agent CLI this profile runs (ADR 0094): the `cli` key, else [`infer_cli`] of
+    /// `command`; always `None` for a shell. It picks the `/` commands the prompt boxes list.
+    pub cli: Option<Cli>,
     pub args: Vec<String>,
     pub prompt_mode: PromptMode,
@@ -39,4 +43,44 @@
     /// `[usage]` (SPEC §20E.3); `None` = no usage for this agent.
     pub usage: Option<UsageConfig>,
+    /// `[slash]`: fixes to the `/` command list.
+    pub slash: SlashOverrides,
+}
+
+/// `[slash]` of a profile (spec 2026-11-04 §4.2). Names have no leading `/`.
+#[derive(Debug, Clone, Default, PartialEq, Eq)]
+pub struct SlashOverrides {
+    /// Rows removed by name after every source is merged.
+    pub hide: Vec<String>,
+    /// `[[slash.add]]` rows; they win over every other source of the same name.
+    pub add: Vec<SlashAdd>,
+}
+
+/// One `[[slash.add]]` row: a command the tables and scans do not know.
+#[derive(Debug, Clone, PartialEq, Eq)]
+pub struct SlashAdd {
+    pub name: String,
+    pub description: String,
+    pub argument_hint: Option<String>,
+}
+
+/// The CLI `command` starts, from its program name: the file name, lowercased, without
+/// `.exe`, `.cmd`, `.bat` or `.ps1`. A wrapper (`npx`, a script) infers nothing; set `cli`.
+pub fn infer_cli(command: &str) -> Option<Cli> {
+    let program = command.trim().rsplit(['/', '\\']).next()?.to_lowercase();
+    let stem = [".exe", ".cmd", ".bat", ".ps1"]
+        .iter()
+        .find_map(|ext| program.strip_suffix(ext))
+        .unwrap_or(&program);
+    parse_cli(stem)
+}
+
+fn parse_cli(name: &str) -> Option<Cli> {
+    match name {
+        "claude" => Some(Cli::Claude),
+        "codex" => Some(Cli::Codex),
+        "omp" => Some(Cli::Omp),
+        "copilot" => Some(Cli::Copilot),
+        _ => None,
+    }
 }
 
@@ -331,4 +375,52 @@
     };
 
+    let cli = match raw.cli {
+        Some(value) => {
+            let span = value.span();
+            if raw.kind == ProfileKind::Shell {
+                return Err(err(Some(span), "`cli` is only for agent profiles".into()));
+            }
+            let name = value.into_inner();
+            match parse_cli(name.trim()) {
+                Some(cli) => Some(cli),
+                None => {
+                    return Err(err(
+                        Some(span),
+                        format!(
+                            "unknown `cli` `{name}`, expected one of `claude`, `codex`, `omp` or `copilot`"
+                        ),
+                    ));
+                }
+            }
+        }
+        None if raw.kind == ProfileKind::Agent => infer_cli(&command),
+        None => None,
+    };
+
+    let mut slash = SlashOverrides::default();
+    for name in raw.slash.hide {
+        let span = name.span();
+        let name = command_name(&name.into_inner());
+        if name.is_empty() {
+            return Err(err(Some(span), "`[slash] hide` has an empty name".into()));
+        }
+        slash.hide.push(name);
+    }
+    for add in raw.slash.add {
+        let span = add.name.span();
+        let name = command_name(&add.name.into_inner());
+        if name.is_empty() || name.contains(char::is_whitespace) {
+            return Err(err(
+                Some(span),
+                format!("`[[slash.add]]` name `{name}` must be one word without spaces"),
+            ));
+        }
+        slash.add.push(SlashAdd {
+            name,
+            description: add.description.trim().to_owned(),
+            argument_hint: add.argument_hint.filter(|h| !h.trim().is_empty()),
+        });
+    }
+
     let mut needs_input_patterns = Vec::with_capacity(raw.state.needs_input_patterns.len());
     for pattern in raw.state.needs_input_patterns {
@@ -350,4 +442,5 @@
         kind: raw.kind,
         command,
+        cli,
         args: raw.args,
         prompt_mode: raw.prompt_mode,
@@ -366,5 +459,11 @@
             context_window: u.context_window,
         }),
+        slash,
     })
+}
+
+/// A command name as written in `[slash]`: trimmed, without the `/` a user types.
+fn command_name(written: &str) -> String {
+    written.trim().trim_start_matches('/').to_owned()
 }
 
@@ -377,4 +476,5 @@
     kind: ProfileKind,
     command: Option<Spanned<String>>,
+    cli: Option<Spanned<String>>,
     #[serde(default)]
     args: Vec<String>,
@@ -390,4 +490,23 @@
     state: RawState,
     usage: Option<RawUsage>,
+    #[serde(default)]
+    slash: RawSlash,
+}
+
+#[derive(Default, Deserialize)]
+#[serde(deny_unknown_fields)]
+struct RawSlash {
+    #[serde(default)]
+    hide: Vec<Spanned<String>>,
+    #[serde(default)]
+    add: Vec<RawSlashAdd>,
+}
+
+#[derive(Deserialize)]
+#[serde(deny_unknown_fields)]
+struct RawSlashAdd {
+    name: Spanned<String>,
+    description: String,
+    argument_hint: Option<String>,
 }
 
```

In `crates/mkrew-core/src/lib.rs`, replace the `pub use profile::{...}` block (lines 33-36) with:

```rust
pub use profile::{
    AgentProfile, ProfileError, ProfileKind, ProfileSet, PromptMode, SlashAdd, SlashOverrides,
    StateConfig, StateMode, UsageConfig, UsageFormat, infer_cli, load_dir, parse_profile,
};
```

Each shipped agent states its CLI.
Insert the line right after the `command = ...` line of the four files:

| File | Line to insert after `command = "..."` |
| --- | --- |
| `agents/claude.toml` (line 6, keys are aligned there) | `cli     = "claude"` |
| `agents/codex.toml` (line 4) | `cli = "codex"` |
| `agents/omp.toml` (line 5) | `cli = "omp"` |
| `agents/copilot.toml` (line 5) | `cli = "copilot"` |

`fake.toml` and the shell profiles get no `cli`.

The unknown-field snapshot lists every key, in the order of `RawProfile`.
Replace the last line of `crates/mkrew-core/src/snapshots/mkrew_core__profile__tests__snapshot_unknown_field.snap` with:

```text
agents/broken.toml:2:1: unknown field `promt_mode`, expected one of `name`, `icon`, `kind`, `command`, `cli`, `args`, `prompt_mode`, `env`, `resume`, `oneshot`, `ask`, `ask_web`, `state`, `usage`, `slash`
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core profile`
Expected: PASS.
If `snapshot_unknown_field` still reports a diff (a hand edit that missed the trailing newline), run `cargo insta accept -p mkrew-core` and check with `git diff` that only that `.snap` line changed.

Run: `cargo check -p mkrew-daemon --tests`
Expected: no errors (`launch.rs` builds its profile with `..parsed`).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/profile.rs crates/mkrew-core/src/lib.rs crates/mkrew-core/src/snapshots/mkrew_core__profile__tests__snapshot_unknown_field.snap agents/claude.toml agents/codex.toml agents/omp.toml agents/copilot.toml
git -c user.email=tungnv975@gmail.com commit -m "feat(core): profile cli key, [slash] overrides and infer_cli"
```

---
### Task 4 (A3): `slash::front_matter`

**Files:**
- Create: `crates/mkrew-core/src/slash.rs`
- Create: `crates/mkrew-core/src/slash/front_matter.rs`
- Test: `crates/mkrew-core/src/slash/front_matter/tests.rs`
- Modify: `crates/mkrew-core/src/lib.rs` (module list at lines 8-29, header doc at lines 3-6)

**Interfaces:**
- Produces: `pub mod front_matter { pub struct FrontMatter { pub name: Option<String>, pub description: Option<String>, pub argument_hint: Option<String>, pub user_invocable: Option<bool>, pub when_to_use: Option<String> } pub fn parse(text: &str) -> (Option<FrontMatter>, &str); pub fn first_line(body: &str) -> Option<&str> }`.
- Behavior (spec 5): front matter exists only when the first line is `---` and a closing `---` line follows; a block that is never closed is not front matter.
  Keys are read at top level only, as plain (folded over indented lines), single-quoted, double-quoted, `|` and `>` scalars; `argumentHint` is read as `argument_hint`; nested keys and sequences are ignored.
  A block that does not follow those rules gives a `FrontMatter` with every field `None`, as Claude Code lists such a file.

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-core/src/slash/front_matter/tests.rs`:

```rust
use super::*;

fn fields(text: &str) -> FrontMatter {
    parse(text).0.expect("front matter")
}

#[test]
fn plain_scalars_and_every_key() {
    let f = fields(
        "---\nname: deploy\ndescription: Deploy to staging\nargument-hint: <env>\n\
         user-invocable: false\nwhen_to_use: after a merge\n---\nBody\n",
    );
    assert_eq!(f.name.as_deref(), Some("deploy"));
    assert_eq!(f.description.as_deref(), Some("Deploy to staging"));
    assert_eq!(f.argument_hint.as_deref(), Some("<env>"));
    assert_eq!(f.user_invocable, Some(false));
    assert_eq!(f.when_to_use.as_deref(), Some("after a merge"));
}

#[test]
fn omp_spells_the_hint_in_camel_case() {
    let f = fields("---\nargumentHint: <file>\n---\n");
    assert_eq!(f.argument_hint.as_deref(), Some("<file>"));
}

#[test]
fn body_starts_after_the_closing_line_and_is_not_trimmed() {
    let (f, body) = parse("---\nname: a\n---\n\n  indented\n");
    assert!(f.is_some());
    assert_eq!(body, "\n  indented\n");
}

#[test]
fn no_front_matter_means_the_whole_text_is_the_body() {
    for text in ["Just text\n---\nname: a\n---\n", " ---\nname: a\n---\n", ""] {
        assert_eq!(parse(text), (None, text), "{text:?}");
    }
}

#[test]
fn a_block_that_is_never_closed_is_not_front_matter() {
    let text = "---\nname: a\nBody without a closing line\n";
    assert_eq!(parse(text), (None, text));
}

#[test]
fn crlf_files_and_a_byte_order_mark_parse() {
    let (f, body) = parse("\u{feff}---\r\nname: a\r\ndescription: d\r\n---\r\nBody\r\n");
    let f = f.expect("front matter");
    assert_eq!(f.name.as_deref(), Some("a"));
    assert_eq!(f.description.as_deref(), Some("d"));
    assert_eq!(body, "Body\r\n");
}

#[test]
fn quoted_scalars_decode_their_escapes() {
    let f =
        fields("---\nname: 'it''s'\ndescription: \"say \\\"hi\\\"\\tnow \\\\ \"  # note\n---\n");
    assert_eq!(f.name.as_deref(), Some("it's"));
    assert_eq!(f.description.as_deref(), Some("say \"hi\"\tnow \\ "));
}

#[test]
fn a_hash_after_a_space_starts_a_comment_in_a_plain_scalar_only() {
    let f = fields("---\nname: a#b\ndescription: Fix it #later\nargument-hint: '#id'\n---\n");
    assert_eq!(f.name.as_deref(), Some("a#b"));
    assert_eq!(f.description.as_deref(), Some("Fix it"));
    assert_eq!(f.argument_hint.as_deref(), Some("#id"));
}

#[test]
fn a_colon_inside_a_value_is_kept() {
    let f = fields("---\ndescription: Run gh issue view:* then stop\n---\n");
    assert_eq!(
        f.description.as_deref(),
        Some("Run gh issue view:* then stop")
    );
}

#[test]
fn literal_block_keeps_line_breaks() {
    let f = fields("---\ndescription: |\n  first\n    second\n\n  third\nname: x\n---\n");
    assert_eq!(f.description.as_deref(), Some("first\n  second\n\nthird"));
    assert_eq!(f.name.as_deref(), Some("x"));
}

#[test]
fn folded_block_joins_lines_with_spaces_and_blank_lines_with_breaks() {
    let f = fields("---\ndescription: >-\n  one\n  two\n\n  three\n---\n");
    assert_eq!(f.description.as_deref(), Some("one two\nthree"));
}

#[test]
fn a_plain_value_continues_on_indented_lines() {
    let f = fields("---\ndescription: Use when\n  the build fails\n  twice\nname: x\n---\n");
    assert_eq!(
        f.description.as_deref(),
        Some("Use when the build fails twice")
    );
    let next_line = fields("---\ndescription:\n  starts below\n---\n");
    assert_eq!(next_line.description.as_deref(), Some("starts below"));
}

#[test]
fn nested_keys_and_sequences_are_ignored() {
    let f = fields(
        "---\nmetadata:\n  name: inner\n  description: inner\ntools:\n- a\n- b\nname: outer\n---\n",
    );
    assert_eq!(f.name.as_deref(), Some("outer"));
    assert_eq!(f.description, None);
}

#[test]
fn empty_values_and_other_user_invocable_values_are_none() {
    let f = fields("---\nname:\ndescription: ''\nuser-invocable: maybe\n---\n");
    assert_eq!(f, FrontMatter::default());
    assert_eq!(
        fields("---\nuser-invocable: TRUE\n---\n").user_invocable,
        Some(true)
    );
}

#[test]
fn broken_yaml_lists_the_file_with_no_fields_set() {
    for block in [
        "name: ok\nthis line is not a key\n",
        "name: ok\ndescription: \"never closed\n",
        "name: ok\ndescription: 'x' trailing\n",
    ] {
        let text = format!("---\n{block}---\nBody\n");
        let (f, body) = parse(&text);
        assert_eq!(f, Some(FrontMatter::default()), "{block:?}");
        assert_eq!(body, "Body\n");
    }
}

#[test]
fn first_line_skips_blank_lines_and_trims() {
    assert_eq!(
        first_line("\n  \n  Review the diff  \nmore"),
        Some("Review the diff")
    );
    assert_eq!(first_line(" \n\t\n"), None);
}
```

Create `crates/mkrew-core/src/slash.rs` (it grows in A4 to A10):

```rust
//! The `/` commands an agent CLI shows (spec 2026-11-04 §4, §5): a built-in table per CLI plus
//! the skill, command and prompt files that CLI scans, read from the agent's own machine.
//!
//! The CLI is never started. Every scan reads small files from `roots`, stops at the repo root
//! and keeps what it cannot read as one line in [`SlashList::errors`].

pub mod front_matter;
```

In `crates/mkrew-core/src/lib.rs`, add `pub mod slash;` between `pub mod settings_fields;` and `pub mod state;`, and extend the header doc so the I/O list is true:

```diff
--- a/crates/mkrew-core/src/lib.rs
+++ b/crates/mkrew-core/src/lib.rs
@@ -3,6 +3,7 @@
 //! No async runtime, GPUI or SQLite here (SPEC §4); the only I/O is reading profile and
 //! settings files and the prompt library, plus seeding it (and looking up the platform
 //! directories in [`paths`] and programs on `PATH` in [`lookup`]), and the one-shot calls to the
-//! `tailscale` CLI in [`tailscale`], shared by `mk web`, the daemon and the UI.
+//! `tailscale` CLI in [`tailscale`], shared by `mk web`, the daemon and the UI, and reading the
+//! skill, command and prompt files of an agent CLI in [`slash`].
 
 pub mod conflict;
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core front_matter`
Expected: `error[E0583]: file not found for module front_matter` (`slash.rs` declares it, the file does not exist yet).

- [ ] **Step 3: Implement** `crates/mkrew-core/src/slash/front_matter.rs`:

```rust
//! Front matter of skill, command and prompt files (spec 2026-11-04 §5).
//!
//! A file whose first line is `---` has YAML front matter up to the next `---` line.
//! Only the top-level keys Mkrew needs are read, and only as scalars: plain (also folded over
//! indented continuation lines), `'single'` and `"double"` quoted, and `|` / `>` blocks.
//! A block that is not valid YAML for these rules gives a [`FrontMatter`] with no fields set,
//! as Claude Code lists such a file; a block that is never closed is not front matter at all.

/// The keys read from a front matter block. A key that is missing, empty or not a scalar is `None`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    pub name: Option<String>,
    pub description: Option<String>,
    /// `argument-hint`, or `argumentHint` (omp).
    pub argument_hint: Option<String>,
    /// `user-invocable`; only `true` and `false` count.
    pub user_invocable: Option<bool>,
    pub when_to_use: Option<String>,
}

/// Splits `text` into its front matter and the body after the closing `---` line.
///
/// `None` = the text has no front matter; the body is then the whole text.
pub fn parse(text: &str) -> (Option<FrontMatter>, &str) {
    let rest = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = rest.split_inclusive('\n');
    let Some(first) = lines.next().filter(|first| first.trim_end() == "---") else {
        return (None, text);
    };
    let mut offset = first.len();
    let mut block = Vec::new();
    for line in lines {
        offset += line.len();
        if line.trim_end() == "---" {
            let fields = parse_block(&block).unwrap_or_default();
            return (Some(fields), &rest[offset..]);
        }
        block.push(line.trim_end_matches(['\r', '\n']));
    }
    (None, text)
}

/// The first non-empty line of `body`, trimmed (the description when `description` is missing).
pub fn first_line(body: &str) -> Option<&str> {
    body.lines().map(str::trim).find(|line| !line.is_empty())
}

/// `None` = the block is not valid for the rules in the module docs.
fn parse_block(lines: &[&str]) -> Option<FrontMatter> {
    let mut fields = FrontMatter::default();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        let top_level = !line.starts_with([' ', '\t']);
        if line.trim().is_empty() || line.trim_start().starts_with('#') || !top_level {
            continue;
        }
        let Some((key, inline)) = split_key(line) else {
            if line.starts_with('-') {
                continue; // an item of a sequence under the previous key
            }
            return None;
        };
        // Lines that belong to this key: the indented ones that follow (blank lines too).
        let start = i;
        while i < lines.len() && (lines[i].trim().is_empty() || lines[i].starts_with([' ', '\t'])) {
            i += 1;
        }
        let more = &lines[start..i];
        let value = scalar(inline, more)?;
        let value = (!value.is_empty()).then_some(value);
        match key {
            "name" => fields.name = value,
            "description" => fields.description = value,
            "argument-hint" | "argumentHint" => fields.argument_hint = value,
            "when_to_use" => fields.when_to_use = value,
            "user-invocable" => {
                fields.user_invocable = match value.as_deref().map(str::to_ascii_lowercase) {
                    Some(v) if v == "true" => Some(true),
                    Some(v) if v == "false" => Some(false),
                    _ => None,
                }
            }
            _ => {}
        }
    }
    Some(fields)
}

/// `key: value` or `key:` at the start of a line.
fn split_key(line: &str) -> Option<(&str, &str)> {
    let colon = line
        .find(": ")
        .or_else(|| line.strip_suffix(':').map(str::len))?;
    let key = line[..colon].trim_end();
    (!key.is_empty()).then(|| (key, line[colon + 1..].trim()))
}

/// The value after `key:` plus the indented lines that follow it.
fn scalar(inline: &str, more: &[&str]) -> Option<String> {
    let is_block = inline
        .strip_prefix(['|', '>'])
        .is_some_and(|marks| marks.chars().all(|c| matches!(c, '+' | '-' | '1'..='9')));
    if is_block {
        return Some(block(inline.starts_with('>'), more));
    }
    if let Some(rest) = inline.strip_prefix('"') {
        return quoted(rest, '"');
    }
    if let Some(rest) = inline.strip_prefix('\'') {
        return quoted(rest, '\'');
    }
    // Plain: ` #` starts a comment; indented lines continue the value, folded with spaces.
    let mut words = Vec::new();
    for part in std::iter::once(inline).chain(more.iter().copied()) {
        let part = part.trim();
        let part = if part.starts_with('#') {
            ""
        } else {
            part.split(" #").next().unwrap_or(part).trim_end()
        };
        if !part.is_empty() {
            words.push(part);
        }
    }
    Some(words.join(" "))
}

/// A `|` (keep line breaks) or `>` (fold single breaks into spaces) block, trimmed.
fn block(fold: bool, lines: &[&str]) -> String {
    let indent = lines
        .iter()
        .find(|l| !l.trim().is_empty())
        .map_or(0, |l| l.len() - l.trim_start().len());
    let lines: Vec<&str> = lines
        .iter()
        .map(|l| l.get(indent..).unwrap_or("").trim_end())
        .collect();
    let text = if fold {
        lines
            .split(|l| l.is_empty())
            .map(|paragraph| paragraph.join(" "))
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        lines.join("\n")
    };
    text.trim().to_owned()
}

/// A quoted scalar whose opening `quote` is already consumed; `None` = not closed on this line
/// or followed by anything but a comment.
fn quoted(rest: &str, quote: char) -> Option<String> {
    let mut value = String::new();
    let mut chars = rest.char_indices().peekable();
    while let Some((at, c)) = chars.next() {
        if c == quote {
            if quote == '\'' && matches!(chars.peek(), Some((_, '\''))) {
                chars.next();
                value.push('\'');
                continue;
            }
            let after = rest[at + 1..].trim();
            return (after.is_empty() || after.starts_with('#')).then_some(value);
        }
        if c == '\\' && quote == '"' {
            match chars.next()?.1 {
                'n' => value.push('\n'),
                't' => value.push('\t'),
                '"' => value.push('"'),
                '\\' => value.push('\\'),
                '/' => value.push('/'),
                other => {
                    value.push('\\');
                    value.push(other);
                }
            }
            continue;
        }
        value.push(c);
    }
    None
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core front_matter`
Expected: PASS, 16 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/slash.rs crates/mkrew-core/src/slash/front_matter.rs crates/mkrew-core/src/slash/front_matter/tests.rs crates/mkrew-core/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): front matter parser for skill and command files"
```

---
### Task 5 (A4): Built-in tables and their loader

**Files:**
- Create: `crates/mkrew-core/slash/codex.toml`, `crates/mkrew-core/slash/omp.toml`, `crates/mkrew-core/slash/copilot.toml`, `crates/mkrew-core/slash/claude.toml`
- Create: `crates/mkrew-core/src/slash/tables.rs`
- Test: `crates/mkrew-core/src/slash/tables/tests.rs`
- Modify: `crates/mkrew-core/src/slash.rs` (module list, `Os`)

**Interfaces:**
- Produces: `pub enum Os { Windows, Macos, Linux }` (`Deserialize`, lowercase) in `slash.rs`; in `tables.rs` `pub(super) struct Builtin { pub item: SlashItem, pub bundled: bool }` and `pub(super) fn builtins(cli: Cli, os: Os) -> Result<Vec<Builtin>, String>` (rows of the CLI that exist on `os`, in table order; the error names the table file).
- Table schema (one file per CLI, spec 5.5):

```toml
# header: `cli_version` (Claude Code, omp, Copilot) or `source_commit` (Codex), and `source` (URL)
cli_version = "18.13.2"
source = "https://example.invalid/where-the-rows-came-from"

[[command]]
name = "plan"                      # without the leading `/`; unique in the file, case-insensitively
description = "Toggle plan mode"   # one line
argument_hint = "<description>"    # optional
os = ["windows", "macos"]          # optional; missing or empty = every OS
bundled = true                     # optional, default false: a bundled skill (Claude Code), or the
                                   # markdown fallback `init` (omp)
```

Unknown keys in a row are an error (`deny_unknown_fields`); the header keys are checked by the tests, not by the loader.

**Verify unit:** clippy reports `dead_code` for `tables.rs` until `list` uses it in A5; run clippy after A5, not here.

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-core/src/slash/tables/tests.rs`:

```rust
use super::*;

fn names(rows: &[Builtin]) -> Vec<&str> {
    rows.iter().map(|b| b.item.name.as_str()).collect()
}

const ALL: [(Cli, &str, &str); 4] = [
    (Cli::Claude, CLAUDE, "cli_version"),
    (Cli::Codex, CODEX, "source_commit"),
    (Cli::Omp, OMP, "cli_version"),
    (Cli::Copilot, COPILOT, "cli_version"),
];

/// Spec §5.5: a unit test parses every table and checks unique names.
#[test]
fn every_embedded_table_parses_with_unique_names_and_a_header() {
    for (cli, src, version_key) in ALL {
        let rows = parse(src, None).unwrap_or_else(|e| panic!("{cli:?}: {e}"));
        assert!(!rows.is_empty(), "{cli:?}");
        let mut seen = std::collections::HashSet::new();
        for row in &rows {
            let name = &row.item.name;
            assert!(!name.is_empty() && !name.starts_with('/'), "{cli:?} {name}");
            assert!(!row.item.description.trim().is_empty(), "{cli:?} {name}");
            assert!(
                seen.insert(name.to_lowercase()),
                "{cli:?}: duplicate {name}"
            );
        }
        let header: toml::Table = toml::from_str(src).unwrap();
        let version = header.get(version_key).and_then(toml::Value::as_str);
        assert!(
            version.is_some_and(|v| !v.is_empty()),
            "{cli:?}: {version_key}"
        );
        let url = header.get("source").and_then(toml::Value::as_str);
        assert!(
            url.is_some_and(|u| u.starts_with("https://")),
            "{cli:?}: source"
        );
    }
}

#[test]
fn a_row_with_an_unknown_key_is_an_error() {
    let src = "[[command]]\nname = \"a\"\ndescription = \"d\"\nargument_hnt = \"x\"\n";
    assert!(
        parse(src, None)
            .unwrap_err()
            .to_string()
            .contains("unknown field")
    );
}

#[test]
fn os_filters_rows_and_an_empty_os_means_everywhere() {
    let src = "[[command]]\nname = \"all\"\ndescription = \"d\"\n\
               [[command]]\nname = \"win\"\ndescription = \"d\"\nos = [\"windows\", \"macos\"]\n";
    assert_eq!(
        names(&parse(src, Some(Os::Windows)).unwrap()),
        ["all", "win"]
    );
    assert_eq!(names(&parse(src, Some(Os::Linux)).unwrap()), ["all"]);
}

#[test]
fn codex_rows_follow_the_enum_order_and_leave_out_the_hidden_ones() {
    let rows = builtins(Cli::Codex, Os::Linux).unwrap();
    let names = names(&rows);
    assert_eq!(
        names[..3],
        ["model", "presets", "ide"],
        "daybreak is gated internally"
    );
    let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
    assert!(at("permissions") < at("review") && at("review") < at("compact"));
    for left_out in [
        "debug-config",
        "debug-m-drop",
        "debug-m-update",
        "apps",
        "rollout",
        "test-approval",
        "quit",
        "btw",
        "daybreak",
        "setup-default-sandbox",
        "fast",
        "app",
    ] {
        assert!(!names.contains(&left_out), "{left_out}");
    }
    assert!(
        builtins(Cli::Codex, Os::Macos)
            .unwrap()
            .iter()
            .any(|b| b.item.name == "app")
    );
    let model = &rows[0].item;
    assert_eq!(
        model.description,
        "choose what model and reasoning effort to use"
    );
}

#[test]
fn omp_ends_with_the_bundled_commands_and_a_bundled_init() {
    let rows = builtins(Cli::Omp, Os::Linux).unwrap();
    let names = names(&rows);
    assert_eq!(names[..2], ["security", "settings"]);
    assert_eq!(
        names[names.len() - 4..],
        ["green", "review", "annotate", "init"]
    );
    let bundled: Vec<_> = rows
        .iter()
        .filter(|b| b.bundled)
        .map(|b| &b.item.name[..])
        .collect();
    assert_eq!(bundled, ["init"]);
}

#[test]
fn claude_rows_are_sorted_mark_bundled_skills_and_limit_desktop_to_two_platforms() {
    let rows = builtins(Cli::Claude, Os::Linux).unwrap();
    let names = names(&rows);
    assert!(names.windows(2).all(|p| p[0] < p[1]), "sorted and unique");
    assert!(!names.contains(&"desktop"), "windows and macos only");
    assert!(
        builtins(Cli::Claude, Os::Windows)
            .unwrap()
            .iter()
            .any(|b| b.item.name == "desktop")
    );
    for left_out in [
        "heapdump",
        "setup-bedrock",
        "setup-vertex",
        "new",
        "pr-comments",
    ] {
        assert!(!names.contains(&left_out), "{left_out}");
    }
    let bundled = |n: &str| rows.iter().find(|b| b.item.name == n).map(|b| b.bundled);
    assert_eq!(bundled("batch"), Some(true));
    assert_eq!(bundled("compact"), Some(false));
    let add_dir = rows.iter().find(|b| b.item.name == "add-dir").unwrap();
    assert_eq!(add_dir.item.argument_hint.as_deref(), Some("<path>"));
}

#[test]
fn copilot_keeps_the_reference_order_and_an_experimental_tag() {
    let rows = builtins(Cli::Copilot, Os::Linux).unwrap();
    let names = names(&rows);
    assert_eq!(names[..3], ["add-dir", "after", "agent"]);
    assert_eq!(names[names.len() - 3..], ["fork", "worktree", "move"]);
    let after = rows.iter().find(|b| b.item.name == "after").unwrap();
    assert!(after.item.description.ends_with("(experimental)"));
}
```

In `slash.rs`, replace the module list and add `Os` (this is the A4 state of the file; the rest of the file arrives in A5):

```diff
--- a/crates/mkrew-core/src/slash.rs
+++ b/crates/mkrew-core/src/slash.rs
@@ -5,3 +5,15 @@
 //! and keeps what it cannot read as one line in [`SlashList::errors`].
 
 pub mod front_matter;
+mod tables;
+
+use serde::Deserialize;
+
+/// The operating system a table row can be limited to (`os = [...]` in `slash/*.toml`).
+#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
+#[serde(rename_all = "lowercase")]
+pub enum Os {
+    Windows,
+    Macos,
+    Linux,
+}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core tables`
Expected: `error[E0583]: file not found for module tables`.

- [ ] **Step 3: Write `codex.toml`** (rows from `slash_command.rs`, exact strings, enum order)

The commit below was `main` of `openai/codex` when this plan was written (2026-11-04).
This is a real step: fetch the current commit, and if it differs, re-verify the rows against the file at that commit.

```bash
git ls-remote https://github.com/openai/codex refs/heads/main
```

Save this verifier outside the repo (for example `$TEMP/verify_codex.py`).
It checks that every description is a string literal of `slash_command.rs` at the file's `source_commit`, that every name is a name of `enum SlashCommand` and that the rows are in enum order:

```python
"""Checks crates/mkrew-core/slash/codex.toml against slash_command.rs at its source_commit.

Usage: python verify_codex.py crates/mkrew-core/slash/codex.toml [local slash_command.rs]
"""
import re
import sys
import tomllib
import urllib.request

table = tomllib.load(open(sys.argv[1], "rb"))
if len(sys.argv) > 2:
    src = open(sys.argv[2], encoding="utf-8").read()
else:
    url = ("https://raw.githubusercontent.com/openai/codex/%s/codex-rs/tui/src/slash_command.rs"
           % table["source_commit"])
    src = urllib.request.urlopen(url).read().decode("utf-8")

# Enum order is popup order. A variant's name is `to_string`, else its first `serialize`, else
# its kebab-cased identifier.
body = re.search(r"pub enum SlashCommand \{(.*?)\n\}", src, re.S).group(1)
names, attrs = [], []
for line in body.splitlines():
    line = line.strip()
    if not line or line.startswith("//"):
        continue
    if line.startswith("#["):
        attrs.append(line)
        continue
    ident = line.rstrip(",")
    named = [m for a in attrs for m in re.findall(r'(?:to_string|serialize) = "([^"]+)"', a)]
    to_string = [m for a in attrs for m in re.findall(r'to_string = "([^"]+)"', a)]
    names.append((to_string or named or [re.sub(r"(?<!^)(?=[A-Z])", "-", ident).lower()])[0])
    attrs = []

rows = table["command"]
position = {name: i for i, name in enumerate(names)}
problems = []
for row in rows:
    if row["name"] not in position:
        problems.append("not in the enum: " + row["name"])
    if '"%s"' % row["description"] not in src:
        problems.append("description not in the source: " + row["name"])
order = [position[r["name"]] for r in rows if r["name"] in position]
if order != sorted(order):
    problems.append("rows are not in enum order")
print("\n".join(problems) or "ok: %d rows match %s" % (len(rows), table["source_commit"][:10]))
sys.exit(1 if problems else 0)
```

Create `crates/mkrew-core/slash/codex.toml` with this content.
Set `source_commit` to the hash `git ls-remote` printed (it is the one below when `main` has not moved).
Rows left out on purpose are named in the header comment: they are the ones the Codex popup never shows.
`app` is the only row with `os`, because the source builds it for macOS and Windows only (`cfg!`).

```toml
# Built-in `/` commands of the OpenAI Codex CLI, in the order of `enum SlashCommand` in
# codex-rs/tui/src/slash_command.rs (its comment: "DO NOT ALPHA-SORT! Enum order is presentation
# order in the popup"). Descriptions are the strings of `SlashCommand::description()`.
# Left out as the Codex popup leaves them out: names starting with `debug`, `apps`, `rollout`,
# `test-approval`, the aliases `quit` and `btw`, and `daybreak` and `setup-default-sandbox`
# (their gates are internal). Service-tier commands (`/fast`) come from the model catalog at
# run time and are not listed. Codex skills live in its `$` popup, not in `/`.
source_commit = "ac558e2a3264e1025fd8025fdd71d03dfbe8c86e"
source = "https://github.com/openai/codex/blob/main/codex-rs/tui/src/slash_command.rs"

[[command]]
name = "model"
description = "choose what model and reasoning effort to use"

[[command]]
name = "presets"
description = "run or save reusable prompts and slash commands"

[[command]]
name = "ide"
description = "include current selection, open files, and other context from your IDE"

[[command]]
name = "permissions"
description = "choose what Codex is allowed to do"

[[command]]
name = "keymap"
description = "remap TUI shortcuts"

[[command]]
name = "vim"
description = "toggle Vim mode for the composer"

[[command]]
name = "experimental"
description = "toggle experimental features"

[[command]]
name = "approve"
description = "approve one retry of a recent auto-review denial"

[[command]]
name = "memories"
description = "configure memory use and generation"

[[command]]
name = "skills"
description = "use skills to improve how Codex performs specific tasks"

[[command]]
name = "import"
description = "import setup, this project, and recent chats from Claude Code"

[[command]]
name = "hooks"
description = "view and manage lifecycle hooks"

[[command]]
name = "review"
description = "review my current changes and find issues"

[[command]]
name = "rename"
description = "rename the current thread"

[[command]]
name = "new"
description = "start a new chat during a conversation"

[[command]]
name = "archive"
description = "archive this session"

[[command]]
name = "delete"
description = "permanently delete this session"

[[command]]
name = "resume"
description = "resume a saved chat"

[[command]]
name = "fork"
description = "fork the current chat"

[[command]]
name = "worktree"
description = "start or continue a conversation in a new worktree"

[[command]]
name = "app"
description = "continue this session in the Desktop app"
os = ["macos", "windows"]

[[command]]
name = "init"
description = "create an AGENTS.md file with instructions for Codex"

[[command]]
name = "compact"
description = "summarize conversation to prevent hitting the context limit"

[[command]]
name = "config"
description = "configure Codex preferences"

[[command]]
name = "recap"
description = "summarize the current conversation now"

[[command]]
name = "plan"
description = "switch to Plan mode"

[[command]]
name = "voice"
description = "start or stop voice; use /voice settings to choose a voice"

[[command]]
name = "goal"
description = "set or view the goal for a long-running task"

[[command]]
name = "agents"
description = "open the agent command center"

[[command]]
name = "side"
description = "start a side conversation in an ephemeral fork"

[[command]]
name = "copy"
description = "copy the last response or part of it"

[[command]]
name = "export"
description = "export the conversation as markdown"

[[command]]
name = "raw"
description = "toggle raw scrollback mode for copy-friendly terminal selection"

[[command]]
name = "tui"
description = "choose the TUI mode for the next launch"

[[command]]
name = "diff"
description = "show git diff (including untracked files)"

[[command]]
name = "mention"
description = "mention a file"

[[command]]
name = "status"
description = "show current session configuration and token usage"

[[command]]
name = "daemon"
description = "Manage the local background server"

[[command]]
name = "warnings"
description = "view retained warnings and diagnostic details"

[[command]]
name = "cd"
description = "change the current working directory"

[[command]]
name = "pwd"
description = "show the current working directory"

[[command]]
name = "usage"
description = "view account usage or use a usage limit reset"

[[command]]
name = "title"
description = "configure which items appear in the terminal title"

[[command]]
name = "statusline"
description = "configure which items appear in the status line"

[[command]]
name = "theme"
description = "choose a syntax highlighting theme"

[[command]]
name = "pets"
description = "choose or hide the terminal pet"

[[command]]
name = "mcp"
description = "list MCP tools; use /mcp verbose or /mcp login <name>"

[[command]]
name = "plugins"
description = "browse plugins"

[[command]]
name = "logout"
description = "log out of Codex"

[[command]]
name = "exit"
description = "exit Codex"

[[command]]
name = "feedback"
description = "send logs to maintainers"

[[command]]
name = "ps"
description = "list background terminals"

[[command]]
name = "stop"
description = "stop all background terminals"

[[command]]
name = "clear"
description = "clear the terminal and start a new chat"

[[command]]
name = "subagents"
description = "switch between this session's subagents"
```

Run the verifier, then the quick parse check:

```bash
python "$TEMP/verify_codex.py" crates/mkrew-core/slash/codex.toml
python -c "import sys, tomllib; t = tomllib.load(open(sys.argv[1], 'rb')); n = [c['name'] for c in t['command']]; assert len(n) == len(set(n)); print(len(n), 'rows')" crates/mkrew-core/slash/codex.toml
```

Expected: `ok: 55 rows match ac558e2a32` (or the new hash prefix), then `55 rows`.
If the verifier names a changed description or a new variant, fix that row from the fetched source (it is the authority) and keep the enum order.

- [ ] **Step 4: Write `omp.toml`** (rows from the omp source at tag `v18.13.2`, the version installed here)

The rows below were read from the files `slash-commands/builtin-{modes,collaboration,session,lifecycle,marketplace,skills,control}.ts` at that tag, in registry order, plus the three bundled custom commands and the markdown `init`.
The last four rows are `green`, `review`, `annotate` (descriptions from `extensibility/custom-commands/bundled/{ci-green,review,annotate}/index.ts`, registered in `extensibility/custom-commands/loader.ts` lines 154-178) and `init` (from `prompts/agents/init.md`, the embedded template `EMBEDDED_COMMAND_TEMPLATES`).
`unpin` is not in this table: it does not exist at the tag.

Save this verifier outside the repo (for example `$TEMP/verify_omp.py`).
It downloads those files and checks names, order and descriptions (three descriptions embed a key hint and are compared up to it):

```python
"""Checks crates/mkrew-core/slash/omp.toml against the omp source at tag v<cli_version>.

Usage: python verify_omp.py crates/mkrew-core/slash/omp.toml
"""
import re
import sys
import tomllib
import urllib.request

table = tomllib.load(open(sys.argv[1], "rb"))
base = ("https://raw.githubusercontent.com/can1357/oh-my-pi/v%s/packages/coding-agent/src/"
        % table["cli_version"])


def fetch(path):
    return urllib.request.urlopen(base + path).read().decode("utf-8")


registry = "".join(
    fetch("slash-commands/builtin-%s.ts" % part)
    for part in ["modes", "collaboration", "session", "lifecycle", "marketplace", "skills", "control"]
)
# `name:` at entry indent, in registry order; subcommand entries are indented deeper.
source_names = re.findall(r'^\t\{\n\t\tname: "([^"]+)"', registry, re.M)
bundled = {
    "green": ("extensibility/custom-commands/bundled/ci-green/index.ts", "description = "),
    "review": ("extensibility/custom-commands/bundled/review/index.ts", "description = "),
    "annotate": ("extensibility/custom-commands/bundled/annotate/index.ts", "description = "),
}
rows = table["command"]
problems = []
names = [r["name"] for r in rows if r["name"] not in bundled and r["name"] != "init"]
if names != source_names:
    problems.append("registry names differ:\n  table  %s\n  source %s" % (names, source_names))
for row in rows:
    name, description = row["name"], row["description"]
    if name in bundled:
        path, key = bundled[name]
        if '%s"%s"' % (key, description) not in fetch(path):
            problems.append("bundled description differs: " + name)
    elif name == "init":
        if "description: " + description not in fetch("prompts/agents/init.md"):
            problems.append("init description differs")
    elif description not in registry:
        # Getters that embed a key hint: compare the text before the first hint.
        head = re.split(r"[;(]", description)[0] if name in ("switch", "effort") else description[:60]
        if head not in registry:
            problems.append("description not in the source: " + name)
print("\n".join(problems) or "ok: %d rows match v%s" % (len(rows), table["cli_version"]))
sys.exit(1 if problems else 0)
```

Create `crates/mkrew-core/slash/omp.toml`:

```toml
# Built-in `/` commands of Oh My Pi (omp), in registry order: BUILTIN_SLASH_COMMAND_REGISTRY in
# packages/coding-agent/src/slash-commands/builtin-registry.ts = modes, collaboration, session,
# lifecycle, marketplace, skills, control. Descriptions are the static `description` of each
# entry; the live status text omp shows for some of them is not known without running omp.
# Aliases (`models`, `providers`, `status`, ...) are not rows.
# After the registry come the bundled custom commands `green`, `review` and `annotate`
# (extensibility/custom-commands/loader.ts, `loadBundledCommands`), then the last row, `init`:
# `bundled = true` here means "embedded markdown command, listed only when no file command has
# the same name" (extensibility/slash-commands.ts, `EMBEDDED_COMMAND_TEMPLATES`).
# Read from the upstream tag v18.13.2 (the installed omp is a minified binary). Em dashes of the
# source are written `-`; key hints (`Esc`, `alt+p`, `shift+tab`) are as the default bindings show them.
cli_version = "18.13.2"
source = "https://github.com/can1357/oh-my-pi/blob/v18.13.2/packages/coding-agent/src/slash-commands/builtin-registry.ts"

[[command]]
name = "security"
description = "Plan, run, inspect, import, and compare OMP-native security scans"

[[command]]
name = "settings"
description = "Open settings menu"

[[command]]
name = "setup"
description = "Open provider setup"

[[command]]
name = "plan"
description = "Toggle plan mode (agent plans before executing)"

[[command]]
name = "plan-review"
description = "Re-open the plan review for the latest plan (plan mode only)"

[[command]]
name = "vibe"
description = "Toggle vibe mode (direct persistent fast/good worker sessions; read-only toolset)"

[[command]]
name = "goal"
description = "Toggle goal mode (persistent autonomous objective for this session)"

[[command]]
name = "guided-goal"
description = "Have the agent interview you in chat, then set up goal mode"

[[command]]
name = "loop"
description = "Toggle loop mode. While enabled, the next prompt you send re-submits after every yield. Bound it with a count/duration, or gate it with `--until '<cmd>'` / `--while '<cmd>'` - the command's exit status decides whether the next iteration runs. Esc suspends the ongoing loop; /loop again to disable."

[[command]]
name = "queue"
description = "Queue a message for after the agent yields"

[[command]]
name = "model"
description = "Switch model for this session"

[[command]]
name = "switch"
description = "Switch model for this session (same as alt+p); accepts fuzzy ids, provider/id, @role, :level"

[[command]]
name = "fast"
description = "Toggle fast service (OpenAI service_tier=priority or ultrafast, Anthropic speed=fast, Google priority)"

[[command]]
name = "slow"
description = "Toggle slow mode: flex tier on OpenAI/Google; on Anthropic, continue at low priority after the Claude session limit"

[[command]]
name = "skillful"
description = "Toggle listing available skills in the system prompt (session only)"

[[command]]
name = "extended-context"
description = "Toggle extended context windows"

[[command]]
name = "computer"
description = "Toggle the native computer-use eval prelude for this session"

[[command]]
name = "ratchet"
description = "Build (or reuse) an eval for an LLM flow, then hillclimb it unattended"

[[command]]
name = "prewalk"
description = "Arm, restart, or cancel a one-shot model handoff"

[[command]]
name = "modelpreset"
description = "Save and switch model presets (role models + thinking level)"

[[command]]
name = "effort"
description = "Set reasoning effort (thinking level, intelligence) for this session; shift+tab cycles levels"

[[command]]
name = "advisor"
description = "Toggle the advisor (a second model that reviews each turn and injects notes)"

[[command]]
name = "export"
description = "Export session to HTML file"

[[command]]
name = "trace"
description = "Open this session's trace in the stats dashboard"

[[command]]
name = "dump"
description = "Copy session transcript to clipboard (and write LLM request JSON to tmp)"

[[command]]
name = "share"
description = "Share session via an encrypted link (share server or secret gist)"

[[command]]
name = "collab"
description = "Share this session live via a relay"

[[command]]
name = "join"
description = "Join a shared collab session"

[[command]]
name = "leave"
description = "Leave the collab session"

[[command]]
name = "browser"
description = "Toggle browser eval-prelude headless vs visible mode"

[[command]]
name = "copy"
description = "Pick text or code from the conversation to copy"

[[command]]
name = "open"
description = "Open the last link from the conversation in your browser (or pick one with /copy)"

[[command]]
name = "todo"
description = "View or modify the agent's todo list"

[[command]]
name = "session"
description = "Session management commands"

[[command]]
name = "jobs"
description = "Show async background jobs status"

[[command]]
name = "usage"
description = "Show provider usage and limits"

[[command]]
name = "stats"
description = "Launch the local stats dashboard"

[[command]]
name = "changelog"
description = "Show changelog entries"

[[command]]
name = "hotkeys"
description = "Show all keyboard shortcuts"

[[command]]
name = "tools"
description = "Show tools currently visible to the agent"

[[command]]
name = "context"
description = "Browse context usage and detailed content"

[[command]]
name = "extensions"
description = "Open Extension Control Center dashboard"

[[command]]
name = "agents"
description = "Open the agents hub (per-agent model, prewalk, and advisor)"

[[command]]
name = "git"
description = "Open the git UI (split diff viewer, staging, commit composer)"

[[command]]
name = "local"
description = "Open a session local:// file in $EDITOR (picker when no name given)"

[[command]]
name = "hub"
description = "Open the live Agent Hub (subagents, including finished ones)"

[[command]]
name = "branch"
description = "Rewind to a previous message, keeping the old path as a branch"

[[command]]
name = "fork"
description = "Fork this session, or open it in a multiplexer pane or window"

[[command]]
name = "tree"
description = "Navigate session tree (switch branches)"

[[command]]
name = "login"
description = "Login with OAuth provider"

[[command]]
name = "logout"
description = "Logout from OAuth provider"

[[command]]
name = "mcp"
description = "Manage MCP servers (add, list, remove, test)"

[[command]]
name = "ssh"
description = "Manage SSH hosts (add, list, remove)"

[[command]]
name = "new"
description = "Start a new session"

[[command]]
name = "fresh"
description = "Reset provider stream state without changing the local transcript"

[[command]]
name = "clear"
description = "Clear the conversation context in place, keeping the session"

[[command]]
name = "delete"
description = "Delete the current session and start a new one"

[[command]]
name = "compact"
description = "Manually compact the session context"

[[command]]
name = "shake"
description = "Drop heavy content from context (tool results, large blocks)"

[[command]]
name = "handoff"
description = "Summarize the session into a handoff document and compact in place"

[[command]]
name = "resume"
description = "Resume a different session"

[[command]]
name = "pin"
description = "Pin or unpin a session at the top of the resume list"

[[command]]
name = "btw"
description = "Ask a side question, or browse this session's BTW history"

[[command]]
name = "tan"
description = "Run a full background agent on tangential work"

[[command]]
name = "omfg"
description = "Forge a TTSR rule from a complaint to stop a recurring behavior"

[[command]]
name = "cleanse"
description = "Detect and fix project diagnostics with weighted parallel subagents"

[[command]]
name = "retry"
description = "Retry the last failed agent turn"

[[command]]
name = "debug"
description = "Open debug tools selector"

[[command]]
name = "memory"
description = "Inspect and operate memory maintenance"

[[command]]
name = "rename"
description = "Rename the current session (omit title to generate)"

[[command]]
name = "move"
description = "Move the current session to a different directory"

[[command]]
name = "wt"
description = "Move this session into a new worktree, changes included"

[[command]]
name = "add-dir"
description = "Add a workspace directory to this session (multi-root)"

[[command]]
name = "remove-dir"
description = "Remove a workspace directory from this session"

[[command]]
name = "dirs"
description = "List this session's workspace directories"

[[command]]
name = "exit"
description = "Exit the application"

[[command]]
name = "restart"
description = "Restart omp with the same launch flags, resuming this session"

[[command]]
name = "marketplace"
description = "Manage marketplace plugin sources and installed plugins"

[[command]]
name = "plugins"
description = "View and manage installed plugins"

[[command]]
name = "reload-plugins"
description = "Reload all plugins (skills, commands, hooks, tools, agents, MCP)"

[[command]]
name = "skills"
description = "Search, install, and update skills from the skills.omp.sh registry"

[[command]]
name = "force"
description = "Force next turn to use a specific tool"

[[command]]
name = "live"
description = "Start Codex-backed realtime voice mode"

[[command]]
name = "record"
description = "Start or stop recording this screen to a replayable file (omp play)"

[[command]]
name = "pause"
description = "Freeze all agents (main, subagents, advisor) until resumed"

[[command]]
name = "forget-approvals"
description = "Forget every file and folder approval Ask and remember has recorded"

[[command]]
name = "quit"
description = "Quit the application"

[[command]]
name = "green"
description = "Generate a prompt to iterate on CI failures until the branch is green"

[[command]]
name = "review"
description = "Launch interactive code review"

[[command]]
name = "annotate"
description = "Annotate a diff or text from a file, prompt, latest reply, or session"

[[command]]
name = "init"
description = "Generate AGENTS.md for current codebase"
bundled = true
```

Run:

```bash
python "$TEMP/verify_omp.py" crates/mkrew-core/slash/omp.toml
python -c "import sys, tomllib; t = tomllib.load(open(sys.argv[1], 'rb')); n = [c['name'] for c in t['command']]; assert len(n) == len(set(n)); print(len(n), 'rows')" crates/mkrew-core/slash/omp.toml
```

Expected: `ok: 91 rows match v18.13.2`, then `91 rows`.

- [ ] **Step 5: Write `copilot.toml`** (fetch the CLI command reference, extract the rows, write TOML)

Source: the table "Slash commands in the interactive interface" of `https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference` (its Markdown source is `https://raw.githubusercontent.com/github/docs/main/content/copilot/reference/copilot-cli-reference/cli-command-reference.md`).
The docs table is not the whole popup (Copilot is closed source, `/help` has the full list), so the table keeps the reference order and the docs wording.

Row rules, so the result is reproducible:
- One row per table row whose first cell starts with a command; a later row of an already seen name (`/limits set ...`, `/skills list`, `/plugin install ...`, `/worktree new`) is skipped.
- `name` is the first command of the first cell, aliases are not rows.
- `argument_hint` is the text after that name inside the first backticks, with `\|` read as `|`; absent when empty.
- `description` is the first sentence of the "Purpose" cell, with links reduced to their text, `kbd` and backticks removed, Liquid `{% data ... %}` tags replaced by their plain name (`GitHub`, `Copilot`) and the experimental tag replaced by the suffix ` (experimental)`.

Schema and three fully written rows (the first three of the page as of 2026-11-04):

```toml
# Built-in `/` commands of the GitHub Copilot CLI, in the order of "Slash commands in the
# interactive interface" of the CLI command reference. The docs say the list is not complete
# ("For a complete list ... enter /help"); Copilot is closed source, so the real popup order is
# not known. A row is the first sentence of the docs "Purpose" cell; aliases (`/yolo`, `/quit`,
# ...) and subcommand rows (`/skills list`) are not rows; experimental commands end with
# " (experimental)". Written by a script that reads the page named in `source`.
cli_version = "1.0.99"
source = "https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference"

[[command]]
name = "add-dir"
description = "Allow file access to a directory and load its skills (.github/, .agents/, .claude/) and agents (.github/, .claude/) as trusted configurations"
argument_hint = "PATH"

[[command]]
name = "agent"
description = "Browse and select from available agents (if any)"

[[command]]
name = "after"
description = "Schedule a non-recurring prompt, skill, or schedulable slash command for the current session (for example, /after 30m remind me the time or /after 1h /chronicle standup) (experimental)"
argument_hint = "[DELAY PROMPT]"
```

Save this extractor outside the repo (for example `$TEMP/gen_copilot.py`) and run it; it fetches the page and writes the whole file:

```python
"""Writes crates/mkrew-core/slash/copilot.toml from the Copilot CLI command reference."""
import re, sys, urllib.request

URL = ("https://raw.githubusercontent.com/github/docs/main/content/copilot/reference/"
       "copilot-cli-reference/cli-command-reference.md")
OUT = sys.argv[1]
text = urllib.request.urlopen(URL).read().decode("utf-8")
section = text.split("## Slash commands in the interactive interface", 1)[1].split("\n## ", 1)[0]

NAMES = {
    "variables.product.github": "GitHub",
    "variables.product.prodname_copilot_short": "Copilot",
    "variables.copilot.github_copilot_app": "GitHub Copilot app",
}

def liquid(match):
    key = match.group(1).strip()
    return NAMES.get(key, key.rsplit(".", 1)[-1].replace("_", " "))

def clean(cell):
    cell = re.sub(r"\{% data (?!reusables)(.*?) %\}", liquid, cell)
    cell = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", cell)
    cell = re.sub(r"</?kbd>", "", cell).replace("`", "").replace("\\|", "|")
    return cell.replace("\u2014", " - ").strip()

def first_sentence(cell):
    experimental = "reusables.copilot.experimental" in cell
    cell = clean(re.sub(r"\{% data reusables\.copilot\.experimental %\}", "", cell))
    sentence = re.split(r"(?<=[a-z)])\. ", cell, maxsplit=1)[0].rstrip(".")
    return sentence + (" (experimental)" if experimental else "")

rows, seen = [], set()
for line in section.splitlines():
    if not line.startswith("| `/"):
        continue
    cells = re.split(r"(?<!\\)\|", line)
    command, purpose = cells[1].strip(), cells[2].strip()
    first = re.match(r"`/([a-z0-9-]+)([^`]*)`", command)
    name, args = first.group(1), first.group(2).replace("\\|", "|").strip()
    if name in seen:  # `/limits set ...`, `/skills list`: one row per name
        continue
    seen.add(name)
    rows.append((name, first_sentence(purpose), args or None))

out = ['''# Built-in `/` commands of the GitHub Copilot CLI, in the order of "Slash commands in the
# interactive interface" of the CLI command reference. The docs say the list is not complete
# ("For a complete list ... enter /help"); Copilot is closed source, so the real popup order is
# not known. A row is the first sentence of the docs "Purpose" cell; aliases (`/yolo`, `/quit`,
# ...) and subcommand rows (`/skills list`) are not rows; experimental commands end with
# " (experimental)". Written by a script that reads the page named in `source`.
cli_version = "1.0.99"
source = "https://docs.github.com/en/copilot/reference/copilot-cli-reference/cli-command-reference"
''']
for name, description, args in rows:
    d = description.replace("\\", "\\\\").replace('"', '\\"')
    out.append(f'\n[[command]]\nname = "{name}"\ndescription = "{d}"\n')
    if args:
        a = args.replace("\\", "\\\\").replace('"', '\\"')
        out.append(f'argument_hint = "{a}"\n')
open(OUT, "w", encoding="utf-8", newline="\n").write("".join(out))
print(len(rows), "rows")
```

```bash
python "$TEMP/gen_copilot.py" crates/mkrew-core/slash/copilot.toml
python -c "import sys, tomllib; t = tomllib.load(open(sys.argv[1], 'rb')); n = [c['name'] for c in t['command']]; assert len(n) == len(set(n)); print(len(n), 'rows')" crates/mkrew-core/slash/copilot.toml
```

Expected: `76 rows` printed twice (the page had 76 distinct commands on 2026-11-04; a different count only means the page changed).
Open the file and check the first three rows against the example above, and that `fork`, `worktree` and `move` are the last three (the docs list them after `/workflows`).
The header's `cli_version = "1.0.99"` is the Copilot CLI version this table is pinned to (spec 5.4); do not change it when the page changes.

- [ ] **Step 6: Write `claude.toml`** (Claude Code built-ins and bundled skills)

Source: the "All commands" table of `https://code.claude.com/docs/en/commands.md`, read on 2026-11-03.
The docs publish a "Purpose" column, not the exact strings the TUI shows.
Spec 5.1 asks for the strings of `supportedCommands()` of Claude Code 2.1.312 when that runs without a model turn.
That needs a running Claude Code and was not available while planning, so each description below is a condensed purpose; the header comment says so.
If the executor can run `supportedCommands()` without a model turn, replace the descriptions with its strings, keep names and order, and update the header comment to say so.

Rules used for the rows: bundled skills (the docs mark them `S`) are `bundled = true`; `/desktop` has `os = ["windows", "macos"]` (the docs say macOS and x64 Windows); rows hidden until typed in full (`H`: `/heapdump`, `/setup-bedrock`, `/setup-vertex`), removed commands and plain aliases are left out; a command the docs call an alias of another (`/cost`, `/stats`, `/ultrareview`) keeps its own row because the TUI lists it.
Multi-name docs rows are split (`/run`, `/run-skill-generator`, `/verify`; `/design`, `/design-login`, `/design-sync`; `/login`, `/logout`).
Rows are sorted by name, the order the picker shows with an empty filter.

Create `crates/mkrew-core/slash/claude.toml`:

```toml
# Built-in commands and bundled skills of Claude Code, from the "All commands" table of
# https://code.claude.com/docs/en/commands.md (read 2026-11-03; changelog head 2.1.317).
# The docs publish a "Purpose" column, not the exact strings the TUI shows, so each description
# is a condensed purpose. Rows are sorted by name (the order the picker shows with an empty
# filter). Left out: aliases that the TUI matches on the filter without a row of their own
# (`/new`, `/reset`, `/review`, `/settings`, `/quit`, ...), the removed `/pr-comments`, `/vim`
# and `/ultraplan`, and the commands the docs call hidden until fully typed (`/heapdump`,
# `/setup-bedrock`, `/setup-vertex`). `bundled = true` marks bundled skills, which
# `disableBundledSkills` hides and a user or project skill of the same name replaces.
# `os` marks the platforms the docs restrict a row to; other conditions are not evaluated.
cli_version = "2.1.312"
source = "https://code.claude.com/docs/en/commands.md"

[[command]]
name = "add-dir"
description = "Add a working directory"
argument_hint = "<path>"

[[command]]
name = "advisor"
description = "Turn the advisor tool on or off"
argument_hint = "[model|off]"

[[command]]
name = "agents"
description = "Show how to manage subagents"

[[command]]
name = "artifact-capabilities"
description = "Artifact runtime reference"
bundled = true

[[command]]
name = "artifact-diagramming"
description = "Diagram guidance for artifacts"
bundled = true

[[command]]
name = "artifacts"
description = "List and attach artifacts"

[[command]]
name = "auto-mode-setup"
description = "Draft autoMode.environment entries"

[[command]]
name = "autocompact"
description = "Set the auto-compact window"
argument_hint = "[auto|tokens]"

[[command]]
name = "autofix-pr"
description = "Start a cloud session that fixes a pull request"
argument_hint = "[prompt]"

[[command]]
name = "background"
description = "Detach the session to the background"
argument_hint = "[prompt]"

[[command]]
name = "batch"
description = "Parallel large-scale change"
argument_hint = "<instruction>"
bundled = true

[[command]]
name = "branch"
description = "Branch the conversation"
argument_hint = "[name]"

[[command]]
name = "btw"
description = "Ask a side question"
argument_hint = "[question]"

[[command]]
name = "bug"
description = "Report a bug"
argument_hint = "[report]"

[[command]]
name = "cd"
description = "Move the session's working directory"
argument_hint = "<path>"

[[command]]
name = "chrome"
description = "Claude in Chrome settings"

[[command]]
name = "claude-api"
description = "Claude API reference and workflows"
argument_hint = "[sub]"
bundled = true

[[command]]
name = "claude-in-chrome"
description = "Do a task in the browser"
argument_hint = "[task]"
bundled = true

[[command]]
name = "clear"
description = "Start a new, empty conversation"
argument_hint = "[name]"

[[command]]
name = "code-review"
description = "Review the diff"
argument_hint = "[...]"
bundled = true

[[command]]
name = "color"
description = "Set the prompt bar color"
argument_hint = "[color]"

[[command]]
name = "compact"
description = "Summarize the context"
argument_hint = "[instructions]"

[[command]]
name = "config"
description = "Open settings"
argument_hint = "[key=value]"

[[command]]
name = "context"
description = "Show the context usage grid"
argument_hint = "[all]"

[[command]]
name = "copy"
description = "Copy a response"
argument_hint = "[N]"

[[command]]
name = "cost"
description = "Alias of /usage"

[[command]]
name = "dataviz"
description = "Chart guidance"
bundled = true

[[command]]
name = "debug"
description = "Debug logging and analysis"
bundled = true

[[command]]
name = "deep-research"
description = "Cited research report"
bundled = true

[[command]]
name = "design"
description = "Claude Design"
argument_hint = "[brief]"
bundled = true

[[command]]
name = "design-login"
description = "Sign in to Claude Design"

[[command]]
name = "design-sync"
description = "Sync Claude Design"
bundled = true

[[command]]
name = "desktop"
description = "Continue in the Desktop app"
os = ["windows", "macos"]

[[command]]
name = "diff"
description = "Show the working-tree diff"

[[command]]
name = "doctor"
description = "Setup checkup"
bundled = true

[[command]]
name = "effort"
description = "Set the effort level"
argument_hint = "[level]"

[[command]]
name = "exit"
description = "Exit"

[[command]]
name = "export"
description = "Export the conversation"
argument_hint = "[file]"

[[command]]
name = "fast"
description = "Fast mode"
argument_hint = "[on|off]"

[[command]]
name = "feedback"
description = "Send product feedback"
argument_hint = "[report]"

[[command]]
name = "fewer-permission-prompts"
description = "Build an allowlist from past prompts"
bundled = true

[[command]]
name = "focus"
description = "Focus view"

[[command]]
name = "fork"
description = "Copy the session to the background"
argument_hint = "[prompt]"

[[command]]
name = "goal"
description = "Goal loop"
argument_hint = "[condition|clear]"

[[command]]
name = "help"
description = "Help"

[[command]]
name = "hooks"
description = "Hook configuration"

[[command]]
name = "ide"
description = "IDE integrations"

[[command]]
name = "import"
description = "Import config from Codex, Gemini, or Cursor"
argument_hint = "[...]"

[[command]]
name = "init"
description = "Create CLAUDE.md"

[[command]]
name = "insights"
description = "Usage report"

[[command]]
name = "install-github-app"
description = "Install the GitHub app"

[[command]]
name = "install-slack-app"
description = "Install the Slack app"

[[command]]
name = "keybindings"
description = "Open the shortcuts file"

[[command]]
name = "list-agents"
description = "List messageable agents"

[[command]]
name = "login"
description = "Sign in"

[[command]]
name = "logout"
description = "Sign out"

[[command]]
name = "loop"
description = "Repeat a prompt"
argument_hint = "[interval] [prompt]"
bundled = true

[[command]]
name = "mcp"
description = "Manage MCP servers"
argument_hint = "[...]"

[[command]]
name = "memory"
description = "Edit CLAUDE.md and auto memory"

[[command]]
name = "mobile"
description = "Mobile app QR code"

[[command]]
name = "model"
description = "Switch model"
argument_hint = "[model]"

[[command]]
name = "output-style"
description = "Set the output style"
argument_hint = "[style]"

[[command]]
name = "passes"
description = "Guest passes"

[[command]]
name = "permissions"
description = "Permission rules"

[[command]]
name = "plan"
description = "Plan mode"
argument_hint = "[description]"

[[command]]
name = "plugin"
description = "Manage plugins"
argument_hint = "[sub]"

[[command]]
name = "plugin-authoring"
description = "Plugin authoring reference"

[[command]]
name = "powerup"
description = "Lessons"

[[command]]
name = "privacy-settings"
description = "Privacy settings"

[[command]]
name = "radio"
description = "Lo-fi radio"

[[command]]
name = "rate-limit-options"
description = "Options when a limit is hit"

[[command]]
name = "recap"
description = "One-line summary of the session"

[[command]]
name = "release-notes"
description = "Show the changelog"

[[command]]
name = "reload-plugins"
description = "Reload plugins"
argument_hint = "[--force]"

[[command]]
name = "reload-skills"
description = "Re-scan skills and commands"

[[command]]
name = "remote-control"
description = "Remote Control"

[[command]]
name = "remote-env"
description = "Cloud environment"

[[command]]
name = "rename"
description = "Rename the session"
argument_hint = "[name]"

[[command]]
name = "restart"
description = "Restart to update"

[[command]]
name = "resume"
description = "Resume a session"
argument_hint = "[session]"

[[command]]
name = "rewind"
description = "Checkpoints"

[[command]]
name = "run"
description = "Run and verify the app"
bundled = true

[[command]]
name = "run-skill-generator"
description = "Generate a run skill"
bundled = true

[[command]]
name = "sandbox"
description = "Sandbox settings"

[[command]]
name = "schedule"
description = "Cloud scheduled tasks"
argument_hint = "[description]"

[[command]]
name = "scroll-speed"
description = "Scroll speed"

[[command]]
name = "security-review"
description = "Security review of the changes"

[[command]]
name = "simplify"
description = "Cleanup review"
argument_hint = "[target]"
bundled = true

[[command]]
name = "skill-doctor"
description = "Skill cost and usage"

[[command]]
name = "skills"
description = "List skills"

[[command]]
name = "slides"
description = "Slide artifact"
bundled = true

[[command]]
name = "stats"
description = "Alias of /usage"

[[command]]
name = "status"
description = "Status tab"

[[command]]
name = "statusline"
description = "Status line"

[[command]]
name = "stickers"
description = "Stickers"

[[command]]
name = "stop"
description = "Stop a background session"

[[command]]
name = "subtask"
description = "Forked subagent"
argument_hint = "<task>"

[[command]]
name = "tasks"
description = "Background work"

[[command]]
name = "team-onboarding"
description = "Onboarding guide"

[[command]]
name = "teleport"
description = "Pull a cloud session"

[[command]]
name = "terminal-setup"
description = "Newline keybinding"

[[command]]
name = "theme"
description = "Theme"

[[command]]
name = "tui"
description = "Choose the renderer"
argument_hint = "[default|fullscreen]"

[[command]]
name = "ultrareview"
description = "Alias of /code-review ultra"

[[command]]
name = "update-config"
description = "Edit settings.json"
bundled = true

[[command]]
name = "upgrade"
description = "Upgrade the plan"

[[command]]
name = "usage"
description = "Cost and limits"

[[command]]
name = "usage-credits"
description = "Usage credits"

[[command]]
name = "verify"
description = "Verify the app"
bundled = true

[[command]]
name = "voice"
description = "Dictation"

[[command]]
name = "web-setup"
description = "GitHub for cloud sessions"

[[command]]
name = "workflow-authoring"
description = "Workflow script reference"
bundled = true

[[command]]
name = "workflows"
description = "Workflow progress view"
```

Run: `python -c "import sys, tomllib; t = tomllib.load(open(sys.argv[1], 'rb')); n = [c['name'] for c in t['command']]; assert len(n) == len(set(n)); print(len(n), 'rows')" crates/mkrew-core/slash/claude.toml`
Expected: `113 rows`.

- [ ] **Step 7: Implement the loader** `crates/mkrew-core/src/slash/tables.rs`:

```rust
//! The built-in rows of each CLI, embedded from `crates/mkrew-core/slash/*.toml` (spec 2026-11-04 §5.5).

use mkrew_protocol::{Cli, SlashItem, SlashSource};
use serde::Deserialize;

use super::Os;

pub(super) const CODEX: &str = include_str!("../../slash/codex.toml");
pub(super) const OMP: &str = include_str!("../../slash/omp.toml");
pub(super) const COPILOT: &str = include_str!("../../slash/copilot.toml");
pub(super) const CLAUDE: &str = include_str!("../../slash/claude.toml");

/// A built-in row and whether the table marks it `bundled` (a bundled skill in Claude Code,
/// the markdown fallback `init` in omp).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Builtin {
    pub item: SlashItem,
    pub bundled: bool,
}

/// The header keys (`cli_version` or `source_commit`, `source`) are for people and are checked
/// by the tests; only the rows are read here.
#[derive(Deserialize)]
struct Table {
    command: Vec<Row>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    name: String,
    description: String,
    #[serde(default)]
    argument_hint: Option<String>,
    #[serde(default)]
    os: Vec<Os>,
    #[serde(default)]
    bundled: bool,
}

fn parse(src: &str, os: Option<Os>) -> Result<Vec<Builtin>, toml::de::Error> {
    let table: Table = toml::from_str(src)?;
    Ok(table
        .command
        .into_iter()
        .filter(|row| os.is_none_or(|os| row.os.is_empty() || row.os.contains(&os)))
        .map(|row| Builtin {
            item: SlashItem {
                name: row.name,
                description: row.description,
                argument_hint: row.argument_hint,
                source: SlashSource::BuiltIn,
                group: None,
            },
            bundled: row.bundled,
        })
        .collect())
}

/// The rows of `cli` that exist on `os`, in table order.
pub(super) fn builtins(cli: Cli, os: Os) -> Result<Vec<Builtin>, String> {
    let (name, src) = match cli {
        Cli::Claude => ("claude", CLAUDE),
        Cli::Codex => ("codex", CODEX),
        Cli::Omp => ("omp", OMP),
        Cli::Copilot => ("copilot", COPILOT),
    };
    parse(src, Some(os)).map_err(|e| format!("built-in table {name}.toml: {e}"))
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 8: Run to see it pass**

Run: `cargo test -p mkrew-core tables`
Expected: PASS, 7 tests (every table parses with unique names and a header, an unknown row key is an error, `os` filtering, and one test per CLI for order and left-out rows).

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-core/slash/codex.toml crates/mkrew-core/slash/omp.toml crates/mkrew-core/slash/copilot.toml crates/mkrew-core/slash/claude.toml crates/mkrew-core/src/slash.rs crates/mkrew-core/src/slash/tables.rs crates/mkrew-core/src/slash/tables/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): built-in slash command tables for Claude, Codex, omp and Copilot"
```

---
### Task 6 (A5): `ScanRoots`, `list` and the profile overrides

**Files:**
- Modify: `crates/mkrew-core/src/slash.rs` (the A4 state shown in A4)
- Create: `crates/mkrew-core/src/slash/testing.rs` (test-only helpers)
- Test: `crates/mkrew-core/src/slash/tests.rs`

**Interfaces:**
- Consumes: `tables::builtins` (A4), `profile::{SlashOverrides, SlashAdd}` (A2), `mkrew_protocol::{Cli, SlashItem, SlashSource}` (A1).
- Produces: `pub struct ScanRoots { pub folder: PathBuf, pub home: PathBuf, pub managed: Vec<PathBuf>, pub os: Os, pub env: BTreeMap<String, String> }`, `ScanRoots::for_machine(folder: PathBuf, home: PathBuf, env: BTreeMap<String, String>) -> Self`, `pub struct SlashList { pub items: Vec<SlashItem>, pub errors: Vec<String> }`, `pub fn list(cli: Cli, roots: &ScanRoots, overrides: &SlashOverrides) -> SlashList`.
- Behavior: in this task `list` returns the built-in rows of the CLI for every CLI (Codex needs nothing more; A7, A9 and A10 route Claude, omp and Copilot through their scanners).
  `[[slash.add]]` rows replace every row of the same name case-insensitively, in place, else go last, with source `Profile`; then `[slash] hide` removes by name, case-insensitively, so a name that is hidden and added is gone.
  Claude rows are sorted by name, case-insensitively, after the overrides.
  A built-in table that fails to parse is one line in `errors`, never a panic.
  `for_machine` takes the managed folder (`C:\Program Files\ClaudeCode`, `/Library/Application Support/ClaudeCode`, `/etc/claude-code`) and the OS from the build target.

- [ ] **Step 1: Write the failing tests and the test helpers**

Create `crates/mkrew-core/src/slash/testing.rs` (a fake home, managed folder and repo in a tempdir):

```rust
//! A fake home and repo in a tempdir for the scanner tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{Os, ScanRoots};

pub(super) struct World {
    _tmp: tempfile::TempDir,
    pub home: PathBuf,
    pub repo: PathBuf,
    pub managed: PathBuf,
}

/// `home/`, `managed/` and `repo/` (with a `.git` folder) in a new tempdir.
pub(super) fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let (home, repo, managed) = (
        tmp.path().join("home"),
        tmp.path().join("repo"),
        tmp.path().join("managed"),
    );
    fs::create_dir_all(repo.join(".git")).unwrap();
    World {
        _tmp: tmp,
        home,
        repo,
        managed,
    }
}

impl World {
    pub fn roots(&self, folder: &Path) -> ScanRoots {
        ScanRoots {
            folder: folder.to_path_buf(),
            home: self.home.clone(),
            managed: vec![self.managed.clone()],
            os: Os::Linux,
            env: BTreeMap::new(),
        }
    }
}

/// Writes `text` to `root/rel`, creating the folders.
pub(super) fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

/// A markdown file with `description` as front matter.
pub(super) fn doc(description: &str) -> String {
    format!("---\ndescription: {description}\n---\nBody\n")
}

/// A skill file with `name` and `description`.
pub(super) fn skill(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\nBody\n")
}
```

Create `crates/mkrew-core/src/slash/tests.rs`:

```rust
use std::collections::BTreeMap;

use mkrew_protocol::{Cli, SlashItem, SlashSource};

use super::testing::{doc, put, skill, world};
use super::*;
use crate::profile::{SlashAdd, SlashOverrides};

fn names(out: &SlashList) -> Vec<&str> {
    out.items.iter().map(|i| i.name.as_str()).collect()
}

fn add(name: &str, description: &str) -> SlashAdd {
    SlashAdd {
        name: name.into(),
        description: description.into(),
        argument_hint: None,
    }
}

#[test]
fn codex_lists_only_its_built_ins_whatever_the_folder_holds() {
    let w = world();
    put(&w.repo, ".claude/commands/mine.md", &doc("x"));
    put(&w.repo, ".agents/skills/s/SKILL.md", &skill("skilled", "x"));
    let out = list(Cli::Codex, &w.roots(&w.repo), &SlashOverrides::default());
    assert!(out.errors.is_empty());
    assert_eq!(names(&out)[..2], ["model", "presets"]);
    assert!(out.items.iter().all(|i| i.source == SlashSource::BuiltIn));
    assert!(!names(&out).contains(&"mine") && !names(&out).contains(&"skilled"));
}

#[test]
fn hide_removes_rows_by_name_case_insensitively() {
    let w = world();
    let hide = |names: &[&str]| SlashOverrides {
        hide: names.iter().map(|n| (*n).to_owned()).collect(),
        add: vec![],
    };
    let out = list(
        Cli::Codex,
        &w.roots(&w.repo),
        &hide(&["MODEL", "review", "nope"]),
    );
    assert!(!names(&out).contains(&"model") && !names(&out).contains(&"review"));
    assert!(names(&out).contains(&"presets"));
}

#[test]
fn an_added_row_replaces_every_source_of_its_name_in_place() {
    let w = world();
    put(&w.repo, ".claude/commands/deploy.md", &doc("file"));
    let overrides = SlashOverrides {
        hide: vec![],
        add: vec![SlashAdd {
            name: "compact".into(),
            description: "Mine".into(),
            argument_hint: Some("<x>".into()),
        }],
    };
    let plain = list(Cli::Claude, &w.roots(&w.repo), &SlashOverrides::default());
    let out = list(Cli::Claude, &w.roots(&w.repo), &overrides);
    assert_eq!(out.items.iter().filter(|i| i.name == "compact").count(), 1);
    let row = out.items.iter().find(|i| i.name == "compact").unwrap();
    assert_eq!(row.source, SlashSource::Profile);
    assert_eq!(
        (row.description.as_str(), row.argument_hint.as_deref()),
        ("Mine", Some("<x>"))
    );
    assert_eq!(names(&out), names(&plain), "same place in the order");
}

#[test]
fn an_added_name_that_no_source_has_goes_last_but_claude_stays_sorted_by_name() {
    let w = world();
    let overrides = SlashOverrides {
        hide: vec![],
        add: vec![add("deploy", "Staging")],
    };
    let codex = list(Cli::Codex, &w.roots(&w.repo), &overrides);
    assert_eq!(names(&codex).last(), Some(&"deploy"));
    let claude = list(Cli::Claude, &w.roots(&w.repo), &overrides);
    let sorted: Vec<_> = {
        let mut v: Vec<_> = names(&claude).iter().map(|n| n.to_lowercase()).collect();
        v.sort();
        v
    };
    assert_eq!(
        names(&claude)
            .iter()
            .map(|n| n.to_lowercase())
            .collect::<Vec<_>>(),
        sorted
    );
    assert!(names(&claude).contains(&"deploy"));
}

#[test]
fn hide_applies_after_the_added_rows_are_merged() {
    let w = world();
    let overrides = SlashOverrides {
        hide: vec!["deploy".into()],
        add: vec![add("deploy", "Staging"), add("keep", "Kept")],
    };
    let out = list(Cli::Copilot, &w.roots(&w.repo), &overrides);
    assert!(!names(&out).contains(&"deploy"));
    assert!(names(&out).contains(&"keep"));
}

#[test]
fn a_cli_with_no_files_still_lists_its_built_ins_in_an_empty_folder() {
    let w = world();
    for cli in [Cli::Claude, Cli::Codex, Cli::Omp, Cli::Copilot] {
        let out = list(cli, &w.roots(&w.repo), &SlashOverrides::default());
        assert!(out.errors.is_empty(), "{cli:?}: {:?}", out.errors);
        assert!(out.items.len() > 40, "{cli:?}");
    }
}

#[test]
fn for_machine_picks_the_managed_folder_and_the_os_of_this_build() {
    let roots = ScanRoots::for_machine("/p".into(), "/h".into(), BTreeMap::new());
    assert_eq!(roots.managed.len(), 1);
    let managed = roots.managed[0].to_string_lossy().into_owned();
    if cfg!(windows) {
        assert_eq!(
            (roots.os, managed.as_str()),
            (Os::Windows, r"C:\Program Files\ClaudeCode")
        );
    } else if cfg!(target_os = "macos") {
        assert_eq!(roots.os, Os::Macos);
    } else {
        assert_eq!(
            (roots.os, managed.as_str()),
            (Os::Linux, "/etc/claude-code")
        );
    }
}

#[test]
fn desktop_shows_only_where_the_docs_say() {
    let w = world();
    let on = |os| {
        let mut roots = w.roots(&w.repo);
        roots.os = os;
        list(Cli::Claude, &roots, &SlashOverrides::default())
            .items
            .iter()
            .any(|i: &SlashItem| i.name == "desktop")
    };
    assert!(on(Os::Windows) && on(Os::Macos) && !on(Os::Linux));
}
```

Add the two test-module lines at the end of `slash.rs`, after the last item:

```rust
#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core slash::tests`
Expected: compile errors, `unresolved import super::*` items `list`, `ScanRoots`, `SlashList` not found.

- [ ] **Step 3: Implement.** `crates/mkrew-core/src/slash.rs` becomes:

```rust
//! The `/` commands an agent CLI shows (spec 2026-11-04 §4, §5): a built-in table per CLI plus
//! the skill, command and prompt files that CLI scans, read from the agent's own machine.
//!
//! The CLI is never started. Every scan reads small files from `roots`, stops at the repo root
//! and keeps what it cannot read as one line in [`SlashList::errors`].

pub mod front_matter;
mod tables;

use std::collections::BTreeMap;
use std::path::PathBuf;

use mkrew_protocol::{Cli, SlashItem, SlashSource};
use serde::Deserialize;

use crate::profile::SlashOverrides;

/// The operating system a table row can be limited to (`os = [...]` in `slash/*.toml`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Os {
    Windows,
    Macos,
    Linux,
}

/// Where a scan looks. Tests build it by hand; the daemon uses [`ScanRoots::for_machine`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanRoots {
    /// The folder the agent runs in: the project, or its task worktree.
    pub folder: PathBuf,
    /// The home folder whose `~/.claude` and friends are read.
    pub home: PathBuf,
    /// Folders holding `.claude/skills` and `managed-settings.json` for an organisation.
    pub managed: Vec<PathBuf>,
    pub os: Os,
    /// The environment the agent runs with (profile `env` over the daemon's): only
    /// `COPILOT_SKILLS_DIRS` is read.
    pub env: BTreeMap<String, String>,
}

impl ScanRoots {
    /// This machine: the Claude Code managed folder and the OS come from the build target.
    pub fn for_machine(folder: PathBuf, home: PathBuf, env: BTreeMap<String, String>) -> Self {
        let (os, managed) = if cfg!(windows) {
            (Os::Windows, r"C:\Program Files\ClaudeCode")
        } else if cfg!(target_os = "macos") {
            (Os::Macos, "/Library/Application Support/ClaudeCode")
        } else {
            (Os::Linux, "/etc/claude-code")
        };
        Self {
            folder,
            home,
            managed: vec![PathBuf::from(managed)],
            os,
            env,
        }
    }
}

/// The rows of one CLI plus the files that could not be read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlashList {
    pub items: Vec<SlashItem>,
    pub errors: Vec<String>,
}

/// What typing `/` shows for `cli` in `roots.folder`, in the order the CLI shows it with an empty
/// filter, with the profile's `[slash]` changes applied.
pub fn list(cli: Cli, roots: &ScanRoots, overrides: &SlashOverrides) -> SlashList {
    let mut errors = Vec::new();
    let builtins = tables::builtins(cli, roots.os).unwrap_or_else(|e| {
        errors.push(e);
        Vec::new()
    });
    let mut items: Vec<SlashItem> = builtins.into_iter().map(|b| b.item).collect();
    apply(&mut items, overrides);
    if cli == Cli::Claude {
        items.sort_by_cached_key(|item| item.name.to_lowercase());
    }
    SlashList { items, errors }
}

/// `[[slash.add]]` rows replace every row of the same name (in place, else at the end), then
/// `[slash] hide` removes by name. Names compare case-insensitively.
fn apply(items: &mut Vec<SlashItem>, overrides: &SlashOverrides) {
    for add in &overrides.add {
        let row = SlashItem {
            name: add.name.clone(),
            description: add.description.clone(),
            argument_hint: add.argument_hint.clone(),
            source: SlashSource::Profile,
            group: None,
        };
        let first = items
            .iter()
            .position(|i| i.name.eq_ignore_ascii_case(&row.name));
        items.retain(|i| !i.name.eq_ignore_ascii_case(&row.name));
        items.insert(first.unwrap_or(items.len()), row);
    }
    items.retain(|i| {
        !overrides
            .hide
            .iter()
            .any(|h| i.name.eq_ignore_ascii_case(h))
    });
}

#[cfg(test)]
mod testing;
#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core slash::tests`
Expected: PASS, 8 tests.

Run: `cargo clippy -p mkrew-core --all-targets -- -D warnings`
Expected: no warnings (`tables` is used now).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/slash.rs crates/mkrew-core/src/slash/testing.rs crates/mkrew-core/src/slash/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): slash::list with ScanRoots and profile overrides"
```

---

### Task 7 (A6): Scan helpers: repo root, linked worktrees, skill and command files

**Files:**
- Create: `crates/mkrew-core/src/slash/scan.rs`
- Test: `crates/mkrew-core/src/slash/scan/tests.rs`
- Modify: `crates/mkrew-core/src/slash.rs` (module list)

**Interfaces:**
- Consumes: `front_matter::{parse, first_line, FrontMatter}` (A3), `testing::{world, put}` (A5).
- Produces (all `pub(super)`):
  `fn project_dirs(folder: &Path) -> Vec<PathBuf>`,
  `fn main_checkout_folder(folder: &Path) -> Option<PathBuf>`,
  `struct Doc { pub key: String, pub fm: FrontMatter }` with `fn description(&self, cut: Option<usize>) -> String`,
  `fn read_doc(path: &Path, key: String, errors: &mut Vec<String>) -> Option<Doc>`,
  `fn doc_from_text(key: String, text: &str) -> Doc`,
  `fn entries(dir: &Path, errors: &mut Vec<String>) -> Vec<(String, PathBuf)>`,
  `fn skill_docs(dir: &Path, errors: &mut Vec<String>) -> Vec<Doc>`,
  `fn command_docs(dir: &Path, recursive: bool, errors: &mut Vec<String>) -> Vec<Doc>`.
- Behavior (the scanning edge, spec 5): the repo root is the nearest ancestor of `folder`, `folder` included, that contains `.git` as a file or a folder; the scan visits `folder` up to and including that root and never above it; without a repo it is `folder` alone.
  A repo root whose `.git` is a file `gitdir: <path>` (a linked worktree) where `<path>/commondir` names the common `.git` folder maps `folder` to the same relative folder in the main checkout; a clone, a bare repo or an unreadable link maps to `None`.
  A skill is `<dir>/<folder>/SKILL.md` and its `key` is the folder name; a command is a `*.md` file (case-insensitive extension) and its `key` is the path below the directory with `:` between the parts and `.md` removed.
  The description is `description`, else `when_to_use`, else the first non-empty body line (cut to `cut` characters plus `...` when given); line breaks become spaces.
  A missing directory is silent; any other read failure is one line in `errors` and the file is skipped.
  Recursion stops at depth 8 so a symlink loop ends.

**Verify unit:** clippy reports `dead_code` here until A7 uses these helpers; run clippy after A7.

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-core/src/slash/scan/tests.rs`:

```rust
use super::*;
use crate::slash::testing::{put, world};

#[test]
fn the_scan_climbs_to_the_repo_root_and_stops_there() {
    let w = world();
    let deep = w.repo.join("a/b");
    std::fs::create_dir_all(&deep).unwrap();
    assert_eq!(
        project_dirs(&deep),
        [deep.clone(), w.repo.join("a"), w.repo.clone()]
    );
    assert_eq!(project_dirs(&w.repo), std::slice::from_ref(&w.repo));
}

#[test]
fn without_a_repo_the_scan_is_the_folder_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let folder = tmp.path().join("loose/inner");
    std::fs::create_dir_all(&folder).unwrap();
    assert_eq!(project_dirs(&folder), [folder]);
}

#[test]
fn a_git_file_marks_a_repo_root_like_a_git_folder() {
    let w = world();
    let tree = w.repo.join("tree");
    put(&tree, ".git", "gitdir: elsewhere\n");
    let inner = tree.join("pkg");
    std::fs::create_dir_all(&inner).unwrap();
    assert_eq!(project_dirs(&inner), [inner.clone(), tree.clone()]);
}

/// A linked worktree of `repo` named `wt`, as `git worktree add` writes it.
fn linked(w: &crate::slash::testing::World, link: &str) -> PathBuf {
    let tree = w.repo.parent().unwrap().join("wt");
    put(&w.repo, ".git/worktrees/wt/commondir", "../..\n");
    put(&tree, ".git", link);
    tree
}

#[test]
fn a_linked_worktree_maps_its_folder_into_the_main_checkout() {
    let w = world();
    let link = format!("gitdir: {}\n", w.repo.join(".git/worktrees/wt").display());
    let tree = linked(&w, &link);
    let folder = tree.join("apps/web");
    std::fs::create_dir_all(&folder).unwrap();
    assert_eq!(main_checkout_folder(&folder), Some(w.repo.join("apps/web")));
    assert_eq!(main_checkout_folder(&tree), Some(w.repo.clone()));
}

#[test]
fn a_relative_gitdir_is_read_from_the_worktree_root() {
    let w = world();
    let tree = linked(&w, "gitdir: ../repo/.git/worktrees/wt\n");
    assert_eq!(main_checkout_folder(&tree), Some(w.repo.clone()));
}

#[test]
fn a_clone_or_a_broken_link_has_no_main_checkout() {
    let w = world();
    assert_eq!(main_checkout_folder(&w.repo), None, "a clone");
    let tree = w.repo.parent().unwrap().join("wt");
    put(&tree, ".git", "gitdir: nowhere/at/all\n");
    assert_eq!(main_checkout_folder(&tree), None, "no commondir");
    put(&tree, ".git", "not a link\n");
    assert_eq!(main_checkout_folder(&tree), None, "not a gitdir line");
}

#[test]
fn a_skill_is_its_folder_and_a_command_is_its_path_joined_by_colons() {
    let w = world();
    put(
        &w.repo,
        "s/alpha/SKILL.md",
        "---\nname: renamed\n---\nUse it\n",
    );
    put(&w.repo, "s/not-a-skill/README.md", "x");
    put(&w.repo, "c/top.md", "Top line\nmore\n");
    put(
        &w.repo,
        "c/frontend/component.md",
        "---\ndescription: Make one\n---\n",
    );
    put(&w.repo, "c/frontend/notes.txt", "x");
    put(&w.repo, "c/UPPER.MD", "x");
    let mut errors = Vec::new();
    let skills = skill_docs(&w.repo.join("s"), &mut errors);
    assert_eq!(skills.len(), 1);
    assert_eq!(
        (skills[0].key.as_str(), skills[0].fm.name.as_deref()),
        ("alpha", Some("renamed"))
    );
    assert_eq!(skills[0].description(None), "Use it");

    let mut keys = |recursive| {
        command_docs(&w.repo.join("c"), recursive, &mut errors)
            .into_iter()
            .map(|d| d.key)
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(true), ["UPPER", "frontend:component", "top"]);
    assert_eq!(keys(false), ["UPPER", "top"]);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn descriptions_fall_back_and_omp_cuts_the_first_line() {
    let w = world();
    put(
        &w.repo,
        "c/a.md",
        "\n\n  A very long first line that goes on and on and on past sixty characters\n",
    );
    put(
        &w.repo,
        "c/b.md",
        "---\nwhen_to_use: After a merge\n---\nBody\n",
    );
    put(
        &w.repo,
        "c/c.md",
        "---\ndescription: >\n  Two\n  lines\n---\n",
    );
    let docs = command_docs(&w.repo.join("c"), false, &mut Vec::new());
    assert_eq!(
        docs[0].description(Some(60)),
        "A very long first line that goes on and on and on past sixty..."
    );
    assert!(docs[0].description(None).ends_with("sixty characters"));
    assert_eq!(docs[1].description(None), "After a merge");
    assert_eq!(docs[2].description(None), "Two lines");
}

#[test]
fn an_unreadable_file_is_one_error_line_and_a_missing_folder_is_silent() {
    let w = world();
    put(&w.repo, "c/ok.md", "fine\n");
    std::fs::write(w.repo.join("c/binary.md"), [0xff, 0xfe, 0x00, 0xc3]).unwrap();
    let mut errors = Vec::new();
    let docs = command_docs(&w.repo.join("c"), false, &mut errors);
    assert_eq!(docs.len(), 1);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("binary.md"), "{errors:?}");
    assert!(command_docs(&w.repo.join("missing"), true, &mut errors).is_empty());
    assert_eq!(errors.len(), 1);
}
```

Add `mod scan;` to the module list of `slash.rs` (after `pub mod front_matter;`):

```diff
--- a/crates/mkrew-core/src/slash.rs
+++ b/crates/mkrew-core/src/slash.rs
@@ -5,6 +5,7 @@
 //! and keeps what it cannot read as one line in [`SlashList::errors`].
 
 pub mod front_matter;
+mod scan;
 mod tables;
 
 use std::collections::BTreeMap;
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core slash::scan`
Expected: `error[E0583]: file not found for module scan`.

- [ ] **Step 3: Implement** `crates/mkrew-core/src/slash/scan.rs`:

```rust
//! File scanning shared by the CLI scanners: the repo root rule, linked worktrees and
//! reading skill and command files (spec 2026-11-04 §5).

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use super::front_matter::{self, FrontMatter};

/// Recursion limit for `**/*.md`, so a symlink loop ends.
const MAX_DEPTH: usize = 8;

/// The folders of a project scan: `folder`, then each parent, up to and including the repo
/// root, which is the nearest of them holding `.git` (a folder for a clone, a file for a linked
/// worktree). Without a repo it is `folder` alone: the scan never climbs past a folder that is
/// not inside a repo.
pub(super) fn project_dirs(folder: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for dir in folder.ancestors() {
        dirs.push(dir.to_path_buf());
        if dir.join(".git").exists() {
            return dirs;
        }
    }
    vec![folder.to_path_buf()]
}

/// `folder` seen from the main checkout when its repo root is a linked worktree (its `.git` is a
/// file `gitdir: <main>/.git/worktrees/<name>` and that folder has a `commondir` file): the same
/// relative folder inside the main checkout. `None` for a normal clone, a bare repo or an
/// unreadable link.
pub(super) fn main_checkout_folder(folder: &Path) -> Option<PathBuf> {
    let root = project_dirs(folder).pop()?;
    let link = fs::read_to_string(root.join(".git")).ok()?;
    let gitdir = Path::new(link.lines().next()?.strip_prefix("gitdir:")?.trim());
    let gitdir = root.join(gitdir); // an absolute `gitdir` replaces `root`
    let common = fs::read_to_string(gitdir.join("commondir")).ok()?;
    let common = lexical(&gitdir.join(common.trim()));
    if common.file_name()? != ".git" {
        return None;
    }
    let relative = folder.strip_prefix(&root).ok()?;
    Some(common.parent()?.join(relative))
}

/// `path` with `.` and `..` resolved by text only (no symlinks, no `\\?\` prefix on Windows).
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// A skill or command file, read once.
pub(super) struct Doc {
    /// A skill's folder name, or a command's path below its folder with `:` between the parts
    /// and `.md` removed (`frontend/component.md` is `frontend:component`).
    pub key: String,
    pub fm: FrontMatter,
    first_line: Option<String>,
}

impl Doc {
    /// The one-line description: `description`, else `when_to_use`, else the first non-empty
    /// body line, cut to `cut` characters plus `...` when given. Line breaks become spaces.
    pub fn description(&self, cut: Option<usize>) -> String {
        let declared = self
            .fm
            .description
            .as_ref()
            .or(self.fm.when_to_use.as_ref());
        if let Some(text) = declared {
            return text.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        let line = self.first_line.as_deref().unwrap_or_default();
        match cut {
            Some(n) if line.chars().count() > n => {
                format!("{}...", line.chars().take(n).collect::<String>())
            }
            _ => line.to_owned(),
        }
    }
}

/// Reads `path`; a failure is one line in `errors` and the file is skipped.
pub(super) fn read_doc(path: &Path, key: String, errors: &mut Vec<String>) -> Option<Doc> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => {
            errors.push(format!("{}: {e}", path.display()));
            return None;
        }
    };
    Some(doc_from_text(key, &text))
}

/// The document `text` (front matter and body) under `key`.
pub(super) fn doc_from_text(key: String, text: &str) -> Doc {
    let (fm, body) = front_matter::parse(text);
    Doc {
        key,
        fm: fm.unwrap_or_default(),
        first_line: front_matter::first_line(body).map(str::to_owned),
    }
}

/// The entries of `dir` by name; a missing folder is empty, any other failure is one line in
/// `errors`.
pub(super) fn entries(dir: &Path, errors: &mut Vec<String>) -> Vec<(String, PathBuf)> {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
            ) =>
        {
            return Vec::new();
        }
        Err(e) => {
            errors.push(format!("{}: {e}", dir.display()));
            return Vec::new();
        }
    };
    let mut found: Vec<_> = read
        .filter_map(Result::ok)
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    found.sort();
    found
}

/// `dir/<folder>/SKILL.md` for every folder, in folder order.
pub(super) fn skill_docs(dir: &Path, errors: &mut Vec<String>) -> Vec<Doc> {
    let mut docs = Vec::new();
    for (name, path) in entries(dir, errors) {
        let file = path.join("SKILL.md");
        if file.is_file() {
            docs.extend(read_doc(&file, name, errors));
        }
    }
    docs
}

/// The `*.md` files of `dir`, and with `recursive` those of its sub folders, in name order.
pub(super) fn command_docs(dir: &Path, recursive: bool, errors: &mut Vec<String>) -> Vec<Doc> {
    let mut docs = Vec::new();
    walk(dir, &mut Vec::new(), recursive, errors, &mut docs);
    docs
}

fn walk(
    dir: &Path,
    parents: &mut Vec<String>,
    recursive: bool,
    errors: &mut Vec<String>,
    docs: &mut Vec<Doc>,
) {
    for (name, path) in entries(dir, errors) {
        if path.is_dir() {
            if recursive && parents.len() < MAX_DEPTH {
                parents.push(name);
                walk(&path, parents, recursive, errors, docs);
                parents.pop();
            }
        } else if let Some(stem) = markdown_stem(&name).filter(|_| path.is_file()) {
            let key = parents
                .iter()
                .map(String::as_str)
                .chain([stem])
                .collect::<Vec<_>>();
            docs.extend(read_doc(&path, key.join(":"), errors));
        }
    }
}

fn markdown_stem(name: &str) -> Option<&str> {
    let stem = name.len().checked_sub(3).filter(|&n| n > 0)?;
    (name.is_char_boundary(stem) && name[stem..].eq_ignore_ascii_case(".md")).then(|| &name[..stem])
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core slash::scan`
Expected: PASS, 9 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/slash.rs crates/mkrew-core/src/slash/scan.rs crates/mkrew-core/src/slash/scan/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): slash scan helpers for repo roots, worktrees and skill files"
```

---
### Task 8 (A7): Claude Code scanner: skills and commands of the managed, user and project scopes

**Files:**
- Create: `crates/mkrew-core/src/slash/claude.rs`
- Test: `crates/mkrew-core/src/slash/claude/tests.rs`
- Modify: `crates/mkrew-core/src/slash.rs` (module list, `list`)

**Interfaces:**
- Consumes: `scan::{project_dirs, main_checkout_folder, skill_docs, command_docs, Doc}` (A6), `tables::Builtin` (A4).
- Produces: `pub(super) fn scan(roots: &ScanRoots, builtins: Vec<Builtin>, errors: &mut Vec<String>) -> Vec<SlashItem>` (unsorted; `list` sorts Claude rows by name).
- Behavior (spec 5.1):
  Sources and chips: `<managed>/.claude/skills/*/SKILL.md` (Managed), `~/.claude/skills/*/SKILL.md` and `~/.claude/commands/**/*.md` (User), and the same two in `folder` and each parent up to the repo root (Project).
  A skill's name is its `name`, else its folder; a command's name is its path below `commands/` joined with `:`.
  A linked worktree with no `.claude/skills` anywhere between `folder` and its repo root also reads the main checkout (the same relative folder up to the main checkout's root), after its own folders (Claude Code 2.1.277).
  Same name, case-insensitively, one row wins: a skill beats a command file whatever the scope, then managed beats user beats project, then the folder nearer the working folder.
  A skill replaces the built-in row of its name (a command file does not: the built-in stays and the file row is dropped).
  `user-invocable: false` rows are left out after the name is resolved, so a hidden skill still owns its name.
  Decisions the spec leaves open, pinned by tests: skill beats command across scopes (`a_skill_beats_a_command_file_even_from_a_lower_scope`), a command file never replaces a built-in (`a_skill_replaces_the_built_in_row_but_a_command_file_does_not`), the nearer folder wins (`the_nearer_folder_wins_between_two_project_skills`).
  Plugins and settings arrive in A8.

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-core/src/slash/claude/tests.rs`:

```rust
use mkrew_protocol::{Cli, SlashSource};

use crate::profile::SlashOverrides;
use crate::slash::testing::{doc, put, skill, world};
use crate::slash::{SlashItem, list};

fn rows(w: &crate::slash::testing::World, folder: &std::path::Path) -> Vec<SlashItem> {
    let out = list(Cli::Claude, &w.roots(folder), &SlashOverrides::default());
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    out.items
}

fn find<'a>(items: &'a [SlashItem], name: &str) -> &'a SlashItem {
    items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("no /{name}"))
}

#[test]
fn claude_lists_rows_by_name_whatever_the_source() {
    let w = world();
    put(&w.home, ".claude/commands/zzz.md", &doc("z"));
    put(&w.repo, ".claude/commands/Aaa.md", &doc("a"));
    put(&w.repo, ".claude/skills/mid/SKILL.md", &skill("Mmm", "m"));
    let lower: Vec<_> = rows(&w, &w.repo)
        .iter()
        .map(|i| i.name.to_lowercase())
        .collect();
    let mut sorted = lower.clone();
    sorted.sort();
    assert_eq!(lower, sorted);
    for name in ["aaa", "mmm", "zzz"] {
        assert!(lower.contains(&name.to_owned()), "{name}");
    }
}

#[test]
fn skill_names_come_from_front_matter_else_the_folder_and_commands_join_paths() {
    let w = world();
    put(
        &w.home,
        ".claude/skills/dir-name/SKILL.md",
        &skill("shown", "A skill"),
    );
    put(
        &w.home,
        ".claude/skills/folder-only/SKILL.md",
        "First body line\n",
    );
    put(&w.repo, ".claude/commands/a/b.md", &doc("Nested command"));
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "shown").description, "A skill");
    assert_eq!(find(&items, "folder-only").description, "First body line");
    assert_eq!(find(&items, "a:b").source, SlashSource::Project);
    assert!(items.iter().all(|i| i.name != "dir-name"));
}

#[test]
fn user_invocable_false_is_left_out() {
    let w = world();
    put(
        &w.repo,
        ".claude/skills/hidden/SKILL.md",
        "---\nuser-invocable: false\n---\nx\n",
    );
    put(
        &w.repo,
        ".claude/skills/shown/SKILL.md",
        "---\nuser-invocable: true\n---\nx\n",
    );
    let items = rows(&w, &w.repo);
    assert!(items.iter().all(|i| i.name != "hidden"));
    find(&items, "shown");
}

#[test]
fn managed_beats_user_beats_project_for_skills_of_one_name() {
    let w = world();
    put(
        &w.managed,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "managed"),
    );
    put(
        &w.home,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "user"),
    );
    put(
        &w.repo,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "project"),
    );
    put(
        &w.home,
        ".claude/skills/two/SKILL.md",
        &skill("two", "user"),
    );
    put(
        &w.repo,
        ".claude/skills/two/SKILL.md",
        &skill("two", "project"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "dup").source, SlashSource::Managed);
    assert_eq!(find(&items, "two").source, SlashSource::User);
    assert_eq!(items.iter().filter(|i| i.name == "dup").count(), 1);
}

#[test]
fn a_skill_beats_a_command_file_even_from_a_lower_scope() {
    let w = world();
    put(&w.home, ".claude/commands/ship.md", &doc("command"));
    put(
        &w.repo,
        ".claude/skills/ship/SKILL.md",
        &skill("ship", "skill"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "ship").description, "skill");
    assert_eq!(find(&items, "ship").source, SlashSource::Project);
}

#[test]
fn names_compare_case_insensitively() {
    let w = world();
    put(
        &w.home,
        ".claude/skills/a/SKILL.md",
        &skill("Deploy", "user"),
    );
    put(&w.repo, ".claude/commands/deploy.md", &doc("project"));
    let items = rows(&w, &w.repo);
    assert_eq!(
        items
            .iter()
            .filter(|i| i.name.eq_ignore_ascii_case("deploy"))
            .count(),
        1
    );
    assert_eq!(find(&items, "Deploy").source, SlashSource::User);
}

#[test]
fn a_skill_replaces_the_built_in_row_but_a_command_file_does_not() {
    let w = world();
    put(
        &w.home,
        ".claude/skills/compact/SKILL.md",
        &skill("compact", "my compact"),
    );
    put(&w.home, ".claude/commands/memory.md", &doc("my memory"));
    let items = rows(&w, &w.repo);
    assert_eq!(items.iter().filter(|i| i.name == "compact").count(), 1);
    assert_eq!(find(&items, "compact").source, SlashSource::User);
    assert_eq!(items.iter().filter(|i| i.name == "memory").count(), 1);
    assert_eq!(find(&items, "memory").source, SlashSource::BuiltIn);
}

#[test]
fn the_scan_reads_each_parent_up_to_the_repo_root_and_no_further() {
    let w = world();
    let folder = w.repo.join("apps/web");
    std::fs::create_dir_all(&folder).unwrap();
    put(
        &w.repo,
        ".claude/skills/at-root/SKILL.md",
        &skill("at-root", "r"),
    );
    put(
        &w.repo,
        "apps/.claude/skills/in-middle/SKILL.md",
        &skill("in-middle", "m"),
    );
    put(
        &w.repo,
        "apps/web/.claude/skills/in-folder/SKILL.md",
        &skill("in-folder", "f"),
    );
    put(
        &w.repo,
        "apps/web/deeper/.claude/skills/below/SKILL.md",
        &skill("below", "b"),
    );
    put(
        w.repo.parent().unwrap(),
        ".claude/skills/above-repo/SKILL.md",
        &skill("above-repo", "a"),
    );
    let names: Vec<_> = rows(&w, &folder).into_iter().map(|i| i.name).collect();
    for name in ["at-root", "in-middle", "in-folder"] {
        assert!(names.contains(&name.to_owned()), "{name}");
    }
    for name in ["below", "above-repo"] {
        assert!(!names.contains(&name.to_owned()), "{name}");
    }
}

#[test]
fn the_nearer_folder_wins_between_two_project_skills() {
    let w = world();
    let folder = w.repo.join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    put(
        &w.repo,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "root"),
    );
    put(
        &folder,
        ".claude/skills/dup/SKILL.md",
        &skill("dup", "near"),
    );
    assert_eq!(find(&rows(&w, &folder), "dup").description, "near");
}

#[test]
fn a_folder_outside_any_repo_scans_only_itself() {
    let w = world();
    let loose = w.repo.parent().unwrap().join("loose/inner");
    put(&loose, ".claude/skills/here/SKILL.md", &skill("here", "h"));
    put(
        loose.parent().unwrap(),
        ".claude/skills/parent/SKILL.md",
        &skill("parent", "p"),
    );
    let names: Vec<_> = rows(&w, &loose).into_iter().map(|i| i.name).collect();
    assert!(names.contains(&"here".to_owned()));
    assert!(!names.contains(&"parent".to_owned()));
}

/// A linked worktree `wt` of `w.repo`; its `.git` file points into `repo/.git/worktrees/wt`.
fn worktree(w: &crate::slash::testing::World) -> std::path::PathBuf {
    let tree = w.repo.parent().unwrap().join("wt");
    put(&w.repo, ".git/worktrees/wt/commondir", "../..\n");
    put(
        &tree,
        ".git",
        &format!("gitdir: {}\n", w.repo.join(".git/worktrees/wt").display()),
    );
    tree
}

#[test]
fn a_linked_worktree_without_claude_skills_reads_the_main_checkout() {
    let w = world();
    let tree = worktree(&w);
    put(
        &w.repo,
        ".claude/skills/untracked/SKILL.md",
        &skill("untracked", "main only"),
    );
    put(&tree, ".claude/commands/local.md", &doc("worktree command"));
    let items = rows(&w, &tree);
    assert_eq!(find(&items, "untracked").description, "main only");
    find(&items, "local");
}

#[test]
fn a_linked_worktree_with_its_own_claude_skills_ignores_the_main_checkout() {
    let w = world();
    let tree = worktree(&w);
    put(
        &w.repo,
        ".claude/skills/untracked/SKILL.md",
        &skill("untracked", "main only"),
    );
    put(
        &tree,
        ".claude/skills/own/SKILL.md",
        &skill("own", "tracked"),
    );
    let names: Vec<_> = rows(&w, &tree).into_iter().map(|i| i.name).collect();
    assert!(names.contains(&"own".to_owned()));
    assert!(!names.contains(&"untracked".to_owned()));
}

#[test]
fn a_clone_never_reads_another_checkout() {
    let w = world();
    let other = w.repo.parent().unwrap().join("other");
    put(&other, ".git/HEAD", "ref: refs/heads/main\n");
    put(
        &other,
        ".claude/skills/elsewhere/SKILL.md",
        &skill("elsewhere", "e"),
    );
    let names: Vec<_> = rows(&w, &w.repo).into_iter().map(|i| i.name).collect();
    assert!(!names.contains(&"elsewhere".to_owned()));
}

#[test]
fn a_file_that_cannot_be_read_is_an_error_line_and_the_rest_still_lists() {
    let w = world();
    put(&w.repo, ".claude/commands/ok.md", &doc("fine"));
    std::fs::write(w.repo.join(".claude/commands/bad.md"), [0xff, 0xfe, 0xfd]).unwrap();
    let out = list(Cli::Claude, &w.roots(&w.repo), &SlashOverrides::default());
    assert_eq!(out.errors.len(), 1, "{:?}", out.errors);
    assert!(out.errors[0].contains("bad.md"));
    find(&out.items, "ok");
}

#[test]
fn a_file_with_broken_front_matter_is_listed_with_no_fields_set() {
    let w = world();
    put(
        &w.repo,
        ".claude/commands/odd.md",
        "---\nthis is not yaml\n---\nFirst line wins\n",
    );
    let item = find(&rows(&w, &w.repo), "odd").clone();
    assert_eq!(item.description, "First line wins");
    assert_eq!(item.argument_hint, None);
}
```

In `slash.rs`, add `mod claude;` before `pub mod front_matter;` and route Claude through its scanner in `list`:

```diff
--- a/crates/mkrew-core/src/slash.rs
+++ b/crates/mkrew-core/src/slash.rs
@@ -4,6 +4,7 @@
 //! The CLI is never started. Every scan reads small files from `roots`, stops at the repo root
 //! and keeps what it cannot read as one line in [`SlashList::errors`].
 
+mod claude;
 pub mod front_matter;
 mod scan;
 mod tables;
@@ -75,7 +76,10 @@
         errors.push(e);
         Vec::new()
     });
-    let mut items: Vec<SlashItem> = builtins.into_iter().map(|b| b.item).collect();
+    let mut items = match cli {
+        Cli::Claude => claude::scan(roots, builtins, &mut errors),
+        Cli::Codex | Cli::Omp | Cli::Copilot => builtins.into_iter().map(|b| b.item).collect(),
+    };
     apply(&mut items, overrides);
     if cli == Cli::Claude {
         items.sort_by_cached_key(|item| item.name.to_lowercase());
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core slash::claude`
Expected: `error[E0583]: file not found for module claude`.

- [ ] **Step 3: Implement** `crates/mkrew-core/src/slash/claude.rs`:

```rust
//! Claude Code (spec 2026-11-04 §5.1): built-ins and bundled skills from the table, plus the
//! skill and command files of the managed, user and project scopes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use mkrew_protocol::{SlashItem, SlashSource};

use super::ScanRoots;
use super::scan::{self, Doc};
use super::tables::Builtin;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Skill,
    Command,
}

/// A skill or command file that may become a row.
struct Candidate {
    kind: Kind,
    /// 0 managed, 1 user, 2 project.
    scope: u8,
    item: SlashItem,
    invocable: bool,
}

/// The rows of Claude Code in `roots.folder`, unordered (the caller sorts by name).
pub(super) fn scan(
    roots: &ScanRoots,
    builtins: Vec<Builtin>,
    errors: &mut Vec<String>,
) -> Vec<SlashItem> {
    let mut found = Vec::new();
    for managed in &roots.managed {
        add_skills(
            &mut found,
            &managed.join(".claude/skills"),
            SlashSource::Managed,
            0,
            errors,
        );
    }
    let user = roots.home.join(".claude");
    add_skills(
        &mut found,
        &user.join("skills"),
        SlashSource::User,
        1,
        errors,
    );
    add_commands(
        &mut found,
        &user.join("commands"),
        SlashSource::User,
        1,
        errors,
    );
    for dir in project_dirs(&roots.folder) {
        let claude = dir.join(".claude");
        add_skills(
            &mut found,
            &claude.join("skills"),
            SlashSource::Project,
            2,
            errors,
        );
        add_commands(
            &mut found,
            &claude.join("commands"),
            SlashSource::Project,
            2,
            errors,
        );
    }
    resolve(found, builtins)
}

/// The folders to scan: `folder` up to the repo root. A linked worktree with no `.claude/skills`
/// on that way also reads the main checkout (Claude Code 2.1.277), after its own folders.
fn project_dirs(folder: &Path) -> Vec<PathBuf> {
    let mut dirs = scan::project_dirs(folder);
    if dirs.iter().any(|dir| dir.join(".claude/skills").is_dir()) {
        return dirs;
    }
    if let Some(main) = scan::main_checkout_folder(folder) {
        dirs.extend(scan::project_dirs(&main));
    }
    dirs
}

fn add_skills(
    found: &mut Vec<Candidate>,
    dir: &Path,
    source: SlashSource,
    scope: u8,
    errors: &mut Vec<String>,
) {
    for doc in scan::skill_docs(dir, errors) {
        let name = doc.fm.name.clone().unwrap_or_else(|| doc.key.clone());
        found.push(candidate(Kind::Skill, scope, name, &doc, source.clone()));
    }
}

fn add_commands(
    found: &mut Vec<Candidate>,
    dir: &Path,
    source: SlashSource,
    scope: u8,
    errors: &mut Vec<String>,
) {
    for doc in scan::command_docs(dir, true, errors) {
        found.push(candidate(
            Kind::Command,
            scope,
            doc.key.clone(),
            &doc,
            source.clone(),
        ));
    }
}

fn candidate(kind: Kind, scope: u8, name: String, doc: &Doc, source: SlashSource) -> Candidate {
    Candidate {
        kind,
        scope,
        invocable: doc.fm.user_invocable != Some(false),
        item: SlashItem {
            name,
            description: doc.description(None),
            argument_hint: doc.fm.argument_hint.clone(),
            source,
            group: None,
        },
    }
}

/// Same name, case-insensitively: a skill beats a command file, then managed beats user beats
/// project, then the folder nearer to the working folder. A skill also replaces the built-in row
/// of its name; a command file never does. Skills with `user-invocable: false` are left out.
fn resolve(mut found: Vec<Candidate>, builtins: Vec<Builtin>) -> Vec<SlashItem> {
    found.sort_by_key(|c| (c.kind, c.scope)); // stable: nearer folders stay first
    let mut seen = HashSet::new();
    found.retain(|c| seen.insert(c.item.name.to_lowercase()));
    let replaced: HashSet<String> = found
        .iter()
        .filter(|c| c.kind == Kind::Skill)
        .map(|c| c.item.name.to_lowercase())
        .collect();
    let builtin_names: HashSet<String> = builtins
        .iter()
        .map(|b| b.item.name.to_lowercase())
        .collect();
    let mut items: Vec<SlashItem> = builtins
        .into_iter()
        .filter(|b| !replaced.contains(&b.item.name.to_lowercase()))
        .map(|b| b.item)
        .collect();
    items.extend(
        found
            .into_iter()
            .filter(|c| c.invocable)
            .filter(|c| {
                c.kind == Kind::Skill || !builtin_names.contains(&c.item.name.to_lowercase())
            })
            .map(|c| c.item),
    );
    items
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core slash`
Expected: PASS (15 new tests in `slash::claude`, and the earlier ones).

Run: `cargo clippy -p mkrew-core --all-targets -- -D warnings`
Expected: no warnings (A6's helpers are used now).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/slash.rs crates/mkrew-core/src/slash/claude.rs crates/mkrew-core/src/slash/claude/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): Claude Code slash scanner for skills and command files"
```

---

### Task 9 (A8): Claude Code plugins and settings

**Files:**
- Create: `crates/mkrew-core/src/slash/claude/settings.rs`, `crates/mkrew-core/src/slash/claude/plugins.rs`
- Modify: `crates/mkrew-core/src/slash/claude.rs` (replaced by the file below)
- Test: `crates/mkrew-core/src/slash/claude/tests.rs` (append)

**Interfaces:**
- Consumes: `scan::{read_doc, doc_from_text, skill_docs, command_docs, Doc}` (A6), `claude::{Candidate, Kind}` (A7).
- Produces: `claude::settings::{Settings, load}` and `claude::plugins::scan`, both `pub(super)`.
- Behavior (spec 5.1):
  Settings files, lowest to highest: `~/.claude/settings.json`, `<folder>/.claude/settings.json`, `<folder>/.claude/settings.local.json`, then `<managed>/managed-settings.json` of each managed folder (the file's place inside the managed folder is an inference: the spec names the file, not its folder).
  `enabledPlugins` and `skillOverrides` merge by key (a later file wins per key); `disableBundledSkills` is the last value set.
  `disableBundledSkills: true` hides the rows the table marks `bundled`; `skillOverrides` `"off"` hides that skill (a file skill or a bundled one), any other value keeps it.
  A missing settings file is skipped; one that is not a JSON object is a line in `errors`.
  Plugins: each entry of `~/.claude/plugins/installed_plugins.json` (`plugins` object, value an array of `{installPath}` or one object) whose `<name>@<marketplace>` key is `true` in `enabledPlugins`, else `defaultEnabled` in its manifest `.claude-plugin/plugin.json`, else off.
  Rows are named `<manifest name>:<name>` (the key's name part when the manifest has none) with source `Plugin(<manifest name>)`; they lose every clash (scope 3).
  Skills: `<installPath>/skills/*/SKILL.md` plus each `skills` path of the manifest (a folder of skill folders, or one skill folder).
  Commands: `<installPath>/commands/*.md` (not recursive), unless the manifest has `commands`, which replaces that folder: a path or an array of paths (a folder of `*.md`, or one `.md` file) or a map `name -> {source | content, description, argumentHint}`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/mkrew-core/src/slash/claude/tests.rs`:

```rust
// --- settings and plugins ---

use crate::slash::testing::World;

fn names(items: &[SlashItem]) -> Vec<String> {
    items.iter().map(|i| i.name.clone()).collect()
}

/// Installs plugin `key` at `home/store/<dir>` and lists it in `installed_plugins.json`.
fn install(w: &World, key: &str, dir: &str, manifest: &str) -> std::path::PathBuf {
    let path = w.home.join("store").join(dir);
    put(&path, ".claude-plugin/plugin.json", manifest);
    let index = w.home.join(".claude/plugins/installed_plugins.json");
    let mut plugins: serde_json::Value = std::fs::read_to_string(&index)
        .map(|t| serde_json::from_str(&t).unwrap())
        .unwrap_or_else(|_| serde_json::json!({"version": 2, "plugins": {}}));
    plugins["plugins"][key] =
        serde_json::json!([{ "scope": "user", "installPath": path.to_str().unwrap() }]);
    put(
        &w.home,
        ".claude/plugins/installed_plugins.json",
        &plugins.to_string(),
    );
    path
}

fn enable(w: &World, switches: &str) {
    put(
        &w.home,
        ".claude/settings.json",
        &format!("{{\"enabledPlugins\": {{{switches}}}}}"),
    );
}

#[test]
fn an_enabled_plugin_lists_its_skills_and_commands_under_its_manifest_name() {
    let w = world();
    let dir = install(&w, "kit@market", "kit", r#"{"name": "toolkit"}"#);
    put(&dir, "skills/deploy/SKILL.md", &skill("deploy", "Ship it"));
    put(
        &dir,
        "commands/lint.md",
        "---\nargument-hint: <path>\n---\nLint files\n",
    );
    put(&dir, "commands/sub/ignored.md", "x");
    enable(&w, r#""kit@market": true"#);
    let items = rows(&w, &w.repo);
    let deploy = find(&items, "toolkit:deploy");
    assert_eq!(deploy.source, SlashSource::Plugin("toolkit".into()));
    assert_eq!(deploy.description, "Ship it");
    let lint = find(&items, "toolkit:lint");
    assert_eq!(
        (lint.description.as_str(), lint.argument_hint.as_deref()),
        ("Lint files", Some("<path>"))
    );
    assert!(
        !names(&items).contains(&"toolkit:sub:ignored".to_owned()),
        "commands/*.md is not recursive"
    );
}

#[test]
fn a_plugin_is_off_unless_a_setting_or_its_manifest_turns_it_on() {
    let w = world();
    for (key, dir, manifest) in [
        ("off@m", "off", r#"{"name": "off"}"#),
        (
            "default@m",
            "default",
            r#"{"name": "default", "defaultEnabled": true}"#,
        ),
        (
            "vetoed@m",
            "vetoed",
            r#"{"name": "vetoed", "defaultEnabled": true}"#,
        ),
    ] {
        let path = install(&w, key, dir, manifest);
        put(&path, "commands/c.md", "x\n");
    }
    enable(&w, r#""vetoed@m": false"#);
    let names = names(&rows(&w, &w.repo));
    assert!(names.contains(&"default:c".to_owned()));
    assert!(!names.contains(&"off:c".to_owned()));
    assert!(!names.contains(&"vetoed:c".to_owned()));
}

#[test]
fn settings_merge_by_key_from_user_to_project_to_local_to_managed() {
    let w = world();
    for key in ["a", "b", "c"] {
        let path = install(
            &w,
            &format!("{key}@m"),
            key,
            &format!(r#"{{"name": "{key}"}}"#),
        );
        put(&path, "commands/c.md", "x\n");
    }
    put(
        &w.home,
        ".claude/settings.json",
        r#"{"enabledPlugins": {"a@m": true, "b@m": true, "c@m": false}}"#,
    );
    put(
        &w.repo,
        ".claude/settings.json",
        r#"{"enabledPlugins": {"b@m": false, "c@m": true}}"#,
    );
    put(
        &w.repo,
        ".claude/settings.local.json",
        r#"{"enabledPlugins": {"b@m": true}}"#,
    );
    put(
        &w.managed,
        "managed-settings.json",
        r#"{"enabledPlugins": {"a@m": false}}"#,
    );
    let names = names(&rows(&w, &w.repo));
    assert!(!names.contains(&"a:c".to_owned()), "managed wins");
    assert!(
        names.contains(&"b:c".to_owned()),
        "local beats project beats user"
    );
    assert!(names.contains(&"c:c".to_owned()), "project beats user");
}

#[test]
fn a_manifest_commands_path_replaces_the_default_folder_and_skills_adds_to_it() {
    let w = world();
    let dir = install(
        &w,
        "kit@m",
        "kit",
        r#"{"name": "kit", "commands": ["./extra", "./single.md"], "skills": "./more"}"#,
    );
    put(&dir, "commands/default.md", "x\n");
    put(&dir, "extra/one.md", "x\n");
    put(&dir, "single.md", "Single\n");
    put(&dir, "skills/base/SKILL.md", &skill("base", "b"));
    put(&dir, "more/added/SKILL.md", &skill("added", "a"));
    enable(&w, r#""kit@m": true"#);
    let names = names(&rows(&w, &w.repo));
    for name in ["kit:one", "kit:single", "kit:base", "kit:added"] {
        assert!(names.contains(&name.to_owned()), "{name}");
    }
    assert!(
        !names.contains(&"kit:default".to_owned()),
        "commands replaces the default folder"
    );
}

#[test]
fn a_manifest_commands_map_gives_names_descriptions_and_hints() {
    let w = world();
    let dir = install(
        &w,
        "kit@m",
        "kit",
        r#"{"name": "kit", "commands": {
            "inline": {"content": "---\ndescription: From content\n---\nbody", "argumentHint": "<x>"},
            "filed": {"source": "./cmds/filed.md"},
            "told": {"source": "./cmds/filed.md", "description": "Declared"}
        }}"#,
    );
    put(&dir, "cmds/filed.md", "From the file\n");
    enable(&w, r#""kit@m": true"#);
    let items = rows(&w, &w.repo);
    let inline = find(&items, "kit:inline");
    assert_eq!(
        (inline.description.as_str(), inline.argument_hint.as_deref()),
        ("From content", Some("<x>"))
    );
    assert_eq!(find(&items, "kit:filed").description, "From the file");
    assert_eq!(find(&items, "kit:told").description, "Declared");
}

#[test]
fn disable_bundled_skills_hides_only_the_bundled_rows() {
    let w = world();
    put(
        &w.home,
        ".claude/settings.json",
        r#"{"disableBundledSkills": true}"#,
    );
    let names = names(&rows(&w, &w.repo));
    assert!(!names.contains(&"simplify".to_owned()), "bundled skill");
    assert!(names.contains(&"compact".to_owned()), "built-in command");
}

#[test]
fn skill_overrides_off_hides_that_skill_and_other_values_keep_it() {
    let w = world();
    put(
        &w.repo,
        ".claude/skills/noisy/SKILL.md",
        &skill("noisy", "n"),
    );
    put(
        &w.repo,
        ".claude/skills/quiet/SKILL.md",
        &skill("quiet", "q"),
    );
    put(
        &w.repo,
        ".claude/skills/terse/SKILL.md",
        &skill("terse", "t"),
    );
    put(
        &w.home,
        ".claude/settings.json",
        r#"{"skillOverrides": {"Noisy": "off", "terse": "name-only", "simplify": "off"}}"#,
    );
    let names = names(&rows(&w, &w.repo));
    assert!(
        !names.contains(&"noisy".to_owned()),
        "key matches case-insensitively"
    );
    assert!(names.contains(&"quiet".to_owned()));
    assert!(names.contains(&"terse".to_owned()));
    assert!(
        !names.contains(&"simplify".to_owned()),
        "a bundled skill too"
    );
}

#[test]
fn a_settings_file_that_does_not_parse_is_an_error_line_and_the_scan_goes_on() {
    let w = world();
    put(&w.home, ".claude/settings.json", "{ not json");
    put(
        &w.repo,
        ".claude/skills/still/SKILL.md",
        &skill("still", "s"),
    );
    let out = list(Cli::Claude, &w.roots(&w.repo), &SlashOverrides::default());
    assert_eq!(out.errors.len(), 1, "{:?}", out.errors);
    assert!(out.errors[0].contains("settings.json"));
    find(&out.items, "still");
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core slash::claude`
Expected: the 8 new tests FAIL (`no /toolkit:deploy`, `no /default:c`-style panics from the `find` helper, and `assertion left == right` on the error count): plugins and settings are not read yet.

- [ ] **Step 3: Implement.**

Create `crates/mkrew-core/src/slash/claude/settings.rs`:

```rust
//! The Claude Code settings that decide which rows show (spec 2026-11-04 §5.1).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use serde_json::Value;

use crate::slash::ScanRoots;

/// `enabledPlugins`, `skillOverrides` and `disableBundledSkills` merged over the settings files.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Settings {
    /// `"<name>@<marketplace>"` to its switch.
    pub enabled_plugins: BTreeMap<String, bool>,
    /// Skill name (lowercase) to its `skillOverrides` value; only `"off"` hides.
    pub skill_overrides: BTreeMap<String, String>,
    pub disable_bundled_skills: bool,
}

impl Settings {
    pub fn skill_is_off(&self, name: &str) -> bool {
        self.skill_overrides
            .get(&name.to_lowercase())
            .is_some_and(|v| v == "off")
    }
}

/// Lowest to highest precedence: user, project, project local, then each managed folder's
/// `managed-settings.json`. `enabledPlugins` and `skillOverrides` merge by key, so a later file
/// wins per key; `disableBundledSkills` is the last one set. A missing file is skipped, one that
/// does not parse is a line in `errors`.
pub(super) fn load(roots: &ScanRoots, errors: &mut Vec<String>) -> Settings {
    let project = roots.folder.join(".claude");
    let mut files: Vec<PathBuf> = vec![
        roots.home.join(".claude/settings.json"),
        project.join("settings.json"),
        project.join("settings.local.json"),
    ];
    files.extend(
        roots
            .managed
            .iter()
            .map(|m| m.join("managed-settings.json")),
    );

    let mut settings = Settings::default();
    for file in files {
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => {
                errors.push(format!("{}: {e}", file.display()));
                continue;
            }
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(map)) => merge(&mut settings, &map),
            Ok(_) => errors.push(format!("{}: not a JSON object", file.display())),
            Err(e) => errors.push(format!("{}: {e}", file.display())),
        }
    }
    settings
}

fn merge(settings: &mut Settings, map: &serde_json::Map<String, Value>) {
    if let Some(Value::Object(plugins)) = map.get("enabledPlugins") {
        for (key, value) in plugins {
            if let Some(on) = value.as_bool() {
                settings.enabled_plugins.insert(key.clone(), on);
            }
        }
    }
    if let Some(Value::Object(overrides)) = map.get("skillOverrides") {
        for (key, value) in overrides {
            if let Some(mode) = value.as_str() {
                settings
                    .skill_overrides
                    .insert(key.to_lowercase(), mode.to_owned());
            }
        }
    }
    if let Some(off) = map.get("disableBundledSkills").and_then(Value::as_bool) {
        settings.disable_bundled_skills = off;
    }
}
```

Create `crates/mkrew-core/src/slash/claude/plugins.rs`:

```rust
//! Claude Code plugins on disk (spec 2026-11-04 §5.1): the enabled entries of
//! `~/.claude/plugins/installed_plugins.json` and their skills and commands.

use std::fs;
use std::io;
use std::path::Path;

use mkrew_protocol::{SlashItem, SlashSource};
use serde_json::Value;

use super::settings::Settings;
use super::{Candidate, Kind};
use crate::slash::ScanRoots;
use crate::slash::scan::{self, Doc};

/// Adds the rows of every enabled plugin to `found`. A plugin is enabled by `enabledPlugins`
/// (`"<name>@<marketplace>"`), else by `defaultEnabled` in its manifest. Names are
/// `<manifest name>:<name>` and the scope is 3, below every other.
pub(super) fn scan(
    roots: &ScanRoots,
    settings: &Settings,
    found: &mut Vec<Candidate>,
    errors: &mut Vec<String>,
) {
    let index = roots.home.join(".claude/plugins/installed_plugins.json");
    let text = match fs::read_to_string(&index) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return,
        Err(e) => {
            errors.push(format!("{}: {e}", index.display()));
            return;
        }
    };
    let installed: Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(e) => {
            errors.push(format!("{}: {e}", index.display()));
            return;
        }
    };
    let Some(plugins) = installed.get("plugins").and_then(Value::as_object) else {
        return;
    };
    for (key, entries) in plugins {
        let entries = match entries {
            Value::Array(list) => list.iter().collect(),
            single => vec![single],
        };
        for entry in entries {
            let Some(install) = entry.get("installPath").and_then(Value::as_str) else {
                continue;
            };
            plugin(Path::new(install), key, settings, found, errors);
        }
    }
}

fn plugin(
    install: &Path,
    key: &str,
    settings: &Settings,
    found: &mut Vec<Candidate>,
    errors: &mut Vec<String>,
) {
    let manifest_file = install.join(".claude-plugin/plugin.json");
    let manifest = match fs::read_to_string(&manifest_file) {
        Ok(text) => serde_json::from_str::<Value>(&text).unwrap_or_else(|e| {
            errors.push(format!("{}: {e}", manifest_file.display()));
            Value::Null
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Value::Null,
        Err(e) => {
            errors.push(format!("{}: {e}", manifest_file.display()));
            Value::Null
        }
    };
    let default_on = manifest
        .get("defaultEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !settings
        .enabled_plugins
        .get(key)
        .copied()
        .unwrap_or(default_on)
    {
        return;
    }
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| key.split('@').next())
        .unwrap_or(key)
        .to_owned();
    let add = |found: &mut Vec<Candidate>, kind: Kind, local: &str, doc: &Doc| {
        let local = match kind {
            Kind::Skill => doc.fm.name.clone().unwrap_or_else(|| local.to_owned()),
            Kind::Command => local.to_owned(),
        };
        found.push(row(
            kind,
            &name,
            &local,
            doc.description(None),
            doc.fm.argument_hint.clone(),
            doc,
        ));
    };

    // Skills: the default folder, plus every `skills` path of the manifest.
    let mut skill_dirs = vec![install.join("skills")];
    skill_dirs.extend(paths(manifest.get("skills")).map(|p| install.join(p)));
    for dir in skill_dirs {
        for doc in scan::skill_docs(&dir, errors) {
            add(found, Kind::Skill, &doc.key, &doc);
        }
        if dir.join("SKILL.md").is_file() {
            let folder = dir.file_name().map(|n| n.to_string_lossy().into_owned());
            let doc = scan::read_doc(&dir.join("SKILL.md"), folder.unwrap_or_default(), errors);
            if let Some(doc) = doc {
                add(found, Kind::Skill, &doc.key, &doc);
            }
        }
    }

    // Commands: `commands` of the manifest replaces the default folder.
    match manifest.get("commands") {
        Some(Value::Object(map)) => {
            for (command, spec) in map {
                let doc = command_spec(install, spec, errors);
                let description = spec
                    .get("description")
                    .and_then(Value::as_str)
                    .map_or_else(|| doc.description(None), str::to_owned);
                let hint = spec
                    .get("argumentHint")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| doc.fm.argument_hint.clone());
                found.push(row(Kind::Command, &name, command, description, hint, &doc));
            }
        }
        Some(paths_value) => {
            for path in paths(Some(paths_value)) {
                let path = install.join(path);
                if path.is_dir() {
                    for doc in scan::command_docs(&path, false, errors) {
                        add(found, Kind::Command, &doc.key, &doc);
                    }
                } else if let Some(stem) =
                    path.file_stem().map(|s| s.to_string_lossy().into_owned())
                    && let Some(doc) = scan::read_doc(&path, stem, errors)
                {
                    add(found, Kind::Command, &doc.key, &doc);
                }
            }
        }
        None => {
            for doc in scan::command_docs(&install.join("commands"), false, errors) {
                add(found, Kind::Command, &doc.key, &doc);
            }
        }
    }
}

/// A manifest `commands` entry `{source | content, ...}` as a document: the file at `source`
/// below `install`, else the inline `content`, else an empty one.
fn command_spec(install: &Path, spec: &Value, errors: &mut Vec<String>) -> Doc {
    let from_file = spec
        .get("source")
        .and_then(Value::as_str)
        .and_then(|source| scan::read_doc(&install.join(source), String::new(), errors));
    from_file.unwrap_or_else(|| {
        let content = spec
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        scan::doc_from_text(String::new(), content)
    })
}

/// A manifest path value: one string or an array of strings, `./` removed.
fn paths(value: Option<&Value>) -> impl Iterator<Item = String> + '_ {
    let list: Vec<&Value> = match value {
        Some(Value::Array(list)) => list.iter().collect(),
        Some(single) => vec![single],
        None => Vec::new(),
    };
    list.into_iter()
        .filter_map(Value::as_str)
        .map(|p| p.trim_start_matches("./").to_owned())
}

fn row(
    kind: Kind,
    plugin: &str,
    local: &str,
    description: String,
    argument_hint: Option<String>,
    doc: &Doc,
) -> Candidate {
    Candidate {
        kind,
        scope: 3,
        invocable: doc.fm.user_invocable != Some(false),
        item: SlashItem {
            name: format!("{plugin}:{local}"),
            description,
            argument_hint,
            source: SlashSource::Plugin(plugin.to_owned()),
            group: None,
        },
    }
}
```

Replace `crates/mkrew-core/src/slash/claude.rs` with:

```rust
//! Claude Code (spec 2026-11-04 §5.1): built-ins and bundled skills from the table, plus the
//! skill and command files of the managed, user and project scopes and of enabled plugins.

mod plugins;
mod settings;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use mkrew_protocol::{SlashItem, SlashSource};

use super::ScanRoots;
use super::scan::{self, Doc};
use super::tables::Builtin;
use settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Skill,
    Command,
}

/// A skill or command file that may become a row.
struct Candidate {
    kind: Kind,
    /// 0 managed, 1 user, 2 project, 3 plugin.
    scope: u8,
    item: SlashItem,
    invocable: bool,
}

/// The rows of Claude Code in `roots.folder`, unordered (the caller sorts by name).
pub(super) fn scan(
    roots: &ScanRoots,
    builtins: Vec<Builtin>,
    errors: &mut Vec<String>,
) -> Vec<SlashItem> {
    let settings = settings::load(roots, errors);
    let mut found = Vec::new();
    for managed in &roots.managed {
        add_skills(
            &mut found,
            &managed.join(".claude/skills"),
            SlashSource::Managed,
            0,
            errors,
        );
    }
    let user = roots.home.join(".claude");
    add_skills(
        &mut found,
        &user.join("skills"),
        SlashSource::User,
        1,
        errors,
    );
    add_commands(
        &mut found,
        &user.join("commands"),
        SlashSource::User,
        1,
        errors,
    );
    for dir in project_dirs(&roots.folder) {
        let claude = dir.join(".claude");
        add_skills(
            &mut found,
            &claude.join("skills"),
            SlashSource::Project,
            2,
            errors,
        );
        add_commands(
            &mut found,
            &claude.join("commands"),
            SlashSource::Project,
            2,
            errors,
        );
    }
    plugins::scan(roots, &settings, &mut found, errors);
    resolve(found, builtins, &settings)
}

/// The folders to scan: `folder` up to the repo root. A linked worktree with no `.claude/skills`
/// on that way also reads the main checkout (Claude Code 2.1.277), after its own folders.
fn project_dirs(folder: &Path) -> Vec<PathBuf> {
    let mut dirs = scan::project_dirs(folder);
    if dirs.iter().any(|dir| dir.join(".claude/skills").is_dir()) {
        return dirs;
    }
    if let Some(main) = scan::main_checkout_folder(folder) {
        dirs.extend(scan::project_dirs(&main));
    }
    dirs
}

fn add_skills(
    found: &mut Vec<Candidate>,
    dir: &Path,
    source: SlashSource,
    scope: u8,
    errors: &mut Vec<String>,
) {
    for doc in scan::skill_docs(dir, errors) {
        let name = doc.fm.name.clone().unwrap_or_else(|| doc.key.clone());
        found.push(candidate(Kind::Skill, scope, name, &doc, source.clone()));
    }
}

fn add_commands(
    found: &mut Vec<Candidate>,
    dir: &Path,
    source: SlashSource,
    scope: u8,
    errors: &mut Vec<String>,
) {
    for doc in scan::command_docs(dir, true, errors) {
        found.push(candidate(
            Kind::Command,
            scope,
            doc.key.clone(),
            &doc,
            source.clone(),
        ));
    }
}

fn candidate(kind: Kind, scope: u8, name: String, doc: &Doc, source: SlashSource) -> Candidate {
    Candidate {
        kind,
        scope,
        invocable: doc.fm.user_invocable != Some(false),
        item: SlashItem {
            name,
            description: doc.description(None),
            argument_hint: doc.fm.argument_hint.clone(),
            source,
            group: None,
        },
    }
}

/// Same name, case-insensitively: a skill beats a command file, then managed beats user beats
/// project, then the folder nearer to the working folder. A skill also replaces the built-in row
/// of its name; a command file never does. Left out: `user-invocable: false`, skills that
/// `skillOverrides` turns `off` (bundled ones too) and, with `disableBundledSkills`, the bundled
/// rows of the table.
fn resolve(
    mut found: Vec<Candidate>,
    builtins: Vec<Builtin>,
    settings: &Settings,
) -> Vec<SlashItem> {
    found.sort_by_key(|c| (c.kind, c.scope)); // stable: nearer folders stay first
    let mut seen = HashSet::new();
    found.retain(|c| seen.insert(c.item.name.to_lowercase()));
    let replaced: HashSet<String> = found
        .iter()
        .filter(|c| c.kind == Kind::Skill)
        .map(|c| c.item.name.to_lowercase())
        .collect();
    let builtin_names: HashSet<String> = builtins
        .iter()
        .map(|b| b.item.name.to_lowercase())
        .collect();
    let mut items: Vec<SlashItem> = builtins
        .into_iter()
        .filter(|b| !replaced.contains(&b.item.name.to_lowercase()))
        .filter(|b| {
            !(b.bundled && (settings.disable_bundled_skills || settings.skill_is_off(&b.item.name)))
        })
        .map(|b| b.item)
        .collect();
    items.extend(
        found
            .into_iter()
            .filter(|c| c.invocable)
            .filter(|c| !(c.kind == Kind::Skill && settings.skill_is_off(&c.item.name)))
            .filter(|c| {
                c.kind == Kind::Skill || !builtin_names.contains(&c.item.name.to_lowercase())
            })
            .map(|c| c.item),
    );
    items
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core slash::claude`
Expected: PASS, 23 tests.

Run: `cargo clippy -p mkrew-core --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/slash/claude.rs crates/mkrew-core/src/slash/claude/settings.rs crates/mkrew-core/src/slash/claude/plugins.rs crates/mkrew-core/src/slash/claude/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): Claude Code plugins, enabledPlugins, skillOverrides and disableBundledSkills"
```

---
### Task 10 (A9): omp scanner

**Files:**
- Create: `crates/mkrew-core/src/slash/omp.rs`
- Test: `crates/mkrew-core/src/slash/omp/tests.rs`
- Modify: `crates/mkrew-core/src/slash.rs` (module list, `list`)

**Interfaces:**
- Consumes: `scan::*` (A6), `tables::Builtin` (A4).
- Produces: `pub(super) fn scan(roots: &ScanRoots, builtins: Vec<Builtin>, errors: &mut Vec<String>) -> Vec<SlashItem>`.
- Behavior (spec 5.3), in the order of the omp popup with an empty filter: built-ins (table order, which ends with `green`, `review`, `annotate` and the markdown fallback `init`), the `skill:` group row followed by its skills, markdown commands, prompt templates.
  Skills, `skill:<name>` (name from `name`, else the folder; a name with `/` or `\` is dropped), first of a name wins by provider priority: native `.omp/skills/*/SKILL.md` in `folder` up to the repo root (project, nearest first) then `~/.omp/agent/skills` (user), omp plugins `~/.omp/plugins/node_modules/*/skills/*/SKILL.md` (chip `Plugin(<package folder>)`), Claude `<folder>/.claude/skills/*/SKILL.md` (the folder only, no parents).
  The group row is named `skill:`, describes itself `N skills` (`1 skill`), has `group: Some(N)` and source `BuiltIn`, and is absent when there are no skills.
  Markdown commands, the file stem is the name, first of a name wins by provider priority omp 100, Claude 80, `.agent(s)` 70, Codex 70, opencode 55: `<folder>/.omp/commands/*.md` (project) then `~/.omp/agent/commands/*.md` (user); `<folder>/.claude/commands/**/*.md` where `foo/bar.md` gives the two rows `bar` and `foo:bar`; `~/.agent/commands` and `~/.agents/commands` (user) then `.agent/commands` and `.agents/commands` in `folder` up to the repo root (project); `<folder>/.codex/commands`; `<folder>/.opencode/commands`.
  Descriptions come from `description`, else the first body line cut to 60 characters plus `...`; `argumentHint` and `argument-hint` both give the hint.
  Prompt templates: `~/.omp/agent/prompts/**/*.md` (user) then `<folder>/.omp/prompts/**/*.md` (project); the name is the file name without `.md` whatever the sub folder; a template whose name another row already has is hidden, two templates may share a name.
  The bundled `init` row is dropped when a markdown command is named `init`.
  The user locations that omp turns on only by a setting (`~/.claude/commands`, `~/.codex/commands`, `~/.opencode/commands`) are not read: omp defaults are used (spec 3).

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-core/src/slash/omp/tests.rs`:

```rust
use mkrew_protocol::{Cli, SlashSource};

use crate::profile::SlashOverrides;
use crate::slash::testing::{World, doc, put, skill, world};
use crate::slash::{SlashItem, list};

fn rows(w: &World, folder: &std::path::Path) -> Vec<SlashItem> {
    let out = list(Cli::Omp, &w.roots(folder), &SlashOverrides::default());
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    out.items
}

fn find<'a>(items: &'a [SlashItem], name: &str) -> &'a SlashItem {
    items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("no /{name}"))
}

fn count(items: &[SlashItem], name: &str) -> usize {
    items.iter().filter(|i| i.name == name).count()
}

fn at(items: &[SlashItem], name: &str) -> usize {
    items
        .iter()
        .position(|i| i.name == name)
        .unwrap_or_else(|| panic!("no /{name}"))
}

#[test]
fn the_order_is_built_ins_bundled_group_skills_commands_templates() {
    let w = world();
    put(&w.repo, ".omp/skills/a/SKILL.md", &skill("alpha", "A"));
    put(&w.repo, ".omp/commands/mine.md", &doc("Mine"));
    put(&w.home, ".omp/agent/prompts/tpl.md", &doc("Template"));
    let items = rows(&w, &w.repo);
    let order = [
        "security",
        "quit",
        "green",
        "annotate",
        "init",
        "skill:",
        "skill:alpha",
        "mine",
        "tpl",
    ];
    let places: Vec<_> = order.iter().map(|n| at(&items, n)).collect();
    assert!(places.windows(2).all(|p| p[0] < p[1]), "{places:?}");
}

#[test]
fn skills_collapse_into_a_group_row_that_counts_them() {
    let w = world();
    put(&w.repo, ".omp/skills/a/SKILL.md", &skill("alpha", "A"));
    put(&w.home, ".omp/agent/skills/b/SKILL.md", &skill("beta", "B"));
    let items = rows(&w, &w.repo);
    let group = find(&items, "skill:");
    assert_eq!(
        (group.group, group.description.as_str()),
        (Some(2), "2 skills")
    );
    assert_eq!(find(&items, "skill:alpha").source, SlashSource::Project);
    assert_eq!(find(&items, "skill:beta").source, SlashSource::User);
    assert_eq!(find(&items, "skill:alpha").group, None);
}

#[test]
fn without_skills_there_is_no_group_row() {
    let w = world();
    assert_eq!(count(&rows(&w, &w.repo), "skill:"), 0);
}

#[test]
fn skill_clashes_go_to_the_provider_priority_and_project_beats_user_inside_omp() {
    let w = world();
    put(
        &w.repo,
        ".claude/skills/c/SKILL.md",
        &skill("dup", "claude"),
    );
    put(
        &w.home,
        ".omp/plugins/node_modules/pkg/skills/p/SKILL.md",
        &skill("dup", "plugin"),
    );
    put(
        &w.home,
        ".omp/agent/skills/u/SKILL.md",
        &skill("dup", "user"),
    );
    put(&w.repo, ".omp/skills/n/SKILL.md", &skill("dup", "project"));
    put(
        &w.home,
        ".omp/agent/skills/x/SKILL.md",
        &skill("lower", "user"),
    );
    put(
        &w.home,
        ".omp/plugins/node_modules/pkg/skills/y/SKILL.md",
        &skill("lower", "plugin"),
    );
    put(
        &w.repo,
        ".claude/skills/z/SKILL.md",
        &skill("lowest", "claude"),
    );
    put(
        &w.home,
        ".omp/plugins/node_modules/pkg/skills/w/SKILL.md",
        &skill("lowest", "plugin"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(count(&items, "skill:dup"), 1);
    assert_eq!(find(&items, "skill:dup").description, "project");
    assert_eq!(find(&items, "skill:lower").description, "user");
    let lowest = find(&items, "skill:lowest");
    assert_eq!(lowest.description, "plugin");
    assert_eq!(lowest.source, SlashSource::Plugin("pkg".into()));
}

#[test]
fn a_skill_name_comes_from_front_matter_else_the_folder_and_slashes_drop_it() {
    let w = world();
    put(&w.repo, ".omp/skills/folder-name/SKILL.md", "Plain skill\n");
    put(&w.repo, ".omp/skills/odd/SKILL.md", &skill("a/b", "x"));
    put(&w.repo, ".omp/skills/odd2/SKILL.md", &skill("a\\b", "x"));
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "skill:folder-name").description, "Plain skill");
    assert!(
        items
            .iter()
            .all(|i| !i.name.contains('/') && !i.name.contains('\\'))
    );
}

#[test]
fn omp_skill_folders_climb_to_the_repo_root_but_claude_skills_do_not() {
    let w = world();
    let folder = w.repo.join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    put(&w.repo, ".omp/skills/a/SKILL.md", &skill("from-root", "r"));
    put(
        &w.repo,
        ".claude/skills/b/SKILL.md",
        &skill("claude-root", "r"),
    );
    let items = rows(&w, &folder);
    find(&items, "skill:from-root");
    assert_eq!(count(&items, "skill:claude-root"), 0);
}

#[test]
fn command_clashes_go_to_the_provider_priority() {
    let w = world();
    put(&w.repo, ".opencode/commands/dup.md", &doc("opencode"));
    put(&w.repo, ".codex/commands/dup.md", &doc("codex"));
    put(&w.repo, ".agents/commands/dup.md", &doc("agents"));
    put(&w.repo, ".claude/commands/dup.md", &doc("claude"));
    put(&w.repo, ".omp/commands/dup.md", &doc("omp"));
    put(&w.repo, ".opencode/commands/low.md", &doc("opencode"));
    put(&w.repo, ".codex/commands/low.md", &doc("codex"));
    put(&w.repo, ".codex/commands/lower.md", &doc("codex"));
    put(&w.repo, ".agents/commands/lower.md", &doc("agents"));
    put(&w.repo, ".claude/commands/lowest.md", &doc("claude"));
    put(&w.repo, ".agent/commands/lowest.md", &doc("agent"));
    let items = rows(&w, &w.repo);
    assert_eq!(count(&items, "dup"), 1);
    assert_eq!(find(&items, "dup").description, "omp");
    assert_eq!(find(&items, "low").description, "codex");
    assert_eq!(find(&items, "lower").description, "agents");
    assert_eq!(find(&items, "lowest").description, "claude");
}

#[test]
fn inside_omp_the_project_beats_the_user_and_inside_agents_the_user_beats_the_project() {
    let w = world();
    put(&w.home, ".omp/agent/commands/a.md", &doc("user"));
    put(&w.repo, ".omp/commands/a.md", &doc("project"));
    put(&w.home, ".agents/commands/b.md", &doc("user"));
    put(&w.repo, ".agents/commands/b.md", &doc("project"));
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "a").description, "project");
    assert_eq!(find(&items, "a").source, SlashSource::Project);
    assert_eq!(find(&items, "b").description, "user");
    assert_eq!(find(&items, "b").source, SlashSource::User);
}

#[test]
fn agents_commands_climb_to_the_repo_root() {
    let w = world();
    let folder = w.repo.join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    put(&w.repo, ".agent/commands/up.md", &doc("up"));
    put(
        w.repo.parent().unwrap(),
        ".agent/commands/past-root.md",
        &doc("no"),
    );
    let items = rows(&w, &folder);
    find(&items, "up");
    assert_eq!(count(&items, "past-root"), 0);
}

#[test]
fn a_claude_command_in_a_folder_gives_the_bare_name_and_the_colon_name() {
    let w = world();
    put(&w.repo, ".claude/commands/foo/bar.md", &doc("Nested"));
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "bar").description, "Nested");
    assert_eq!(find(&items, "foo:bar").description, "Nested");
}

#[test]
fn descriptions_use_camel_case_hints_and_cut_the_first_line_at_sixty_characters() {
    let w = world();
    put(
        &w.repo,
        ".omp/commands/hinted.md",
        "---\nargumentHint: <file>\n---\nOpen a file\n",
    );
    let long = "x".repeat(70);
    put(&w.repo, ".omp/commands/long.md", &format!("{long}\n"));
    let items = rows(&w, &w.repo);
    assert_eq!(
        find(&items, "hinted").argument_hint.as_deref(),
        Some("<file>")
    );
    assert_eq!(
        find(&items, "long").description,
        format!("{}...", "x".repeat(60))
    );
}

#[test]
fn a_template_hides_behind_any_row_with_its_name_and_keeps_its_subfolder_out_of_the_name() {
    let w = world();
    put(
        &w.home,
        ".omp/agent/prompts/model.md",
        &doc("hidden by the built-in"),
    );
    put(
        &w.home,
        ".omp/agent/prompts/deep/er/note.md",
        &doc("User note"),
    );
    put(&w.repo, ".omp/prompts/note.md", &doc("Project note"));
    put(&w.repo, ".omp/commands/taken.md", &doc("command"));
    put(
        &w.repo,
        ".omp/prompts/taken.md",
        &doc("hidden by the command"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(count(&items, "model"), 1);
    assert_eq!(find(&items, "model").source, SlashSource::BuiltIn);
    let notes: Vec<_> = items.iter().filter(|i| i.name == "note").collect();
    assert_eq!(notes.len(), 2, "templates do not hide each other");
    assert_eq!(
        (notes[0].source.clone(), notes[1].source.clone()),
        (SlashSource::User, SlashSource::Project)
    );
    assert_eq!(find(&items, "taken").description, "command");
}

#[test]
fn the_bundled_init_shows_only_when_no_file_command_is_named_init() {
    let w = world();
    assert_eq!(count(&rows(&w, &w.repo), "init"), 1);
    put(&w.repo, ".omp/commands/init.md", &doc("My init"));
    let items = rows(&w, &w.repo);
    assert_eq!(count(&items, "init"), 1);
    assert_eq!(find(&items, "init").description, "My init");
}
```

In `slash.rs`, add `mod omp;` after `pub mod front_matter;` and route omp in `list`:

```diff
--- a/crates/mkrew-core/src/slash.rs
+++ b/crates/mkrew-core/src/slash.rs
@@ -6,6 +6,7 @@
 
 mod claude;
 pub mod front_matter;
+mod omp;
 mod scan;
 mod tables;
 
@@ -78,7 +79,8 @@
     });
     let mut items = match cli {
         Cli::Claude => claude::scan(roots, builtins, &mut errors),
-        Cli::Codex | Cli::Omp | Cli::Copilot => builtins.into_iter().map(|b| b.item).collect(),
+        Cli::Omp => omp::scan(roots, builtins, &mut errors),
+        Cli::Codex | Cli::Copilot => builtins.into_iter().map(|b| b.item).collect(),
     };
     apply(&mut items, overrides);
     if cli == Cli::Claude {
```

The `omp.rs` file does not exist yet, so this is the red state.

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core slash::omp`
Expected: `error[E0583]: file not found for module omp`.

- [ ] **Step 3: Implement** `crates/mkrew-core/src/slash/omp.rs`:

```rust
//! Oh My Pi (spec 2026-11-04 §5.3), in the order of its popup with an empty filter: built-ins
//! and bundled commands, the `skill:` group row and its skills, markdown commands, prompt
//! templates.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use mkrew_protocol::{SlashItem, SlashSource};

use super::ScanRoots;
use super::scan::{self, Doc};
use super::tables::Builtin;

/// The first-line description of a command or template is cut to this many characters.
const CUT: usize = 60;

pub(super) fn scan(
    roots: &ScanRoots,
    builtins: Vec<Builtin>,
    errors: &mut Vec<String>,
) -> Vec<SlashItem> {
    let commands = commands(roots, errors);
    // The bundled markdown `init` is only a fallback for a file command of the same name.
    let has_init = commands.iter().any(|c| c.name == "init");
    let mut items: Vec<SlashItem> = builtins
        .into_iter()
        .filter(|b| !(b.bundled && has_init))
        .map(|b| b.item)
        .collect();

    let skills = skills(roots, errors);
    if !skills.is_empty() {
        let count = skills.len();
        items.push(SlashItem {
            name: "skill:".into(),
            description: if count == 1 {
                "1 skill".into()
            } else {
                format!("{count} skills")
            },
            argument_hint: None,
            source: SlashSource::BuiltIn,
            group: Some(u32::try_from(count).unwrap_or(u32::MAX)),
        });
        items.extend(skills);
    }
    items.extend(commands);

    // A template whose name another row already has is hidden; two templates may share one.
    let taken: HashSet<String> = items.iter().map(|i| i.name.clone()).collect();
    items.extend(
        templates(roots, errors)
            .into_iter()
            .filter(|t| !taken.contains(&t.name)),
    );
    items
}

fn item(name: String, doc: &Doc, source: &SlashSource) -> SlashItem {
    SlashItem {
        name,
        description: doc.description(Some(CUT)),
        argument_hint: doc.fm.argument_hint.clone(),
        source: source.clone(),
        group: None,
    }
}

/// Markdown commands by provider priority: omp 100, Claude 80, `.agent(s)` 70, Codex 70,
/// opencode 55; inside omp the project beats the user, inside `.agent(s)` the user beats the
/// project. The first row of a name wins. `foo/bar.md` under `.claude/commands` is `bar` and
/// `foo:bar`.
fn commands(roots: &ScanRoots, errors: &mut Vec<String>) -> Vec<SlashItem> {
    let (folder, home) = (&roots.folder, &roots.home);
    let mut sources: Vec<(PathBuf, bool, SlashSource)> = vec![
        (folder.join(".omp/commands"), false, SlashSource::Project),
        (home.join(".omp/agent/commands"), false, SlashSource::User),
        (folder.join(".claude/commands"), true, SlashSource::Project),
        (home.join(".agent/commands"), false, SlashSource::User),
        (home.join(".agents/commands"), false, SlashSource::User),
    ];
    for dir in scan::project_dirs(folder) {
        sources.push((dir.join(".agent/commands"), false, SlashSource::Project));
        sources.push((dir.join(".agents/commands"), false, SlashSource::Project));
    }
    sources.push((folder.join(".codex/commands"), false, SlashSource::Project));
    sources.push((
        folder.join(".opencode/commands"),
        false,
        SlashSource::Project,
    ));

    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for (dir, recursive, source) in sources {
        for doc in scan::command_docs(&dir, recursive, errors) {
            let last = doc.key.rsplit(':').next().unwrap_or(&doc.key).to_owned();
            let mut names = vec![last];
            if doc.key.contains(':') {
                names.push(doc.key.clone());
            }
            for name in names {
                if seen.insert(name.clone()) {
                    items.push(item(name, &doc, &source));
                }
            }
        }
    }
    items
}

/// `skill:<name>` rows by provider priority: native 100 (project folders up to the repo root,
/// then the user's `~/.omp/agent/skills`), omp plugins 90, Claude's project folder 80. A name
/// with `/` or `\` is dropped.
fn skills(roots: &ScanRoots, errors: &mut Vec<String>) -> Vec<SlashItem> {
    let mut sources: Vec<(PathBuf, SlashSource)> = scan::project_dirs(&roots.folder)
        .into_iter()
        .map(|dir| (dir.join(".omp/skills"), SlashSource::Project))
        .collect();
    sources.push((roots.home.join(".omp/agent/skills"), SlashSource::User));
    let modules = roots.home.join(".omp/plugins/node_modules");
    for (package, path) in scan::entries(&modules, errors) {
        sources.push((path.join("skills"), SlashSource::Plugin(package)));
    }
    sources.push((roots.folder.join(".claude/skills"), SlashSource::Project));

    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for (dir, source) in sources {
        for doc in scan::skill_docs(&dir, errors) {
            let name = doc.fm.name.clone().unwrap_or_else(|| doc.key.clone());
            if name.contains(['/', '\\']) || !seen.insert(name.clone()) {
                continue;
            }
            items.push(item(format!("skill:{name}"), &doc, &source));
        }
    }
    items
}

/// Prompt templates, user then project; the name is the file name without `.md`, whatever
/// folder it is in.
fn templates(roots: &ScanRoots, errors: &mut Vec<String>) -> Vec<SlashItem> {
    let sources: [(PathBuf, SlashSource); 2] = [
        (roots.home.join(".omp/agent/prompts"), SlashSource::User),
        (roots.folder.join(".omp/prompts"), SlashSource::Project),
    ];
    let mut items = Vec::new();
    for (dir, source) in sources {
        for doc in scan::command_docs(Path::new(&dir), true, errors) {
            let name = doc.key.rsplit(':').next().unwrap_or(&doc.key).to_owned();
            items.push(item(name, &doc, &source));
        }
    }
    items
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core slash::omp`
Expected: PASS, 13 tests.

Run: `cargo clippy -p mkrew-core --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/slash.rs crates/mkrew-core/src/slash/omp.rs crates/mkrew-core/src/slash/omp/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): omp slash scanner with the skill group row"
```

---

### Task 11 (A10): Copilot scanner

**Files:**
- Create: `crates/mkrew-core/src/slash/copilot.rs`
- Test: `crates/mkrew-core/src/slash/copilot/tests.rs`
- Modify: `crates/mkrew-core/src/slash.rs` (module list, `list`)

**Interfaces:**
- Consumes: `scan::*` (A6), `tables::Builtin` (A4), `ScanRoots.env` (A5).
- Produces: `pub(super) fn scan(roots: &ScanRoots, builtins: Vec<Builtin>, errors: &mut Vec<String>) -> Vec<SlashItem>`.
- Behavior (spec 5.4): built-ins in the table's order, then skills, then commands.
  Skills in this order, the first found wins (names compare case-insensitively): `.github/skills`, `.agents/skills` and `.claude/skills` of `folder` (Project), then `.github/skills` of each parent up to the repo root (Project), then `~/.copilot/skills`, `~/.agents/skills` and each folder of `COPILOT_SKILLS_DIRS` read from `roots.env` (User; the list is split with the platform's path separator).
  Commands: `<folder>/.claude/commands/*.md`, the file stem is the name, Project, not recursive.
  A skill beats a command of the same name; `user-invocable: false` rows are left out after the name is resolved; the name is `name`, else the folder.
  Plugin skills are not listed (the docs name no plugin folder).
  A skill or command that has the name of a built-in is listed next to it (the spec has no rule that removes either).

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-core/src/slash/copilot/tests.rs`:

```rust
use mkrew_protocol::{Cli, SlashSource};

use crate::profile::SlashOverrides;
use crate::slash::testing::{World, doc, put, skill, world};
use crate::slash::{SlashItem, list};

fn rows(w: &World, folder: &std::path::Path) -> Vec<SlashItem> {
    let out = list(Cli::Copilot, &w.roots(folder), &SlashOverrides::default());
    assert!(out.errors.is_empty(), "{:?}", out.errors);
    out.items
}

fn find<'a>(items: &'a [SlashItem], name: &str) -> &'a SlashItem {
    items
        .iter()
        .find(|i| i.name == name)
        .unwrap_or_else(|| panic!("no /{name}"))
}

#[test]
fn built_ins_come_first_then_skills_then_commands() {
    let w = world();
    put(&w.repo, ".github/skills/s/SKILL.md", &skill("zeta", "Z"));
    put(&w.repo, ".claude/commands/alpha.md", &doc("A"));
    let items = rows(&w, &w.repo);
    let at = |n: &str| items.iter().position(|i| i.name == n).unwrap();
    assert!(at("add-dir") < at("move") && at("move") < at("zeta") && at("zeta") < at("alpha"));
    assert_eq!(find(&items, "zeta").source, SlashSource::Project);
}

#[test]
fn the_first_found_wins_in_the_documented_order() {
    let w = world();
    put(
        &w.repo,
        ".claude/skills/a/SKILL.md",
        &skill("dup", "claude"),
    );
    put(
        &w.repo,
        ".agents/skills/a/SKILL.md",
        &skill("dup", "agents"),
    );
    put(
        &w.home,
        ".copilot/skills/a/SKILL.md",
        &skill("dup", "copilot home"),
    );
    put(
        &w.repo,
        ".github/skills/a/SKILL.md",
        &skill("dup", "github"),
    );
    put(
        &w.home,
        ".agents/skills/b/SKILL.md",
        &skill("user-dup", "agents home"),
    );
    put(
        &w.home,
        ".copilot/skills/b/SKILL.md",
        &skill("user-dup", "copilot home"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(items.iter().filter(|i| i.name == "dup").count(), 1);
    assert_eq!(find(&items, "dup").description, "github");
    assert_eq!(find(&items, "user-dup").description, "copilot home");
    assert_eq!(find(&items, "user-dup").source, SlashSource::User);
}

#[test]
fn parents_contribute_only_their_github_skills_up_to_the_repo_root() {
    let w = world();
    let folder = w.repo.join("pkg/inner");
    std::fs::create_dir_all(&folder).unwrap();
    put(
        &w.repo,
        ".github/skills/a/SKILL.md",
        &skill("from-root", "r"),
    );
    put(
        &w.repo,
        "pkg/.agents/skills/b/SKILL.md",
        &skill("parent-agents", "no"),
    );
    put(
        &folder,
        ".agents/skills/c/SKILL.md",
        &skill("own-agents", "yes"),
    );
    put(
        w.repo.parent().unwrap(),
        ".github/skills/d/SKILL.md",
        &skill("past-root", "no"),
    );
    let items = rows(&w, &folder);
    find(&items, "from-root");
    find(&items, "own-agents");
    assert!(
        items
            .iter()
            .all(|i| i.name != "parent-agents" && i.name != "past-root")
    );
}

#[test]
fn a_skill_beats_a_command_of_the_same_name_and_hidden_skills_are_left_out() {
    let w = world();
    put(&w.repo, ".claude/commands/ship.md", &doc("command"));
    put(
        &w.repo,
        ".github/skills/s/SKILL.md",
        &skill("ship", "skill"),
    );
    put(
        &w.repo,
        ".github/skills/h/SKILL.md",
        "---\nname: quiet\nuser-invocable: false\n---\nx\n",
    );
    put(
        &w.repo,
        ".claude/commands/quiet.md",
        &doc("shadowed by the hidden skill"),
    );
    let items = rows(&w, &w.repo);
    assert_eq!(find(&items, "ship").description, "skill");
    assert!(
        items.iter().all(|i| i.name != "quiet"),
        "the skill won the name, then was left out"
    );
}

#[test]
fn commands_are_read_from_the_folder_only_and_not_recursively() {
    let w = world();
    let folder = w.repo.join("pkg");
    std::fs::create_dir_all(&folder).unwrap();
    put(&w.repo, ".claude/commands/root.md", &doc("r"));
    put(&folder, ".claude/commands/own.md", &doc("o"));
    put(&folder, ".claude/commands/sub/deep.md", &doc("d"));
    let items = rows(&w, &folder);
    find(&items, "own");
    assert!(
        items
            .iter()
            .all(|i| i.name != "root" && i.name != "deep" && i.name != "sub:deep")
    );
}

#[test]
fn copilot_skills_dirs_come_from_the_given_environment() {
    let w = world();
    let (a, b) = (w.home.join("extra-a"), w.home.join("extra-b"));
    put(&a, "x/SKILL.md", &skill("from-a", "A"));
    put(&b, "y/SKILL.md", &skill("from-b", "B"));
    let mut roots = w.roots(&w.repo);
    let joined = std::env::join_paths([&a, &b]).unwrap();
    roots.env.insert(
        "COPILOT_SKILLS_DIRS".into(),
        joined.to_str().unwrap().into(),
    );
    let items = list(Cli::Copilot, &roots, &SlashOverrides::default()).items;
    assert_eq!(find(&items, "from-a").source, SlashSource::User);
    find(&items, "from-b");
    let none = list(Cli::Copilot, &w.roots(&w.repo), &SlashOverrides::default()).items;
    assert!(none.iter().all(|i| i.name != "from-a"));
}
```

In `slash.rs`, add `mod copilot;` after `mod claude;` and finish the `match` in `list` (the file is then in its final state):

```diff
--- a/crates/mkrew-core/src/slash.rs
+++ b/crates/mkrew-core/src/slash.rs
@@ -5,6 +5,7 @@
 //! and keeps what it cannot read as one line in [`SlashList::errors`].
 
 mod claude;
+mod copilot;
 pub mod front_matter;
 mod omp;
 mod scan;
@@ -79,8 +80,9 @@
     });
     let mut items = match cli {
         Cli::Claude => claude::scan(roots, builtins, &mut errors),
+        Cli::Codex => builtins.into_iter().map(|b| b.item).collect(),
         Cli::Omp => omp::scan(roots, builtins, &mut errors),
-        Cli::Codex | Cli::Copilot => builtins.into_iter().map(|b| b.item).collect(),
+        Cli::Copilot => copilot::scan(roots, builtins, &mut errors),
     };
     apply(&mut items, overrides);
     if cli == Cli::Claude {
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core slash::copilot`
Expected: `error[E0583]: file not found for module copilot`.

- [ ] **Step 3: Implement** `crates/mkrew-core/src/slash/copilot.rs`:

```rust
//! GitHub Copilot CLI (spec 2026-11-04 §5.4): built-ins in the reference order, then skills,
//! then `.claude/commands` files. The first found wins in the order of the rows below; a skill
//! beats a command of the same name; `user-invocable: false` is left out.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::PathBuf;

use mkrew_protocol::{SlashItem, SlashSource};

use super::ScanRoots;
use super::scan::{self, Doc};
use super::tables::Builtin;

pub(super) fn scan(
    roots: &ScanRoots,
    builtins: Vec<Builtin>,
    errors: &mut Vec<String>,
) -> Vec<SlashItem> {
    // The working folder holds `.github`, `.agents` and `.claude` skills; its parents up to the
    // repo root only `.github/skills`.
    let mut skill_dirs: Vec<(PathBuf, SlashSource)> = Vec::new();
    for (at, dir) in scan::project_dirs(&roots.folder).into_iter().enumerate() {
        skill_dirs.push((dir.join(".github/skills"), SlashSource::Project));
        if at == 0 {
            skill_dirs.push((dir.join(".agents/skills"), SlashSource::Project));
            skill_dirs.push((dir.join(".claude/skills"), SlashSource::Project));
        }
    }
    skill_dirs.push((roots.home.join(".copilot/skills"), SlashSource::User));
    skill_dirs.push((roots.home.join(".agents/skills"), SlashSource::User));
    if let Some(list) = roots.env.get("COPILOT_SKILLS_DIRS") {
        for dir in std::env::split_paths(OsStr::new(list)).filter(|d| !d.as_os_str().is_empty()) {
            skill_dirs.push((dir, SlashSource::User));
        }
    }

    let mut seen = HashSet::new();
    let mut rows = Vec::new();
    for (dir, source) in skill_dirs {
        for doc in scan::skill_docs(&dir, errors) {
            let name = doc.fm.name.clone().unwrap_or_else(|| doc.key.clone());
            push(&mut rows, &mut seen, name, &doc, &source);
        }
    }
    let commands = roots.folder.join(".claude/commands");
    for doc in scan::command_docs(&commands, false, errors) {
        push(
            &mut rows,
            &mut seen,
            doc.key.clone(),
            &doc,
            &SlashSource::Project,
        );
    }

    let mut items: Vec<SlashItem> = builtins.into_iter().map(|b| b.item).collect();
    items.extend(
        rows.into_iter()
            .filter(|(invocable, _)| *invocable)
            .map(|(_, item)| item),
    );
    items
}

/// Adds a row unless an earlier one has the name (case-insensitively).
fn push(
    rows: &mut Vec<(bool, SlashItem)>,
    seen: &mut HashSet<String>,
    name: String,
    doc: &Doc,
    source: &SlashSource,
) {
    if !seen.insert(name.to_lowercase()) {
        return;
    }
    let item = SlashItem {
        name,
        description: doc.description(None),
        argument_hint: doc.fm.argument_hint.clone(),
        source: source.clone(),
        group: None,
    };
    rows.push((doc.fm.user_invocable != Some(false), item));
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core`
Expected: PASS, with 6 new tests in `slash::copilot` (the whole crate, 102 slash and profile tests among them, was green in the scratch run).

Run: `cargo clippy -p mkrew-core --all-targets -- -D warnings` and `cargo fmt --all --check`
Expected: no warnings, no diff.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/slash.rs crates/mkrew-core/src/slash/copilot.rs crates/mkrew-core/src/slash/copilot/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): Copilot CLI slash scanner"
```

---

## Notes for the integrator

Spec gaps found while planning, and the choice made (each is pinned by a test named above):

1. `Request` dispatch in `mkrew-daemon/src/server.rs` is an exhaustive `match`: A1 adds a temporary `slash.list` arm that answers `invalid_request`, so the workspace builds after every commit; the daemon section's handler task replaces that arm (or Main squashes A1 with that task).
2. Spec 5.1 does not say whether a skill in a lower scope beats a command file in a higher one (A7: skill wins), nor whether a command file replaces a built-in (A7: no; only skills do).
3. Spec 5.4 and 5.3 are silent on a skill or command named like a built-in (Copilot and omp list both rows).
4. Spec 4.2 orders `hide` after the merge but does not say what `hide` plus `add` of one name means (A5: `add` merges first, so `hide` removes it too).
5. `user-invocable: false` is applied after the name is resolved (a hidden skill still owns its name); the spec only says such skills are left out.
6. `when_to_use` is the description fallback between `description` and the first body line; the spec lists the key but names no use.
7. An explicit `cli` on a shell profile is a profile error (spec 4.2 says shells never infer; it does not say what an explicit key on a shell is).
8. `managed-settings.json` is read from inside each managed folder; the spec names the file, not its place.
9. omp: the research table listed `unpin`, but the version the spec pins (18.13.2, tag `v18.13.2`) has no `unpin`, so it is not in `omp.toml`; the sources are pinned to that tag, not to `main`.
10. Claude descriptions come from the docs "Purpose" column, not from `supportedCommands()` (spec 5.1 prefers it, and it needs a running Claude Code); the table header says so.
11. `ScanRoots::for_machine` carries one managed folder chosen by the build OS; spec 5.1 lists the three OS paths.

---

# Part A wire: ADR 0094, daemon `slash.list`, shared keys, `/` picker (A11-A17)

> Task IDs A11-A17 (PlanACore owns A1-A10: protocol types, profile `cli` / `[slash]`, front matter, tables, scanners, `slash::list`). IDs kept at merge; the run order is in the plan header.
> Order inside this section: **A11 first** (spec says the ADR is written by the first task of part A, so Main should place it before A1), then A12 (needs A1, A2 and the core `slash::list` of A5-A10), A13 -> A14 -> A15 -> A16 (needs A1 for the protocol types), A17 last.
> Every commit compiles and passes on its own: A13 keeps `@` working through the old handlers, A14 moves them, A16 adds `/`.

## Assumptions about the other sections (PlanACore confirmed the first three)

- Protocol, `Request::SlashList`, `PROTOCOL_VERSION == 52`, the SPEC §7 request row and protocol-history line are A1. Profile `cli` / `[slash]` are A2. `mkrew_core::slash::{list, ScanRoots}` are A5, the scanners A6-A10.
- `AgentProfile.cli` is already resolved (explicit, else inferred, shell profiles `None`). The daemon never infers.
- The core finds the main checkout of a linked worktree itself from `ScanRoots.folder` (`.git` file, gitdir, commondir). The daemon passes the project folder inside the worktree.
- The wire types derive `Debug, Clone, PartialEq, Eq` (like `PortRow`), `Cli` also `Copy`. The UI and the daemon test need these derives. **PlanACore: please confirm.**
- `std::env::home_dir()` is the home helper already used by `crates/mkrew-daemon/src/usage.rs:32`; on Windows it reads `USERPROFILE` first, on Unix `HOME`. [INFERENCE for Windows; the daemon test below fails loudly if it does not hold.]

## Deviations from the shared contract (decide at merge)

1. `trigger_at` and `Trigger` are `pub`, not `pub(crate)`. They live in the **lib** crate (`crates/mkrew-ui/src/composer.rs`), and `Shell` is in the **bin** crate, so `pub(crate)` would not be visible to it. Same for the `slash_picker` module (`pub mod`).
2. `trigger_at(text, cursor)` is called **before** the char goes in: `text` and `cursor` are the prompt as it is when the key arrives (the pickers swallow the key, like `@` does today). It is true when the cursor is at the start of the text or the char before it is whitespace (`\n` included). The contract sentence "the char before cursor is preceded by line start or whitespace" describes the post-insert view of the same rule.
3. **Behavior change for `@`:** today `@` opens the file picker anywhere in the prompt (`TaskDialog::opens_file_picker`, `composer_key_down`). Spec 4.6 says `@` and `/` share one helper, so `@` now also types plainly inside a word (`a@b.com`). SPEC §20B.1 and HUMAN_CHECKS say so (A11, A17). If Main wants `@` untouched, `composer::trigger` returns `Some(Trigger::File)` without calling `trigger_at`: one line in A13.
4. Spec 4.4 "changing Agent in the dialog while the picker is open" is **not reachable by the user today**: the picker's modal shade covers the dialog's Agent chips and the dialog root does not take keys while a picker is open (`shell/task_dialog.rs:429-445`). A15 implements and tests the model (`retarget`), A16 calls it from the chip click handler, as the spec asks. If the picker ever becomes non-modal the path is already there.
5. A new task with no resolvable agent (no agent picked, workspace has no `default_agent`) has nothing to list: `/` types as a plain `/` (like `@` without a daemon). The spec does not name this case.
6. Not in the spec, so not built: Backspace on an empty filter does not close the picker (Esc does, and types `/`).
7. The README "feature line" is shared with part C's Prompts page; A17 adds the `/` bullet, Main merges the two into one bullet if both land.

## Review Focus candidates

1. **`/` typed inside a path, URL or "and/or", and `@` in an e-mail address must stay plain text.** Pinned by `a_trigger_needs_a_line_start_or_whitespace_before_it` (A13) and `the_dialog_trigger_needs_the_prompt_field_and_an_idle_dialog` (A13).
2. **A slow answer for the agent the user just left (or for a picker already closed) must not replace the rows of the current one.** Pinned by `only_the_answer_to_the_latest_request_is_shown` and `an_agent_change_replaces_the_rows_and_keeps_the_filter` (A15).
3. **The list must come from the folder the agent will run in and from the agent's machine environment:** the task worktree is scanned instead of the project folder, `~` is the daemon user's home, and the profile `env` wins over the daemon's own. Pinned by `slash_list_scans_the_project_folder_or_its_task_worktree` and `slash_list_reads_the_home_and_the_profile_env_of_the_agents_machine` (A12).

---

### Task 1 (A11): ADR 0094 and the SPEC text (run first)

**Files:**
- Create: `docs/decisions/0094-slash-command-picker.md`
- Modify: `docs/SPEC.md` (§6 after line 460; §13 after line 1485; §20B.1 lines 2132-2144; §20B new item 7 after line 2178)

**Interfaces:** none (docs). Check first that no other part took the number: `ls docs/decisions | grep ^0094` must print nothing (memory note: collisions happen; part B takes 0095).

- [ ] **Step 1: Check the number is free**

Run: `ls docs/decisions | grep -E "^009[4-9]"`
Expected: no output. If a file shows, take the next free number and fix every `0094` below and in A16/A17.

- [ ] **Step 2: Write `docs/decisions/0094-slash-command-picker.md`** (one sentence per line, no em dash character)

````markdown
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
- Which CLI an agent runs comes from a new optional profile key `cli` (`claude`, `codex`, `omp`, `copilot`), else from the file stem of `command`; shell profiles and wrappers such as `npx` infer nothing and answer `cli: null` with no rows.
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
````

- [ ] **Step 3: SPEC §6 - the profile keys.** In `docs/SPEC.md`, after line 460 (`answers \`invalid_state\` naming the key and the file.`) and its blank line, insert:

````markdown
Two optional keys serve the `/` picker (§20B.7, ADR 0094):

```toml
cli = "claude"                  # "claude" | "codex" | "omp" | "copilot"
[slash]
hide = ["desktop", "upgrade"]   # names removed from the picker
[[slash.add]]                   # a command the scan cannot see; wins over every other source
name = "deploy"
description = "Deploy to staging"
argument_hint = "<env>"         # optional
```

`cli` names the CLI the agent runs.
A missing `cli` is inferred from the file stem of `command`, lowercased, with `.exe`, `.cmd`, `.bat` and `.ps1` removed: `claude`, `codex`, `omp` or `copilot`.
A wrapper (`npx`, a script) infers nothing, so the user sets `cli`.
A shell profile (`kind = "shell"`) never has a CLI.
An unknown `cli` value is a parse error, listed like the other profile errors.
The shipped `agents/*.toml` state `cli` explicitly; `AgentProfileInfo` (§7) does not change.
`[slash] hide` removes rows by name after every source is merged.
`[[slash.add]]` rows get the source `profile`.
````

- [ ] **Step 4: SPEC §13 - the shortcut table.** After line 1485 (the row `| Agent (v2) | Open the prompt composer of the focused agent pane | Ctrl+Shift+Enter |`) insert:

```markdown
| Agent (v2) | In a prompt box (the composer, the task dialog's Prompt), at the start of a line or after a space: pick the agent's command (§20B.7) / pick a file (§12A); Enter or Tab types the pick, Esc types the character | / / @ |
```

- [ ] **Step 5: SPEC §20B.1.** Replace the text at lines 2132-2134

```
   paste (§7). Typing `@` opens the quick-open file picker (§12A); the pick inserts
   `composer.file_ref` with `{path}` = the project-relative path, `/` separators, quoted with `"`
   if it contains spaces. Drafts are kept per session in UI memory (lost on UI restart, which is fine).
```

with

```
   paste (§7). Typing `@` or `/` at the start of a line or after whitespace opens a picker; in the
   middle of a word (`a@b.com`, `src/main.rs`) they type as they are. `@` opens the quick-open file
   picker (§12A); the pick inserts
   `composer.file_ref` with `{path}` = the project-relative path, `/` separators, quoted with `"`
   if it contains spaces. `/` opens the agent's command picker (§20B.7). Drafts are kept per session in UI memory (lost on UI restart, which is fine).
```

and in lines 2140-2141 change "`@` picks a" + "file, "From prompt…" inserts" to read "`@` picks a file, `/` an agent command, "From prompt…" inserts" (keep the rest of the sentence, including the ✦ and saving rules).

- [ ] **Step 6: SPEC §20B item 7.** After line 2178 (`then runs it. Only for Todo or Review cards without a live session.`) insert:

```markdown
7. **Slash picker.** Typing `/` at the start of a line or after whitespace in the composer or the task dialog's Prompt opens a picker that lists what typing `/` lists in the agent's own terminal, with the same names, descriptions and order (ADR 0094).
   The daemon builds the list on the machine that runs the agent (`slash.list`, §7): the CLI's built-in commands from a table pinned to a CLI version (`crates/mkrew-core/slash/<cli>.toml`), plus the skills, command files and plugin commands that CLI finds on disk by its documented rules.
   The CLI is never started.
   Which CLI an agent runs comes from the profile's `cli` (§6); a profile with no known CLI shows a note and a button that opens its file.
   The folder scanned is the session's project (its task worktree when the task has one) for a composer, the project folder for a new task, and the task's worktree (else the project folder) for an edited task.
   The agent is the session's profile in a composer, the dialog's Agent (else the workspace default) in the task dialog; with none, `/` types as it is.
   The last answer per (agent, project, worktree) is cached for the connection: reopening shows it at once and refreshes it.
   An empty filter lists the rows in the CLI's order; a filter ranks by fuzzy score on the name, ties in the CLI's order.
   Each row shows `/name`, its argument hint, a one-line description and a source chip (`built-in`, `project`, `user`, `managed`, `plugin: <name>`, `profile`).
   ↑ ↓ move, Enter or Tab types `/name ` into the box, Esc types `/` and what was typed after it.
   In omp the `skill:` row stands for its skills and expands when the filter starts with `skill:`; a group row types nothing.
   Not listed: MCP prompts (the footer says so for Claude), Codex skills (its `$` popup), omp extension and TypeScript commands (add them with `[[slash.add]]`), rows that depend on the user's plan or login (hide them with `[slash] hide`).
   Files that cannot be read are counted in the footer, with the list in a tooltip.
```

- [ ] **Step 7: Check the new text has no em dash and no stray tab**

Run: `grep -c "—" docs/decisions/0094-slash-command-picker.md`
Expected: `0`.

- [ ] **Step 8: Commit**

```bash
git add docs/decisions/0094-slash-command-picker.md docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "docs: ADR 0094 and SPEC for the / command picker"
```

---

### Task 12 (A12): Daemon `slash.list` handler

**Files:**
- Create: `crates/mkrew-daemon/src/server/slash.rs`
- Modify: `crates/mkrew-daemon/src/server.rs` (module list, after `mod session_read;` at line 49; dispatch, after the `Request::FsFind` arm at line 442)
- Create: `crates/mkrew-daemon/tests/daemon/slash.rs`
- Modify: `crates/mkrew-daemon/tests/daemon/main.rs` (module list at lines 46-61: add `mod slash;` after `mod runs;`)

**Interfaces:**
- Consumes (A1, A2, A5-A10): `mkrew_protocol::{Cli, SlashItem, SlashListParams, SlashListResult, SlashSource, Request::SlashList}`; `AgentProfile { cli: Option<Cli>, slash: SlashOverrides, env: BTreeMap<String, String>, .. }`; `mkrew_core::slash::{list, ScanRoots}` with `ScanRoots::for_machine(folder: PathBuf, home: PathBuf, env: BTreeMap<String, String>) -> ScanRoots`, `list(cli: Cli, roots: &ScanRoots, overrides: &SlashOverrides) -> SlashList { items, errors }`. Existing: `Daemon::load_profile(&self, id: &str) -> Result<AgentProfile, Outcome>` (`server.rs:911`, `not_found` for a missing or broken profile), `Daemon::target(&self, project_id, worktree_task_id, repo) -> Result<Target, Outcome>` (`server/target.rs:53`, no watcher; `invalid_state` for a task without a worktree, `invalid_request` for a task of another project, `not_found` for an unknown project).
- Produces: `Daemon::slash_list(&self, params: SlashListParams) -> Outcome` (`pub(super) async`).

`target()` is used instead of `watched_target()` on purpose: `fs.*` keeps a watcher for its view, the picker only reads a few small files.

- [ ] **Step 1: Write the failing tests.** Create `crates/mkrew-daemon/tests/daemon/slash.rs`:

```rust
//! `slash.list` (SPEC §20B.7, ADR 0094): the agent's command list, read on the daemon's machine.

use mkrew_protocol::{Cli, SlashItem, SlashListParams, SlashListResult, SlashSource};

use super::*;

fn slash_list(project: &Project, profile: &str, worktree_task_id: Option<Ulid>) -> Request {
    Request::SlashList(SlashListParams {
        profile_id: profile.into(),
        project_id: project.id,
        worktree_task_id,
    })
}

fn listed(client: &mut Client, request: Request) -> SlashListResult {
    client.call(request).into_result().unwrap()
}

fn find<'a>(list: &'a SlashListResult, name: &str) -> Option<&'a SlashItem> {
    list.items.iter().find(|item| item.name == name)
}

/// `<dir>/<name>.md` with a front matter `description`.
fn write_command(dir: &Path, name: &str, description: &str) {
    std::fs::create_dir_all(dir).unwrap();
    let text = format!("---\ndescription: {description}\n---\nDo it.\n");
    std::fs::write(dir.join(format!("{name}.md")), text).unwrap();
}

/// `<dir>/<name>/SKILL.md` with a front matter `name` and `description`.
fn write_skill(dir: &Path, name: &str) {
    std::fs::create_dir_all(dir.join(name)).unwrap();
    let text = format!("---\nname: {name}\ndescription: A skill\n---\nDo it.\n");
    std::fs::write(dir.join(name).join("SKILL.md"), text).unwrap();
}

/// The folder is the project's, or with `worktree_task_id` the project's folder inside the
/// task's worktree (resolved like `fs.*`); the profile's `[[slash.add]]` rows come along.
#[test]
fn slash_list_scans_the_project_folder_or_its_task_worktree() {
    let env = Env::new();
    let scenario = env.dir.path().join("worktree.toml");
    std::fs::write(&scenario, "[[step]]\naction = \"ask\"\n").unwrap();
    env.write_fake_profile_as("wt", &format!("['--scenario', '{}']", scenario.display()));
    env.write_profile(
        "cl",
        "command = \"claude\"\ncli = \"claude\"\n\n[[slash.add]]\nname = \"deploy\"\n\
         description = \"Deploy to staging\"\nargument_hint = \"<env>\"\n",
    );
    let repo = env.repo();
    let app = repo.join("app");
    write_command(&app.join(".claude/commands"), "ship", "Ship it");
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&app);
    let task: Task = client
        .call(Request::TaskCreate(TaskCreateParams {
            project_id: project.id,
            title: "Slash".into(),
            prompt: String::new(),
            agent_profile: Some("wt".into()),
            position: None,
            worktree: true,
        }))
        .into_result()
        .unwrap();
    let run: SessionSpawnResult = client.call(run_task(task.id)).into_result().unwrap();
    client.wait_state(run.session_id, SessionState::NeedsInput);
    let task = wait_task(&mut client, task.id, |t| t.worktree_path.is_some());
    let root = PathBuf::from(task.worktree_path.clone().unwrap());
    // Only in the worktree: not tracked, written after the checkout.
    write_command(&root.join("app/.claude/commands"), "wt-only", "Only here");

    let in_project = listed(&mut client, slash_list(&project, "cl", None));
    assert_eq!(in_project.cli, Some(Cli::Claude));
    let ship = find(&in_project, "ship").unwrap();
    assert_eq!(
        (&ship.source, ship.description.as_str()),
        (&SlashSource::Project, "Ship it")
    );
    assert!(find(&in_project, "wt-only").is_none());
    let deploy = find(&in_project, "deploy").unwrap();
    assert_eq!(
        (&deploy.source, deploy.argument_hint.as_deref()),
        (&SlashSource::Profile, Some("<env>"))
    );

    let in_worktree = listed(&mut client, slash_list(&project, "cl", Some(task.id)));
    assert!(find(&in_worktree, "wt-only").is_some());
    assert!(
        find(&in_worktree, "ship").is_some(),
        "the tracked command is in the checkout too"
    );
    client.stop();
}

/// `~` is the daemon user's home and the profile's `env` wins over the daemon's own
/// (Copilot reads `COPILOT_SKILLS_DIRS`).
#[test]
fn slash_list_reads_the_home_and_the_profile_env_of_the_agents_machine() {
    let env = Env::new();
    let home = env.dir.path().join("home");
    let (daemon_skills, profile_skills) = (
        env.dir.path().join("daemon-skills"),
        env.dir.path().join("profile-skills"),
    );
    write_skill(&home.join(".copilot/skills"), "from-home");
    write_skill(&daemon_skills, "from-daemon-env");
    write_skill(&profile_skills, "from-profile-env");
    env.write_profile("plain", "command = \"copilot\"\ncli = \"copilot\"\n");
    env.write_profile(
        "own-env",
        &format!(
            "command = \"copilot\"\ncli = \"copilot\"\n\
             env = {{ COPILOT_SKILLS_DIRS = '{}' }}\n",
            profile_skills.display()
        ),
    );
    let repo = env.repo();
    let (_daemon, mut client) = env.start_with(&[
        ("HOME", home.as_path()),
        ("USERPROFILE", home.as_path()),
        ("COPILOT_SKILLS_DIRS", daemon_skills.as_path()),
    ]);
    let project = client.add_project(&repo);

    let plain = listed(&mut client, slash_list(&project, "plain", None));
    assert_eq!(plain.cli, Some(Cli::Copilot));
    assert!(find(&plain, "from-home").is_some());
    assert!(find(&plain, "from-daemon-env").is_some());
    assert!(find(&plain, "from-profile-env").is_none());

    let own = listed(&mut client, slash_list(&project, "own-env", None));
    assert!(find(&own, "from-home").is_some());
    assert!(find(&own, "from-profile-env").is_some());
    assert!(
        find(&own, "from-daemon-env").is_none(),
        "the profile's env replaces the daemon's value"
    );
    client.stop();
}

/// An unknown CLI is an empty answer, not an error; a missing profile, project or worktree is.
#[test]
fn slash_list_names_what_it_cannot_scan() {
    let env = Env::new();
    env.write_profile("custom", "command = \"aider\"\n");
    env.write_profile("cl", "command = \"claude\"\ncli = \"claude\"\n");
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    assert_eq!(
        listed(&mut client, slash_list(&project, "custom", None)),
        SlashListResult {
            cli: None,
            items: Vec::new(),
            errors: Vec::new()
        }
    );
    assert_eq!(
        error_code(client.call(slash_list(&project, "nope", None))),
        ErrorCode::NotFound
    );
    let other = Project {
        id: Ulid::new(),
        ..project.clone()
    };
    assert_eq!(
        error_code(client.call(slash_list(&other, "cl", None))),
        ErrorCode::NotFound
    );
    let plain: Task = client
        .call(create_task(&project, "No worktree", None))
        .into_result()
        .unwrap();
    assert_eq!(
        error_code(client.call(slash_list(&project, "cl", Some(plain.id)))),
        ErrorCode::InvalidState,
        "the task has no worktree"
    );
    client.stop();
}
```

In `crates/mkrew-daemon/tests/daemon/main.rs` add `mod slash;` between `mod runs;` (line 54) and `mod states;` (line 55).

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-daemon --test daemon slash_list`
Expected: the three tests fail: `slash.list is not implemented yet` (the daemon's `other =>` arm at `server.rs:468` answers `invalid_request`), so `listed(..).unwrap()` panics and `error_code` returns `InvalidRequest` instead of `NotFound`. (If A1 is not merged yet this is a compile error: `SlashListParams` not found.)

- [ ] **Step 3: Implement.** Create `crates/mkrew-daemon/src/server/slash.rs`:

```rust
//! `slash.list` (SPEC §20B.7, ADR 0094): the commands that typing `/` shows in the agent's own
//! terminal, read on the machine that runs the agent.

use std::collections::BTreeMap;
use std::ffi::OsString;

use mkrew_core::slash::{self, ScanRoots};
use mkrew_protocol::{ErrorCode, Outcome, SlashListParams, SlashListResult};

use super::{Daemon, ok};

impl Daemon {
    /// The commands of profile `params.profile_id`'s CLI for the project folder, or for the
    /// project's folder inside the task worktree `worktree_task_id` (resolved like `fs.*`, but
    /// no watcher is kept: a few small files are read once). A profile whose CLI is unknown
    /// answers `cli: None` and no items.
    pub(super) async fn slash_list(&self, params: SlashListParams) -> Outcome {
        let profile = match self.load_profile(&params.profile_id) {
            Ok(profile) => profile,
            Err(outcome) => return outcome,
        };
        let target = match self
            .target(params.project_id, params.worktree_task_id, None)
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let Some(cli) = profile.cli else {
            return ok(&SlashListResult {
                cli: None,
                items: Vec::new(),
                errors: Vec::new(),
            });
        };
        // `~` means the home of the user the agent runs as: the daemon's.
        let Some(home) = std::env::home_dir() else {
            return Outcome::error(ErrorCode::Internal, "cannot determine the home directory");
        };
        let roots = ScanRoots::for_machine(
            target.dir,
            home,
            merged_env(std::env::vars_os(), &profile.env),
        );
        let overrides = profile.slash;
        let listed =
            tokio::task::spawn_blocking(move || slash::list(cli, &roots, &overrides)).await;
        match listed {
            Ok(list) => ok(&SlashListResult {
                cli: Some(cli),
                items: list.items,
                errors: list.errors,
            }),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("slash.list failed: {e}")),
        }
    }
}

/// The daemon's environment with the profile's `env` on top: what the agent would see. A name
/// or value that is not Unicode names no setting the scan reads, so it is left out.
fn merged_env(
    daemon: impl IntoIterator<Item = (OsString, OsString)>,
    profile: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut env: BTreeMap<String, String> = daemon
        .into_iter()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    env.extend(profile.iter().map(|(name, value)| (name.clone(), value.clone())));
    env
}
```

In `crates/mkrew-daemon/src/server.rs`:
- after `mod session_read;` (line 49) add `mod slash;` (keeps the list alphabetical: `session_read`, `slash`, `target`).
- REPLACE the temporary `Request::SlashList(_)` arm that A1 added right after the `Request::AgentProfiles` arm (about line 405, answers `invalid_request`) with:

```rust
            Request::SlashList(params) => (self.slash_list(params).await, After::Continue),
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-daemon --test daemon slash_list`
Expected: 3 passed. If `slash_list_reads_the_home_and_the_profile_env_of_the_agents_machine` finds no `from-home` on Windows, `std::env::home_dir()` is not reading `USERPROFILE` there: stop and tell Main (the fallback would be a `MKREW_HOME` override read by the daemon, which needs an ADR line).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/src/server/slash.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/tests/daemon/slash.rs crates/mkrew-daemon/tests/daemon/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): slash.list lists an agent's commands on its machine"
```

---

### Task 13 (A13): `trigger_at` replaces `opens_file_picker`

**Files:**
- Modify: `crates/mkrew-ui/src/composer.rs` (import at line 14; `Composers::trigger` after `shown` at line 121; `Trigger`, `trigger_at`, `trigger` after the `impl Composers` block that ends at line 239; tests appended to `mod tests` that starts at line 364)
- Modify: `crates/mkrew-ui/src/task_dialog.rs` (import at line 11; replace `opens_file_picker` at lines 287-290; update the test at lines 624-666; add one test)
- Modify: `crates/mkrew-ui/src/shell/composer.rs` (lines 47-51, the `@` check in `composer_key_down`)
- Modify: `crates/mkrew-ui/src/shell/task_dialog.rs` (lines 81-83, the `@` check in `task_dialog_key_down`)

**Interfaces:**
- Produces (lib crate, `pub`):
  - `composer::Trigger { File, Slash }` (`Debug, Clone, Copy, PartialEq, Eq`)
  - `composer::trigger_at(text: &str, cursor: usize) -> bool`
  - `composer::trigger(key: &DialogKey, input: &Input) -> Option<Trigger>`
  - `Composers::trigger(&self, session: Ulid, key: &DialogKey) -> Option<Trigger>`
  - `TaskDialog::trigger(&self, key: &DialogKey) -> Option<Trigger>` (replaces `TaskDialog::opens_file_picker`)
- Consumes: `text_box::Input::{selection, cursor, as_str}`, `task_dialog::DialogKey`.

- [ ] **Step 1: Write the failing tests.** Append to `mod tests` in `crates/mkrew-ui/src/composer.rs` (the module already has `use super::*;`; add `use crate::editor::Motion;` at the top of the module if it is not there):

```rust
    /// A `/` in a path, a URL or "and/or", and an `@` in an address, are typed as they are.
    #[test]
    fn a_trigger_needs_a_line_start_or_whitespace_before_it() {
        for (text, cursor, opens) in [
            ("", 0, true),
            ("fix ", 4, true),
            ("fix\n", 4, true),
            ("fix\t", 4, true),
            ("a b", 2, true),
            ("héllo ", "héllo ".len(), true),
            ("src", 3, false),
            ("see http:", 9, false),
            ("a@b", 3, false),
            ("ab", 1, false),
            ("héllo", "héllo".len(), false),
            // Never a panic: past the end, or inside a multi-byte char.
            ("ab", 5, false),
            ("é", 1, false),
        ] {
            assert_eq!(trigger_at(text, cursor), opens, "{text:?} at {cursor}");
        }
    }

    #[test]
    fn a_trigger_is_picked_by_the_typed_char_and_a_selection_moves_the_spot() {
        let (at, slash) = (DialogKey::Insert("@".into()), DialogKey::Insert("/".into()));
        let mut input = Input::default();
        assert_eq!(trigger(&at, &input), Some(Trigger::File));
        assert_eq!(trigger(&slash, &input), Some(Trigger::Slash));
        assert_eq!(trigger(&DialogKey::Insert("a".into()), &input), None);
        assert_eq!(trigger(&DialogKey::Backspace, &input), None);

        input.insert("src");
        assert_eq!(trigger(&slash, &input), None, "`src/` is a path");
        input.insert(" b");
        assert_eq!(trigger(&slash, &input), None, "after `b`");
        // `b` selected: the typed char replaces it, so the spot is after the space.
        input.edit(&DialogKey::Move {
            motion: Motion::Left,
            extend: true,
        });
        assert_eq!(input.selected_text(), Some("b"));
        assert_eq!(trigger(&slash, &input), Some(Trigger::Slash));
    }

    #[test]
    fn a_composer_trigger_needs_a_shown_idle_draft() {
        let session = Ulid::new();
        let slash = DialogKey::Insert("/".into());
        let mut composers = Composers::default();
        assert_eq!(composers.trigger(session, &slash), None, "no draft");
        composers.open(session);
        assert_eq!(composers.trigger(session, &slash), Some(Trigger::Slash));
        composers.insert(session, "fix");
        assert_eq!(composers.trigger(session, &slash), None, "after a word");
        composers.insert(session, " ");
        assert_eq!(composers.trigger(session, &slash), Some(Trigger::Slash));

        composers.begin_send(session);
        assert_eq!(composers.trigger(session, &slash), None, "sending");
        composers.sent(session, Err("session exited".into()));
        composers.hide(session);
        assert_eq!(composers.trigger(session, &slash), None, "hidden");
    }
```

In `crates/mkrew-ui/src/task_dialog.rs` tests:

1. In `the_prompt_is_improved_like_the_composer` (lines 626-666) replace lines 628-633

```rust
        let at = DialogKey::Insert("@".into());
        let mut dialog = TaskDialog::edit(task());
        assert!(!dialog.opens_file_picker(&at), "the title types an @");
        dialog.key(DialogKey::SwitchField);
        assert!(dialog.opens_file_picker(&at));
        assert!(!dialog.opens_file_picker(&DialogKey::Insert("a".into())));
```

with

```rust
        let mut dialog = TaskDialog::edit(task());
        dialog.key(DialogKey::SwitchField);
        assert_eq!(dialog.trigger(&DialogKey::Insert("a".into())), None);
```

and replace lines 663-664

```rust
        dialog.busy = true;
        assert!(!dialog.opens_file_picker(&at), "not while saving");
```

with

```rust
        dialog.busy = true;
```

2. Add after that test:

```rust
    /// `@` and `/` open a picker in the prompt only where they start something, and only while
    /// the dialog takes typing.
    #[test]
    fn the_dialog_trigger_needs_the_prompt_field_and_an_idle_dialog() {
        let (at, slash) = (DialogKey::Insert("@".into()), DialogKey::Insert("/".into()));
        let mut dialog = TaskDialog::edit(task());
        assert_eq!(
            (dialog.trigger(&at), dialog.trigger(&slash)),
            (None, None),
            "the title types them"
        );
        dialog.key(DialogKey::SwitchField);
        assert_eq!(dialog.trigger(&slash), None, "after `bug`: a path or an address");
        type_text(&mut dialog, " ");
        assert_eq!(
            (dialog.trigger(&at), dialog.trigger(&slash)),
            (Some(Trigger::File), Some(Trigger::Slash))
        );
        assert_eq!(dialog.trigger(&DialogKey::Insert("a".into())), None);
        dialog.busy = true;
        assert_eq!(dialog.trigger(&slash), None, "not while saving");
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui trigger`
Expected: compile error: `cannot find function trigger_at`, `cannot find type Trigger`, no method `trigger` on `Composers` / `TaskDialog`.

- [ ] **Step 3: Implement.**

`crates/mkrew-ui/src/composer.rs`:

- line 14: `use crate::text_box::{self, Input, TextBox};`
- in `impl Composers`, after `shown` (line 121):

```rust
    /// The picker typing `key` opens in `session`'s composer (§20B.1); none while it is hidden
    /// or sending.
    pub fn trigger(&self, session: Ulid, key: &DialogKey) -> Option<Trigger> {
        let draft = self.shown(session).filter(|draft| !draft.busy)?;
        trigger(key, draft.prompt.text.input())
    }
```

- after the `impl Composers` block (line 239), before `file_ref`:

```rust
/// What a typed char opens in a prompt box (§20B.1, §20B.7): `@` the file picker, `/` the
/// agent's command picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    File,
    Slash,
}

/// Whether a char typed at byte `cursor` of `text` starts something: the cursor is at the start
/// of the text or right after whitespace (a line break included). Anywhere else `@` and `/` are
/// part of a word (`a@b.com`, `src/main.rs`). `text` and `cursor` are as they are before the
/// char goes in; a `cursor` past the end or inside a char is not a start.
pub fn trigger_at(text: &str, cursor: usize) -> bool {
    text.get(..cursor).is_some_and(|before| {
        before
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace)
    })
}

/// The picker typing `key` opens in a prompt whose text is `input`; `None` = it types as usual.
/// A selection is replaced by the typed char, so the char goes where the selection starts.
pub fn trigger(key: &DialogKey, input: &Input) -> Option<Trigger> {
    let DialogKey::Insert(text) = key else {
        return None;
    };
    let picker = match text.as_str() {
        "@" => Trigger::File,
        "/" => Trigger::Slash,
        _ => return None,
    };
    let at = input
        .selection()
        .map_or(input.cursor(), |selection| selection.start);
    trigger_at(input.as_str(), at).then_some(picker)
}
```

`crates/mkrew-ui/src/task_dialog.rs`:
- line 11: `use crate::composer::{self, Prompt, Trigger};`
- replace lines 287-290 (`opens_file_picker` and its doc) with:

```rust
    /// The picker typing `key` opens in the prompt (§20B.1, §20B.7): `@` files, `/` the agent's
    /// commands. The title types them as they are, and so does a dialog that is saving.
    pub fn trigger(&self, key: &DialogKey) -> Option<Trigger> {
        if self.field != Field::Prompt || self.busy {
            return None;
        }
        composer::trigger(key, self.prompt.text.input())
    }
```

`crates/mkrew-ui/src/shell/composer.rs` lines 47-51, replace

```rust
        if key == DialogKey::Insert("@".into())
            && self.composers.shown(session).is_some_and(|d| !d.busy)
        {
            return self.open_file_ref_picker(composer::Target::Composer(session), window, cx);
        }
```

with

```rust
        if self.composers.trigger(session, &key) == Some(composer::Trigger::File) {
            return self.open_file_ref_picker(composer::Target::Composer(session), window, cx);
        }
```

`crates/mkrew-ui/src/shell/task_dialog.rs` lines 81-83, replace

```rust
        if (self.task_dialog.as_ref()).is_some_and(|dialog| dialog.opens_file_picker(&key)) {
            return self.open_file_ref_picker(composer::Target::TaskDialog, window, cx);
        }
```

with

```rust
        let trigger = (self.task_dialog.as_ref()).and_then(|dialog| dialog.trigger(&key));
        if trigger == Some(composer::Trigger::File) {
            return self.open_file_ref_picker(composer::Target::TaskDialog, window, cx);
        }
```

(`/` stays plain here until A16.)

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-ui trigger`
Expected: the 4 new tests pass; `cargo test -p mkrew-ui the_prompt_is_improved_like_the_composer` passes.

- [ ] **Step 5: Run the crate's tests**

Run: `cargo test -p mkrew-ui`
Expected: all pass (no other caller of `opens_file_picker` exists: it was only in `shell/task_dialog.rs` and the test).

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/composer.rs crates/mkrew-ui/src/task_dialog.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/task_dialog.rs
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): trigger_at gates the @ picker and prepares /"
```

---

### Task 14 (A14): One `Shell::prompt_key_down` for the composer and the task dialog

**Files:**
- Create: `crates/mkrew-ui/src/shell/prompt_keys.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (module list, after `mod ports;` at line 51: add `mod prompt_keys;`)
- Modify: `crates/mkrew-ui/src/shell/composer.rs` (`composer_key_down`, lines 33-83)
- Modify: `crates/mkrew-ui/src/shell/task_dialog.rs` (`task_dialog_key_down`, lines 53-111)

**Interfaces:**
- Produces: `Shell::prompt_key_down(&mut self, target: composer::Target, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<'_, Shell>) -> bool`. It types, pastes, copies, opens the `@` picker and keeps the caret in view. It returns `true` when the key was typed or opened a picker, and `false` when the key asks the box to leave (Esc, Ctrl+Enter, Enter in the dialog's title): nothing changed, the caller answers it. Part B relies on it: it replaces the `(None, TaskDialog)` arm of the paste match with an image paste.
- Consumes (existing): `composer::Trigger`, `Composers::{trigger, key, insert, shown}`, `TaskDialog::{trigger, key, insert, selected_text}`, `Shell::{open_file_ref_picker, composer_paste_image}`, `task_dialog::dialog_key`.

There is no headless harness for `Shell` (it needs a GPUI window), so this refactor has no unit test of its own: its gating rules are pinned by A13's tests, and its behavior by HUMAN_CHECKS (A17). The check here is that the old behavior is kept: the same `cargo test -p mkrew-ui` suite, clippy `-D warnings`, and the walk-through in Step 4.

- [ ] **Step 1: Baseline**

Run: `cargo test -p mkrew-ui`
Expected: all pass (the state A13 left).

- [ ] **Step 2: Create `crates/mkrew-ui/src/shell/prompt_keys.rs`**

```rust
//! Keys typed in a prompt box (SPEC §20B.1): the composer's draft and the task dialog's fields
//! share one path for typing, pasting, copying and the `@` picker.

use crate::*;

impl Shell {
    /// Applies `event` to the prompt `target`: `@` opens the file picker (at a line start or
    /// after a space), a paste takes the clipboard's text (an image too in the composer), Ctrl+C
    /// / Ctrl+X copy the selection, every other key edits. Returns `true` when the key was
    /// typed or opened a picker, `false` when it asks the box to leave (Esc, Ctrl+Enter, Enter
    /// in the dialog's title): nothing changed and the caller answers it.
    pub(crate) fn prompt_key_down(
        &mut self,
        target: composer::Target,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return true;
        };
        if self.prompt_trigger(target, &key) == Some(composer::Trigger::File) {
            self.open_file_ref_picker(target, window, cx);
            return true;
        }
        if key == DialogKey::Paste
            && let Some(item) = cx.read_from_clipboard()
        {
            match (item.text(), target) {
                (Some(text), _) => self.prompt_insert(target, &text),
                (None, composer::Target::Composer(session)) => {
                    self.composer_paste_image(session, item, window, cx);
                }
                // The dialog takes text only: attachments belong to a session (§20B.5).
                (None, composer::Target::TaskDialog) => {}
            }
        }
        if matches!(key, DialogKey::Copy | DialogKey::Cut)
            && let Some(text) = self.prompt_selection(target)
        {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
        }
        let typed = match target {
            composer::Target::Composer(session) => {
                self.composers.key(session, key) == text_box::Outcome::Edited
            }
            composer::Target::TaskDialog => self
                .task_dialog
                .as_mut()
                .is_some_and(|dialog| dialog.key(key) == task_dialog::Outcome::Edited),
        };
        if typed {
            self.prompt_follow_caret(target);
            cx.notify();
        }
        typed
    }

    /// The picker typing `key` would open in `target`.
    fn prompt_trigger(
        &self,
        target: composer::Target,
        key: &DialogKey,
    ) -> Option<composer::Trigger> {
        match target {
            composer::Target::Composer(session) => self.composers.trigger(session, key),
            composer::Target::TaskDialog => {
                (self.task_dialog.as_ref()).and_then(|dialog| dialog.trigger(key))
            }
        }
    }

    /// Pasted text goes where the caret is (the dialog's title stays on one line).
    fn prompt_insert(&mut self, target: composer::Target, text: &str) {
        match target {
            composer::Target::Composer(session) => self.composers.insert(session, text),
            composer::Target::TaskDialog => {
                if let Some(dialog) = self.task_dialog.as_mut() {
                    dialog.insert(text);
                }
            }
        }
    }

    /// The selected text of the field that has the keys.
    fn prompt_selection(&self, target: composer::Target) -> Option<String> {
        let selected = match target {
            composer::Target::Composer(session) => self
                .composers
                .shown(session)?
                .prompt
                .text
                .input()
                .selected_text(),
            composer::Target::TaskDialog => self.task_dialog.as_ref()?.selected_text(),
        };
        selected.map(str::to_owned)
    }

    /// Keeps the end of the text in view while typing at the end.
    // ponytail: follows the caret only at the end; a caret moved above the visible lines is not
    // scrolled into view (needs line-height maths).
    fn prompt_follow_caret(&self, target: composer::Target) {
        match target {
            composer::Target::Composer(session) => {
                let at_end = self.composers.shown(session).is_none_or(|draft| {
                    let input = draft.prompt.text.input();
                    input.cursor() == input.len()
                });
                if at_end {
                    self.composer_scroll.scroll_to_bottom();
                }
            }
            composer::Target::TaskDialog => {
                let at_end = self.task_dialog.as_ref().is_some_and(|dialog| {
                    let input = dialog.prompt.text.input();
                    dialog.field == Field::Prompt && input.cursor() == input.len()
                });
                if at_end {
                    self.prompt_scroll.scroll_to_bottom();
                }
            }
        }
    }
}
```

- [ ] **Step 3: Thin callers.**

`crates/mkrew-ui/src/shell.rs`: add `mod prompt_keys;` after `mod ports;`.

`crates/mkrew-ui/src/shell/composer.rs`: replace the whole `composer_key_down` (lines 33-83) with:

```rust
    /// Keys typed in the composer ([`Shell::prompt_key_down`]): Enter is a new line, Ctrl+Enter
    /// sends, Esc hides it.
    pub(crate) fn composer_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(session) = self.composer_session() else {
            return;
        };
        if self.prompt_key_down(composer::Target::Composer(session), event, window, cx) {
            return;
        }
        match task_dialog::dialog_key(&event.keystroke) {
            Some(DialogKey::Cancel) => {
                self.composers.hide(session);
                self.focus_pane(window, cx);
                cx.notify();
            }
            // Ctrl+Enter; ignored while sending or rewriting (`begin_send`).
            _ => self.send_composer(session, window, cx),
        }
    }
```

`crates/mkrew-ui/src/shell/task_dialog.rs`: replace `task_dialog_key_down` (lines 53-111, including its doc comment) with:

```rust
    /// Keys typed in the task dialog ([`Shell::prompt_key_down`]) edit the focused field;
    /// Tab / Shift+Tab move between the title and the prompt, then through the buttons; Enter
    /// (title) or Ctrl+Enter saves, Esc cancels; `@` in the prompt picks a file, as in the
    /// composer.
    pub(crate) fn task_dialog_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let edge = (self.task_dialog.as_ref())
            .is_some_and(|dialog| dialog.at_edge(!event.keystroke.modifiers.shift));
        if self.dialog_nav(event, edge, window, cx, |shell, forward| {
            if let Some(dialog) = shell.task_dialog.as_mut() {
                dialog.enter(forward);
            }
        }) {
            return;
        }
        cx.stop_propagation();
        let Some(key) = task_dialog::dialog_key(&event.keystroke) else {
            return;
        };
        // A control has the keyboard: the hidden field takes nothing but Esc / Ctrl+Enter.
        if self.dialog_control_focused(window)
            && !matches!(key, DialogKey::Cancel | DialogKey::Save)
        {
            return;
        }
        if self.prompt_key_down(composer::Target::TaskDialog, event, window, cx) {
            return;
        }
        match key {
            DialogKey::Cancel => self.close_task_dialog(window, cx),
            // Ctrl+Enter, or Enter in the title.
            _ => self.save_task(window, cx),
        }
    }
```

- [ ] **Step 4: Verify**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean.

Run: `cargo test -p mkrew-ui`
Expected: all pass.

Then walk through once in the built app (`cargo run -p mkrew-ui`): in a composer, type text, Ctrl+V text and an image, Ctrl+C / Ctrl+X a selection, `@` after a space opens the file picker, `a@b` types `@`, Ctrl+Enter sends, Esc hides; in New task, the same in the Prompt, Enter in the Title saves, Esc closes, Tab reaches the buttons.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/shell/prompt_keys.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/task_dialog.rs
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): one prompt_key_down for the composer and the task dialog"
```

---

### Task 15 (A15): `SlashPicker` model: state, filter, order

**Files:**
- Create: `crates/mkrew-ui/src/slash_picker.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (module list, after `pub mod router;` at line 80: `pub mod slash_picker;`)
- Modify: `crates/mkrew-ui/src/task_dialog.rs` (add `TaskDialog::slash_key` after `trigger`; import `crate::slash_picker::SlashKey`; one test)

**Interfaces:**
- Consumes (A1): `mkrew_protocol::{Cli, SlashItem, SlashListParams, SlashListResult, SlashSource}`. Existing: `crate::palette::fuzzy(query, text) -> Option<(i64, Vec<u32>)>` (`palette.rs:764`), `crate::quick_open::Key`, `crate::text_box::Input`, `crate::composer::Target`.
- Produces (all `pub`):
  - `type SlashKey = (String, Ulid, Option<Ulid>)` (agent profile, project, task worktree: the request and the cache key)
  - `struct Row { index: usize, matched: Vec<u32> }`, `enum Empty { Loading, UnknownCli, NoCommands, NoMatch }`
  - `fn filter(items: &[SlashItem], query: &str) -> Vec<Row>`
  - `fn source_label(source: &SlashSource) -> String`, `fn unreadable_note(count: usize) -> String`
  - `struct SlashPicker { pub target: Target, pub selected: usize, .. }` with `new(target, scope, cached: Option<Vec<SlashItem>>)`, `scope() -> &SlashKey`, `query() -> &Input`, `click_query(at, extend, clicks)`, `items()`, `rows()`, `cli()`, `error()`, `unreadable()`, `request() -> (u64, SlashListParams)`, `answered(number, Result<SlashListResult, String>) -> bool`, `retarget(scope, cached)`, `apply(Key)`, `insert(&str)`, `step(i32)`, `pick(usize)`, `accept() -> Option<String>`, `escaped() -> String`, `empty() -> Option<Empty>`
  - `TaskDialog::slash_key(&self, default_agent: Option<&str>) -> Option<SlashKey>`

Design notes pinned by the tests: members of the omp group (`skill:<name>`, `group: None`) are hidden unless the filter starts with `skill:` (case-insensitive) or, with a filter, the member's bare name starts with it and no other row's name does; the hiding applies only when the list has a group row (in Claude a command file `skill/x.md` is a plain `skill:x` row). An unchanged answer keeps the pick; a changed one goes back to the best match.

- [ ] **Step 1: Write the failing tests.** Create `crates/mkrew-ui/src/slash_picker.rs` with only the test module and the signatures it needs; first the tests (the implementation goes in Step 3, in the same file above them):

```rust
#[cfg(test)]
mod tests {
    use crate::task_dialog::DialogKey;

    use super::*;

    fn item(name: &str) -> SlashItem {
        SlashItem {
            name: name.into(),
            description: format!("About {name}"),
            argument_hint: None,
            source: SlashSource::BuiltIn,
            group: None,
        }
    }

    fn items(names: &[&str]) -> Vec<SlashItem> {
        names.iter().map(|name| item(name)).collect()
    }

    /// omp: a `skill:` group row standing for two skills, and the skills themselves.
    fn omp_items() -> Vec<SlashItem> {
        vec![
            item("help"),
            SlashItem {
                group: Some(2),
                description: "2 skills".into(),
                ..item("skill:")
            },
            item("skill:lint"),
            item("skill:test"),
            item("review"),
        ]
    }

    fn names<'a>(items: &'a [SlashItem], rows: &[Row]) -> Vec<&'a str> {
        rows.iter()
            .map(|row| items[row.index].name.as_str())
            .collect()
    }

    fn shown(picker: &SlashPicker) -> Vec<&str> {
        names(picker.items(), picker.rows())
    }

    fn answer(cli: Option<Cli>, items: Vec<SlashItem>) -> Result<SlashListResult, String> {
        Ok(SlashListResult {
            cli,
            items,
            errors: Vec::new(),
        })
    }

    fn scope() -> SlashKey {
        ("claude".into(), Ulid::new(), None)
    }

    fn type_in(picker: &mut SlashPicker, text: &str) {
        for c in text.chars() {
            picker.apply(Key::Insert(c.to_string()));
        }
    }

    #[test]
    fn an_empty_filter_keeps_the_clis_own_order() {
        let list = items(&["init", "compact", "clear"]);
        let rows = filter(&list, "");
        assert_eq!(names(&list, &rows), ["init", "compact", "clear"]);
        assert!(rows.iter().all(|row| row.matched.is_empty()));
        assert_eq!(names(&list, &filter(&list, "  ")), ["init", "compact", "clear"]);
    }

    #[test]
    fn a_filter_ranks_by_fuzzy_score_and_ties_keep_the_clis_order() {
        let list = items(&["review", "preview", "rename", "remember", "model"]);
        let rows = filter(&list, "re");
        // `preview` starts later in the word, so it scores lower although the CLI lists it
        // second; the three that start with `re` tie and keep the CLI's order.
        assert_eq!(
            names(&list, &rows),
            ["review", "rename", "remember", "preview"]
        );
        assert_eq!(rows[0].matched, vec![0, 1]);
        assert!(names(&list, &filter(&list, "zz")).is_empty());
    }

    #[test]
    fn the_skill_group_row_expands_when_the_filter_starts_with_skill() {
        let list = omp_items();
        assert_eq!(
            names(&list, &filter(&list, "")),
            ["help", "skill:", "review"],
            "the skills are behind their row"
        );
        assert_eq!(
            names(&list, &filter(&list, "skill:")),
            ["skill:lint", "skill:test"]
        );
        assert_eq!(names(&list, &filter(&list, "SKILL:te")), ["skill:test"]);
        // `sk` matches the group row's own name, so no skill shows on its bare name.
        assert_eq!(names(&list, &filter(&list, "sk")), ["skill:"]);
    }

    #[test]
    fn a_single_skill_shows_on_its_bare_name_when_no_other_row_starts_with_it() {
        let list = omp_items();
        let rows = filter(&list, "lin");
        assert_eq!(names(&list, &rows), ["skill:lint"]);
        assert_eq!(rows[0].matched, vec![6, 7, 8], "shifted past `skill:`");
        assert_eq!(names(&list, &filter(&list, "he")), ["help"]);
        assert!(
            names(&list, &filter(&list, "int")).is_empty(),
            "a bare name must start with the filter"
        );
    }

    #[test]
    fn without_a_group_row_skill_names_are_plain_rows() {
        // Claude: a command file `commands/skill/x.md` is named `skill:x`.
        let list = items(&["help", "skill:x"]);
        assert_eq!(names(&list, &filter(&list, "")), ["help", "skill:x"]);
    }

    #[test]
    fn a_pick_types_the_name_and_escape_types_what_was_typed() {
        let mut picker =
            SlashPicker::new(Target::TaskDialog, scope(), Some(items(&["review", "init"])));
        assert_eq!(picker.accept().as_deref(), Some("/review "));
        picker.step(1);
        assert_eq!(picker.accept().as_deref(), Some("/init "));
        picker.step(1);
        assert_eq!(picker.selected, 0, "wraps");
        picker.pick(1);
        assert_eq!(picker.selected, 1);
        picker.pick(9);
        assert_eq!(picker.selected, 1, "out of range is ignored");

        type_in(&mut picker, "ini");
        assert_eq!(picker.escaped(), "/ini");
        assert_eq!(picker.accept().as_deref(), Some("/init "));
        type_in(&mut picker, "zz");
        assert_eq!(picker.empty(), Some(Empty::NoMatch));
        assert_eq!(picker.accept(), None);
        assert_eq!(picker.escaped(), "/inizz");
        picker.apply(Key::Edit(DialogKey::Backspace));
        picker.apply(Key::Edit(DialogKey::Backspace));
        assert_eq!(picker.empty(), None);
    }

    #[test]
    fn a_group_row_sets_the_filter_and_types_nothing() {
        let mut picker = SlashPicker::new(Target::TaskDialog, scope(), Some(omp_items()));
        picker.step(1);
        assert_eq!(shown(&picker)[picker.selected], "skill:");
        assert_eq!(picker.accept(), None, "nothing is typed");
        assert_eq!(picker.query().as_str(), "skill:");
        assert_eq!(shown(&picker), ["skill:lint", "skill:test"]);
        assert_eq!(picker.selected, 0);
        assert_eq!(picker.escaped(), "/skill:");
        assert_eq!(picker.accept().as_deref(), Some("/skill:lint "));
    }

    #[test]
    fn only_the_answer_to_the_latest_request_is_shown() {
        let mut picker = SlashPicker::new(Target::TaskDialog, scope(), None);
        let (first, params) = picker.request();
        assert_eq!(params.profile_id, "claude");
        assert_eq!(picker.empty(), Some(Empty::Loading));
        // Try again: a newer request supersedes the first.
        let (second, _) = picker.request();
        assert!(!picker.answered(first, answer(Some(Cli::Claude), items(&["old"]))));
        assert!(picker.items().is_empty());
        assert!(picker.answered(second, answer(Some(Cli::Claude), items(&["new"]))));
        assert_eq!(shown(&picker), ["new"]);

        // Numbers are unique across pickers: an answer meant for a closed one never lands in
        // the next.
        let mut next = SlashPicker::new(Target::TaskDialog, scope(), None);
        let (third, _) = next.request();
        assert!(third > second);
        assert!(!next.answered(second, answer(Some(Cli::Claude), items(&["late"]))));
        assert!(next.items().is_empty());
    }

    #[test]
    fn an_agent_change_replaces_the_rows_and_keeps_the_filter() {
        let project = Ulid::new();
        let claude: SlashKey = ("claude".into(), project, None);
        let mut picker = SlashPicker::new(
            Target::TaskDialog,
            claude,
            Some(items(&["review", "init"])),
        );
        type_in(&mut picker, "re");
        assert_eq!(shown(&picker), ["review"]);
        let (claude_request, _) = picker.request();

        picker.retarget(
            ("codex".into(), project, None),
            Some(items(&["resume", "model"])),
        );
        assert_eq!(picker.query().as_str(), "re", "the filter stays");
        assert_eq!(shown(&picker), ["resume"]);
        let (codex_request, params) = picker.request();
        assert_eq!(
            (params.profile_id.as_str(), params.project_id),
            ("codex", project)
        );
        assert!(
            !picker.answered(claude_request, answer(Some(Cli::Claude), items(&["review"]))),
            "claude's late answer"
        );
        assert!(picker.answered(
            codex_request,
            answer(Some(Cli::Codex), items(&["resume", "model", "review"]))
        ));
        assert_eq!(shown(&picker), ["resume", "review"], "a tie keeps the CLI's order");

        // Nothing cached for the next agent: loading, still filtered by `re`.
        picker.retarget(("omp".into(), project, None), None);
        assert_eq!(picker.query().as_str(), "re");
        assert_eq!(picker.empty(), Some(Empty::Loading));
        assert_eq!(picker.cli(), None);
    }

    #[test]
    fn the_empty_states_say_why_nothing_is_listed() {
        let mut picker = SlashPicker::new(Target::TaskDialog, scope(), None);
        assert_eq!(picker.empty(), Some(Empty::Loading));

        let (number, _) = picker.request();
        picker.answered(number, Err("agent profile x not found".into()));
        assert_eq!(picker.empty(), None, "the error line says it");
        assert_eq!(picker.error(), Some("agent profile x not found"));
        let (number, _) = picker.request();
        assert_eq!(picker.error(), None, "Try again clears it");
        assert_eq!(picker.empty(), Some(Empty::Loading));

        picker.answered(number, answer(None, Vec::new()));
        assert_eq!(picker.empty(), Some(Empty::UnknownCli));
        let (number, _) = picker.request();
        picker.answered(number, answer(Some(Cli::Omp), Vec::new()));
        assert_eq!(picker.empty(), Some(Empty::NoCommands));
        let (number, _) = picker.request();
        picker.answered(number, answer(Some(Cli::Omp), items(&["model"])));
        assert_eq!(picker.empty(), None);
        type_in(&mut picker, "zz");
        assert_eq!(picker.empty(), Some(Empty::NoMatch));
    }

    #[test]
    fn cached_rows_show_at_once_and_a_failed_refresh_keeps_them() {
        let mut picker =
            SlashPicker::new(Target::TaskDialog, scope(), Some(items(&["a", "b", "c"])));
        assert_eq!(shown(&picker), ["a", "b", "c"], "no wait for the daemon");
        assert_eq!(picker.empty(), None);
        picker.step(2);

        let (number, _) = picker.request();
        picker.answered(number, Err("boom".into()));
        assert_eq!(shown(&picker), ["a", "b", "c"]);
        assert_eq!(picker.error(), Some("boom"));

        // The same rows again: the pick stays; different rows: back to the best match.
        let (number, _) = picker.request();
        picker.answered(number, answer(Some(Cli::Claude), items(&["a", "b", "c"])));
        assert_eq!((picker.selected, picker.error()), (2, None));
        assert_eq!(picker.cli(), Some(Cli::Claude));
        let (number, _) = picker.request();
        picker.answered(number, answer(Some(Cli::Claude), items(&["a", "b", "c", "d"])));
        assert_eq!(picker.selected, 0);
    }

    #[test]
    fn unreadable_files_are_counted_for_the_footer() {
        let mut picker = SlashPicker::new(Target::TaskDialog, scope(), None);
        let (number, _) = picker.request();
        picker.answered(
            number,
            Ok(SlashListResult {
                cli: Some(Cli::Claude),
                items: items(&["init"]),
                errors: vec!["a.md: cannot read".into(), "b.md: cannot read".into()],
            }),
        );
        assert_eq!(picker.unreadable().len(), 2);
        assert_eq!(unreadable_note(1), "1 file could not be read");
        assert_eq!(unreadable_note(3), "3 files could not be read");
    }

    #[test]
    fn the_chip_names_the_source() {
        let label = |source| source_label(&source);
        assert_eq!(label(SlashSource::BuiltIn), "built-in");
        assert_eq!(label(SlashSource::Project), "project");
        assert_eq!(label(SlashSource::User), "user");
        assert_eq!(label(SlashSource::Managed), "managed");
        assert_eq!(label(SlashSource::Plugin("acme".into())), "plugin: acme");
        assert_eq!(label(SlashSource::Profile), "profile");
    }
}
```

In `crates/mkrew-ui/src/task_dialog.rs` tests add:

```rust
    /// Which agent and folder the `/` list is for (§20B.7): the picked agent, else the workspace
    /// default; the project folder, or an edited task's worktree.
    #[test]
    fn the_slash_list_is_for_the_picked_agent_in_the_project_or_the_tasks_worktree() {
        let mut dialog = TaskDialog::new(Ulid::new());
        let project = dialog.project_id();
        assert_eq!(dialog.slash_key(None), None, "no agent to ask about");
        assert_eq!(
            dialog.slash_key(Some("claude")),
            Some(("claude".into(), project, None))
        );
        dialog.choose_agent("codex");
        assert_eq!(
            dialog.slash_key(Some("claude")),
            Some(("codex".into(), project, None)),
            "the picked agent wins"
        );

        let mut saved = task();
        let dialog = TaskDialog::edit(saved.clone());
        assert_eq!(
            dialog.slash_key(None),
            Some(("codex".into(), saved.project_id, None)),
            "no worktree yet"
        );
        saved.worktree_path = Some("C:/wt/app".into());
        let dialog = TaskDialog::edit(saved.clone());
        assert_eq!(
            dialog.slash_key(None),
            Some(("codex".into(), saved.project_id, Some(saved.id)))
        );
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui slash`
Expected: compile errors, `SlashPicker`, `filter`, `SlashKey`, `source_label`, `TaskDialog::slash_key` not found.

- [ ] **Step 3: Implement.** In `crates/mkrew-ui/src/lib.rs` add `pub mod slash_picker;` after `pub mod router;`. Put this above the test module in `crates/mkrew-ui/src/slash_picker.rs`:

```rust
//! The `/` command picker (SPEC §20B.7, ADR 0094): typing `/` in the composer or the task
//! dialog's Prompt lists what typing `/` lists in the agent's own terminal. The daemon builds
//! the list (`slash.list`); this keeps the latest answer, the filter and the pick. Pure and
//! tested headlessly; `shell/slash.rs` renders it and reads keys with
//! [`crate::quick_open::key`].

use std::sync::atomic::{AtomicU64, Ordering};

use mkrew_protocol::{Cli, SlashItem, SlashListParams, SlashListResult, SlashSource};
use ulid::Ulid;

use crate::composer::Target;
use crate::palette::fuzzy;
use crate::quick_open::Key;
use crate::text_box::Input;

/// The name of omp's group row and the prefix of the skills it stands for.
const SKILL_PREFIX: &str = "skill:";

/// What one `slash.list` answers for, and the cache key: the agent profile, the project and the
/// task worktree (`None` = the project folder).
pub type SlashKey = (String, Ulid, Option<Ulid>);

/// One shown row: an index into [`SlashPicker::items`] and the matched chars of its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub index: usize,
    pub matched: Vec<u32>,
}

/// Why no row is shown (the picker says so in one sentence).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Empty {
    /// No answer yet and nothing cached.
    Loading,
    /// The profile's CLI is unknown: Mkrew cannot list anything.
    UnknownCli,
    /// The CLI is known and every row is hidden or absent.
    NoCommands,
    /// Rows exist but the filter matches none.
    NoMatch,
}

/// The rows to show for `query`: with an empty one the CLI's own order; with one, the fuzzy
/// score on the name, ties in the CLI's order. In omp's list (it has a group row) the skills
/// stay behind their `skill:` row until the filter starts with `skill:`, except a skill whose
/// bare name starts with the filter while no other row's name does.
pub fn filter(items: &[SlashItem], query: &str) -> Vec<Row> {
    let query = query.trim();
    let grouped = items.iter().any(|item| item.group.is_some());
    let skill =
        |item: &SlashItem| grouped && item.group.is_none() && item.name.starts_with(SKILL_PREFIX);
    if query.is_empty() {
        return items
            .iter()
            .enumerate()
            .filter(|(_, item)| !skill(item))
            .map(|(index, _)| Row {
                index,
                matched: Vec::new(),
            })
            .collect();
    }
    let expanded = grouped
        && query
            .get(..SKILL_PREFIX.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(SKILL_PREFIX));
    let mut found: Vec<(i64, Row)> = items
        .iter()
        .enumerate()
        // Expanded: the skills are listed, so their group row is redundant.
        .filter(|(_, item)| if expanded { item.group.is_none() } else { !skill(item) })
        .filter_map(|(index, item)| {
            let (score, matched) = fuzzy(query, &item.name)?;
            Some((score, Row { index, matched }))
        })
        .collect();
    if grouped && !expanded {
        let wanted = query.to_lowercase();
        let prefixed = |name: &str| name.to_lowercase().starts_with(&wanted);
        let taken = found
            .iter()
            .any(|(_, row)| items.get(row.index).is_some_and(|item| prefixed(&item.name)));
        if !taken {
            let shift = u32::try_from(SKILL_PREFIX.len()).unwrap_or_default();
            for (index, item) in items.iter().enumerate().filter(|(_, item)| skill(item)) {
                let Some(bare) = item.name.strip_prefix(SKILL_PREFIX) else {
                    continue;
                };
                if prefixed(bare)
                    && let Some((score, matched)) = fuzzy(query, bare)
                {
                    let matched = matched.into_iter().map(|at| at + shift).collect();
                    found.push((score, Row { index, matched }));
                }
            }
        }
    }
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.index.cmp(&b.1.index)));
    found.into_iter().map(|(_, row)| row).collect()
}

/// The chip at the right of a row.
pub fn source_label(source: &SlashSource) -> String {
    match source {
        SlashSource::BuiltIn => "built-in".to_owned(),
        SlashSource::Project => "project".to_owned(),
        SlashSource::User => "user".to_owned(),
        SlashSource::Managed => "managed".to_owned(),
        SlashSource::Plugin(name) => format!("plugin: {name}"),
        SlashSource::Profile => "profile".to_owned(),
    }
}

/// The footer note for files the scan could not read.
pub fn unreadable_note(count: usize) -> String {
    match count {
        1 => "1 file could not be read".to_owned(),
        count => format!("{count} files could not be read"),
    }
}

/// The open picker.
#[derive(Debug, Clone)]
pub struct SlashPicker {
    /// Where the pick is typed.
    pub target: Target,
    /// Index into [`Self::rows`] of the pick.
    pub selected: usize,
    scope: SlashKey,
    query: Input,
    items: Vec<SlashItem>,
    cli: Option<Cli>,
    /// Files the scan could not read, one line each.
    errors: Vec<String>,
    /// Why the last `slash.list` failed; the rows of an earlier answer stay.
    error: Option<String>,
    /// An answer or cached rows were seen: an empty list is then a fact, not a wait.
    ready: bool,
    /// Private: every change of `items` or `query` must filter again.
    rows: Vec<Row>,
    /// Number of the last request sent; older answers are dropped.
    sent: u64,
}

impl SlashPicker {
    /// A picker for `scope` showing the `cached` rows of an earlier answer at once (`None` =
    /// loading). The caller sends [`Self::request`].
    pub fn new(target: Target, scope: SlashKey, cached: Option<Vec<SlashItem>>) -> Self {
        let mut picker = Self {
            target,
            selected: 0,
            scope,
            query: Input::default(),
            items: Vec::new(),
            cli: None,
            errors: Vec::new(),
            error: None,
            ready: false,
            rows: Vec::new(),
            sent: 0,
        };
        picker.show(cached);
        picker
    }

    /// Takes `cached` as the rows (`None` = loading) and forgets what the last answer said.
    fn show(&mut self, cached: Option<Vec<SlashItem>>) {
        self.ready = cached.is_some();
        self.items = cached.unwrap_or_default();
        self.cli = None;
        self.errors.clear();
        self.error = None;
        self.refilter();
    }

    fn refilter(&mut self) {
        self.rows = filter(&self.items, self.query.as_str());
        self.selected = 0;
    }

    /// The agent changed (the dialog's Agent chips): the rows of `scope` replace these, the
    /// filter text stays. The caller sends a new [`Self::request`]; answers to the old one are
    /// dropped.
    pub fn retarget(&mut self, scope: SlashKey, cached: Option<Vec<SlashItem>>) {
        self.scope = scope;
        self.show(cached);
    }

    pub fn scope(&self) -> &SlashKey {
        &self.scope
    }

    pub fn query(&self) -> &Input {
        &self.query
    }

    /// A press or drag at byte `at` of the filter (see `Input::click`).
    pub fn click_query(&mut self, at: usize, extend: bool, clicks: usize) {
        self.query.click(at, extend, clicks);
    }

    pub fn items(&self) -> &[SlashItem] {
        &self.items
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The CLI of the last answer (`None` before one, and when the CLI is unknown).
    pub fn cli(&self) -> Option<Cli> {
        self.cli
    }

    /// Why the last request failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Files the last answer could not read.
    pub fn unreadable(&self) -> &[String] {
        &self.errors
    }

    /// The `slash.list` for the scope as it is now, with its number for [`Self::answered`].
    pub fn request(&mut self) -> (u64, SlashListParams) {
        static SENT: AtomicU64 = AtomicU64::new(0);
        self.sent = SENT.fetch_add(1, Ordering::Relaxed) + 1;
        self.error = None;
        let (profile_id, project_id, worktree_task_id) = self.scope.clone();
        (
            self.sent,
            SlashListParams {
                profile_id,
                project_id,
                worktree_task_id,
            },
        )
    }

    /// The answer to request `number`; kept only if no newer request was sent (a stale one is
    /// dropped). Rows that did not change keep the pick; new rows put it back on the best
    /// match. A failure keeps the rows already shown. Returns `true` when the answer was kept.
    pub fn answered(&mut self, number: u64, answer: Result<SlashListResult, String>) -> bool {
        if number != self.sent {
            return false;
        }
        match answer {
            Ok(result) => {
                let changed = result.items != self.items;
                (self.ready, self.cli, self.errors, self.error) =
                    (true, result.cli, result.errors, None);
                if changed {
                    self.items = result.items;
                    self.refilter();
                }
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    /// Applies a key to the filter and the pick; Enter, Tab and Esc are the caller's
    /// ([`Self::accept`], [`Self::escaped`]).
    pub fn apply(&mut self, key: Key) {
        match key {
            Key::Move(by) => self.step(by),
            Key::Edit(key) => {
                if self.query.edit(&key) {
                    self.refilter();
                }
            }
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => {}
        }
    }

    /// Types or pastes `text` into the filter (line breaks dropped: a name is one line).
    pub fn insert(&mut self, text: &str) {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        if self.query.insert(&line) {
            self.refilter();
        }
    }

    /// Moves the pick by `by`, wrapping around the rows.
    pub fn step(&mut self, by: i32) {
        let count = self.rows.len();
        if count == 0 {
            return;
        }
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        self.selected = usize::try_from((at + i64::from(by)).rem_euclid(count)).unwrap_or(0);
    }

    /// Picks row `index` (a click).
    pub fn pick(&mut self, index: usize) {
        if index < self.rows.len() {
            self.selected = index;
        }
    }

    /// Enter or Tab: the text to type for the pick, `/name `. A group row types nothing: it
    /// sets the filter to `skill:` and the skills are listed. `None` when nothing is picked.
    pub fn accept(&mut self) -> Option<String> {
        let item = self.items.get(self.rows.get(self.selected)?.index)?;
        if item.group.is_some() {
            self.query.clear();
            self.query.insert(SKILL_PREFIX);
            self.refilter();
            return None;
        }
        Some(format!("/{} ", item.name))
    }

    /// Esc: the `/` that opened the picker and what was typed after it.
    pub fn escaped(&self) -> String {
        format!("/{}", self.query.as_str())
    }

    /// Why no row is shown, if none is. A failed request with no rows is not an empty state:
    /// the error line says it.
    pub fn empty(&self) -> Option<Empty> {
        if !self.rows.is_empty() || self.error.is_some() {
            return None;
        }
        if !self.ready {
            return Some(Empty::Loading);
        }
        if self.items.is_empty() {
            return Some(if self.cli.is_none() {
                Empty::UnknownCli
            } else {
                Empty::NoCommands
            });
        }
        Some(Empty::NoMatch)
    }
}
```

In `crates/mkrew-ui/src/task_dialog.rs`: add `use crate::slash_picker::SlashKey;` and, after `TaskDialog::trigger`:

```rust
    /// What the `/` picker lists for (§20B.7): the agent picked here, else the workspace's
    /// `default_agent`, in the project folder; or in the task's worktree when an edited task
    /// has one (a new task has none until it runs, §20E.1). `None`: no agent to ask about.
    pub fn slash_key(&self, default_agent: Option<&str>) -> Option<SlashKey> {
        let profile = self.agent.as_deref().or(default_agent)?;
        let worktree = match &self.mode {
            Mode::Edit(task) if task.worktree_path.is_some() => Some(task.id),
            _ => None,
        };
        Some((profile.to_owned(), self.project_id(), worktree))
    }
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-ui slash`
Expected: all pass (the 12 picker tests and `the_slash_list_is_for_the_picked_agent_in_the_project_or_the_tasks_worktree`).

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. (`SlashPicker` is `pub` in the lib, so nothing is dead code before A16.)

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/slash_picker.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/task_dialog.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): SlashPicker model with fuzzy filter, cache scope and omp skill group"
```

---

### Task 16 (A16): Shell wiring and render of the `/` picker

**Files:**
- Create: `crates/mkrew-ui/src/shell/slash.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (after `mod settings;` at line 54: `mod slash;`)
- Modify: `crates/mkrew-ui/src/main.rs` (protocol import at lines 29-45: add `SlashItem` between `SessionUsageEvent` and `StateSource`; import after line 106: `use mkrew_ui::slash_picker::{self, SlashKey, SlashPicker};`; fields after `prompt_picker_scroll` at line 836; render child after the prompt picker's at lines 1087-1091)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (constructor after line 465; `close_popups` after line 757; `reset_connection_state` after line 803)
- Modify: `crates/mkrew-ui/src/shell/focus.rs` (`modal_open`, after line 26)
- Modify: `crates/mkrew-ui/src/shell/prompt_keys.rs` (the trigger `if` from A14)
- Modify: `crates/mkrew-ui/src/shell/task_dialog.rs` (dialog-root condition at lines 429-431; Agent chip click at lines 349-354)
- Modify: `crates/mkrew-ui/src/shell/composer.rs` (tooltip at line 888)

**Interfaces:**
- Consumes: everything A15 produced; `Request::SlashList`, `mkrew_protocol::{Cli, SlashListResult}`; existing `Shell::{prompt_typed, focus_pane, local_only, toast_error, typing, modal_shade}`, `quick_open::{key, match_ranges, Key}`, `widgets::{button, button_fg, icon_text, caret_field, tooltip}`, `zed::open`, tokens in `theme.rs`.
- Produces: `Shell::{open_slash_picker, request_slash, slash_picker_key_down, accept_slash_pick, close_slash_picker, retarget_slash_picker, open_slash_profile, render_slash_picker}`; fields `slash_picker: Option<SlashPicker>`, `slash_picker_scroll: ScrollHandle`, `slash_cache: HashMap<SlashKey, Vec<SlashItem>>`.

UI rules (skill `mkrew-ui-design`, ADR 0085): tokens from `theme.rs` only (`space::*` are the 4/8/12/16/24/32 steps, `radius`, `font_size`, `opacity`, `theme::DIALOG_WIDTH`, `theme::QUICK_OPEN_LIST_HEIGHT`, `theme::BORDER_WIDTH`), dark and light through `Theme`, every list has empty states, rows have hover, active and the focus ring, the picker is keyboard-only usable, and the shortcut keys are in the footer. No new palette command: the picker is a typing aid, like `@` (spec 4.6). No new theme token is needed.

There is no headless harness for `Shell`; the model carries the tests (A15). The checks here are the build, clippy, the existing suite and one look in both themes.

- [ ] **Step 1: Shell state.** In `crates/mkrew-ui/src/main.rs`:

- add `SlashItem` to the `mkrew_protocol` import (alphabetical: after `SessionUsageEvent`, before `StateSource`);
- after `use mkrew_ui::sidebar::{...};` (line 106) add `use mkrew_ui::slash_picker::{self, SlashKey, SlashPicker};`;
- after `prompt_picker_scroll: ScrollHandle,` (line 836) add:

```rust
    /// The open `/` command picker (modal; over the task dialog when it types into it).
    slash_picker: Option<SlashPicker>,
    /// Keeps the `/` picker's pick in view.
    slash_picker_scroll: ScrollHandle,
    /// The last `slash.list` answer per (agent profile, project, task worktree): shown at once
    /// when the picker opens again, then refreshed (§20B.7). Lives for the connection.
    slash_cache: HashMap<SlashKey, Vec<SlashItem>>,
```

- after the prompt picker's `.children(...)` (lines 1087-1091) add:

```rust
                    .children(
                        self.slash_picker
                            .as_ref()
                            .map(|picker| self.render_slash_picker(picker, &theme, cx)),
                    )
```

In `crates/mkrew-ui/src/shell/connection.rs`: after `prompt_picker_scroll: ScrollHandle::new(),` (line 465) add `slash_picker: None, slash_picker_scroll: ScrollHandle::new(), slash_cache: HashMap::new(),`; in `close_popups` after `self.prompt_picker = None;` (line 757) add `self.slash_picker = None;`; in `reset_connection_state` after `self.diagram_dialog = None;` (line 803) add `self.slash_cache.clear();`.

In `crates/mkrew-ui/src/shell/focus.rs` `modal_open`, after `|| self.prompt_picker.is_some()` (line 26) add `|| self.slash_picker.is_some()`.

In `crates/mkrew-ui/src/shell.rs` add `mod slash;` after `mod settings;`.

In `crates/mkrew-ui/src/shell/task_dialog.rs`:
- the dialog root takes keys unless a picker is over it (lines 429-431): replace `self.prompt_picker.is_none() && self.quick_open.is_none(),` with

```rust
                self.prompt_picker.is_none()
                    && self.quick_open.is_none()
                    && self.slash_picker.is_none(),
```

- the Agent chip click (lines 349-354): replace

```rust
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                if let Some(dialog) = shell.task_dialog.as_mut() {
                                    dialog.choose_agent(&agent);
                                    cx.notify();
                                }
                            }))
```

with

```rust
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                if let Some(dialog) = shell.task_dialog.as_mut() {
                                    dialog.choose_agent(&agent);
                                }
                                // An open `/` picker lists the new agent's commands (§20B.7).
                                shell.retarget_slash_picker(window, cx);
                                cx.notify();
                            }))
```

In `crates/mkrew-ui/src/shell/composer.rs` line 888 change the tooltip text to `"Insert a prompt from the library (type / in the box for the agent's commands)"`.

In `crates/mkrew-ui/src/shell/prompt_keys.rs` replace the A14 trigger check

```rust
        if self.prompt_trigger(target, &key) == Some(composer::Trigger::File) {
            self.open_file_ref_picker(target, window, cx);
            return true;
        }
```

with

```rust
        if let Some(trigger) = self.prompt_trigger(target, &key) {
            match trigger {
                composer::Trigger::File => self.open_file_ref_picker(target, window, cx),
                composer::Trigger::Slash => self.open_slash_picker(target, window, cx),
            }
            return true;
        }
```

and update its doc sentence to "`@` opens the file picker and `/` the agent's command picker (at a line start or after a space)".

- [ ] **Step 2: Create `crates/mkrew-ui/src/shell/slash.rs`: open, request, keys, close**

```rust
//! The `/` command picker over a prompt box (SPEC §20B.7, ADR 0094): opening it, asking the
//! daemon, keys and rendering. The model is [`slash_picker::SlashPicker`].

use crate::*;

impl Shell {
    /// What the picker for `target` lists: the composer's session agent in its project (its task
    /// worktree when the task has one), or the dialog's agent (§4.4 of the design spec).
    fn slash_key(&self, target: composer::Target) -> Option<SlashKey> {
        match target {
            composer::Target::Composer(session) => {
                let session = self.sidebar.sessions().iter().find(|s| s.id == session)?;
                let worktree = (session.task_id)
                    .and_then(|id| self.board.task(id))
                    .filter(|task| task.worktree_path.is_some())
                    .map(|task| task.id);
                Some((session.agent_profile.clone(), session.project_id, worktree))
            }
            composer::Target::TaskDialog => {
                let dialog = self.task_dialog.as_ref()?;
                let default_agent = (self.sidebar.projects().iter())
                    .find(|project| project.id == dialog.project_id())
                    .and_then(|project| project.default_agent.as_deref());
                dialog.slash_key(default_agent)
            }
        }
    }

    /// `/` in a prompt box: the picker over `target`, the cached rows at once and a fresh
    /// `slash.list`. With no agent to ask about, or no daemon, the `/` is typed as is.
    pub(crate) fn open_slash_picker(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(scope) = self.slash_key(target).filter(|_| self.connection.is_some()) else {
            self.prompt_typed(target, "/", window, cx);
            cx.notify();
            return;
        };
        let cached = self.slash_cache.get(&scope).cloned();
        self.workspace_menu = None;
        self.slash_picker = Some(SlashPicker::new(target, scope, cached));
        self.slash_picker_scroll.scroll_to_item(0);
        window.focus(&self.dialog_focus, cx);
        self.request_slash(window, cx);
        cx.notify();
    }

    /// `slash.list` for the open picker's scope. The answer fills the cache of that scope
    /// whichever picker is open by then; the picker takes it only if it is still the latest
    /// request (a stale answer is dropped).
    pub(crate) fn request_slash(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(picker) = self.slash_picker.as_mut() else {
            return;
        };
        let scope = picker.scope().clone();
        let (number, params) = picker.request();
        let listed = connection.call::<mkrew_protocol::SlashListResult>(Request::SlashList(params));
        cx.spawn_in(window, async move |this, cx| {
            let answer = listed.await.map_err(|error| format!("{error:#}"));
            let _ = this.update(cx, |shell, cx| {
                match &answer {
                    Ok(result) => {
                        shell.slash_cache.insert(scope, result.items.clone());
                    }
                    Err(error) => {
                        tracing::warn!(profile = %scope.0, "slash.list failed: {error}");
                    }
                }
                if let Some(picker) = shell.slash_picker.as_mut()
                    && picker.answered(number, answer)
                {
                    shell.slash_picker_scroll.scroll_to_item(picker.selected);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The dialog's Agent changed while the picker is open: the new agent's cached rows replace
    /// the old ones (the filter stays) and its list is asked for.
    pub(crate) fn retarget_slash_picker(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(picker) = self
            .slash_picker
            .as_ref()
            .filter(|picker| picker.target == composer::Target::TaskDialog)
        else {
            return;
        };
        let Some(scope) = self.slash_key(composer::Target::TaskDialog) else {
            return;
        };
        if &scope == picker.scope() {
            return;
        }
        let cached = self.slash_cache.get(&scope).cloned();
        if let Some(picker) = self.slash_picker.as_mut() {
            picker.retarget(scope, cached);
        }
        self.request_slash(window, cx);
        cx.notify();
    }

    /// Closes the picker: `typed` (a pick, or the `/` and filter Esc gives back) goes into its
    /// target, which gets the keyboard; `None` closes without typing.
    pub(crate) fn close_slash_picker(
        &mut self,
        typed: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(picker) = self.slash_picker.take() else {
            return;
        };
        match (typed, picker.target) {
            (Some(text), target) => self.prompt_typed(target, &text, window, cx),
            (None, composer::Target::Composer(session))
                if self.composer_session() == Some(session) =>
            {
                window.focus(&self.composer_focus, cx);
            }
            (None, _) => self.focus_pane(window, cx),
        }
        cx.notify();
    }

    /// Enter, Tab or a click: types the pick; a group row only expands (its skills are listed).
    pub(crate) fn accept_slash_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(picker) = self.slash_picker.as_mut() else {
            return;
        };
        match picker.accept() {
            Some(text) => self.close_slash_picker(Some(text), window, cx),
            None => {
                self.slash_picker_scroll.scroll_to_item(0);
                cx.notify();
            }
        }
    }

    /// "Open profile": the agent's profile file in Zed, to set `cli` there. A machine window's
    /// profile is on the other computer, so it is refused there.
    pub(crate) fn open_slash_profile(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(profile) = self
            .slash_picker
            .as_ref()
            .map(|picker| picker.scope().0.clone())
        else {
            return;
        };
        if self.local_only(cx) {
            return;
        }
        let path = self.config_dir.join("agents").join(format!("{profile}.toml"));
        if let Err(error) = zed::open(&path, None) {
            tracing::warn!(path = %path.display(), "open agent profile failed: {error}");
            self.toast_error(error.to_string(), cx);
        }
        self.close_slash_picker(None, window, cx);
    }

    /// Typing filters; ↑ / ↓ move the pick (kept in view); Enter or Tab types it; Esc types the
    /// `/` and what was typed after it.
    pub(crate) fn slash_picker_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        // Tab picks like Enter: it is the completion key of the agents' own `/` popups.
        let key = if task_dialog::dialog_key(&event.keystroke) == Some(DialogKey::SwitchField) {
            Some(quick_open::Key::Open)
        } else {
            quick_open::key(&event.keystroke)
        };
        let Some(key) = key else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(picker) = self.slash_picker.as_mut() else {
            return;
        };
        match key {
            quick_open::Key::Close => {
                let typed = picker.escaped();
                return self.close_slash_picker(Some(typed), window, cx);
            }
            quick_open::Key::Open => return self.accept_slash_pick(window, cx),
            quick_open::Key::Move(_) => {
                picker.apply(key);
                self.slash_picker_scroll.scroll_to_item(picker.selected);
            }
            quick_open::Key::Paste => {
                if let Some(text) = paste {
                    picker.insert(&text);
                }
            }
            quick_open::Key::Edit(_) | quick_open::Key::Insert(_) => {
                if let quick_open::Key::Edit(edit) = &key {
                    copy_selection(edit, picker.query(), cx);
                }
                picker.apply(key);
            }
        }
        cx.notify();
    }
}
```

- [ ] **Step 3: Render.** Append to `crates/mkrew-ui/src/shell/slash.rs` (inside a second `impl Shell`, or at the end of the first):

```rust
impl Shell {
    /// The picker: the filter field over the rows (`/name` in the mono font with its matched
    /// chars in the accent color, the argument hint and the one-line description muted, a source
    /// chip on the right, the pick ringed), then the note that says why there are none, the
    /// error line, and the footer. A click or Enter types the pick.
    pub(crate) fn render_slash_picker(
        &self,
        picker: &SlashPicker,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let hover = theme.surface_raised;
        let muted = |text: String| {
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let rows: Vec<gpui::AnyElement> = picker
            .rows()
            .iter()
            .enumerate()
            .filter_map(|(position, row)| {
                let item = picker.items().get(row.index)?;
                let picked = position == picker.selected;
                let name = format!("/{}", item.name);
                // The `/` comes first in the text, so the matched chars move one place.
                let shifted: Vec<u32> = row.matched.iter().map(|at| at + 1).collect();
                let highlights = quick_open::match_ranges(&name, &shifted)
                    .into_iter()
                    .map(|range| {
                        let style = gpui::HighlightStyle {
                            color: Some(accent),
                            font_weight: Some(FontWeight(weight::SEMIBOLD)),
                            ..gpui::HighlightStyle::default()
                        };
                        (range, style)
                    });
                Some(
                    div()
                        .id(("slash-row", position))
                        .flex_none()
                        .flex()
                        .items_center()
                        .gap(px(space::M))
                        .px(px(space::S))
                        .py(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        // The pick has a focus-colored ring, so it stays visible under hover.
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(rgb(if picked {
                            theme.pane_focus_border
                        } else {
                            theme.surface
                        }))
                        .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                        .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                        .cursor_pointer()
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            if let Some(picker) = shell.slash_picker.as_mut() {
                                picker.pick(position);
                            }
                            shell.accept_slash_pick(window, cx);
                        }))
                        .child(
                            div()
                                .flex_none()
                                .font_family(theme.terminal_font_family.clone())
                                .child(gpui::StyledText::new(name).with_highlights(highlights)),
                        )
                        .children(item.argument_hint.clone().map(|hint| {
                            div()
                                .flex_none()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(hint)
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(item.description.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .px(px(space::XS))
                                .rounded(px(radius::SMALL))
                                .border(px(theme::BORDER_WIDTH))
                                .border_color(rgb(theme.border))
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(slash_picker::source_label(&item.source)),
                        )
                        .into_any_element(),
                )
            })
            .collect();
        let note = picker.empty().map(|empty| match empty {
            slash_picker::Empty::Loading => muted("Loading commands…".to_owned()).into_any_element(),
            slash_picker::Empty::UnknownCli => div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .child(div().flex_1().min_w_0().child(muted(
                    "Mkrew doesn't know which CLI this agent runs. Set `cli` in its profile."
                        .to_owned(),
                )))
                .child(
                    button(
                        "slash-open-profile",
                        icon_text(Icon::FileText, "Open profile", button_fg(theme, true)),
                        theme,
                        true,
                    )
                    .flex_none()
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.open_slash_profile(window, cx);
                    })),
                )
                .into_any_element(),
            slash_picker::Empty::NoCommands => {
                muted("This agent lists no commands.".to_owned()).into_any_element()
            }
            slash_picker::Empty::NoMatch => muted(format!(
                "No commands match \"/{}\". Esc types it as typed.",
                picker.query().as_str().trim()
            ))
            .into_any_element(),
        });
        let failure = picker.error().map(|error| {
            div()
                .flex()
                .items_center()
                .gap(px(space::M))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                        .child(div().text_color(rgb(theme.text)).child(error.to_owned())),
                )
                .child(
                    button(
                        "slash-retry",
                        icon_text(Icon::RotateCw, "Try again", button_fg(theme, true)),
                        theme,
                        true,
                    )
                    .flex_none()
                    .on_click(cx.listener(|shell, _, window, cx| {
                        shell.request_slash(window, cx);
                        cx.notify();
                    })),
                )
        });
        let keys = match picker.target {
            composer::Target::Composer(_) => {
                "↑ ↓ to pick · Enter or Tab types it in the composer · Esc types /"
            }
            composer::Target::TaskDialog => {
                "↑ ↓ to pick · Enter or Tab types it in the task prompt · Esc types /"
            }
        };
        let unreadable = picker.unreadable();
        let panel = div()
            .id("slash-picker")
            .w(px(theme::DIALOG_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::XL))
            .rounded(px(radius::LARGE))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .shadow_md()
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::slash_picker_key_down))
            .relative()
            .child(self.typing(&self.dialog_focus, Self::slash_picker_key_down, cx))
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(format!("Insert command · {}", picker.scope().0)),
            )
            .child(caret_field(
                "slash-query",
                picker.query(),
                "Type part of a command name",
                true,
                theme,
                cx,
                |shell, caret| {
                    if let Some(picker) = shell.slash_picker.as_mut() {
                        picker.click_query(caret.at, caret.extend, caret.clicks);
                    }
                },
            ))
            .children(failure)
            .when(!rows.is_empty(), |el| {
                el.child(
                    div()
                        .id("slash-rows")
                        .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.slash_picker_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .children(note)
            .child(muted(keys.to_owned()))
            .when(picker.cli() == Some(mkrew_protocol::Cli::Claude), |el| {
                el.child(muted(
                    "MCP prompts are not listed; type /mcp__server__prompt".to_owned(),
                ))
            })
            .when(!unreadable.is_empty(), |el| {
                el.child(
                    div()
                        .id("slash-unreadable")
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .tooltip(tooltip(theme, &unreadable.join("\n"), ""))
                        .child(div().text_color(rgb(theme.status_needs_input)).child("⚠"))
                        .child(muted(slash_picker::unreadable_note(unreadable.len()))),
                )
            });
        Self::modal_shade("slash-picker-shade", panel, cx)
    }
}
```

- [ ] **Step 4: Build and test**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean. If a gpui or token name does not exist as written (`theme.status_exited`, `theme.status_needs_input`, `Icon::RotateCw`, `Icon::FileText`, `theme.terminal_font_family` were all read in the current code), look the name up in `theme.rs` / `icons.rs` and use the existing one; do not add a token.

Run: `cargo test -p mkrew-ui`
Expected: all pass.

- [ ] **Step 5: Look at it (dark and light)**

Run the app (`cargo run -p mkrew-ui`) against a daemon built from this branch with a Claude profile: open a composer (Ctrl+Shift+Enter) and type `/`; check the rows, the pick ring, hover, the chips, the footer (the MCP line shows for Claude), then the same in Settings theme light. Open Board, New task, click Prompt, type `/`. Check the keys: ↑ ↓, Enter and Tab type `/name ` with the caret after the space, Esc types `/` and the filter, a typed `zz` shows the no-match sentence, a profile with `command = "aider"` shows the unknown-CLI note and Open profile opens its file in Zed. Stop the daemon: the picker shows the error line and Try again. Fix what looks wrong using tokens only.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/shell/slash.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/prompt_keys.rs crates/mkrew-ui/src/shell/task_dialog.rs crates/mkrew-ui/src/shell/composer.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): / opens the agent's command picker in the composer and the New task prompt"
```

---

### Task 17 (A17): HUMAN_CHECKS and README

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (new block above line 24, the first "### To verify" block)
- Modify: `README.md` (the "Daily use" list, after the **Command palette** bullet at line 186)

**Interfaces:** none (docs).

- [ ] **Step 1: HUMAN_CHECKS.** Insert before `### To verify - 2026-11-03 A restart drops the panes it cannot bring back (ADR 0093)` (line 24):

```markdown
### To verify - 2026-11-04 `/` command picker in the prompt boxes (ADR 0094)

- [ ] **Install the new build and restart the daemon** (protocol 52; the daemon reads the lists).
- [ ] **Look (dark + light):** on a Claude pane open the composer (Ctrl+Shift+Enter) and type `/`.
  The picker has the same frame as the prompt picker: a filter field, rows with `/name` in the mono font, the argument hint and the description in muted text (one line, cut with an ellipsis), a chip on the right (`built-in`, `project`, `user`, `managed`, `plugin: x`, `profile`), the pick ringed, hover on the others.
  The footer shows the keys and, for Claude, "MCP prompts are not listed; type /mcp__server__prompt".
  Matched letters turn accent colored while you type.
- [ ] **Claude order vs the TUI:** open `claude` in a plain terminal pane and type `/`, then compare with the composer's picker for the same workspace.
  Names, descriptions and order should match for the first screen of rows; Claude's order here is by name because the docs state none, so tell which row is the first that differs.
  Do the same for Codex, omp (the `skill:` row, then `skill:` expands the skills) and Copilot.
- [ ] **Keys:** ↑ ↓ move; Enter and Tab type `/name ` with the caret after the space; Esc types `/` and what you typed after it (`/re` stays `/re`).
  `/` after a word or inside a path (`src/`) types a plain `/`; `@` inside an address (`a@b.com`) types a plain `@`. This is a change: `@` used to open the file picker anywhere, now only at a line start or after a space.
- [ ] **New task:** Board, New task, click Prompt, type `/`: the commands of the workspace's default agent; pick another agent with the Agent chips first and the next `/` lists that agent's. Edit a task that has a worktree: a command file that exists only in its worktree is listed.
- [ ] **Empty states:** a profile with `command = "aider"` and no `cli` shows "Mkrew doesn't know which CLI this agent runs. Set `cli` in its profile." and Open profile opens its file in Zed; `no-such` typed in the filter shows the no-match sentence; with the daemon stopped the picker shows the error and Try again; an unreadable command file adds "1 file could not be read" with its name in the tooltip.
- [ ] **Remote machine window (ADR 0089):** the list is the remote computer's (its `~/.claude/commands`, its project folder); Open profile says it works on this computer only.
- [ ] **Decision (default taken):** when no agent can be told (New task with no agent picked and no workspace default), `/` types a plain `/` and opens no picker. Want a picker of every agent's commands there instead?
```

- [ ] **Step 2: README.** After the `**Command palette**` bullet (line 186) insert:

```markdown
- **Prompt boxes** (the composer, `Ctrl+Shift+Enter`, and the New task Prompt):
  type `/` at the start of a line to pick one of the agent's own commands, skills and plugin commands (Claude Code, Codex, Oh My Pi, Copilot), as its terminal would list them.
  Type `@` to pick a file.
  If an agent's CLI is not recognised, set `cli = "claude"` (or `codex`, `omp`, `copilot`) in its profile; `[slash] hide` and `[[slash.add]]` fix single rows.
```

If part C's Prompts-page bullet landed first, merge the two into one "Prompt boxes" bullet (one sentence per line).

- [ ] **Step 3: Check**

Run: `git diff -U0 README.md docs/HUMAN_CHECKS.md | grep -c "^+.*—"`
Expected: `0` (no added line has the em dash).

- [ ] **Step 4: Commit**

```bash
git add docs/HUMAN_CHECKS.md README.md
git -c user.email=tungnv975@gmail.com commit -m "docs: HUMAN_CHECKS and README for the / command picker"
```

---

# Part B: New task image paste and drops, protocol 53, ADR 0095 (B1-B4)

Spec: `docs/superpowers/specs/2026-11-04-prompt-skills-design.md` section 6.
Worktree: `.worktrees/prompt-skills`, branch `feat/prompt-skills`. All paths below are relative to it.
Part B needs part A merged first: protocol must already be 52 (`crates/mkrew-protocol/src/lib.rs:16`) and `Shell::prompt_key_down` must exist in `crates/mkrew-ui/src/shell/prompt_keys.rs`.
Check both before Task B1: `grep -n "PROTOCOL_VERSION: u32" crates/mkrew-protocol/src/lib.rs` shows `52`, and `grep -n "fn prompt_key_down" crates/mkrew-ui/src/shell/prompt_keys.rs` finds one line.
If either fails, finish part A first.

### Review Focus candidates

1. **The task list cannot be read during the hourly check** (database busy or broken).
   Expected: no project image is deleted, because "no task names it" was never proven.
   Pinned by `an_unreadable_task_list_keeps_every_file` (Task B2).
2. **A task that is Running, NeedsInput or Review (not only Todo) still names its screenshot.**
   Expected: the file stays until the task is Done and the file is over 7 days old.
   Pinned by `open_task_prompts_lists_every_prompt_but_done_tasks` and `retention_keeps_the_screenshot_of_a_task_until_it_is_done` (Task B2).
3. **Ctrl+V with an image while the Title field has the keyboard, or while the dialog is saving.**
   Expected: nothing is attached and no file is written (a title is one line of text; a saving dialog is about to close).
   Pinned by `an_image_pasted_in_the_prompt_belongs_to_the_project` (Task B3).

Also pinned on the way: stray files and folders under `attachments/projects/` that are not ULID-named are never deleted (`stray_entries_under_projects_are_left_alone`), and the old per-session cleanup never touches the `projects` folder (`session_cleanup_ignores_the_projects_dir`).

### Decisions made while reading the code (the spec is silent or differs)

- **Age comes from the ULID in the file name**, not from the file's modified time.
  `attachment::save` names every file `<Ulid::new()>.<ext>`, so the creation time is in the name.
  A copy or a backup restore does not reset the clock, and tests need no `set_modified`.
  The comparison is `now_ms - created > KEEP_MS`, the same strict rule as the session cleanup.
- **A failed task read keeps the files** (`open_prompts` returns `None`).
- **File-tree row drags onto the open task dialog cannot start with the mouse today.**
  `Shell::modal_shade` (`crates/mkrew-ui/src/shell/focus.rs:344-368`) is `.occlude()`, so no tree row below it can begin a drag while the dialog is open, and a drag cannot begin before the dialog exists.
  The spec asks for the handler, so Task B4 adds it (`on_drop::<DraggedPath>` plus the accent ring through `drag_over`), with the same project check the pane uses.
  It is reachable only if the shade ever stops blocking drags.
  ADR 0095 and HUMAN_CHECKS say so, so nobody waits for a check that cannot be done.
  Main agent: tell the user.
- **The pane's `drop_hover` slot becomes `Option<(DropTarget, ExternalPaths)>`** with `DropTarget::{Pane(Ulid), TaskPrompt}`.
  One slot and one `drop_catcher` serve both targets.
  The pane's `on_drag_move` must stop setting the slot while a modal dialog is open: the pane is still painted under the dialog, and its capture-phase mouse-up listener would run first, take the slot and `stop_propagation`.
- **The ring for OS drops is state-driven** (`drop_hover` is the dialog's slot and `cx.has_active_drag()` is true), because the pane's `drag_over::<DraggedPath>` approach never shows for `ExternalPaths`.
  The ring is the accent border plus `theme.tint(theme.accent)`, as `pane_drop_overlay` draws.
  The tint is needed because the focused Prompt field already has an accent border.

---

### Task 18 (B1): Protocol 53: `AttachmentOwner`, daemon save, every caller, ADR 0095

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs:1310-1319` (`AttachmentSaveParams`)
- Modify: `crates/mkrew-protocol/src/lib.rs:16` (`PROTOCOL_VERSION`)
- Modify: `crates/mkrew-protocol/src/message/tests.rs:310-314` (round trip list)
- Modify: `crates/mkrew-daemon/src/attachment.rs:1-3, 35-43` and its tests at `:97-111, :113-141`
- Modify: `crates/mkrew-daemon/src/server/attachment.rs` (whole file, 30 lines)
- Modify: `crates/mkrew-ui/src/shell/composer.rs:107-113` (`composer_paste_image`, caller 1)
- Modify: `crates/mkrew-ui/src/terminal/view.rs:23-26, 888-894` (`paste_image`, caller 2)
- Modify: `crates/mkrew-daemon/tests/daemon/main.rs:14-16, 433-439` (helper `save_attachment`, caller 3)
- Modify: `crates/mkrew-daemon/tests/daemon/states.rs:55-91`
- Create: `docs/decisions/0095-task-dialog-attachments.md`
- Modify: `docs/SPEC.md` (protocol list near line 665; table row at line 2187)
- Test: `crates/mkrew-protocol/src/message/tests.rs`, `crates/mkrew-daemon/src/attachment.rs` (unit), `crates/mkrew-daemon/tests/daemon/states.rs`

No other crate calls `attachment.save`: `crates/mkrew-cli` has no mention (grep `attachment` there finds only a comment in `attach.rs`).

**Interfaces:**
- Consumes: `Ulid`, `base64_bytes` serde helper (both already used by `AttachmentSaveParams`), `Store::get_session` and `Store::get_project` (both return `StoreError::SessionNotFound` / `ProjectNotFound`, mapped to `ErrorCode::NotFound` by `store_outcome`, `crates/mkrew-daemon/src/server.rs:1775-1797`).
- Produces:
```rust
// mkrew-protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentOwner { Session(Ulid), Project(Ulid) }
pub struct AttachmentSaveParams { pub owner: AttachmentOwner, pub ext: String, pub data: Vec<u8> /* base64 on the wire */ }
// PROTOCOL_VERSION = 53
// mkrew-daemon
pub const PROJECTS: &str = "projects";
pub fn save(dir: &Path, owner: AttachmentOwner, ext: &str, data: &[u8]) -> io::Result<PathBuf>;
```
Wire form: `{"owner":{"session":"<ulid>"},"ext":"png","data":"<base64>"}` or `{"owner":{"project":"<ulid>"},...}`.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-protocol/src/message/tests.rs`, replace the `Request::AttachmentSave(...)` entry at lines 310-314 with two entries:

```rust
        Request::AttachmentSave(AttachmentSaveParams {
            owner: AttachmentOwner::Session(sid()),
            ext: "png".into(),
            data: vec![0x89, b'P', b'N', b'G'],
        }),
        Request::AttachmentSave(AttachmentSaveParams {
            owner: AttachmentOwner::Project(sid()),
            ext: "jpg".into(),
            data: vec![0xff, 0xd8],
        }),
```

Add this test at the end of the same file's test list (any place at module level of the tests file, next to other `#[test]` fns):

```rust
#[test]
fn an_attachment_owner_is_named_by_its_kind_on_the_wire() {
    let id = sid();
    assert_eq!(
        serde_json::to_value(AttachmentOwner::Session(id)).unwrap(),
        serde_json::json!({ "session": id.to_string() })
    );
    assert_eq!(
        serde_json::to_value(AttachmentOwner::Project(id)).unwrap(),
        serde_json::json!({ "project": id.to_string() })
    );
    // The old shape (`session_id`) is not accepted: protocol 53 is a clean cutover.
    let old = serde_json::json!({ "session_id": id.to_string(), "ext": "png", "data": "AA==" });
    assert!(serde_json::from_value::<AttachmentSaveParams>(old).is_err());
}
```

In `crates/mkrew-daemon/src/attachment.rs` tests, change the three `save(...)` call sites to the new signature and add a project test.
In `saved_files_land_in_the_session_dir_under_new_names` (lines 101-102):

```rust
        let first = save(dir.path(), AttachmentOwner::Session(session), "png", b"one").unwrap();
        let second = save(dir.path(), AttachmentOwner::Session(session), "png", b"two").unwrap();
```

In `cleanup_deletes_only_sessions_ended_more_than_7_days_ago` (line 119):

```rust
            save(dir.path(), AttachmentOwner::Session(id), "png", b"x").unwrap();
```

Add after `saved_files_land_in_the_session_dir_under_new_names`:

```rust
    #[test]
    fn project_files_land_in_the_projects_dir() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        let path = save(dir.path(), AttachmentOwner::Project(project), "jpg", b"shot").unwrap();
        assert_eq!(
            path.parent().unwrap(),
            dir.path().join(PROJECTS).join(project.to_string())
        );
        assert_eq!(path.extension().unwrap(), "jpg");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "shot");
    }

    #[test]
    fn session_cleanup_ignores_the_projects_dir() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        save(dir.path(), AttachmentOwner::Project(project), "png", b"x").unwrap();
        // Even when every session looks long dead, `projects` is not a session dir.
        let removed = cleanup(dir.path(), 100 * KEEP_MS, |_| Some(0)).unwrap();
        assert!(removed.is_empty());
        assert!(dir.path().join(PROJECTS).join(project.to_string()).is_dir());
    }
```

Daemon integration: in `crates/mkrew-daemon/tests/daemon/main.rs` add `AttachmentOwner,` before `AttachmentSaveParams,` in the `use mkrew_protocol::{ ... }` list (line 15; `cargo fmt` rewraps it) and replace the helper (lines 433-439):

```rust
fn save_attachment(owner: AttachmentOwner, ext: &str, data: Vec<u8>) -> Request {
    Request::AttachmentSave(AttachmentSaveParams {
        owner,
        ext: ext.into(),
        data,
    })
}
```

In `crates/mkrew-daemon/tests/daemon/states.rs` update the existing test (lines 68, 79-81, 86) to `AttachmentOwner::Session(...)`:

```rust
        .call(save_attachment(AttachmentOwner::Session(id), "png", image.clone()))
```
```rust
        save_attachment(AttachmentOwner::Session(id), "exe", vec![1]),
        save_attachment(AttachmentOwner::Session(id), "png", Vec::new()),
        save_attachment(AttachmentOwner::Session(id), "png", too_big),
```
```rust
        error_code(client.call(save_attachment(
            AttachmentOwner::Session(Ulid::new()),
            "png",
            vec![1]
        ))),
```

and add this test right after it (before `hook_reports_drive_the_state_of_live_sessions_only`):

```rust
/// A project image (New task dialog) lands under `attachments/projects/<project_id>/`; an
/// unknown project is `not_found`, a session id is not a project id (SPEC §20B.5, ADR 0095).
#[test]
fn attachments_are_saved_per_project_and_unknown_projects_rejected() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());

    let image = vec![0x89, b'P', b'N', b'G', 3];
    let saved: AttachmentSaveResult = client
        .call(save_attachment(
            AttachmentOwner::Project(project.id),
            "webp",
            image.clone(),
        ))
        .into_result()
        .unwrap();
    let path = PathBuf::from(&saved.path);
    let dir = env
        .data_dir()
        .join("attachments")
        .join("projects")
        .join(project.id.to_string());
    assert_eq!(path.parent().unwrap(), dir);
    assert_eq!(path.extension().unwrap(), "webp");
    assert_eq!(std::fs::read(&path).unwrap(), image);

    assert_eq!(
        error_code(client.call(save_attachment(
            AttachmentOwner::Project(Ulid::new()),
            "png",
            vec![1]
        ))),
        ErrorCode::NotFound
    );
    assert_eq!(
        error_code(client.call(save_attachment(
            AttachmentOwner::Project(project.id),
            "exe",
            vec![1]
        ))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(dir.read_dir().unwrap().count(), 1);
    client.stop();
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-protocol attachment_owner`
Expected: FAIL to compile, `cannot find type AttachmentOwner in this scope`.

Run: `cargo test -p mkrew-daemon attachment`
Expected: FAIL to compile, same reason (`AttachmentOwner` not found in `mkrew_protocol`).

- [ ] **Step 3: Implement the protocol**

`crates/mkrew-protocol/src/message.rs`, replace lines 1310-1319:

```rust
/// `attachment.save`: an image pasted into an agent pane, a composer or the task dialog
/// (SPEC §20B.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentSaveParams {
    pub owner: AttachmentOwner,
    /// `png`, `jpg`, `gif` or `webp`.
    pub ext: String,
    /// The file content (≤ 10 MiB), base64 on the wire.
    #[serde(with = "base64_bytes")]
    pub data: Vec<u8>,
}

/// Whose folder an attachment is written to (SPEC §20B.5, ADR 0095).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentOwner {
    /// `<data>/attachments/<session_id>/`: deleted 7 days after the session ended.
    Session(Ulid),
    /// `<data>/attachments/projects/<project_id>/`: kept while a task of the project that is
    /// not done names the file, else deleted after 7 days.
    Project(Ulid),
}
```

`crates/mkrew-protocol/src/lib.rs:16`:

```rust
pub const PROTOCOL_VERSION: u32 = 53;
```

- [ ] **Step 4: Implement the daemon**

`crates/mkrew-daemon/src/attachment.rs`, replace the module doc (lines 1-3) and imports (5-8), add `PROJECTS`, and replace `save` (35-43):

```rust
//! Pasted images (SPEC §20B.5): `attachment.save` writes `<dir>/<session_id>/<ulid>.<ext>` or
//! `<dir>/projects/<project_id>/<ulid>.<ext>`; the hourly backup check ([`crate::backup`])
//! deletes the dirs of sessions that ended more than [`KEEP_MS`] ago and, per file, the
//! project images no open task names ([`cleanup_projects`]).

use std::io;
use std::path::{Path, PathBuf};

use mkrew_protocol::AttachmentOwner;
use ulid::Ulid;

/// The folder under the attachments dir that holds project images; it is not a ULID, so the
/// session cleanup skips it.
pub const PROJECTS: &str = "projects";
```

```rust
/// Writes `data` to a new file in the owner's dir and returns its path. `ext` must have
/// passed [`check`].
pub fn save(dir: &Path, owner: AttachmentOwner, ext: &str, data: &[u8]) -> io::Result<PathBuf> {
    let dir = match owner {
        AttachmentOwner::Session(id) => dir.join(id.to_string()),
        AttachmentOwner::Project(id) => dir.join(PROJECTS).join(id.to_string()),
    };
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.{ext}", Ulid::new()));
    std::fs::write(&path, data)?;
    Ok(path)
}
```

(`KEEP_MS`, `EXTS`, `MAX_BYTES`, `check` and `cleanup` stay as they are. The doc link to `cleanup_projects` resolves in Task B2; `cargo doc` is not part of this task's checks.)

`crates/mkrew-daemon/src/server/attachment.rs`, whole file:

```rust
//! `attachment.save` (SPEC §20B.5).

use mkrew_protocol::{
    AttachmentOwner, AttachmentSaveParams, AttachmentSaveResult, ErrorCode, Outcome,
};

use super::{Daemon, ok, store_outcome};
use crate::attachment;

impl Daemon {
    /// Stores a pasted image for a known session or project and returns its absolute path.
    pub(super) async fn attachment_save(&self, params: AttachmentSaveParams) -> Outcome {
        if let Err(message) = attachment::check(&params.ext, params.data.len()) {
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let known = match params.owner {
            AttachmentOwner::Session(id) => self.hub.store().get_session(id).map(|_| ()),
            AttachmentOwner::Project(id) => self.hub.store().get_project(id).map(|_| ()),
        };
        if let Err(e) = known {
            return store_outcome::<()>(Err(e));
        }
        let dir = self.paths.attachments_dir();
        let saved = tokio::task::spawn_blocking(move || {
            attachment::save(&dir, params.owner, &params.ext, &params.data)
        })
        .await;
        match saved {
            Ok(Ok(path)) => ok(&AttachmentSaveResult {
                path: path.to_string_lossy().into_owned(),
            }),
            Ok(Err(e)) => Outcome::error(ErrorCode::Internal, format!("save attachment: {e}")),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("save attachment failed: {e}")),
        }
    }
}
```

- [ ] **Step 5: Migrate the UI callers (clean cutover, no shim)**

`crates/mkrew-ui/src/shell/composer.rs`, in `composer_paste_image` replace lines 107-113:

```rust
        let saved = connection.call::<mkrew_protocol::AttachmentSaveResult>(
            Request::AttachmentSave(mkrew_protocol::AttachmentSaveParams {
                owner: mkrew_protocol::AttachmentOwner::Session(session),
                ext: ext.to_owned(),
                data,
            }),
        );
```

`crates/mkrew-ui/src/terminal/view.rs`: add `AttachmentOwner` to the `use mkrew_protocol::{ ... }` list at lines 23-26 (`AttachmentOwner, AttachmentSaveParams, AttachmentSaveResult, ...`) and replace lines 890-894:

```rust
            .call::<AttachmentSaveResult>(Request::AttachmentSave(AttachmentSaveParams {
                owner: AttachmentOwner::Session(session_id),
                ext: ext.to_owned(),
                data,
            }));
```

- [ ] **Step 6: Write ADR 0095**

Create `docs/decisions/0095-task-dialog-attachments.md` (one sentence per line, no em dash):

```markdown
# 0095 - The New task dialog takes images and dropped files

- Date: 2026-11-04
- Status: accepted (amends SPEC §20B.1 and §20B.5, ADR 0060 for attachments)
- Milestone: none (human request 2026-11-04, spec `docs/superpowers/specs/2026-11-04-prompt-skills-design.md` section 6)

## Context

The task dialog's Prompt box shares `composer::Prompt` with the composer, but it ignored pasted images and dropped files.
SPEC §20B.1 and a HUMAN_CHECKS decision said why: attachments were stored per session (`attachment.save` took a `session_id`), and a new task has no session until it runs.
A new task has no task id either: the daemon makes it at `task.create`.

## Decision

- `attachment.save` takes `owner`: `{"session": id}` or `{"project": id}` instead of `session_id` (protocol 53, no compatibility shim: the version check rejects an old client).
- `Session` keeps `<data>/attachments/<session_id>/<ulid>.<ext>`.
- `Project` writes `<data>/attachments/projects/<project_id>/<ulid>.<ext>`.
- An unknown session or project is `not_found`.
- The composer and the panes send `Session`; the task dialog sends `Project(dialog.project_id())`, for a new task and for an edit.
- Retention (ADR 0060): the hourly check keeps the session rule and deletes a file under `projects/<id>/` when it is more than 7 days old and no task of that project that is not done has a prompt containing the file name.
  The age is the time in the ULID of the file name, which `attachment.save` wrote, so a copy of the file does not reset it.
  When the tasks cannot be read, nothing is deleted.
- Ctrl+V with an image in the dialog's Prompt box saves it and types its path, through the code the composer uses (`prompt_key_down`, ADR 0094).
  The Title field, and a dialog that is saving, take no image.
- The Prompt box takes OS file drops and file tree row drops.
  Paths are typed with the pane's rules (`composer::dropped_files`, `composer::dropped_ref`).
  While a drag is over the box it shows the accent ring.
  The box uses the pane's `drop_hover` slot and `drop_catcher` (the gpui-pre 0.3.7 OLE workaround) with a new `DropTarget::TaskPrompt`.
- `type_into_pane` keeps ignoring drops while a dialog is open; the dialog's box handles its own.
- A machine window refuses OS files for the dialog as for a pane (ADR 0089).

## Alternatives

- A folder per task id: a new task has no id before save, so the file would have to move after `task.create`, and the prompt text would change after the user saw it.
- A client generated task id: changes `task.create` and the store for one feature.
- Keeping the image bytes in the task row: prompts are text sent to the agent as a path.
- Deleting project images with the project or the task only: a crashed daemon or an abandoned dialog would leave files forever; the age rule bounds them.

## Consequences

- A task in the backlog keeps its screenshot as long as it is not done; a done or deleted task's screenshot goes within about 8 days.
- An image pasted and then abandoned (dialog cancelled) goes after 7 days, like a session's.
- Editing a task so its prompt no longer names an image releases the file for the next check after 7 days.
- A file tree row cannot be dragged onto the open dialog today: the modal shade blocks the tree, so the drop handler for rows is reachable only if that changes.
  OS file drops work.
- Every client of `attachment.save` migrates in this change: the composer, the pane's paste and the daemon tests.
- SPEC §20B.1, §20B.5 and the protocol section say so.
- Tests: `attachments_are_saved_per_project_and_unknown_projects_rejected` (daemon), `retention_keeps_the_screenshot_of_a_task_until_it_is_done` (daemon unit), `an_image_pasted_in_the_prompt_belongs_to_the_project` (UI).
```

- [ ] **Step 7: SPEC protocol list and table**

`docs/SPEC.md`, after the line `Protocol 51 (port manager, spec 2026-11-01): requests `port.list` and `port.stop`.` (line 665) and after the `Protocol 52` line part A added there (if present), insert, with a blank line before it:

```markdown
Protocol 53 (ADR 0095): `attachment.save` takes `owner` (`{"session": id}` or `{"project": id}`) instead of `session_id`.
```

Replace the table row at line 2187 (the `attachment.save` row) with:

```markdown
| `attachment.save` | `owner` (`{"session": id}` or `{"project": id}`), `ext` (`png`/`jpg`/`gif`/`webp`), `data` (base64, ≤ 10 MiB) | `path`: `<data>/mkrew/attachments/<session_id>/<ulid>.<ext>`, or `<data>/mkrew/attachments/projects/<project_id>/<ulid>.<ext>` for a project; an unknown session or project is `not_found` |
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo fmt --all`
Run: `cargo test -p mkrew-protocol`
Expected: PASS (including `an_attachment_owner_is_named_by_its_kind_on_the_wire` and the round trip list).

Run: `cargo test -p mkrew-daemon attachment`
Expected: PASS for the unit tests (`project_files_land_in_the_projects_dir`, `session_cleanup_ignores_the_projects_dir`, the updated two).

Run: `cargo test -p mkrew-daemon --test daemon attachments_`
Expected: PASS for `attachments_are_saved_per_session_and_bad_ones_rejected` and `attachments_are_saved_per_project_and_unknown_projects_rejected`.

Run: `cargo check -p mkrew-ui -p mkrew-cli`
Expected: no errors (the two UI callers compile with `owner`).

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-protocol/src/message/tests.rs crates/mkrew-daemon/src/attachment.rs crates/mkrew-daemon/src/server/attachment.rs crates/mkrew-daemon/tests/daemon/main.rs crates/mkrew-daemon/tests/daemon/states.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/terminal/view.rs docs/decisions/0095-task-dialog-attachments.md docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(protocol): attachment.save takes an owner, session or project (protocol 53, ADR 0095)"
```

---

### Task 19 (B2): Retention of project images in the hourly check

**Files:**
- Modify: `crates/mkrew-daemon/src/attachment.rs` (add two functions after `cleanup`, line 78; imports at top; tests in `mod tests`)
- Modify: `crates/mkrew-daemon/src/backup.rs:27-29, 47-56, 64-76` (`start`)
- Modify: `docs/SPEC.md:2189`
- Test: `crates/mkrew-daemon/src/attachment.rs` (`mod tests`)

**Interfaces:**
- Consumes: `attachment::PROJECTS`, `attachment::save`, `KEEP_MS` (Task B1); `Store::list_tasks(project_id: Option<Ulid>, status: Option<TaskStatus>) -> Result<Vec<Task>>` (`crates/mkrew-store/src/lib.rs:520`), `Task.status`, `Task.prompt`, `TaskStatus::Done` (`crates/mkrew-protocol/src/message.rs:624-630`); `hub.store()` derefs to `&Store` as in `backup::start`.
- Produces:
```rust
/// Prompts of the project's tasks that are not done; `None` when they cannot be read.
pub fn open_task_prompts(store: &Store, project: Ulid) -> Option<Vec<String>>;
/// Deletes project images older than KEEP_MS that no open task prompt names.
pub fn cleanup_projects(
    dir: &Path,
    now_ms: i64,
    open_prompts: impl FnMut(Ulid) -> Option<Vec<String>>,
) -> io::Result<Vec<PathBuf>>;
```

The status model: `TaskStatus` is `Todo | Running | NeedsInput | Review | Done`; every status except `Done` counts as "not done" (the board shows a Review card until the user approves it, and a Running card is live).
`list_tasks(Some(project), None)` returns every status in board order; the filter is in Rust.
An unknown project returns an empty list (no `ProjectNotFound`), so a deleted project's old files are deleted by the age rule.

- [ ] **Step 1: Write the failing tests**

Add these imports to `mod tests` in `crates/mkrew-daemon/src/attachment.rs` (below `use super::*;`):

```rust
    use std::cell::Cell;

    use mkrew_protocol::TaskStatus;
    use mkrew_store::Store;
```

Add these tests at the end of `mod tests`:

```rust
    const NOW: i64 = 100 * KEEP_MS;

    /// A project image created `age_ms` before [`NOW`]; the ULID in the name carries the age.
    fn project_file(dir: &Path, project: Ulid, age_ms: i64) -> (PathBuf, String) {
        let created = u64::try_from(NOW - age_ms).unwrap();
        let name = format!("{}.png", Ulid::from_parts(created, 1));
        let folder = dir.join(PROJECTS).join(project.to_string());
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join(&name);
        std::fs::write(&path, b"x").unwrap();
        (path, name)
    }

    #[test]
    fn project_files_older_than_7_days_go_unless_a_prompt_names_them() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        let (unused, _) = project_file(dir.path(), project, KEEP_MS + 1);
        let (named, named_name) = project_file(dir.path(), project, KEEP_MS + 1);
        let (boundary, _) = project_file(dir.path(), project, KEEP_MS);
        let (fresh, _) = project_file(dir.path(), project, 1);

        let removed = cleanup_projects(dir.path(), NOW, |id| {
            assert_eq!(id, project);
            Some(vec!["Fix login".into(), format!("see \"C:\\a b\\{named_name}\" too")])
        })
        .unwrap();

        assert_eq!(removed, [unused.clone()]);
        assert!(!unused.exists());
        for kept in [named, boundary, fresh] {
            assert!(kept.exists(), "{}", kept.display());
        }
    }

    #[test]
    fn an_unreadable_task_list_keeps_every_file() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        let (old, _) = project_file(dir.path(), project, 3 * KEEP_MS);
        let removed = cleanup_projects(dir.path(), NOW, |_| None).unwrap();
        assert!(removed.is_empty());
        assert!(old.exists());
    }

    #[test]
    fn the_tasks_are_read_only_for_projects_with_old_files() {
        let dir = tempfile::tempdir().unwrap();
        let (fresh, _) = project_file(dir.path(), Ulid::new(), 1);
        let calls = Cell::new(0);
        cleanup_projects(dir.path(), NOW, |_| {
            calls.set(calls.get() + 1);
            Some(Vec::new())
        })
        .unwrap();
        assert_eq!(calls.get(), 0);
        assert!(fresh.exists());
    }

    #[test]
    fn stray_entries_under_projects_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let project = Ulid::new();
        let folder = dir.path().join(PROJECTS).join(project.to_string());
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("notes.txt"), "mine").unwrap();
        std::fs::write(folder.join("not-a-ulid.png"), "mine").unwrap();
        std::fs::create_dir_all(dir.path().join(PROJECTS).join("scratch")).unwrap();
        std::fs::write(dir.path().join(PROJECTS).join("readme.txt"), "mine").unwrap();

        let removed = cleanup_projects(dir.path(), NOW, |_| Some(Vec::new())).unwrap();

        assert!(removed.is_empty());
        assert!(folder.join("notes.txt").is_file());
        assert!(folder.join("not-a-ulid.png").is_file());
        assert!(dir.path().join(PROJECTS).join("scratch").is_dir());
        assert!(dir.path().join(PROJECTS).join("readme.txt").is_file());
    }

    #[test]
    fn cleaning_projects_without_the_dir_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let removed = cleanup_projects(&dir.path().join("attachments"), NOW, |_| None).unwrap();
        assert!(removed.is_empty());
    }

    fn store_with_project(dir: &Path, name: &str) -> (Store, mkrew_protocol::Project) {
        let store = Store::open(&dir.join("mkrew.db")).unwrap();
        let repo = dir.join(name);
        std::fs::create_dir_all(&repo).unwrap();
        let project = store.add_project(&repo, None).unwrap();
        (store, project)
    }

    #[test]
    fn open_task_prompts_lists_every_prompt_but_done_tasks() {
        let data = tempfile::tempdir().unwrap();
        let (store, project) = store_with_project(data.path(), "repo");
        for (title, status) in [
            ("todo", TaskStatus::Todo),
            ("run", TaskStatus::Running),
            ("input", TaskStatus::NeedsInput),
            ("review", TaskStatus::Review),
            ("done", TaskStatus::Done),
        ] {
            let task = store
                .create_task(project.id, title, &format!("prompt {title}"), None, None, false)
                .unwrap();
            store.set_task_status(task.id, status, false).unwrap();
        }
        let other = data.path().join("other");
        std::fs::create_dir_all(&other).unwrap();
        let other = store.add_project(&other, None).unwrap();
        store
            .create_task(other.id, "x", "prompt other", None, None, false)
            .unwrap();

        let mut prompts = open_task_prompts(&store, project.id).unwrap();
        prompts.sort();
        assert_eq!(
            prompts,
            ["prompt input", "prompt review", "prompt run", "prompt todo"]
        );
        assert_eq!(open_task_prompts(&store, Ulid::new()), Some(Vec::new()));
    }

    /// SPEC §20B.5: a task in the backlog keeps its screenshot; a done task's goes after 7 days.
    #[test]
    fn retention_keeps_the_screenshot_of_a_task_until_it_is_done() {
        let data = tempfile::tempdir().unwrap();
        let attachments = data.path().join("attachments");
        let (store, project) = store_with_project(data.path(), "repo");
        let (path, name) = project_file(&attachments, project.id, 3 * KEEP_MS);
        let task = store
            .create_task(project.id, "Fix", &format!("Look at {}", path.display()), None, None, false)
            .unwrap();
        assert!(path.to_string_lossy().ends_with(&name));
        let check = |store: &Store| {
            cleanup_projects(&attachments, NOW, |id| open_task_prompts(store, id)).unwrap()
        };

        assert!(check(&store).is_empty(), "a Todo task keeps it");
        assert!(path.exists());
        store.set_task_status(task.id, TaskStatus::Review, false).unwrap();
        assert!(check(&store).is_empty(), "a Review task keeps it");
        assert!(path.exists());

        store.set_task_status(task.id, TaskStatus::Done, false).unwrap();
        assert_eq!(check(&store), [path.clone()]);
        assert!(!path.exists());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-daemon --lib attachment`
Expected: FAIL to compile, `cannot find function cleanup_projects` and `cannot find function open_task_prompts`.

- [ ] **Step 3: Implement**

`crates/mkrew-daemon/src/attachment.rs`: extend the imports added in Task B1:

```rust
use mkrew_protocol::{AttachmentOwner, TaskStatus};
use mkrew_store::Store;
use ulid::Ulid;
```

Insert after `cleanup` (after line 78, before `#[cfg(test)]`):

```rust
/// Prompts of `project`'s tasks that are not done (Todo, Running, NeedsInput, Review), the
/// text that keeps a project image alive. `None` when the tasks cannot be read: the caller
/// must then keep every file.
pub fn open_task_prompts(store: &Store, project: Ulid) -> Option<Vec<String>> {
    match store.list_tasks(Some(project), None) {
        Ok(tasks) => Some(
            tasks
                .into_iter()
                .filter(|task| task.status != TaskStatus::Done)
                .map(|task| task.prompt)
                .collect(),
        ),
        Err(e) => {
            tracing::warn!(%project, "read tasks for attachment cleanup: {e}");
            None
        }
    }
}

/// When a project image was made: the time in the ULID that [`save`] gave its name.
fn created_ms(path: &Path) -> Option<i64> {
    let stem = path.file_stem()?.to_str()?;
    i64::try_from(Ulid::from_string(stem).ok()?.timestamp_ms()).ok()
}

/// Deletes the files of `<dir>/projects/<project_id>/` that are more than [`KEEP_MS`] old
/// before `now_ms` and whose name no prompt of `open_prompts(project)` contains (SPEC §20B.5).
/// `open_prompts` is asked once per project and only when it has an old file; `None` (the
/// tasks could not be read) keeps the project's files. Entries whose names are not ULIDs are
/// left alone. A file that cannot be deleted is logged and retried at the next check.
/// Returns the deleted files.
pub fn cleanup_projects(
    dir: &Path,
    now_ms: i64,
    mut open_prompts: impl FnMut(Ulid) -> Option<Vec<String>>,
) -> io::Result<Vec<PathBuf>> {
    let projects = match std::fs::read_dir(dir.join(PROJECTS)) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut removed = Vec::new();
    for entry in projects.flatten() {
        let Some(project) = entry
            .file_name()
            .to_str()
            .and_then(|name| Ulid::from_string(name).ok())
        else {
            continue;
        };
        let Ok(files) = std::fs::read_dir(entry.path()) else {
            continue;
        };
        let mut prompts: Option<Option<Vec<String>>> = None;
        for file in files.flatten() {
            let path = file.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                continue;
            };
            let Some(created) = created_ms(&path) else {
                continue;
            };
            if now_ms - created <= KEEP_MS {
                continue;
            }
            let open = prompts.get_or_insert_with(|| open_prompts(project));
            let Some(open) = open.as_ref() else {
                continue;
            };
            if open.iter().any(|prompt| prompt.contains(&name)) {
                continue;
            }
            match std::fs::remove_file(&path) {
                Ok(()) => removed.push(path),
                Err(e) => tracing::warn!(path = %path.display(), "delete attachment: {e}"),
            }
        }
    }
    Ok(removed)
}
```

`crates/mkrew-daemon/src/backup.rs`, update the doc comment of `start` (lines 27-29):

```rust
/// Checks for today's backup now and then every [`EVERY`]; each check also deletes old
/// attachments in `attachments` ([`attachment::cleanup`] for sessions,
/// [`attachment::cleanup_projects`] for project images, SPEC §20B.5) and runs retention
/// over the database and `sessions` (ADR 0060). Runs until aborted.
```

Replace the blocking closure and the result handling (lines 47-76):

```rust
            let done = tokio::task::spawn_blocking(move || {
                let now = crate::server::now_ms();
                let backed_up = run(&hub.store(), &dir, now);
                let cleaned = attachment::cleanup(&attachments, now, |id| {
                    hub.store().get_session(id).ok().and_then(|s| s.ended_at)
                });
                let cleaned_projects = attachment::cleanup_projects(&attachments, now, |id| {
                    attachment::open_task_prompts(&hub.store(), id)
                });
                let retained = crate::retention::run(&hub.store(), &sessions, now);
                (backed_up, cleaned, cleaned_projects, retained)
            })
            .await;
            let (backed_up, cleaned, cleaned_projects, retained) = match done {
                Ok(done) => done,
                Err(e) => {
                    tracing::warn!("database backup task failed: {e}");
                    continue;
                }
            };
            match backed_up {
                Ok(Some(path)) => tracing::info!(path = %path.display(), "database backed up"),
                Ok(None) => {}
                Err(e) => tracing::warn!("database backup failed: {e:#}"),
            }
            for cleaned in [cleaned, cleaned_projects] {
                match cleaned {
                    Ok(removed) => {
                        for path in removed {
                            tracing::info!(path = %path.display(), "old attachments deleted");
                        }
                    }
                    Err(e) => tracing::warn!("attachment cleanup failed: {e}"),
                }
            }
```

(The `match retained { ... }` block that follows stays as it is.)

`docs/SPEC.md:2189`, replace the line with two lines:

```markdown
Attachments of sessions that ended more than 7 days ago are deleted by the hourly backup check (§8).
A project attachment is deleted by the same check when it is more than 7 days old (counted from the ULID in its file name) and no task of that project that is not done has a prompt containing its file name.
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo fmt --all`
Run: `cargo test -p mkrew-daemon --lib attachment`
Expected: PASS (the 7 new tests plus the earlier ones).

Run: `cargo test -p mkrew-daemon --lib backup`
Expected: PASS (the `start` change compiles and the backup tests still pass).

Run: `cargo clippy -p mkrew-daemon --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/src/attachment.rs crates/mkrew-daemon/src/backup.rs docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): delete project images 7 days old that no open task names"
```

---

### Task 20 (B3): Ctrl+V image paste in the task dialog Prompt box

**Files:**
- Modify: `crates/mkrew-ui/src/task_dialog.rs:6-8` (imports), `:177-182` (add `attachment_owner` after `project_id`), tests (end of `mod tests`)
- Modify: `crates/mkrew-ui/src/shell/composer.rs:85-131` (`composer_paste_image` becomes `prompt_paste_image`, add `prompt_paste`)
- Modify: `crates/mkrew-ui/src/shell/prompt_keys.rs` (the paste branch of `prompt_key_down`, written by part A)
- Modify: `docs/SPEC.md:2143-2144` (§20B.1)
- Modify: `docs/HUMAN_CHECKS.md:63` (the "Decision (default taken)" line) and add one "To verify" section above it
- Test: `crates/mkrew-ui/src/task_dialog.rs`

**Interfaces:**
- Consumes: Task B1's `mkrew_protocol::AttachmentOwner`; `composer::attachment(image) -> Option<(&'static str, Vec<u8>)>`, `composer::attachment_refusal(len) -> Option<String>`, `composer::typed_paths`; `Shell::prompt_typed(target, text, window, cx)` (`shell/composer.rs:146`); `Shell::open_dialog(serial) -> Option<&mut TaskDialog>` (`shell/task_dialog.rs:41`); `Shell::dialog_serial`; part A's `Shell::prompt_key_down(target, event, window, cx) -> bool` in `prompt_keys.rs`.
- Produces:
```rust
// crates/mkrew-ui/src/task_dialog.rs
impl TaskDialog {
    /// The owner of an image pasted now: the project, only while the Prompt box has the keys and nothing is saving.
    pub fn attachment_owner(&self) -> Option<AttachmentOwner>;
}
// crates/mkrew-ui/src/shell/composer.rs
impl Shell {
    pub(crate) fn prompt_paste(&mut self, target: composer::Target, window: &mut Window, cx: &mut Context<'_, Self>);
    pub(crate) fn prompt_paste_image(&mut self, target: composer::Target, item: gpui::ClipboardItem, window: &mut Window, cx: &mut Context<'_, Self>);
}
```

- [ ] **Step 1: Write the failing test**

In `crates/mkrew-ui/src/task_dialog.rs`, extend the import at lines 6-8 to include `AttachmentOwner`:

```rust
use mkrew_protocol::{
    AgentProfileInfo, AttachmentOwner, Request, SessionKind, Task, TaskCreateParams,
    TaskUpdateParams,
};
```

Add at the end of `mod tests` (before the closing `}` at line 780):

```rust
    /// Ctrl+V with an image: the project owns it (a new task has no id yet). The Title takes
    /// no image and a saving dialog is about to close (SPEC §20B.5).
    #[test]
    fn an_image_pasted_in_the_prompt_belongs_to_the_project() {
        let project_id = Ulid::new();
        let mut dialog = TaskDialog::new(project_id);
        assert_eq!(dialog.field, Field::Title);
        assert_eq!(dialog.attachment_owner(), None, "the title takes no image");

        dialog.key(DialogKey::SwitchField);
        assert_eq!(
            dialog.attachment_owner(),
            Some(AttachmentOwner::Project(project_id))
        );

        dialog.busy = true;
        assert_eq!(dialog.attachment_owner(), None, "saving: nothing to attach to");

        let task = task();
        let mut edit = TaskDialog::edit(task.clone());
        edit.key(DialogKey::SwitchField);
        assert_eq!(
            edit.attachment_owner(),
            Some(AttachmentOwner::Project(task.project_id)),
            "an edited task's image goes to its project too"
        );
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p mkrew-ui an_image_pasted_in_the_prompt`
Expected: FAIL to compile, `no method named attachment_owner found for struct TaskDialog`.

- [ ] **Step 3: Implement the model**

`crates/mkrew-ui/src/task_dialog.rs`, add after `project_id` (after line 182):

```rust
    /// Who owns an image pasted now (§20B.5): the dialog's project, because a new task has no
    /// id and no session before it is saved. `None` unless the Prompt box has the keys and
    /// nothing is being saved.
    pub fn attachment_owner(&self) -> Option<AttachmentOwner> {
        (self.field == Field::Prompt && !self.busy)
            .then(|| AttachmentOwner::Project(self.project_id()))
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p mkrew-ui an_image_pasted_in_the_prompt`
Expected: PASS.

- [ ] **Step 5: Wire the shell (replace the composer-only image code with one path for both boxes)**

Confirm part A's file: `grep -n "composer_paste_image\|read_from_clipboard" crates/mkrew-ui/src/shell/prompt_keys.rs`.
Part A moved the two paste blocks (composer: `shell/composer.rs:52-59`, dialog: `shell/task_dialog.rs:84-92`) into `prompt_key_down`.
Replace that whole paste branch (every line that reads the clipboard, inserts its text and, for the composer only, calls `composer_paste_image`) with one call:

```rust
        if key == DialogKey::Paste {
            self.prompt_paste(target, window, cx);
        }
```

(Keep whatever guard part A put before the branch, for example the "a control has the keyboard" gate for the dialog. The `copy_selection` / `DialogKey::Copy | Cut` handling after it stays.)

In `crates/mkrew-ui/src/shell/composer.rs`, replace `composer_paste_image` (lines 85-131) with:

```rust
    /// Ctrl+V in a prompt box (§20B.1, §20B.5): clipboard text goes into the box, an image is
    /// attached.
    pub(crate) fn prompt_paste(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        let Some(text) = item.text() else {
            return self.prompt_paste_image(target, item, window, cx);
        };
        match target {
            composer::Target::Composer(session) => self.composers.insert(session, &text),
            composer::Target::TaskDialog => {
                if let Some(dialog) = self.task_dialog.as_mut() {
                    dialog.insert(&text);
                }
            }
        }
    }

    /// An image pasted into a prompt box (§20B.5): saved through `attachment.save` (a BMP as
    /// PNG), then its path is typed into the box. A composer's image belongs to its session,
    /// the task dialog's to its project. A failure is a toast, like a pane's paste.
    pub(crate) fn prompt_paste_image(
        &mut self,
        target: composer::Target,
        item: gpui::ClipboardItem,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let owner = match target {
            composer::Target::Composer(session) => {
                Some(mkrew_protocol::AttachmentOwner::Session(session))
            }
            composer::Target::TaskDialog => self
                .task_dialog
                .as_ref()
                .and_then(TaskDialog::attachment_owner),
        };
        let Some(owner) = owner else {
            return;
        };
        let Some((ext, data)) = item.into_entries().find_map(|entry| match entry {
            gpui::ClipboardEntry::Image(image) => composer::attachment(image),
            _ => None,
        }) else {
            return;
        };
        if let Some(message) = composer::attachment_refusal(data.len()) {
            self.toast_error(message, cx);
            return;
        }
        let serial = self.dialog_serial;
        let saved = connection.call::<mkrew_protocol::AttachmentSaveResult>(
            Request::AttachmentSave(mkrew_protocol::AttachmentSaveParams {
                owner,
                ext: ext.to_owned(),
                data,
            }),
        );
        cx.spawn_in(window, async move |this, cx| match saved.await {
            Ok(saved) => {
                let _ = this.update_in(cx, |shell, window, cx| {
                    // The dialog it was pasted into may be gone, or another one open.
                    if target == composer::Target::TaskDialog && shell.open_dialog(serial).is_none()
                    {
                        return;
                    }
                    let text = composer::typed_paths([saved.path.as_str()]);
                    shell.prompt_typed(target, &text, window, cx);
                });
            }
            Err(error) => {
                tracing::warn!(?target, "attachment.save failed: {error:#}");
                let _ = this.update(cx, |shell, cx| {
                    shell.toast_error(format!("Cannot attach the image: {error:#}"), cx);
                });
            }
        })
        .detach();
    }
```

(`TaskDialog` and `Request` come from `use crate::*`, as the file's other code already uses `Request` and `composer::`. `prompt_typed` for the dialog calls `insert_prompt`, which also switches the field to Prompt, and a ✦ rewrite in flight holds the text, as it does for a pick.)

- [ ] **Step 6: SPEC §20B.1 and HUMAN_CHECKS**

`docs/SPEC.md` lines 2143-2144 (`saving waits for the answer. Pasted images are not attached there: ...` and its second line), replace both lines with:

```markdown
   saving waits for the answer.
   Ctrl+V with an image saves it as a project attachment (§20B.5) and types its path, as in the composer.
   Dropping OS files or a file tree row on the prompt box types their paths (§20B.5).
```

`docs/HUMAN_CHECKS.md`, replace the "Decision (default taken)" line (line 63) with:

```markdown
- [ ] **Image paste (dark + light):** copy a screenshot, Board → New task, click the Prompt field, Ctrl+V.
  The path `...\attachments\projects\<workspace id>\<id>.png` is typed into the prompt and the caret stays in the prompt; the file exists.
  Ctrl+V in the Title field does nothing for an image; Ctrl+V of text still pastes text in both fields.
  Edit an existing task and paste: same result.
  With the dialog's agent set to a real agent, create the task and run it: the agent can read the image.
  (Replaces the earlier decision "image paste does nothing", ADR 0095.)
```

Add this section above `### To verify - 2026-11-02 Task dialog prompt works like the composer` (and below part A's newest section if present):

```markdown
### To verify - 2026-11-04 Task dialog attachments (ADR 0095)

- [ ] **Install the new build and restart the daemon** (protocol 53; a window from before it shows a version mismatch).
- [ ] **Composer and pane paste still work:** Ctrl+V of an image in an agent pane and in its composer types a path under `attachments\<session id>\`.
- [ ] **Backlog keeps the screenshot:** the file from the New task check above is still there after a daemon restart (the hourly check runs at start); it is deleted only once its task is done and the file is over 7 days old (covered by the daemon tests; no wait needed).
```

- [ ] **Step 7: Run the checks**

Run: `cargo fmt --all`
Run: `cargo check -p mkrew-ui`
Expected: no errors, no leftover reference to `composer_paste_image` (`grep -rn composer_paste_image crates/` finds nothing).

Run: `cargo test -p mkrew-ui task_dialog`
Expected: PASS.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/task_dialog.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/prompt_keys.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): paste an image into the New task prompt as a project attachment"
```

---

### Task 21 (B4): OS file and file tree drops on the task dialog Prompt box

**Files:**
- Modify: `crates/mkrew-ui/src/composer.rs:286-300` (add `dropped_files` after `typed_paths`) and its tests (`mod tests`, line 363+)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs:499-512` (pane `on_drag_move`, `drop_catcher` call), `:982-1010` (`drop_catcher`), add `DropTarget` above it
- Modify: `crates/mkrew-ui/src/main.rs:665-667` (`drop_hover` type and doc)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs:67-107` (`drop_paths`, `drop_tree_path`; add `dropped_files_text`, `file_ref_template`, `drop_prompt_paths`, `drop_prompt_tree_path`)
- Modify: `crates/mkrew-ui/src/shell/task_dialog.rs:515-525` (the Prompt field wrapper) and `:526-539` (hint line)
- Modify: `docs/SPEC.md` (§20B.5, after line 2176), `docs/HUMAN_CHECKS.md`
- Test: `crates/mkrew-ui/src/composer.rs`

**Interfaces:**
- Consumes: `composer::typed_paths`, `composer::dropped_ref(template, path, is_dir, agent) -> String`, `Shell::prompt_typed(composer::Target::TaskDialog, ...)`, `Shell::machine_name()`, `mkrew_ui::machines::local_files_refused`, `DraggedPath {project_id, path, is_dir}` (`shell/files_render.rs:927`), `TaskDialog::project_id()`, `TaskDialog::busy`.
- Produces:
```rust
// crates/mkrew-ui/src/composer.rs
/// What an OS file drop types, on a pane or on the task dialog's prompt.
pub fn dropped_files(paths: &[std::path::PathBuf]) -> String;
// crates/mkrew-ui/src/shell/pane_render.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropTarget { Pane(Ulid), TaskPrompt }
pub(crate) fn drop_catcher(target: DropTarget, cx: &Context<'_, Shell>) -> impl IntoElement + use<>;
// main.rs Shell field
drop_hover: Option<(DropTarget, ExternalPaths)>,
// crates/mkrew-ui/src/shell/navigation.rs
impl Shell {
    fn dropped_files_text(&mut self, paths: &ExternalPaths, cx: &mut Context<'_, Self>) -> Option<String>;
    fn file_ref_template(&self) -> String;
    pub(crate) fn drop_prompt_paths(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<'_, Self>);
    pub(crate) fn drop_prompt_tree_path(&mut self, dragged: &DraggedPath, window: &mut Window, cx: &mut Context<'_, Self>);
}
```

- [ ] **Step 1: Write the failing test**

In `crates/mkrew-ui/src/composer.rs` `mod tests`, add (and `use std::path::PathBuf;` next to the other test imports at line 365-367):

```rust
    /// A drop on the task dialog types what a drop on a pane types (§20B.5): absolute paths,
    /// a path with whitespace quoted, separated by one space, no Enter, no trailing space.
    #[test]
    fn dropped_files_type_their_paths_like_a_pane_drop() {
        let paths = [
            PathBuf::from("/tmp/a.png"),
            PathBuf::from(r"C:\Users\Nguyen Tung\Pictures\ảnh 1.png"),
        ];
        assert_eq!(
            dropped_files(&paths),
            "/tmp/a.png \"C:\\Users\\Nguyen Tung\\Pictures\\ảnh 1.png\""
        );
        assert_eq!(dropped_files(&paths[..1]), "/tmp/a.png");
        assert_eq!(dropped_files(&[]), "");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p mkrew-ui dropped_files_type`
Expected: FAIL to compile, `cannot find function dropped_files in this scope`.

- [ ] **Step 3: Implement the pure helper**

`crates/mkrew-ui/src/composer.rs`, add after `typed_paths` (after line 300):

```rust
/// What an OS file drop types (§20B.5), on an agent pane or on the task dialog's prompt:
/// the absolute paths as [`typed_paths`] writes them.
pub fn dropped_files(paths: &[std::path::PathBuf]) -> String {
    let paths: Vec<_> = paths.iter().map(|path| path.to_string_lossy()).collect();
    typed_paths(paths.iter().map(|path| &**path))
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p mkrew-ui dropped_files_type`
Expected: PASS.

- [ ] **Step 5: One drop slot for panes and the dialog (`DropTarget`)**

`crates/mkrew-ui/src/main.rs:665-667`, replace with:

```rust
    /// What is under an OS file drag (§20B.5): a pane, or the task dialog's Prompt box. Tracked
    /// without GPUI's hover check: gpui-pre 0.3.7 keeps keyboard modality during OLE drags, so
    /// `on_drop` misses drops after typing.
    drop_hover: Option<(DropTarget, ExternalPaths)>,
```

`crates/mkrew-ui/src/shell/pane_render.rs`, replace the pane's `on_drag_move` (lines 499-511):

```rust
            .on_drag_move::<ExternalPaths>(cx.listener(
                move |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                    // A modal dialog covers the pane: its own box takes the drop, and this
                    // pane's catcher (painted first) must not take the slot from it.
                    if !shell.modal_open() && event.bounds.contains(&event.event.position) {
                        shell.drop_hover = Some((DropTarget::Pane(session), event.drag(cx).clone()));
                    } else if shell
                        .drop_hover
                        .as_ref()
                        .is_some_and(|(hovered, _)| *hovered == DropTarget::Pane(session))
                    {
                        shell.drop_hover = None;
                    }
                },
            ))
            .child(drop_catcher(DropTarget::Pane(session), cx))
```

(The line `.child(drop_catcher(session, cx))` at 512 is replaced by the last line above.)

Replace `drop_catcher` (lines 982-1010) and add the enum above it:

```rust
/// What an OS file drag hovers (§20B.5, ADR 0095).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DropTarget {
    /// An agent or shell pane.
    Pane(Ulid),
    /// The Prompt box of the open task dialog.
    TaskPrompt,
}

/// Delivers an OS file drop on `target` (see `Shell::drop_hover`).
// ponytail: works around gpui-pre 0.3.7 keeping keyboard modality during OLE drags
// (window.rs:5415 skips FileDrop), which makes every hover-gated mouse-up listener (`on_drop`,
// `capture_any_mouse_up`) miss drops after typing. Use plain `.on_drop` on the pane when
// upstream treats FileDrop as mouse input.
pub(crate) fn drop_catcher(target: DropTarget, cx: &Context<'_, Shell>) -> impl IntoElement + use<> {
    let view = cx.entity();
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let view = view.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                // A plain click has no active drag: stale paths are never typed.
                if phase != DispatchPhase::Capture || !cx.has_active_drag() {
                    return;
                }
                view.update(cx, |shell, cx| {
                    let hovered = shell.drop_hover.take_if(|(hovered, _)| *hovered == target);
                    if let Some((_, paths)) = hovered {
                        cx.stop_propagation();
                        match target {
                            DropTarget::Pane(session) => {
                                shell.drop_paths(session, &paths, window, cx);
                            }
                            DropTarget::TaskPrompt => shell.drop_prompt_paths(&paths, window, cx),
                        }
                    }
                });
            });
        },
    )
    .absolute()
    .size_full()
}
```

- [ ] **Step 6: Share the path typing between pane and dialog (navigation.rs)**

`crates/mkrew-ui/src/shell/navigation.rs`, replace `drop_paths` and `drop_tree_path` (lines 67-107) with:

```rust
    /// What an OS file drop types (§20B.5), or `None` after a toast: in a machine window these
    /// files are on this computer, the session or project is not.
    fn dropped_files_text(
        &mut self,
        paths: &ExternalPaths,
        cx: &mut Context<'_, Self>,
    ) -> Option<String> {
        if let Some(name) = self.machine_name().map(str::to_owned) {
            self.toast_info(mkrew_ui::machines::local_files_refused(&name), cx);
            return None;
        }
        Some(composer::dropped_files(paths.paths()))
    }

    /// The `composer.file_ref` template. A missing or broken settings file gives the default
    /// form (the daemon reports it).
    fn file_ref_template(&self) -> String {
        mkrew_core::settings::load(&self.settings_file)
            .unwrap_or_default()
            .composer
            .file_ref
    }

    /// Files dropped from the OS on `session`'s pane (§20B.5): their absolute paths.
    pub(crate) fn drop_paths(
        &mut self,
        session: Ulid,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(text) = self.dropped_files_text(paths, cx) {
            self.type_into_pane(session, &text, window, cx);
        }
    }

    /// A file tree row dropped on `session`'s pane (§20B.5, ADR 0081): `composer.file_ref`
    /// on an agent, the bare relative path on a shell.
    pub(crate) fn drop_tree_path(
        &mut self,
        session: Ulid,
        dragged: &DraggedPath,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let agent = (self.sidebar.sessions().iter())
            .any(|s| s.id == session && s.kind == SessionKind::Agent);
        let text = composer::dropped_ref(
            &self.file_ref_template(),
            &dragged.path,
            dragged.is_dir,
            agent,
        );
        self.type_into_pane(session, &text, window, cx);
    }

    /// Files dropped from the OS on the task dialog's Prompt box (§20B.5, ADR 0095): typed
    /// like a pane drop. A dialog that is saving takes nothing.
    pub(crate) fn drop_prompt_paths(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(text) = self.dropped_files_text(paths, cx) {
            self.type_into_prompt(&text, window, cx);
        }
    }

    /// A file tree row dropped on the task dialog's Prompt box: `composer.file_ref`, as on an
    /// agent pane, when the row is of the dialog's project.
    pub(crate) fn drop_prompt_tree_path(
        &mut self,
        dragged: &DraggedPath,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if (self.task_dialog.as_ref()).is_none_or(|dialog| dialog.project_id() != dragged.project_id)
        {
            return;
        }
        let text = composer::dropped_ref(
            &self.file_ref_template(),
            &dragged.path,
            dragged.is_dir,
            true,
        );
        self.type_into_prompt(&text, window, cx);
    }

    /// Types a drop into the open task dialog's prompt, unless it is saving or the drop is empty.
    fn type_into_prompt(&mut self, text: &str, window: &mut Window, cx: &mut Context<'_, Self>) {
        if text.is_empty() || (self.task_dialog.as_ref()).is_none_or(|dialog| dialog.busy) {
            return;
        }
        self.prompt_typed(composer::Target::TaskDialog, text, window, cx);
        cx.notify();
    }
```

`type_into_pane` (lines 109-132) stays unchanged: it keeps returning early while `modal_open()`.

- [ ] **Step 7: Dialog box: drop handlers, accent ring, hint line (shell/task_dialog.rs)**

Before `let panel = div()` (line 415), add:

```rust
        // The accent ring while a drag is over the Prompt box: state-driven for OS files
        // (`drop_hover`), `drag_over` for tree rows. The tint keeps it visible on a focused
        // field, whose own border is already the accent.
        let ring = matches!(self.drop_hover, Some((DropTarget::TaskPrompt, _))) && cx.has_active_drag();
        let (accent, tint) = (theme.accent, theme.tint(theme.accent));
        let ring_border: gpui::Hsla = if ring {
            rgb(accent).into()
        } else {
            gpui::transparent_black()
        };
        let drop_ring = div()
            .absolute()
            .inset_0()
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(ring_border)
            .when(ring, move |el| el.bg(tint))
            .drag_over::<DraggedPath>(move |style, _, _, _| {
                style.border_color(rgb(accent)).bg(tint)
            });
```

Replace the Prompt `field(...)` child (lines 515-525):

```rust
                    .child(
                        div()
                            .relative()
                            .on_drag_move::<ExternalPaths>(cx.listener(
                                |shell, event: &DragMoveEvent<ExternalPaths>, _, cx| {
                                    let inside = event.bounds.contains(&event.event.position);
                                    let hovered = matches!(
                                        shell.drop_hover,
                                        Some((DropTarget::TaskPrompt, _))
                                    );
                                    if inside {
                                        shell.drop_hover =
                                            Some((DropTarget::TaskPrompt, event.drag(cx).clone()));
                                    } else if hovered {
                                        shell.drop_hover = None;
                                    }
                                    if inside != hovered {
                                        cx.notify();
                                    }
                                },
                            ))
                            .on_drop(cx.listener(
                                |shell, dragged: &DraggedPath, window, cx| {
                                    shell.drop_prompt_tree_path(dragged, window, cx);
                                },
                            ))
                            .child(
                                field(
                                    "task-prompt",
                                    Field::Prompt,
                                    "Sent to the agent when the task runs.",
                                )
                                .h(px(theme::PROMPT_FIELD_HEIGHT))
                                .overflow_y_scroll()
                                .track_scroll(&self.prompt_scroll)
                                .when(rewriting, |el| el.opacity(opacity::DISABLED)),
                            )
                            .child(drop_catcher(DropTarget::TaskPrompt, cx))
                            .child(drop_ring),
                    )
```

Replace the hint text (lines 535-539; if part A already changed the string, keep its additions and add the paste or drop part):

```rust
                            .child(if rewriting {
                                "Improving the prompt…"
                            } else {
                                "@ for a file · paste or drop an image · Ctrl+Enter to save"
                            }),
```

- [ ] **Step 8: SPEC §20B.5 and HUMAN_CHECKS**

`docs/SPEC.md`, after the paragraph ending `and the pane shows an accent ring while the row hovers it.` (line 2176, before item `6. **Run with…**`), insert:

```markdown
   **Task dialog.** The Prompt box of the task dialog takes the same inputs (ADR 0095).
   A pasted image is saved with `attachment.save` and owner `project` (the dialog's project), because a new task has no id and no session yet.
   Dropped OS files type their absolute paths as on a pane, and a dropped tree row types `composer.file_ref` as on an agent pane.
   While a drag is over the box it shows the accent ring.
   A machine window refuses OS files as on a pane.
   `type_into_pane` keeps ignoring drops while a dialog is open: the box handles its own.
```

`docs/HUMAN_CHECKS.md`, append to the "To verify - 2026-11-04 Task dialog attachments (ADR 0095)" section from Task B3:

```markdown
- [ ] **OS file drop (dark + light):** Board → New task; drag a file from Explorer over the Prompt box: it shows the accent ring (border plus a faint tint, also while the box is focused); let go: the quoted absolute path is typed at the end of the prompt, no Enter, the caret stays in the prompt.
  Drag two files: both paths, one space apart.
  Move the drag out of the box without dropping: the ring goes away.
  Drop on the Title, the buttons or beside the dialog: nothing is typed.
- [ ] **Drop in a machine window:** the same drop on a machine window's New task shows "These files are on this computer; <machine> cannot read them." and types nothing.
  Paste of an image there works (it is uploaded as bytes).
- [ ] **While busy:** drop a file while ✦ Improve runs: the path is added after the rewritten text.
  While "Saving…": nothing is typed.
- [ ] **File tree rows (known limit):** a Files tree row cannot be dragged onto the open dialog, because the dialog's shade blocks the tree.
  This is expected; the row handler (project check, `composer.file_ref`, accent ring) exists for when that changes.
- [ ] **Panes unchanged:** drop a file and a tree row on an agent pane and on a shell pane with no dialog open: same text and the same ring as before.
```

- [ ] **Step 9: Run the checks**

Run: `cargo fmt --all`
Run: `cargo check -p mkrew-ui`
Expected: no errors; `grep -rn "drop_catcher(session" crates/` finds nothing; no remaining `Option<(Ulid, ExternalPaths)>`.

Run: `cargo test -p mkrew-ui composer`
Expected: PASS (including `dropped_files_type_their_paths_like_a_pane_drop` and the existing `dropped_refs_follow_the_pane_kind_and_mark_folders`).

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

Run (manual, dark and light): `cargo run -p mkrew-ui`, open Board → New task, drag a file over the Prompt box; the ring shows in both themes (screenshots go to HUMAN_CHECKS as before).

- [ ] **Step 10: Commit**

```bash
git add crates/mkrew-ui/src/composer.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/task_dialog.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): drop files and tree rows on the New task prompt with an accent ring"
```

---

### Part B self-review

- **Spec coverage:** 6.1 owner and paths (B1), unknown project error (B1 test), 6.2 retention with the exact "not done task prompt contains file name" query (B2), 6.3 paste through the shared key path (B3), 6.4 OS and tree drops, ring, shared typing, `type_into_pane` unchanged (B4), ADR 0095 (B1), SPEC §20B.1 (B3), §20B.5 and retention and protocol (B1, B2, B4), HUMAN_CHECKS entry changed plus To verify (B3, B4), testing table "Attachments" and "Dialog" rows (B1, B2, B3, B4).
- **Spec gap found:** section 6.4 asks the dialog box to take file tree row drags, but `modal_shade` is `.occlude()` and the dialog is modal, so a tree row can never start a drag while the dialog is open. B4 builds the handler anyway and records the limit in ADR 0095 and HUMAN_CHECKS; if the user wants drags to work, the shade must stop blocking the Files tree (a separate design decision).
- **Cross-part dependency:** B3 step 5 edits the paste branch in part A's `prompt_keys.rs` without seeing it. If A's branch differs in shape (for example the dialog already inserts text elsewhere), keep A's guard lines and replace only the clipboard handling with `self.prompt_paste(target, window, cx)`. The hint line in B4 step 7 may also have been edited by A (`/` hint); keep A's text and add the paste or drop words.
- **Protocol numbering:** B1 hard-assumes part A leaves `PROTOCOL_VERSION = 52`; the SPEC protocol line is inserted after A's `Protocol 52` line.

---

# Part C: Prompt library manager (C1-C5)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Settings → Prompts page (Ctrl+,) where the user lists, adds, edits, renames and deletes the prompt library (`<config>/mkrew/prompts/*.md`) without a file manager, reachable from the palette and from the prompt picker (which the composer and the New task "From prompt…" button share).

**Architecture:** `mkrew_core::prompt` gets the file operations (`validate_name`, `create`, `save` via temp file and rename, `rename`, `delete`, `read`) with thiserror errors.
A pure UI model `mkrew_ui::prompts_page` (list, draft, dirty state, which write a Save needs) is driven by `SettingsDialog` (`Tab::Prompts`, `Focus::Prompts`, `Effect::Prompts`).
`shell/prompts.rs` runs the effects (folder reads, writes, the Save / Don't save / Cancel question, delete confirmation) and draws the page.
The prompt picker gets a last row "Manage prompts…"; the palette gets `Extra::ManagePrompts`; both call `Shell::open_prompts_settings`.

**Tech Stack:** Rust, `thiserror` 2 and `tempfile` 3 (both already in `mkrew-core`; `tempfile` already a `mkrew-ui` dev-dependency), GPUI.

**Spec:** `docs/superpowers/specs/2026-11-04-prompt-skills-design.md` section 7 (and section 8 "Prompt CRUD").

## Global Constraints (part C)

- Work in `.worktrees/prompt-skills` (branch `feat/prompt-skills`); commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, no co-author line, `git add <explicit files>` only.
- No protocol change, no migration, no ADR (the deviations below are refinements, listed in "Spec notes").
- No `unwrap` / `expect` outside tests; library errors with `thiserror`.
- UI: every color, spacing, radius and font size from `crates/mkrew-ui/src/theme.rs` (new tokens in Task C4 only); spacing 4/8/12/16/24/32; works in dark and light; hover, focus, active and disabled states come from `button`, `secondary_button`, `danger_button`, `disabled_button`, `focus_stop` and `focusable_bordered`; keyboard-only use (ADR 0085); tooltips show shortcuts.
- Never the em dash character in code, docs or UI text.
- Markdown docs: one sentence per line.
- Line numbers below are from the worktree at the time of writing; parts A and B also edit `shell/composer.rs`, `shell/navigation.rs`, `main.rs`, `lib.rs` and `palette.rs`, so anchor on the quoted symbols, not on the numbers.
- Verify (once, at the end of C5): `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`. Fast loop: `cargo test -p <crate> <filter>`.
- Part C does not use `prompt_key_down` (part A) and can be built on a tree where A and B have not landed.

## Review Focus candidates

1. A rename to another case of the same name (`review` to `Review`) must work on a case-insensitive disk (Windows, macOS) and must not look like a duplicate of itself, while `Review` and `review` as two files on a case-sensitive disk must never overwrite each other. Pinned by `rename_to_a_new_case_of_its_own_name_works` and `rename_refuses_an_existing_target_in_any_case` (C1).
2. Unsaved text is never lost: a rename together with edited text, or a text write that fails after the rename succeeded, keeps the draft dirty under the new name so the retry saves it. Pinned by `a_rename_with_edited_text_moves_the_file_then_writes_the_text` and `a_failed_save_after_a_rename_keeps_the_new_name_for_the_retry` (C2); a name with only blanks added or removed around it is not a change (`an_opened_prompt_is_dirty_when_name_or_text_changes_and_clean_again_when_reverted`, C2).
3. A name typed in the page can never leave the prompts folder or name a Windows device (`../x`, `a\b`, `CON`, `aux.v2`). Pinned by `a_refused_name_writes_nothing_and_cannot_escape_the_folder` and `every_invalid_name_is_refused_with_its_own_reason` (C1).

## Spec notes (gaps and refinements found while planning)

- The spec names the error type `PromptError`, but `mkrew_core::prompt::PromptError { file, message }` already exists (an unreadable prompt file in `PromptLibrary.errors`). Task C1 renames the existing struct to `ReadError` (only `prompt.rs` names the type; `shell/composer.rs` uses its fields) and gives `PromptError` to the new thiserror enum.
- The editor must keep the `# Title` line, but `load_dir` strips it into `Prompt.title`. Task C1 adds `read(dir, name) -> Result<String, PromptError>` (the whole file, BOM removed, `\n` line breaks) so the editor shows and rewrites the file exactly. The spec lists five functions; `read` is additive.
- The spec's reserved-name rule is applied to the part before the first dot too (`aux.v2` is `AUX` on Windows). One sentence in design section 7.1 is updated in C1.
- `create` and `save` create a missing folder (a user who deleted the folder, or a first run) instead of failing; `delete`, `rename` and `read` report `Missing`.
- Spec 7.3 says the New task "From prompt" menu gets the row. Since T10.5 "From prompt…" is a button that opens the same modal prompt picker as the composer, so the picker's last row serves both; Task C5 pins it with `Target::TaskDialog`.
- The list shows the prompt's name in muted text only when it differs from the title (a prompt without `# Title` would otherwise show its name twice).
- Closing the whole window with unsaved prompt text does not ask (like the task dialog's draft); Esc, the only way to close Settings, does.

---

### Task 22 (C1): Core: prompt CRUD in `mkrew_core::prompt`

**Files:**
- Modify: `crates/mkrew-core/src/prompt.rs` (module doc lines 1-5; rename `PromptError` at lines 44-49, 57, 176 and 212-213; new code before `#[cfg(test)]` at line 219; tests appended to `mod tests`)
- Modify: `docs/superpowers/specs/2026-11-04-prompt-skills-design.md` (section 7.1, the name rule bullet, line 287)

**Interfaces:**
- Consumes: `thiserror` (already a `mkrew-core` dependency), `tempfile` (dev).
- Produces:
```rust
pub const NAME_MAX: usize = 64;
pub enum NameError { Empty, TooLong, LeadingDot, Forbidden(char), Control, Trailing, Reserved(String), Duplicate(String) }
pub enum PromptError { Name(NameError), Missing(String), Io { action: &'static str, path: PathBuf, source: io::Error } }
pub struct ReadError { pub file: PathBuf, pub message: String }   // the old PromptError struct, renamed
pub fn validate_name(dir: &Path, name: &str, renaming: Option<&str>) -> Result<(), NameError>;
pub fn create(dir: &Path, name: &str, body: &str) -> Result<(), PromptError>;
pub fn save(dir: &Path, name: &str, body: &str) -> Result<(), PromptError>;
pub fn rename(dir: &Path, old: &str, new: &str) -> Result<(), PromptError>;
pub fn delete(dir: &Path, name: &str) -> Result<(), PromptError>;
pub fn read(dir: &Path, name: &str) -> Result<String, PromptError>;
```
`body` of `create` and `save` is the whole file text (a `# Title` first line included); `PromptLibrary.errors` becomes `Vec<ReadError>`.

- [ ] **Step 1: Write the failing tests**

Append inside `mod tests` of `crates/mkrew-core/src/prompt.rs` (after `md_directories_are_skipped`):

```rust
    /// A prompts folder inside a fresh tempdir.
    fn library_dir() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("prompts");
        std::fs::create_dir(&dir).unwrap();
        (tmp, dir)
    }

    /// The file names of `dir`, sorted.
    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn create_writes_the_file_and_the_library_reads_it_back() {
        let (_tmp, dir) = library_dir();
        create(&dir, "mine", "# Mine\nhello {file}\n").unwrap();
        assert_eq!(files(&dir), ["mine.md"]);
        assert_eq!(
            load_dir(&dir).prompts,
            [Prompt {
                name: "mine".into(),
                title: "Mine".into(),
                body: "hello {file}\n".into(),
            }]
        );
    }

    #[test]
    fn create_and_save_make_a_missing_folder_and_validation_accepts_it() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("not").join("yet");
        assert_eq!(validate_name(&dir, "mine", None), Ok(()));
        create(&dir, "mine", "x").unwrap();
        assert_eq!(files(&dir), ["mine.md"]);
        let other = tmp.path().join("other");
        save(&other, "kept", "typed text").unwrap();
        assert_eq!(read(&other, "kept").unwrap(), "typed text");
    }

    #[test]
    fn create_refuses_a_duplicate_in_any_case_and_keeps_the_first_file() {
        let (_tmp, dir) = library_dir();
        create(&dir, "Review", "first").unwrap();
        let result = create(&dir, "review", "second");
        assert!(
            matches!(&result, Err(PromptError::Name(NameError::Duplicate(other))) if other == "Review"),
            "{result:?}"
        );
        assert_eq!(files(&dir), ["Review.md"]);
        assert_eq!(read(&dir, "Review").unwrap(), "first");
        // A folder named like a prompt counts too: creating over it could not work.
        std::fs::create_dir(dir.join("Folder.md")).unwrap();
        assert_eq!(
            validate_name(&dir, "folder", None),
            Err(NameError::Duplicate("Folder".into()))
        );
    }

    #[test]
    fn every_invalid_name_is_refused_with_its_own_reason() {
        let (_tmp, dir) = library_dir();
        let long = "a".repeat(65);
        let wide = "é".repeat(65);
        let refused: Vec<(&str, NameError)> = vec![
            ("", NameError::Empty),
            ("   ", NameError::Empty),
            (long.as_str(), NameError::TooLong),
            (wide.as_str(), NameError::TooLong),
            (".hidden", NameError::LeadingDot),
            ("..", NameError::LeadingDot),
            ("a\tb", NameError::Control),
            ("a\u{7}", NameError::Control),
            ("a ", NameError::Trailing),
            ("a.", NameError::Trailing),
            ("CON", NameError::Reserved("CON".into())),
            ("nul", NameError::Reserved("nul".into())),
            ("Com1", NameError::Reserved("Com1".into())),
            ("LPT9", NameError::Reserved("LPT9".into())),
            ("aux.v2", NameError::Reserved("aux.v2".into())),
        ];
        for (name, expected) in refused {
            assert_eq!(validate_name(&dir, name, None), Err(expected), "{name:?}");
        }
        for c in ['/', '\\', ':', '*', '?', '"', '<', '>', '|'] {
            assert_eq!(
                validate_name(&dir, &format!("a{c}b"), None),
                Err(NameError::Forbidden(c)),
                "{c}"
            );
        }
        // The limit counts characters, not bytes; near misses are fine.
        let ok_long = "a".repeat(64);
        let ok_wide = "é".repeat(64);
        for name in [
            "review",
            "Fix tests",
            "COM0",
            "console",
            "name.with.dots",
            "ünï cødé 名前",
            "-dash",
            ok_long.as_str(),
            ok_wide.as_str(),
        ] {
            assert_eq!(validate_name(&dir, name, None), Ok(()), "{name:?}");
        }
    }

    #[test]
    fn a_refused_name_writes_nothing_and_cannot_escape_the_folder() {
        let (tmp, dir) = library_dir();
        let victim = tmp.path().join("victim.md");
        std::fs::write(&victim, "keep").unwrap();
        for evil in ["../victim", "..\\victim", "sub/x", ".."] {
            assert!(matches!(create(&dir, evil, "x"), Err(PromptError::Name(_))), "{evil}");
            assert!(matches!(save(&dir, evil, "x"), Err(PromptError::Name(_))), "{evil}");
            assert!(matches!(delete(&dir, evil), Err(PromptError::Name(_))), "{evil}");
            assert!(matches!(read(&dir, evil), Err(PromptError::Name(_))), "{evil}");
            assert!(matches!(rename(&dir, evil, "ok"), Err(PromptError::Name(_))), "{evil}");
            assert!(matches!(rename(&dir, "ok", evil), Err(PromptError::Name(_))), "{evil}");
        }
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert!(files(&dir).is_empty());
        assert_eq!(files(tmp.path()), ["prompts", "victim.md"]);
    }

    #[test]
    fn save_replaces_the_text_and_leaves_no_temp_file() {
        let (_tmp, dir) = library_dir();
        create(&dir, "x", "old").unwrap();
        save(&dir, "x", "# New\nnew text").unwrap();
        assert_eq!(read(&dir, "x").unwrap(), "# New\nnew text");
        assert_eq!(files(&dir), ["x.md"]);
        // A prompt deleted meanwhile is written again: the typed text is not lost.
        delete(&dir, "x").unwrap();
        save(&dir, "x", "typed").unwrap();
        assert_eq!(read(&dir, "x").unwrap(), "typed");
    }

    #[test]
    fn a_failed_save_leaves_neither_a_temp_file_nor_a_changed_target() {
        let (_tmp, dir) = library_dir();
        // The target is a folder: the final rename cannot succeed.
        std::fs::create_dir(dir.join("x.md")).unwrap();
        let result = save(&dir, "x", "body");
        assert!(matches!(result, Err(PromptError::Io { .. })), "{result:?}");
        assert_eq!(files(&dir), ["x.md"]);
    }

    #[test]
    fn rename_moves_the_file_and_keeps_its_text() {
        let (_tmp, dir) = library_dir();
        create(&dir, "one", "# One\nbody\n").unwrap();
        rename(&dir, "one", "two").unwrap();
        assert_eq!(files(&dir), ["two.md"]);
        assert_eq!(read(&dir, "two").unwrap(), "# One\nbody\n");
        rename(&dir, "two", "two").unwrap();
        assert_eq!(files(&dir), ["two.md"], "the same name is a no-op");
    }

    #[test]
    fn rename_refuses_an_existing_target_in_any_case() {
        let (_tmp, dir) = library_dir();
        create(&dir, "a", "text a").unwrap();
        create(&dir, "b", "text b").unwrap();
        let result = rename(&dir, "a", "B");
        assert!(
            matches!(&result, Err(PromptError::Name(NameError::Duplicate(other))) if other == "b"),
            "{result:?}"
        );
        assert_eq!(files(&dir), ["a.md", "b.md"]);
        assert_eq!(read(&dir, "a").unwrap(), "text a");
        assert_eq!(read(&dir, "b").unwrap(), "text b");
    }

    #[test]
    fn rename_to_a_new_case_of_its_own_name_works() {
        let (_tmp, dir) = library_dir();
        create(&dir, "review", "text").unwrap();
        // Its own name does not count as a duplicate while it is the one being renamed.
        assert_eq!(validate_name(&dir, "REVIEW", Some("review")), Ok(()));
        assert_eq!(
            validate_name(&dir, "REVIEW", None),
            Err(NameError::Duplicate("review".into()))
        );
        rename(&dir, "review", "Review").unwrap();
        assert_eq!(files(&dir), ["Review.md"]);
        assert_eq!(read(&dir, "Review").unwrap(), "text");
    }

    #[test]
    fn rename_and_delete_of_a_missing_prompt_say_so() {
        let (tmp, dir) = library_dir();
        let result = rename(&dir, "ghost", "x");
        assert!(matches!(&result, Err(PromptError::Missing(name)) if name == "ghost"), "{result:?}");
        let result = delete(&dir, "ghost");
        assert!(matches!(&result, Err(PromptError::Missing(name)) if name == "ghost"), "{result:?}");
        let gone = tmp.path().join("no-folder");
        assert!(matches!(delete(&gone, "x"), Err(PromptError::Missing(_))));
        assert!(matches!(rename(&gone, "x", "y"), Err(PromptError::Missing(_))));
        assert!(matches!(read(&gone, "x"), Err(PromptError::Missing(_))));
    }

    #[test]
    fn delete_removes_only_that_file() {
        let (_tmp, dir) = library_dir();
        create(&dir, "a", "x").unwrap();
        create(&dir, "b", "y").unwrap();
        delete(&dir, "a").unwrap();
        assert_eq!(files(&dir), ["b.md"]);
    }

    #[test]
    fn read_gives_the_whole_file_with_unix_line_breaks() {
        let (_tmp, dir) = library_dir();
        std::fs::write(dir.join("t.md"), "\u{feff}# T\r\nA\r\n").unwrap();
        assert_eq!(read(&dir, "t").unwrap(), "# T\nA\n");
    }

    #[test]
    fn the_error_messages_are_the_sentences_the_page_shows() {
        assert_eq!(NameError::Empty.to_string(), "Give the prompt a name.");
        assert_eq!(
            NameError::Duplicate("Review".into()).to_string(),
            "A prompt named \"Review\" exists already."
        );
        assert_eq!(
            PromptError::Missing("x".into()).to_string(),
            "There is no prompt named \"x\" any more."
        );
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-core prompt`
Expected: compile error, `validate_name`, `create`, `save`, `rename`, `delete`, `read`, `NameError`, `PromptError` (as an enum) not found.

- [ ] **Step 3: Implement**

3a. Update the module doc (lines 1-5) to:

```rust
//! Prompt library (SPEC §20B.3): `<config>/mkrew/prompts/*.md`, rendered with [`render`].
//!
//! File stem = name; an optional first line `# Title` gives the title (else the name) and is
//! not part of the body. Three samples are seeded when the directory is missing, so a user who
//! deletes them all keeps an empty library. The Settings → Prompts page edits the folder with
//! [`create`], [`save`], [`rename`], [`delete`] and [`read`].
```

3b. Rename the old struct. At lines 44-49:

```rust
/// A prompt file that could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadError {
    pub file: PathBuf,
    pub message: String,
}
```

Line 57: `pub errors: Vec<ReadError>,`
Line 176: `lib.errors.push(ReadError {`
Lines 212-213:

```rust
fn io_error(path: &Path, e: &io::Error) -> ReadError {
    ReadError {
```

3c. Insert before `#[cfg(test)]` (line 219):

```rust
/// The longest prompt name, in characters.
pub const NAME_MAX: usize = 64;

/// Characters a file name cannot hold on Windows (and `/` on every system).
const FORBIDDEN: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Names Windows keeps for devices, with any extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Why a prompt name is refused. Each message is the sentence the Prompts page shows under the
/// Name field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("Give the prompt a name.")]
    Empty,
    #[error("A name has at most 64 characters.")]
    TooLong,
    #[error("A name cannot start with a dot.")]
    LeadingDot,
    #[error("A name cannot contain `{0}`.")]
    Forbidden(char),
    #[error("A name cannot contain control characters.")]
    Control,
    #[error("A name cannot end with a space or a dot.")]
    Trailing,
    #[error("\"{0}\" is a reserved name on Windows.")]
    Reserved(String),
    /// Carries the existing prompt's own spelling.
    #[error("A prompt named \"{0}\" exists already.")]
    Duplicate(String),
}

/// A prompt operation that did not happen.
#[derive(Debug, thiserror::Error)]
pub enum PromptError {
    #[error(transparent)]
    Name(#[from] NameError),
    #[error("There is no prompt named \"{0}\" any more.")]
    Missing(String),
    #[error("Cannot {action} {}: {source}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
}

fn io_failure(action: &'static str, path: &Path, source: io::Error) -> PromptError {
    PromptError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

fn file(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.md"))
}

/// The rules for a name as a file stem, without looking at the folder. Every operation runs it
/// first, so a name can never leave `dir`.
fn check_format(name: &str) -> Result<(), NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    if name.chars().count() > NAME_MAX {
        return Err(NameError::TooLong);
    }
    if name.starts_with('.') {
        return Err(NameError::LeadingDot);
    }
    for c in name.chars() {
        if c.is_control() {
            return Err(NameError::Control);
        }
        if FORBIDDEN.contains(&c) {
            return Err(NameError::Forbidden(c));
        }
    }
    if name.ends_with([' ', '.']) {
        return Err(NameError::Trailing);
    }
    // Windows reads `aux.v2.md` as the device `AUX`: the part before the first dot decides.
    let base = name.split('.').next().unwrap_or(name).trim_end();
    if RESERVED.iter().any(|reserved| reserved.eq_ignore_ascii_case(base)) {
        return Err(NameError::Reserved(name.to_owned()));
    }
    Ok(())
}

/// The stems of every `*.md` entry of `dir`; a folder that cannot be listed has none (the write
/// that follows reports the real error).
fn stems(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if !path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("md")) {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_owned())
        })
        .collect()
}

/// Whether `name` can be the name of a prompt in `dir`. `renaming` is the prompt being renamed:
/// its own name is not a duplicate, so `review` may become `Review`. Names compare
/// case-insensitively; the error carries the other prompt's spelling.
pub fn validate_name(dir: &Path, name: &str, renaming: Option<&str>) -> Result<(), NameError> {
    check_format(name)?;
    let lower = name.to_lowercase();
    let taken = stems(dir)
        .into_iter()
        .find(|stem| stem.to_lowercase() == lower && Some(stem.as_str()) != renaming);
    match taken {
        Some(other) => Err(NameError::Duplicate(other)),
        None => Ok(()),
    }
}

/// Writes `text` as `dir/name.md` through a temp file in the same folder and a rename over the
/// target, so a crash never leaves half a prompt. The folder is created when missing. The temp
/// file ends in `.tmp`, which [`load_dir`] ignores.
fn write_atomic(dir: &Path, name: &str, text: &str) -> Result<(), PromptError> {
    std::fs::create_dir_all(dir).map_err(|e| io_failure("create", dir, e))?;
    let target = file(dir, name);
    let temp = dir.join(format!(".{name}.md.tmp"));
    let result = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, &target));
    if let Err(source) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(io_failure("write", &target, source));
    }
    Ok(())
}

/// A new prompt; the name must be valid and unused. `body` is the whole file, `# Title` first
/// line included.
// ponytail: the duplicate check and the write are not one atomic step; a second writer between
// them (another Mkrew window, an editor) can be overwritten. One user, one folder; upgrade path:
// `create_new` on the final file.
pub fn create(dir: &Path, name: &str, body: &str) -> Result<(), PromptError> {
    validate_name(dir, name, None)?;
    write_atomic(dir, name, body)
}

/// Replaces the text of `name` (written again when the file went away meanwhile).
pub fn save(dir: &Path, name: &str, body: &str) -> Result<(), PromptError> {
    check_format(name)?;
    write_atomic(dir, name, body)
}

/// Renames `old` to `new`; refused when another prompt has that name in any case. A new case of
/// the same name is allowed.
pub fn rename(dir: &Path, old: &str, new: &str) -> Result<(), PromptError> {
    check_format(old)?;
    validate_name(dir, new, Some(old))?;
    if old == new {
        return Ok(());
    }
    let from = file(dir, old);
    std::fs::rename(&from, file(dir, new)).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            PromptError::Missing(old.to_owned())
        } else {
            io_failure("rename", &from, source)
        }
    })
}

/// Deletes the prompt file.
pub fn delete(dir: &Path, name: &str) -> Result<(), PromptError> {
    check_format(name)?;
    let path = file(dir, name);
    std::fs::remove_file(&path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            PromptError::Missing(name.to_owned())
        } else {
            io_failure("delete", &path, source)
        }
    })
}

/// The whole file of `name` as the editor shows it: without a byte order mark, with `\n` line
/// breaks, the `# Title` line included.
pub fn read(dir: &Path, name: &str) -> Result<String, PromptError> {
    check_format(name)?;
    let path = file(dir, name);
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(text
            .strip_prefix('\u{feff}')
            .unwrap_or(&text)
            .replace("\r\n", "\n")),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            Err(PromptError::Missing(name.to_owned()))
        }
        Err(source) => Err(io_failure("read", &path, source)),
    }
}
```

3d. Design doc `docs/superpowers/specs/2026-11-04-prompt-skills-design.md` line 287: replace `or a Windows reserved name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`).` with `or a Windows reserved name (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`), also when it is the part before the first dot (`aux.v2`).`
In the bullet list after line 290 (`rename` refuses...) add one line: `- `read(dir, name)` returns the whole file (title line included) for the editor; `create` and `save` create a missing folder.`

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-core prompt`
Expected: all prompt tests pass, the old ones included (`PromptLibrary` is unchanged except the error type name).

Run: `cargo clippy -p mkrew-core --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```
git add crates/mkrew-core/src/prompt.rs docs/superpowers/specs/2026-11-04-prompt-skills-design.md
git -c user.email=tungnv975@gmail.com commit -m "feat(core): prompt create, save, rename, delete and read with name rules"
```

---

### Task 23 (C2): UI model: `prompts_page` (list, draft, dirty state, which write)

**Files:**
- Create: `crates/mkrew-ui/src/prompts_page.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod prompts_page;` after `pub mod prompt_picker;`, line 77)

**Interfaces:**
- Consumes: `mkrew_core::prompt::{self, PromptError, PromptLibrary}` (Task C1), `crate::text_box::{Input, TextBox}`.
- Produces:
```rust
pub enum Spot { List, Name, Body }
pub enum PromptAction { Open(String), New, Save, Delete, OpenFolder }
pub enum Write { Create { name: String, text: String }, Save { name: String, text: String }, Rename { old: String, new: String, text: Option<String> } }
pub struct Written { pub renamed: bool, pub result: Result<(), PromptError> }
impl Write { pub fn run(&self, dir: &Path) -> Written }
pub struct Draft { pub origin: Option<String>, pub name: Input, pub body: TextBox, pub name_error: Option<String>, /* private saved_name, saved_text */ }
impl Draft { pub fn new_prompt() -> Self; pub fn open(name: &str, text: &str) -> Self; pub fn name(&self) -> &str; pub fn is_dirty(&self) -> bool; pub fn write(&self) -> Option<Write> }
pub struct PromptsPage { pub library: PromptLibrary, pub draft: Option<Draft>, pub error: Option<String>, pub stale: bool }
impl Default for PromptsPage   // stale: true
impl PromptsPage {
    pub fn is_dirty(&self) -> bool;
    pub fn clean_origin(&self) -> Option<&str>;
    pub fn start_new(&mut self);
    pub fn open(&mut self, name: &str, text: &str);
    pub fn refresh(&mut self, name: &str, text: &str);
    pub fn reload(&mut self, library: PromptLibrary);
    pub fn neighbour(&self, by: i32) -> Option<String>;
    pub fn written(&mut self, written: Written) -> bool;
}
```

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-ui/src/prompts_page.rs` with only the module doc and the test module first, so the tests fail to compile for the right reason:

```rust
//! The Settings → Prompts page's model (SPEC §14, §20B.3): the library as listed, the draft being
//! edited (a prompt of the folder or a new one), when it counts as changed and which file write
//! saving it takes. `settings_dialog` routes the keys, `shell/prompts.rs` shows it and asks the
//! user. Tested headlessly.

#[cfg(test)]
mod tests {
    use mkrew_core::prompt::{NameError, Prompt, PromptError};

    use super::*;

    fn prompt(name: &str) -> Prompt {
        Prompt {
            name: name.into(),
            title: name.to_uppercase(),
            body: String::new(),
        }
    }

    fn page(names: &[&str]) -> PromptsPage {
        PromptsPage {
            library: PromptLibrary {
                prompts: names.iter().map(|name| prompt(name)).collect(),
                errors: Vec::new(),
            },
            ..PromptsPage::default()
        }
    }

    #[test]
    fn a_new_draft_is_clean_until_something_is_typed() {
        let mut draft = Draft::new_prompt();
        assert!(!draft.is_dirty());
        assert_eq!(draft.write(), None, "nothing to save");
        draft.name.insert("idea");
        assert!(draft.is_dirty());
        assert_eq!(
            draft.write(),
            Some(Write::Create {
                name: "idea".into(),
                text: String::new()
            })
        );
        draft.name.clear();
        assert!(!draft.is_dirty(), "typed text deleted again");
        draft.body.insert("only text");
        assert!(draft.is_dirty(), "a body without a name is a change the Save refuses by name");
    }

    #[test]
    fn an_opened_prompt_is_dirty_when_name_or_text_changes_and_clean_again_when_reverted() {
        let mut draft = Draft::open("review", "# Review\nbody\n");
        assert!(!draft.is_dirty());
        assert_eq!(draft.origin.as_deref(), Some("review"));
        draft.body.insert("more");
        assert!(draft.is_dirty());
        draft.body.key(crate::task_dialog::DialogKey::Backspace);
        draft.body.key(crate::task_dialog::DialogKey::Backspace);
        draft.body.key(crate::task_dialog::DialogKey::Backspace);
        draft.body.key(crate::task_dialog::DialogKey::Backspace);
        assert!(!draft.is_dirty(), "text typed and deleted again");
        // Blanks around the name are not part of it.
        draft.name.insert("  ");
        assert!(!draft.is_dirty());
        assert_eq!(draft.name(), "review");
        draft.name.insert("x");
        assert!(draft.is_dirty());
    }

    #[test]
    fn write_picks_create_save_or_rename() {
        let mut draft = Draft::open("old", "text");
        assert_eq!(draft.write(), None);
        draft.body.insert("!");
        assert_eq!(
            draft.write(),
            Some(Write::Save {
                name: "old".into(),
                text: "text!".into()
            })
        );
        let mut renamed = Draft::open("old", "text");
        renamed.name.clear();
        renamed.name.insert(" new ");
        assert_eq!(
            renamed.write(),
            Some(Write::Rename {
                old: "old".into(),
                new: "new".into(),
                text: None
            }),
            "only the name changed: the text is not written"
        );
        renamed.body.insert("!");
        assert_eq!(
            renamed.write(),
            Some(Write::Rename {
                old: "old".into(),
                new: "new".into(),
                text: Some("text!".into())
            })
        );
    }

    #[test]
    fn a_rename_with_edited_text_moves_the_file_then_writes_the_text() {
        let dir = tempfile::tempdir().unwrap();
        prompt::create(dir.path(), "old", "# Old\nbody\n").unwrap();
        let mut draft = Draft::open("old", "# Old\nbody\n");
        draft.name.clear();
        draft.name.insert("new");
        draft.body.insert("more ");
        let write = draft.write().unwrap();
        let done = write.run(dir.path());
        assert!(done.renamed && done.result.is_ok(), "{:?}", done.result);
        assert_eq!(
            prompt::read(dir.path(), "new").unwrap(),
            "# Old\nbody\nmore "
        );
        assert!(matches!(
            prompt::read(dir.path(), "old"),
            Err(PromptError::Missing(_))
        ));
    }

    #[test]
    fn a_refused_name_keeps_the_file_and_the_draft_and_says_why_under_the_field() {
        let dir = tempfile::tempdir().unwrap();
        prompt::create(dir.path(), "a", "text a").unwrap();
        prompt::create(dir.path(), "b", "text b").unwrap();
        let mut page = PromptsPage::default();
        page.open("a", "text a");
        let draft = page.draft.as_mut().unwrap();
        draft.name.clear();
        draft.name.insert("B");
        draft.body.insert("!");
        let write = page.draft.as_ref().unwrap().write().unwrap();
        let done = write.run(dir.path());
        assert!(!done.renamed);
        assert!(!page.written(done));
        let draft = page.draft.as_ref().unwrap();
        assert_eq!(
            draft.name_error.as_deref(),
            Some("A prompt named \"b\" exists already.")
        );
        assert_eq!(page.error, None, "a name problem is shown under the field");
        assert!(draft.is_dirty(), "the typed text is still there");
        assert_eq!(prompt::read(dir.path(), "a").unwrap(), "text a");
        assert_eq!(
            prompt::validate_name(dir.path(), "", None),
            Err(NameError::Empty)
        );
    }

    #[test]
    fn a_failed_save_after_a_rename_keeps_the_new_name_for_the_retry() {
        let mut page = PromptsPage::default();
        page.open("old", "text");
        let draft = page.draft.as_mut().unwrap();
        draft.name.clear();
        draft.name.insert("new");
        draft.body.insert("!");
        let io = PromptError::Io {
            action: "write",
            path: "prompts/new.md".into(),
            source: std::io::Error::other("disk full"),
        };
        assert!(!page.written(Written {
            renamed: true,
            result: Err(io)
        }));
        assert_eq!(
            page.error.as_deref(),
            Some("Cannot write prompts/new.md: disk full")
        );
        let draft = page.draft.as_ref().unwrap();
        assert_eq!(draft.origin.as_deref(), Some("new"), "the file has its new name");
        assert_eq!(
            draft.write(),
            Some(Write::Save {
                name: "new".into(),
                text: "text!".into()
            }),
            "the retry only writes the text"
        );
    }

    #[test]
    fn a_successful_write_makes_the_draft_clean_and_clears_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut page = PromptsPage::default();
        page.start_new();
        page.error = Some("old problem".into());
        let draft = page.draft.as_mut().unwrap();
        draft.name.insert("fresh");
        draft.body.insert("# Fresh\nhello");
        let done = page.draft.as_ref().unwrap().write().unwrap().run(dir.path());
        assert!(page.written(done));
        assert!(!page.is_dirty());
        assert_eq!(page.error, None);
        assert_eq!(page.draft.as_ref().unwrap().origin.as_deref(), Some("fresh"));
        assert_eq!(
            page.draft.as_ref().unwrap().write(),
            None,
            "a second Save has nothing to write"
        );
    }

    #[test]
    fn neighbour_moves_through_the_prompts_and_stops_at_the_ends() {
        let mut page = page(&["a", "b", "c"]);
        assert_eq!(page.neighbour(1).as_deref(), Some("a"), "nothing open: the first");
        assert_eq!(page.neighbour(-1).as_deref(), Some("c"), "nothing open: the last");
        page.open("b", "x");
        assert_eq!(page.neighbour(1).as_deref(), Some("c"));
        assert_eq!(page.neighbour(-1).as_deref(), Some("a"));
        page.open("c", "x");
        assert_eq!(page.neighbour(1), None, "no wrap past the last");
        page.open("a", "x");
        assert_eq!(page.neighbour(-1), None, "no wrap before the first");
        assert_eq!(super::PromptsPage::default().neighbour(1), None, "empty library");
    }

    #[test]
    fn reload_drops_a_clean_draft_whose_file_is_gone_and_keeps_an_edited_one() {
        let mut page = page(&["a", "b"]);
        page.open("a", "x");
        page.reload(PromptLibrary {
            prompts: vec![prompt("b")],
            errors: Vec::new(),
        });
        assert!(page.draft.is_none());
        assert!(!page.stale);
        page.open("b", "x");
        page.draft.as_mut().unwrap().body.insert("!");
        page.reload(PromptLibrary::default());
        assert!(page.draft.is_some(), "the typed text outlives its file");
        // A new, empty draft has no file to lose.
        page.start_new();
        page.reload(PromptLibrary::default());
        assert!(page.draft.is_some());
    }

    #[test]
    fn a_clean_draft_follows_the_file_and_an_edited_one_does_not() {
        let mut page = PromptsPage::default();
        page.open("a", "old");
        page.refresh("a", "old");
        assert_eq!(page.draft.as_ref().unwrap().body.text(), "old");
        page.refresh("a", "changed in Zed");
        assert_eq!(page.draft.as_ref().unwrap().body.text(), "changed in Zed");
        assert!(!page.is_dirty());
        page.draft.as_mut().unwrap().body.insert("!");
        page.refresh("a", "changed again");
        assert_eq!(
            page.draft.as_ref().unwrap().body.text(),
            "changed in Zed!",
            "typed text is never replaced"
        );
        assert_eq!(page.clean_origin(), None);
    }
}
```

- [ ] **Step 2: Run to see it fail**

Add `pub mod prompts_page;` to `crates/mkrew-ui/src/lib.rs` after `pub mod prompt_picker;`.
Run: `cargo test -p mkrew-ui --lib prompts_page`
Expected: compile error, `PromptsPage`, `Draft`, `Write`, `Written`, `PromptLibrary`, `prompt` not found in the module.

- [ ] **Step 3: Implement**

Insert between the module doc and `#[cfg(test)]` of `prompts_page.rs`:

```rust
use std::path::Path;

use mkrew_core::prompt::{self, PromptError, PromptLibrary};

use crate::text_box::{Input, TextBox};

/// Where the keyboard is on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    /// The prompt list.
    List,
    /// The Name field of the draft.
    Name,
    /// The prompt text of the draft.
    Body,
}

/// What the shell does for the page after a key or click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    /// Edit the prompt of this name (asks first when the draft has unsaved changes).
    Open(String),
    /// Start a new prompt (asks first, like `Open`).
    New,
    /// Ctrl+S, Ctrl+Enter, the Save button.
    Save,
    /// Delete the open prompt (the shell asks first).
    Delete,
    OpenFolder,
}

/// The file work saving a draft takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Write {
    Create { name: String, text: String },
    Save { name: String, text: String },
    /// Rename first, then write `text` under the new name when it was edited too.
    Rename {
        old: String,
        new: String,
        text: Option<String>,
    },
}

/// What a [`Write`] did.
#[derive(Debug)]
pub struct Written {
    /// The file has its new name already (a later step may still have failed).
    pub renamed: bool,
    pub result: Result<(), PromptError>,
}

impl Write {
    /// Does the file work in `dir`. A rename that worked stays done when the text write after it
    /// fails, and [`Written::renamed`] says so.
    pub fn run(&self, dir: &Path) -> Written {
        match self {
            Write::Create { name, text } => Written {
                renamed: false,
                result: prompt::create(dir, name, text),
            },
            Write::Save { name, text } => Written {
                renamed: false,
                result: prompt::save(dir, name, text),
            },
            Write::Rename { old, new, text } => {
                if let Err(error) = prompt::rename(dir, old, new) {
                    return Written {
                        renamed: false,
                        result: Err(error),
                    };
                }
                let result = match text {
                    Some(text) => prompt::save(dir, new, text),
                    None => Ok(()),
                };
                Written {
                    renamed: true,
                    result,
                }
            }
        }
    }
}

/// The prompt being edited: one of the folder, or a new one.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// The name of the file this edits; `None` = a new prompt not written yet.
    pub origin: Option<String>,
    pub name: Input,
    /// The whole file, `# Title` line included.
    pub body: TextBox,
    /// Why the last Save refused the name, shown under the Name field.
    pub name_error: Option<String>,
    /// What the file held when it was read or last written.
    saved_name: String,
    saved_text: String,
}

impl Draft {
    pub fn new_prompt() -> Self {
        Self {
            origin: None,
            name: Input::default(),
            body: TextBox::default(),
            name_error: None,
            saved_name: String::new(),
            saved_text: String::new(),
        }
    }

    /// `text` is the whole file as `prompt::read` gives it.
    pub fn open(name: &str, text: &str) -> Self {
        let mut body = TextBox::default();
        body.insert(text);
        Self {
            origin: Some(name.to_owned()),
            name: Input::from(name),
            name_error: None,
            saved_name: name.to_owned(),
            saved_text: body.text().to_owned(),
            body,
        }
    }

    /// The name as it is written: what was typed, without the blanks around it.
    pub fn name(&self) -> &str {
        self.name.trim()
    }

    pub fn is_dirty(&self) -> bool {
        self.name() != self.saved_name || self.body.text() != self.saved_text
    }

    /// The file work a Save takes; `None` when nothing changed.
    pub fn write(&self) -> Option<Write> {
        if !self.is_dirty() {
            return None;
        }
        let name = self.name().to_owned();
        let text = self.body.text().to_owned();
        Some(match &self.origin {
            None => Write::Create { name, text },
            Some(old) if *old == name => Write::Save { name, text },
            Some(old) => Write::Rename {
                old: old.clone(),
                text: (text != self.saved_text).then_some(text),
                new: name,
            },
        })
    }

    /// The file has the name in the field now.
    fn renamed(&mut self) {
        let name = self.name().to_owned();
        self.origin = Some(name.clone());
        self.saved_name = name;
    }

    /// The file holds what the draft shows now.
    fn saved(&mut self) {
        self.renamed();
        let text = self.body.text().to_owned();
        self.saved_text = text;
        self.name_error = None;
    }
}

/// The page: the library as last read and the draft.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptsPage {
    pub library: PromptLibrary,
    pub draft: Option<Draft>,
    /// Why the last read, write or delete failed; shown at the top of the page.
    pub error: Option<String>,
    /// The folder has to be read again (the page opened, or something was written).
    pub stale: bool,
}

impl Default for PromptsPage {
    fn default() -> Self {
        Self {
            library: PromptLibrary::default(),
            draft: None,
            error: None,
            stale: true,
        }
    }
}

impl PromptsPage {
    pub fn is_dirty(&self) -> bool {
        self.draft.as_ref().is_some_and(Draft::is_dirty)
    }

    /// The name of the open prompt while the draft equals its file.
    pub fn clean_origin(&self) -> Option<&str> {
        (self.draft.as_ref())
            .filter(|draft| !draft.is_dirty())
            .and_then(|draft| draft.origin.as_deref())
    }

    /// "New prompt": an empty draft.
    pub fn start_new(&mut self) {
        self.draft = Some(Draft::new_prompt());
        self.error = None;
    }

    /// Edits the prompt `name` whose file holds `text`.
    pub fn open(&mut self, name: &str, text: &str) {
        self.draft = Some(Draft::open(name, text));
        self.error = None;
    }

    /// The file `name` was read again: a clean draft of it shows the new text; typed text is
    /// never replaced, and an unchanged text keeps the caret where it is.
    pub fn refresh(&mut self, name: &str, text: &str) {
        let fresh = Draft::open(name, text);
        let replace = (self.draft.as_ref()).is_some_and(|draft| {
            !draft.is_dirty()
                && draft.origin.as_deref() == Some(name)
                && draft.saved_text != fresh.saved_text
        });
        if replace {
            self.draft = Some(fresh);
        }
    }

    /// The folder was read. A clean draft whose file is gone closes; an edited one stays (its
    /// Save writes the file again).
    pub fn reload(&mut self, library: PromptLibrary) {
        self.library = library;
        self.stale = false;
        let gone = self.clean_origin().is_some_and(|name| {
            !self.library.prompts.iter().any(|prompt| prompt.name == name)
        });
        if gone {
            self.draft = None;
        }
    }

    /// The name of the prompt `by` rows from the open one (the first, or the last when going
    /// back, while none is open); `None` at the ends of the list.
    pub fn neighbour(&self, by: i32) -> Option<String> {
        let prompts = &self.library.prompts;
        let last = prompts.len().checked_sub(1)?;
        let open = self.draft.as_ref().and_then(|draft| draft.origin.as_deref());
        let at = open.and_then(|name| prompts.iter().position(|prompt| prompt.name == name));
        let step = usize::try_from(by.unsigned_abs()).unwrap_or(usize::MAX);
        let to = match at {
            None if by > 0 => 0,
            None => last,
            Some(at) if by < 0 => at.saturating_sub(step),
            Some(at) => at.saturating_add(step).min(last),
        };
        (prompts.get(to))
            .filter(|prompt| Some(prompt.name.as_str()) != open)
            .map(|prompt| prompt.name.clone())
    }

    /// Takes the result of a [`Write`]: the draft follows what happened to the file. `true` when
    /// the file now holds the draft. A refused name shows under the Name field, any other
    /// failure at the top of the page.
    pub fn written(&mut self, written: Written) -> bool {
        let Some(draft) = self.draft.as_mut() else {
            return false;
        };
        if written.renamed {
            draft.renamed();
        }
        match written.result {
            Ok(()) => {
                draft.saved();
                self.error = None;
                true
            }
            Err(PromptError::Name(error)) => {
                draft.name_error = Some(error.to_string());
                false
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }
}
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-ui --lib prompts_page`
Expected: 10 tests pass.

Run: `cargo clippy -p mkrew-ui --lib --tests -- -D warnings`
Expected: no warnings (the module is pub, so nothing is dead code).

- [ ] **Step 5: Commit**

```
git add crates/mkrew-ui/src/prompts_page.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): prompts page model with draft, dirty state and write choice"
```

---

### Task 24 (C3): Settings dialog `Tab::Prompts` and the shell actions behind it

The tab exists and is driven end to end (keys, effects, files, questions), but is not listed in the page list until Task C4 draws it, so no commit shows a blank page.

**Files:**
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (imports lines 5-16; `Tab` 18-40; `Focus` 87-96; `Effect` 106-122; `SettingsDialog` 160-178 and `new`; `rows` ~206; `set_tab` ~271; `step_focus` ~385; `chain_edge` ~405-419; `enter` ~429; `key` ~455-560; `typing` ~590; new methods after `close_add` ~634; new fn after `binding_error`; tests appended)
- Create: `crates/mkrew-ui/src/shell/prompts.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (add `mod prompts;` after `mod ports;`, about line 51)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (imports line 5; `settings_key_down` lines 59-66 and after the effect at line 81; `apply_settings_effect` lines 99-145)
- Modify: `crates/mkrew-ui/src/main.rs` (after `settings_scroll: ScrollHandle,` line 528)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (after `settings_scroll: ScrollHandle::new(),` line 316)
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs` (`open_prompts_dir`, lines 753-763)

**Interfaces:**
- Consumes: `prompts_page::{PromptAction, PromptsPage, Spot, Written, Draft}` (C2), `mkrew_core::prompt::{read, delete, load_or_seed}` (C1), `files_view::{LEAVE_ANSWERS, leave_choice, Leave}`, `task_dialog::{DELETE_ANSWERS, confirms_delete}`, `AfterSave` (main.rs).
- Produces:
```rust
// settings_dialog
pub enum Tab { Settings(Page), Keyboard, Machines, Prompts }
pub enum Focus { Tabs, Search, Row(usize), Add(Field), Prompts(Spot) }
pub enum Effect { /* existing */, Prompts(PromptAction) }
pub fn prompts_shortcut(keystroke: &Keystroke) -> Option<PromptAction>;      // Ctrl+N, Ctrl+S (Cmd on macOS)
impl SettingsDialog {
    pub prompts: PromptsPage,
    pub fn prompts_new(&mut self);
    pub fn prompts_open(&mut self, name: &str, text: &str);
    pub fn prompts_loaded(&mut self, library: PromptLibrary);
    pub fn prompts_deleted(&mut self);
    pub fn prompts_written(&mut self, written: Written) -> bool;
}
// Shell (shell/prompts.rs)
pub(crate) fn open_prompts_settings(&mut self, window: &mut Window, cx: &mut Context<'_, Self>);
pub(crate) fn leave_prompt_draft(&mut self, then: AfterSave, window: &mut Window, cx: &mut Context<'_, Self>);
pub(crate) fn sync_prompts(&mut self);
// Shell fields: prompts_scroll: ScrollHandle, prompts_body_scroll: ScrollHandle
// Shell (workspace_actions.rs): pub(crate) fn open_prompts_folder(&mut self, cx: &mut Context<'_, Self>)
```

- [ ] **Step 1: Write the failing tests**

In `settings_dialog.rs` `mod tests`, add to the imports at the top of the module:

```rust
    use crate::prompts_page::{PromptAction, Spot, Written};
    use mkrew_core::prompt::{NameError, Prompt, PromptError, PromptLibrary};
```

Append these tests at the end of `mod tests` (they reuse `dialog()`, `key()`, `typed()` and `type_text()` of the module):

```rust
    fn library(names: &[&str]) -> PromptLibrary {
        PromptLibrary {
            prompts: names
                .iter()
                .map(|name| Prompt {
                    name: (*name).into(),
                    title: (*name).into(),
                    body: String::new(),
                })
                .collect(),
            errors: Vec::new(),
        }
    }

    #[test]
    fn the_prompts_tab_asks_for_a_read_when_it_opens_and_has_no_setting_rows() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.prompts.stale = false;
        d.set_tab(Tab::Prompts);
        assert!(d.prompts.stale, "the folder is read again when the page opens");
        assert!(d.rows(&keymap).is_empty());
        assert_eq!(Tab::Prompts.title(), "Prompts");
    }

    #[test]
    fn ctrl_n_and_ctrl_s_reach_the_shell_on_the_prompts_page_only() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        assert_eq!(d.key(&key("ctrl-s"), None, &keymap), Effect::None, "General page");
        d.set_tab(Tab::Prompts);
        assert_eq!(
            d.key(&key("ctrl-n"), None, &keymap),
            Effect::Prompts(PromptAction::New)
        );
        assert_eq!(
            d.key(&key("ctrl-s"), None, &keymap),
            Effect::Prompts(PromptAction::Save)
        );
        assert_eq!(d.key(&key("ctrl-shift-n"), None, &keymap), Effect::None);
        assert_eq!(d.key(&key("alt-s"), None, &keymap), Effect::None);
        assert_eq!(prompts_shortcut(&key("ctrl-n")), Some(PromptAction::New));
        assert_eq!(prompts_shortcut(&key("n")), None);
    }

    #[test]
    fn tab_walks_the_prompts_list_name_and_text_and_then_leaves() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Prompts);
        d.prompts_loaded(library(&["a", "b"]));
        d.key(&key("tab"), None, &keymap);
        assert_eq!(d.focus, Focus::Prompts(Spot::List));
        assert!(
            d.chain_edge(false, &keymap),
            "no prompt open: the list is the last stop"
        );
        d.prompts_new();
        assert_eq!(d.focus, Focus::Prompts(Spot::Name));
        assert!(!d.chain_edge(false, &keymap));
        d.key(&key("tab"), None, &keymap);
        assert_eq!(d.focus, Focus::Prompts(Spot::Body));
        assert!(d.chain_edge(false, &keymap));
        d.enter(false, &keymap);
        assert_eq!(d.focus, Focus::Prompts(Spot::Body), "Shift+Tab from the buttons");
        d.enter(true, &keymap);
        assert_eq!(d.focus, Focus::Tabs, "Tab from the last button");
    }

    #[test]
    fn arrows_in_the_list_ask_for_the_neighbour_and_stop_at_the_ends() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Prompts);
        d.prompts_loaded(library(&["a", "b", "c"]));
        d.prompts_open("b", "text");
        d.focus = Focus::Prompts(Spot::List);
        assert_eq!(
            d.key(&key("down"), None, &keymap),
            Effect::Prompts(PromptAction::Open("c".into()))
        );
        assert_eq!(
            d.key(&key("up"), None, &keymap),
            Effect::Prompts(PromptAction::Open("a".into()))
        );
        d.prompts_open("c", "text");
        assert_eq!(d.key(&key("down"), None, &keymap), Effect::None);
        d.key(&key("enter"), None, &keymap);
        assert_eq!(d.focus, Focus::Prompts(Spot::Name), "Enter on the list goes to the editor");
    }

    #[test]
    fn typing_goes_to_the_focused_field_spaces_included_and_enter_moves_on() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Prompts);
        d.prompts_new();
        type_text(&mut d, "my review", &keymap);
        assert_eq!(d.prompts.draft.as_ref().unwrap().name(), "my review");
        d.key(&key("enter"), None, &keymap);
        assert_eq!(d.focus, Focus::Prompts(Spot::Body));
        type_text(&mut d, "a", &keymap);
        d.key(&key("enter"), None, &keymap);
        type_text(&mut d, "b c", &keymap);
        assert_eq!(d.prompts.draft.as_ref().unwrap().body.text(), "a\nb c");
        assert_eq!(
            d.key(&key("ctrl-enter"), None, &keymap),
            Effect::Prompts(PromptAction::Save)
        );
        assert_eq!(d.key(&key("escape"), None, &keymap), Effect::Close);
    }

    #[test]
    fn a_pasted_name_keeps_its_first_line_and_a_pasted_text_keeps_every_line() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Prompts);
        d.prompts_new();
        d.key(&key("ctrl-v"), Some("first\nsecond"), &keymap);
        assert_eq!(d.prompts.draft.as_ref().unwrap().name(), "first");
        d.key(&key("tab"), None, &keymap);
        d.key(&key("ctrl-v"), Some("one\r\ntwo"), &keymap);
        assert_eq!(d.prompts.draft.as_ref().unwrap().body.text(), "one\ntwo");
    }

    #[test]
    fn editing_the_name_clears_its_refusal_and_typing_names_the_focused_text() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Prompts);
        d.prompts_new();
        d.prompts.draft.as_mut().unwrap().name_error = Some("A name cannot start with a dot.".into());
        type_text(&mut d, "x", &keymap);
        assert_eq!(d.prompts.draft.as_ref().unwrap().name_error, None);
        assert_eq!(d.typing().map(|input| input.as_str()), Some("x"));
        d.key(&key("tab"), None, &keymap);
        assert_eq!(d.typing().map(|input| input.as_str()), Some(""));
        d.focus = Focus::Prompts(Spot::List);
        assert!(d.typing().is_none());
    }

    #[test]
    fn a_file_deleted_outside_closes_a_clean_draft_and_hands_the_keyboard_to_the_list() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Prompts);
        d.prompts_loaded(library(&["a", "b"]));
        d.prompts_open("a", "x");
        d.focus = Focus::Prompts(Spot::Body);
        d.prompts_loaded(library(&["b"]));
        assert!(d.prompts.draft.is_none());
        assert_eq!(d.focus, Focus::Prompts(Spot::List));
        // Typed text outlives its file.
        d.prompts_open("b", "x");
        d.focus = Focus::Prompts(Spot::Body);
        d.key(&typed("!"), None, &keymap);
        d.prompts_loaded(library(&[]));
        assert!(d.prompts.draft.is_some());
        assert_eq!(d.focus, Focus::Prompts(Spot::Body));
        d.prompts_deleted();
        assert_eq!(d.focus, Focus::Prompts(Spot::List));
    }

    #[test]
    fn a_refused_name_puts_the_keyboard_on_the_name_field() {
        let mut d = dialog();
        d.set_tab(Tab::Prompts);
        d.prompts_new();
        d.focus = Focus::Prompts(Spot::Body);
        let written = Written {
            renamed: false,
            result: Err(PromptError::Name(NameError::Empty)),
        };
        assert!(!d.prompts_written(written));
        assert_eq!(d.focus, Focus::Prompts(Spot::Name));
        assert_eq!(
            d.prompts.draft.as_ref().unwrap().name_error.as_deref(),
            Some("Give the prompt a name.")
        );
    }

    #[test]
    fn switching_tabs_keeps_an_edited_draft() {
        let keymap = Keymap::defaults();
        let mut d = dialog();
        d.set_tab(Tab::Prompts);
        d.prompts_new();
        type_text(&mut d, "idea", &keymap);
        d.set_tab(Tab::Keyboard);
        d.set_tab(Tab::Prompts);
        assert_eq!(d.prompts.draft.as_ref().unwrap().name(), "idea");
        assert!(d.prompts.is_dirty());
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui --lib settings_dialog`
Expected: compile error, `Tab::Prompts`, `Focus::Prompts`, `Effect::Prompts`, `prompts_shortcut`, `SettingsDialog::prompts`, `prompts_new` … not found.

- [ ] **Step 3: Implement the model (`settings_dialog.rs`)**

3a. Imports. After `use mkrew_core::settings::Settings;` add `use mkrew_core::prompt::PromptLibrary;` (keep the group sorted: it goes before `mkrew_core::settings`). After `use crate::palette;` add:

```rust
use crate::prompts_page::{PromptAction, PromptsPage, Spot, Written};
```

3b. `Tab` (lines 18-40). Replace the enum and `title`; leave `all()` as it is (Task C4 lists the page):

```rust
pub enum Tab {
    Settings(Page),
    Keyboard,
    Machines,
    Prompts,
}
```

and in `title()` add after `Tab::Machines => "Machines",`:

```rust
            Tab::Prompts => "Prompts",
```

3c. `Focus` (line 87). Add after `Add(Field),`:

```rust
    /// A spot of the Prompts page.
    Prompts(Spot),
```

3d. `Effect` (line 106). Add after `AddMachine(Machine),`:

```rust
    /// A Prompts page action the shell runs: file reads and writes, the questions.
    Prompts(PromptAction),
```

3e. `SettingsDialog` struct (line 160): add the field after `pub add: Option<AddForm>,`:

```rust
    /// The Prompts page (spec 2026-11-04 section 7.2); its draft outlives a switch of page.
    pub prompts: PromptsPage,
```

and in `new` after `add: None,`: `prompts: PromptsPage::default(),`.

3f. `rows` (line 207): change `Tab::Machines => Vec::new(),` to `Tab::Machines | Tab::Prompts => Vec::new(),`.

3g. `set_tab`: after `self.tab = tab;` add:

```rust
        if tab == Tab::Prompts {
            // The folder is read when the page opens (spec 2026-11-04 section 7.2).
            self.prompts.stale = true;
        }
```

3h. `step_focus`: after the `if self.tab == Tab::Keyboard { order.push(Focus::Search); }` block add:

```rust
        if self.tab == Tab::Prompts {
            order.push(Focus::Prompts(Spot::List));
            if self.prompts.draft.is_some() {
                order.extend([Spot::Name, Spot::Body].map(Focus::Prompts));
            }
        }
```

3i. `chain_edge`: replace the `match self.focus { ... }` arms so they read:

```rust
        match self.focus {
            Focus::Row(i) => i + 1 >= rows,
            Focus::Search => rows == 0,
            Focus::Add(field) => field == Field::Mk,
            Focus::Prompts(spot) => spot == self.prompts_last_spot(),
            Focus::Tabs => {
                self.tab != Tab::Keyboard
                    && self.tab != Tab::Prompts
                    && rows == 0
                    && self.add.is_none()
            }
        }
```

3j. `enter`: in `let last = match self.rows(keymap).len() {` add an arm before `0 => Focus::Tabs,`:

```rust
            0 if self.tab == Tab::Prompts => Focus::Prompts(self.prompts_last_spot()),
```

3k. `key`. After the `if let Some(command) = self.capture.take() { ... }` block and before `let Some(key) = dialog_key(keystroke) else {`, insert:

```rust
        // Ctrl+N and Ctrl+S are not text keys, so `dialog_key` has no meaning for them.
        if self.tab == Tab::Prompts
            && let Some(action) = prompts_shortcut(keystroke)
        {
            return Effect::Prompts(action);
        }
```

After `if self.list_pick.is_some() { return self.list_key(key); }` insert:

```rust
        if self.tab == Tab::Prompts
            && let Focus::Prompts(spot) = self.focus
            && let Some(effect) = self.prompts_key(spot, &key, paste)
        {
            return effect;
        }
```

In the Up/Down arm of `match key`, change `Focus::Search | Focus::Add(_) => Effect::None,` to `Focus::Search | Focus::Add(_) | Focus::Prompts(_) => Effect::None,`.

3l. `typing`: in the `if self.capture.is_some() { None } else if ...` chain add, before the final `else`:

```rust
        } else if let Focus::Prompts(spot) = self.focus {
            let draft = self.prompts.draft.as_ref();
            match spot {
                Spot::Name => draft.map(|draft| &draft.name),
                Spot::Body => draft.map(|draft| draft.body.input()),
                Spot::List => None,
            }
```

3m. New methods, after `close_add`:

```rust
    /// "New prompt": an empty draft with the keyboard on its Name.
    pub fn prompts_new(&mut self) {
        self.prompts.start_new();
        self.focus = Focus::Prompts(Spot::Name);
    }

    /// The file of `name` was read: it is the draft now (the keyboard stays where it is).
    pub fn prompts_open(&mut self, name: &str, text: &str) {
        self.prompts.open(name, text);
    }

    /// The folder was read.
    pub fn prompts_loaded(&mut self, library: PromptLibrary) {
        self.prompts.reload(library);
        self.settle_prompts_focus();
    }

    /// The open prompt was deleted.
    pub fn prompts_deleted(&mut self) {
        self.prompts.draft = None;
        self.settle_prompts_focus();
    }

    /// A write ran; `true` when the file holds the draft now. A refused name takes the keyboard.
    pub fn prompts_written(&mut self, written: Written) -> bool {
        let saved = self.prompts.written(written);
        if (self.prompts.draft.as_ref()).is_some_and(|draft| draft.name_error.is_some()) {
            self.focus = Focus::Prompts(Spot::Name);
        }
        saved
    }

    /// The editor spots need a draft: without one the keyboard goes back to the list.
    fn settle_prompts_focus(&mut self) {
        if matches!(self.focus, Focus::Prompts(Spot::Name | Spot::Body))
            && self.prompts.draft.is_none()
        {
            self.focus = Focus::Prompts(Spot::List);
        }
    }

    /// The last keyboard stop of the Prompts page before its buttons.
    fn prompts_last_spot(&self) -> Spot {
        if self.prompts.draft.is_some() {
            Spot::Body
        } else {
            Spot::List
        }
    }

    fn prompts_step(&self, by: i32) -> Effect {
        match self.prompts.neighbour(by) {
            Some(name) => Effect::Prompts(PromptAction::Open(name)),
            None => Effect::None,
        }
    }

    /// A key on the Prompts page's list, Name field or text. `None` = the generic handling
    /// applies (Esc closes, Tab moves).
    fn prompts_key(&mut self, spot: Spot, key: &DialogKey, paste: Option<&str>) -> Option<Effect> {
        match (spot, key) {
            (_, DialogKey::Cancel | DialogKey::SwitchField) => None,
            (_, DialogKey::Save) => Some(Effect::Prompts(PromptAction::Save)),
            (
                Spot::List,
                DialogKey::Move {
                    motion: Motion::Up,
                    extend: false,
                },
            ) => Some(self.prompts_step(-1)),
            (
                Spot::List,
                DialogKey::Move {
                    motion: Motion::Down,
                    extend: false,
                },
            ) => Some(self.prompts_step(1)),
            (Spot::List, DialogKey::Enter) => {
                if self.prompts.draft.is_some() {
                    self.focus = Focus::Prompts(Spot::Name);
                }
                Some(Effect::None)
            }
            (Spot::List, _) => Some(Effect::None),
            (Spot::Name, DialogKey::Enter) => {
                self.focus = Focus::Prompts(Spot::Body);
                Some(Effect::None)
            }
            (
                Spot::Name,
                DialogKey::Move {
                    motion: Motion::Up | Motion::Down,
                    ..
                },
            ) => Some(Effect::None),
            (Spot::Name, key) => {
                if let Some(draft) = self.prompts.draft.as_mut() {
                    // What was refused is being fixed.
                    draft.name_error = None;
                    match key {
                        // A name is one line.
                        DialogKey::Paste => {
                            let line = paste.and_then(|text| text.lines().next());
                            draft.name.insert(line.unwrap_or_default());
                        }
                        DialogKey::Insert(text) => {
                            draft.name.insert(text);
                        }
                        other => {
                            draft.name.edit(other);
                        }
                    }
                }
                Some(Effect::None)
            }
            (Spot::Body, key) => {
                if let Some(draft) = self.prompts.draft.as_mut() {
                    match key {
                        DialogKey::Paste => draft.body.insert(paste.unwrap_or_default()),
                        other => {
                            draft.body.key(other.clone());
                        }
                    }
                }
                Some(Effect::None)
            }
        }
    }
```

3n. After `binding_error` (end of the non-test code) add:

```rust
/// Ctrl+N (new prompt) and Ctrl+S (save) of the Prompts page; Cmd on macOS. They are page keys
/// of the dialog, not commands of `keymap.toml`.
pub fn prompts_shortcut(keystroke: &Keystroke) -> Option<PromptAction> {
    let m = keystroke.modifiers;
    if !(m.control || m.platform) || m.alt || m.shift {
        return None;
    }
    match keystroke.key.as_str() {
        "n" => Some(PromptAction::New),
        "s" => Some(PromptAction::Save),
        _ => None,
    }
}
```

- [ ] **Step 4: Run the model tests**

Run: `cargo test -p mkrew-ui --lib settings_dialog`
Expected: all `settings_dialog` tests pass, the new ones included.

- [ ] **Step 5: Shell: state fields, folder helper**

`crates/mkrew-ui/src/main.rs`, after `settings_scroll: ScrollHandle,` (line 528):

```rust
    /// Keeps the open prompt's row in view in Settings → Prompts.
    prompts_scroll: ScrollHandle,
    /// Scrolls the prompt text of Settings → Prompts (not `prompt_scroll`: the task dialog can
    /// be open under Settings).
    prompts_body_scroll: ScrollHandle,
```

`crates/mkrew-ui/src/shell/connection.rs`, after `settings_scroll: ScrollHandle::new(),` (line 316):

```rust
            prompts_scroll: ScrollHandle::new(),
            prompts_body_scroll: ScrollHandle::new(),
```

`crates/mkrew-ui/src/shell.rs`: add `mod prompts;` after `mod ports;`.

`crates/mkrew-ui/src/shell/workspace_actions.rs`, replace `open_prompts_dir` (doc comment and fn, lines 752-763) with:

```rust
    /// Opens the prompts folder (created if missing) in Zed; a failure toasts.
    pub(crate) fn open_prompts_folder(&mut self, cx: &mut Context<'_, Self>) {
        let opened = std::fs::create_dir_all(&self.prompts_dir)
            .map_err(|error| format!("Cannot create the prompts folder: {error}"))
            .and_then(|()| zed::open(&self.prompts_dir, None).map_err(|error| error.to_string()));
        if let Err(error) = opened {
            tracing::warn!(path = %self.prompts_dir.display(), "open prompts folder failed: {error}");
            self.toast_error(error, cx);
        }
    }

    /// The prompt picker's empty state: opens the prompts folder in Zed to add Markdown prompts.
    pub(crate) fn open_prompts_dir(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.open_prompts_folder(cx);
        self.close_prompt_picker(None, window, cx);
    }
```

- [ ] **Step 6: Shell: `shell/prompts.rs`**

Create `crates/mkrew-ui/src/shell/prompts.rs`:

```rust
//! Settings → Prompts (SPEC §14, §20B.3): reads and writes `<config>/mkrew/prompts` through
//! `mkrew_core::prompt` for the page model (`mkrew_ui::prompts_page`, driven by the dialog's
//! `Tab::Prompts`) and asks before unsaved text is left. The page is drawn in `prompts_render`.

use crate::*;
use mkrew_core::prompt::{self, PromptError};
use mkrew_ui::prompts_page::{Draft, PromptAction};
use mkrew_ui::settings_dialog::Tab;

impl Shell {
    /// The palette's "Global: Manage prompts…" and the prompt picker's last row: Settings on the
    /// Prompts page (an open Settings dialog keeps its state, an edited draft included).
    pub(crate) fn open_prompts_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.settings_dialog.is_none() {
            self.open_settings_dialog(window, cx);
        }
        let effect = (self.settings_dialog.as_mut()).map(|dialog| dialog.set_tab(Tab::Prompts));
        window.focus(&self.dialog_focus, cx);
        if let Some(effect) = effect {
            self.apply_settings_effect(effect, window, cx);
        }
    }

    /// Runs what the page asked for.
    pub(crate) fn prompts_action(
        &mut self,
        action: PromptAction,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match action {
            PromptAction::Open(name) => {
                let then: AfterSave = Box::new(move |shell, _, _| shell.open_prompt_now(&name));
                self.leave_prompt_draft(then, window, cx);
            }
            PromptAction::New => {
                let then: AfterSave = Box::new(|shell, _, _| {
                    if let Some(dialog) = shell.settings_dialog.as_mut() {
                        dialog.prompts_new();
                    }
                });
                self.leave_prompt_draft(then, window, cx);
            }
            PromptAction::Save => self.save_prompt(None, window, cx),
            PromptAction::Delete => self.ask_delete_prompt(window, cx),
            PromptAction::OpenFolder => self.open_prompts_folder(cx),
        }
    }

    /// Reads the folder when the page asks for it (it opened, or something was written): the
    /// list, and the text of a clean open prompt (an edit made in Zed shows; typed text is never
    /// replaced). The first read seeds the three samples, like the prompt picker.
    pub(crate) fn sync_prompts(&mut self) {
        let stale = (self.settings_dialog.as_ref())
            .is_some_and(|dialog| dialog.tab == Tab::Prompts && dialog.prompts.stale);
        if !stale {
            return;
        }
        let library = prompt::load_or_seed(&self.prompts_dir);
        let open = (self.settings_dialog.as_ref())
            .and_then(|dialog| dialog.prompts.clean_origin().map(str::to_owned));
        let text = open.map(|name| {
            let read = prompt::read(&self.prompts_dir, &name);
            (name, read)
        });
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        dialog.prompts_loaded(library);
        if let Some((name, Ok(text))) = text {
            dialog.prompts.refresh(&name, &text);
        }
    }

    /// Edits the prompt `name`: its file is read now.
    fn open_prompt_now(&mut self, name: &str) {
        let read = prompt::read(&self.prompts_dir, name);
        let Some(dialog) = self.settings_dialog.as_mut() else {
            return;
        };
        match read {
            Ok(text) => dialog.prompts_open(name, &text),
            Err(error) => {
                dialog.prompts.error = Some(error.to_string());
                // The folder changed under the list.
                dialog.prompts.stale = true;
            }
        }
        let at = dialog.prompts.library.prompts.iter().position(|p| p.name == name);
        if let Some(at) = at {
            self.prompts_scroll.scroll_to_item(at);
        }
    }

    /// Runs `then` once the unsaved changes of the open prompt are dealt with: at once when
    /// there are none; after "Save" and a successful write, or "Don't save". "Cancel" (or Esc)
    /// stops: `then` does not run.
    pub(crate) fn leave_prompt_draft(
        &mut self,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let label = (self.settings_dialog.as_ref())
            .and_then(|dialog| dialog.prompts.draft.as_ref())
            .filter(|draft| draft.is_dirty())
            .map(|draft| match draft.name() {
                "" => "the new prompt".to_owned(),
                name => format!("“{name}”"),
            });
        let Some(label) = label else {
            return then(self, window, cx);
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to {label}?"),
            Some("Your changes are lost if you don't save them."),
            &files_view::LEAVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = files_view::leave_choice(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| {
                match choice {
                    Some(files_view::Leave::Save) => shell.save_prompt(Some(then), window, cx),
                    Some(files_view::Leave::DontSave) => then(shell, window, cx),
                    None => window.focus(&shell.dialog_focus, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Ctrl+S, the Save button and "Save" of the question: writes the draft (nothing happens
    /// when it is clean), reads the folder again, then runs `then`. A refused name or a failed
    /// write shows on the page and `then` does not run.
    pub(crate) fn save_prompt(
        &mut self,
        then: Option<AfterSave>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let write = (self.settings_dialog.as_ref())
            .and_then(|dialog| dialog.prompts.draft.as_ref())
            .and_then(Draft::write);
        let Some(write) = write else {
            if let Some(then) = then {
                then(self, window, cx);
            }
            return;
        };
        let written = write.run(&self.prompts_dir);
        let saved = (self.settings_dialog.as_mut())
            .is_some_and(|dialog| dialog.prompts_written(written));
        if let Some(dialog) = self.settings_dialog.as_mut() {
            dialog.prompts.stale = true;
        }
        self.sync_prompts();
        if saved && let Some(then) = then {
            then(self, window, cx);
        }
        cx.notify();
    }

    /// Delete: asks, then removes the open prompt's file.
    fn ask_delete_prompt(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let name = (self.settings_dialog.as_ref())
            .and_then(|dialog| dialog.prompts.draft.as_ref())
            .and_then(|draft| draft.origin.clone());
        let Some(name) = name else {
            return;
        };
        let answer = window.prompt(
            PromptLevel::Critical,
            &format!("Delete “{name}”?"),
            Some("Its file is deleted from the prompts folder. This cannot be undone."),
            &task_dialog::DELETE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let confirmed = task_dialog::confirms_delete(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| {
                if confirmed {
                    shell.delete_prompt(&name);
                }
                window.focus(&shell.dialog_focus, cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn delete_prompt(&mut self, name: &str) {
        let result = prompt::delete(&self.prompts_dir, name);
        if let Some(dialog) = self.settings_dialog.as_mut() {
            match result {
                // A prompt already gone is what Delete wanted.
                Ok(()) | Err(PromptError::Missing(_)) => {
                    dialog.prompts_deleted();
                    dialog.prompts.error = None;
                }
                Err(error) => dialog.prompts.error = Some(error.to_string()),
            }
            dialog.prompts.stale = true;
        }
        self.sync_prompts();
    }
}
```

- [ ] **Step 7: Shell: `settings.rs` wiring**

Imports (line 5): `use mkrew_ui::settings_dialog::{Effect, Focus, Row, Tab};` becomes

```rust
use mkrew_ui::prompts_page::Spot;
use mkrew_ui::settings_dialog::{Effect, Focus, Row, Tab};
```

In `settings_key_down`, replace

```rust
        let key = task_dialog::dialog_key(&event.keystroke);
        // A button or link has the keyboard: the rows take nothing but Esc / Ctrl+Enter.
        if self.dialog_control_focused(window)
            && !matches!(key, Some(DialogKey::Cancel | DialogKey::Save))
        {
            return;
        }
```

with

```rust
        let key = task_dialog::dialog_key(&event.keystroke);
        // Ctrl+N and Ctrl+S of the Prompts page work from a button too.
        let page_key = settings_dialog::prompts_shortcut(&event.keystroke).is_some();
        // A button or link has the keyboard: the rows take nothing but Esc / Ctrl+Enter.
        if self.dialog_control_focused(window)
            && !page_key
            && !matches!(key, Some(DialogKey::Cancel | DialogKey::Save))
        {
            return;
        }
```

Right after `self.apply_settings_effect(effect, window, cx);` in `settings_key_down` (the one before "Keep the focused row on screen") add:

```rust
        // Typing at the end of a long prompt keeps its last line in view, like the task dialog.
        if let Some(dialog) = &self.settings_dialog
            && dialog.focus == Focus::Prompts(Spot::Body)
            && let Some(draft) = &dialog.prompts.draft
            && draft.body.input().cursor() == draft.body.input().len()
        {
            self.prompts_body_scroll.scroll_to_bottom();
        }
```

In `apply_settings_effect` replace `Effect::Close => return self.close_settings_dialog(window, cx),` with

```rust
            // Esc: unsaved prompt text asks first (Save / Don't save / Cancel).
            Effect::Close => {
                let then: AfterSave =
                    Box::new(|shell, window, cx| shell.close_settings_dialog(window, cx));
                return self.leave_prompt_draft(then, window, cx);
            }
```

and after `Effect::AddMachine(machine) => self.add_machine(machine, cx),` add

```rust
            Effect::Prompts(action) => self.prompts_action(action, window, cx),
```

Then before the final `cx.notify();` of `apply_settings_effect` add `self.sync_prompts();`:

```rust
        }
        // The Prompts page reads its folder when it opens and after every write.
        self.sync_prompts();
        cx.notify();
    }
```

(`Spot`, `Draft` and `PromptAction` are re-used by `shell/prompts_render.rs` in Task C4; the unused-import lint does not fire because `Spot` is used in `settings_key_down`.)

- [ ] **Step 8: Build, lint, run**

Run: `cargo test -p mkrew-ui --lib settings_dialog prompts_page`
Expected: pass.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings (`PromptAction` is `pub` in a `pub mod`, so its variants are not flagged even though only `prompts_action` and the Task C4 buttons build them).

- [ ] **Step 9: Commit**

```
git add crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/shell/prompts.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/settings.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/workspace_actions.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Prompts settings tab model and its shell actions"
```

---

### Task 25 (C4): Settings → Prompts page: drawing, tab listing, tokens, SPEC §14 and §13

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (after `SETTINGS_FONT_LIST_HEIGHT`, line 304)
- Modify: `crates/mkrew-ui/src/settings_dialog.rs` (`Tab::all`, lines 25-31; one test)
- Create: `crates/mkrew-ui/src/shell/prompts_render.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (add `mod prompts_render;` after `mod prompts;`)
- Modify: `crates/mkrew-ui/src/shell/machines.rs` (`machines_error` renamed to `error_line` and made `pub(crate)`; 3 places)
- Modify: `crates/mkrew-ui/src/shell/settings.rs` (body children, footer text)
- Modify: `docs/SPEC.md` (§14 Settings paragraph at line 1650 and a new bullet after line 1664; §13 key table after line 1410)

**Interfaces:**
- Consumes: `SettingsDialog.prompts`, `Focus::Prompts`, `Effect::Prompts`, `PromptAction` (C3); widgets `button`, `secondary_button`, `danger_button`, `disabled_button`, `caret_field`, `meta_line`, `tooltip`, `DialogControl::{focus_stop, focusable_bordered}`.
- Produces: `Shell::render_prompts_page(&self, dialog: &SettingsDialog, typing: bool, theme: &Theme, cx) -> Vec<gpui::AnyElement>`; tokens `theme::SETTINGS_PROMPTS_LIST_WIDTH`, `theme::SETTINGS_PROMPTS_LIST_HEIGHT`, `theme::SETTINGS_PROMPT_BODY_HEIGHT`; `Shell::error_line`.

The drawing has no unit test (GPUI render code; this repo tests the models, see ADR 0085 and `settings_dialog` tests). The model behavior it shows is covered in C2 and C3; the look is a HUMAN_CHECKS entry (Task C5).

- [ ] **Step 1: Write the failing test**

In `settings_dialog.rs` `mod tests`:

```rust
    #[test]
    fn the_page_list_has_prompts_before_machines_and_up_down_reaches_it() {
        let keymap = Keymap::defaults();
        let all = Tab::all();
        let prompts = all.iter().position(|t| *t == Tab::Prompts).unwrap();
        let machines = all.iter().position(|t| *t == Tab::Machines).unwrap();
        assert_eq!(prompts + 1, machines);
        let mut d = dialog();
        d.set_tab(Tab::Keyboard);
        d.key(&key("down"), None, &keymap);
        assert_eq!(d.tab, Tab::Prompts, "Down from Keyboard");
    }
```

Note: `set_tab` resets focus to `Focus::Tabs`, which is where Down moves between pages.

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui --lib the_page_list_has_prompts`
Expected: FAIL, `position(...).unwrap()` panics (`Tab::all()` has no `Prompts`).

- [ ] **Step 3: Implement**

3a. `settings_dialog.rs` `Tab::all()`:

```rust
    pub fn all() -> Vec<Tab> {
        let mut all: Vec<Tab> = Page::ALL.into_iter().map(Tab::Settings).collect();
        all.push(Tab::Keyboard);
        all.push(Tab::Prompts);
        all.push(Tab::Machines);
        all
    }
```

3b. `theme.rs`, after `pub const SETTINGS_FONT_LIST_HEIGHT: f32 = 200.0;`:

```rust
/// Settings → Prompts (spec 2026-11-04 section 7.2): the prompt list's width, how tall it grows
/// before it scrolls, and the height of the prompt text field (it scrolls past that).
pub const SETTINGS_PROMPTS_LIST_WIDTH: f32 = 200.0;
pub const SETTINGS_PROMPTS_LIST_HEIGHT: f32 = 360.0;
pub const SETTINGS_PROMPT_BODY_HEIGHT: f32 = 240.0;
```

3c. `shell.rs`: add `mod prompts_render;` after `mod prompts;`.

3d. `shell/machines.rs`: rename the method so the Prompts page can use the same line. Replace its definition

```rust
    /// The broken-file line: its text names the file and the line; nothing can be changed.
    fn machines_error(&self, text: String, theme: &Theme) -> gpui::AnyElement {
```

with

```rust
    /// A page's error line (the Machines page's broken file, the Prompts page's failed write): a
    /// red ✕ and the text on a raised row.
    pub(crate) fn error_line(&self, text: String, theme: &Theme) -> gpui::AnyElement {
```

and its two callers: `self.machines_error(text.clone(), theme)` becomes `self.error_line(text.clone(), theme)`, `self.machines_error(error.clone(), theme)` becomes `self.error_line(error.clone(), theme)`.
Check: `rg machines_error crates` finds nothing.

3e. `shell/settings.rs`, in `render_settings_dialog` `body`: after the Machines `.children(...)` block add

```rust
            .children(
                (dialog.tab == Tab::Prompts)
                    .then(|| self.render_prompts_page(dialog, typing, theme, cx))
                    .into_iter()
                    .flatten(),
            )
```

and change the footer's right-hand text from `.child("Changes apply at once · Esc closes"),` to

```rust
                    .child(if dialog.tab == Tab::Prompts {
                        "Ctrl+N new · Ctrl+S saves · Esc closes"
                    } else {
                        "Changes apply at once · Esc closes"
                    }),
```

3f. Create `crates/mkrew-ui/src/shell/prompts_render.rs`:

```rust
//! The body of Settings → Prompts (spec 2026-11-04 section 7.2): the prompt list on the left,
//! the editor on the right. The model is `mkrew_ui::prompts_page`; the actions are in
//! `prompts.rs`.

use crate::*;
use mkrew_ui::prompts_page::{PromptAction, Spot};
use mkrew_ui::settings_dialog::{Effect, Focus};

impl Shell {
    /// The page: a note in a machine window, the page error, then the list and the editor.
    pub(crate) fn render_prompts_page(
        &self,
        dialog: &SettingsDialog,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut page: Vec<gpui::AnyElement> = Vec::new();
        // The picker reads this computer's folder, so a machine window edits it too.
        if self.target.is_machine() {
            page.push(
                meta_line(
                    theme,
                    "These prompts are on this computer; every Mkrew window uses them.".into(),
                )
                .into_any_element(),
            );
        }
        if let Some(error) = &dialog.prompts.error {
            page.push(self.error_line(error.clone(), theme));
        }
        page.push(
            div()
                .flex()
                .items_start()
                .gap(px(space::M))
                .child(self.prompts_list(dialog, theme, cx))
                .child(self.prompt_editor(dialog, typing, theme, cx))
                .into_any_element(),
        );
        page
    }

    /// "New prompt" and one row per prompt: its title (the `# Title` line, else the name) and,
    /// when different, its name; a file that cannot be read shows its error in its own row.
    fn prompts_list(
        &self,
        dialog: &SettingsDialog,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let library = &dialog.prompts.library;
        let selected = dialog
            .prompts
            .draft
            .as_ref()
            .and_then(|draft| draft.origin.as_deref());
        let on_list = dialog.focus == Focus::Prompts(Spot::List);
        let empty = library.prompts.is_empty() && library.errors.is_empty();

        // The filled button while the library is empty, like Machines' first "Add machine…".
        let new_button = if empty {
            button("prompt-new", "New prompt", theme, true).focus_stop(theme)
        } else {
            secondary_button("prompt-new", "New prompt", theme).focusable_bordered(theme)
        }
        .tooltip(tooltip(theme, "Start a new prompt", "Ctrl+N"))
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.settings_do(|_, _| Effect::Prompts(PromptAction::New), window, cx);
        }));

        let rows = library.prompts.iter().map(|prompt| {
            let active = selected == Some(prompt.name.as_str());
            let focused = active && on_list;
            let name = prompt.name.clone();
            div()
                .id(SharedString::from(format!("prompt-row-{}", prompt.name)))
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .cursor_pointer()
                .border(px(theme::BORDER_WIDTH))
                .border_color(gpui::transparent_black())
                .when(active, |el| el.bg(rgb(theme.surface_raised)))
                .when(focused, |el| el.border_color(rgb(theme.selected_ring)))
                .hover(|style| style.bg(rgb(theme.surface_raised)))
                .active(|style| style.opacity(opacity::ACTIVE))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    let name = name.clone();
                    shell.settings_do(
                        move |dialog, _| {
                            dialog.focus = Focus::Prompts(Spot::List);
                            Effect::Prompts(PromptAction::Open(name))
                        },
                        window,
                        cx,
                    );
                }))
                .child(
                    div()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .child(prompt.title.clone()),
                )
                .when(prompt.title != prompt.name, |el| {
                    el.child(meta_line(theme, prompt.name.clone().into()))
                })
        });

        let broken = library.errors.iter().map(|error| {
            let file = error.file.file_name().map_or_else(
                || error.file.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(space::XS))
                .px(px(space::S))
                .py(px(space::XS))
                .child(
                    div()
                        .flex()
                        .gap(px(space::XS))
                        .text_size(font_size::META)
                        .child(div().text_color(rgb(theme.status_exited)).child("✕"))
                        .child(
                            div()
                                .min_w_0()
                                .text_color(rgb(theme.text))
                                .child(file),
                        ),
                )
                .child(meta_line(theme, error.message.clone().into()))
        });

        div()
            .flex_none()
            .w(px(theme::SETTINGS_PROMPTS_LIST_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::S))
            .when(empty, |el| {
                el.child(meta_line(theme, "No prompts yet.".into()))
            })
            .child(div().flex().child(new_button))
            .when(!empty, |el| {
                el.child(
                    div()
                        .id("prompts-list")
                        .max_h(px(theme::SETTINGS_PROMPTS_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.prompts_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows)
                        .children(broken),
                )
            })
            .into_any_element()
    }

    /// The Name field, the prompt text, the variables hint and Save, Delete and Open folder; a
    /// line to pick or start a prompt while none is open.
    fn prompt_editor(
        &self,
        dialog: &SettingsDialog,
        typing: bool,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let Some(draft) = &dialog.prompts.draft else {
            return div()
                .flex_1()
                .min_w_0()
                .child(meta_line(
                    theme,
                    "Pick a prompt to edit it, or start a new one.".into(),
                ))
                .into_any_element();
        };
        let name_focused = dialog.focus == Focus::Prompts(Spot::Name) && typing;
        let body_focused = dialog.focus == Focus::Prompts(Spot::Body) && typing;

        let name_field = caret_field(
            "prompt-name",
            &draft.name,
            "review",
            name_focused,
            theme,
            cx,
            |shell, caret| {
                let draft = (shell.settings_dialog.as_mut()).and_then(|d| d.prompts.draft.as_mut());
                if let Some(draft) = draft {
                    draft.name.click(caret.at, caret.extend, caret.clicks);
                }
            },
        )
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.settings_do(
                |dialog, _| {
                    dialog.focus = Focus::Prompts(Spot::Name);
                    Effect::None
                },
                window,
                cx,
            );
        }));

        // A click puts the caret under the pointer, a drag selects (SPEC §13).
        let body_field = caret_field(
            "prompt-body",
            draft.body.input(),
            "Write the prompt…",
            body_focused,
            theme,
            cx,
            |shell, caret| {
                let draft = (shell.settings_dialog.as_mut()).and_then(|d| d.prompts.draft.as_mut());
                if let Some(draft) = draft {
                    draft.body.click(caret.at, caret.extend, caret.clicks);
                }
            },
        )
        .on_click(cx.listener(|shell, _, window, cx| {
            shell.settings_do(
                |dialog, _| {
                    dialog.focus = Focus::Prompts(Spot::Body);
                    Effect::None
                },
                window,
                cx,
            );
        }))
        .h(px(theme::SETTINGS_PROMPT_BODY_HEIGHT))
        .overflow_y_scroll()
        .track_scroll(&self.prompts_body_scroll);

        let save = if draft.is_dirty() {
            button("prompt-save", "Save", theme, true)
                .focus_stop(theme)
                .tooltip(tooltip(theme, "Save the prompt", "Ctrl+S"))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.settings_do(|_, _| Effect::Prompts(PromptAction::Save), window, cx);
                }))
        } else {
            disabled_button("prompt-save", "Save", theme, true)
        };
        let delete = if draft.origin.is_some() {
            danger_button("prompt-delete", "Delete", theme)
                .focus_stop(theme)
                .tooltip(tooltip(theme, "Delete this prompt (asks first)", ""))
                .on_click(cx.listener(|shell, _, window, cx| {
                    shell.settings_do(|_, _| Effect::Prompts(PromptAction::Delete), window, cx);
                }))
        } else {
            disabled_button("prompt-delete", "Delete", theme, false)
        };
        let folder = secondary_button("prompt-folder", "Open folder", theme)
            .focusable_bordered(theme)
            .tooltip(tooltip(theme, "Show the prompts folder in Zed", ""))
            .on_click(cx.listener(|shell, _, window, cx| {
                shell.settings_do(|_, _| Effect::Prompts(PromptAction::OpenFolder), window, cx);
            }));

        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(space::S))
            .child(meta_line(theme, "Name".into()))
            .child(name_field)
            .children(draft.name_error.as_ref().map(|message| {
                div()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.status_exited))
                    .child(message.clone())
            }))
            .child(meta_line(theme, "Prompt".into()))
            .child(body_field)
            .child(meta_line(
                theme,
                "Variables: {file}, {selection}, {task}, {project}. A first line “# Title” names the prompt in lists."
                    .into(),
            ))
            .child(
                div()
                    .flex()
                    .gap(px(space::S))
                    .child(save)
                    .child(delete)
                    .child(folder),
            )
            .into_any_element()
    }
}
```

- [ ] **Step 4: Run, build, lint**

Run: `cargo test -p mkrew-ui --lib settings_dialog`
Expected: pass, the new test included.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings. If a method name differs from the plan (the plan's GPUI names were read on 2026-11-04 from `shell/machines.rs` and `shell/settings.rs`), fix the call to the name those files use.

Run: `cargo build -p mkrew-ui`
Expected: builds.

- [ ] **Step 5: SPEC §14 and §13**

`docs/SPEC.md` §14, Settings paragraph (line 1650): change `(General, Appearance, Notifications, Voice, Jira, Phone, Features, Keyboard, Machines)` to `(General, Appearance, Notifications, Voice, Jira, Phone, Features, Keyboard, Prompts, Machines)`.

After the Machines bullet group (after line 1664, the line that starts `  In a machine window every Settings page starts with ...`) insert:

```markdown
- Settings → Prompts (spec 2026-11-04, SPEC §20B.3): the prompt library `<config>/mkrew/prompts` without a file manager.
  The folder is read when the page opens and after every write; a prompt file that cannot be read shows its error in its own row.
  A machine window edits this computer's folder and says so in a line at the top.
  Left, `SETTINGS_PROMPTS_LIST_WIDTH` wide: a **New prompt** button (Ctrl+N) and one row per prompt, its `# Title` (else its name) and, when different, its name in `text-muted`; the open prompt has the raised fill and the one the keyboard is on the selected ring; ↑ / ↓ open the neighbour.
  An empty library shows "No prompts yet." and a filled **New prompt** button.
  Right: a Name field, the prompt text in a multi-line box (`SETTINGS_PROMPT_BODY_HEIGHT`, ADR 0075; the `# Title` line is part of the text), a hint naming `{file}`, `{selection}`, `{task}` and `{project}`, **Save** (Ctrl+S, or Ctrl+Enter in a field; disabled while nothing changed), **Delete** (asks first; disabled for a prompt not saved yet) and **Open folder** (in Zed).
  Tab walks the page list, the prompt list, Name, the text and the buttons; Enter in Name moves to the text.
  A refused name (empty, over 64 characters, a leading dot, one of `/ \ : * ? " < > |`, a control character, a trailing space or dot, a Windows reserved name such as `CON` or `aux.v2`, or another prompt's name in any case) shows its reason under the field; a failed write shows at the top of the page; neither closes the page.
  Renaming to another case of the same name is allowed.
  Switching prompt, New prompt and Esc ask "Save changes to …?" (Save, Don't save, Cancel) while the name or the text has unsaved changes; switching to another Settings page keeps the draft.
  Palette: "Global: Manage prompts…" (also in a machine window); the prompt picker's last row "Manage prompts…" opens the page too.
```

§13 key table, after the row `| Global | Open settings | Ctrl+, |` (line 1410) insert:

```markdown
| Settings | On the Prompts page (page keys of the dialog, not in `keymap.toml`): new prompt / save the open prompt | Ctrl+N / Ctrl+S |
```

- [ ] **Step 6: Commit**

```
git add crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/settings_dialog.rs crates/mkrew-ui/src/shell/prompts_render.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/machines.rs crates/mkrew-ui/src/shell/settings.rs docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Settings Prompts page with list, editor and page keys"
```

---

### Task 26 (C5): Entry points: palette "Manage prompts", picker last row; SPEC §20B.3, README, HUMAN_CHECKS

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs` (`enum Extra` after `InsertPrompt`; `ALL` length 43 and list; `title`; `available`)
- Modify: `crates/mkrew-ui/src/palette/tests.rs` (const `SEPARATE` in `fn extras` at line 477; new test after `ports_is_listed_without_a_workspace_and_in_a_machine_window`)
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (two `Extra` matches: near line 649 and near line 834)
- Modify: `crates/mkrew-ui/src/prompt_picker.rs` (`step`, `pick`, new `manage_row`, `on_manage`; two existing tests; one new test)
- Modify: `crates/mkrew-ui/src/shell/composer.rs` (`insert_prompt_pick` lines 462-468; `render_prompt_picker` lines 503-650)
- Modify: `docs/SPEC.md` (§20B.3, lines 2154-2158)
- Modify: `README.md` ("Daily use", after the Command palette bullet)
- Modify: `docs/HUMAN_CHECKS.md` (new entry above line 24)

**Interfaces:**
- Consumes: `Shell::open_prompts_settings` (C3), the page (C4).
- Produces: `Extra::ManagePrompts` ("Global: Manage prompts…"); `PromptPicker::manage_row(&self) -> usize`, `PromptPicker::on_manage(&self) -> bool`.

- [ ] **Step 1: Write the failing tests**

`crates/mkrew-ui/src/prompt_picker.rs` `mod tests`. Replace the middle of `the_pick_wraps_and_renders_with_the_known_variables` (the part from `picker.step(-1);` to the first `picker.step(1);` assertion):

```rust
        picker.step(-1);
        assert!(picker.on_manage(), "Up from the first prompt wraps to the Manage row");
        assert_eq!(picker.preview(), None);
        picker.step(-1);
        assert_eq!(picker.selected, 1);
        let preview = picker.preview().unwrap();
        assert_eq!(preview.text, "Review Fix login: ");
        assert_eq!(preview.missing, ["selection", "nope"]);
        picker.step(1);
        picker.step(1);
        assert_eq!(picker.selected, 0);
```

In `an_empty_library_has_no_preview` add after the existing assertion:

```rust
        assert!(picker.on_manage(), "the Manage row is all an empty library has");
```

Add a new test:

```rust
    #[test]
    fn the_manage_row_is_the_last_stop_of_the_keyboard_in_both_pickers() {
        // The composer's picker and the New task dialog's "From prompt…" are the same picker.
        for target in [Target::Composer(Ulid::new()), Target::TaskDialog] {
            let mut picker = PromptPicker::new(target, library(), PromptVars::default());
            assert_eq!(picker.manage_row(), 2);
            picker.step(1);
            picker.step(1);
            assert!(picker.on_manage());
            assert_eq!(picker.preview(), None, "nothing is typed for it");
            picker.step(1);
            assert_eq!(
                (picker.selected, picker.on_manage()),
                (0, false),
                "wraps to the first prompt"
            );
            picker.step(-1);
            assert!(picker.on_manage());
            picker.pick(0);
            picker.pick(2);
            assert!(picker.on_manage(), "a click on the row picks it");
            picker.pick(3);
            assert_eq!(picker.selected, 2, "past the Manage row is ignored");
        }
    }
```

`crates/mkrew-ui/src/palette/tests.rs`: in `fn extras`, change `const SEPARATE: [Extra; 11] = [` to `[Extra; 12]` and add `Extra::ManagePrompts,` after `Extra::AddMachine,`. After `ports_is_listed_without_a_workspace_and_in_a_machine_window` add:

```rust
#[test]
fn manage_prompts_is_listed_without_a_workspace_and_in_a_machine_window() {
    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        machine: true,
        ..context(View::Terminals)
    };
    assert!(Extra::ManagePrompts.available(&context(View::Terminals)));
    let listed = entries(&Keymap::defaults(), &none, &[], &[], &[]);
    let entry = listed
        .iter()
        .find(|e| e.action == Action::Extra(Extra::ManagePrompts))
        .expect("listed without a workspace");
    assert_eq!(entry.title, "Global: Manage prompts…");
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui --lib prompt_picker palette`
Expected: compile error, `Extra::ManagePrompts`, `PromptPicker::on_manage`, `manage_row` not found.

- [ ] **Step 3: Implement**

3a. `prompt_picker.rs`. Replace `step` and `pick` and add the two helpers:

```rust
    /// The row after the prompts: "Manage prompts…", which opens Settings → Prompts instead of
    /// typing anything.
    pub fn manage_row(&self) -> usize {
        self.library.prompts.len()
    }

    /// Whether the pick is the Manage row.
    pub fn on_manage(&self) -> bool {
        self.selected == self.manage_row()
    }

    /// Moves the pick by `by`, wrapping around the prompts and the Manage row.
    pub fn step(&mut self, by: i32) {
        let rows = i64::try_from(self.manage_row() + 1).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        self.selected = usize::try_from((at + i64::from(by)).rem_euclid(rows)).unwrap_or(0);
    }

    /// Picks row `index` (a click); the Manage row counts.
    pub fn pick(&mut self, index: usize) {
        if index <= self.manage_row() {
            self.selected = index;
        }
    }
```

Update the field doc: `/// Index into `library.prompts` of the picked one (the Manage row when it equals their count).` Update the module doc's first sentence list: add "and a last row that opens Settings → Prompts".

3b. `palette.rs`. In `enum Extra` add `ManagePrompts,` after `InsertPrompt,`; in `Extra::ALL` change `[Self; 43]` to `[Self; 44]` and add `Self::ManagePrompts,` after `Self::InsertPrompt,`; in `title()` add `Self::ManagePrompts => "Global: Manage prompts…",` after the `InsertPrompt` arm; in `available()` change `Self::Ports => true,` to `Self::Ports | Self::ManagePrompts => true,`.

3c. `shell/navigation.rs`. After `palette::Action::Extra(Extra::Ports) => self.open_ports(window, cx),` (line 651) add:

```rust
            palette::Action::Extra(Extra::ManagePrompts) => self.open_prompts_settings(window, cx),
```

and in the later `match extra { ... }` (after the `Extra::AddMachine` arm, line 834) add:

```rust
                    // Handled above: it needs no workspace.
                    Extra::ManagePrompts => self.open_prompts_settings(window, cx),
```

3d. `shell/composer.rs`. Replace `insert_prompt_pick` (lines 461-468) with:

```rust
    /// Enter or a click on the pick: types the picked prompt, rendered, into the picker's
    /// target; on the last row it opens Settings → Prompts instead.
    pub(crate) fn insert_prompt_pick(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(picker) = self.prompt_picker.as_ref() else {
            return;
        };
        let (manage, rendered) = (picker.on_manage(), picker.preview());
        if manage {
            self.close_prompt_picker(None, window, cx);
            self.open_prompts_settings(window, cx);
        } else if let Some(rendered) = rendered {
            self.close_prompt_picker(Some(rendered.text), window, cx);
        }
    }
```

In `render_prompt_picker`:

(i) Turn the row frame into a closure shared with the new row. In the `.map(|(index, prompt)| { ... })` closure, keep `let picked = index == picker.selected;` and the existing `.on_click(...)` and `.child(...)` blocks, and replace only the `div().id(("prompt-entry", index)) ... .active(|style| style.opacity(opacity::ACTIVE))` chain (the lines before `.on_click`) with the call shown after the closure below. Before `let rows: Vec<gpui::AnyElement> = picker` add:

```rust
        // One frame for every row: the pick has a focus-colored ring, so it stays visible under
        // hover.
        let frame = |id: gpui::ElementId, picked: bool| {
            div()
                .id(id)
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::M))
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .text_size(font_size::BODY)
                .text_color(rgb(theme.text))
                .border(px(theme::BORDER_WIDTH))
                .border_color(rgb(if picked {
                    theme.pane_focus_border
                } else {
                    theme.surface
                }))
                .when(picked, |el| el.bg(rgb(theme.surface_raised)))
                .when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))
                .cursor_pointer()
                .active(|style| style.opacity(opacity::ACTIVE))
        };
```

and make `let rows` mutable: `let mut rows: Vec<gpui::AnyElement> = picker`. Inside the `.map` closure replace the `div().id(("prompt-entry", index)) ... .active(...)` chain (everything before `.on_click(cx.listener(move |shell, _, window, cx| {` of the row) with:

```rust
                let picked = index == picker.selected;
                frame(("prompt-entry", index).into(), picked)
```

keeping the existing `.on_click(...)` and the two `.child(...)` blocks unchanged.

(ii) After the `.collect();` of `rows`, add:

```rust
        let empty = rows.is_empty();
        rows.push(
            frame("prompt-manage".into(), picker.on_manage())
                .on_click(cx.listener(|shell, _, window, cx| {
                    if let Some(picker) = shell.prompt_picker.as_mut() {
                        picker.pick(picker.manage_row());
                        shell.insert_prompt_pick(window, cx);
                    }
                }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(rgb(theme.accent_text))
                        .child("Manage prompts…"),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child("Settings → Prompts"),
                )
                .into_any_element(),
        );
```

The click closure above borrows `picker` mutably while calling `picker.manage_row()`; write it as `let manage = picker.manage_row(); picker.pick(manage);` to satisfy the borrow checker:

```rust
                    if let Some(picker) = shell.prompt_picker.as_mut() {
                        let manage = picker.manage_row();
                        picker.pick(manage);
                        shell.insert_prompt_pick(window, cx);
                    }
```

(iii) The hint line `into` (the `let into = match picker.target { ... };`) gains a branch; replace it with:

```rust
        let into = if picker.on_manage() {
            "Enter opens Settings → Prompts · Esc closes"
        } else {
            match picker.target {
                composer::Target::Composer(_) => {
                    "↑ ↓ to pick · Enter types it in the composer · Esc closes"
                }
                composer::Target::TaskDialog => {
                    "↑ ↓ to pick · Enter adds it to the task prompt · Esc closes"
                }
            }
        };
```

(iv) The list now always has the Manage row. Replace `.when(rows.is_empty(), |el| {` with `.when(empty, |el| {`, and the list block `.when(!rows.is_empty(), |el| { el.child( div().id("prompt-entries") ... .children(rows), ) })` with an unconditional child:

```rust
            .child(
                div()
                    .id("prompt-entries")
                    .max_h(px(theme::QUICK_OPEN_LIST_HEIGHT))
                    .overflow_y_scroll()
                    .track_scroll(&self.prompt_picker_scroll)
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .children(rows),
            )
```

The empty-state message `"No prompts yet: add Markdown files to the prompts folder."` and its "Open prompts folder" button stay; the Manage row under them is the way to add prompts without a file manager. Change the message to `"No prompts yet: use Manage prompts… below, or add Markdown files to the prompts folder."`.

3e. Docs.

`docs/SPEC.md` §20B.3 (lines 2154-2158): replace from `` `mkrew_core::prompt::render(template, vars)`. Entry points:`` through `is missing: `review.md`, `explain.md`, `fix-tests.md`. Read by the UI directly (config, not state).` with:

```markdown
   `mkrew_core::prompt::render(template, vars)`. Entry points: palette "Insert prompt…" (into the
   open composer, else opens one on the focused agent pane), a "Prompts" menu in the composer,
   and a "From prompt…" menu in the task dialog (the same modal picker, §20B.3 T10.5). Three
   samples are seeded on first run if the dir is missing: `review.md`, `explain.md`,
   `fix-tests.md`. Read and written by the UI directly (config, not state); a machine window uses
   this computer's folder.
   The picker's last row "Manage prompts…" and the palette's "Global: Manage prompts…" open
   Settings → Prompts (§14), where prompts are added, edited, renamed and deleted with
   `mkrew_core::prompt::{validate_name, create, save, rename, delete, read}`.
   A name is at most 64 characters and cannot be empty, start with a dot, contain
   `/ \ : * ? " < > |` or a control character, end in a space or a dot, or be a Windows reserved name
   (`CON`, `PRN`, `AUX`, `NUL`, `COM1`-`COM9`, `LPT1`-`LPT9`, also before a first dot); it is
   unique ignoring case, except that a prompt may be renamed to a new case of its own name.
   `save` writes a temp file in the folder and renames it over the target.
```

(keep the original three-space indentation and hard-wrapped style of that item).

`README.md`, "Daily use", after the `**Command palette**` bullet (line 186):

```markdown
- **Prompts**: Settings → Prompts (`Ctrl+,`, or the palette's "Global: Manage prompts…") adds, edits,
  renames and deletes your prompt library; insert one from the composer or the New task dialog's
  "From prompt…".
```

`docs/HUMAN_CHECKS.md`: insert above `### To verify - 2026-11-03 A restart drops the panes it cannot bring back (ADR 0093)` (line 24):

```markdown
### To verify - 2026-11-04 Prompts page in Settings (spec 2026-11-04 part C)

- [ ] **Look (dark + light):** Ctrl+, then Prompts (between Keyboard and Machines).
  The prompt list is 200 px wide on the left: a **New prompt** button, then one row per prompt with its title and, when different, its name in muted text.
  The open prompt has the raised fill; with the keyboard on the list it also has a ring.
  The editor on the right shows Name, a 240 px prompt box that scrolls, the variables hint, and Save, Delete, Open folder; Save is disabled until something changes; Delete is disabled for a new prompt.
  Nothing is clipped at the dialog's 760 px width.
- [ ] **Empty state (dark + light):** delete every prompt; the page says "No prompts yet." with a filled **New prompt** button and "Pick a prompt to edit it, or start a new one." on the right.
- [ ] **Errors (dark + light):** type `con` or `a/b` or an existing name in another case, then Save: the reason shows in red under the Name field and the page stays open with your text.
  Make the folder read-only or lock a file, then Save: the error shows at the top of the page.
  Put a non-UTF-8 file in the folder: it shows with a red ✕ and its error in its own row.
- [ ] **Keyboard only:** Tab goes tab list, prompt list, Name, text, then New prompt / Save / Delete / Open folder with the focus ring.
  ↑ / ↓ on the list open the neighbour; Enter on the list goes to Name; Enter in Name goes to the text; Ctrl+N starts a new prompt; Ctrl+S and Ctrl+Enter save, also with a button focused.
  With a file open in the Editor behind Settings, Ctrl+S saves the prompt and not the file.
- [ ] **Unsaved changes:** edit the text, then click or press ↓ on another prompt, or Ctrl+N, or Esc: "Save changes to …?" appears with Save, Don't save and Cancel.
  Save writes and continues, Don't save drops the edit and continues, Cancel (or Esc) stays.
  Switching to another Settings page and back keeps the edit.
- [ ] **Rename (Windows):** rename `review` to `Review`: it works, the list shows `Review`, no "exists already" message.
  Rename to the name of another prompt in any case: refused under the field.
  Rename and edit the text in one Save: both are kept.
- [ ] **Delete:** Delete asks "Delete “name”?" with Delete and Cancel; Delete removes the file and the list drops it.
- [ ] **Open folder and re-read:** Open folder opens the prompts folder in Zed; edit a prompt there, switch to another Settings page and back (or reopen Settings): the page shows the new text and the new file.
- [ ] **Entry points:** the palette lists "Global: Manage prompts…" (also in a machine window, where the page says the prompts are on this computer).
  The prompt picker (composer Prompts, or palette "Insert prompt…") ends with a "Manage prompts…" row: ↓ from the last prompt reaches it, Enter opens Settings → Prompts.
  New task → "From prompt…" → Manage prompts… opens Settings over the dialog; Esc closes Settings and the dialog keeps its text and the keyboard.
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p mkrew-ui --lib prompt_picker palette`
Expected: pass.

Run: `cargo build -p mkrew-ui`
Expected: builds (the shell arms and the picker render compile).

- [ ] **Step 5: Verify the whole part once**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and every test pass. If `cargo fmt --check` fails, run `cargo fmt --all` and commit only the files of part C.

- [ ] **Step 6: Commit**

```
git add crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/prompt_picker.rs crates/mkrew-ui/src/shell/composer.rs docs/SPEC.md README.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Manage prompts in the palette and as the picker's last row"
```
