# Polish P2 - Scoped palette and protocol 54 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One palette with seven scopes (All, Commands, Sessions, Files, Search, Branches, Commits) replaces the command palette and Quick open, fed by three new daemon requests (`git.branches`, `git.checkout`, `fs.search`) at protocol 54.

**Architecture:** The daemon gets two `GitBackend` methods (`branches`, `checkout`) and a `search` module on ripgrep's crates; `fs.search` runs beside the connection's read loop so a newer search can stop an older one through a per-connection `AtomicU64`.
`mkrew_ui::palette` stays pure: `Palette` holds the synchronous entries (Commands, Sessions) and one `Remote` per daemon scope, hands the shell the calls it wants (`wanted()` returns numbered `Fetch`es with their debounce) and takes the answers back (`answer(serial, ..)` drops late ones).
`shell/navigation.rs` only sends the calls, runs the picked action and draws the lines; `quick_open.rs` is deleted and its serial rule, `Loading / Loaded / Failed` states and `insert_into` live in the palette's Files scope.

**Tech Stack:** Rust, gpui-pre 0.3.7, `ignore` (parallel walker, already used by `fs.find`), `grep-searcher` 0.1.17 + `grep-regex` 0.1.14 + `grep-matcher` 0.1.9 (new), `git switch` and `git for-each-ref` through the daemon's `git()` helper, serde.

**Spec:** `docs/superpowers/specs/2026-11-21-polish-palette-settings-design.md` (P2 row of section 9: sections 7 and 8, the P2 part of sections 10, 11 and 12).

## Global Constraints

- Work in the worktree `.worktrees/polish` on branch `feat/polish` (superpowers:using-git-worktrees), shared with Plans P1 and P3; commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, only the files of the task, never push.
- Plan P1 is merged first. Tasks 1 to 5 (protocol and daemon) do not need it; Tasks 7b, 11, 12 and 13 consume its names.
- P1 names this plan uses, exactly and never redefined here: `theme::{PALETTE_WIDTH, PALETTE_LIST_HEIGHT, PALETTE_SELECTED_BAR, PALETTE_GROUP_LIMIT}` (the last is a `usize` = 5), `widgets::floating_panel(id, &Theme) -> Stateful<Div>`, `widgets::list_row(id, selected: bool, &Theme) -> Stateful<Div>`, `widgets::kbd(text: String, &Theme, on_accent: bool) -> Div` (exists today), `Look::Chip`, and `Pressable::{pressable, on_press}` (re-exported by `shell/widgets.rs`, so `use shell::widgets::*` in `main.rs` brings them to every `shell/*.rs`).
- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens.
  Spacing only 4 / 8 / 12 / 16 / 24 / 32.
  No hex colors or raw px numbers outside `theme.rs`.
  Dark and light themes both work.
- No `unwrap` / `expect` outside tests unless provably infallible (say why in a comment).
  `thiserror` in libraries, `anyhow` in binaries.
- Tests never call a real agent or the network; git tests use temporary repositories (see the `fn git(dir, &[..])` helpers).
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fmt check, clippy `-D warnings`, tests).
  Fast loops: `cargo test -p mkrew-protocol`, `cargo test -p mkrew-daemon --lib <filter>`, `cargo test -p mkrew-daemon --test daemon <filter>`, `cargo test -p mkrew-ui <filter>` (lib tests, includes `palette`) and `cargo test -p mkrew-ui --bin mkrew <filter>` (tests of `shell/*`).
- ADR number 0097 belongs to Plan P1; this plan writes no ADR.
  `PROTOCOL_VERSION` 54 (checked free on 2026-11-21).
- Never the em dash character in code, docs or UI text; use `-`.
  In long Markdown put each sentence on its own line.
- Windows native is first-class: paths in replies use `/`, `git` runs with the daemon's `git()` helper (no console window), tests do not assume `sh`.
- Palette keys: `Command::QuickOpen` keeps its name and `ctrl-shift-o` (user keymaps keep working); `Command::SearchInFiles` (`search_in_files`) is bound to `ctrl-alt-shift-f`.
  Checked free on 2026-11-21: `keymap.rs` `default_keys` has no `ctrl-alt-shift-*` binding and the keymap tests only use `ctrl-alt-shift-y`.
- Versions on crates.io on 2026-11-21 (`cargo search`, API `dependencies`): `grep-searcher` 0.1.17, `grep-regex` 0.1.14, `grep-matcher` 0.1.9, all edition 2024 without a `rust-version`; they need `bstr ^1.6.2`, `encoding_rs ^0.8.33`, `memchr ^2.6.3`, `regex-automata ^0.4`, `regex-syntax ^0.8` (the workspace `Cargo.lock` has bstr 1.13.1, encoding_rs 0.8.42, memchr 2.8.3, regex-automata 0.4.18, regex-syntax 0.8.11), plus `encoding_rs_io`, `memmap2` and `log`, which are new to the tree.

## Spec details this plan adds (the spec is silent or looser)

1. **Section 7.1, `Row` versus `Entry`.**
   `Row` borrows an `Entry`, so `detail`, `age` and the daemon highlight ranges (`marks`) live on `Entry`; `Row` exposes `section`, `detail` and `age` as `&str` borrows.
   `section` is derived from the action (`Action::scope()`), not stored.
2. **Section 7.2, repo and worktree.**
   The daemon refuses `repo` together with `worktree_task_id` (ADR 0029).
   `Source::new` drops the repo when a worktree is shown.
3. **Section 7.6, `ShowCommit` for a commit older than the History's 50.**
   The History's `git.log` asks for 50 commits and moves the selection to the first one when the picked commit is not listed.
   `GitView::show_commit` raises the log to `palette::COMMIT_LIMIT` (500, the palette's own length) until the History is left (Task 9).
4. **Section 8.2, error code and remote branches.**
   The error code is `invalid_state` and the message is git's trimmed stderr with no prefix (`GitCli::checkout` builds the `io::Error` from it).
   A success broadcasts `git.changed`.
   A remote branch is told from a local one by `refs/heads/<branch>` / `refs/remotes/<branch>`, since local names may contain `/`.
   The UI never sends `regex: true` (the spec has no toggle).
5. **Section 7.7, callers of `QUICK_OPEN_LIST_HEIGHT`.**
   The `/` picker (`slash.rs`) and the prompt picker (`composer.rs`) also use it; they move to `PALETTE_LIST_HEIGHT` (480 instead of 360).

## Review Focus

1. Typing fast in Files or Search, or closing and reopening the palette: an old answer must never replace a newer one or land in the next palette.
   Pinned by `files_are_asked_on_open_debounced_and_late_answers_are_dropped` and `a_closed_palettes_answer_never_lands_in_the_next` (Task 7b).
2. A query with regex metacharacters (`(`, `[`, `a.c`) in the Search scope is plain text, never an "invalid regex" error; an invalid pattern with `regex: true` says what is wrong.
   Pinned by `a_query_is_literal_unless_regex_is_on` and `an_invalid_regex_names_the_problem` (Task 4).
3. Checkout with a dirty tree or a missing branch: git's refusal reaches the toast unchanged and nothing else changes; a remote branch becomes a tracking branch.
   Pinned by `branches_lists_newest_first_and_checkout_switches_or_tracks` (Task 2) and `git_branches_and_checkout_run_in_the_project_repo` (Task 3).
4. A task worktree shown in Files while the Git view has a child repo selected: the palette must not send `repo` with `worktree_task_id` (the daemon answers `invalid_request` and every Branches and Commits row would fail).
   Pinned by `a_worktree_never_carries_a_repo` (Task 7b).
5. Search over awkward files: binary files, files over 2 MB, CRLF lines, a line with multi-byte characters cut at 200 characters, a repository with more hits than the limit.
   Pinned by `binary_and_big_files_are_skipped`, `previews_are_trimmed_cut_and_carry_byte_ranges` and `the_limit_stops_the_search_and_says_so` (Task 4).

## File structure

- Create: `crates/mkrew-daemon/src/search.rs` (the content search).
- Delete: `crates/mkrew-ui/src/quick_open.rs`.
- Protocol: `crates/mkrew-protocol/src/files.rs` (types and their tests), `message.rs` (three requests), `lib.rs` (version), `message/tests.rs` (version test).
- Daemon: `Cargo.toml`, `src/lib.rs`, `src/git.rs` (`branches`, `checkout`), `src/undo.rs` (test stub), `src/files.rs` (three helpers made `pub(crate)`), `src/server.rs` (dispatch, `Conn`), `src/server/git.rs`, `src/server/fs.rs`, tests in `tests/daemon/{main,git,fs_ai}.rs`.
- UI lib: `src/palette.rs` and `src/palette/tests.rs` (model), `src/keymap.rs`, `src/git_view.rs` and `src/git_view/tests.rs`, `src/theme.rs`, `src/lib.rs`, `src/slash_picker.rs`, `src/prompt_picker.rs`, `src/settings_dialog.rs`.
- UI bin: `src/main.rs`, `src/shell/{navigation,git,git_menus,composer,docs,jira,slash,focus,hosts,connection,task_dialog,detached_render,files_render,sessions}.rs`.
- Docs: `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`.

---

### Task 1: Protocol 54

**Files:**
- Modify: `crates/mkrew-protocol/src/files.rs` (new types after `FsMatch`, line 575; tests in `mod tests`)
- Modify: `crates/mkrew-protocol/src/message.rs:19-24` (imports), the `Request` enum (after `GitRepos` at line 240 and after `FsFind` at line 270)
- Modify: `crates/mkrew-protocol/src/lib.rs:16`
- Modify: `crates/mkrew-protocol/src/message/tests.rs` (end)

**Interfaces:**
- Produces (all `pub`, re-exported through `mkrew_protocol::*`):
  - `GitBranchesParams { project_id: Ulid, worktree_task_id: Option<Ulid>, repo: Option<String> }`
  - `Branch { name: String, remote: bool, current: bool, upstream: Option<String>, last_commit_at: i64, subject: String }`
  - `GitCheckoutParams { project_id: Ulid, worktree_task_id: Option<Ulid>, repo: Option<String>, branch: String }`
  - `FsSearchParams { project_id: Ulid, worktree_task_id: Option<Ulid>, query: String, regex: bool, limit: u32 }` with `FsSearchParams::DEFAULT_LIMIT: u32 = 200`
  - `SearchHit { path: String, line: u32, preview: String, ranges: Vec<(u32, u32)> }`, `FsSearchResult { hits: Vec<SearchHit>, truncated: bool }`
  - `Request::GitBranches(GitBranchesParams)` (`git.branches`, result `Branch[]`), `Request::GitCheckout(GitCheckoutParams)` (`git.checkout`, result `null`), `Request::FsSearch(FsSearchParams)` (`fs.search`, result `FsSearchResult`)
  - `PROTOCOL_VERSION = 54`

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-protocol/src/files.rs`, `every_files_and_git_request_round_trips`: after the `Request::GitRepos(ProjectRef { project_id }),` entry of `requests` add

```rust
            Request::GitBranches(GitBranchesParams {
                project_id,
                worktree_task_id: wt,
                repo: Some("fe".into()),
            }),
            Request::GitCheckout(GitCheckoutParams {
                project_id,
                worktree_task_id: None,
                repo: None,
                branch: "origin/feature-x".into(),
            }),
```

after the `Request::FsFind(FsFindParams { .. }),` entry add

```rust
            Request::FsSearch(FsSearchParams {
                project_id,
                worktree_task_id: wt,
                query: "needle".into(),
                regex: true,
                limit: 200,
            }),
```

and in `methods`: add `"git.branches", "git.checkout",` after `"git.repos",` and `"fs.search",` after `"fs.find",`.

Add this test after `optional_params_default_when_missing`:

```rust
    #[test]
    fn palette_requests_default_and_keep_their_wire_shapes() {
        let id = sid().to_string();
        let branches: GitBranchesParams = serde_json::from_value(json!({"project_id": id})).unwrap();
        assert_eq!((branches.worktree_task_id, branches.repo), (None, None));
        let checkout: GitCheckoutParams =
            serde_json::from_value(json!({"project_id": id, "branch": "main"})).unwrap();
        assert_eq!((checkout.repo, checkout.branch.as_str()), (None, "main"));
        let search: FsSearchParams =
            serde_json::from_value(json!({"project_id": id, "query": "x"})).unwrap();
        assert_eq!(
            (search.regex, search.limit, search.worktree_task_id),
            (false, 200, None)
        );

        let result = FsSearchResult {
            hits: vec![SearchHit {
                path: "src/a.rs".into(),
                line: 3,
                preview: "let x = 1;".into(),
                ranges: vec![(4, 5)],
            }],
            truncated: true,
        };
        assert_eq!(
            serde_json::to_value(&result).unwrap(),
            json!({
                "hits": [{"path": "src/a.rs", "line": 3, "preview": "let x = 1;", "ranges": [[4, 5]]}],
                "truncated": true
            })
        );
        let branch = Branch {
            name: "origin/x".into(),
            remote: true,
            current: false,
            upstream: None,
            last_commit_at: 1_700_000_000_000,
            subject: "fix".into(),
        };
        let back: Branch = serde_json::from_value(serde_json::to_value(&branch).unwrap()).unwrap();
        assert_eq!(back, branch);
    }
```

Append to `crates/mkrew-protocol/src/message/tests.rs`:

```rust
#[test]
fn the_palette_protocol_is_version_54() {
    assert_eq!(crate::PROTOCOL_VERSION, 54);
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p mkrew-protocol`
Expected: FAIL to compile (`GitBranchesParams`, `FsSearchParams`, ... not found).

- [ ] **Step 3: Add the types**

In `files.rs`, after the `FsMatch` struct and before `#[cfg(test)]`:

```rust
/// `git.branches`: the repository's local and remote-tracking branches, newest commit first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitBranchesParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
    /// A direct child folder of the project folder that is its own git repository (ADR 0029);
    /// `None` = the project folder. Not with `worktree_task_id`.
    #[serde(default)]
    pub repo: Option<String>,
}

/// One `git.branches` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Branch {
    /// `main`, or `origin/feature-x` for a remote-tracking branch.
    pub name: String,
    pub remote: bool,
    /// HEAD is on it.
    pub current: bool,
    /// The remote-tracking branch a local branch follows (`origin/main`).
    pub upstream: Option<String>,
    /// Unix ms of the branch's last commit.
    pub last_commit_at: i64,
    /// First line of that commit's message.
    pub subject: String,
}

/// `git.checkout`: `git switch <branch>`; a remote-tracking branch with no local namesake
/// gets a tracking branch (`git switch --track`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitCheckoutParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
    /// As in [`GitBranchesParams`].
    #[serde(default)]
    pub repo: Option<String>,
    pub branch: String,
}

/// `fs.search`: the text of the project's files (ripgrep's engine, no index).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsSearchParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
    pub query: String,
    /// `false`: the query is plain text. Smart case either way: an all-lowercase query ignores
    /// case.
    #[serde(default)]
    pub regex: bool,
    /// Hits after which the search stops and sets `truncated`.
    #[serde(default = "FsSearchParams::default_limit")]
    pub limit: u32,
}

impl FsSearchParams {
    pub const DEFAULT_LIMIT: u32 = 200;

    fn default_limit() -> u32 {
        Self::DEFAULT_LIMIT
    }
}

/// One `fs.search` hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Relative to the searched folder, `/`-separated.
    pub path: String,
    /// 1-based.
    pub line: u32,
    /// The matching line, trimmed and cut to 200 characters.
    pub preview: String,
    /// Byte ranges (start, end) of the matches in `preview`.
    pub ranges: Vec<(u32, u32)>,
}

/// `fs.search` result: hits ordered by path, then line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsSearchResult {
    pub hits: Vec<SearchHit>,
    /// More than `limit` hits exist.
    pub truncated: bool,
}
```

In `message.rs` the import (lines 19-24) becomes

```rust
use crate::files::{
    AiGenerateParams, AiGeneratedEvent, FsCreateParams, FsFindParams, FsIgnoreParams, FsListParams,
    FsReadParams, FsSearchParams, FsWriteParams, GitBranchesParams, GitChangedEvent,
    GitCheckoutParams, GitCommitParams, GitDiffParams, GitDiscardParams, GitLogParams,
    GitRemoteParams, GitSetBaseParams, GitSetViewedParams, GitShowParams, GitStageParams,
    GitStatusParams, ProjectRef, WatchReleaseParams,
};
```

and the enum gets, after `GitRepos(ProjectRef),`:

```rust
    /// Returns `Branch[]`, newest commit first (Polish spec 2026-11-21 §8.1).
    #[serde(rename = "git.branches")]
    GitBranches(GitBranchesParams),
    /// Switches branch; returns `null` and broadcasts `git.changed` (§8.2).
    #[serde(rename = "git.checkout")]
    GitCheckout(GitCheckoutParams),
```

and after `FsFind(FsFindParams),`:

```rust
    /// Returns [`FsSearchResult`]: a content search of the target's files (§8.3).
    #[serde(rename = "fs.search")]
    FsSearch(FsSearchParams),
```

In `lib.rs`: `pub const PROTOCOL_VERSION: u32 = 54;`.

- [ ] **Step 4: Run the tests**

Run: `cargo fmt --all && cargo test -p mkrew-protocol`
Expected: PASS. (`cargo fmt` may touch other files: stage only this task's.)

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-protocol/src
git -c user.email=tungnv975@gmail.com commit -m "feat(protocol): git.branches, git.checkout and fs.search (protocol 54)"
```

---

### Task 2: `GitBackend::branches` and `checkout`

**Files:**
- Modify: `crates/mkrew-daemon/src/git.rs` (imports line 4-8, trait after `delete_branch` at line 156, `impl GitBackend for GitCli` after `delete_branch` at line 748, a free fn near `nul_separated` at line 927, tests in `mod tests`)
- Modify: `crates/mkrew-daemon/src/undo.rs` (test stub `BigDiff`, after the `worktree_add` stub at line 332)

**Interfaces:**
- Consumes: `git(dir)`, `view_git(dir)`, `run(command)`, `text(bytes)` (private helpers of `git.rs`), `mkrew_protocol::Branch` (Task 1).
- Produces: `GitBackend::branches(&self, dir: &Path) -> io::Result<Vec<mkrew_protocol::Branch>>` and `GitBackend::checkout(&self, dir: &Path, branch: &str) -> io::Result<()>`; `fn parse_branches(text: &str) -> Vec<mkrew_protocol::Branch>` (private, tested).
  Note the local struct `crate::git::Branch` already exists (a local branch and its upstream for `push`): the trait signature spells the protocol type `mkrew_protocol::Branch`.

- [ ] **Step 1: Write the failing tests**

In the `mod tests` of `git.rs` (it has `run(dir, args)`; add the helper below it):

```rust
    /// `git commit` of the index with both dates fixed, so branch order does not depend on
    /// the clock.
    fn commit_at(dir: &Path, args: &[&str], when: &str) {
        let output = git(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false", "commit", "-q"])
            .args(args)
            .env("GIT_AUTHOR_DATE", when)
            .env("GIT_COMMITTER_DATE", when)
            .output()
            .unwrap();
        assert!(output.status.success(), "git commit {args:?} failed");
    }

    #[test]
    fn branch_rows_drop_remote_head_and_remote_namesakes() {
        let text = "refs/heads/topic\0\0origin/topic\0200\0topic work\n\
                    refs/remotes/origin/topic\0\0\0200\0topic work\n\
                    refs/remotes/origin/HEAD\0\0\0100\0one\n\
                    refs/remotes/origin/only-remote\0\0\0150\0remote work\n\
                    refs/heads/main\0*\0origin/main\0100\0one\n\
                    refs/remotes/origin/main\0\0\0100\0one\n";
        let rows: Vec<_> = parse_branches(text)
            .into_iter()
            .map(|b| (b.name, b.remote, b.current, b.upstream, b.last_commit_at))
            .collect();
        assert_eq!(
            rows,
            [
                ("topic".to_owned(), false, false, Some("origin/topic".to_owned()), 200_000),
                ("origin/only-remote".to_owned(), true, false, None, 150_000),
                ("main".to_owned(), false, true, Some("origin/main".to_owned()), 100_000),
            ]
        );
    }

    #[test]
    fn branches_lists_newest_first_and_checkout_switches_or_tracks() {
        let origin = tempfile::tempdir().unwrap();
        run(origin.path(), &["init", "-q", "--bare"]);
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        run(repo, &["init", "-q", "-b", "main"]);
        run(repo, &["config", "core.autocrlf", "false"]);
        std::fs::write(repo.join("a.txt"), "main\n").unwrap();
        run(repo, &["add", "."]);
        commit_at(repo, &["-m", "one"], "2026-01-01T00:00:00Z");
        run(repo, &["remote", "add", "origin", &origin.path().to_string_lossy()]);
        run(repo, &["push", "-q", "origin", "main"]);
        run(repo, &["checkout", "-q", "-b", "remote-only"]);
        commit_at(repo, &["--allow-empty", "-m", "remote work"], "2026-02-01T00:00:00Z");
        run(repo, &["push", "-q", "origin", "remote-only"]);
        run(repo, &["checkout", "-q", "main"]);
        run(repo, &["branch", "-D", "remote-only"]);
        run(repo, &["checkout", "-q", "-b", "topic"]);
        std::fs::write(repo.join("a.txt"), "topic\n").unwrap();
        commit_at(repo, &["-a", "-m", "topic work"], "2026-03-01T00:00:00Z");
        run(repo, &["push", "-q", "origin", "topic"]);
        run(repo, &["checkout", "-q", "main"]);
        run(repo, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main"]);

        let branches = GitCli.branches(repo).unwrap();
        let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
        // origin/HEAD, origin/main and origin/topic are left out.
        assert_eq!(names, ["topic", "origin/remote-only", "main"]);
        let current: Vec<bool> = branches.iter().map(|b| b.current).collect();
        assert_eq!(current, [false, false, true]);
        assert_eq!(branches[0].subject, "topic work");
        // 2026-03-01T00:00:00Z.
        assert_eq!(branches[0].last_commit_at, 1_772_323_200_000);
        assert!(branches[1].remote && !branches[0].remote);

        GitCli.checkout(repo, "topic").unwrap();
        assert_eq!(run(repo, &["rev-parse", "--abbrev-ref", "HEAD"]), "topic");
        // A remote-tracking branch with no local namesake gets a tracking branch.
        GitCli.checkout(repo, "origin/remote-only").unwrap();
        assert_eq!(run(repo, &["rev-parse", "--abbrev-ref", "HEAD"]), "remote-only");
        assert_eq!(run(repo, &["config", "branch.remote-only.remote"]), "origin");

        // Git's own words come back, with nothing in front of them.
        GitCli.checkout(repo, "main").unwrap();
        std::fs::write(repo.join("a.txt"), "dirty\n").unwrap();
        let refused = GitCli.checkout(repo, "topic").unwrap_err().to_string();
        assert!(refused.contains("a.txt"), "{refused}");
        assert!(!refused.starts_with("git failed"), "{refused}");
        let missing = GitCli.checkout(repo, "nope").unwrap_err().to_string();
        assert!(missing.contains("nope"), "{missing}");
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-daemon --lib git::tests::branch`
Expected: FAIL to compile (`parse_branches`, `branches`, `checkout` not found).

- [ ] **Step 3: Implement**

Imports of `git.rs`: add `use std::collections::HashSet;` above `use std::io;`.

Trait, after `delete_branch`:

```rust
    /// The branches under `refs/heads` and `refs/remotes`, newest commit first.
    /// `refs/remotes/<remote>/HEAD` and a remote branch whose short name a local branch has
    /// are left out.
    fn branches(&self, dir: &Path) -> io::Result<Vec<mkrew_protocol::Branch>>;

    /// `git switch <branch>`, with `--track` for a remote-tracking branch that has no local
    /// namesake. An error carries git's stderr as is.
    fn checkout(&self, dir: &Path, branch: &str) -> io::Result<()>;
```

In `impl GitBackend for GitCli`, after `delete_branch`:

```rust
    fn branches(&self, dir: &Path) -> io::Result<Vec<mkrew_protocol::Branch>> {
        let mut command = view_git(dir);
        // The full ref name, because `refname:short` of `origin/HEAD` is just `origin`.
        // `%00` separates the fields; a subject never holds a NUL.
        command.args([
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname)%00%(HEAD)%00%(upstream:short)%00%(committerdate:unix)%00%(subject)",
            "refs/heads",
            "refs/remotes",
        ]);
        Ok(parse_branches(&text(run(command)?)))
    }

    fn checkout(&self, dir: &Path, branch: &str) -> io::Result<()> {
        let has = |namespace: &str| -> io::Result<bool> {
            Ok(view_git(dir)
                .args(["show-ref", "--verify", "--quiet"])
                .arg(format!("refs/{namespace}/{branch}"))
                .status()?
                .success())
        };
        let mut command = git(dir);
        command.arg("switch");
        // Local names may hold `/`, so `origin/x` is a remote-tracking branch only when no
        // local branch has that name.
        if !has("heads")? && has("remotes")? {
            command.arg("--track");
        }
        command.arg(branch);
        let output = command.stderr(Stdio::piped()).output()?;
        if output.status.success() {
            return Ok(());
        }
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
```

Free function next to `nul_separated`:

```rust
/// The rows of the `for-each-ref` format of [`GitBackend::branches`], in their order.
fn parse_branches(text: &str) -> Vec<mkrew_protocol::Branch> {
    struct Row<'a> {
        full: &'a str,
        current: bool,
        upstream: &'a str,
        at: i64,
        subject: &'a str,
    }
    let rows: Vec<Row<'_>> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(5, '\0');
            Some(Row {
                full: fields.next()?,
                current: fields.next()? == "*",
                upstream: fields.next()?,
                at: fields.next()?.parse().unwrap_or(0),
                subject: fields.next()?,
            })
        })
        .collect();
    let local: HashSet<&str> = rows
        .iter()
        .filter_map(|row| row.full.strip_prefix("refs/heads/"))
        .collect();
    rows.iter()
        .filter_map(|row| {
            let (name, remote) = match row.full.strip_prefix("refs/heads/") {
                Some(name) => (name, false),
                None => (row.full.strip_prefix("refs/remotes/")?, true),
            };
            if remote {
                let (_, short) = name.split_once('/')?;
                if short == "HEAD" || local.contains(short) {
                    return None;
                }
            }
            Some(mkrew_protocol::Branch {
                name: name.to_owned(),
                remote,
                current: row.current,
                upstream: (!row.upstream.is_empty()).then(|| row.upstream.to_owned()),
                last_commit_at: row.at.saturating_mul(1000),
                subject: row.subject.to_owned(),
            })
        })
        .collect()
}
```

In `undo.rs`, inside `impl GitBackend for BigDiff`, after the `worktree_add` stub:

```rust
        fn branches(&self, _: &Path) -> io::Result<Vec<mkrew_protocol::Branch>> {
            unreachable!()
        }
        fn checkout(&self, _: &Path, _: &str) -> io::Result<()> {
            unreachable!()
        }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-daemon --lib git::tests`
Expected: PASS (needs `git` 2.23+ for `switch`).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/src/git.rs crates/mkrew-daemon/src/undo.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): GitBackend branches and checkout"
```

---

### Task 3: `git.branches` and `git.checkout` handlers

**Files:**
- Modify: `crates/mkrew-daemon/src/server/git.rs:7-12` (imports) and after `git_show` (line 404)
- Modify: `crates/mkrew-daemon/src/server.rs` (dispatch, after the `GitShow` arm at line 435)
- Modify: `crates/mkrew-daemon/tests/daemon/main.rs:14-41` (imports)
- Modify: `crates/mkrew-daemon/tests/daemon/git.rs` (append a test)

**Interfaces:**
- Consumes: `GitBackend::{branches, checkout}` (Task 2), `Daemon::target`, `GitCache::repo`, `check_ref`, `view_outcome`, `ok` (all in `server/git.rs`), `Request::{GitBranches, GitCheckout}` (Task 1).
- Produces: `Daemon::git_branches(&self, GitBranchesParams) -> Outcome`, `Daemon::git_checkout(&self, GitCheckoutParams) -> Outcome`. Errors: `invalid_state` outside a repository, `invalid_request` for a branch starting with `-` or holding whitespace, `invalid_state` with git's stderr when git refuses.

- [ ] **Step 1: Write the failing test**

In `tests/daemon/main.rs` add `Branch`, `GitBranchesParams` and `GitCheckoutParams` to the `use mkrew_protocol::{ .. }` list (Task 5 adds `FsSearchParams` and `FsSearchResult`: a name imported before its first use is an `unused_imports` warning in the test target).
Append to `tests/daemon/git.rs`:

```rust
#[test]
fn git_branches_and_checkout_run_in_the_project_repo() {
    let env = Env::new();
    let repo = env.repo();
    let origin = env.dir.path().join("origin.git");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "-q", "--bare"]);
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    // Commits with fixed dates: the order must not depend on the clock.
    let commit_at = |args: &[&str], when: &str| {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false", "commit", "-q"])
            .args(args)
            .env("GIT_AUTHOR_DATE", when)
            .env("GIT_COMMITTER_DATE", when)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(output.status.success(), "git commit {args:?} failed");
    };
    std::fs::write(repo.join("a.txt"), "main\n").unwrap();
    git(&repo, &["add", "."]);
    commit_at(&["-m", "one"], "2026-01-01T00:00:00Z");
    git(&repo, &["remote", "add", "origin", &origin.to_string_lossy()]);
    git(&repo, &["push", "-q", "origin", "main"]);
    git(&repo, &["checkout", "-q", "-b", "remote-only"]);
    commit_at(&["--allow-empty", "-m", "remote work"], "2026-02-01T00:00:00Z");
    git(&repo, &["push", "-q", "origin", "remote-only"]);
    git(&repo, &["checkout", "-q", "main"]);
    git(&repo, &["branch", "-D", "remote-only"]);
    git(&repo, &["checkout", "-q", "-b", "topic"]);
    std::fs::write(repo.join("a.txt"), "topic\n").unwrap();
    commit_at(&["-a", "-m", "topic work"], "2026-03-01T00:00:00Z");
    git(&repo, &["push", "-q", "origin", "topic"]);
    git(&repo, &["checkout", "-q", "main"]);
    git(&repo, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main"]);
    let plain = env.dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();

    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let outside = client.add_project(&plain);
    let branches = |project_id| {
        Request::GitBranches(GitBranchesParams {
            project_id,
            worktree_task_id: None,
            repo: None,
        })
    };
    let checkout = |branch: &str| {
        Request::GitCheckout(GitCheckoutParams {
            project_id: project.id,
            worktree_task_id: None,
            repo: None,
            branch: branch.into(),
        })
    };

    let listed: Vec<Branch> = client.call(branches(project.id)).into_result().unwrap();
    let names: Vec<&str> = listed.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["topic", "origin/remote-only", "main"]);
    assert_eq!(listed.iter().filter(|b| b.current).count(), 1);
    assert!(listed[2].current && !listed[0].remote && listed[1].remote);
    assert_eq!(listed[0].subject, "topic work");
    assert_eq!(
        error_code(client.call(branches(outside.id))),
        ErrorCode::InvalidState
    );

    let _: () = client.call(checkout("topic")).into_result().unwrap();
    assert_eq!(git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]), "topic");
    client.wait_event(&Event::GitChanged(GitChangedEvent {
        project_id: project.id,
        worktree_task_id: None,
    }));
    let _: () = client
        .call(checkout("origin/remote-only"))
        .into_result()
        .unwrap();
    assert_eq!(
        git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "remote-only"
    );

    // Git refuses to overwrite a change: its words, nothing else changes.
    git(&repo, &["checkout", "-q", "main"]);
    std::fs::write(repo.join("a.txt"), "dirty\n").unwrap();
    let Err(ResponseError::Daemon(refused)) = client.call(checkout("topic")).into_result::<()>()
    else {
        panic!("a switch over a dirty file must be refused");
    };
    assert_eq!(refused.code, ErrorCode::InvalidState);
    assert!(refused.message.contains("a.txt"), "{}", refused.message);
    assert!(!refused.message.contains("git failed"), "{}", refused.message);
    assert_eq!(git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");
    assert_eq!(
        error_code(client.call(checkout("--detach"))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(checkout("nope"))),
        ErrorCode::InvalidState
    );
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-daemon --test daemon git_branches_and_checkout`
Expected: FAIL (`GitBranches is not implemented yet`, an `invalid_request`, so `into_result().unwrap()` panics).

- [ ] **Step 3: Implement**

`server/git.rs` imports (lines 7-12) become

```rust
use mkrew_protocol::{
    ChangedFile, ErrorCode, Event, GitBranchesParams, GitChangedEvent, GitCheckoutParams,
    GitCommitParams, GitCommitResult, GitDiffParams, GitDiscardParams, GitLogParams,
    GitRemoteParams, GitSetBaseParams, GitSetViewedParams, GitShowParams, GitStageParams,
    GitStatusParams, HunkTarget, Outcome, Project, ProjectRef, ViewedFile,
};
```

After `git_show`:

```rust
    /// `git.branches`: every local and remote-tracking branch, newest commit first.
    pub(super) async fn git_branches(&self, params: GitBranchesParams) -> Outcome {
        let dir = match self
            .target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target.dir,
            Err(outcome) => return outcome,
        };
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let branches = tokio::task::spawn_blocking(move || {
            cache.repo(&*git, &dir)?;
            git.branches(&dir).map_err(ViewError::from)
        })
        .await;
        view_outcome("git.branches", branches)
    }

    /// `git.checkout`: `git switch` in the target's folder. Git's refusal comes back as
    /// `invalid_state` with its own words; a success broadcasts `git.changed` at once (the
    /// watcher misses a `.git` outside the project folder).
    pub(super) async fn git_checkout(&self, params: GitCheckoutParams) -> Outcome {
        if let Err(reason) = check_ref(&params.branch) {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let GitCheckoutParams {
            project_id,
            worktree_task_id,
            repo,
            branch,
        } = params;
        let dir = match self
            .target(project_id, worktree_task_id, repo.as_deref())
            .await
        {
            Ok(target) => target.dir,
            Err(outcome) => return outcome,
        };
        let (git, cache) = (Arc::clone(&self.git), Arc::clone(&self.git_cache));
        let switched = tokio::task::spawn_blocking(move || {
            cache.repo(&*git, &dir)?;
            git.checkout(&dir, &branch).map_err(ViewError::from)
        })
        .await;
        match switched {
            Ok(Ok(())) => {
                let _ = self.hub.events.send(Event::GitChanged(GitChangedEvent {
                    project_id,
                    worktree_task_id,
                }));
                ok(&())
            }
            Ok(Err(ViewError::Git(error))) => {
                Outcome::error(ErrorCode::InvalidState, error.to_string())
            }
            other => view_outcome("git.checkout", other),
        }
    }
```

`server.rs`, after the `Request::GitShow(params)` arm:

```rust
            Request::GitBranches(params) => (self.git_branches(params).await, After::Continue),
            Request::GitCheckout(params) => (self.git_checkout(params).await, After::Continue),
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-daemon --test daemon git_branches_and_checkout`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/server/git.rs crates/mkrew-daemon/tests/daemon
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): git.branches and git.checkout handlers"
```

---

### Task 4: The content search (`search.rs`)

**Files:**
- Create: `crates/mkrew-daemon/src/search.rs`
- Modify: `crates/mkrew-daemon/Cargo.toml` (dependencies, after `dunce`)
- Modify: `crates/mkrew-daemon/src/lib.rs` (add `mod search;` after `mod scan;`)
- Modify: `crates/mkrew-daemon/src/files.rs` (`walker` line 72, `relative` line 114 and `has_nul` line 388 become `pub(crate)`)

**Interfaces:**
- Consumes: `files::{walker, relative, has_nul}`, `docs::MAX_BYTES` (2 MiB, `u64`), `mkrew_protocol::{FsSearchResult, SearchHit}`.
- Produces: `search::search(root: &Path, query: &str, regex: bool, limit: usize, generation: &AtomicU64, mine: u64) -> Result<FsSearchResult, search::Error>` and `search::Error::{Pattern(String), Cancelled}` (`thiserror`).
  The walk is parallel and stops (`Err(Error::Cancelled)`) as soon as `generation` no longer holds `mine`.
  Tasks 4 and 5 go to one implementer back to back (two commits, no clippy gate between): until Task 5 calls `search::search`, the Task 4 commit has `dead_code` warnings only, and the clippy gate runs after Task 5.
  The first build of this task needs the network (`grep-searcher`, `grep-regex`, `grep-matcher` and `encoding_rs_io` are not in `Cargo.lock` or the local registry); the tests do not.

- [ ] **Step 1: Add the dependencies and write the failing tests**

`Cargo.toml`, after the `dunce` line:

```toml
# `fs.search` (Polish spec 2026-11-21 §8.3): ripgrep's own engine over the files `ignore` walks,
# no index. `grep-matcher` is the `Matcher` trait that `grep-regex` implements (the match ranges
# of a line); the three are released together.
grep-matcher = "0.1.9"
grep-regex = "0.1.14"
grep-searcher = "0.1.17"
```

Create `crates/mkrew-daemon/src/search.rs` with only the test module for now (the imports are in Step 3):

```rust
#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::AtomicU64;

    use super::*;

    fn write(root: &Path, path: &str, bytes: &[u8]) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }

    /// A folder that looks like a repository (the walk reads `.gitignore` only inside one).
    fn repo(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        for (path, text) in files {
            write(dir.path(), path, text.as_bytes());
        }
        dir
    }

    fn run(root: &Path, query: &str, regex: bool, limit: usize) -> Result<FsSearchResult, Error> {
        search(root, query, regex, limit, &AtomicU64::new(1), 1)
    }

    fn places(result: &FsSearchResult) -> Vec<(&str, u32)> {
        result.hits.iter().map(|h| (h.path.as_str(), h.line)).collect()
    }

    #[test]
    fn gitignore_is_respected_and_paths_are_relative_with_slashes() {
        let dir = repo(&[
            (".gitignore", "out/\n"),
            ("src/deep/a.rs", "a needle\n"),
            ("out/b.rs", "needle\n"),
        ]);
        let result = run(dir.path(), "needle", false, 10).unwrap();
        assert_eq!(places(&result), [("src/deep/a.rs", 1)]);
        assert!(!result.truncated);
    }

    #[test]
    fn binary_and_big_files_are_skipped() {
        let dir = repo(&[("text.txt", "needle\n")]);
        write(dir.path(), "image.bin", b"needle\0\x01\x02");
        let mut big = b"needle\n".to_vec();
        big.resize(usize::try_from(MAX_BYTES).unwrap() + 1, b'x');
        write(dir.path(), "big.txt", &big);
        let result = run(dir.path(), "needle", false, 10).unwrap();
        assert_eq!(places(&result), [("text.txt", 1)]);
    }

    #[test]
    fn the_limit_stops_the_search_and_says_so() {
        let text = "needle\n".repeat(10);
        let dir = repo(&[("a.txt", text.as_str()), ("b.txt", text.as_str())]);
        let cut = run(dir.path(), "needle", false, 5).unwrap();
        assert_eq!(cut.hits.len(), 5);
        assert!(cut.truncated);
        let exact = run(dir.path(), "needle", false, 20).unwrap();
        assert_eq!(exact.hits.len(), 20);
        assert!(!exact.truncated, "exactly the limit is not truncated");
    }

    #[test]
    fn an_all_lowercase_query_ignores_case() {
        let dir = repo(&[("a.txt", "Needle\nneedle\n")]);
        assert_eq!(run(dir.path(), "needle", false, 10).unwrap().hits.len(), 2);
        assert_eq!(run(dir.path(), "Needle", false, 10).unwrap().hits.len(), 1);
    }

    #[test]
    fn a_query_is_literal_unless_regex_is_on() {
        let dir = repo(&[("a.txt", "a.c\nabc\nf(x)\n")]);
        let literal = run(dir.path(), "a.c", false, 10).unwrap();
        assert_eq!(places(&literal), [("a.txt", 1)]);
        let regex = run(dir.path(), "a.c", true, 10).unwrap();
        assert_eq!(places(&regex), [("a.txt", 1), ("a.txt", 2)]);
        // A paren is text, not a broken pattern.
        let paren = run(dir.path(), "f(", false, 10).unwrap();
        assert_eq!(places(&paren), [("a.txt", 3)]);
    }

    #[test]
    fn an_invalid_regex_names_the_problem() {
        let dir = repo(&[("a.txt", "x\n")]);
        let Err(Error::Pattern(message)) = run(dir.path(), "(", true, 10) else {
            panic!("an open paren is not a regex");
        };
        assert!(!message.is_empty());
    }

    #[test]
    fn previews_are_trimmed_cut_and_carry_byte_ranges() {
        let long = format!("needle {}", "é".repeat(300));
        let dir = repo(&[
            ("a.rs", "    let needle = 1;\r\n"),
            ("b.rs", long.as_str()),
        ]);
        let result = run(dir.path(), "needle", false, 10).unwrap();
        assert_eq!(result.hits[0].preview, "let needle = 1;");
        assert_eq!(result.hits[0].ranges, [(4, 10)]);
        assert_eq!(result.hits[1].preview.chars().count(), 200);
        assert_eq!(result.hits[1].ranges, [(0, 6)]);
    }

    #[test]
    fn a_newer_search_stops_this_one() {
        let dir = repo(&[("a.txt", "needle\n")]);
        let generation = AtomicU64::new(2);
        let stopped = search(dir.path(), "needle", false, 10, &generation, 1);
        assert!(matches!(stopped, Err(Error::Cancelled)));
        assert!(search(dir.path(), "needle", false, 10, &generation, 2).is_ok());
    }
}
```

In `lib.rs` add `mod search;` after `mod scan;`.

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-daemon --lib search::`
Expected: FAIL to compile (`search`, `Error`, `MAX_BYTES`, `FsSearchResult` not found).

- [ ] **Step 3: Implement**

In `files.rs` change `fn walker(` to `pub(crate) fn walker(`, `fn relative(` to `pub(crate) fn relative(` and `fn has_nul(` to `pub(crate) fn has_nul(`.

Put this above the test module in `search.rs`:

```rust
//! `fs.search`: ripgrep's engine over the files `fs.find` lists (Polish spec 2026-11-21 §8.3).
//! Blocking; the server runs it in `spawn_blocking`.
//!
//! The query is compiled once and the walk is parallel (`ignore`): each worker reads a text
//! file whole (at most 2 MiB, no NUL in its head) and searches the bytes, so there is nothing
//! to memory-map. The walk stops at `limit` hits and as soon as a newer search of the same
//! connection bumped the generation.
// ponytail: no index; add a persistent trigram index if search on big repos measurably lags

use std::io::Read as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};

use grep_matcher::Matcher as _;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::SearcherBuilder;
use grep_searcher::sinks::Lossy;
use ignore::WalkState;
use mkrew_protocol::{FsSearchResult, SearchHit};

use crate::docs::MAX_BYTES;
use crate::files::{has_nul, relative, walker};

/// Longest preview line, in characters.
const PREVIEW_CHARS: usize = 200;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The query is not a valid pattern; the text says what is wrong.
    #[error("invalid search pattern: {0}")]
    Pattern(String),
    /// A newer search of the same connection replaced this one.
    #[error("a newer search replaced this one")]
    Cancelled,
}

/// The hits of `query` in the files under `root`, ordered by path then line, at most `limit`
/// (`truncated` when more exist). `regex: false` makes the query plain text; either way an
/// all-lowercase query ignores case. Returns [`Error::Cancelled`] once `generation` is no
/// longer `mine`.
pub fn search(
    root: &Path,
    query: &str,
    regex: bool,
    limit: usize,
    generation: &AtomicU64,
    mine: u64,
) -> Result<FsSearchResult, Error> {
    let matcher = RegexMatcherBuilder::new()
        .case_smart(true)
        .fixed_strings(!regex)
        .build(query)
        .map_err(|error| Error::Pattern(error.to_string()))?;
    let hits = Mutex::new(Vec::new());
    let found = AtomicUsize::new(0);
    let truncated = AtomicBool::new(false);
    // Copy: every worker's visitor takes its own.
    let cancelled = move || generation.load(Ordering::Relaxed) != mine;
    walker(root).build_parallel().run(|| {
        let mut searcher = SearcherBuilder::new().line_number(true).build();
        let (matcher, hits, found, truncated) = (&matcher, &hits, &found, &truncated);
        Box::new(move |entry| {
            if cancelled() || truncated.load(Ordering::Relaxed) {
                return WalkState::Quit;
            }
            let Ok(entry) = entry else {
                return WalkState::Continue;
            };
            if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                return WalkState::Continue;
            }
            let Some(path) = relative(root, entry.path()) else {
                return WalkState::Continue;
            };
            let Some(bytes) = read_text(entry.path()) else {
                return WalkState::Continue;
            };
            let mut local = Vec::new();
            let sink = Lossy(|line, text| {
                // The (limit + 1)th hit proves there are more.
                if found.fetch_add(1, Ordering::Relaxed) >= limit {
                    truncated.store(true, Ordering::Relaxed);
                    return Ok(false);
                }
                local.push(hit(&path, line, text, matcher));
                Ok(true)
            });
            // A file that cannot be searched is skipped, never fatal.
            let _ = searcher.search_slice(matcher, &bytes, sink);
            hits.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(local);
            WalkState::Continue
        })
    });
    if cancelled() {
        return Err(Error::Cancelled);
    }
    let mut hits = hits.into_inner().unwrap_or_else(PoisonError::into_inner);
    hits.sort_by(|a, b| (&a.path, a.line).cmp(&(&b.path, b.line)));
    Ok(FsSearchResult {
        hits,
        truncated: truncated.load(Ordering::Relaxed),
    })
}

/// The bytes of `path`, or `None` when it cannot be read, is over [`MAX_BYTES`] or looks
/// binary (a NUL in its head, like `fs.read`).
fn read_text(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= MAX_BYTES && !has_nul(&bytes)).then_some(bytes)
}

/// One hit: the line trimmed and cut to [`PREVIEW_CHARS`], and where the matcher matches in
/// what is kept (byte offsets, so a cut never splits a character or leaves a stale range).
fn hit(path: &str, line: u64, text: &str, matcher: &RegexMatcher) -> SearchHit {
    let preview: String = text.trim().chars().take(PREVIEW_CHARS).collect();
    let mut ranges = Vec::new();
    let to_u32 = |at: usize| u32::try_from(at).unwrap_or(u32::MAX);
    // An error here only means no highlight.
    let _ = matcher.find_iter(preview.as_bytes(), |found| {
        if found.start() < found.end() {
            ranges.push((to_u32(found.start()), to_u32(found.end())));
        }
        true
    });
    SearchHit {
        path: path.to_owned(),
        line: u32::try_from(line).unwrap_or(u32::MAX),
        preview,
        ranges,
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-daemon --lib search::`
Expected: PASS (8 tests). If `fixed_strings` or `case_smart` does not resolve on `RegexMatcherBuilder`, read `cargo doc -p grep-regex --open` for the 0.1.14 names: both exist in `grep-regex` 0.1.14 (`case_smart`, `fixed_strings`).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/Cargo.toml Cargo.lock crates/mkrew-daemon/src/search.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/files.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): content search on ripgrep's crates"
```

---

### Task 5: `fs.search` handler, generation per connection, concurrent with the read loop

**Files:**
- Modify: `crates/mkrew-daemon/src/server.rs` (imports line 6-7, the read loop at lines 202-206, `Conn` at line 1717)
- Modify: `crates/mkrew-daemon/src/server/fs.rs` (imports lines 5-20, new method after `fs_find` at line 235)
- Modify: `crates/mkrew-daemon/tests/daemon/main.rs` (imports lines 14-41: add `FsSearchParams`, `FsSearchResult`; two `Client` helpers after `call`, line 287)
- Modify: `crates/mkrew-daemon/tests/daemon/fs_ai.rs` (append a test)

**Interfaces:**
- Consumes: `search::search`, `search::Error` (Task 4), `Daemon::watched_target`, `Request::FsSearch` (Task 1), `Conn`, `Outgoing`.
- Produces: `Daemon::fs_search_start(&self, FsSearchParams, generation: Arc<AtomicU64>, mine: u64) -> Result<tokio::task::JoinHandle<Outcome>, Outcome>`; `Conn.search_generation: Arc<AtomicU64>`.
  Test helpers `Client::send(&mut self, Request) -> u64` and `Client::response(&mut self) -> (u64, Outcome)`.

- [ ] **Step 1: Write the failing test**

`tests/daemon/main.rs`, in `impl Client` after `call`:

```rust
    /// Sends `request` without waiting for its answer; [`Client::response`] reads answers in
    /// whatever order the daemon sends them.
    fn send(&mut self, request: Request) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let frame = Frame::Message(Message::Request { id, request });
        self.stream.write_all(&encode(&frame).unwrap()).unwrap();
        id
    }

    /// The next response, whichever request it answers (events and output are kept).
    fn response(&mut self) -> (u64, Outcome) {
        let mut buf = [0u8; 4096];
        loop {
            while let Some(frame) = self.decoder.next_frame().unwrap() {
                match frame {
                    Frame::Message(Message::Response { id, outcome }) => return (id, outcome),
                    Frame::Message(Message::Event(event)) => self.events.push(event),
                    Frame::Output { session_id, data } => {
                        self.output.entry(session_id).or_default().extend(data);
                    }
                    other => panic!("unexpected frame {other:?}"),
                }
            }
            let n = self.stream.read(&mut buf).unwrap();
            assert!(n > 0, "daemon closed the connection");
            self.decoder.push(&buf[..n]);
        }
    }
```

Append to `tests/daemon/fs_ai.rs`:

```rust
#[test]
fn fs_search_finds_text_and_a_newer_search_never_waits_for_an_older_one() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::create_dir(repo.join("src")).unwrap();
    std::fs::create_dir(repo.join("out")).unwrap();
    std::fs::write(repo.join(".gitignore"), "out/\n").unwrap();
    std::fs::write(repo.join("src/main.rs"), "fn main() {\n    run(Needle);\n}\n").unwrap();
    std::fs::write(repo.join("out/gen.rs"), "needle\n").unwrap();
    for n in 0..200 {
        let text = "filler needle text\n".repeat(50);
        std::fs::write(repo.join(format!("bulk{n}.txt")), text).unwrap();
    }
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let search = |query: &str, regex: bool, limit: u32| {
        Request::FsSearch(FsSearchParams {
            project_id: id,
            worktree_task_id: None,
            query: query.into(),
            regex,
            limit,
        })
    };

    // Smart case: the capital makes it exact, so only src/main.rs matches.
    let found: FsSearchResult = client
        .call(search("Needle", false, 10))
        .into_result()
        .unwrap();
    assert_eq!(found.hits.len(), 1);
    let hit = &found.hits[0];
    assert_eq!((hit.path.as_str(), hit.line), ("src/main.rs", 2));
    assert_eq!(hit.preview, "run(Needle);");
    assert_eq!(hit.ranges, [(4, 10)]);
    assert!(!found.truncated);
    // The ignored folder is not searched.
    let all: FsSearchResult = client
        .call(search("needle", false, 1_000_000))
        .into_result()
        .unwrap();
    assert!(all.hits.iter().all(|hit| !hit.path.starts_with("out/")));
    let cut: FsSearchResult = client
        .call(search("needle", false, 7))
        .into_result()
        .unwrap();
    assert_eq!((cut.hits.len(), cut.truncated), (7, true));
    assert_eq!(
        error_code(client.call(search("(", true, 10))),
        ErrorCode::InvalidRequest
    );

    // Back to back on one connection: the read loop reads the second request while the
    // first walks, so both are answered; the older answer is complete or "replaced".
    let slow = client.send(search("needle", false, 1_000_000));
    let quick = client.send(search("Needle", false, 10));
    let mut answers = HashMap::new();
    for _ in 0..2 {
        let (got, outcome) = client.response();
        answers.insert(got, outcome);
    }
    let quick: FsSearchResult = answers.remove(&quick).unwrap().into_result().unwrap();
    assert_eq!(quick.hits.len(), 1);
    match answers.remove(&slow).unwrap().into_result::<FsSearchResult>() {
        Ok(done) => assert!(done.hits.len() > 1),
        Err(ResponseError::Daemon(body)) => assert_eq!(body.code, ErrorCode::InvalidState),
        Err(other) => panic!("{other:?}"),
    }
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-daemon --test daemon fs_search_finds_text`
Expected: FAIL (`fs.search is not implemented yet`).

- [ ] **Step 3: Implement**

`server.rs` imports: add `use std::sync::atomic::{AtomicU64, Ordering};` after `use std::sync::Arc;`.

`Conn` (line 1717):

```rust
/// Per-connection state kept by the read loop.
#[derive(Default)]
struct Conn {
    greeted: bool,
    /// Sessions this client is attached to; dropping an entry detaches it.
    attachments: HashMap<Ulid, Attached>,
    /// Set by `session.attach`; its flusher starts after the reply is queued.
    new_attachment: Option<NewAttachment>,
    /// Number of the latest `fs.search` read on this connection: a newer one makes the walks
    /// of older ones stop before they open their next file.
    search_generation: Arc<AtomicU64>,
}

impl Drop for Conn {
    /// A closed connection stops the search it left running.
    fn drop(&mut self) {
        self.search_generation.fetch_add(1, Ordering::SeqCst);
    }
}
```

Read loop: replace

```rust
                let (outcome, after) = self.handle(&mut conn, request).await;
```

with

```rust
                // A content search runs beside the read loop: the connection's next
                // `fs.search` has to be read while this one walks, or it could never stop it.
                // Its reply is sent by its own task, so replies can pass each other (they
                // carry their request's id).
                if conn.greeted && let Request::FsSearch(params) = request {
                    let mine = conn.search_generation.fetch_add(1, Ordering::SeqCst) + 1;
                    let generation = Arc::clone(&conn.search_generation);
                    let started = self.fs_search_start(params, generation, mine).await;
                    let out = out.clone();
                    tokio::spawn(async move {
                        let outcome = match started {
                            Ok(job) => job.await.unwrap_or_else(|e| {
                                Outcome::error(ErrorCode::Internal, format!("fs.search failed: {e}"))
                            }),
                            Err(outcome) => outcome,
                        };
                        if let Ok(bytes) = encode(&Frame::Message(Message::Response { id, outcome })) {
                            let _ = out.send(bytes.into()).await;
                        }
                    });
                    continue;
                }
                let (outcome, after) = self.handle(&mut conn, request).await;
```

`server/fs.rs`: imports gain `use std::sync::atomic::AtomicU64;`, `FsSearchParams` in the protocol list, and `use crate::search;` (`use crate::{docs, file_ops, files, search};`). After `fs_find`:

```rust
    /// `fs.search`, started by the connection's read loop (see `Daemon::connection`): the
    /// target is resolved now, the walk runs on a blocking thread and the caller answers when
    /// it ends. `generation` is the connection's counter, already bumped to `mine`: a later
    /// `fs.search` of the connection bumps it again and this walk stops.
    pub(super) async fn fs_search_start(
        &self,
        params: FsSearchParams,
        generation: Arc<AtomicU64>,
        mine: u64,
    ) -> Result<tokio::task::JoinHandle<Outcome>, Outcome> {
        let target = self
            .watched_target(params.project_id, params.worktree_task_id, None)
            .await?;
        Ok(tokio::task::spawn_blocking(move || {
            let searched = search::search(
                &target.dir,
                &params.query,
                params.regex,
                params.limit as usize,
                &generation,
                mine,
            );
            match searched {
                Ok(result) => ok(&result),
                Err(error @ search::Error::Pattern(_)) => {
                    Outcome::error(ErrorCode::InvalidRequest, error.to_string())
                }
                Err(search::Error::Cancelled) => Outcome::error(
                    ErrorCode::InvalidState,
                    "fs.search was replaced by a newer search",
                ),
            }
        }))
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-daemon --test daemon fs_search_finds_text`
Expected: PASS. Also `cargo test -p mkrew-daemon --test daemon fs_list_read_and_find` (the serial path is unchanged).
Then the gate for Tasks 4 and 5 together: `cargo clippy -p mkrew-daemon --all-targets -- -D warnings` (clean: `search::search` has its caller now).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/server/fs.rs crates/mkrew-daemon/tests/daemon
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): fs.search beside the read loop, stopped by a newer search"
```


---

### Task 6: Move the picker key map into `palette.rs`

`quick_open.rs` holds three things other modules depend on: `Key`, `key(&Keystroke)` and `match_ranges`.
They move to `palette.rs` (Plan P1 asked for `match_ranges` there, and its `settings_dialog.rs` imports it), every caller is migrated, and `quick_open.rs` keeps only `QuickOpen` until Task 13 deletes it.

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs` (imports lines 7-19; new items after `word_start`, line 853)
- Modify: `crates/mkrew-ui/src/palette/tests.rs` (append two moved tests)
- Modify: `crates/mkrew-ui/src/quick_open.rs` (remove the three items and their tests, fix imports)
- Modify: `crates/mkrew-ui/src/slash_picker.rs:4-14`, `prompt_picker.rs:6`, `settings_dialog.rs` (the P1 import)
- Modify: `crates/mkrew-ui/src/main.rs:100`, `shell/{composer,docs,git_menus,jira,navigation,slash}.rs` (every `quick_open::` path)

**Interfaces:**
- Produces: `palette::Key` (same variants as today: `Close`, `Move(i32)`, `Open`, `Paste`, `Insert(String)`, `Edit(DialogKey)`), `palette::key(&Keystroke) -> Option<Key>`, `palette::match_ranges(&str, &[u32]) -> Vec<Range<usize>>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/mkrew-ui/src/palette/tests.rs` (they are the `keys` and `matched_chars_become_merged_byte_ranges` tests of `quick_open.rs`, moved):

```rust
#[test]
fn the_picker_keys() {
    let parse = |text: &str| key(&Keystroke::parse(text).unwrap());
    assert_eq!(parse("up"), Some(Key::Move(-1)));
    assert_eq!(parse("down"), Some(Key::Move(1)));
    assert_eq!(parse("escape"), Some(Key::Close));
    assert_eq!(parse("enter"), Some(Key::Open));
    assert_eq!(parse("ctrl-enter"), Some(Key::Open));
    assert_eq!(parse("backspace"), Some(Key::Edit(DialogKey::Backspace)));
    assert_eq!(parse("ctrl-v"), Some(Key::Paste));
    assert_eq!(
        parse("ctrl-backspace"),
        Some(Key::Edit(DialogKey::DeleteWord))
    );
    assert_eq!(parse("ctrl-a"), Some(Key::Edit(DialogKey::SelectAll)));
    assert_eq!(parse("tab"), None);
    assert_eq!(parse("ctrl-up"), None);
    assert_eq!(parse("ctrl-x"), Some(Key::Edit(DialogKey::Cut)));
}

#[test]
fn matched_chars_become_merged_byte_ranges() {
    assert_eq!(
        match_ranges("src/main.rs", &[4, 5, 6, 9]),
        vec![4..7, 9..10]
    );
    // Multi-byte chars: indices are chars, ranges are bytes.
    assert_eq!(match_ranges("é/ab", &[0, 1, 3]), vec![0..3, 4..5]);
    assert_eq!(match_ranges("ab", &[5]), Vec::<Range<usize>>::new());
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui palette::tests::the_picker_keys`
Expected: FAIL to compile (`key` and `Keystroke` not found in `palette`).

- [ ] **Step 3: Move the code**

`palette.rs`: the import block (lines 7-19) becomes

```rust
use std::ops::Range;

use gpui::Keystroke;
use mkrew_core::settings::FeatureSettings;
use mkrew_protocol::{Project, Session, SessionState, Task};
use ulid::Ulid;

use crate::board::CardKey;
use crate::board::View;
use crate::history;
use crate::keymap::{Command, Keymap, KeymapError};
use crate::split::Direction;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::Input;
```

After `fn word_start` add:

```rust
/// What a key press in a picker's query field means: the palette, the prompt and `/` pickers
/// and the filters of the Docs, Git and Jira lists share this map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// Esc.
    Close,
    /// ↑ / ↓ (-1 / +1).
    Move(i32),
    /// Enter (or Ctrl+Enter).
    Open,
    /// Ctrl+V: the caller inserts the clipboard text.
    Paste,
    Insert(String),
    /// An editing or caret key for [`Input::edit`] (Backspace, Ctrl+A, ← →, Ctrl+Z…). On
    /// Copy / Cut the caller puts the selection on the clipboard first.
    Edit(DialogKey),
}

/// The picker meaning of `keystroke`; `None` = ignored.
pub fn key(keystroke: &Keystroke) -> Option<Key> {
    if !keystroke.modifiers.modified() {
        match keystroke.key.as_str() {
            "up" => return Some(Key::Move(-1)),
            "down" => return Some(Key::Move(1)),
            _ => {}
        }
    }
    match dialog_key(keystroke)? {
        DialogKey::Cancel => Some(Key::Close),
        DialogKey::Enter | DialogKey::Save => Some(Key::Open),
        DialogKey::Paste => Some(Key::Paste),
        DialogKey::Insert(text) => Some(Key::Insert(text)),
        DialogKey::SwitchField => None,
        key => Some(Key::Edit(key)),
    }
}

/// The byte ranges of `path` to highlight for the matched char `indices` (as `fs.find`
/// returns them), adjacent chars merged; indices past the end are ignored.
pub fn match_ranges(path: &str, indices: &[u32]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (index, (start, c)) in path.char_indices().enumerate() {
        let matched = u32::try_from(index).is_ok_and(|index| indices.contains(&index));
        if !matched {
            continue;
        }
        let end = start + c.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == start => last.end = end,
            _ => ranges.push(start..end),
        }
    }
    ranges
}
```

`quick_open.rs`: delete the `Key` enum and its doc (lines 17-32), `fn key` (34-51), `fn match_ranges` (174-190), and the tests `keys` and `matched_chars_become_merged_byte_ranges` (lines 314-343). The header imports become

```rust
use std::sync::atomic::{AtomicU64, Ordering};

use mkrew_protocol::{FsFindParams, FsMatch};
use ulid::Ulid;

use crate::composer::Target;
use crate::palette::Key;
use crate::text_box::Input;
```

and the test module starts `use super::*;` then `use crate::task_dialog::DialogKey;`.

Other lib files: `slash_picker.rs` lines 13-14 (`use crate::palette::fuzzy;` and `use crate::quick_open::Key;`) become the single line `use crate::palette::{Key, fuzzy};`; the doc comments of `slash_picker.rs:5` and `prompt_picker.rs:6` name [`crate::palette::key`]; if Plan P1 landed, `settings_dialog.rs` `use crate::quick_open::match_ranges;` becomes `use crate::palette::match_ranges;`.

Every `quick_open::` path in the shell becomes `palette::` (`quick_open::key(` to `palette::key(`, `quick_open::Key::` to `palette::Key::`, `quick_open::match_ranges(` to `palette::match_ranges(`) in `shell/composer.rs`, `docs.rs`, `git_menus.rs`, `jira.rs`, `navigation.rs` and `slash.rs`. `palette` is the module `main.rs` imports (`use mkrew_ui::palette::{self, Extra, Palette};`); a local variable named `palette` does not clash with the module path.
`main.rs:100` becomes `use mkrew_ui::quick_open::{QuickOpen, Results};`.

Check: `grep -rn "quick_open::" crates/mkrew-ui/src --include=*.rs` lists only `main.rs:100` and docs/comments.

- [ ] **Step 4: Run the tests and the build**

Run: `cargo test -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS and clean (no unused import left in `quick_open.rs`).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): move the picker key map and match_ranges into palette"
```

---

### Task 7a: Scope, the new actions, `Entry` and `Row`

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs` (`Action` line 323-340, `Entry` and `Row` lines 359-374, the `Entry { .. }` literals (nine become `Entry::new`, the tenth, the session row at line 773, is a struct update), `entries` lines 719-796, `Palette::rows`)
- Modify: `crates/mkrew-ui/src/palette/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (`run_palette_pick`: one temporary arm, see Step 3)

**Interfaces:**
- Produces:
  - `pub enum Scope { All, Commands, Sessions, Files, Search, Branches, Commits }` (`Clone, Copy, Debug, PartialEq, Eq, Hash`) with `Scope::ORDER: [Scope; 7]`, `title(self) -> &'static str`, `placeholder(self) -> &'static str`, `next(self)`, `prev(self)`.
  - `pub fn scope_step(&Keystroke) -> Option<isize>`: Tab = `Some(1)`, Shift+Tab = `Some(-1)`, anything else `None`.
  - `Action` (now `Clone`, no longer `Copy`) gains `OpenFile { path: String }`, `OpenSearchHit { path: String, line: u32 }`, `Checkout { repo: Option<String>, branch: String }`, `ShowCommit { repo: Option<String>, sha: String }` and `fn scope(&self) -> Scope`.
  - `Entry { action, title, shortcut, detail: Option<String>, age: Option<String>, marks: Vec<Range<usize>> }` and `Entry::new(action, title, shortcut)`.
  - `Row<'a> { entry: &'a Entry, ranges: Vec<Range<usize>>, section: Scope, detail: Option<&'a str>, age: Option<&'a str> }`.
  - `shell/navigation.rs` `run_palette_pick` gets one temporary arm for the four new variants (comment "wired in Task 11"; Task 11 replaces it with the real arms), so the bin and every commit compile: its final `match action` has no `_` arm.

- [ ] **Step 1: Write the failing tests**

Append to `palette/tests.rs`:

```rust
#[test]
fn tab_walks_the_scopes_in_order_and_wraps() {
    assert_eq!(
        Scope::ORDER,
        [
            Scope::All,
            Scope::Commands,
            Scope::Sessions,
            Scope::Files,
            Scope::Search,
            Scope::Branches,
            Scope::Commits
        ]
    );
    let mut scope = Scope::All;
    let mut walked = Vec::new();
    for _ in 0..7 {
        scope = scope.next();
        walked.push(scope);
    }
    assert_eq!(walked[..6], Scope::ORDER[1..]);
    assert_eq!(scope, Scope::All, "Commits wraps to All");
    assert_eq!(Scope::All.prev(), Scope::Commits, "Shift+Tab from All wraps");
    assert_eq!(Scope::Files.prev(), Scope::Sessions);
}

#[test]
fn tab_and_shift_tab_are_the_scope_keys() {
    let step = |keys: &str| scope_step(&Keystroke::parse(keys).unwrap());
    assert_eq!(step("tab"), Some(1));
    assert_eq!(step("shift-tab"), Some(-1));
    assert_eq!(step("ctrl-tab"), None);
    assert_eq!(step("alt-tab"), None);
    assert_eq!(step("a"), None);
}

#[test]
fn an_entry_belongs_to_the_section_of_its_action() {
    let section = |action: Action| Entry::new(action, "x", "").action.scope();
    assert_eq!(section(Action::Run(Command::NewTask)), Scope::Commands);
    assert_eq!(section(Action::Theme(1)), Scope::Commands);
    assert_eq!(
        section(Action::GoTo(Target::Workspace(Ulid::new()))),
        Scope::Sessions
    );
    assert_eq!(
        section(Action::OpenFile { path: "a".into() }),
        Scope::Files
    );
    assert_eq!(
        section(Action::OpenSearchHit {
            path: "a".into(),
            line: 1
        }),
        Scope::Search
    );
    assert_eq!(
        section(Action::Checkout {
            repo: None,
            branch: "main".into()
        }),
        Scope::Branches
    );
    assert_eq!(
        section(Action::ShowCommit {
            repo: None,
            sha: "abc".into()
        }),
        Scope::Commits
    );
}

#[test]
fn a_row_carries_its_section_detail_and_age() {
    let entry = Entry {
        detail: Some("abc1234".into()),
        age: Some("3d".into()),
        ..Entry::new(
            Action::ShowCommit {
                repo: None,
                sha: "abc1234def".into(),
            },
            "fix login",
            "",
        )
    };
    let palette = Palette::new(vec![entry]);
    let rows = palette.rows();
    assert_eq!(rows[0].section, Scope::Commits);
    assert_eq!(rows[0].detail, Some("abc1234"));
    assert_eq!(rows[0].age, Some("3d"));
}
```

Also in the existing `go_to_workspaces_live_sessions_and_tasks`: replace the two shortcut asserts (lines 1042 and 1054)

```rust
    assert_eq!(working.and_then(|e| e.detail.as_deref()), Some("● working"));
    assert_eq!(working.map(|e| e.shortcut.as_str()), Some(""));
```

and

```rust
    assert_eq!(all[0].detail.as_deref(), Some("? needs input"));
```

The two exhaustive matches over `Action` in `palette/tests.rs` get the four new variants, or the lib tests do not compile.
In `commands()` (line 101) the `=> None` arm becomes

```rust
            Action::Extra(_)
            | Action::GoTo(_)
            | Action::Theme(_)
            | Action::NewAgentPane(_)
            | Action::RunCommand(..)
            | Action::OpenMachine(_)
            | Action::OpenFile { .. }
            | Action::OpenSearchHit { .. }
            | Action::Checkout { .. }
            | Action::ShowCommit { .. } => None,
```

and in the `kinds` map (line 748) add

```rust
            Action::OpenFile { .. }
            | Action::OpenSearchHit { .. }
            | Action::Checkout { .. }
            | Action::ShowCommit { .. } => 7,
```

The two `(entry.action, ..)` tuples built over `.iter()` (lines 75 and 1011) need `entry.action.clone()` now that `Action` is not `Copy`.

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui palette::tests`
Expected: FAIL to compile (`Scope`, `scope_step`, `Entry::new`, ... not found).

- [ ] **Step 3: Implement**

Add before `Action`:

```rust
/// What the palette searches (Polish spec 2026-11-21 §7.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scope {
    All,
    Commands,
    Sessions,
    Files,
    Search,
    Branches,
    Commits,
}

impl Scope {
    /// The tab order: Tab moves to the next scope, Shift+Tab to the previous one, wrapping.
    pub const ORDER: [Scope; 7] = [
        Scope::All,
        Scope::Commands,
        Scope::Sessions,
        Scope::Files,
        Scope::Search,
        Scope::Branches,
        Scope::Commits,
    ];

    /// The tab label and the section overline.
    pub fn title(self) -> &'static str {
        match self {
            Scope::All => "All",
            Scope::Commands => "Commands",
            Scope::Sessions => "Sessions",
            Scope::Files => "Files",
            Scope::Search => "Search",
            Scope::Branches => "Branches",
            Scope::Commits => "Commits",
        }
    }

    /// The query field's placeholder.
    pub fn placeholder(self) -> &'static str {
        match self {
            Scope::All => "Search commands, sessions, files, branches and commits",
            Scope::Commands => "Type a command",
            Scope::Sessions => "Type a workspace, session or task",
            Scope::Files => "Type part of a file name or path",
            Scope::Search => "Search the text of the files",
            Scope::Branches => "Type a branch name",
            Scope::Commits => "Type a commit message or hash",
        }
    }

    pub fn next(self) -> Scope {
        self.shifted(1)
    }

    pub fn prev(self) -> Scope {
        self.shifted(Self::ORDER.len() - 1)
    }

    fn shifted(self, by: usize) -> Scope {
        let at = Self::ORDER
            .iter()
            .position(|scope| *scope == self)
            .unwrap_or(0);
        Self::ORDER[(at + by) % Self::ORDER.len()]
    }
}

/// Tab (`Some(1)`) and Shift+Tab (`Some(-1)`) without another modifier: the scope switch of
/// the open palette. Every other key is not one.
pub fn scope_step(keystroke: &Keystroke) -> Option<isize> {
    let m = keystroke.modifiers;
    (keystroke.key == "tab" && !m.control && !m.alt && !m.platform)
        .then_some(if m.shift { -1 } else { 1 })
}
```

`Action`: change the derive to `#[derive(Debug, Clone, PartialEq, Eq)]`, add after `OpenMachine(usize),`:

```rust
    /// Files scope: open the file in the Files view (`@` insert mode types its path instead).
    OpenFile { path: String },
    /// Search scope: open the file with the caret on `line` (1-based).
    OpenSearchHit { path: String, line: u32 },
    /// Branches scope: `git switch` (after a question when something is at risk). `repo` is
    /// the repository of the Git view (ADR 0029), `None` = the workspace folder.
    Checkout { repo: Option<String>, branch: String },
    /// Commits scope: the Git view's History on that commit.
    ShowCommit { repo: Option<String>, sha: String },
```

and below the enum:

```rust
impl Action {
    /// The scope whose section lists this action.
    pub fn scope(&self) -> Scope {
        match self {
            Action::Run(_)
            | Action::Extra(_)
            | Action::Theme(_)
            | Action::NewAgentPane(_)
            | Action::RunCommand(..)
            | Action::OpenMachine(_) => Scope::Commands,
            Action::GoTo(_) => Scope::Sessions,
            Action::OpenFile { .. } => Scope::Files,
            Action::OpenSearchHit { .. } => Scope::Search,
            Action::Checkout { .. } => Scope::Branches,
            Action::ShowCommit { .. } => Scope::Commits,
        }
    }
}
```

`Entry` and `Row` (replace lines 359-374):

```rust
/// One palette line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub action: Action,
    /// Matched against the query, e.g. "Tabs: New shell tab".
    pub title: String,
    /// Shortcut label, e.g. "Ctrl+Shift+T"; empty if none.
    pub shortcut: String,
    /// Muted text after the title: a path, a short hash, a session state, a branch note.
    pub detail: Option<String>,
    /// Right-aligned age of a commit or branch (`git_history::age`).
    pub age: Option<String>,
    /// Byte ranges of `title` to highlight when the daemon ranked the entry (Files, Search);
    /// empty = highlight what the query matched.
    pub marks: Vec<Range<usize>>,
}

impl Entry {
    pub fn new(action: Action, title: impl Into<String>, shortcut: impl Into<String>) -> Self {
        Self {
            action,
            title: title.into(),
            shortcut: shortcut.into(),
            detail: None,
            age: None,
            marks: Vec::new(),
        }
    }
}

/// A shown entry with the byte ranges of its title the query matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row<'a> {
    pub entry: &'a Entry,
    pub ranges: Vec<Range<usize>>,
    /// The scope section the entry belongs to.
    pub section: Scope,
    pub detail: Option<&'a str>,
    pub age: Option<&'a str>,
}
```

The nine `Entry { .. }` literals of `palette.rs` that have no struct update become `Entry::new`:

```rust
// keymap_errors
.map(|error| {
    Entry::new(
        Action::Run(Command::OpenSettings),
        format!("Keymap error: {error}"),
        keymap.label(Command::OpenSettings),
    )
})

// settings_error
.map(|error| {
    Entry::new(
        Action::Run(Command::OpenSettings),
        format!("Settings error: {error}"),
        keymap.label(Command::OpenSettings),
    )
})

// theme_entries
Entry::new(
    Action::Theme(index),
    entry.name,
    if entry.name == current {
        format!("✓ {look}")
    } else {
        look.to_owned()
    },
)

// agent_entries
.map(|agent| {
    Entry::new(
        Action::NewAgentPane(agent.index),
        format!("Agent: New {} pane", agent.name),
        if agent.default {
            keymap.label(Command::SplitAgent)
        } else {
            String::new()
        },
    )
})

// run_entries
entries.push(Entry::new(
    Action::RunCommand(action, index),
    format!("Run: {verb} {}", row.name),
    "",
));

// machine_entries
.map(|(index, name)| {
    Entry::new(
        Action::OpenMachine(index),
        format!("Machine: Open {name}"),
        "",
    )
})

// entries(): commands, extras, go_to
.map(|command| Entry::new(Action::Run(command), title(command), keymap.label(command)))
...
.map(|extra| Entry::new(Action::Extra(extra), extra.title(), ""))
...
let go_to = |target, title: String| Entry::new(Action::GoTo(target), title, "");
```

and in the session branch of `entries()` the state moves from `shortcut` to `detail`:

```rust
            let entry = Entry {
                detail: Some(format!("{} {}", status.icon, status.label)),
                ..go_to(
                    Target::Session(session.id),
                    format!("Go to session: {} · {name}", history::label(session)),
                )
            };
```

`Palette::rows` builds the new `Row` fields (Task 7b replaces `rows` again; this keeps the build green now):

```rust
                Row {
                    entry,
                    ranges: match_ranges(&entry.title, indices),
                    section: entry.action.scope(),
                    detail: entry.detail.as_deref(),
                    age: entry.age.as_deref(),
                }
```

`palette.rs` needs `Keystroke` (Task 6 imported it). `picked()` returns `Some(self.entries[*index].action.clone())`.
Any existing test that uses an `Action` after moving it needs `.clone()`; the compiler lists them.

`shell/navigation.rs`: `run_palette_pick`'s final `match action` has no `_` arm, so the four new variants need one now.
After the `palette::Action::GoTo(palette::Target::Task { .. }) => { .. }` arm (the last one) add

```rust
            // Wired in Task 11, which replaces this arm; nothing produces these four before then.
            palette::Action::OpenFile { .. }
            | palette::Action::OpenSearchHit { .. }
            | palette::Action::Checkout { .. }
            | palette::Action::ShowCommit { .. } => {}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui palette` and `cargo build -p mkrew-ui --bins`
Expected: PASS; the bin compiles (`run_palette_pick` matches `action` by value, which still works: its patterns bind `Copy` parts, and the temporary arm covers the new variants).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette crates/mkrew-ui/src/shell/navigation.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): palette scopes, new actions and entry detail"
```

---

### Task 7b: The scoped palette engine (sources, serial drop, grouping)

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs` (imports; replace `Palette` and `impl Palette`, lines 855-980; helpers after it)
- Modify: `crates/mkrew-ui/src/theme.rs` (two debounce tokens after `REHIGHLIGHT_DELAY`, line 293)
- Modify: `crates/mkrew-ui/src/palette/tests.rs`

**Interfaces:**
- Consumes: `theme::PALETTE_GROUP_LIMIT` (P1, `usize`), `Scope`, `Action`, `Entry`, `Row` (Task 7a), `FsFindParams`, `FsSearchParams`, `GitBranchesParams`, `GitLogParams`, `FsMatch`, `FsSearchResult`, `Branch`, `Commit` (Task 1 and existing).
- Produces:
  - `theme::PALETTE_FILES_DELAY: Duration` (80 ms), `theme::PALETTE_SEARCH_DELAY: Duration` (150 ms)
  - `palette::COMMIT_LIMIT: u32 = 500`, `palette::SEARCH_MIN_CHARS: usize = 2`
  - `Source { project_id, worktree_task_id, repo }` with `Source::new(project_id, worktree_task_id, repo)` (drops `repo` with a worktree)
  - `Fetch { scope, serial, delay: Duration, call: Call }`, `Call::{Files(FsFindParams), Search(FsSearchParams), Branches(GitBranchesParams), Commits(GitLogParams)}`, `Answer::{Files(Result<Vec<FsMatch>, String>), Search(Result<FsSearchResult, String>), Branches(Result<Vec<Branch>, String>), Commits(Result<Vec<Commit>, String>)}`
  - `Line<'a>::{Header(&'static str), Row { row: Row<'a>, pick: usize }, More { scope: Scope, hidden: usize, pick: usize }, Note { text: String, error: bool }}`
  - `Palette::{new(entries) (flat list), scoped(entries, source: Option<Source>, scope, recent: Vec<Action>), file_ref(source, target: composer::Target), scope(), tabs(), source(), insert_into(), set_scope(scope), step_scope(by: isize), wanted() -> Vec<Fetch>, is_current(scope, serial) -> bool, answer(serial, Answer, now_ms: i64) -> bool, lines() -> Vec<Line<'_>>, rows() -> Vec<Row<'_>>, picked() -> Option<Action>, picked_more() -> Option<Scope>, selected_line() -> usize}` plus the existing `query()`, `click`, `key`, `insert`, `step`, `pick`, `pick_entry`, `selected`.
  - `pub selected` indexes the selectable lines (rows and "+N more"), in the flat list the rows.

- [ ] **Step 1: Write the failing tests**

At the top of `palette/tests.rs` extend the imports:

```rust
use mkrew_protocol::{
    Branch, Commit, FsMatch, FsSearchResult, SearchHit, SessionKind, StateSource, TaskStatus,
};
```

Append the tests:

```rust
fn source() -> Source {
    Source::new(Ulid::new(), None, None)
}

fn command(n: usize) -> Entry {
    Entry::new(Action::Theme(n), format!("Theme {n}"), "")
}

fn session_entry(n: usize) -> Entry {
    Entry::new(
        Action::GoTo(Target::Session(Ulid::new())),
        format!("Go to session: s{n}"),
        "",
    )
}

fn matches_of(paths: &[&str]) -> Vec<FsMatch> {
    paths
        .iter()
        .map(|path| FsMatch {
            path: (*path).to_owned(),
            score: 1,
            indices: vec![],
        })
        .collect()
}

fn branch(name: &str, remote: bool, current: bool, at: i64) -> Branch {
    Branch {
        name: name.into(),
        remote,
        current,
        upstream: None,
        last_commit_at: at,
        subject: "work".into(),
    }
}

fn log_commit(hash: &str, summary: &str, at: i64) -> Commit {
    Commit {
        hash: hash.into(),
        parents: vec![],
        author_name: "t".into(),
        author_email: "t@t".into(),
        authored_at: at,
        summary: summary.into(),
        body: String::new(),
    }
}

fn headers(palette: &Palette) -> Vec<&'static str> {
    palette
        .lines()
        .into_iter()
        .filter_map(|line| match line {
            Line::Header(title) => Some(title),
            _ => None,
        })
        .collect()
}

fn scopes(fetches: &[Fetch]) -> Vec<Scope> {
    fetches.iter().map(|fetch| fetch.scope).collect()
}

#[test]
fn a_worktree_never_carries_a_repo() {
    let (project, task) = (Ulid::new(), Ulid::new());
    assert_eq!(Source::new(project, Some(task), Some("fe".into())).repo, None);
    assert_eq!(
        Source::new(project, None, Some("fe".into())).repo.as_deref(),
        Some("fe")
    );
}

#[test]
fn all_caps_each_section_and_adds_a_more_row() {
    let entries: Vec<Entry> = (0..8)
        .map(command)
        .chain((0..7).map(session_entry))
        .collect();
    let mut palette = Palette::scoped(entries, None, Scope::All, vec![]);
    palette.insert("e");
    assert_eq!(headers(&palette), ["Commands", "Sessions"]);
    let lines = palette.lines();
    let rows = lines
        .iter()
        .filter(|line| matches!(line, Line::Row { .. }))
        .count();
    assert_eq!(rows, 2 * theme::PALETTE_GROUP_LIMIT);
    let more: Vec<(Scope, usize)> = lines
        .iter()
        .filter_map(|line| match line {
            Line::More { scope, hidden, .. } => Some((*scope, *hidden)),
            _ => None,
        })
        .collect();
    assert_eq!(more, [(Scope::Commands, 3), (Scope::Sessions, 2)]);
}

#[test]
fn the_more_row_switches_to_its_scope() {
    let entries: Vec<Entry> = (0..8).map(command).collect();
    let mut palette = Palette::scoped(entries, None, Scope::All, vec![]);
    palette.insert("e");
    let pick = palette
        .lines()
        .into_iter()
        .find_map(|line| match line {
            Line::More { pick, .. } => Some(pick),
            _ => None,
        })
        .unwrap();
    palette.pick(pick);
    assert_eq!(palette.picked(), None);
    assert_eq!(palette.picked_more(), Some(Scope::Commands));
    palette.set_scope(Scope::Commands);
    assert_eq!(palette.rows().len(), 8);
}

#[test]
fn a_flat_palette_has_no_scopes() {
    let mut palette = Palette::new(vec![command(0)]);
    palette.step_scope(1);
    assert_eq!(palette.scope(), Scope::All);
    assert!(!palette.tabs());
    let mut scoped = Palette::scoped(vec![], None, Scope::All, vec![]);
    scoped.step_scope(1);
    assert_eq!(scoped.scope(), Scope::Commands);
    scoped.step_scope(-1);
    assert_eq!(scoped.scope(), Scope::All);
}

#[test]
fn search_never_runs_or_shows_in_all() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::All, vec![]);
    palette.insert("needle");
    let asked = palette.wanted();
    assert_eq!(
        scopes(&asked),
        [Scope::Files, Scope::Branches, Scope::Commits]
    );
    assert!(!headers(&palette).contains(&"Search"));
}

#[test]
fn files_are_asked_on_open_debounced_and_late_answers_are_dropped() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    let first = palette.wanted().remove(0);
    // Opening Files does not wait.
    assert_eq!(first.delay, Duration::ZERO);
    let Call::Files(params) = &first.call else {
        panic!("{:?}", first.call)
    };
    assert_eq!(
        (params.query.as_str(), params.limit),
        ("", FsFindParams::DEFAULT_LIMIT)
    );
    assert_eq!(palette.wanted(), [], "nothing new to ask");
    assert!(palette.answer(first.serial, Answer::Files(Ok(matches_of(&["README.md"]))), 0));

    palette.insert("ma");
    let second = palette.wanted().remove(0);
    assert_eq!(second.delay, theme::PALETTE_FILES_DELAY);
    assert!(palette.is_current(Scope::Files, second.serial));
    assert!(!palette.is_current(Scope::Files, first.serial));
    // The previous matches stay on screen while the new answer is awaited.
    assert_eq!(palette.rows().len(), 1);
    // The first query's answer arrives late: dropped.
    assert!(!palette.answer(first.serial, Answer::Files(Ok(matches_of(&["old.rs"]))), 0));
    assert!(palette.answer(
        second.serial,
        Answer::Files(Ok(matches_of(&["src/main.rs", "Makefile"]))),
        0
    ));
    assert_eq!(
        palette.picked(),
        Some(Action::OpenFile {
            path: "src/main.rs".into()
        })
    );
    palette.step(1);
    assert_eq!(
        palette.picked(),
        Some(Action::OpenFile {
            path: "Makefile".into()
        })
    );
    palette.step(1);
    assert_eq!(palette.selected, 0, "down from the bottom wraps");
}

/// An `@` picker closed and opened again: the old palette's late answer has another number,
/// so it never shows in the new one.
#[test]
fn a_closed_palettes_answer_never_lands_in_the_next() {
    let mut closed = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    closed.wanted();
    closed.insert("a");
    let late = closed.wanted().remove(0);
    let mut open = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    let current = open.wanted().remove(0);
    assert_ne!(late.serial, current.serial);
    assert!(!open.answer(late.serial, Answer::Files(Ok(matches_of(&["stale.rs"]))), 0));
    assert!(open.rows().is_empty());
    assert_eq!(
        open.lines(),
        [Line::Note {
            text: "Loading…".into(),
            error: false
        }]
    );
}

#[test]
fn search_waits_for_two_chars_debounces_and_clears_short_queries() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Search, vec![]);
    assert_eq!(palette.wanted(), []);
    palette.insert("a");
    assert_eq!(palette.wanted(), [], "one char is not searched");
    palette.insert("b");
    let asked = palette.wanted().remove(0);
    assert_eq!(asked.delay, theme::PALETTE_SEARCH_DELAY);
    let Call::Search(params) = &asked.call else {
        panic!("{:?}", asked.call)
    };
    assert_eq!(
        (params.query.as_str(), params.regex, params.limit),
        ("ab", false, 200)
    );
    let hit = SearchHit {
        path: "src/a.rs".into(),
        line: 3,
        preview: "let ab = 1;".into(),
        ranges: vec![(4, 6)],
    };
    let result = FsSearchResult {
        hits: vec![hit],
        truncated: false,
    };
    assert!(palette.answer(asked.serial, Answer::Search(Ok(result.clone())), 0));
    assert_eq!(
        palette.picked(),
        Some(Action::OpenSearchHit {
            path: "src/a.rs".into(),
            line: 3
        })
    );
    let rows = palette.rows();
    assert_eq!(rows[0].ranges, [4..6]);
    assert_eq!(rows[0].detail, Some("src/a.rs:3"));
    // Back to one char: the hits go, and a late answer for them is dropped.
    palette.key(Key::Edit(DialogKey::Backspace));
    palette.wanted();
    assert!(palette.rows().is_empty());
    assert!(!palette.answer(asked.serial, Answer::Search(Ok(result)), 0));
}

#[test]
fn branches_are_asked_once_and_filtered_here() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Branches, vec![]);
    let asked = palette.wanted();
    assert_eq!(scopes(&asked), [Scope::Branches]);
    let Call::Branches(params) = &asked[0].call else {
        panic!("{:?}", asked[0].call)
    };
    assert_eq!(params.repo, None);
    let listed = vec![
        branch("main", false, true, 1_000),
        branch("feature-x", false, false, 500),
        branch("origin/feature-y", true, false, 400),
    ];
    assert!(palette.answer(asked[0].serial, Answer::Branches(Ok(listed)), 60_000_000));
    palette.insert("fe");
    assert_eq!(palette.wanted(), [], "filtered here, never asked again");
    let rows = palette.rows();
    let titles: Vec<&str> = rows.iter().map(|row| row.entry.title.as_str()).collect();
    assert!(titles.contains(&"feature-x") && titles.contains(&"origin/feature-y"));
    assert!(!titles.contains(&"main"));
    assert!(rows.iter().all(|row| row.age.is_some()));
    assert_eq!(
        palette.picked(),
        Some(Action::Checkout {
            repo: None,
            branch: "feature-x".into()
        })
    );
}

#[test]
fn commits_are_one_log_of_500_found_by_message_or_hash() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Commits, vec![]);
    let asked = palette.wanted().remove(0);
    let Call::Commits(params) = &asked.call else {
        panic!("{:?}", asked.call)
    };
    assert_eq!((params.limit, params.path.clone()), (COMMIT_LIMIT, None));
    let log = vec![
        log_commit("abcdef0123456789", "fix login", 0),
        log_commit("1234567890abcdef", "add tests", 0),
    ];
    assert!(palette.answer(asked.serial, Answer::Commits(Ok(log)), 0));
    palette.insert("abcd");
    assert_eq!(palette.rows().len(), 1, "found by its hash");
    assert_eq!(
        palette.picked(),
        Some(Action::ShowCommit {
            repo: None,
            sha: "abcdef0123456789".into()
        })
    );
    assert_eq!(palette.rows()[0].detail, Some("abcdef0"));
    palette.key(Key::Edit(DialogKey::SelectAll));
    palette.insert("tests");
    assert_eq!(palette.rows()[0].entry.title, "add tests");
}

#[test]
fn all_lists_a_scope_only_once_it_has_rows_and_never_shows_waits() {
    let mut palette = Palette::scoped(vec![command(0)], Some(source()), Scope::All, vec![]);
    palette.insert("t");
    let asked = palette.wanted();
    assert_eq!(
        scopes(&asked),
        [Scope::Files, Scope::Branches, Scope::Commits]
    );
    // Files, Branches and Commits are still loading: All has no "Loading" lines, only rows.
    assert_eq!(headers(&palette), ["Commands"]);
    let branches = asked.iter().find(|f| f.scope == Scope::Branches).unwrap();
    palette.answer(
        branches.serial,
        Answer::Branches(Ok(vec![branch("feature-x", false, false, 0)])),
        0,
    );
    assert_eq!(headers(&palette), ["Commands", "Branches"]);
    // A failed scope shows its error in its own tab only.
    let files = asked.iter().find(|f| f.scope == Scope::Files).unwrap();
    palette.answer(files.serial, Answer::Files(Err("no such workspace".into())), 0);
    assert_eq!(headers(&palette), ["Commands", "Branches"]);
    palette.set_scope(Scope::Files);
    assert_eq!(
        palette.lines(),
        [Line::Note {
            text: "no such workspace".into(),
            error: true
        }]
    );
}

#[test]
fn a_background_answer_keeps_the_pick() {
    let entries: Vec<Entry> = (0..3).map(command).collect();
    let mut palette = Palette::scoped(entries, Some(source()), Scope::All, vec![]);
    palette.insert("t");
    let asked = palette.wanted();
    palette.step(1);
    let before = palette.picked();
    assert!(before.is_some());
    let branches = asked.iter().find(|f| f.scope == Scope::Branches).unwrap();
    palette.answer(
        branches.serial,
        Answer::Branches(Ok(vec![branch("t1", false, false, 0)])),
        0,
    );
    assert_eq!(palette.picked(), before);
}

#[test]
fn a_waiting_or_failed_scope_says_so() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    let asked = palette.wanted().remove(0);
    assert_eq!(
        palette.lines(),
        [Line::Note {
            text: "Loading…".into(),
            error: false
        }]
    );
    palette.answer(asked.serial, Answer::Files(Err("no such workspace".into())), 0);
    assert_eq!(
        palette.lines(),
        [Line::Note {
            text: "no such workspace".into(),
            error: true
        }]
    );
    assert_eq!(palette.picked(), None);
}

#[test]
fn without_a_workspace_the_daemon_scopes_ask_for_one() {
    for scope in [Scope::Files, Scope::Search, Scope::Branches, Scope::Commits] {
        let mut palette = Palette::scoped(vec![], None, scope, vec![]);
        assert_eq!(palette.wanted(), []);
        assert_eq!(
            palette.lines(),
            [Line::Note {
                text: "Open a workspace to search it.".into(),
                error: false
            }],
            "{scope:?}"
        );
    }
}

#[test]
fn the_at_picker_lists_files_and_hands_the_path_to_its_prompt() {
    let target = crate::composer::Target::TaskDialog;
    let mut palette = Palette::file_ref(source(), target);
    assert_eq!(palette.insert_into(), Some(target));
    assert_eq!((palette.scope(), palette.tabs()), (Scope::Files, false));
    let asked = palette.wanted();
    assert_eq!(scopes(&asked), [Scope::Files]);
    palette.answer(asked[0].serial, Answer::Files(Ok(matches_of(&["src/a.rs"]))), 0);
    assert_eq!(
        palette.picked(),
        Some(Action::OpenFile {
            path: "src/a.rs".into()
        })
    );
    assert_eq!(Palette::new(vec![]).insert_into(), None);
}

#[test]
fn an_empty_all_lists_sessions_then_the_recent_commands() {
    let entries = vec![
        command(0),
        command(1),
        command(2),
        session_entry(0),
        session_entry(1),
    ];
    let recent = vec![Action::Theme(2), Action::Theme(0)];
    let palette = Palette::scoped(entries, None, Scope::All, recent);
    assert_eq!(headers(&palette), ["Sessions", "Recent"]);
    let titles: Vec<String> = palette
        .rows()
        .iter()
        .map(|row| row.entry.title.clone())
        .collect();
    assert_eq!(
        titles,
        ["Go to session: s0", "Go to session: s1", "Theme 2", "Theme 0"]
    );
}

#[test]
fn every_row_kind_yields_its_action() {
    let one = |scope: Scope, answer: Answer| {
        let mut palette = Palette::scoped(vec![], Some(source()), scope, vec![]);
        palette.insert("ab");
        let serial = palette.wanted().remove(0).serial;
        assert!(palette.answer(serial, answer, 0));
        palette.picked()
    };
    assert_eq!(
        one(Scope::Files, Answer::Files(Ok(matches_of(&["ab.rs"])))),
        Some(Action::OpenFile {
            path: "ab.rs".into()
        })
    );
    let hit = SearchHit {
        path: "ab.rs".into(),
        line: 7,
        preview: "ab".into(),
        ranges: vec![],
    };
    let result = FsSearchResult {
        hits: vec![hit],
        truncated: false,
    };
    assert_eq!(
        one(Scope::Search, Answer::Search(Ok(result))),
        Some(Action::OpenSearchHit {
            path: "ab.rs".into(),
            line: 7
        })
    );
    assert_eq!(
        one(
            Scope::Branches,
            Answer::Branches(Ok(vec![branch("ab", false, false, 0)]))
        ),
        Some(Action::Checkout {
            repo: None,
            branch: "ab".into()
        })
    );
    assert_eq!(
        one(
            Scope::Commits,
            Answer::Commits(Ok(vec![log_commit("ab12", "ab", 0)]))
        ),
        Some(Action::ShowCommit {
            repo: None,
            sha: "ab12".into()
        })
    );
    let mut commands = Palette::scoped(vec![command(4)], None, Scope::Commands, vec![]);
    commands.insert("theme");
    assert_eq!(commands.picked(), Some(Action::Theme(4)));
    let mut sessions = Palette::scoped(vec![session_entry(1)], None, Scope::Sessions, vec![]);
    sessions.insert("s1");
    assert!(matches!(
        sessions.picked(),
        Some(Action::GoTo(Target::Session(_)))
    ));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui palette::tests`
Expected: FAIL to compile (`Source`, `Fetch`, `Palette::scoped`, ... not found).

- [ ] **Step 3: Implement**

`theme.rs`, after `REHIGHLIGHT_DELAY`:

```rust
/// The palette's Files scope asks `fs.find` this long after the last keystroke (spec 7.2).
pub const PALETTE_FILES_DELAY: Duration = Duration::from_millis(80);

/// The palette's Search scope asks `fs.search` this long after the last keystroke.
pub const PALETTE_SEARCH_DELAY: Duration = Duration::from_millis(150);
```

`palette.rs` imports (Task 6's block) gain

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
```
(after `use std::ops::Range;`),
`use mkrew_protocol::{Branch, Commit, FsFindParams, FsMatch, FsSearchParams, FsSearchResult, GitBranchesParams, GitLogParams, Project, SearchHit, Session, SessionState, Task};` and `use crate::theme;` (after `use crate::text_box::Input;`).

Replace lines 855-980 (`/// The open palette.` through the end of `impl Palette`) with:

```rust
/// The commits the Commits scope lists (one `git.log` per opening, filtered here), and how
/// many the History reads after a "Show commit" pick so the commit is listed there too.
pub const COMMIT_LIMIT: u32 = 500;

/// The Search scope asks the daemon from this many characters on.
pub const SEARCH_MIN_CHARS: usize = 2;

/// What Files, Search, Branches and Commits say without a workspace.
const NO_WORKSPACE: &str = "Open a workspace to search it.";

/// The folder the daemon scopes read (spec 7.2): the active workspace, the task worktree its
/// Files view shows (what Quick open read) and, outside a worktree, the repository its Git
/// view shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub project_id: Ulid,
    pub worktree_task_id: Option<Ulid>,
    pub repo: Option<String>,
}

impl Source {
    /// The daemon refuses `repo` together with a worktree (ADR 0029), so a worktree drops it.
    pub fn new(project_id: Ulid, worktree_task_id: Option<Ulid>, repo: Option<String>) -> Self {
        Self {
            project_id,
            repo: if worktree_task_id.is_some() { None } else { repo },
            worktree_task_id,
        }
    }
}

/// What a daemon scope holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    /// Nothing asked yet.
    Idle,
    Loading,
    Ready,
    Failed(String),
}

/// A daemon call the palette wants made now. The shell sends it after `delay` (when the
/// palette still wants it, see [`Palette::is_current`]) and gives the answer back to
/// [`Palette::answer`] with the same `serial`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetch {
    pub scope: Scope,
    pub serial: u64,
    pub delay: Duration,
    pub call: Call,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Files(FsFindParams),
    Search(FsSearchParams),
    Branches(GitBranchesParams),
    Commits(GitLogParams),
}

/// The daemon's answer to a [`Call`]; an error is its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    Files(Result<Vec<FsMatch>, String>),
    Search(Result<FsSearchResult, String>),
    Branches(Result<Vec<Branch>, String>),
    Commits(Result<Vec<Commit>, String>),
}

/// One drawn line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line<'a> {
    /// A section overline (All).
    Header(&'static str),
    /// A row; `pick` is its place among the selectable lines (for a click).
    Row { row: Row<'a>, pick: usize },
    /// "+N more": picking it switches to `scope`.
    More {
        scope: Scope,
        hidden: usize,
        pick: usize,
    },
    /// A muted line: a wait, a failure, a missing workspace.
    Note { text: String, error: bool },
}

/// Where a shown item's entry lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Store {
    Local,
    Files,
    Search,
    Branches,
    Commits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Header(&'static str),
    Item {
        store: Store,
        index: usize,
        ranges: Vec<Range<usize>>,
    },
    More(Scope, usize),
    Note(String, bool),
}

impl Slot {
    fn selectable(&self) -> bool {
        matches!(self, Slot::Item { .. } | Slot::More(..))
    }
}

/// One daemon scope: its rows, what it is doing, and the number of the last call, so an
/// older answer is dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Remote {
    load: Load,
    rows: Vec<Entry>,
    /// The query the last call carried (Files, Search).
    asked: Option<String>,
    serial: u64,
}

impl Remote {
    fn new() -> Self {
        Self {
            load: Load::Idle,
            rows: Vec::new(),
            asked: None,
            serial: 0,
        }
    }

    /// A call is about to be sent: the scope waits for it, under a number no other call of
    /// any palette has.
    fn start(&mut self, query: Option<&str>) -> u64 {
        self.load = Load::Loading;
        self.asked = query.map(str::to_owned);
        self.serial = next_serial();
        self.serial
    }
}

/// Unique across palettes, so an answer meant for a closed one never lands in the next.
fn next_serial() -> u64 {
    static SENT: AtomicU64 = AtomicU64::new(0);
    SENT.fetch_add(1, Ordering::Relaxed) + 1
}

/// The open palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    /// Commands and Sessions, listed by the shell when the palette opened.
    entries: Vec<Entry>,
    /// Private: every edit must lay out again ([`Self::key`], [`Self::insert`]).
    query: Input,
    scope: Scope,
    /// The scope tabs show and Tab switches them.
    tabs: bool,
    /// Scopes and daemon sources work. Only the theme picker is one flat list instead (a
    /// detached window's palette is scoped too).
    scoped: bool,
    source: Option<Source>,
    /// Opened by `@` in a prompt: Enter on a file types its path there instead of opening it.
    insert_into: Option<crate::composer::Target>,
    /// The commands run from the palette lately, newest first.
    recent: Vec<Action>,
    files: Remote,
    hits: Remote,
    branches: Remote,
    commits: Remote,
    shown: Vec<Slot>,
    /// Index among the selectable lines (rows and "+N more") of the picked one.
    pub selected: usize,
}

impl Palette {
    fn base(entries: Vec<Entry>, scope: Scope) -> Self {
        Self {
            entries,
            query: Input::default(),
            scope,
            tabs: false,
            scoped: false,
            source: None,
            insert_into: None,
            recent: Vec::new(),
            files: Remote::new(),
            hits: Remote::new(),
            branches: Remote::new(),
            commits: Remote::new(),
            shown: Vec::new(),
            selected: 0,
        }
    }

    /// A flat list of `entries` filtered by the query: the theme picker (every other palette,
    /// a detached window's included, is [`Self::scoped`]).
    pub fn new(entries: Vec<Entry>) -> Self {
        let mut palette = Self::base(entries, Scope::All);
        palette.layout(false);
        palette
    }

    /// The scoped palette on `scope`. `source` is what Files, Search, Branches and Commits
    /// read (`None`: they ask for a workspace), `recent` the commands run from it lately.
    pub fn scoped(
        entries: Vec<Entry>,
        source: Option<Source>,
        scope: Scope,
        recent: Vec<Action>,
    ) -> Self {
        let mut palette = Self {
            tabs: true,
            scoped: true,
            source,
            recent,
            ..Self::base(entries, scope)
        };
        palette.layout(false);
        palette
    }

    /// The `@` picker of a prompt: Files only, no tabs; Enter hands the path to `target`.
    pub fn file_ref(source: Source, target: crate::composer::Target) -> Self {
        let mut palette = Self {
            scoped: true,
            source: Some(source),
            insert_into: Some(target),
            ..Self::base(Vec::new(), Scope::Files)
        };
        palette.layout(false);
        palette
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The scope tabs show (and Tab switches them).
    pub fn tabs(&self) -> bool {
        self.tabs
    }

    pub fn source(&self) -> Option<&Source> {
        self.source.as_ref()
    }

    /// The prompt an `@` pick types its path into.
    pub fn insert_into(&self) -> Option<crate::composer::Target> {
        self.insert_into
    }

    /// Shows `scope`, the pick on its first row.
    pub fn set_scope(&mut self, scope: Scope) {
        if self.scope != scope {
            self.scope = scope;
            self.layout(false);
        }
    }

    /// Tab (`by` > 0) or Shift+Tab: the next or previous scope, wrapping; nothing in a list
    /// without tabs.
    pub fn step_scope(&mut self, by: isize) {
        if self.tabs {
            let scope = if by < 0 {
                self.scope.prev()
            } else {
                self.scope.next()
            };
            self.set_scope(scope);
        }
    }

    /// Picks the row showing entry `entry_index` (the theme picker opens on the current theme).
    pub fn pick_entry(&mut self, entry_index: usize) {
        let at = self
            .shown
            .iter()
            .filter(|slot| slot.selectable())
            .position(|slot| {
                matches!(slot, Slot::Item { store: Store::Local, index, .. } if *index == entry_index)
            });
        if let Some(at) = at {
            self.selected = at;
        }
    }

    /// The typed query with its caret and selection.
    pub fn query(&self) -> &Input {
        &self.query
    }

    /// A press or drag in the query (see [`Input::click`]).
    pub fn click(&mut self, at: usize, extend: bool, clicks: usize) {
        self.query.click(at, extend, clicks);
    }

    /// Applies a key; Esc, Enter and Ctrl+V are the caller's (close, [`Self::picked`],
    /// [`Self::insert`] the clipboard), and so is copying the selection before Copy / Cut.
    pub fn key(&mut self, key: Key) {
        match key {
            Key::Move(by) => self.step(by),
            Key::Edit(key) => {
                if self.query.edit(&key) {
                    self.layout(false);
                }
            }
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => {}
        }
    }

    /// Appends typed or pasted text (line breaks dropped) and lays the list out again.
    pub fn insert(&mut self, text: &str) {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        if self.query.insert(&line) {
            self.layout(false);
        }
    }

    /// The daemon calls the palette wants now, after it opened, a key or a scope switch:
    /// Files per query (no wait while the query is empty), Search per query from
    /// [`SEARCH_MIN_CHARS`] characters (only on its own scope), Branches and Commits once per
    /// opening, when their scope or All is first shown. Asking the same thing twice returns
    /// nothing. Too short a query for Search empties its hits.
    pub fn wanted(&mut self) -> Vec<Fetch> {
        let Some(source) = self.source.clone() else {
            return Vec::new();
        };
        let query = self.query.as_str().to_owned();
        let all = self.scope == Scope::All;
        let mut fetches = Vec::new();
        if (self.scope == Scope::Files || (all && !query.trim().is_empty()))
            && self.files.asked.as_deref() != Some(query.as_str())
        {
            let serial = self.files.start(Some(&query));
            fetches.push(Fetch {
                scope: Scope::Files,
                serial,
                delay: if query.is_empty() {
                    Duration::ZERO
                } else {
                    theme::PALETTE_FILES_DELAY
                },
                call: Call::Files(FsFindParams {
                    project_id: source.project_id,
                    worktree_task_id: source.worktree_task_id,
                    query: query.clone(),
                    limit: FsFindParams::DEFAULT_LIMIT,
                }),
            });
        }
        if self.scope == Scope::Search {
            if query.chars().count() >= SEARCH_MIN_CHARS {
                if self.hits.asked.as_deref() != Some(query.as_str()) {
                    let serial = self.hits.start(Some(&query));
                    fetches.push(Fetch {
                        scope: Scope::Search,
                        serial,
                        delay: theme::PALETTE_SEARCH_DELAY,
                        call: Call::Search(FsSearchParams {
                            project_id: source.project_id,
                            worktree_task_id: source.worktree_task_id,
                            query: query.clone(),
                            regex: false,
                            limit: FsSearchParams::DEFAULT_LIMIT,
                        }),
                    });
                }
            } else if self.hits.asked.is_some() {
                // Too short to search: the old hits go, and their late answer is dropped by
                // its number (a fresh `Remote` has number 0).
                self.hits = Remote::new();
                self.layout(false);
            }
        }
        if (self.scope == Scope::Branches || all) && self.branches.load == Load::Idle {
            let serial = self.branches.start(None);
            fetches.push(Fetch {
                scope: Scope::Branches,
                serial,
                delay: Duration::ZERO,
                call: Call::Branches(GitBranchesParams {
                    project_id: source.project_id,
                    worktree_task_id: source.worktree_task_id,
                    repo: source.repo.clone(),
                }),
            });
        }
        if (self.scope == Scope::Commits || all) && self.commits.load == Load::Idle {
            let serial = self.commits.start(None);
            fetches.push(Fetch {
                scope: Scope::Commits,
                serial,
                delay: Duration::ZERO,
                call: Call::Commits(GitLogParams {
                    project_id: source.project_id,
                    worktree_task_id: source.worktree_task_id,
                    repo: source.repo.clone(),
                    limit: COMMIT_LIMIT,
                    path: None,
                }),
            });
        }
        if !fetches.is_empty() {
            self.layout(true);
        }
        fetches
    }

    /// Whether call number `serial` of `scope` is still the latest (the shell checks after a
    /// debounce, before it sends).
    pub fn is_current(&self, scope: Scope, serial: u64) -> bool {
        match scope {
            Scope::Files => self.files.serial == serial,
            Scope::Search => self.hits.serial == serial,
            Scope::Branches => self.branches.serial == serial,
            Scope::Commits => self.commits.serial == serial,
            Scope::All | Scope::Commands | Scope::Sessions => false,
        }
    }

    /// The answer to call number `serial`; kept only if no newer call of that scope was sent
    /// (the previous rows stay on screen until then). `now_ms` dates the ages. Returns `true`
    /// when the answer was kept. A Files or Search answer puts the pick on the best row; a
    /// background Branches or Commits answer keeps the pick where it was.
    pub fn answer(&mut self, serial: u64, answer: Answer, now_ms: i64) -> bool {
        let repo = self.source.as_ref().and_then(|source| source.repo.clone());
        let repo = repo.as_deref();
        let background = matches!(answer, Answer::Branches(_) | Answer::Commits(_));
        let (remote, rows) = match answer {
            Answer::Files(found) => (
                &mut self.files,
                found.map(|matches| matches.into_iter().map(file_entry).collect()),
            ),
            Answer::Search(found) => (
                &mut self.hits,
                found.map(|result| result.hits.into_iter().map(hit_entry).collect()),
            ),
            Answer::Branches(found) => (
                &mut self.branches,
                found.map(|branches| {
                    (branches.into_iter())
                        .map(|branch| branch_entry(branch, repo, now_ms))
                        .collect()
                }),
            ),
            Answer::Commits(found) => (
                &mut self.commits,
                found.map(|commits| {
                    (commits.into_iter())
                        .map(|commit| commit_entry(commit, repo, now_ms))
                        .collect()
                }),
            ),
        };
        if serial != remote.serial {
            return false;
        }
        match rows {
            Ok(rows) => {
                remote.rows = rows;
                remote.load = Load::Ready;
            }
            Err(error) => {
                remote.rows.clear();
                remote.load = Load::Failed(error);
            }
        }
        self.layout(background);
        true
    }

    /// The list as it is drawn: overlines, rows, "+N more" and notes.
    pub fn lines(&self) -> Vec<Line<'_>> {
        let mut pick = 0;
        let mut lines = Vec::with_capacity(self.shown.len());
        for slot in &self.shown {
            match slot {
                Slot::Header(title) => lines.push(Line::Header(title)),
                Slot::Note(text, error) => lines.push(Line::Note {
                    text: text.clone(),
                    error: *error,
                }),
                Slot::Item {
                    store,
                    index,
                    ranges,
                } => {
                    if let Some(entry) = self.entry(*store, *index) {
                        lines.push(Line::Row {
                            row: Row {
                                entry,
                                ranges: ranges.clone(),
                                section: entry.action.scope(),
                                detail: entry.detail.as_deref(),
                                age: entry.age.as_deref(),
                            },
                            pick,
                        });
                    }
                    pick += 1;
                }
                Slot::More(scope, hidden) => {
                    lines.push(Line::More {
                        scope: *scope,
                        hidden: *hidden,
                        pick,
                    });
                    pick += 1;
                }
            }
        }
        lines
    }

    /// The shown rows, best match first ("+N more" and notes left out).
    pub fn rows(&self) -> Vec<Row<'_>> {
        self.lines()
            .into_iter()
            .filter_map(|line| match line {
                Line::Row { row, .. } => Some(row),
                _ => None,
            })
            .collect()
    }

    /// The index in [`Self::lines`] of the picked line (what to scroll into view).
    pub fn selected_line(&self) -> usize {
        self.shown
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.selectable())
            .nth(self.selected)
            .map_or(0, |(line, _)| line)
    }

    /// Moves the pick by `by`, wrapping around the selectable lines.
    pub fn step(&mut self, by: i32) {
        let count = self.shown.iter().filter(|slot| slot.selectable()).count();
        if count == 0 {
            return;
        }
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let at = i64::try_from(self.selected).unwrap_or(0);
        self.selected = usize::try_from((at + i64::from(by)).rem_euclid(count)).unwrap_or(0);
    }

    /// Picks selectable line `index` (a click).
    pub fn pick(&mut self, index: usize) {
        if index < self.shown.iter().filter(|slot| slot.selectable()).count() {
            self.selected = index;
        }
    }

    /// What the picked row does; `None` on "+N more" or with nothing shown.
    pub fn picked(&self) -> Option<Action> {
        match self.picked_slot()? {
            Slot::Item { store, index, .. } => {
                self.entry(*store, *index).map(|entry| entry.action.clone())
            }
            _ => None,
        }
    }

    /// The scope of the picked "+N more" row, if that is the pick.
    pub fn picked_more(&self) -> Option<Scope> {
        match self.picked_slot()? {
            Slot::More(scope, _) => Some(*scope),
            _ => None,
        }
    }

    fn picked_slot(&self) -> Option<&Slot> {
        self.shown
            .iter()
            .filter(|slot| slot.selectable())
            .nth(self.selected)
    }

    fn entry(&self, store: Store, index: usize) -> Option<&Entry> {
        match store {
            Store::Local => self.entries.get(index),
            Store::Files => self.files.rows.get(index),
            Store::Search => self.hits.rows.get(index),
            Store::Branches => self.branches.rows.get(index),
            Store::Commits => self.commits.rows.get(index),
        }
    }

    /// Lays the list out again and picks its first row, or, with `keep`, the row of the
    /// action that was picked (a background answer must not move the pick).
    fn layout(&mut self, keep: bool) {
        let before = if keep { self.picked() } else { None };
        self.shown = self.slots();
        self.selected = 0;
        if let Some(action) = before {
            let at = self
                .shown
                .iter()
                .filter(|slot| slot.selectable())
                .position(|slot| {
                    matches!(slot, Slot::Item { store, index, .. }
                        if self.entry(*store, *index).is_some_and(|entry| entry.action == action))
                });
            if let Some(at) = at {
                self.selected = at;
            }
        }
    }

    fn slots(&self) -> Vec<Slot> {
        if !self.scoped {
            return ranked(Store::Local, &self.entries, &self.query, None);
        }
        if self.scope == Scope::All {
            return self.grouped();
        }
        let items = self.matches(self.scope);
        match self.note(self.scope) {
            Some(note) if items.is_empty() => vec![note],
            _ => items,
        }
    }

    /// The items of one scope for the query, best first.
    fn matches(&self, scope: Scope) -> Vec<Slot> {
        let query = self.query.as_str();
        match scope {
            // Ranked by the daemon: kept in its order, highlighted by its ranges.
            Scope::Files => ordered(Store::Files, &self.files.rows),
            Scope::Search => ordered(Store::Search, &self.hits.rows),
            Scope::Commands | Scope::Sessions => {
                ranked(Store::Local, &self.entries, query, Some(scope))
            }
            Scope::Branches => ranked(Store::Branches, &self.branches.rows, query, None),
            Scope::Commits => ranked(Store::Commits, &self.commits.rows, query, None),
            Scope::All => Vec::new(),
        }
    }

    /// Whether `scope` can have a section in All: every scope but Search. A scope without
    /// rows (not loaded yet, failed, no workspace) adds no section: its wait and its failure
    /// show in its own tab only ([`Self::group`] skips an empty one).
    fn joins_all(&self, scope: Scope) -> bool {
        !matches!(scope, Scope::All | Scope::Search)
    }

    /// All: with an empty query the sessions (waiting ones lead) and the recent commands;
    /// with a query one section per scope in [`Scope::ORDER`] except Search, each capped.
    fn grouped(&self) -> Vec<Slot> {
        let mut slots = Vec::new();
        if self.query.trim().is_empty() {
            let sessions = self.matches(Scope::Sessions);
            self.group(&mut slots, Scope::Sessions.title(), Scope::Sessions, sessions);
            let recent: Vec<Slot> = self
                .recent
                .iter()
                .filter_map(|action| {
                    let index = self.entries.iter().position(|e| e.action == *action)?;
                    Some(Slot::Item {
                        store: Store::Local,
                        index,
                        ranges: Vec::new(),
                    })
                })
                .collect();
            self.group(&mut slots, "Recent", Scope::Commands, recent);
        } else {
            for scope in Scope::ORDER.into_iter().filter(|s| self.joins_all(*s)) {
                self.group(&mut slots, scope.title(), scope, self.matches(scope));
            }
        }
        slots
    }

    /// One section of All: its overline, at most `PALETTE_GROUP_LIMIT` rows and a "+N more"
    /// row. A scope without rows is left out: All has no loading or error lines (spec 7.2).
    fn group(&self, slots: &mut Vec<Slot>, title: &'static str, scope: Scope, items: Vec<Slot>) {
        if items.is_empty() {
            return;
        }
        slots.push(Slot::Header(title));
        let hidden = items.len().saturating_sub(theme::PALETTE_GROUP_LIMIT);
        slots.extend(items.into_iter().take(theme::PALETTE_GROUP_LIMIT));
        if hidden > 0 {
            slots.push(Slot::More(scope, hidden));
        }
    }

    /// The muted line of a daemon scope with no row: the missing workspace, the wait, the
    /// failure, or (Search) how to start.
    fn note(&self, scope: Scope) -> Option<Slot> {
        let remote = match scope {
            Scope::Files => &self.files,
            Scope::Search => &self.hits,
            Scope::Branches => &self.branches,
            Scope::Commits => &self.commits,
            Scope::All | Scope::Commands | Scope::Sessions => return None,
        };
        if self.source.is_none() {
            return Some(Slot::Note(NO_WORKSPACE.into(), false));
        }
        match &remote.load {
            Load::Loading => Some(Slot::Note("Loading…".into(), false)),
            Load::Failed(error) => Some(Slot::Note(error.clone(), true)),
            Load::Idle if scope == Scope::Search => Some(Slot::Note(
                format!("Type {SEARCH_MIN_CHARS} characters to search the files."),
                false,
            )),
            Load::Idle | Load::Ready => None,
        }
    }
}

/// `rows` in their order with the daemon's highlight ranges.
fn ordered(store: Store, rows: &[Entry]) -> Vec<Slot> {
    rows.iter()
        .enumerate()
        .map(|(index, entry)| Slot::Item {
            store,
            index,
            ranges: entry.marks.clone(),
        })
        .collect()
}

/// The `rows` of `section` (all when `None`) that `query` matches, best score first, ties in
/// their order. A commit is found by its short hash too.
fn ranked(store: Store, rows: &[Entry], query: &str, section: Option<Scope>) -> Vec<Slot> {
    let mut scored: Vec<(i64, usize, Vec<u32>)> = rows
        .iter()
        .enumerate()
        .filter(|(_, entry)| section.is_none_or(|section| entry.action.scope() == section))
        .filter_map(|(index, entry)| {
            let title = fuzzy(query, &entry.title);
            let detail = (store == Store::Commits)
                .then(|| entry.detail.as_deref().and_then(|detail| fuzzy(query, detail)))
                .flatten();
            let (score, indices) = match (title, detail) {
                (Some(title), Some(detail)) if detail.0 > title.0 => (detail.0, Vec::new()),
                (Some(title), _) => title,
                (None, Some(detail)) => (detail.0, Vec::new()),
                (None, None) => return None,
            };
            Some((score, index, indices))
        })
        .collect();
    scored.sort_by_key(|(score, ..)| std::cmp::Reverse(*score));
    scored
        .into_iter()
        .map(|(_, index, indices)| Slot::Item {
            store,
            index,
            ranges: match_ranges(&rows[index].title, &indices),
        })
        .collect()
}

fn file_entry(found: FsMatch) -> Entry {
    let marks = match_ranges(&found.path, &found.indices);
    Entry {
        marks,
        ..Entry::new(
            Action::OpenFile {
                path: found.path.clone(),
            },
            found.path,
            "",
        )
    }
}

fn hit_entry(hit: SearchHit) -> Entry {
    let marks = (hit.ranges.iter())
        .map(|&(start, end)| start as usize..end as usize)
        .collect();
    Entry {
        detail: Some(format!("{}:{}", hit.path, hit.line)),
        marks,
        ..Entry::new(
            Action::OpenSearchHit {
                path: hit.path,
                line: hit.line,
            },
            hit.preview,
            "",
        )
    }
}

fn branch_entry(branch: Branch, repo: Option<&str>, now_ms: i64) -> Entry {
    let detail = if branch.current {
        format!("current · {}", branch.subject)
    } else {
        branch.subject
    };
    Entry {
        detail: Some(detail),
        age: Some(crate::git_history::age(branch.last_commit_at, now_ms)),
        ..Entry::new(
            Action::Checkout {
                repo: repo.map(str::to_owned),
                branch: branch.name.clone(),
            },
            branch.name,
            "",
        )
    }
}

fn commit_entry(commit: Commit, repo: Option<&str>, now_ms: i64) -> Entry {
    Entry {
        detail: Some(crate::git_history::short_hash(&commit.hash).to_owned()),
        age: Some(crate::git_history::age(commit.authored_at, now_ms)),
        ..Entry::new(
            Action::ShowCommit {
                repo: repo.map(str::to_owned),
                sha: commit.hash,
            },
            commit.summary,
            "",
        )
    }
}
```

Delete the `Row` construction `rows()` of Task 7a (the new `rows()` above replaces it) and the old private `filter`. `use crate::theme;` makes `crate::theme::THEMES` in `theme_entries` still work.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui palette` then `cargo test -p mkrew-ui` and `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS (the old flat-palette tests still pass: `Palette::new` keeps `rows`, `step`, `pick`, `pick_entry`, `picked`, `selected`).
`theme::PALETTE_GROUP_LIMIT` must exist (Plan P1, Task 1 of that plan); if P1 is not merged, stop here and merge P1 first.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): scoped palette model with daemon sources, serial drop and All grouping"
```

---

### Task 8: `Command::SearchInFiles`

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum line 145-146, `all()` line 330, `name()` line 440, `default_keys()` line 563, tests `spec_defaults` line 1186)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title` line 447, `available` line 522, new `scope_for` after `scope_step`)
- Modify: `crates/mkrew-ui/src/palette/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/sessions.rs:487` (the dispatch arm)

**Interfaces:**
- Produces: `Command::SearchInFiles` (`name` `search_in_files`, default keys `ctrl-alt-shift-f`, label `Ctrl+Alt+Shift+F`, palette title `Files: Search in files`, available with a workspace, no key context).
  `palette::scope_for(Command) -> Option<Scope>`: `QuickOpen` -> `Files`, `SearchInFiles` -> `Search`, `CommandPalette` -> `All`, every other command `None`; Task 11 opens the palette on that scope (spec 11: "Ctrl+Shift+O opens Files" is pinned here, not only by HUMAN_CHECKS).
  Until Task 11 the `SearchInFiles` dispatch arm opens the palette on All (`open_palette`); Task 11 gives it the Search scope.

- [ ] **Step 1: Write the failing tests**

In `keymap.rs` `spec_defaults`, after the `ctrl-shift-o` assert add

```rust
        assert_eq!(bound("ctrl-alt-shift-f"), Some(Command::SearchInFiles));
        assert_eq!(keymap.label(Command::SearchInFiles), "Ctrl+Alt+Shift+F");
        assert_eq!(Command::SearchInFiles.context(), None);
        assert_eq!(Command::from_name("search_in_files"), Some(Command::SearchInFiles));
```

and add a test next to `every_default_parses_and_is_unique`:

```rust
    #[test]
    fn ctrl_alt_shift_f_belongs_to_search_in_files_alone() {
        let keymap = Keymap::defaults();
        let taken: Vec<Command> = keymap
            .bindings()
            .iter()
            .filter(|(_, keys)| same_keys(keys, &keystroke("ctrl-alt-shift-f")))
            .map(|(command, _)| *command)
            .collect();
        assert_eq!(taken, [Command::SearchInFiles]);
    }
```

In `palette/tests.rs` `availability_follows_the_view`: add `Command::SearchInFiles,` to the loop that asserts commands are offered (`for command in [..]` at line 239, next to `Command::NewShellTab`), because it is offered whenever a workspace is open (`context(View::Terminals)` has one).
After the `commands(&none)` equality add `assert!(!commands(&none).contains(&Command::SearchInFiles));`.
Do not add it to the "not offered" loop at line 252.

Append the test of the opening scope:

```rust
#[test]
fn the_open_commands_name_the_scope_they_open_on() {
    assert_eq!(scope_for(Command::QuickOpen), Some(Scope::Files));
    assert_eq!(scope_for(Command::SearchInFiles), Some(Scope::Search));
    assert_eq!(scope_for(Command::CommandPalette), Some(Scope::All));
    assert_eq!(scope_for(Command::NewShellTab), None);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui keymap palette`
Expected: FAIL to compile (`Command::SearchInFiles`).

- [ ] **Step 3: Implement**

`keymap.rs`: the `QuickOpen` doc becomes `/// Open the palette on its Files scope: fuzzy-find a file of the active workspace and open it in the Editor view.` and after `QuickOpen,` add

```rust
    /// Open the palette on its Search scope: the text of the active workspace's files.
    SearchInFiles,
```

`all()`: after `Command::QuickOpen,` add `Command::SearchInFiles,`.
`name()`: after the `QuickOpen` arm `Command::SearchInFiles => "search_in_files".into(),`.
`default_keys()`: after the `QuickOpen` arm `Command::SearchInFiles => "ctrl-alt-shift-f".into(),`.

`palette.rs` `title`: after `Command::QuickOpen => "Files: Quick open file".into(),` add `Command::SearchInFiles => "Files: Search in files".into(),`; in `available` add `| Command::SearchInFiles` to the `context.workspace` group (after `| Command::QuickOpen`).

`shell/sessions.rs` after the `Command::QuickOpen` arm: `Command::SearchInFiles => self.open_palette(window, cx),`.

`palette.rs`, after `scope_step`:

```rust
/// The scope a palette-opening command opens on (`Ctrl+Shift+O` Files, `Ctrl+Alt+Shift+F`
/// Search, `Ctrl+Shift+P` All); `None` for every other command.
pub fn scope_for(command: Command) -> Option<Scope> {
    match command {
        Command::QuickOpen => Some(Scope::Files),
        Command::SearchInFiles => Some(Scope::Search),
        Command::CommandPalette => Some(Scope::All),
        _ => None,
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui` and `cargo build -p mkrew-ui --bins`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette crates/mkrew-ui/src/shell/sessions.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Command::SearchInFiles on ctrl-alt-shift-f"
```

---

### Task 9: `GitView::show_commit` (History on a commit older than 50)

**Files:**
- Modify: `crates/mkrew-ui/src/git_view.rs` (field after `log_again` line 668, `new` line 705, `show` line 986, `begin_log` line 1217 (its `limit:` line is 1228), new method after `select_commit` line 1207)
- Modify: `crates/mkrew-ui/src/git_view/tests.rs`

**Interfaces:**
- Consumes: `palette::COMMIT_LIMIT` (Task 7b), `History::select`, `GitView::show`.
- Produces: `GitView::show_commit(&mut self, hash: String) -> Requests`: the History on `hash`, its log read with `COMMIT_LIMIT` commits until the History is left; `Requests { log: true, show: Some(hash), .. }`.

- [ ] **Step 1: Write the failing test**

Append to `git_view/tests.rs`:

```rust
#[test]
fn show_commit_opens_the_history_on_a_commit_older_than_the_default_log() {
    let mut view = GitView::new(Ulid::new(), None);
    let requests = view.show_commit("old".into());
    assert_eq!(view.section(), Section::History);
    assert!(requests.log);
    assert_eq!(requests.show.as_deref(), Some("old"));
    assert_eq!(
        view.begin_log().map(|params| params.limit),
        Some(crate::palette::COMMIT_LIMIT)
    );
    // The log lists it: the selection stays and its files are not read twice.
    let requests = view.logged(Ok(vec![commit("new"), commit("old")]));
    assert_eq!(requests.show, None);
    assert_eq!(
        view.history
            .as_ref()
            .and_then(|history| history.selected.as_deref()),
        Some("old")
    );
    // Leaving the History puts the default length back.
    view.show(Section::Working);
    view.toggle_history();
    assert_eq!(view.begin_log().map(|params| params.limit), Some(50));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui git_view::tests::show_commit`
Expected: FAIL to compile (`show_commit`).

- [ ] **Step 3: Implement**

Field, after `log_again: bool,`:

```rust
    /// How many commits the History's `git.log` asks for: [`GitLogParams::DEFAULT_LIMIT`],
    /// more after [`Self::show_commit`] so the picked commit is listed.
    log_limit: u32,
```

`new`: `log_again: false,` is followed by `log_limit: GitLogParams::DEFAULT_LIMIT,`.
`begin_log`: `limit: self.log_limit,` instead of `limit: GitLogParams::DEFAULT_LIMIT,`.
`show`: right after `self.select_none();` add

```rust
        if section != Section::History {
            self.log_limit = GitLogParams::DEFAULT_LIMIT;
        }
```

New method after `select_commit`:

```rust
    /// The palette's "Show commit": the History with `hash` expanded. The log is read with
    /// [`crate::palette::COMMIT_LIMIT`] commits (until the History is left), because the
    /// default 50 may not list the commit and the selection would jump to the first one.
    pub fn show_commit(&mut self, hash: String) -> Requests {
        self.log_limit = crate::palette::COMMIT_LIMIT;
        let mut requests = self.show(Section::History);
        // Already in the History: read the longer log now.
        requests.log = true;
        requests.show = self.history.as_mut().and_then(|history| history.select(hash));
        requests
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui git_view`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): GitView show_commit reads a log long enough for the palette's commit"
```

---

### Task 10: Checkout question, agents in a folder, recent commands

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs` (imports; new items after `Source`'s impl)
- Modify: `crates/mkrew-ui/src/palette/tests.rs`

**Interfaces:**
- Produces:
  - `palette::checkout_question(branch: &str, changes: usize, agents: usize) -> Option<(String, String)>`: the confirm's message and detail, `None` when nothing is at risk.
  - `palette::CHECKOUT_ANSWERS: [&str; 2] = ["Switch", "Cancel"]`, `palette::confirms_checkout(answer: Option<usize>) -> bool`.
  - `palette::agents_in_folder(sessions: &[Session], project_id: Ulid, worktree_task_id: Option<Ulid>, has_worktree: impl Fn(Ulid) -> bool) -> usize`.
  - `palette::remember(recent: &mut Vec<Action>, action: &Action)`: records an `Action::Run` as the latest of `PALETTE_GROUP_LIMIT` recent ones.

- [ ] **Step 1: Write the failing tests**

Append to `palette/tests.rs`:

```rust
#[test]
fn a_checkout_asks_only_when_something_is_at_risk() {
    assert_eq!(checkout_question("main", 0, 0), None);
    let (message, detail) = checkout_question("main", 2, 0).unwrap();
    assert_eq!(message, "Switch to “main”?");
    assert_eq!(
        detail,
        "2 files with uncommitted changes. Git refuses the switch when a change would be overwritten."
    );
    let (_, detail) = checkout_question("main", 1, 0).unwrap();
    assert!(detail.starts_with("1 file with uncommitted changes."), "{detail}");
    let (_, detail) = checkout_question("main", 0, 1).unwrap();
    assert_eq!(
        detail,
        "1 live agent session runs in this folder; its files change under it."
    );
    let (_, detail) = checkout_question("main", 0, 3).unwrap();
    assert_eq!(
        detail,
        "3 live agent sessions run in this folder; their files change under them."
    );
    let (_, both) = checkout_question("main", 1, 2).unwrap();
    assert_eq!(both.lines().count(), 2);
    assert!(confirms_checkout(Some(0)));
    assert!(!confirms_checkout(Some(1)) && !confirms_checkout(None));
}

#[test]
fn agents_count_in_the_folder_the_checkout_changes() {
    let (app, other, task) = (Ulid::new(), Ulid::new(), Ulid::new());
    let live = |project, task_id| Session {
        task_id,
        ..session(project, None, SessionState::Working)
    };
    let shell = Session {
        kind: SessionKind::Shell,
        ..session(app, None, SessionState::Idle)
    };
    let sessions = vec![
        live(app, None),
        live(app, Some(task)),
        live(app, Some(Ulid::new())),
        live(other, None),
        session(app, None, SessionState::Exited),
        shell,
    ];
    // A task that has a worktree runs there, not in the project folder.
    let has_worktree = |id: Ulid| id == task;
    assert_eq!(agents_in_folder(&sessions, app, None, has_worktree), 2);
    assert_eq!(agents_in_folder(&sessions, app, Some(task), has_worktree), 1);
    assert_eq!(agents_in_folder(&sessions, other, None, has_worktree), 1);
}

#[test]
fn the_recent_commands_are_the_last_five_distinct_runs() {
    let mut recent = Vec::new();
    remember(&mut recent, &Action::GoTo(Target::Workspace(Ulid::new())));
    assert!(recent.is_empty(), "only commands are remembered");
    for command in [
        Command::NewTask,
        Command::OpenSettings,
        Command::NewTask,
        Command::ShowBoard,
        Command::ShowDiff,
        Command::NextTab,
        Command::PreviousTab,
    ] {
        remember(&mut recent, &Action::Run(command));
    }
    assert_eq!(
        recent,
        [
            Action::Run(Command::PreviousTab),
            Action::Run(Command::NextTab),
            Action::Run(Command::ShowDiff),
            Action::Run(Command::ShowBoard),
            Action::Run(Command::NewTask),
        ]
    );
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui palette::tests::a_checkout`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

Imports: `use mkrew_protocol::{.., SessionKind, ..}` gains `SessionKind`. After `impl Source`:

```rust
/// The answers of the checkout question, in `window.prompt` order.
pub const CHECKOUT_ANSWERS: [&str; 2] = ["Switch", "Cancel"];

/// Whether the checkout question's answer (an index into [`CHECKOUT_ANSWERS`]) switches.
pub fn confirms_checkout(answer: Option<usize>) -> bool {
    answer == Some(0)
}

/// The question before a checkout (message, detail), or `None` when nothing is at risk:
/// `changes` files with uncommitted changes in the repository (git refuses a switch that
/// would overwrite one, and the others follow to the new branch) and `agents` live agent
/// sessions running in that folder.
pub fn checkout_question(branch: &str, changes: usize, agents: usize) -> Option<(String, String)> {
    let mut risks = Vec::new();
    if changes > 0 {
        let files = if changes == 1 { "file" } else { "files" };
        risks.push(format!(
            "{changes} {files} with uncommitted changes. Git refuses the switch when a change would be overwritten."
        ));
    }
    match agents {
        0 => {}
        1 => risks.push(
            "1 live agent session runs in this folder; its files change under it.".to_owned(),
        ),
        n => risks.push(format!(
            "{n} live agent sessions run in this folder; their files change under them."
        )),
    }
    (!risks.is_empty()).then(|| (format!("Switch to “{branch}”?"), risks.join("\n")))
}

/// Live agent sessions that run in the folder a checkout changes: with no `worktree_task_id`
/// the project folder (sessions of tasks that have no worktree of their own, per
/// `has_worktree`), else that task's worktree.
pub fn agents_in_folder(
    sessions: &[Session],
    project_id: Ulid,
    worktree_task_id: Option<Ulid>,
    has_worktree: impl Fn(Ulid) -> bool,
) -> usize {
    sessions
        .iter()
        .filter(|session| {
            session.project_id == project_id
                && session.kind == SessionKind::Agent
                && session.state != SessionState::Exited
        })
        .filter(|session| match worktree_task_id {
            Some(task) => session.task_id == Some(task),
            None => session.task_id.is_none_or(|task| !has_worktree(task)),
        })
        .count()
}

/// Records a command run from the palette as the latest of the recent ones (All lists them
/// while the query is empty). Other actions are not remembered.
// ponytail: UI memory only; save it in the UI state file if users ask for it across restarts.
pub fn remember(recent: &mut Vec<Action>, action: &Action) {
    if !matches!(action, Action::Run(_)) {
        return;
    }
    recent.retain(|other| other != action);
    recent.insert(0, action.clone());
    recent.truncate(theme::PALETTE_GROUP_LIMIT);
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui palette`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): checkout question, agents in a folder and recent commands"
```


---

### Task 11: Shell wiring (open on a scope, fetches, picks, `@` mode, Quick open gone from the shell)

The shell stops owning a second finder.
It opens the palette on a scope, sends the `Fetch`es the model asks for, gives the answers back, runs the new actions and types a picked path into a prompt in `@` mode.
After this task nothing in `shell/*` or `main.rs` names `QuickOpen`; Task 12 draws the new layout and Task 13 deletes the file.
Shell code needs a GPUI window and has no unit test; the model behind it is pinned in Tasks 7a to 10, and `docs/HUMAN_CHECKS.md` (Task 14) lists what to look at.

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs` (`Call::failed`) and `palette/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (lines 6-68, 160-264, 383-539, 615-680, 715-955, 957-1113)
- Modify: `crates/mkrew-ui/src/main.rs` (import line 100, fields lines 843-846 and 850, the doc comment of `picker_host` line 906, render lines 1147-1152)
- Modify: `crates/mkrew-ui/src/shell/{focus,hosts,connection,detached_render,task_dialog,files_render,sessions,git_menus}.rs`

**Interfaces:**
- Consumes: everything Tasks 7a to 10 produce (`palette::scope_for` from Task 8 included), `Shell::{reveal_file, open_file_at, retarget_git, open_git, git_requests, refresh_git, git_view, in_window, prompt_typed, toast_error, toast_info, activate_workspace}`, `Connection::call`.
- Produces: `Call::failed(&self, error: impl Into<String>) -> Answer`; `Shell::{open_palette_on(scope, window, cx), palette_changed, run_palette_fetches, palette_source, insert_palette_pick, in_main_window, open_palette_file, checkout_branch, show_palette_commit}`; the field `Shell.palette_recent: Vec<palette::Action>`.
- Replaces Task 7a's temporary `run_palette_pick` arm for `OpenFile`, `OpenSearchHit`, `Checkout` and `ShowCommit` with the real ones.
- Routing: files, search hits, checkouts and commits picked in a detached window run in the main window (`in_main_window`), after `checkout_branch` activates the palette's workspace so `repo_changes` reads that workspace's summary.

- [ ] **Step 1: Write the failing test**

Append to `palette/tests.rs`:

```rust
#[test]
fn a_call_without_a_daemon_fails_in_its_own_scope() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    let fetch = palette.wanted().remove(0);
    let failed = fetch.call.failed("Not connected to the daemon.");
    assert!(matches!(failed, Answer::Files(Err(_))));
    assert!(palette.answer(fetch.serial, failed, 0));
    assert_eq!(
        palette.lines(),
        [Line::Note {
            text: "Not connected to the daemon.".into(),
            error: true
        }]
    );
    for call in [
        Call::Search(FsSearchParams {
            project_id: Ulid::new(),
            worktree_task_id: None,
            query: "ab".into(),
            regex: false,
            limit: 200,
        }),
        Call::Branches(GitBranchesParams {
            project_id: Ulid::new(),
            worktree_task_id: None,
            repo: None,
        }),
    ] {
        let kind = std::mem::discriminant(&call.failed("x"));
        let wanted = match call {
            Call::Search(_) => Answer::Search(Err(String::new())),
            _ => Answer::Branches(Err(String::new())),
        };
        assert_eq!(kind, std::mem::discriminant(&wanted));
    }
}
```

with `FsSearchParams` and `GitBranchesParams` added to the `mkrew_protocol` import of the test file (`FsFindParams` is already reachable through `use super::*`).

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p mkrew-ui palette::tests::a_call_without`
Expected: FAIL to compile (`Call::failed`).

- [ ] **Step 3: Implement `Call::failed`**

In `palette.rs` after `enum Call`:

```rust
impl Call {
    /// The answer for this call when it could not be sent (no daemon): the same kind as the
    /// real answer, carrying `error`, so the scope shows it as its muted failure row.
    pub fn failed(&self, error: impl Into<String>) -> Answer {
        let error = error.into();
        match self {
            Call::Files(_) => Answer::Files(Err(error)),
            Call::Search(_) => Answer::Search(Err(error)),
            Call::Branches(_) => Answer::Branches(Err(error)),
            Call::Commits(_) => Answer::Commits(Err(error)),
        }
    }
}
```

Run: `cargo test -p mkrew-ui palette::tests::a_call_without`
Expected: PASS.

- [ ] **Step 4: The shell fields (`main.rs`)**

Line 100: delete `use mkrew_ui::quick_open::{self, QuickOpen, Results};`.
Lines 843-846: delete the `quick_open` and `quick_open_scroll` fields with their doc comments.
After `palette_scroll: ScrollHandle,` (line 850) add:

```rust
    /// The commands run from the palette lately, newest first (UI memory only; All lists
    /// them while its query is empty).
    palette_recent: Vec<palette::Action>,
```

Lines 1147-1152: delete the whole `.children(self.quick_open ... render_quick_open ...)` block.

Line 906: the doc comment "The window the open `/` picker, prompt picker or `@` quick open is drawn in" becomes "... prompt picker or `@` file picker is drawn in".

`shell/connection.rs`: in the `Shell { .. }` literal (lines 476-477) delete `quick_open: None,` and `quick_open_scroll: ScrollHandle::new(),`; after `palette_scroll: ScrollHandle::new(),` add `palette_recent: Vec::new(),`; at line 793 delete `self.quick_open = None;`; the doc comment of `close_popups` (line 789) "menus, palette, quick open," becomes "menus, palette,".

`shell/focus.rs`: delete line 24 `|| self.quick_open.is_some()`; the doc comment of `picker_open` (line 32) "or the `@` quick open is open" becomes "or the `@` file picker is open"; `picker_open` becomes

```rust
    pub(crate) fn picker_open(&self) -> bool {
        self.prompt_picker.is_some() || self.slash_picker.is_some()
    }
```

and in `prompt_box_covered` delete `self.quick_open.is_some()` and its `||` (the chain starts at `self.prompt_picker.is_some()`).
`shell/hosts.rs` line 252: delete `self.quick_open = None;`.
`shell/detached_render.rs` lines 123-125: delete the `overlays.extend((self.quick_open ...))` statement.
`shell/task_dialog.rs` line 429: `&& self.quick_open.is_none()` becomes `&& self.palette.is_none()` (the `@` picker is a palette now and has the keyboard over the dialog).
`shell/git_menus.rs` line 142: `fn repo_changes` becomes `pub(crate) fn repo_changes`.
`shell/files_render.rs` line 78: `shell.open_quick_open(window, cx)` becomes `shell.open_palette_on(palette::Scope::Files, window, cx)`; update the comment on line 59 to "A button that opens the palette on its Files scope".
`shell/sessions.rs` (the match on `*command`): the `Command::QuickOpen => self.open_quick_open(window, cx),` arm (line 487) becomes

```rust
            Command::QuickOpen | Command::SearchInFiles => {
                let scope = palette::scope_for(*command).unwrap_or(palette::Scope::All);
                self.open_palette_on(scope, window, cx);
            }
```

(`scope_for` is Task 8's: `QuickOpen` opens Files, `SearchInFiles` opens Search; the `Command::SearchInFiles => self.open_palette(window, cx),` arm Task 8 added is deleted.)

- [ ] **Step 5: `navigation.rs`: open, close, `@`, keys, fetches**

Replace the module doc (line 1) with `//! Links, file reveal and the palette.`

Delete lines 6-68 (`open_quick_open`, `close_quick_open`, `open_file_ref_picker`) and put in their place:

```rust
    /// `@` in a prompt (a composer, the task dialog): the palette on Files in insert mode, over
    /// its workspace's files; the pick is typed as `composer.file_ref` (§20B.1). Without a
    /// daemon the `@` is typed as is.
    pub(crate) fn open_file_ref_picker(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let project_id = match target {
            composer::Target::Composer(session) => (self.sidebar.sessions().iter())
                .find(|s| s.id == session)
                .map(|s| s.project_id),
            composer::Target::TaskDialog => self.task_dialog.as_ref().map(TaskDialog::project_id),
        };
        let Some(project_id) = project_id.filter(|_| self.connection.is_some()) else {
            self.prompt_typed(target, "@", window, cx);
            cx.notify();
            return;
        };
        // Project-relative paths for the prompt, whatever the tree shows.
        let source = palette::Source::new(project_id, None, None);
        self.restore_theme_pick(window, cx);
        self.palette = Some(Palette::file_ref(source, target));
        self.palette_return = None;
        self.palette_host = self.host_of(window);
        // The task dialog under it stays the open dialog: its late replies (the agent list)
        // still land.
        if target != composer::Target::TaskDialog {
            self.dialog_serial += 1;
        }
        self.palette_scroll.scroll_to_item(0);
        window.focus(&self.dialog_focus, cx);
        self.palette_changed(window, cx);
    }
```

(the two `Source` args: an `@` path is relative to the project folder, so no worktree and no repo.)

Delete `find_files`, `quick_open_key_down` and `open_quick_pick` (lines 160-264).

Rename `open_palette` (line 385) to `open_palette_on` with a scope parameter, and keep the old name as a wrapper:

```rust
    /// The command palette (Ctrl+Shift+P) on All: the commands that can act where the keyboard
    /// is, palette-only actions, "Go to" items and the workspace's files, branches and commits.
    pub(crate) fn open_palette(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.open_palette_on(palette::Scope::All, window, cx);
    }

    /// The palette on `scope` (Quick open is Files, Search in files is Search).
    pub(crate) fn open_palette_on(
        &mut self,
        scope: palette::Scope,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
```

The tail of that function (lines 530-538) becomes

```rust
        self.restore_theme_pick(window, cx);
        self.workspace_menu = None;
        self.palette_host = self.host_of(window);
        let source = workspace.map(|project_id| self.palette_source(project_id));
        self.palette = Some(Palette::scoped(
            entries,
            source,
            scope,
            self.palette_recent.clone(),
        ));
        self.palette_return = window.focused(cx).map(|focus| (focus, terminal));
        self.palette_scroll.scroll_to_item(0);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        self.palette_changed(window, cx);
    }

    /// What the palette's Files, Search, Branches and Commits read for `project_id`: the task
    /// worktree its Files view shows (what Quick open read), and the repository its Git view
    /// shows while that view is on the same worktree (`Source::new` drops the repo of a
    /// worktree anyway).
    fn palette_source(&self, project_id: Ulid) -> palette::Source {
        let worktree = (self.files.get(&project_id)).and_then(|view| view.worktree_task_id);
        let repo = (self.git.get(&project_id))
            .filter(|view| view.worktree_task_id == worktree)
            .and_then(|view| view.repo.clone());
        palette::Source::new(project_id, worktree, repo)
    }

    /// After any change of the palette: sends the calls it now wants, keeps the pick in view
    /// (the list's children are its lines), previews a picked theme and redraws.
    pub(crate) fn palette_changed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let fetches = (self.palette.as_mut()).map_or_else(Vec::new, Palette::wanted);
        self.run_palette_fetches(fetches, window, cx);
        if let Some(palette) = self.palette.as_ref() {
            self.palette_scroll.scroll_to_item(palette.selected_line());
        }
        self.preview_theme_pick(cx);
        cx.notify();
    }

    /// Sends the daemon calls of the open palette. A call with a delay waits, then checks it
    /// is still the palette's latest (a newer keystroke made it stale); an answer goes back by
    /// its serial, so an old one, or one for a palette that closed, is dropped there.
    fn run_palette_fetches(
        &mut self,
        fetches: Vec<palette::Fetch>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if fetches.is_empty() {
            return;
        }
        let Some(connection) = self.connection.clone() else {
            if let Some(palette) = self.palette.as_mut() {
                for fetch in fetches {
                    let failed = fetch.call.failed("Not connected to the daemon.");
                    palette.answer(fetch.serial, failed, status::now_ms());
                }
            }
            return;
        };
        for fetch in fetches {
            let palette::Fetch {
                scope,
                serial,
                delay,
                call,
            } = fetch;
            let connection = connection.clone();
            cx.spawn_in(window, async move |this, cx| {
                if !delay.is_zero() {
                    cx.background_executor().timer(delay).await;
                    let current = this.update(cx, |shell, _| {
                        (shell.palette.as_ref()).is_some_and(|palette| palette.is_current(scope, serial))
                    });
                    if !current.unwrap_or(false) {
                        return;
                    }
                }
                let answer = match call {
                    palette::Call::Files(params) => palette::Answer::Files(
                        (connection.call::<Vec<FsMatch>>(Request::FsFind(params)).await)
                            .map_err(|error| format!("{error:#}")),
                    ),
                    palette::Call::Search(params) => palette::Answer::Search(
                        (connection
                            .call::<mkrew_protocol::FsSearchResult>(Request::FsSearch(params))
                            .await)
                            .map_err(|error| format!("{error:#}")),
                    ),
                    palette::Call::Branches(params) => palette::Answer::Branches(
                        (connection
                            .call::<Vec<mkrew_protocol::Branch>>(Request::GitBranches(params))
                            .await)
                            .map_err(|error| format!("{error:#}")),
                    ),
                    palette::Call::Commits(params) => palette::Answer::Commits(
                        (connection.call::<Vec<Commit>>(Request::GitLog(params)).await)
                            .map_err(|error| format!("{error:#}")),
                    ),
                };
                let _ = this.update(cx, |shell, cx| {
                    if let Some(palette) = shell.palette.as_mut()
                        && palette.answer(serial, answer, status::now_ms())
                    {
                        shell.palette_scroll.scroll_to_item(palette.selected_line());
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }
```

If clippy complains that `palette` is borrowed twice in the last closure, take `let line = palette.selected_line();` first.

`close_palette` (line 617) becomes:

```rust
    pub(crate) fn close_palette(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        // Esc from the theme picker: back to the theme it opened on.
        self.restore_theme_pick(window, cx);
        let insert_into = self.palette.take().and_then(|palette| palette.insert_into());
        self.palette_host = HostId::Main;
        // Esc on an `@` picker types the `@` it replaced (an e-mail, a handle).
        if let Some(target) = insert_into {
            self.palette_return = None;
            self.prompt_typed(target, "@", window, cx);
            cx.notify();
            return;
        }
        match self.palette_return.take() {
            Some((focus, session))
                if session.is_none_or(|session| self.panes.contains_key(&session)) =>
            {
                window.focus(&focus, cx);
            }
            _ => self.focus_pane(window, cx),
        }
        cx.notify();
    }
```

`palette_key_down` (lines 633-668) becomes:

```rust
    /// Typing filters, Tab / Shift+Tab switch the scope, ↑ / ↓ move the pick (kept in view),
    /// Enter runs it, Esc closes.
    pub(crate) fn palette_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        if let Some(by) = palette::scope_step(&event.keystroke) {
            if let Some(palette) = self.palette.as_mut() {
                palette.step_scope(by);
            }
            return self.palette_changed(window, cx);
        }
        let Some(key) = palette::key(&event.keystroke) else {
            return;
        };
        let paste = (key == palette::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(palette) = self.palette.as_mut() else {
            return;
        };
        match key {
            palette::Key::Close => return self.close_palette(window, cx),
            palette::Key::Open => return self.run_palette_pick(window, cx),
            palette::Key::Paste => {
                if let Some(text) = paste {
                    palette.insert(&text);
                }
            }
            palette::Key::Move(_) | palette::Key::Edit(_) | palette::Key::Insert(_) => {
                if let palette::Key::Edit(edit) = &key {
                    copy_selection(edit, palette.query(), cx);
                }
                palette.key(key);
            }
        }
        self.palette_changed(window, cx);
    }
```

- [ ] **Step 6: `navigation.rs`: picks**

Top of `run_palette_pick` (line 672), before `let Some(action) = ...`:

```rust
        // "+N more": the scope of that section.
        if let Some(scope) = self.palette.as_ref().and_then(Palette::picked_more) {
            if let Some(palette) = self.palette.as_mut() {
                palette.set_scope(scope);
            }
            return self.palette_changed(window, cx);
        }
        // `@` in a prompt: the path is typed there, nothing is opened.
        if let Some(target) = self.palette.as_ref().and_then(Palette::insert_into) {
            return self.insert_palette_pick(target, window, cx);
        }
```

and after the `let Some(action) = ... else { return; };` line add:

```rust
        let source = (self.palette.as_ref()).and_then(|palette| palette.source().cloned());
        palette::remember(&mut self.palette_recent, &action);
```

In the final `match action` (line 715), replace the temporary arm Task 7a added after the `palette::Action::GoTo(Task { .. })` arm (`OpenFile { .. } | OpenSearchHit { .. } | Checkout { .. } | ShowCommit { .. } => {}`) with:

```rust
            palette::Action::OpenFile { path } => {
                if let Some(source) = source {
                    self.open_palette_file(source, path, None, window, cx);
                }
            }
            palette::Action::OpenSearchHit { path, line } => {
                if let Some(source) = source {
                    self.open_palette_file(source, path, Some(line), window, cx);
                }
            }
            palette::Action::Checkout { repo, branch } => {
                if let Some(source) = source {
                    self.in_main_window(window, cx, move |shell, window, cx| {
                        shell.checkout_branch(source, repo, branch, window, cx);
                    });
                }
            }
            palette::Action::ShowCommit { repo, sha } => {
                if let Some(source) = source {
                    self.in_main_window(window, cx, move |shell, window, cx| {
                        shell.show_palette_commit(source, repo, sha, window, cx);
                    });
                }
            }
```

Add the helpers after `run_palette_pick`, in place of the deleted `render_quick_open` (lines 957-1113):

```rust
    /// Enter on an `@` picker: types the picked file as `composer.file_ref` into `target`.
    fn insert_palette_pick(
        &mut self,
        target: composer::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(palette::Action::OpenFile { path }) =
            self.palette.as_ref().and_then(Palette::picked)
        else {
            return;
        };
        self.palette = None;
        self.palette_host = HostId::Main;
        // A missing or broken settings file types the default form (the daemon reports it).
        let template = mkrew_core::settings::load(&self.settings_file)
            .unwrap_or_default()
            .composer
            .file_ref;
        let text = composer::file_ref(&template, &path);
        self.prompt_typed(target, &text, window, cx);
        cx.notify();
    }

    /// Runs `run` in the main window: at once when the palette was opened there, else after
    /// activating that window. Files, checkouts and commits all live in the main window (the
    /// Editor and Git views, the title bar summary), also when a detached window's palette
    /// picked them.
    fn in_main_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
        run: impl FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>) + 'static,
    ) {
        if self.host_of(window) == HostId::Main {
            return run(self, window, cx);
        }
        self.in_window(HostId::Main, cx, move |shell, window, cx| {
            window.activate_window();
            run(shell, window, cx);
        });
    }

    /// Files and Search picks: the file in the Editor view (its folders open, the cursor lands
    /// on it), the caret on `line` for a search hit. A detached window's pick opens in the
    /// main one.
    fn open_palette_file(
        &mut self,
        source: palette::Source,
        path: String,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (project, target) = (source.project_id, source.worktree_task_id);
        self.in_main_window(window, cx, move |shell, window, cx| {
            shell.open_file_at(project, target, path, line, window, cx);
        });
    }

    /// A Branches pick: `git.checkout`, after a question when something is at risk (changed
    /// files in the repository, live agents in that folder). A failure is a toast with git's
    /// own message; a success refreshes the Git view. Runs in the main window
    /// ([`Self::in_main_window`]) with the palette's workspace active, so `repo_changes` reads
    /// that workspace's summary.
    fn checkout_branch(
        &mut self,
        source: palette::Source,
        repo: Option<String>,
        branch: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if self.workspace != Some(source.project_id) {
            self.activate_workspace(source.project_id, window, cx);
        }
        // ponytail: a task worktree's own changes are not in the title bar summary, so no
        // question is asked there; git refuses a switch that would overwrite a change. The
        // same for a workspace whose summary has not loaded yet (`repo_changes` is `None`).
        let changes = if source.worktree_task_id.is_none() {
            self.repo_changes(repo.as_deref()).unwrap_or(0)
        } else {
            0
        };
        let has_worktree = |task: Ulid| {
            (self.board.task(task)).is_some_and(|task| task.worktree_path.is_some())
        };
        let agents = palette::agents_in_folder(
            self.sidebar.sessions(),
            source.project_id,
            source.worktree_task_id,
            has_worktree,
        );
        let answer = palette::checkout_question(&branch, changes, agents).map(|(message, detail)| {
            window.prompt(
                PromptLevel::Warning,
                &message,
                Some(detail.as_str()),
                &palette::CHECKOUT_ANSWERS,
            )
        });
        let params = GitCheckoutParams {
            project_id: source.project_id,
            worktree_task_id: source.worktree_task_id,
            repo,
            branch,
        };
        cx.spawn_in(window, async move |this, cx| {
            if let Some(answer) = answer
                && !palette::confirms_checkout(answer.await.ok())
            {
                return;
            }
            let project_id = params.project_id;
            let branch = params.branch.clone();
            let done = connection.call::<()>(Request::GitCheckout(params)).await;
            let _ = this.update_in(cx, |shell, window, cx| match done {
                Ok(()) => {
                    shell.toast_info(format!("Switched to {branch}"), cx);
                    if shell.git.contains_key(&project_id) {
                        shell.refresh_git(project_id, window, cx);
                    }
                }
                Err(error) => shell.toast_error(format!("{error:#}"), cx),
            });
        })
        .detach();
    }

    /// A Commits pick: the Git view's History on that commit (its log is read long enough to
    /// list it, see `GitView::show_commit`).
    fn show_palette_commit(
        &mut self,
        source: palette::Source,
        repo: Option<String>,
        sha: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let project_id = source.project_id;
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.retarget_git(project_id, source.worktree_task_id, repo);
        let requests = self.git_view(project_id).show_commit(sha);
        self.open_git(project_id, window, cx);
        // A second `git.log` while the first is in flight is coalesced by `begin_log`.
        self.git_requests(project_id, requests, window, cx);
        cx.notify();
    }
```

`GitCheckoutParams` comes through the `mkrew_protocol::{..}` import of `main.rs` (add `GitCheckoutParams` to it; `cargo fmt` sorts the list).
`Ulid`, `PromptLevel`, `status`, `FsMatch`, `Commit` and `Request` are already in scope through `crate::*`.
If `has_worktree` fails to borrow while `self.sidebar.sessions()` is used, compute `let sessions = self.sidebar.sessions();` first (both are shared borrows).

- [ ] **Step 7: Build and lint**

Run: `cargo fmt` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean.
`grep -rn "quick_open::\|QuickOpen\|quick_open_scroll\|render_quick_open" crates/mkrew-ui/src/shell crates/mkrew-ui/src/main.rs` lists nothing (the keymap names `Command::QuickOpen` match the first pattern: expect only `sessions.rs` and `files_render.rs` lines naming `Command::QuickOpen`).

Run: `cargo test -p mkrew-ui` and `cargo test -p mkrew-ui --bin mkrew`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): the shell drives the scoped palette (fetches, picks, checkout, @ mode)"
```

---

### Task 12: Draw the scoped palette

**Files:**
- Modify: `crates/mkrew-ui/src/shell/navigation.rs` (`render_palette`, from `/// The command palette: the query field` to the end of the impl)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (delete the `#[allow(dead_code)] // used by Plan P2` line Plan P1 left above `list_row`, line 857)
- Modify: `crates/mkrew-ui/src/theme.rs` (`PALETTE_TITLE_MAX_FRACTION` after `PALETTE_GROUP_LIMIT`, line 455; the comment above the `PALETTE_*` block, line 451)
- Modify: `crates/mkrew-ui/src/main.rs` (the `palette` import adds `Scope`)

**Interfaces:**
- Consumes: `widgets::{floating_panel, list_row, kbd, search_box, overline, caret_field}` and `widgets::Look` with `Pressable::pressable` (the scope pills), `icons::{Icon, icon_svg}` (the error mark), `theme::{PALETTE_WIDTH, PALETTE_LIST_HEIGHT, BORDER_WIDTH, SEGMENT_HEIGHT}`, `radius::PILL`, `Palette::{lines, tabs, scope, selected, selected_line, insert_into, source}`, `palette::Line`, `Scope::{ORDER, title, placeholder}`.
- Produces: `theme::PALETTE_TITLE_MAX_FRACTION: f32 = 0.6`; the new `render_palette(&self, &Palette, &Theme, cx) -> AnyElement` (same signature, so `main.rs` and `detached_render.rs` are unchanged).
  `segmented` and `segment` stay for their other callers; the scope tabs are not segments (spec 7.5 wants accent-outlined pills, `segment` marks the current one with a fill).

Layout, top to bottom (spec 7.5): an optional title (theme picker, `@` mode), the query field as `widgets::search_box` (search icon, the scope's placeholder, an `esc` key cap), the scope pills (only when `Palette::tabs()`; the current one has a 1 px accent border and no fill), the list (overline headers, rows, "+N more", muted notes; at most `PALETTE_LIST_HEIGHT` tall, scrolls), and the footer: "↑↓ navigate · ↵ run · Tab next scope" on the left, "esc close" on the right.

- [ ] **Step 1: Replace `render_palette`**

```rust
    /// The palette: the query field, the scope pills, then the lines of the scope (overlines,
    /// rows with their matched chars in the accent color, "+N more", notes) and the key hints.
    /// A click on a pill switches scope; a click on a row runs it.
    pub(crate) fn render_palette(
        &self,
        palette: &Palette,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let accent = gpui::Hsla::from(rgb(theme.accent));
        let highlights = move |ranges: Vec<std::ops::Range<usize>>| {
            ranges
                .into_iter()
                .map(|range| {
                    let style = gpui::HighlightStyle {
                        color: Some(accent),
                        font_weight: Some(FontWeight(weight::SEMIBOLD)),
                        ..gpui::HighlightStyle::default()
                    };
                    (range, style)
                })
                .collect::<Vec<_>>()
        };
        let muted = |text: String| {
            div()
                .flex_none()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let lines = palette.lines();
        let empty = lines.is_empty();
        let rows: Vec<gpui::AnyElement> = lines
            .into_iter()
            .enumerate()
            .map(|(at, line)| match line {
                palette::Line::Header(title) => overline(title, theme)
                    .flex_none()
                    .px(px(space::M))
                    .pt(px(space::S))
                    .pb(px(space::XS))
                    .into_any_element(),
                palette::Line::Note { text, error } => div()
                    .flex_none()
                    .flex()
                    .gap(px(space::XS))
                    .px(px(space::M))
                    .py(px(space::XS))
                    .text_size(font_size::META)
                    .when(error, |el| {
                        el.child(icon_svg(
                            Icon::X,
                            theme::ICON_SIZE_SMALL,
                            theme.status_exited,
                        ))
                    })
                    .child(
                        div()
                            .text_color(rgb(if error { theme.text } else { theme.text_muted }))
                            .child(text),
                    )
                    .into_any_element(),
                palette::Line::More {
                    scope,
                    hidden,
                    pick,
                } => list_row(("palette-more", at), pick == palette.selected, theme)
                    .flex_none()
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        if let Some(palette) = shell.palette.as_mut() {
                            palette.set_scope(scope);
                        }
                        shell.palette_changed(window, cx);
                    }))
                    .child(muted(format!("+{hidden} more ›")))
                    .into_any_element(),
                palette::Line::Row { row, pick } => {
                    let title = gpui::StyledText::new(row.entry.title.clone())
                        .with_highlights(highlights(row.ranges));
                    let has_detail = row.detail.is_some();
                    list_row(("palette-row", at), pick == palette.selected, theme)
                        .flex_none()
                        .gap(px(space::M))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            if let Some(palette) = shell.palette.as_mut() {
                                palette.pick(pick);
                            }
                            shell.run_palette_pick(window, cx);
                        }))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .when(has_detail, |el| {
                                    el.flex_none()
                                        .max_w(relative(theme::PALETTE_TITLE_MAX_FRACTION))
                                })
                                .when(!has_detail, |el| el.flex_1())
                                .child(title),
                        )
                        .children(row.detail.map(|detail| {
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(font_size::META)
                                .text_color(rgb(theme.text_muted))
                                .child(detail.to_owned())
                        }))
                        .children(row.age.map(|age| muted(age.to_owned())))
                        .when(!row.entry.shortcut.is_empty(), |el| {
                            // The flat theme picker keeps its muted "Dark" / "✓ Light" text.
                            if palette.tabs() {
                                el.child(kbd(row.entry.shortcut.clone(), theme, false))
                            } else {
                                el.child(muted(row.entry.shortcut.clone()))
                            }
                        })
                        .into_any_element()
                }
            })
            .collect();

        let picking_theme = self.theme_pick.is_some();
        let workspace = palette
            .source()
            .and_then(|source| {
                (self.sidebar.projects().iter()).find(|project| project.id == source.project_id)
            })
            .map_or_else(String::new, |project| format!(" · {}", project.name));
        let title = if picking_theme {
            Some("Select theme".to_owned())
        } else if palette.insert_into().is_some() {
            Some(format!("Insert file{workspace}"))
        } else {
            None
        };
        // Pills, not `segment`s: the current scope is outlined in the accent, no fill.
        let tabs = palette.tabs().then(|| {
            div().flex().items_center().gap(px(space::XS)).children(
                Scope::ORDER.iter().enumerate().map(|(index, scope)| {
                    let scope = *scope;
                    let current = scope == palette.scope();
                    div()
                        .id(("palette-scope", index))
                        .flex_none()
                        .flex()
                        .items_center()
                        .h(theme::rems_px(theme::SEGMENT_HEIGHT))
                        .px(px(space::M))
                        .rounded(px(radius::PILL))
                        .border(px(theme::BORDER_WIDTH))
                        .border_color(if current {
                            gpui::Hsla::from(rgb(theme.accent))
                        } else {
                            gpui::transparent_black()
                        })
                        .whitespace_nowrap()
                        .text_size(font_size::BODY)
                        .text_color(rgb(if current { theme.text } else { theme.text_muted }))
                        .pressable(Look::Chip, true, theme)
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            if let Some(palette) = shell.palette.as_mut() {
                                palette.set_scope(scope);
                            }
                            shell.palette_changed(window, cx);
                        }))
                        .child(scope.title())
                }),
            )
        });
        let (hint, close): (&str, &str) = if picking_theme {
            ("↑↓ preview · ↵ save", "esc restore")
        } else if palette.insert_into().is_some() {
            ("↑↓ navigate · ↵ type its path", "esc type @")
        } else {
            ("↑↓ navigate · ↵ run · Tab next scope", "esc close")
        };
        let panel = floating_panel("palette", theme)
            .w(px(theme::PALETTE_WIDTH))
            .flex()
            .flex_col()
            .gap(px(space::M))
            .p(px(space::L))
            .font_family(theme.ui_font_family.clone())
            .track_focus(&self.dialog_focus)
            .on_key_down(cx.listener(Self::palette_key_down))
            .child(self.typing(&self.dialog_focus, Self::palette_key_down, cx))
            .children(title.map(|title| {
                div()
                    .text_size(font_size::CARD_TITLE)
                    .font_weight(FontWeight(weight::SEMIBOLD))
                    .text_color(rgb(theme.text))
                    .child(title)
            }))
            .child(search_box(
                caret_field(
                    "palette-query",
                    palette.query(),
                    if picking_theme {
                        "Type a theme name"
                    } else {
                        palette.scope().placeholder()
                    },
                    true,
                    theme,
                    cx,
                    |shell, caret| {
                        if let Some(palette) = shell.palette.as_mut() {
                            palette.click(caret.at, caret.extend, caret.clicks);
                        }
                    },
                ),
                Some(kbd("esc".to_owned(), theme, false).into_any_element()),
                true,
                theme,
            ))
            .children(tabs)
            .when(!empty, |el| {
                el.child(
                    div()
                        .id("palette-entries")
                        .max_h(px(theme::PALETTE_LIST_HEIGHT))
                        .overflow_y_scroll()
                        .track_scroll(&self.palette_scroll)
                        .flex()
                        .flex_col()
                        .gap(px(space::XS))
                        .children(rows),
                )
            })
            .when(empty, |el| {
                el.child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.text_muted))
                        .child(format!("Nothing matches “{}”.", palette.query().trim())),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(hint)
                    .child(close),
            );
        Self::modal_shade("palette-shade", panel, cx)
    }
}
```

`Scope`, `palette::Line`, `Look`, `Pressable`, `Icon`, `icon_svg`, `radius` and the P1 widgets are in scope through `crate::*` / `shell::widgets::*` (add `Scope` to the `use mkrew_ui::palette::{self, Extra, Palette};` line of `main.rs` as `use mkrew_ui::palette::{self, Extra, Palette, Scope};`).
`list_row(..)` already carries the selected fill, the accent bar and the pressed look; the old focus-colored ring is gone on purpose (the bar is the pick marker now).
If `.flex_none()` on the `Stateful<Div>` that `list_row` returns does not type-check after `.on_click`, move it before `.on_click`.

In `widgets.rs` delete the `#[allow(dead_code)] // used by Plan P2` line (line 857) above `list_row`.

In `theme.rs` replace the comment above the `PALETTE_*` block (line 451, "The scoped palette (Plan P2 uses these; Plan P1 only defines them).") with `/// The scoped palette.` and add after `PALETTE_GROUP_LIMIT`:

```rust
/// Widest share of a palette row its title may take while a muted detail follows (the title
/// is cut with `…` past that).
pub const PALETTE_TITLE_MAX_FRACTION: f32 = 0.6;
```

- [ ] **Step 2: Build, lint, test**

Run: `cargo fmt` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean.
Run: `cargo test -p mkrew-ui --bin mkrew`
Expected: PASS.

- [ ] **Step 3: Look at it**

Run the UI against a throwaway data dir (`MKREW_DATA_DIR`, `MKREW_CONFIG_DIR`, `MKREW_SOCKET` set to temp folders, as in `docs/HUMAN_CHECKS.md`), add a workspace that is a git repository, press `Ctrl+Shift+P`, type `a`, press Tab through the scopes; repeat on the light theme.
Check against the list Task 14 adds to `docs/HUMAN_CHECKS.md`.

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/shell/navigation.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): scoped palette layout on floating_panel with search field, scope pills, sections and key caps"
```

---

### Task 13: Delete Quick open

**Files:**
- Delete: `crates/mkrew-ui/src/quick_open.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (line 10 doc, line 81)
- Modify: `crates/mkrew-ui/src/theme.rs` (lines 441-442)
- Modify: `crates/mkrew-ui/src/shell/composer.rs:691`, `shell/slash.rs:463`
- Modify: `crates/mkrew-ui/src/palette.rs` (module doc, lines 1-5), `src/keymap.rs` (the `QuickOpen` doc was changed in Task 8)

**Interfaces:**
- Removes: `mkrew_ui::quick_open` (`QuickOpen`, `Results`, its `Key`, `key` and `match_ranges`, which Task 6 already moved), `theme::QUICK_OPEN_LIST_HEIGHT`, `Shell::render_quick_open` (gone in Task 11).
- The finder's tests are not lost: serial drop is `files_are_asked_on_open_debounced_and_late_answers_are_dropped` and `a_closed_palettes_answer_never_lands_in_the_next`, `insert_into` is `the_at_picker_lists_files_and_hands_the_path_to_its_prompt`, and `Key` / `match_ranges` moved in Task 6.

- [ ] **Step 1: Delete and migrate**

```bash
git rm crates/mkrew-ui/src/quick_open.rs
```

`lib.rs`: delete line 81 `pub mod quick_open;`; in the crate doc (line 9-10) `its syntax highlighting and quick open, the Git view-model` becomes `its syntax highlighting, the Git view-model`; `the command palette` there becomes `the scoped palette`.
`theme.rs`: delete lines 441-442 (`/// Height of the quick open match list ...` and `pub const QUICK_OPEN_LIST_HEIGHT: f32 = 360.0;`) and nothing else: line 440 is the blank line before them and line 443 is the first line of the `GLASS_TINT_HEIGHT` doc comment, which stays.
`shell/composer.rs` line 691 and `shell/slash.rs` line 463: `theme::QUICK_OPEN_LIST_HEIGHT` becomes `theme::PALETTE_LIST_HEIGHT`.
`palette.rs` module doc (lines 1-5) becomes:

```rust
//! The palette (Ctrl+Shift+P, SPEC §13): one list with scope tabs (All, Commands, Sessions,
//! Files, Search, Branches, Commits). Commands are every keymap command that can act in the
//! current view plus palette-only actions that have no key; Sessions are "Go to" workspace /
//! session / task items; Files, Search, Branches and Commits are asked of the daemon
//! (`fs.find`, `fs.search`, `git.branches`, `git.log`), each call numbered so an old answer
//! is dropped. Typing filters with a fuzzy subsequence match ranked by word starts and runs;
//! Enter picks. `Ctrl+Shift+O` opens it on Files, `Ctrl+Alt+Shift+F` on Search, `@` in a
//! prompt on Files in insert mode. Pure and tested headlessly; `main.rs` renders it and runs
//! the pick.
```

- [ ] **Step 2: Prove nothing is left**

Run: `grep -rn "quick_open\|QuickOpen\|QUICK_OPEN\|render_quick_open" crates scripts .claude`
Expected: only the keymap's `Command::QuickOpen` (the enum variant, its `name`, `default_keys`, `all`, the palette `title` and `available` arms, the tests, `sessions.rs`, `files_render.rs`) and the string `"quick_open"` (the user-facing key name that keeps working).
Anything else is a caller to migrate.

Run: `cargo clippy --workspace --all-targets -- -D warnings` then `cargo test -p mkrew-ui`
Expected: clean and PASS.

- [ ] **Step 3: Commit**

```bash
git add -A crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "refactor(ui): remove Quick open, its list height and its tests (the palette's Files scope replaces them)"
```

---

### Task 14: Docs, human checks and the full verify

**Files:**
- Modify: `docs/SPEC.md` (dependency table line 185, Editor view and Tabs lines 1244 and 1257, Quick open bullet lines 1316-1318, text fields line 1438, keys table lines 1452-1453, command-palette note line 1568, protocol table after the `fs.find` row at line 613, version notes after line 691, §14 palette layout bullet after the Settings bullet (line 1710), checklist line 1953)
- Modify: `README.md` (lines 182 and 186)
- Modify: `docs/HUMAN_CHECKS.md` (new section before line 24)

**Interfaces:** none (docs).
Each Markdown sentence goes on its own line; no em dash.
SPEC line numbers are those of the file before this task; Steps 1 to 3 insert lines, so later steps find their anchors by the quoted text.

- [ ] **Step 1: SPEC section 13 (keys)**

Replace the two table rows at lines 1452-1453:

```markdown
| Global | Command palette (All scope) | Ctrl+Shift+P |
| Global | Palette on Files: find a file of the active workspace | Ctrl+Shift+O |
| Global | Palette on Search: find text in the files of the active workspace | Ctrl+Alt+Shift+F |
```

Line 1568 `The command palette shows each command's shortcut` stays true; after it add a paragraph:

```markdown
The palette has scope tabs: All, Commands, Sessions, Files, Search, Branches, Commits.
Tab and Shift+Tab switch scope, Up and Down move, Enter runs, Esc closes, a click on a tab switches to it.
All lists, for a query, a section per scope except Search (five rows each, then "+N more ›" which switches to that scope); with an empty query it lists the sessions (waiting ones first) and the commands run lately.
Files asks `fs.find` (debounced 80 ms), Search asks `fs.search` from two characters (150 ms, plain text, smart case), Branches asks `git.branches` and Commits asks `git.log` (500 commits) once per opening and both are filtered in the UI.
A late answer is dropped.
Branches switches with `git.checkout`; a question comes first when the repository has changed files or a live agent session runs in that folder; git's own error is shown as a toast.
Commits opens the Git view's History on the commit.
A search hit opens the file with the caret on its line.
`@` in a prompt opens the palette on Files in insert mode: Enter types the path as `composer.file_ref`, Esc types the `@`.
The theme picker (`Ctrl+Alt+J`) is the same palette without tabs.
```

- [ ] **Step 2: SPEC other mentions**

- Line 185: after the `nucleo-matcher` row add `| Content search | \`grep-searcher\`, \`grep-regex\`, \`grep-matcher\` | \`fs.search\` (ripgrep's engine over the files \`ignore\` walks) |`.
- Lines 1316-1318: replace the **Quick open** bullet with `- **Palette Files scope** (\`Ctrl+Shift+O\`, or the "Search files..." entry at the top of the tree) fuzzy-finds files of the current workspace, in the folder the tree shows (project or task worktree) (\`ignore\` to walk, \`nucleo-matcher\` to rank).` and one more line for Search: `- **Palette Search scope** (\`Ctrl+Alt+Shift+F\`) finds text in those files.`
- Lines 1244 and 1257, 1438: `quick open` becomes `the palette`; line 1438 `quick open, command palette,` becomes `the palette,`.
- Line 1953: `Quick open fuzzy-finds files` becomes `The palette's Files scope fuzzy-finds files`.
- §14 (spec 12): the P1 sections for glass, the pressable rule and Settings are already in `docs/SPEC.md` §14; the palette layout is not.
  After the last line of the Settings bullet (line 1710, `... the hint stays "Ctrl+N new · Ctrl+S saves · Esc closes".`) and before `- Settings → Machines`, insert:

```markdown
- Palette (Ctrl+Shift+P, ADR 0097): a floating glass panel (`floating_panel`, `PALETTE_WIDTH` wide).
  Top: the query field with a search icon, the scope's placeholder and an `esc` key cap (`search_box`).
  Below it: the scope tabs as pills (`Look::Chip`; the current one has a 1 px accent border and no fill, the others are muted and get the hover fill).
  The list is at most `PALETTE_LIST_HEIGHT` tall and scrolls.
  It has section overlines, `list_row` rows (the selected one has the `selected` fill and a `PALETTE_SELECTED_BAR` accent bar) and a muted "+N more ›" row; matched characters are accent and semibold, `detail` is muted after the title, `age` is right-aligned, shortcuts are `kbd` key caps.
  A scope that waits shows a muted "Loading…" and one that failed a muted error line with an `x` icon, in its own tab only.
  Footer: "↑↓ navigate · ↵ run · Tab next scope" on the left and "esc close" on the right.
  The theme picker and the `@` file picker have no tabs; the theme picker keeps its muted "Dark" / "Light" text instead of key caps.
```

- [ ] **Step 3: SPEC protocol section**

Find the rows: `grep -n '^| `fs.find`\|^| `git.show`\|^| `git.repos`' docs/SPEC.md`.
After the `fs.find` row insert:

```markdown
| `fs.search` | `project_id`, `worktree_task_id?`, `query`, `regex?` (default false), `limit?` (default 200) | `{hits: SearchHit[], truncated}`; a `SearchHit` is `{path, line, preview, ranges: [[start, end]]}` (byte ranges in `preview`, at most 200 chars of the line, trimmed); text files up to 2 MiB under the root, smart case (case-insensitive unless the query has an upper-case letter), `invalid_request` for an invalid regex; runs beside the read loop, and a newer `fs.search` on the same connection stops an older one, which answers `invalid_state` |
```

After the `git.repos` row insert:

```markdown
| `git.branches` | `project_id`, `worktree_task_id?`, `repo?` (ADR 0029) | `Branch[]`, newest commit first: `{name, remote, current, upstream?, last_commit_at, subject}`; `refs/remotes/*/HEAD` and remote branches that have a local branch are left out; `invalid_state` outside a repository |
| `git.checkout` | `project_id`, `worktree_task_id?`, `repo?`, `branch` | `null`; `git switch <branch>`, for a remote branch `git switch --track <remote>/<name>`; git's stderr is the `invalid_state` message; broadcasts `git.changed` |
```

After the Protocol 53 note (line 691) add a blank line and:

```markdown
Protocol 54 (Polish spec 2026-11-21 §8): requests `git.branches`, `git.checkout`, `fs.search`.
```

- [ ] **Step 4: README**

Line 182: `Quick open \`Ctrl+Shift+O\`;` becomes `Find a file \`Ctrl+Shift+O\`, search the text of the files \`Ctrl+Alt+Shift+F\`;`.
Line 186: becomes

```markdown
- **Palette** `Ctrl+Shift+P`: every command, your sessions, files, text in files, branches (switch) and commits, with scope tabs (Tab / Shift+Tab); plus actions without a shortcut.
```

- [ ] **Step 5: Human checks**

Insert before line 24 of `docs/HUMAN_CHECKS.md` (`PUT <24`):

```markdown
### To verify - 2026-11-21 Scoped palette and protocol 54 (plan docs/superpowers/plans/2026-11-21-polish-p2-palette.md)
- [ ] **Scopes (dark + light):** `Ctrl+Shift+P` shows the search field on top (search icon, `esc` key cap) and seven scope pills below it, the current one with an accent outline and no fill; Tab / Shift+Tab walk them and wrap; a click on a pill switches; the placeholder changes with the scope; the footer reads "↑↓ navigate · ↵ run · Tab next scope" on the left and "esc close" on the right; the panel is the glass panel of Plan P1.
- [ ] **All:** an empty query lists sessions (a session that needs input first) and the commands run lately; a query lists sections with at most five rows and "+N more ›", which switches to that scope; Search never has a section; Files, Branches and Commits appear only once they have rows, All never shows "Loading" or an error line (those are in the scope's own tab).
- [ ] **Files:** `Ctrl+Shift+O` opens on Files, empty query lists files at once, typing fast shows only the last answer, matched letters are accent colored, Enter opens the file in the Editor view; in a task worktree it searches the worktree.
- [ ] **Search:** `Ctrl+Alt+Shift+F` opens on Search; one character shows "Type 2 characters to search the files."; `(` or `a.c` is searched as text; each hit shows the line with the match colored and `path:line` muted; Enter opens the file with the caret on that line; a repository with many hits stops at 200.
- [ ] **Branches:** newest first, `current` marked, a remote branch without a local one listed as `origin/x`; Enter on a clean repository switches at once with a toast "Switched to ..."; with changed files, or a live agent in that folder, a question comes first and says which; a refusal by git shows git's own message in a toast.
- [ ] **Commits:** 500 commits found by message or hash; Enter opens the Git panel on History with that commit expanded, also a commit older than the 50 the History lists by default; leaving History puts the default back.
- [ ] **`@` in a composer and in the New task prompt:** the palette opens on Files with no tabs and the title "Insert file", Enter types the path (the `composer.file_ref` form), Esc types the `@`; the New task dialog stays open under it.
- [ ] **No workspace:** Files, Search, Branches and Commits say "Open a workspace to search it."; with the daemon stopped they show its error, not "Loading..." forever.
- [ ] **Theme picker (`Ctrl+Alt+J`):** no tabs, still previews on Up / Down and restores on Esc; its rows keep the muted "Dark" / "Light" text.
- [ ] **Detached window:** `Ctrl+Shift+P` there lists what acts in that window; a picked file, search hit, branch or commit is run in the main window (it comes to the front, on the palette's workspace).
- [ ] **`/` and prompt pickers:** their lists are now as tall as the palette's (480 px).
```

- [ ] **Step 6: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and every test green. If the run exceeds 600 s, run `cargo test --workspace` on its own.
Also run `grep -rnP "\x{2014}" docs/SPEC.md README.md docs/HUMAN_CHECKS.md docs/superpowers/plans/2026-11-21-polish-p2-palette.md` for the em dash introduced by this plan: expected none in the lines this plan added.

- [ ] **Step 7: Commit**

```bash
git add docs/SPEC.md README.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: palette scopes, Ctrl+Alt+Shift+F, protocol 54 (fs.search, git.branches, git.checkout)"
```

---

## Self-review (spec coverage)

| Spec | Task |
| --- | --- |
| 7.1 `Scope`, `ORDER`, `Row`, `Action` additions, `fuzzy` stays | 7a (types), 7b (engine) |
| 7.2 sync sources, async sources, serial drop, 80 and 150 ms debounce, empty and failed states, workspace and repo rule, no workspace | 7b, 11 (`palette_source`, `run_palette_fetches`) |
| 7.3 All grouping, `PALETTE_GROUP_LIMIT`, "+N more", Search absent, a scope joins All only with rows (no loading or error lines in All) | 7b (`grouped`, `group`, `joins_all`) |
| 7.4 keys: `Ctrl+Shift+P`, `Ctrl+Shift+O`, `Command::SearchInFiles` on `ctrl-alt-shift-f` (verified free), `Ctrl+Alt+J`, Tab / Shift+Tab, click on a pill | 8 (command, `scope_for`), 7a (`scope_step`), 11 (dispatch through `scope_for`), 12 (pills) |
| 7.4 `@` insert mode | 7b (`file_ref`, `insert_into`), 11 (`open_file_ref_picker`, `insert_palette_pick`, `close_palette`) |
| 7.5 rendering on `floating_panel`, `search_box` with the `esc` key cap, accent-outlined pills, `list_row`, `kbd`, footer, `PALETTE_*` | 12 |
| 7.6 `OpenFile`, `OpenSearchHit`, `Checkout` with confirm, `ShowCommit` (a detached window's pick runs in the main window) | 7a (variants, temporary arm), 9, 10, 11 |
| 7.7 removal of `quick_open.rs`, `render_quick_open`, `QUICK_OPEN_LIST_HEIGHT`, callers migrated | 6 (keys, `match_ranges`), 11 (shell), 13 (file, constant, pickers) |
| 8 protocol 54, types, method names, `PROTOCOL_VERSION` | 1 |
| 8.1, 8.2 daemon handlers on `git()` | 2, 3 |
| 8.3 `fs.search`, `ignore` walker, new deps, cancellation by a per-connection `AtomicU64` | 4, 5 |
| 11 testing: protocol, daemon, palette; "Ctrl+Shift+O opens Files" | 1 to 5 (tests), 6 to 10 (palette tests), 8 (`scope_for`) |
| 12 docs: SPEC 13, protocol section, SPEC 14 palette layout, HUMAN_CHECKS | 14 |

Placeholder scan: every code step holds the code; the only conditional wording is the compiler hints (borrow order, where `.flex_none()` goes) in Tasks 11 and 12 where the exact gpui types cannot be checked without building.
Type consistency: `Action` is `Clone` (not `Copy`) from Task 7a on and every commit from 7a compiles (the temporary `run_palette_pick` arm and the two test matches get the four new variants), `Entry::new` replaces every literal, `Palette::selected` indexes selectable lines (rows and "+N more"), `selected_line()` is the child index for scrolling, `Source::new` is the only constructor, `COMMIT_LIMIT` is defined in Task 7b and used in 7b and 9, `scope_for` is defined in Task 8 and used in Task 11.
Commit order of the daemon part: Tasks 4 and 5 go to one implementer back to back, and the clippy gate runs after Task 5.
