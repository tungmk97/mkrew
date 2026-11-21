# Review Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the review pass in the Git view and the Markdown views: `v` moves on to the next file, `n` / `p` cross files, one hunk can be discarded, and "Send selection" works from a Markdown preview and from the Docs view.

**Architecture:** `git.discard` gains an optional `hunk` (protocol 56); the daemon reverses that hunk in the working tree with `git apply -R`.
The Git view-model (`git_view.rs`) gets `next_unviewed`, a cross-file `step_hunk` with a pending hunk target, and a hunk `Discard`; the shell only sends the requests, scrolls and draws.
`picked_lines` gets two more sources (a Files tab's preview selection, the Docs comment selection), both computed in pure view-model code and tested headlessly.

**Tech Stack:** Rust 2024 workspace (`mkrew-protocol`, `mkrew-daemon`, `mkrew-ui` on gpui), real `git` in temp repos for the daemon tests, insta-free unit tests.

**Spec:** `docs/superpowers/specs/2026-11-22-review-flow-design.md` (commit af88f23). Executors read both.

**Worktree:** `C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow`, branch `feat/review-flow`, base master `895bc4e`.
Every path below is relative to that folder; every file edit and every command uses the absolute path (relative paths resolve to the main checkout, which must not change).
Line numbers are at HEAD `af88f23` and move as tasks land: re-read the range before each edit.

## Global Constraints

- Protocol `55 -> 56`: 55 is reserved by the unmerged Widget bar branch, so this branch bumps `PROTOCOL_VERSION` from 54 straight to 56 and rebases the number if the merge order changes (spec header).
- ADR number is `0103` (0098-0100 macOS plans, 0101 Theme gallery, 0102 Widget bar).
- `GitDiscardParams` gains `#[serde(default)] pub hunk: Option<HunkTarget>`; with `hunk` set, `paths` must be empty, otherwise `invalid_request`; `confirm` stays required (spec 5.1).
- Hunk discard runs `git apply -R` on the working tree, no `--cached`; a patch that no longer applies fails with git's error text (spec 5.2).
- Only a hunk of an unstaged change of a modified file is offered; the UI button is `pressable` with danger text, only in the Changes group (spec 5.3).
- `v` on a file that is not viewed marks it and selects the next not-viewed file below it, wrapping to the top; with every file viewed the selection stays; `v` on a viewed file only clears the mark (spec 2).
- `n` after the last hunk selects the next file and its first hunk; `p` before the first selects the previous file and its last hunk; viewed files are skipped; no wrap at the list ends (spec 2).
- The preview selection maps to its source blocks' line range and the sent text is the Markdown source of those lines; the Docs comment selection is the Docs selection (spec 2).
- `c` stays "focus the commit box" and `Ctrl+Alt+M` stays "add comment"; History comments and viewed marks stay out of scope (spec 2, 10).
- Out of scope: comments and viewed marks in History, rebinding `c`, a continuous multi-file diff page, discarding hunks of staged or untracked changes (spec 10).
- Project rules: no `unwrap` / `expect` outside tests; styling only through `theme.rs` tokens and spacing 4 / 8 / 12 / 16 / 24 / 32; dark and light themes; no em dash in docs; one sentence per line in long Markdown.
- Commit with `git -c user.email=tungnv975@gmail.com commit`, Conventional Commits, only the task's files, never push, never amend.
- Run Windows shell steps with `CARGO_BUILD_JOBS=4`; the full check is `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL).

## Review Focus

Failure modes the spec implies that no obvious test exercises; each has its test in the named task.

1. A hunk discard on a CRLF working file must keep every line ending of the other lines (Task 2, daemon unit test with `core.autocrlf=false` and CRLF content).
2. The diff reloads (an agent edits the file) while the "Discard this hunk?" dialog is open: confirming must not discard a hunk the dialog no longer shows (Task 5, `hunk_is_current` test and a disabled confirm).
3. A preview selection that crosses a diagram or rule (a block with no text), or ends at the first byte of a block, must not widen the line range (Task 6, `source_lines` tests).
4. The Docs selection lies past the end after the file was re-read shorter: nothing is sent, nothing panics (Task 6, Docs test).
5. `v` through the checkbox of a row that is not selected, or on the last unviewed file, must not move the selection or send a request (Task 3, tests); a collapsed (viewed) diff must keep ignoring `n` / `p` and `x` must then discard the file, not a hidden hunk (Tasks 4 and 5, tests).

## File Structure

| File | Responsibility | Tasks |
|---|---|---|
| `crates/mkrew-protocol/src/files.rs` | `GitDiscardParams.hunk`, wire tests | 1 |
| `crates/mkrew-protocol/src/lib.rs`, `message/tests.rs` | `PROTOCOL_VERSION = 56` and its test | 1 |
| `crates/mkrew-daemon/src/git.rs` | `GitBackend::discard_hunk`, shared `apply_patch`, unit test | 2 |
| `crates/mkrew-daemon/src/undo.rs` | test double of `GitBackend` gets `discard_hunk` | 2 |
| `crates/mkrew-daemon/src/git_view.rs` | `ViewError::Refused` | 2 |
| `crates/mkrew-daemon/src/server/git.rs` | `git_discard` with a hunk, `view_outcome` | 2 |
| `crates/mkrew-daemon/tests/daemon/git.rs` | integration test on a real temp repo | 1, 2 |
| `crates/mkrew-ui/src/git_view.rs` | `next_unviewed`, cross-file `step_hunk`, `PendingHunk`, hunk `Discard` | 1, 3, 4, 5 |
| `crates/mkrew-ui/src/git_view/tests.rs` | view-model tests | 3, 4, 5 |
| `crates/mkrew-ui/src/shell/git_viewed.rs`, `shell/git.rs` | `v` then next, `n` / `p` scroll, `x` | 3, 4, 5 |
| `crates/mkrew-ui/src/shell/git_diff.rs`, `shell/git_dialogs.rs` | hunk header button, dialog | 5 |
| `crates/mkrew-ui/src/worktree_dialog.rs` | `hunk: None` on its `GitDiscardParams` | 1 |
| `crates/mkrew-ui/src/keymap.rs`, `palette.rs` | doc comment and title of `GitDiscard`, `SendSelection` in Docs | 5, 6 |
| `crates/mkrew-ui/src/preview_select.rs` | `source_lines` | 6 |
| `crates/mkrew-ui/src/files_view.rs` (+ `files_view/tests.rs`) | `FileTab::picked_lines` | 6 |
| `crates/mkrew-ui/src/docs.rs` | `Docs::picked_lines` | 6 |
| `crates/mkrew-ui/src/shell/composer.rs`, `shell/send_dialog.rs` | `picked_lines` delegates, Docs joins `open_selection_dialog` | 6 |
| `docs/decisions/0103-review-flow.md`, `docs/SPEC.md`, `docs/HUMAN_CHECKS.md` | ADR, SPEC, manual checks | 7 |

## Spec gaps

The spec is binding; where it disagrees with the code this plan takes the conservative option and lists it here (the spec file is not edited).

1. **Protocol number.** The task text says 54 -> 56; the spec header says 55 -> 56. Same thing: HEAD is 54, 55 belongs to the unmerged Widget bar, this branch sets 56 (Task 1).
2. **Spec 4 says `step_hunk` "calls `step_file(+-1)` over `steppable` minus viewed files".** `step_file` (git_view.rs:1465) cannot skip viewed files, and in the History it steps to another commit past the last file.
   The plan adds `step_hunk_file`, which scans `steppable()` for the next / previous file that is not viewed, never leaves the shown commit's files and keeps `step_file` untouched (Task 4).
3. **Spec 3: viewed then next after a click on a row's box.** `toggle_git_viewed` is also called by a row's box and the toolbar button.
   `next_unviewed(target)` moves only when `target` is the selected file; marking another row never moves the selection (Task 3).
4. **Spec 4: a viewed file with a collapsed diff.** `step_hunk` returns early today (test `a_collapsed_diff_ignores_hunk_keys_and_stages_the_whole_file`, tests.rs:1561).
   The plan keeps that: a collapsed diff has no visible hunks, so `n` / `p` do nothing there, and `x` then discards the file, not a hidden hunk (Tasks 4 and 5).
5. **Spec 5.2: "Only a hunk of an unstaged change of a modified file is accepted; anything else is `invalid_request`".** The daemon cannot see which side a `Hunk` came from without comparing it with a fresh diff, and that would turn a stale hunk into `invalid_request` instead of git's text (spec 5.2 and 7 want git's text).
   The plan enforces it in two places: the daemon refuses an untracked path with `invalid_request` (`ViewError::Refused`), and the UI only offers a hunk of a Modified file of the Unstaged group.
   A hand-made request that names a staged-only hunk of a clean worktree file would apply; the UI never sends one.
6. **Spec 6: "tried after the editor's".** `picked_lines` falls back to the editor's caret line, so it always answers and would hide the preview selection.
   The plan orders them per mode: Preview mode asks the page first; Split mode asks the editor's explicit selection, then the page, then the caret; Source mode is unchanged. Text and range come from the page's own source lines, and a page older than the editor text (its lines may have moved) is not used.
7. **Spec 8: "UI: `picked_lines` from a preview selection ... and from a Docs selection".** `Shell::picked_lines` needs a window; the logic moves to `FileTab::picked_lines` and `Docs::picked_lines` (pure, headless tests) and the shell delegates.
8. **Spec 5.3: the dialog "shows that hunk's lines".** The dialog also disables its confirm when the reloaded diff no longer has that hunk at that place (Review Focus 2); the spec is silent.
9. **Spec 6 and the palette.** `SendSelection` is only available in the Editor, Diff and terminal views (palette.rs:735); Docs is added so the command and its key work there.
10. **Spec 9 lists SPEC 12A, 12B, 13 and the protocol table.** The plan also updates the `Protocol 56` note list, SPEC 20B.4 (Send selection) and the M10 checklist line, which describe the same feature; `PROGRESS.md` is left alone to avoid merge conflicts with the sibling branches.
11. **Spec 5.1 keeps `paths` required.** A hunk-only request therefore sends `"paths": []`.

---

### Task 1: Protocol - `git.discard` hunk and protocol 56

**Files:**
- Modify: `crates/mkrew-protocol/src/files.rs:252-265` (struct), `:776-782` (round trip list), tests module (new test after `:956`)
- Modify: `crates/mkrew-protocol/src/lib.rs:15-16`
- Modify: `crates/mkrew-protocol/src/message/tests.rs:1318-1321`
- Modify: `crates/mkrew-daemon/tests/daemon/git.rs:306-312`, `:359-365` (constructors)
- Modify: `crates/mkrew-ui/src/git_view.rs:529-537` (constructor)
- Modify: `crates/mkrew-ui/src/worktree_dialog.rs:209-215` (constructor)

**Interfaces:**
- Consumes: `HunkTarget { path: String, hunk: Hunk }` (files.rs:247, already used by `GitStageParams`).
- Produces: `GitDiscardParams { project_id, worktree_task_id, repo, paths: Vec<String>, hunk: Option<HunkTarget>, confirm: bool }` and `PROTOCOL_VERSION == 56`; Tasks 2 and 5 rely on the field name `hunk`.

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-protocol/src/files.rs`, inside the tests module after `optional_params_default_when_missing` (ends at `:956`), add:

```rust
    #[test]
    fn a_discard_may_name_one_hunk() {
        let id = sid().to_string();
        // The old shape still reads: no hunk.
        let plain: GitDiscardParams =
            serde_json::from_value(json!({"project_id": id, "paths": ["a"], "confirm": true}))
                .unwrap();
        assert_eq!(plain.hunk, None);
        let params = GitDiscardParams {
            project_id: sid(),
            worktree_task_id: None,
            repo: None,
            paths: vec![],
            hunk: Some(HunkTarget {
                path: "a".into(),
                hunk: hunk(),
            }),
            confirm: true,
        };
        let wire = serde_json::to_value(&params).unwrap();
        assert_eq!(wire["hunk"]["path"], "a");
        assert_eq!(
            serde_json::from_value::<GitDiscardParams>(wire).unwrap(),
            params
        );
        let value = round_trip(&Message::Request {
            id: 1,
            request: Request::GitDiscard(params),
        });
        assert_eq!(value["method"], "git.discard");
    }
```

In `crates/mkrew-protocol/src/message/tests.rs` replace `:1318-1321`:

```rust
#[test]
fn the_review_flow_protocol_is_version_56() {
    assert_eq!(crate::PROTOCOL_VERSION, 56);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-protocol`
Expected: FAIL to compile, `struct GitDiscardParams has no field named hunk`.

- [ ] **Step 3: Write the implementation**

`crates/mkrew-protocol/src/files.rs:252-265` becomes:

```rust
/// `git.discard`: destructive; `invalid_request` unless `confirm` is true. Whole files
/// (`paths`) or, with `paths` empty, one hunk of a tracked file's unstaged diff (`hunk`),
/// which the daemon reverses in the working tree only (the index is kept).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitDiscardParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
    /// A direct child folder of the project folder that is its own git repository (ADR 0029);
    /// `None` = the project folder. Not with `worktree_task_id`.
    #[serde(default)]
    pub repo: Option<String>,
    pub paths: Vec<String>,
    #[serde(default)]
    pub hunk: Option<HunkTarget>,
    #[serde(default)]
    pub confirm: bool,
}
```

Also fix the doc of `HunkTarget` (`:245`): `/// A hunk as shown by `git.diff`; the daemon builds the patch for `git apply` (`--cached [-R]` to stage / unstage, `-R` on the working tree to discard).`

`crates/mkrew-protocol/src/files.rs:776-782` (the round trip list) gets `hunk: None,` after `paths: vec!["a".into()],`:

```rust
            Request::GitDiscard(GitDiscardParams {
                project_id,
                worktree_task_id: wt,
                repo: None,
                paths: vec!["a".into()],
                hunk: None,
                confirm: true,
            }),
```

`crates/mkrew-protocol/src/lib.rs:15-16`:

```rust
/// Bumped on any incompatible change; `hello` with another value gets `version_mismatch`.
/// 55 belongs to the unmerged Widget bar branch; this one skips to 56 (rebase the number if the merge order changes).
pub const PROTOCOL_VERSION: u32 = 56;
```

`crates/mkrew-daemon/tests/daemon/git.rs`: in both `GitDiscardParams` literals (`:306-312` and `:359-365`) add `hunk: None,` after the `paths:` line.

`crates/mkrew-ui/src/git_view.rs:529-537` (`Discard::params`): add `hunk: None,` after `paths: vec![self.target.path.clone()],` (Task 5 replaces this with the real value).

`crates/mkrew-ui/src/worktree_dialog.rs:209-215`: add `hunk: None,` after `paths: plan.discardable.clone(),`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-protocol && CARGO_BUILD_JOBS=4 cargo check --workspace --all-targets`
Expected: PASS, then `Finished` with no error (every constructor of `GitDiscardParams` updated).

- [ ] **Step 5: Commit**

```bash
cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow
cargo fmt --all
git add crates/mkrew-protocol crates/mkrew-daemon/tests/daemon/git.rs crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/worktree_dialog.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(protocol): git.discard takes one hunk (protocol 56)"
```

---

### Task 2: Daemon - discard one hunk

**Files:**
- Modify: `crates/mkrew-daemon/src/git.rs:80-84` (trait), `:488-501` (impl), tests module (new test after `:1374`)
- Modify: `crates/mkrew-daemon/src/undo.rs:290-295` (test double)
- Modify: `crates/mkrew-daemon/src/git_view.rs:30-44` (`ViewError`)
- Modify: `crates/mkrew-daemon/src/server/git.rs:325-356` (`git_discard`), `:587-609` (`view_outcome`)
- Test: `crates/mkrew-daemon/tests/daemon/git.rs` (new test after `:370`)

**Interfaces:**
- Consumes: `GitDiscardParams.hunk` (Task 1); `mkrew_core::git::hunk_patch(path: &str, hunk: &Hunk) -> String` (core git.rs:502); `GitBackend::is_tracked(&self, dir, path) -> io::Result<bool>`.
- Produces: `GitBackend::discard_hunk(&self, dir: &Path, repo: &Repo, patch: &str) -> io::Result<()>`; `ViewError::Refused(String)` mapped to `invalid_request`; the `git.discard` behaviour the UI (Task 5) relies on: success broadcasts `git.changed` (through `git_change`), a stale hunk is `internal` with `git.discard failed: git failed: ...`.

- [ ] **Step 1: Write the failing tests**

Unit test, in the `crates/mkrew-daemon/src/git.rs` tests module right after `stage_unstage_discard_and_commit_in_a_subfolder` (ends `:1374`; it uses the module's `run`, `hunks` helpers):

```rust
    /// One hunk goes back to the index text in the working tree only; the other hunk, the
    /// index and the CRLF line endings stay; a hunk that no longer matches is refused with
    /// git's reason.
    #[test]
    fn discard_hunk_reverses_one_unstaged_hunk_in_a_subfolder() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let dir = root.join("app");
        std::fs::create_dir(&dir).unwrap();
        run(root, &["init", "-q"]);
        run(root, &["config", "core.autocrlf", "false"]);
        run(root, &["config", "user.name", "t"]);
        run(root, &["config", "user.email", "t@t"]);
        run(root, &["config", "commit.gpgsign", "false"]);
        let lines: Vec<String> = (1..=20).map(|n| format!("line {n}\r\n")).collect();
        std::fs::write(dir.join("a b.txt"), lines.concat()).unwrap();
        GitCli.stage(&dir, &["a b.txt".to_owned()]).unwrap();
        GitCli.commit(&dir, "one").unwrap();
        let mut changed = lines.clone();
        changed[1] = "line two\r\n".into();
        changed[18] = "line nineteen\r\n".into();
        std::fs::write(dir.join("a b.txt"), changed.concat()).unwrap();
        let unstaged = hunks(&dir, "a b.txt", Side::Unstaged);
        assert_eq!(unstaged.len(), 2);
        let repo_info = GitCli.repo(&dir).unwrap().unwrap();

        let patch = mkrew_core::git::hunk_patch("a b.txt", &unstaged[0]);
        GitCli.discard_hunk(&dir, &repo_info, &patch).unwrap();
        let mut expected = lines.clone();
        expected[18] = "line nineteen\r\n".into();
        assert_eq!(
            std::fs::read(dir.join("a b.txt")).unwrap(),
            expected.concat().into_bytes()
        );
        assert_eq!(run(root, &["diff", "--cached", "--name-only"]), "");
        assert_eq!(hunks(&dir, "a b.txt", Side::Unstaged).len(), 1);

        let error = GitCli
            .discard_hunk(&dir, &repo_info, &patch)
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("git failed: "), "{error}");
    }
```

Integration test, in `crates/mkrew-daemon/tests/daemon/git.rs` right after the stage test (ends `:370`):

```rust
/// Review flow: one hunk of an unstaged change is discarded and the other stays; `confirm`,
/// `paths` together with a hunk, a path outside the repo and an untracked file are refused;
/// a hunk that no longer applies fails with git's text and changes nothing.
#[test]
fn git_discard_one_hunk_leaves_the_other_and_refuses_the_rest() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    git(&repo, &["config", "user.name", "t"]);
    git(&repo, &["config", "user.email", "t@t"]);
    git(&repo, &["config", "commit.gpgsign", "false"]);
    let lines: Vec<String> = (1..=20).map(|n| format!("line {n}\n")).collect();
    std::fs::write(repo.join("a.txt"), lines.concat()).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let mut changed = lines.clone();
    changed[1] = "line two\n".into();
    changed[18] = "line nineteen\n".into();
    std::fs::write(repo.join("a.txt"), changed.concat()).unwrap();
    std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let diff = |client: &mut Client| -> FileDiff {
        let request = Request::GitDiff(GitDiffParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            path: "a.txt".into(),
            mode: GitMode::Working,
            staged: false,
            commit: None,
            since: None,
        });
        client.call(request).into_result().unwrap()
    };
    let discard = |path: &str, paths: &[&str], hunk: &mkrew_protocol::Hunk, confirm| {
        Request::GitDiscard(GitDiscardParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            paths: paths.iter().map(|&p| p.to_owned()).collect(),
            hunk: Some(HunkTarget {
                path: path.into(),
                hunk: hunk.clone(),
            }),
            confirm,
        })
    };
    let read = || std::fs::read_to_string(repo.join("a.txt")).unwrap();

    let hunks = diff(&mut client).hunks;
    assert_eq!(hunks.len(), 2);
    for bad in [
        discard("a.txt", &[], &hunks[0], false),
        discard("a.txt", &["a.txt"], &hunks[0], true),
        discard("../x", &[], &hunks[0], true),
        discard("new.txt", &[], &hunks[0], true),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    assert_eq!(read(), changed.concat(), "the refusals changed nothing");

    let _: () = client
        .call(discard("a.txt", &[], &hunks[0], true))
        .into_result()
        .unwrap();
    let mut expected = lines.clone();
    expected[18] = "line nineteen\n".into();
    assert_eq!(read(), expected.concat());
    let left = diff(&mut client).hunks;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].lines, hunks[1].lines);
    assert_eq!(git(&repo, &["diff", "--cached", "--name-only"]), "");
    // The same hunk again no longer applies: git's reason, nothing changes.
    assert_eq!(
        error_code(client.call(discard("a.txt", &[], &hunks[0], true))),
        ErrorCode::Internal
    );
    assert_eq!(read(), expected.concat());
    client.stop();
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --lib discard_hunk`
Expected: FAIL to compile, `no method named discard_hunk found for struct GitCli`.

Run (after the next step compiles only the trait, or skip until Step 4): the integration test `git_discard_one_hunk_leaves_the_other_and_refuses_the_rest` fails at the valid discard with `invalid_request` ("git.discard needs paths"), because the daemon ignores `hunk`.

- [ ] **Step 3: Write the implementation**

`crates/mkrew-daemon/src/git.rs`, trait: after `fn discard(...)` (`:83-84`) add

```rust
    /// `git apply -R` of `patch` (its paths relative to `dir`) on the working tree only: the
    /// index is kept. Fails with git's reason when the patch no longer matches the file.
    fn discard_hunk(&self, dir: &Path, repo: &Repo, patch: &str) -> io::Result<()>;
```

`crates/mkrew-daemon/src/git.rs:488-501`: replace `apply_cached` by two methods sharing one helper:

```rust
    fn apply_cached(&self, dir: &Path, repo: &Repo, patch: &str, reverse: bool) -> io::Result<()> {
        let flags: &[&str] = if reverse {
            &["--cached", "-R"]
        } else {
            &["--cached"]
        };
        apply_patch(dir, repo, flags, patch)
    }

    fn discard_hunk(&self, dir: &Path, repo: &Repo, patch: &str) -> io::Result<()> {
        apply_patch(dir, repo, &["-R"], patch)
    }
```

and add this free function next to `run_with_input` (same file; find it with the `run_with_input` definition):

```rust
/// `git apply <flags> -` of `patch`. Patch paths are read from the repo root, so a project
/// that is a subfolder of the repo gets `--directory`.
fn apply_patch(dir: &Path, repo: &Repo, flags: &[&str], patch: &str) -> io::Result<()> {
    let mut command = git(dir);
    command.arg("apply").args(flags);
    let prefix = repo.prefix.trim_end_matches('/');
    if !prefix.is_empty() {
        command.arg(format!("--directory={prefix}"));
    }
    command.arg("-");
    run_with_input(command, patch.as_bytes()).map(drop)
}
```

`crates/mkrew-daemon/src/undo.rs:293-295`: after the `discard` double add

```rust
        fn discard_hunk(&self, _: &Path, _: &Repo, _: &str) -> io::Result<()> {
            unreachable!()
        }
```

`crates/mkrew-daemon/src/git_view.rs:42-43`: add a variant before `Git`:

```rust
    /// A change the request is not allowed to make (`invalid_request`).
    #[error("{0}")]
    Refused(String),
```

`crates/mkrew-daemon/src/server/git.rs:325-356` (`git_discard`) becomes:

```rust
    /// `git.discard`: the unstaged changes of `paths` are lost; tracked files go back to their
    /// index version, untracked ones are deleted. With `hunk` and no `paths`, only that hunk
    /// of a tracked file is reversed in the working tree (`git apply -R`; the index is kept).
    /// Refused without `confirm: true`.
    pub(super) async fn git_discard(&self, params: GitDiscardParams) -> Outcome {
        if !params.confirm {
            let message = "git.discard loses changes: it needs confirm: true";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let paths = match relative_paths(&params.paths)
            .and_then(|paths| repo_paths(params.repo.as_deref(), paths))
        {
            Ok(paths) => paths,
            Err(outcome) => return outcome,
        };
        let hunk = match params.hunk {
            Some(HunkTarget { path, hunk }) if paths.is_empty() => {
                match project_path(params.repo.as_deref(), &path) {
                    Ok(path) => Some((path, hunk)),
                    Err(outcome) => return outcome,
                }
            }
            Some(_) => {
                let message = "git.discard takes either paths or a hunk";
                return Outcome::error(ErrorCode::InvalidRequest, message);
            }
            None if paths.is_empty() => {
                return Outcome::error(ErrorCode::InvalidRequest, "git.discard needs paths");
            }
            None => None,
        };
        let target = (params.project_id, params.worktree_task_id, params.repo);
        self.git_change("git.discard", target, move |git, dir, repo| {
            if let Some((path, hunk)) = hunk {
                // An untracked file has no index text to go back to: its one "hunk" is the file.
                if !git.is_tracked(dir, &path)? {
                    let message = format!("{path} is not tracked: discard the file instead");
                    return Err(ViewError::Refused(message));
                }
                let patch = mkrew_core::git::hunk_patch(&path, &hunk);
                return Ok(git.discard_hunk(dir, repo, &patch)?);
            }
            let mut tracked = Vec::new();
            let mut untracked = Vec::new();
            for path in paths {
                if git.is_tracked(dir, &path)? {
                    tracked.push(path);
                } else {
                    untracked.push(path);
                }
            }
            if !tracked.is_empty() {
                git.discard(dir, &tracked)?;
            }
            Ok(git.clean(dir, &untracked)?)
        })
        .await
    }
```

`crates/mkrew-daemon/src/server/git.rs:587-609` (`view_outcome`): extend the doc comment with "a refused change = `invalid_request`;" and add this arm before the `ViewError::Git` arm:

```rust
        Ok(Err(e @ ViewError::Refused(_))) => {
            Outcome::error(ErrorCode::InvalidRequest, e.to_string())
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --lib discard_hunk && CARGO_BUILD_JOBS=4 cargo test -p mkrew-daemon --test daemon git_`
Expected: PASS (the new unit test, the new integration test, and the existing `git_stage_unstage_discard_and_commit_with_git_changed_events`, whose empty-`paths` discard is still `invalid_request`).

- [ ] **Step 5: Commit**

```bash
cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow
cargo fmt --all
git add crates/mkrew-daemon
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): git.discard of one hunk"
```

---

### Task 3: Git view - viewed, then next

**Files:**
- Modify: `crates/mkrew-ui/src/git_view.rs` (new method after `toggle_viewed`, `:1711-1728`)
- Modify: `crates/mkrew-ui/src/shell/git_viewed.rs:47-77`
- Modify: `crates/mkrew-ui/src/shell/git.rs` (new helper after `git_discard_selected`, `:684-694`)
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs:429-437` (tooltip)
- Test: `crates/mkrew-ui/src/git_view/tests.rs` (append at the end of the file)

**Interfaces:**
- Consumes: `GitView::toggle_viewed(&mut self, &Target) -> Option<GitSetViewedParams>` (git_view.rs:1711), `steppable()` (`:1386`), `select()` (`:1300`), `Requests` (`:546`).
- Produces: `GitView::next_unviewed(&mut self, target: &Target) -> Requests` (the file to read, in `Requests.diff`), `Shell::reveal_git_selection(&mut self, project_id: Ulid)`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/mkrew-ui/src/git_view/tests.rs`:

```rust
/// A Working view whose only listed files are the modified Changes files `names`, the first
/// one selected.
fn changes(names: &[&str]) -> GitView {
    let mut view = GitView::new(Ulid::new(), None);
    assert!(view.begin_status().is_some());
    let status = GitStatus {
        unstaged: names
            .iter()
            .map(|name| file(name, ChangeKind::Modified))
            .collect(),
        ..GitStatus::default()
    };
    let requests = view.status_loaded(Ok(status));
    assert_eq!(requests.diff, Some(target(Group::Unstaged, names[0])));
    view
}

/// The daemon answers that the Changes files `names` are viewed.
fn mark_viewed(view: &mut GitView, names: &[&str]) {
    let (params, serial) = view.begin_viewed();
    let files = names.iter().map(|name| viewed_file(name, false)).collect();
    view.viewed_loaded(serial, (params.mode, params.since), files);
}

fn unstaged(path: &str) -> Target {
    target(Group::Unstaged, path)
}

#[test]
fn marking_a_file_viewed_selects_the_next_one_that_is_not_viewed() {
    let mut view = changes(&["a.rs", "b.rs", "c.rs"]);
    // Not viewed yet: nothing moves.
    assert_eq!(view.next_unviewed(&unstaged("a.rs")), Requests::default());
    view.toggle_viewed(&unstaged("a.rs")).unwrap();
    let moved = view.next_unviewed(&unstaged("a.rs"));
    assert_eq!(moved.diff, Some(unstaged("b.rs")));
    assert_eq!(view.selected, Some(unstaged("b.rs")));
    // The optimistic marks decide: a second `v` does not jump back to a.rs.
    view.toggle_viewed(&unstaged("b.rs")).unwrap();
    assert_eq!(
        view.next_unviewed(&unstaged("b.rs")).diff,
        Some(unstaged("c.rs"))
    );
    // Every file viewed: the selection stays and nothing is requested.
    view.toggle_viewed(&unstaged("c.rs")).unwrap();
    assert_eq!(view.next_unviewed(&unstaged("c.rs")), Requests::default());
    assert_eq!(view.selected, Some(unstaged("c.rs")));
}

#[test]
fn next_unviewed_wraps_to_the_top_and_skips_viewed_files() {
    let mut view = changes(&["a.rs", "b.rs", "c.rs"]);
    mark_viewed(&mut view, &["b.rs"]);
    view.select(unstaged("c.rs"));
    view.toggle_viewed(&unstaged("c.rs")).unwrap();
    // Nothing below c.rs: wraps to a.rs (b.rs is viewed).
    assert_eq!(
        view.next_unviewed(&unstaged("c.rs")).diff,
        Some(unstaged("a.rs"))
    );
}

#[test]
fn v_on_a_viewed_file_only_clears_the_mark() {
    let mut view = changes(&["a.rs", "b.rs"]);
    mark_viewed(&mut view, &["a.rs"]);
    assert!(!view.toggle_viewed(&unstaged("a.rs")).unwrap().viewed);
    assert_eq!(view.next_unviewed(&unstaged("a.rs")), Requests::default());
    assert_eq!(view.selected, Some(unstaged("a.rs")));
}

#[test]
fn marking_another_row_or_a_history_file_moves_nothing() {
    let mut view = changes(&["a.rs", "b.rs", "c.rs"]);
    // a.rs is selected; the box of b.rs was ticked.
    view.toggle_viewed(&unstaged("b.rs")).unwrap();
    assert_eq!(view.next_unviewed(&unstaged("b.rs")), Requests::default());
    assert_eq!(view.selected, Some(unstaged("a.rs")));
    let _ = view.show(Section::History);
    assert_eq!(view.next_unviewed(&unstaged("a.rs")), Requests::default());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib git_view`
Expected: FAIL to compile, `no method named next_unviewed found for struct GitView`.

- [ ] **Step 3: Write the implementation**

`crates/mkrew-ui/src/git_view.rs`, right after `toggle_viewed` (ends `:1728`):

```rust
    /// After `v` marked `target` viewed: selects the next file below it that is not viewed,
    /// wrapping to the top (ADR 0103); the file's diff is in the returned requests, as for an
    /// arrow key. Nothing moves when `target` is not the selected file (a row's box), is not
    /// viewed (the mark was cleared), is a History file, or every file is viewed. Uses the
    /// local marks, so a fast second `v` does not jump back.
    pub fn next_unviewed(&mut self, target: &Target) -> Requests {
        let marked = self.viewed.contains(target.group, &target.path);
        if self.history.is_some() || self.selected.as_ref() != Some(target) || !marked {
            return Requests::default();
        }
        let files = self.steppable();
        let Some(at) = files.iter().position(|(listed, _)| listed == target) else {
            return Requests::default();
        };
        let next = (1..files.len())
            .map(|by| &files[(at + by) % files.len()].0)
            .find(|listed| !self.viewed.contains(listed.group, &listed.path))
            .cloned();
        Requests {
            diff: next.and_then(|next| self.select(next)),
            ..Requests::default()
        }
    }
```

`crates/mkrew-ui/src/shell/git.rs`, after `git_discard_selected` (`:684-694`) add:

```rust
    /// Scrolls the file list so the selected file shows (the selection moved by itself).
    pub(crate) fn reveal_git_selection(&mut self, project_id: Ulid) {
        let Some(view) = self.git.get(&project_id) else {
            return;
        };
        let row = view.list_rows().iter().position(|row| {
            matches!(row, git_view::ListRow::File(target, _) if view.selected.as_ref() == Some(target))
        });
        if let Some(at) = row {
            self.git_files_scroll
                .scroll_to_item(at, ScrollStrategy::Nearest);
        }
    }
```

`crates/mkrew-ui/src/shell/git_viewed.rs:57-61` becomes:

```rust
        let Some(params) = self.git_view(project_id).toggle_viewed(&target) else {
            return;
        };
        // Marked: the review goes on with the next file that is not viewed.
        let next = self.git_view(project_id).next_unviewed(&target);
        if next.diff.is_some() {
            self.scroll_diff_to(0);
            self.reveal_git_selection(project_id);
        }
        self.git_requests(project_id, next, window, cx);
        cx.notify();
```

and update the doc comment above `toggle_git_viewed` (`:44-46`): append "Marking the selected file opens the next one that is not viewed."

`crates/mkrew-ui/src/shell/git_diff.rs:432-434`: the tooltip string `"Mark as viewed: its diff collapses until it changes"` becomes `"Mark as viewed: its diff collapses until it changes, and the next file opens"`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib git_view && CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: PASS, and the crate (shell included) compiles. (If `scroll_to_item` needs `&self` or `&mut self`, follow the compiler: the helper takes `&mut self` so either works.)

- [ ] **Step 5: Commit**

```bash
cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow
cargo fmt --all
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): marking a file viewed opens the next one that is not viewed"
```

---

### Task 4: Git view - `n` / `p` cross files

**Files:**
- Modify: `crates/mkrew-ui/src/git_view.rs`: imports none; new enum before `GitView` (`:602`), fields (`:615`, `:649`, new `:684`), `select_none` (`:948-952`), `select` (`:1300-1311`), `diff_loaded` (`:1327-1346`), `key` (`:1367-1383`), `step_hunk` (`:1499-1518`), `Key` docs (`:441-444`)
- Modify: `crates/mkrew-ui/src/shell/git.rs:518-529` (`load_git_diff`), `:652-657` (`git_key`)
- Test: `crates/mkrew-ui/src/git_view/tests.rs:413-438` (update), append new tests

**Interfaces:**
- Consumes: `changes`, `mark_viewed`, `unstaged` test helpers and `viewed_file` (Task 3), `hunk`, `laid_out`, `target` (tests.rs:28-73).
- Produces: `PendingHunk::{First, Last}`, `GitView::take_hunk_reveal(&mut self) -> Option<usize>`, `step_hunk(&mut self, forward: bool) -> Requests`; `Key::NextHunk` / `Key::PreviousHunk` now return the next file's diff request at the edges.

- [ ] **Step 1: Write the failing tests**

First update the one existing test whose behavior changes on purpose. In `crates/mkrew-ui/src/git_view/tests.rs`, `keys_move_files_and_hunks`, replace lines `:413-438` (from `// Hunks: none shown yet` to the closing `}` of the test) with:

```rust
    // Hunks: none shown yet → nothing; then first / last, and moves inside the file.
    view.key(Key::NextHunk);
    assert_eq!(view.hunk, None);
    let shown = target(Group::Unstaged, "b.rs");
    view.begin_diff();
    view.diff_loaded(
        &shown,
        Ok(laid_out(vec![hunk(1, 1, 1, 1), hunk(20, 1, 20, 1)])),
    );
    view.key(Key::PreviousHunk);
    assert_eq!(view.hunk, Some(1));
    view.key(Key::PreviousHunk);
    assert_eq!(view.hunk, Some(0));
    view.key(Key::NextHunk);
    assert_eq!(view.hunk, Some(1));
    // A refreshed diff with fewer hunks drops a hunk that no longer exists.
    view.begin_diff();
    view.diff_loaded(&shown, Ok(laid_out(vec![hunk(1, 1, 1, 1)])));
    assert_eq!(view.hunk, None);
    // Another file clears it.
    view.key(Key::NextHunk);
    view.key(Key::Up);
    assert_eq!(view.hunk, None);
}
```

(The old assertions that `n` at the last hunk and `p` at the first hunk stay put are replaced by the cross-file tests below.)

Then append:

```rust
/// Shows `count` hunks as the diff of `target`, as the shell does once `git.diff` answers.
fn show_hunks(view: &mut GitView, target: &Target, count: u32) {
    let hunks = (0..count)
        .map(|n| hunk(n * 10 + 1, 1, n * 10 + 1, 1))
        .collect();
    view.begin_diff();
    view.diff_loaded(target, Ok(laid_out(hunks)));
}

#[test]
fn n_and_p_past_the_ends_go_on_to_the_next_and_previous_file() {
    let mut view = changes(&["a.rs", "b.rs"]);
    show_hunks(&mut view, &unstaged("a.rs"), 2);
    view.key(Key::NextHunk);
    view.key(Key::NextHunk);
    assert_eq!(view.hunk, Some(1));
    // Past the last hunk: b.rs, and its first hunk once its diff is shown.
    let moved = view.key(Key::NextHunk);
    assert_eq!(moved.diff, Some(unstaged("b.rs")));
    assert_eq!(view.selected, Some(unstaged("b.rs")));
    assert_eq!(view.hunk, None);
    assert_eq!(view.take_hunk_reveal(), None, "nothing to reveal yet");
    show_hunks(&mut view, &unstaged("b.rs"), 3);
    assert_eq!(view.hunk, Some(0));
    assert_eq!(view.take_hunk_reveal(), Some(0));
    assert_eq!(view.take_hunk_reveal(), None, "revealed once");
    // Before the first hunk: a.rs, and its last hunk.
    let back = view.key(Key::PreviousHunk);
    assert_eq!(back.diff, Some(unstaged("a.rs")));
    show_hunks(&mut view, &unstaged("a.rs"), 2);
    assert_eq!(view.hunk, Some(1));
    assert_eq!(view.take_hunk_reveal(), Some(1));
    // A plain refresh of the same file reveals nothing and keeps the hunk.
    show_hunks(&mut view, &unstaged("a.rs"), 2);
    assert_eq!(view.hunk, Some(1));
    assert_eq!(view.take_hunk_reveal(), None);
}

#[test]
fn n_and_p_skip_viewed_files_and_stop_at_the_ends() {
    let mut view = changes(&["a.rs", "b.rs", "c.rs"]);
    mark_viewed(&mut view, &["b.rs"]);
    show_hunks(&mut view, &unstaged("a.rs"), 1);
    view.key(Key::NextHunk);
    assert_eq!(view.hunk, Some(0));
    // b.rs is viewed: c.rs is next.
    assert_eq!(
        view.key(Key::NextHunk).diff,
        Some(unstaged("c.rs"))
    );
    show_hunks(&mut view, &unstaged("c.rs"), 1);
    assert_eq!(view.hunk, Some(0));
    // No wrap at the end.
    assert_eq!(view.key(Key::NextHunk), Requests::default());
    assert_eq!(view.selected, Some(unstaged("c.rs")));
    assert_eq!(view.hunk, Some(0));
    // Back over the viewed file to a.rs, and no wrap at the start.
    assert_eq!(
        view.key(Key::PreviousHunk).diff,
        Some(unstaged("a.rs"))
    );
    show_hunks(&mut view, &unstaged("a.rs"), 1);
    assert_eq!(view.key(Key::PreviousHunk), Requests::default());
    assert_eq!(view.selected, Some(unstaged("a.rs")));
}

#[test]
fn a_file_without_hunks_keeps_the_pending_target_and_n_moves_on_again() {
    let mut view = changes(&["a.rs", "bin.png", "c.rs"]);
    show_hunks(&mut view, &unstaged("a.rs"), 1);
    view.key(Key::NextHunk);
    assert_eq!(
        view.key(Key::NextHunk).diff,
        Some(unstaged("bin.png"))
    );
    // A binary file: its diff has no hunks.
    show_hunks(&mut view, &unstaged("bin.png"), 0);
    assert_eq!(view.hunk, None);
    assert_eq!(view.pending_hunk, Some(PendingHunk::First));
    assert_eq!(view.key(Key::NextHunk).diff, Some(unstaged("c.rs")));
    show_hunks(&mut view, &unstaged("c.rs"), 2);
    assert_eq!(view.hunk, Some(0));
    assert_eq!(view.pending_hunk, None);
}

#[test]
fn a_click_clears_the_pending_target() {
    let mut view = changes(&["a.rs", "b.rs"]);
    show_hunks(&mut view, &unstaged("a.rs"), 1);
    view.key(Key::NextHunk);
    view.key(Key::NextHunk);
    assert_eq!(view.selected, Some(unstaged("b.rs")));
    // A click on the selected row changes nothing else, but drops the target.
    assert_eq!(view.select(unstaged("b.rs")), None);
    show_hunks(&mut view, &unstaged("b.rs"), 2);
    assert_eq!(view.hunk, None);
    // A click on another file does too: `p` three times goes back to a.rs with a pending
    // last hunk, and a click on b.rs drops it.
    view.key(Key::PreviousHunk);
    view.key(Key::PreviousHunk);
    assert_eq!(view.key(Key::PreviousHunk).diff, Some(unstaged("a.rs")));
    assert_eq!(view.pending_hunk, Some(PendingHunk::Last));
    assert!(view.select(unstaged("b.rs")).is_some());
    assert_eq!(view.pending_hunk, None);
}

#[test]
fn in_the_history_n_and_p_move_between_the_commits_files_only() {
    let mut view = history();
    let (x, y) = (commit_target("b", "x"), commit_target("b", "y"));
    show_hunks(&mut view, &x, 1);
    view.key(Key::NextHunk);
    assert_eq!(view.hunk, Some(0));
    // Past the last hunk of x: y, the commit's next file (the History has no viewed marks).
    assert_eq!(view.key(Key::NextHunk).diff, Some(y.clone()));
    show_hunks(&mut view, &y, 1);
    assert_eq!(view.hunk, Some(0));
    // Past the last file: nothing, and never another commit.
    assert_eq!(view.key(Key::NextHunk), Requests::default());
    assert_eq!(view.selected, Some(y));
    assert_eq!(view.key(Key::PreviousHunk).diff, Some(x.clone()));
    show_hunks(&mut view, &x, 1);
    assert_eq!(view.key(Key::PreviousHunk), Requests::default());
    assert_eq!(view.selected, Some(x));
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib git_view`
Expected: FAIL to compile, `cannot find type PendingHunk` and `no method named take_hunk_reveal`.

- [ ] **Step 3: Write the implementation**

`crates/mkrew-ui/src/git_view.rs`, before `pub struct GitView` (`:602`):

```rust
/// Which hunk of the next file a cross-file `n` / `p` selects once that file's diff is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingHunk {
    First,
    Last,
}
```

Fields: in `GitView`, after `pub hunk: Option<usize>,` (`:616`) add

```rust
    /// `n` / `p` went on to another file: the hunk to select when its diff is shown. Cleared by
    /// any other selection change; a file without hunks keeps it.
    pending_hunk: Option<PendingHunk>,
    /// The hunk a pending target just selected, until the shell has scrolled to it.
    reveal_hunk: Option<usize>,
```

and in `GitView::new` after `hunk: None,` (`:684`) add `pending_hunk: None,` and `reveal_hunk: None,`.

`select_none` (`:948`) gets `self.pending_hunk = None;` after `self.hunk = None;`.

`select` (`:1300`): first statement of the body, before the early-return check:

```rust
        // Any click, arrow or refresh that picks a file drops a pending `n` / `p` target.
        self.pending_hunk = None;
```

`diff_loaded` (`:1336-1341`): the `Ok(shown)` arm becomes

```rust
                Ok(shown) => {
                    let count = shown.diff.hunks.len();
                    if self.hunk.is_some_and(|hunk| hunk >= count) {
                        self.hunk = None;
                    }
                    // A cross-file `n` / `p` picks its hunk now; a file without hunks keeps
                    // the target, so the next `n` / `p` moves on again.
                    if let Some(pending) = self.pending_hunk.filter(|_| count > 0) {
                        let hunk = match pending {
                            PendingHunk::First => 0,
                            PendingHunk::Last => count - 1,
                        };
                        self.hunk = Some(hunk);
                        self.reveal_hunk = Some(hunk);
                        self.pending_hunk = None;
                    }
                    Diff::Shown(Rc::new(shown))
                }
```

After `diff_loaded` add:

```rust
    /// The hunk a cross-file `n` / `p` just selected, once: the shell scrolls the diff to it.
    pub fn take_hunk_reveal(&mut self) -> Option<usize> {
        self.reveal_hunk.take()
    }
```

`key` (`:1371-1378`):

```rust
            Key::NextHunk => self.step_hunk(true),
            Key::PreviousHunk => self.step_hunk(false),
```

and the `Key` docs (`:441-444`): `/// n: next hunk; past the last one, the next file that is not viewed.` and `/// p: previous hunk; before the first one, the previous file that is not viewed.`

`step_hunk` (`:1499-1518`) becomes:

```rust
    /// The next / previous hunk of the shown diff (from none: the first / last). Past the last
    /// (first) hunk it goes on to the next (previous) file that is not viewed and, once that
    /// diff is shown, to its first (last) hunk ([`PendingHunk`]); no wrap at the list ends. A
    /// file without hunks counts as past both ends. A collapsed (viewed) diff ignores the keys.
    fn step_hunk(&mut self, forward: bool) -> Requests {
        let Diff::Shown(shown) = &self.diff else {
            return Requests::default();
        };
        if self.diff_collapsed() {
            return Requests::default();
        }
        let count = shown.diff.hunks.len();
        let next = match (self.hunk, forward) {
            _ if count == 0 => None,
            (None, true) => Some(0),
            (None, false) => Some(count - 1),
            (Some(hunk), true) => (hunk + 1 < count).then_some(hunk + 1),
            (Some(hunk), false) => hunk.checked_sub(1),
        };
        if next.is_some() {
            self.hunk = next;
            return Requests::default();
        }
        self.step_hunk_file(forward)
    }

    /// `n` / `p` ran out of hunks: selects the next / previous listed file that is not viewed
    /// (every file counts in the History, which has no marks and never leaves its commit) and
    /// remembers which of its hunks to select. `step_file` is not used: it cannot skip viewed
    /// files and would step to another commit.
    fn step_hunk_file(&mut self, forward: bool) -> Requests {
        let files = self.steppable();
        let Some(at) = self
            .selected
            .as_ref()
            .and_then(|selected| files.iter().position(|(target, _)| target == selected))
        else {
            return Requests::default();
        };
        let open = |(target, _): &&(Target, ChangedFile)| {
            self.history.is_some() || !self.viewed.contains(target.group, &target.path)
        };
        let next = if forward {
            files[at + 1..].iter().find(open)
        } else {
            files[..at].iter().rev().find(open)
        }
        .map(|(target, _)| target.clone());
        let Some(next) = next else {
            return Requests::default();
        };
        let diff = self.select(next);
        self.pending_hunk = Some(if forward {
            PendingHunk::First
        } else {
            PendingHunk::Last
        });
        Requests {
            diff,
            ..Requests::default()
        }
    }
```

Shell, `crates/mkrew-ui/src/shell/git.rs:518-529` (`load_git_diff`'s update closure) becomes:

```rust
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &loaded {
                    tracing::warn!(%project_id, path = target.path, "git.diff failed: {error}");
                }
                let mode = shell.diff_mode;
                let mut reveal = None;
                let again = shell.git_at(project_id, &at).and_then(|view| {
                    let again = view.diff_loaded(&target, loaded);
                    // A cross-file `n` / `p` picked its hunk: scroll to it (not on a refresh).
                    reveal = view.take_hunk_reveal().and_then(|hunk| match &view.diff {
                        git_view::Diff::Shown(shown) => shown.hunk_row(hunk, mode),
                        _ => None,
                    });
                    again
                });
                if let Some(row) = reveal {
                    shell.scroll_diff_to(row);
                }
                if let Some(target) = again {
                    shell.load_git_diff(project_id, target, window, cx);
                }
                cx.notify();
            });
```

and `git_key`'s hunk arm (`:652-657`) becomes:

```rust
            git_view::Key::NextHunk | git_view::Key::PreviousHunk => {
                // Into another file: its diff loads first, then its hunk is scrolled to
                // (`load_git_diff`).
                if requests.diff.is_some() {
                    self.scroll_diff_to(0);
                } else if let Some(at) = hunk_row {
                    self.scroll_diff_to(at);
                }
            }
```

Update the module doc of `git_view.rs` only if it mentions "stops at the ends" (`step_hunk` doc was the only place).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib git_view && CARGO_BUILD_JOBS=4 cargo check -p mkrew-ui --all-targets`
Expected: PASS, including the unchanged `a_collapsed_diff_ignores_hunk_keys_and_stages_the_whole_file`, `shift_down_picks_the_first_line_of_the_current_hunk_and_grows_across_hunks` and `a_last_hunk_that_only_removes_lines_picks_the_last_line`.

- [ ] **Step 5: Commit**

```bash
cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow
cargo fmt --all
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): n and p go on to the next and previous file"
```

---

### Task 5: Git view - discard one hunk

**Files:**
- Modify: `crates/mkrew-ui/src/git_view.rs`: import `Hunk` (`:16-21`), `Discard` (`:495-538`), `GitView::discard` (`:1753-1773`), new `hunks_discardable`, `hunk_discard`, `discard_key` after it
- Modify: `crates/mkrew-ui/src/shell/git_dialogs.rs:6-23`, `:214-385`
- Modify: `crates/mkrew-ui/src/shell/git.rs:684-694`
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs:600-611`, `:626-636`, `:791-824` (add button), `:836`, `:855-923`
- Modify: `crates/mkrew-ui/src/keymap.rs:139`, `crates/mkrew-ui/src/palette.rs:639`
- Test: `crates/mkrew-ui/src/git_view/tests.rs` (append)

**Interfaces:**
- Consumes: `GitDiscardParams.hunk` (Task 1), `show_hunks`, `changes`, `mark_viewed`, `unstaged`, `two_hunks` (tests.rs:1632), the daemon behaviour of Task 2.
- Produces: `Discard.hunk: Option<(usize, Hunk)>`, `Discard::{title, confirm_label, preview_rows, hunk_is_current}`, `GitView::{hunks_discardable, hunk_discard, discard_key}`, `Shell::{open_git_hunk_discard, show_git_discard}`, `hunk_discard_button`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/mkrew-ui/src/git_view/tests.rs`:

```rust
#[test]
fn a_hunk_of_a_modified_changes_file_can_be_discarded_alone() {
    let mut view = changes(&["a.rs", "b.rs"]);
    // Diff still loading: no hunk to name.
    assert!(!view.hunks_discardable());
    assert_eq!(view.hunk_discard(0), None);
    show_hunks(&mut view, &unstaged("a.rs"), 2);
    assert!(view.hunks_discardable());
    let discard = view.hunk_discard(1).unwrap();
    assert_eq!(
        discard.hunk.as_ref().map(|(index, hunk)| (*index, hunk.old_start)),
        Some((1, 11))
    );
    assert_eq!(discard.title(), "Discard this hunk?");
    assert_eq!(discard.confirm_label(), "Discard hunk");
    assert!(discard.sentence().contains("This hunk of “a.rs”"));
    let params = discard.params();
    assert!(params.confirm && params.paths.is_empty());
    let named = params.hunk.unwrap();
    assert_eq!((named.path.as_str(), named.hunk.old_start), ("a.rs", 11));
    assert_eq!(view.hunk_discard(2), None, "no such hunk");
    // The file's own confirmation is unchanged.
    let file = view.discard(&unstaged("a.rs")).unwrap();
    assert_eq!(file.title(), "Discard changes?");
    assert_eq!(file.params().paths, ["a.rs"]);
    assert_eq!(file.params().hunk, None);
}

#[test]
fn x_takes_the_current_hunk_else_the_file() {
    let mut view = changes(&["a.rs"]);
    show_hunks(&mut view, &unstaged("a.rs"), 2);
    assert_eq!(view.discard_key().unwrap().hunk, None, "no current hunk");
    view.key(Key::PreviousHunk);
    assert_eq!(
        view.discard_key().unwrap().hunk.map(|(index, _)| index),
        Some(1)
    );
    // A viewed file's diff is folded: its hidden hunk is not discarded blind.
    mark_viewed(&mut view, &["a.rs"]);
    assert!(view.diff_collapsed());
    assert_eq!(view.discard_key().unwrap().hunk, None);
}

#[test]
fn staged_untracked_and_deleted_files_have_no_hunk_discard() {
    let mut view = loaded();
    let staged = target(Group::Staged, "a.rs");
    show_hunks(&mut view, &staged, 1);
    assert!(!view.hunks_discardable(), "staged");
    assert_eq!(view.discard_key(), None, "staged files have no discard");
    let gone = unstaged("b.rs");
    view.select(gone.clone());
    show_hunks(&mut view, &gone, 1);
    assert!(!view.hunks_discardable(), "deleted");
    assert_eq!(view.hunk_discard(0), None);
    assert_eq!(view.discard_key().unwrap().hunk, None, "the file goes by path");
    let new = target(Group::Untracked, "new.txt");
    view.select(new.clone());
    show_hunks(&mut view, &new, 1);
    assert_eq!(view.hunk_discard(0), None, "untracked");
    assert_eq!(view.discard_key().unwrap().hunk, None);
}

#[test]
fn the_hunk_dialog_lists_only_its_rows_and_stops_when_the_hunk_changed() {
    let mut view = changes(&["a.rs"]);
    let a = unstaged("a.rs");
    view.begin_diff();
    view.diff_loaded(&a, Ok(two_hunks()));
    let shown = view.shown_diff(&a).unwrap();
    // Rows: Hunk(0) + 4 lines, Gap, Hunk(1) + 2 lines.
    assert_eq!(shown.rows.len(), 9);
    let second = view.hunk_discard(1).unwrap();
    assert_eq!(second.preview_rows(&shown), (6..9, 0));
    let first = view.hunk_discard(0).unwrap();
    assert_eq!(first.preview_rows(&shown), (0..5, 0));
    let file = view.discard(&a).unwrap();
    assert_eq!(file.preview_rows(&shown), (0..9, 0));
    assert!(second.hunk_is_current(&shown));
    // The diff reloads (an agent edited the file): the dialog's hunk is no longer there.
    view.begin_diff();
    view.diff_loaded(&a, Ok(laid_out(vec![hunk(1, 1, 1, 1)])));
    let reloaded = view.shown_diff(&a).unwrap();
    assert!(!second.hunk_is_current(&reloaded));
    assert!(file.hunk_is_current(&reloaded), "a file discard has no hunk");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib git_view`
Expected: FAIL to compile, `no method named hunks_discardable` and `no field hunk on type Discard`.

- [ ] **Step 3: Write the implementation**

`crates/mkrew-ui/src/git_view.rs` imports (`:16-21`): add `Hunk` to the `mkrew_protocol` list (after `GitStatusParams`... keep it alphabetical: `..., GitStatusParams, Hunk, HunkTarget, Request, ...`).

`Discard` (`:495-538`): doc becomes "The discard confirmation of one Unstaged / Untracked file, or of one hunk of an Unstaged file; the dialog shows the file's diff (the view's shown diff once the file is selected), or that hunk, as what is lost." Add the field after `error`:

```rust
    /// One hunk of the file instead of the whole file: its index in the shown diff and the
    /// hunk itself (what the dialog lists and the daemon reverses).
    pub hunk: Option<(usize, Hunk)>,
```

and these methods in `impl Discard` (replace `sentence` and `params`, add the rest):

```rust
    /// The dialog's title.
    pub fn title(&self) -> &'static str {
        if self.hunk.is_some() {
            "Discard this hunk?"
        } else {
            "Discard changes?"
        }
    }

    /// The danger button's label.
    pub fn confirm_label(&self) -> &'static str {
        if self.hunk.is_some() {
            "Discard hunk"
        } else {
            "Discard changes"
        }
    }

    /// What confirming does, in one sentence.
    pub fn sentence(&self) -> String {
        let path = &self.target.path;
        if self.hunk.is_some() {
            return format!(
                "This hunk of “{path}” is discarded and cannot be recovered. Its other changes and its staged changes stay."
            );
        }
        if self.target.group == Group::Untracked {
            return format!(
                "“{path}” is deleted. It was never committed, so it cannot be recovered."
            );
        }
        let staged = if self.also_staged {
            " Its staged changes stay."
        } else {
            ""
        };
        format!(
            "The unstaged changes to “{path}” below are discarded and cannot be recovered.{staged}"
        )
    }

    /// The `git.discard` to send (only ever after the user confirmed): the file, or with a
    /// hunk only that hunk (`paths` is empty then).
    pub fn params(&self) -> GitDiscardParams {
        let hunk = self.hunk.as_ref().map(|(_, hunk)| HunkTarget {
            path: self.target.path.clone(),
            hunk: hunk.clone(),
        });
        GitDiscardParams {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            repo: self.repo.clone(),
            paths: if hunk.is_some() {
                vec![]
            } else {
                vec![self.target.path.clone()]
            },
            hunk,
            confirm: true,
        }
    }

    /// The rows of `shown` the dialog lists (at most [`DISCARD_PREVIEW_ROWS`]) and how many
    /// more there are: the whole diff, or the rows of the one hunk.
    pub fn preview_rows(&self, shown: &Shown) -> (Range<usize>, usize) {
        let Some((index, _)) = &self.hunk else {
            let listed = shown.rows.len().min(DISCARD_PREVIEW_ROWS);
            return (0..listed, shown.rows.len() - listed);
        };
        let Some(start) = shown
            .rows
            .iter()
            .position(|row| *row == DiffRow::Hunk(*index))
        else {
            return (0..0, 0);
        };
        let lines = shown.rows[start + 1..]
            .iter()
            .take_while(|row| matches!(row, DiffRow::Line(_)))
            .count();
        let end = start + 1 + lines;
        let stop = end.min(start + DISCARD_PREVIEW_ROWS);
        (start..stop, end - stop)
    }

    /// `shown` (the diff reloads while the dialog is open) still has this hunk at its place:
    /// confirming then discards what the dialog lists. A file discard has no hunk to check.
    pub fn hunk_is_current(&self, shown: &Shown) -> bool {
        self.hunk
            .as_ref()
            .is_none_or(|(index, hunk)| shown.diff.hunks.get(*index) == Some(hunk))
    }
```

(`DISCARD_PREVIEW_ROWS` is defined right after `impl Discard`, `:541`; leave it there.)

`GitView::discard` (`:1764-1772`): add `hunk: None,` to the `Discard { ... }` literal. After `discard` add:

```rust
    /// Hunk headers offer "Discard hunk": the selected file is a modified one of the Changes
    /// group and its diff is shown. (Only modified files go by hunk: the patch has no
    /// file-mode headers. Cheap: the render asks it every frame.)
    pub fn hunks_discardable(&self) -> bool {
        self.selected
            .as_ref()
            .filter(|selected| selected.group == Group::Unstaged)
            .is_some_and(|selected| {
                matches!(self.diff, Diff::Shown(_))
                    && self
                        .file(selected)
                        .is_some_and(|file| file.kind == ChangeKind::Modified)
            })
    }

    /// The discard confirmation of hunk `hunk` of the shown diff.
    pub fn hunk_discard(&self, hunk: usize) -> Option<Discard> {
        if !self.hunks_discardable() {
            return None;
        }
        let Diff::Shown(shown) = &self.diff else {
            return None;
        };
        let target = shown.diff.hunks.get(hunk)?.clone();
        Some(Discard {
            hunk: Some((hunk, target)),
            ..self.discard(self.selected.as_ref()?)?
        })
    }

    /// `x`: the current hunk's confirmation when there is one that can go by hunk (a folded
    /// diff hides its hunks), else the selected file's.
    pub fn discard_key(&self) -> Option<Discard> {
        let hunk = self
            .hunk
            .filter(|_| !self.diff_collapsed())
            .and_then(|hunk| self.hunk_discard(hunk));
        hunk.or_else(|| self.discard(self.selected.as_ref()?))
    }
```

Shell, `crates/mkrew-ui/src/shell/git_dialogs.rs:6-23`: `open_git_discard` keeps its first three statements and its last four become one call; add the shared opener and the hunk opener after it:

```rust
    pub(crate) fn open_git_discard(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(discard) = self.git_view(project_id).discard(&target) else {
            return;
        };
        self.select_git_file(project_id, target, window, cx);
        self.show_git_discard(discard, window, cx);
    }

    /// "Discard hunk" and `x` with a current hunk: the confirmation for that hunk of the shown
    /// diff. It opens over the diff as it is: no selection change, no scroll.
    pub(crate) fn open_git_hunk_discard(
        &mut self,
        project_id: Ulid,
        hunk: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(discard) = self.git_view(project_id).hunk_discard(hunk) {
            self.show_git_discard(discard, window, cx);
        }
    }

    /// Shows the discard confirmation `discard`; the keyboard goes to the dialog.
    pub(crate) fn show_git_discard(
        &mut self,
        discard: git_view::Discard,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.git_discard = Some(discard);
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }
```

`render_git_discard` (`git_dialogs.rs`, line numbers from `:214`):
- after the `failed` binding (`:240-245`) add
  `let stale = shown.as_deref().is_some_and(|shown| !dialog.hunk_is_current(shown));`
- in the `body` match add, right after the `_ if gone => ...` arm (`:247`):
  `_ if stale => muted("This hunk changed meanwhile. Close this and choose it again.".into()).into_any_element(),`
- the `(Some(shown), _) =>` arm (`:259-292`): replace the `let more = ...` statement by `let (rows, more) = dialog.preview_rows(shown);` and the `.children((0..shown.rows.len().min(git_view::DISCARD_PREVIEW_ROWS)).map(|index| {...}))` by
  ```rust
                            .children(rows.map(|index| {
                                git_diff_preview_row(shown, index, project_id, theme, cx)
                            })),
  ```
- `let ready = shown.is_some() && !gone && !dialog.busy;` (`:322`) becomes `let ready = shown.is_some() && !gone && !stale && !dialog.busy;`
- the title (`:341`): `.child(dialog.title())`
- the confirm button (`:366-380`): `danger_button("git-discard-confirm", dialog.confirm_label(), theme)` and, in the disabled branch, `let label = if dialog.busy { "Discarding…" } else { dialog.confirm_label() };`
- update the doc comment (`:214-215`): "\"Discard changes?\" / \"Discard this hunk?\" of a Git view file or hunk: what is lost (the file's diff, or that hunk), ...".

`crates/mkrew-ui/src/shell/git.rs:684-694` (`git_discard_selected`) becomes:

```rust
    /// `x`: the confirmation of the current hunk when there is one that can go by hunk, else
    /// of the selected file (nothing for a file that cannot be discarded: `open_git_discard`
    /// checks).
    pub(crate) fn git_discard_selected(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(view) = self.git.get(&project_id) else {
            return;
        };
        let target = view.selected.clone();
        if let Some(discard) = view.discard_key().filter(|discard| discard.hunk.is_some()) {
            return self.show_git_discard(discard, window, cx);
        }
        if let Some(target) = target {
            self.open_git_discard(project_id, target, window, cx);
        }
    }
```

`crates/mkrew-ui/src/shell/git_diff.rs`:
- after `hunk_stage_button` (`:791-824`) add

```rust
/// "Discard hunk" after "Stage hunk" on a hunk header of an unstaged modified file: danger
/// text, asks first. `keys` is the shortcut, shown on the current hunk only (`x` acts there).
pub(crate) fn hunk_discard_button(
    project_id: Ulid,
    hunk: usize,
    keys: SharedString,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    row_action(
        SharedString::from(format!("git-hunk-discard-{hunk}")),
        "Discard hunk",
        theme,
    )
    .h(px(space::XL))
    .flex()
    .items_center()
    .px(px(space::S))
    .rounded(px(radius::CHIP))
    .text_color(rgb(theme.danger_text))
    .tooltip(tooltip(theme, "Discard this hunk (asks first)", keys))
    .on_click(cx.listener(move |shell, _, window, cx| {
        window.focus(&shell.git_focus, cx);
        shell.open_git_hunk_discard(project_id, hunk, window, cx);
    }))
}
```

- `span_row` (`:855-923`): add a parameter `discard_keys: Option<&str>,` after `stage_keys: &str,` and, after the `.children(hunk_stage.map(...))` (`:915-922`), add

```rust
                .children(discard_keys.map(|keys| {
                    let keys = if is_current {
                        keys.to_owned().into()
                    } else {
                        SharedString::default()
                    };
                    hunk_discard_button(project_id, *at, keys, theme, cx)
                }))
```

  Update its doc comment ("... a small Stage hunk button on the right" gets "and a Discard hunk button after it on unstaged modified files").
- `git_diff_preview_row` (`:836`): `span_row(shown, index, None, None, "", None, project_id, theme, cx)`.
- `git_diff_lines` (`:600-611`): after `stage_keys` add

```rust
        let discard_keys = view
            .hunks_discardable()
            .then(|| self.keymap.label(Command::GitDiscard));
```

  and in the closure's `span` helper (`:625-636`) pass `discard_keys.as_deref(),` right after `&stage_keys,`.

`crates/mkrew-ui/src/keymap.rs:139`: `/// Git only: open the discard confirmation of the current hunk, else of the selected file.`
`crates/mkrew-ui/src/palette.rs:639`: `Command::GitDiscard => "Git: Discard changes to selected file or hunk…".into(),`

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui && CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS and no clippy warning (the `too_many_arguments` allow on `span_row` is already there). If `.keymap.label` returns `SharedString` rather than `String`, `.as_deref()` still works for `Option<SharedString>`; otherwise convert with `.to_string()` and keep `Option<String>`.

- [ ] **Step 5: Commit**

```bash
cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow
cargo fmt --all
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): discard one hunk with Discard hunk and x"
```

---

### Task 6: Send selection from a Markdown preview and from Docs

**Files:**
- Modify: `crates/mkrew-ui/src/preview_select.rs:9`, `:181-220` (+ tests module `:293-424`)
- Modify: `crates/mkrew-ui/src/files_view.rs:1240-1244` (add methods after `preview_copy`)
- Test: `crates/mkrew-ui/src/files_view/tests.rs` (after `:1002`)
- Modify: `crates/mkrew-ui/src/docs.rs:342` (add method), tests module end (`:718`)
- Modify: `crates/mkrew-ui/src/shell/composer.rs:348-385`
- Modify: `crates/mkrew-ui/src/shell/send_dialog.rs:170-185`
- Modify: `crates/mkrew-ui/src/palette.rs:735-737`, `crates/mkrew-ui/src/palette/tests.rs:299-303`

**Interfaces:**
- Consumes: `preview_select::{Selection, flat, Point}` (preview_select.rs:15-92), `FileTab::{previewing, splitting, preview, stamp, preview_selection}` (files_view.rs:1138-1174), `docs::Shown { lines, blocks }`, `Docs { selected, doc, comments }` (docs.rs:130), `comments::Selection { start, end }`.
- Produces: `preview_select::source_lines(&Shown, &Selection) -> Option<Lines>`, `FileTab::picked_lines(&self) -> Option<(String, u32, u32, String)>`, `Docs::picked_lines(&self) -> Option<(String, u32, u32, String)>` (path, first, last, text; the shape `Shell::picked_lines` already returns).

- [ ] **Step 1: Write the failing tests**

In `crates/mkrew-ui/src/preview_select.rs` tests module (before its closing `}` at `:424`):

```rust
    const PAGE: &str = "# Title\n\nfirst para\n\nsecond para\n\n```mermaid\ngraph TD; A-->B\n```\n\nlast\n";

    #[test]
    fn source_lines_are_the_union_of_the_blocks_touched() {
        let page = shown(PAGE);
        let lines = |anchor: Point, head: Point| {
            source_lines(&page, &Selection { anchor, head }).map(|l| (l.start, l.end))
        };
        // Inside one block: its whole range.
        assert_eq!(lines(at(1, 0, 2), at(1, 0, 5)), Some((3, 3)));
        // Across two blocks.
        assert_eq!(lines(at(1, 0, 2), at(2, 0, 3)), Some((3, 5)));
        // A backwards drag is the same.
        assert_eq!(lines(at(2, 0, 3), at(1, 0, 2)), Some((3, 5)));
        // Ending on the first byte of a block selects nothing of it.
        assert_eq!(lines(at(1, 0, 2), at(2, 0, 0)), Some((3, 3)));
        // A diagram shows no text: it neither counts nor splits the selection.
        assert_eq!(lines(at(2, 0, 0), at(4, 0, 4)), Some((5, 11)));
        // Nothing selected.
        assert_eq!(lines(at(1, 0, 2), at(1, 0, 2)), None);
        assert_eq!(
            source_lines(&page, &select_all(&page).unwrap()).map(|l| (l.start, l.end)),
            Some((1, 11))
        );
    }
```

In `crates/mkrew-ui/src/files_view/tests.rs`, after `a_drag_in_the_preview_copies_the_rendered_text` (ends `:1002`):

```rust
#[test]
fn send_selection_from_the_preview_is_the_source_of_the_blocks_touched() {
    let mut view = previewed("# Title\n\nold para\n\nend\n");
    let tab = tab_mut(&mut view);
    // Nothing selected on the page: the editor's caret line, as before.
    assert_eq!(
        tab.picked_lines(),
        Some(("README.md".into(), 1, 1, "# Title".into()))
    );
    tab.preview_click(point(1, 0), false, 1);
    tab.preview_drag(point(2, 3));
    assert_eq!(
        tab.picked_lines(),
        Some(("README.md".into(), 3, 5, "old para\n\nend".into())),
        "the Markdown source of both blocks, blank line included"
    );
    // One block: its whole range, whatever part is selected.
    tab.preview_click(point(1, 1), false, 1);
    tab.preview_drag(point(1, 3));
    assert_eq!(
        tab.picked_lines(),
        Some(("README.md".into(), 3, 3, "old para".into()))
    );
}

#[test]
fn in_split_the_editors_selection_wins_over_the_page() {
    let mut view = previewed("# Title\n\nold para\n\nend\n");
    let tab = tab_mut(&mut view);
    tab.set_mode(MdMode::Split);
    tab.preview_click(point(1, 0), false, 1);
    tab.preview_drag(point(1, 3));
    let picked = tab.picked_lines().unwrap();
    assert_eq!((picked.1, picked.2), (3, 3), "the page selects, the editor does not");
    if let Content::Shown(shown) = &mut tab.content {
        shown.editor.select_all();
    }
    let picked = tab.picked_lines().unwrap();
    assert_eq!(picked.1, 1, "the editor's selection starts at the first line");
}
```

In `crates/mkrew-ui/src/docs.rs` tests module, before its closing `}` (`:719`):

```rust
    #[test]
    fn a_comment_pick_is_the_selection_to_send() {
        let mut docs = Docs::new(Ulid::new());
        docs.selected = Some("a.md".into());
        docs.doc = Doc::Shown(Rc::new(shown("a.md", "one\n\ntwo\nthree\n", "h")));
        assert_eq!(docs.picked_lines(), None, "nothing picked");
        docs.comments.pick(3, 4, false);
        assert_eq!(
            docs.picked_lines(),
            Some(("a.md".into(), 3, 4, "two\nthree".into()))
        );
        // The file was read again, shorter: the pick lies past its end.
        docs.doc = Doc::Shown(Rc::new(shown("a.md", "one\n", "h2")));
        assert_eq!(docs.picked_lines(), None);
        docs.doc = Doc::Loading;
        assert_eq!(docs.picked_lines(), None, "no file shown");
    }
```

In `crates/mkrew-ui/src/palette/tests.rs`, after `assert!(git.contains(&Command::SendSelection));` (`:303`):

```rust
    let docs = commands(&context(View::Docs));
    assert!(docs.contains(&Command::SendSelection));
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui --lib`
Expected: FAIL to compile, `cannot find function source_lines`, `no method named picked_lines`.

- [ ] **Step 3: Write the implementation**

`crates/mkrew-ui/src/preview_select.rs:9`: `use crate::markdown::{Block, BlockKind, Item, Lines};`

`copy_text` (`:195-219`) loses its loop and uses a shared helper; replace those lines by

```rust
    let parts: Vec<String> = touched(shown, selection)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    parts.join("\n")
}

/// The blocks that have selected text, in page order, each with its index and that text.
fn touched(shown: &Shown, selection: &Selection) -> Vec<(usize, String)> {
    let (start, end) = selection.ordered();
    let mut parts = Vec::new();
    for index in start.block..=end.block {
        let Some(block) = shown.blocks.get(index) else {
            break;
        };
        let flat = flat(block, shown);
        if flat.pieces.is_empty() {
            continue;
        }
        let from = if index == start.block {
            flat.offset(start.piece, start.byte)
        } else {
            0
        };
        let to = if index == end.block {
            flat.offset(end.piece, end.byte)
        } else {
            flat.text.len()
        };
        if let Some(text) = flat.text.get(from..to).filter(|text| !text.is_empty()) {
            parts.push((index, text.to_owned()));
        }
    }
    parts
}

/// The source lines of the blocks `selection` touches (a block with no selected text, such as
/// a diagram, is not touched): from the first block's first line to the last block's last
/// line. "Send selection" sends the Markdown of these lines (ADR 0103). `None` when nothing
/// is selected.
pub fn source_lines(shown: &Shown, selection: &Selection) -> Option<Lines> {
    if selection.is_empty() {
        return None;
    }
    let mut blocks = touched(shown, selection)
        .into_iter()
        .filter_map(|(index, _)| shown.blocks.get(index))
        .map(|block| block.lines);
    let first = blocks.next()?;
    let last = blocks.last().unwrap_or(first);
    Some(Lines {
        start: first.start.min(last.start),
        end: first.end.max(last.end),
    })
```

(the original function's closing `}` after the old `parts.join("\n")` closes `source_lines` here; keep the file balanced: `copy_text`, `touched`, `source_lines` each end with one `}`.)

`crates/mkrew-ui/src/files_view.rs`, after `preview_copy` (`:1244`):

```rust
    /// What "Send selection" sends from this tab (§20B.4): the file, first and last line and
    /// their text. In Preview the page's selection (the source lines of the blocks it touches,
    /// as Markdown); in Split the editor's own selection first, then the page's; else the
    /// editor's selected lines, else its caret's line. The editor's caret always answers, so
    /// the page is asked before it.
    pub fn picked_lines(&self) -> Option<(String, u32, u32, String)> {
        let editor = &self.shown()?.editor;
        let from_page = self.previewing() || (self.splitting() && editor.selection().is_none());
        if let Some((first, last, text)) = self.preview_lines().filter(|_| from_page) {
            return Some((self.path.clone(), first, last, text));
        }
        // The selected lines, else the caret's line.
        let (first, last) = match editor.selection() {
            // A selection ending at a line's start does not include that line.
            Some(range) if range.end.col == 0 => (range.start.line, range.end.line - 1),
            Some(range) => (range.start.line, range.end.line),
            None => (editor.cursor().line, editor.cursor().line),
        };
        let text = editor.lines().get(first..=last)?.join("\n");
        let line = |index: usize| u32::try_from(index + 1).ok();
        Some((self.path.clone(), line(first)?, line(last)?, text))
    }

    /// The page's selection as source lines and their text; `None` without a selection, outside
    /// Preview and Split, or while the page is of an older text (its lines may have moved).
    fn preview_lines(&self) -> Option<(u32, u32, String)> {
        if !(self.previewing() || self.splitting()) {
            return None;
        }
        let current = self
            .page
            .as_ref()
            .is_some_and(|page| Some(page.stamp) == self.stamp());
        let page = self.preview().filter(|_| current)?;
        let lines = preview_select::source_lines(page, self.preview_selection.as_ref()?)?;
        let from = usize::try_from(lines.start).ok()?.checked_sub(1)?;
        let to = usize::try_from(lines.end).ok()?;
        Some((lines.start, lines.end, page.lines.get(from..to)?.join("\n")))
    }
```

`crates/mkrew-ui/src/docs.rs`, inside `impl Docs` after `step_selection` (`:341`):

```rust
    /// What "Send selection" sends from Docs (§20B.4): the selected file, the first and last
    /// line of the comment selection (a block or a line range) and the file's source text of
    /// those lines. `None` without a file shown, without a pick, or when the pick lies past
    /// the end of the file (it was read again, shorter).
    pub fn picked_lines(&self) -> Option<(String, u32, u32, String)> {
        let (Doc::Shown(shown), Some(path), Some(pick)) =
            (&self.doc, &self.selected, self.comments.selection)
        else {
            return None;
        };
        let from = usize::try_from(pick.start).ok()?.checked_sub(1)?;
        let to = usize::try_from(pick.end).ok()?;
        let text = shown.lines.get(from..to)?.join("\n");
        Some((path.clone(), pick.start, pick.end, text))
    }
```

`crates/mkrew-ui/src/shell/composer.rs:348-385` (`picked_lines`): doc becomes "The lines picked in `project_id`'s Editor view (a Markdown preview's selection, or the editor's lines, §12A), Docs view (its comment selection) or Git diff (new side): the file, first and last line, and their text (§20B.4)." and the body:

```rust
        match view {
            View::Editor => self.files.get(&project_id)?.tab()?.picked_lines(),
            View::Diff => {
                let git = self.git.get(&project_id)?;
                let (Some(target), Some(selection), git_view::Diff::Shown(shown)) =
                    (&git.selected, git.comments.selection, &git.diff)
                else {
                    return None;
                };
                let text = shown.new_side_text(selection)?;
                Some((target.path.clone(), selection.start, selection.end, text))
            }
            View::Docs => self.docs.get(&project_id)?.picked_lines(),
            _ => None,
        }
```

`crates/mkrew-ui/src/shell/send_dialog.rs:170-172` doc: "\"Send selection\" (Ctrl+Shift+L, §20B.4): the picked lines of Files (a Markdown preview's selection included), Docs or the Git diff, or the focused terminal's selected text, ..." and `:183`: `view @ (View::Editor | View::Diff | View::Docs) => {`.

`crates/mkrew-ui/src/palette.rs:735-737`:

```rust
        Command::SendSelection => {
            in_view(View::Editor)
                || in_view(View::Diff)
                || in_view(View::Docs)
                || (on_panes && context.terminal)
        }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 cargo test -p mkrew-ui && CARGO_BUILD_JOBS=4 cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS, including the unchanged `preview_select` copy tests (the `touched` refactor keeps `copy_text`'s output) and the existing `files_view` preview tests.

- [ ] **Step 5: Commit**

```bash
cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow
cargo fmt --all
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): send selection from a Markdown preview and from Docs"
```

---

### Task 7: Docs (ADR 0103, SPEC, HUMAN_CHECKS) and full verification

**Files:**
- Create: `docs/decisions/0103-review-flow.md`
- Modify: `docs/SPEC.md:621`, `:697`, `:1321`, `:1395-1397`, `:1524`, `:1530`, `:1533`, `:1539`, `:2036`, `:2281-2286`
- Modify: `docs/HUMAN_CHECKS.md` (new entry after line 23)

**Interfaces:**
- Consumes: everything above (docs only).
- Produces: the ADR, the SPEC text and the manual checks the final reviewer reads.

- [ ] **Step 1: Write the ADR**

Create `docs/decisions/0103-review-flow.md` (one sentence per line, no em dash):

```markdown
# 0103 - Review flow: viewed then next, hunks across files, discard a hunk, send from a preview

- Date: 2026-11-22
- Status: accepted (amends SPEC sections 12A, 12B, 13 and the protocol table)
- Milestone: none (Tern-inspired update, sub-project 4; spec `docs/superpowers/specs/2026-11-22-review-flow-design.md`, plan `docs/superpowers/plans/2026-11-22-review-flow.md`)

## Context

Mkrew already had split and unified diffs, "Viewed N/M" with collapsed viewed files (ADR 0065), line comments sent to agents, per-hunk stage and unstage, Markdown Preview, Split and Source (ADR 0083, 0092) and "Send selection" from the editor, the diff and the terminal.
Reviewing an agent's work still took extra steps: marking a file viewed left the selection on it, `n` / `p` stopped at the ends of a file, a bad hunk could only be discarded with its whole file, and a selection in a Markdown preview or in Docs could not be sent.

## Decision

- `v` on a file that is not viewed marks it and selects the next file below it that is not viewed, wrapping to the top; with every file viewed the selection stays; `v` on a viewed file only clears the mark.
  It moves only when the toggled file is the selected one (a row's box never moves the selection).
- `n` past the last hunk selects the next file that is not viewed and its first hunk, `p` before the first hunk the previous such file and its last hunk; no wrap at the ends.
  The target hunk is pending until that file's diff is shown; a file without hunks keeps it, so the next key moves on.
  A viewed file's folded diff keeps ignoring `n` / `p`.
  In the History the keys never leave the commit's files.
- `git.discard` takes `hunk` (a `HunkTarget`) with `paths` empty; the daemon builds the same patch as for a hunk stage and runs `git apply -R` on the working tree.
  Protocol 56 (55 is the Widget bar's).
  The daemon refuses an untracked path with `invalid_request`; the UI offers the button only on a modified file of the Changes group, and `x` takes the current hunk, else the file.
  The confirm dialog lists that hunk's rows and stays disabled if the reloaded diff no longer has that hunk.
- "Send selection" (Ctrl+Shift+L) sends, from a Markdown preview, the Markdown source of the blocks the selection touches with `path:a-b`, and from Docs its comment selection.
  In Preview the page's selection is asked first; in Split the editor's own selection first, then the page's; the editor's caret line stays the fallback.
- `c` stays "focus the commit box" and `Ctrl+Alt+M` stays "add comment".

## Alternatives

- Calling `step_file` from `step_hunk`: it cannot skip viewed files and steps to another commit in the History.
- Sending the rendered text of a preview selection: the agent should see the Markdown, with a line range it can open.
- Discarding by editing the file in the UI: loses the index-safe `git apply -R` and its stale-hunk check.
- Comments and viewed marks in the History: they need anchors on a commit, not on the working file, so they stay out of this change.

## Consequences

- A stale hunk fails with git's own text in the existing error toast, as a stale hunk stage does.
- A hand-made request naming a staged-only hunk of a clean file would apply; only the UI keeps that from happening.
- An older daemon is refused by the protocol version check.
- `docs/HUMAN_CHECKS.md` lists the review pass for a person to check in dark and light.
```

- [ ] **Step 2: Update SPEC**

Make these exact edits in `docs/SPEC.md`, from the bottom of the list upward so the earlier line numbers stay valid (re-read each range first; line numbers are at HEAD):

- `:621` replace the row with:
  `` | `git.discard` | `paths[]` (empty with a hunk) or `hunk` (a `HunkTarget` of the unstaged diff), `confirm: true` | ok (destructive; UI always confirms first). A hunk is reversed in the working tree only (`git apply -R`, the index stays); `invalid_request` for `paths` together with a hunk or for an untracked file; a hunk that no longer applies fails with git's text | ``
- after `:697` (`Protocol 54 ...`) add a blank line and:
  `` Protocol 56 (ADR 0103; 55 is the Widget bar's): `git.discard` takes `hunk`. ``
- after `:1321` (end of the Markdown Split view bullet) add:
  ```
  - **Send selection from the preview** (ADR 0103): `Ctrl+Shift+L` with text selected on the page (Preview, or Split while the editor selects nothing) sends the Markdown source of the blocks the selection touches, with `path:a-b` (§20B.4).
    A selection inside one block sends that block's whole line range.
    With nothing selected on the page it sends the editor's selected lines or its caret's line, as before.
  ```
- replace `:1396-1397` (the Stage and Discard bullets) with:
  ```
  - **Stage / unstage** per file or per hunk (daemon builds the patch, runs `git apply --cached`).
  - **Review flow** (ADR 0103): `v` on the selected file that is not viewed marks it and selects the next file below it that is not viewed, wrapping to the top (every file viewed: the selection stays); `v` on a viewed file only clears the mark.
    `n` past the last hunk of a file selects the next file that is not viewed and its first hunk, `p` before the first hunk the previous such file and its last hunk; there is no wrap, a file without hunks is passed on the next key, and a viewed file's folded diff ignores both keys.
  - **Discard** per file or per hunk: always confirms and shows what will be lost.
    A hunk of a modified file in the Changes group has a "Discard hunk" button after "Stage hunk"; `x` with a current hunk opens the same confirmation for that hunk, else the file's.
    The daemon reverses the hunk in the working tree only (`git apply -R`); the dialog stays disabled if a reload changed the hunk.
  ```
- `:1524` → `| Git | Next / previous hunk (past the last / first one: the next / previous file that is not viewed) | n / p |`
- `:1530` → `| Git | Mark the selected file viewed / not viewed (marking opens the next file that is not viewed) | v |`
- `:1533` → `| Git | Discard the current hunk, else the selected file (asks first) / fetch / push | x / f / Shift+P |`
- `:1539` → `| Agent (v2) | Send selection to an agent (Editor, Markdown preview, Docs, Git diff, terminal) | Ctrl+Shift+L |`
- `:2036` → `- [ ] Send selection from the Editor (also a Markdown preview selection), Docs, Git diff and a terminal opens the right session's composer, prefilled.`
- `:2281-2282`: replace `` `Ctrl+Shift+L` in the Editor (the editor's selected lines, else the caret's line), Git diff (picked new-side lines) and a terminal (selected text) `` with `` `Ctrl+Shift+L` in the Editor (a Markdown preview's selection as its blocks' source lines; else the editor's selected lines, else the caret's line), Docs (the comment selection), Git diff (picked new-side lines) and a terminal (selected text) ``.

- [ ] **Step 3: Add the manual checks**

In `docs/HUMAN_CHECKS.md`, insert after line 23 (the blank line after the "To verify (non-blocking)" intro, before `### To verify - 2026-11-21 Pressable audit (P3)`):

```markdown
### To verify - 2026-11-22 Review flow (ADR 0103, plan docs/superpowers/plans/2026-11-22-review-flow.md)

- [ ] **Restart the daemon first** (protocol 56; a window from before it shows a version mismatch).
- [ ] **Viewed, then next (dark + light):** Git tab with 3+ changed files and a diff open.
  `v` on the first file marks it (its row dims) and the second file's diff opens with the list scrolled to it.
  `v` on a viewed file only clears the mark and moves nothing.
  Mark every file: the selection stays on the last one.
  Tick the box of a row that is not selected: that row is marked, the selection stays.
- [ ] **Hunk stepping:** in a file with 2+ hunks press `n` past the last hunk: the next file that is not viewed opens, its first hunk shows the `▸` marker and is scrolled to.
  `p` before the first hunk goes back to the previous file's last hunk.
  Viewed files and a binary file with no hunks are passed; at the first and last file nothing moves.
- [ ] **Discard hunk (dark + light):** an unstaged modified file with two hunks: each hunk header shows `Stage hunk` and a red `Discard hunk`.
  No such button on Staged, Untracked, deleted or renamed files, in Against base or in History.
  Click one: "Discard this hunk?" lists only that hunk's lines; Cancel keeps it; confirm removes that hunk, the other stays and the diff reloads.
  With `n` choosing a hunk, `x` opens the same dialog; with no hunk chosen `x` still opens the file's dialog.
  Change the file in an editor while the dialog is open: the dialog says the hunk changed and confirm stays off.
- [ ] **CRLF file:** discard a hunk of a CRLF file (Git for Windows with `core.autocrlf` true, and with false): the other lines keep their line endings.
- [ ] **Send from a preview:** Files, a `.md` in Preview: select text across two paragraphs, press `Ctrl+Shift+L`.
  The send dialog names the file as `README.md:3-5` and the composer gets the Markdown source of both blocks.
  Same in Split with the page selected; with the editor selecting too, the editor's lines go.
  With nothing selected the caret's line goes, as before.
- [ ] **Send from Docs:** Docs, click a block (Shift+click extends), press `Ctrl+Shift+L`: the same dialog with the file and line range; in Source mode too.
```

- [ ] **Step 4: Full verification**

Run: `cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow && CARGO_BUILD_JOBS=4 "C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: `verify: OK` (fmt, clippy `-D warnings`, `cargo test --workspace`).
If it runs past 600 s, run `CARGO_BUILD_JOBS=4 cargo fmt --all -- --check`, `CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings` and `CARGO_BUILD_JOBS=4 INSTA_UPDATE=no cargo test --workspace` separately; all three must pass.
Also check the docs rules: `git grep -n $'\xe2\x80\x94' -- docs/decisions/0103-review-flow.md docs/HUMAN_CHECKS.md` finds no em dash in the lines this plan added (HUMAN_CHECKS has older ones: look only at the new entry).

- [ ] **Step 5: Commit**

```bash
cd C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/.worktrees/review-flow
git add docs/decisions/0103-review-flow.md docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: review flow (ADR 0103, SPEC, HUMAN_CHECKS)"
```

---

## Self-Review

**1. Spec coverage**

| Spec section | Task |
|---|---|
| 2 Decisions: viewed then next | 3 |
| 2 Decisions: hunk stepping | 4 |
| 2 Decisions: hunk discard (button, `x`, dialog) | 5 |
| 2 Decisions: send from preview, send from Docs | 6 |
| 2 `c` key and History comments unchanged | no code (Global Constraints; nothing touches `c`, Ctrl+Alt+M or History comments) |
| 3 Viewed then next (`next_unviewed`, same `Requests`, optimistic marks) | 3 |
| 4 Hunk stepping (pending target, scroll, cleared on selection change, no-hunk files) | 4 |
| 5.1 Protocol | 1 |
| 5.2 Daemon (`git apply -R`, stale text, only modified unstaged) | 2 (and Spec gap 5) |
| 5.3 UI (button, `x`, dialog lines, "Discard this hunk?", reload) | 5 (reload through the existing `refresh_git` in `confirm_git_discard`, unchanged) |
| 6 Send selection (`picked_lines` two sources, Markdown source text, fallback unchanged) | 6 |
| 7 Error handling table | stale hunk: 2 and 5; `paths` + `hunk`: 2; older daemon: 1 (version check); preview selection in one block: 6; all viewed: 3 |
| 8 Testing: git_view tests | 3, 4, 5 |
| 8 Testing: protocol round trip and default | 1 |
| 8 Testing: daemon integration on a real temp repo | 2 |
| 8 Testing: `picked_lines` from a preview and Docs | 6 |
| 8 Testing: HUMAN_CHECKS | 7 |
| 9 Docs: ADR 0103, SPEC 12A / 12B / 13 / protocol table | 7 |
| 10 Out of scope | respected (no History, no `c`, no multi-file page, no staged / untracked hunk discard) |

No gaps. Spec gaps (1-11) are listed above the tasks.

**2. Placeholder scan:** no "TBD", "TODO", "similar to Task N" or untyped steps; every code step shows code, every run step shows the command and the expected result.
The only optional text is in Task 4's last test (the second half of `a_click_clears_the_pending_target` MAY be dropped); the first half carries the spec's requirement.

**3. Type consistency:** `GitDiscardParams.hunk` (Task 1) is used by `git_discard` (Task 2) and `Discard::params` (Task 5); `GitBackend::discard_hunk(dir, repo, patch)` is defined and called with that shape; `Requests` is returned by `next_unviewed` and `step_hunk`; `PendingHunk::{First, Last}`, `take_hunk_reveal` and `reveal_hunk` are used the same way in git_view.rs and shell/git.rs; `Discard.hunk: Option<(usize, Hunk)>` is used by `title`, `confirm_label`, `sentence`, `params`, `preview_rows`, `hunk_is_current` and the shell; `picked_lines` keeps the tuple `(String, u32, u32, String)` everywhere (`FileTab`, `Docs`, `Shell::picked_lines`, `open_selection_dialog`); test helpers `changes`, `mark_viewed`, `unstaged` (Task 3) and `show_hunks` (Task 4) are defined before they are used by later tasks.

**4. Review Focus:** each of the five items names its task and test: CRLF discard (Task 2 unit test), stale dialog (Task 5 `the_hunk_dialog_lists_only_its_rows_and_stops_when_the_hunk_changed`), text-less blocks and byte 0 (Task 6 `source_lines_are_the_union_of_the_blocks_touched`), Docs pick past the end (Task 6 `a_comment_pick_is_the_selection_to_send`), row box and all-viewed (Task 3 tests) with the folded-diff cases (Task 4 unchanged collapsed test, Task 5 `x_takes_the_current_hunk_else_the_file`).
