# Files change gutter, History lane graph, terminal layout presets — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Three features taken from Tern (stencil.so/tern):
1. **Change gutter** in the Files editor: a thin bar left of each line that was added or changed, and a short mark where lines were removed, compared with the file's text in the git index. The bars follow unsaved typing.
2. **Lane graph** in the Git view's History: a column left of each commit that draws its branch lanes, merges and forks.
3. **Layout presets** for terminal tabs: the split button `◫` opens a menu with Split right, Split down and five layouts (2 columns, 2 rows, 3 columns, 1 + 2, 2 × 2). Picking a layout rearranges the tab and starts the panes it lacks.

**Architecture:**
1. Gutter: `fs.read` gets an opt-in `git_base` flag. The daemon then also returns the file's text from the index (`git cat-file blob :./path`), taken from the child repository when the file lies in one. The UI diffs the editor text against that text with `similar` (already a dependency). This runs on the background executor, in the same job as the existing 150 ms re-highlight, and the result is stored in `highlight::Code`.
2. Graph: `git log` gets `--topo-order --parents`, so children are always listed before their parents and parents are rewritten to listed commits. A pure `git_graph::layout` turns `Commit.parents` (already on the wire) into one row per commit. The History rows draw it with a gpui `canvas` (`PathBuilder` lines plus a dot).
3. Layouts: a pure `split::Preset` builds a split tree, and `Tabs::arrange` lays out a tab with it. `Shell::apply_layout` starts the missing sessions one after another, then writes `tab.set_layout` once. The daemon is unchanged.

**Tech Stack:** Rust, gpui-pre 0.3.7 (`canvas`, `PathBuilder::stroke`, `Window::paint_path`, `paint_quad`), `similar` 3.2 (`capture_diff_slices_deadline`), git CLI.

**Spec:** `docs/SPEC.md` §7 (`fs.read` row), §10 (Terminals: tab strip, split button), §12A (Files view: Editor), §12B (Git view: History), §13 (shortcuts), §14 (design tokens). This plan writes the new spec text itself (the "SPEC" step in Tasks 1, 2, 4, 5 and 6). Read the listed sections before you start.

The three parts are independent: Tasks 1–2 (gutter), 3–4 (graph) and 5–6 (layouts). Each part works and ships on its own. Do them in order 1 → 6.

## Global Constraints

- Find code by **symbol name**, never by line number. `crates/mkrew-ui/src/main.rs` is ~20k lines, and other plans move code around. Line numbers below are hints taken on 2026-10-11.
- `PROTOCOL_VERSION` (`crates/mkrew-protocol/src/lib.rs`) is 37 today. Only Task 1 changes the wire format: bump it by exactly one from the value you read there. Other pending plans (`2026-10-10-inbox-perf.md`, `2026-10-10-remote-machine.md`, `2026-10-10-auto-update.md`) may have bumped it already.
- ADR: only Task 1 writes one. Run `ls docs/decisions` and take the next free number ≥ 0047 (0042–0046 are reserved by pending plans). It is called `NNNN` below; use the real number everywhere.
- No migrations. No new crates.
- No test may call a real agent or the network.
- UI: every color, spacing, size and radius comes from `crates/mkrew-ui/src/theme.rs`. No hex and no raw pixel literals outside `theme.rs`. Spacing comes from `space::*` (4 · 8 · 12 · 16 · 24 · 32), radius from `radius::*`. Both dark and light must work.
- GPUI: call `.hover()` at most once per element (`menu_item()` and `button()` already set it). An element with `track_focus` grabs focus on any mouse down.
- Shell: in the omp shell `bash` is WSL, so never use it. Run `cargo` and `git` directly and run scripts with `sh`. Build with `-j 4`. The C: drive is nearly full: no `cargo clean`, no extra target dirs. If an `.exe` is "Access is denied" (a running daemon holds it), rename it to `*.locked-old.exe`. Never kill the user's daemon or sessions.
- Daemon e2e tests use the prebuilt `target/debug/mk.exe`. After a protocol change, run `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` BEFORE `cargo test -p mkrew-daemon --test daemon -j 2`.
- Full verify: `sh scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these in order instead: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`.
- `cargo insta` is not installed. No task here adds snapshots.
- Commits: after each task, commit only that task's files (`git add <paths>`, never `-A` or `.`). Use Conventional Commits. Never push.
- Never touch the human's work in progress: `docs/INBOX.md`, `docs/PERF_REVIEW.md`, other files in `docs/superpowers/plans|specs/`, `.github/workflows/*`, `Cargo.lock`, `Cargo.toml`, `crates/mkrew-ui/Cargo.toml`, `docs/BUILD_VI.md`, `packaging/`, `.omp/agents/*`.
- HUMAN_CHECKS: Task 2 creates the section `### To verify — 2026-10-11 gutter, graph, layouts (plan docs/superpowers/plans/2026-10-11-gutter-graph-layouts.md)` directly above the first existing `### To verify —` heading in `docs/HUMAN_CHECKS.md`. Tasks 4 and 6 append their bullets to the end of that section.

## Review Focus

1. **Windows line endings:** with `core.autocrlf=true` the working file has CRLF and the index has LF. This must show no marks at all, not every line "changed". Test: `crlf_lines_match_their_lf_base` (Task 2).
2. **Child repositories (ADR 0029):** a file under `fe/` where `fe` is its own repository must use `fe`'s index, not the parent's (where it is untracked). Test: `fs_read_adds_the_index_text_when_asked` (Task 1).
3. **Staging or committing while the file is open:** the disk hash stays the same, but the index text changes. The gutter must update while caret and undo stay. With unsaved text, the new index text must be used at the next typing pause. Test: `a_new_index_text_moves_the_gutter_and_keeps_the_caret` (Task 2).
4. **History edges:** merge parents beyond the 50 listed commits, octopus merges, commit dates out of order (clock skew), and a project that is a subfolder of its repository. The graph must never panic. Lanes run off the bottom and stay capped. Children are always listed before parents. Parents name listed commits. Tests: `lanes_to_unlisted_parents_run_off_the_bottom`, `lanes_are_capped`, `log_lists_children_before_their_parents`, `log_of_a_subfolder_rewrites_parents_to_listed_commits` (Task 3).
5. **A layout applied while the tab changes:** the user switches tabs during the spawns, a spawn fails, the tab already has more panes than the layout, or the daemon has already placed the first session in an empty tab. No session may be lost or doubled. Test: `arrange_refuses_other_sizes_and_works_on_any_tab` (Task 5).

---

### Task 1: `fs.read` returns the index text (`git_base`) + ADR

Spec: §7 `fs.read` row, §12A, ADR 0029 (child repositories).

**Files:**
- Modify: `crates/mkrew-protocol/src/files.rs` (`FsReadParams`, `FsContent`, the method round-trip test literal `Request::FsRead(FsReadParams { .. })`, the `FsContent` round-trip literal and the "v26 daemon's answer" test)
- Modify: `crates/mkrew-protocol/src/lib.rs` (`PROTOCOL_VERSION` + 1)
- Modify: `crates/mkrew-daemon/src/git.rs` (trait `GitBackend`: new `index_text`; `impl GitBackend for GitCli`; `mod tests`)
- Modify: `crates/mkrew-daemon/src/undo.rs` (test fake `impl GitBackend for BigDiff`: add `index_text` → `unreachable!()`)
- Modify: `crates/mkrew-daemon/src/files.rs` (new `git_base`; `read` literal gets `git_base: None`)
- Modify: `crates/mkrew-daemon/src/server/fs.rs` (`fs_read`)
- Modify: `crates/mkrew-daemon/tests/daemon/fs_ai.rs` (existing `read` closure gets `git_base: false`; new test)
- Modify: `crates/mkrew-daemon/tests/daemon/worktrees.rs` (the `FsReadParams { .. }` literal gets `git_base: false`)
- Modify: `crates/mkrew-ui/src/shell/files.rs` (`read_file`: `git_base: true`), `crates/mkrew-ui/src/files_view.rs` (`highlight_request` literal: `git_base: None` for now; Task 2 changes it), `crates/mkrew-ui/src/files_view/tests.rs` (`file_text_at`), `crates/mkrew-ui/src/highlight.rs` (test literals in `fn read` and `binary_and_too_large_files_keep_their_flags_and_no_lines`)
- Create: `docs/decisions/NNNN-files-change-gutter.md`
- Modify: `docs/SPEC.md` (§7 `fs.read` row)

**Interfaces:**
- Produces: `FsReadParams.git_base: bool` (serde default `false`). `FsContent.git_base: Option<String>` (serde default `None`). `GitBackend::index_text(&self, dir: &Path, path: &str) -> io::Result<Option<String>>`. `mkrew_daemon::files::git_base(git: &dyn GitBackend, root: &Path, path: &str) -> Option<String>`.

- [ ] **Step 1: Write the failing backend test** (`crates/mkrew-daemon/src/git.rs`, in `mod tests`, after `stage_unstage_discard_and_commit_in_a_subfolder`)

```rust
    /// The index's text, not the working tree's or HEAD's, also when the project is a
    /// subfolder of the repo; untracked and unknown files have none; outside a repo git fails.
    #[test]
    fn index_text_reads_the_staged_version() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let dir = root.join("app");
        std::fs::create_dir(&dir).unwrap();
        run(root, &["init", "-q"]);
        run(root, &["config", "core.autocrlf", "false"]);
        std::fs::write(dir.join("a b.txt"), "one\n").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "one"]);
        std::fs::write(dir.join("a b.txt"), "two\n").unwrap();
        run(root, &["add", "."]);
        std::fs::write(dir.join("a b.txt"), "three\n").unwrap();
        std::fs::write(dir.join("new.txt"), "new\n").unwrap();

        let text = GitCli.index_text(&dir, "a b.txt").unwrap();
        assert_eq!(text.as_deref(), Some("two\n"));
        assert_eq!(GitCli.index_text(&dir, "new.txt").unwrap(), None);
        assert_eq!(GitCli.index_text(&dir, "gone.txt").unwrap(), None);
        let outside = tempfile::tempdir().unwrap();
        assert!(GitCli.index_text(outside.path(), "x").is_err());
    }
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p mkrew-daemon --lib index_text_reads_the_staged_version -j 4`
Expected: compile error `no method named index_text`.

- [ ] **Step 3: Add the trait method and the git implementation**

In `pub trait GitBackend`, after `fn is_tracked`:

```rust
    /// Text of `path` (relative to `dir`) in the index; `None` when it is not in the index
    /// (untracked, or only conflict stages), not UTF-8, or over the viewer's 2 MB.
    fn index_text(&self, dir: &Path, path: &str) -> io::Result<Option<String>>;
```

In `impl GitBackend for GitCli`, after `fn is_tracked`:

```rust
    fn index_text(&self, dir: &Path, path: &str) -> io::Result<Option<String>> {
        if !self.is_tracked(dir, path)? {
            return Ok(None);
        }
        // `:./` = stage 0 of the index, relative to `dir` (the project may be a subfolder).
        let mut command = view_git(dir);
        command.args(["cat-file", "blob", &format!(":./{path}")]);
        let bytes = match run(command) {
            Ok(bytes) => bytes,
            // Conflicted: stages 1–3 only, no stage 0.
            Err(_) => return Ok(None),
        };
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > crate::docs::MAX_BYTES {
            return Ok(None);
        }
        Ok(String::from_utf8(bytes).ok())
    }
```

Check the real names before you paste. Find how `is_tracked` and `show_text` build and run their git commands (`view_git`, `run` / `run_output`, or whatever this file uses) and use those same helpers. Also find where `MAX_BYTES` lives (`git grep -n "MAX_BYTES" -- crates/mkrew-daemon/src`) and fix the path.

In `crates/mkrew-daemon/src/undo.rs`, `impl GitBackend for BigDiff`, after `fn is_tracked`:

```rust
        fn index_text(&self, _: &Path, _: &str) -> io::Result<Option<String>> {
            unreachable!()
        }
```

Check that no other fake exists: `git grep -n "impl GitBackend for" -- crates`. Add the same three lines to any other fake you find.

- [ ] **Step 4: Run the backend test**

Run: `cargo test -p mkrew-daemon --lib index_text_reads_the_staged_version -j 4`
Expected: PASS.

- [ ] **Step 5: Protocol fields**

`crates/mkrew-protocol/src/files.rs`, `FsReadParams` gets this after `path`:

```rust
    /// Also return the file's text in the git index ([`FsContent::git_base`]): the Files
    /// editor's change gutter (SPEC §12A, ADR NNNN).
    #[serde(default)]
    pub git_base: bool,
```

`FsContent` gets this after `hash`:

```rust
    /// Asked for with `git_base`: the file's text in the index of the repository holding it
    /// (a child repository's for a file inside one, ADR 0029). `None` = not asked, untracked,
    /// not in a repository, not UTF-8 or over 2 MB (ADR NNNN).
    #[serde(default)]
    pub git_base: Option<String>,
```

In the same file's tests:
- In the method round-trip test, the `Request::FsRead(FsReadParams { .. })` literal gets `git_base: true,`.
- The `FsContent` literal next to `CommitDetail` gets `git_base: None,`.
- After `assert_eq!(old.hash, None);` in the "v26 daemon's answer" test, add `assert_eq!(old.git_base, None);`.

`crates/mkrew-protocol/src/lib.rs`: `PROTOCOL_VERSION` = the value you read + 1.

Compile the workspace to find every other literal: `cargo check --workspace --all-targets -j 4`. Fix each "missing field" error this way:
- `crates/mkrew-daemon/src/files.rs` `read`: `git_base: None,`
- `crates/mkrew-daemon/tests/daemon/fs_ai.rs` `read` closure: `git_base: false,`
- `crates/mkrew-daemon/tests/daemon/worktrees.rs`: `git_base: false,`
- `crates/mkrew-ui/src/shell/files.rs` `read_file`: `git_base: true,`
- `crates/mkrew-ui/src/files_view.rs` `highlight_request`: `git_base: None,`
- `crates/mkrew-ui/src/files_view/tests.rs` `file_text_at`: `git_base: None,`
- `crates/mkrew-ui/src/highlight.rs` test literals (two): `git_base: None,`

- [ ] **Step 6: Write the failing daemon test** (`crates/mkrew-daemon/tests/daemon/fs_ai.rs`, after `fs_list_read_and_find_follow_file_changes`)

```rust
/// `fs.read` with `git_base` adds the file's text in the index (SPEC §12A change gutter): of
/// the project's repository, or of the child repository holding the file (ADR 0029); none for
/// an untracked file or when not asked.
#[test]
fn fs_read_adds_the_index_text_when_asked() {
    let env = Env::new();
    let repo = env.repo();
    let init = |dir: &std::path::Path| {
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "core.autocrlf", "false"]);
        git(dir, &["config", "user.name", "t"]);
        git(dir, &["config", "user.email", "t@t"]);
        git(dir, &["config", "commit.gpgsign", "false"]);
    };
    init(&repo);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    std::fs::write(repo.join("new.txt"), "new\n").unwrap();
    let child = repo.join("fe");
    std::fs::create_dir(&child).unwrap();
    init(&child);
    std::fs::write(child.join("b.txt"), "child\n").unwrap();
    git(&child, &["add", "."]);
    std::fs::write(child.join("b.txt"), "edited\n").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let read = |client: &mut Client, path: &str, git_base: bool| -> FsContent {
        let request = Request::FsRead(FsReadParams {
            project_id: id,
            worktree_task_id: None,
            path: path.into(),
            git_base,
        });
        client.call(request).into_result().unwrap()
    };

    let a = read(&mut client, "a.txt", true);
    assert_eq!(a.content.as_deref(), Some("two\n"));
    assert_eq!(a.git_base.as_deref(), Some("one\n"));
    assert_eq!(read(&mut client, "a.txt", false).git_base, None, "not asked");
    assert_eq!(read(&mut client, "new.txt", true).git_base, None, "untracked");
    let b = read(&mut client, "fe/b.txt", true);
    assert_eq!(b.git_base.as_deref(), Some("child\n"), "the child repo's index");
}
```

Copy the imports (`Client`, `FsContent`, …) from the top of `fs_ai.rs`, matching how `fs_list_read_and_find_follow_file_changes` names them.

- [ ] **Step 7: Run it to see it fail**

Run: `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` then `cargo test -p mkrew-daemon --test daemon fs_read_adds_the_index_text_when_asked -j 2`
Expected: FAIL at `a.git_base` (`None` vs `Some("one\n")`).

- [ ] **Step 8: Implement `files::git_base` and use it in `fs_read`**

`crates/mkrew-daemon/src/files.rs`, after `child_repos` (add `use crate::git::GitBackend;` to the imports):

```rust
/// The index text of the project-relative `path` under `root` (`fs.read` with `git_base`):
/// from the child repository (ADR 0029) the path lies in, else from the project's own.
/// `None` when untracked, outside a repository, or git fails.
pub fn git_base(git: &dyn GitBackend, root: &Path, path: &str) -> Option<String> {
    let (dir, inner) = match path.split_once('/') {
        Some((top, rest)) if child_repos(root).iter().any(|repo| repo == top) => {
            (root.join(top), rest)
        }
        _ => (root.to_path_buf(), path),
    };
    git.index_text(&dir, inner).ok().flatten()
}
```

`crates/mkrew-daemon/src/server/fs.rs`, `fs_read`: replace the line
`let read = tokio::task::spawn_blocking(move || files::read(&file, path)).await;` with

```rust
        let (git, root, want_base) = (Arc::clone(&self.git), target.dir.clone(), params.git_base);
        let read = tokio::task::spawn_blocking(move || {
            files::read(&file, path).map(|mut content| {
                if want_base && content.content.is_some() {
                    content.git_base = files::git_base(&*git, &root, &content.path);
                }
                content
            })
        })
        .await;
```

Check the field names first. Look at how `fs_list` reaches the git backend and the target folder (`self.git`, `target.dir`, …) and use the same names. If `fs_read` moves `target` before this line, clone the dir earlier.

- [ ] **Step 9: Run the daemon tests**

Run: `cargo test -p mkrew-daemon --test daemon fs_ -j 2` and `cargo test -p mkrew-protocol -j 4`
Expected: PASS.

- [ ] **Step 10: ADR**

Create `docs/decisions/NNNN-files-change-gutter.md`:

```markdown
# NNNN — The Files editor's change gutter compares with the git index

- Date: 2026-10-11
- Status: accepted

## Context

The Files editor (SPEC §12A, ADR 0028) showed no git changes. A gutter (as in Zed, VS Code,
Tern) marks added, changed and removed lines. It must follow unsaved typing, so the existing
`git.diff` (a diff of the file on disk) is not enough: the UI needs the text to compare with.

## Decision

- `fs.read` gets an opt-in `git_base: bool`; when set, `FsContent.git_base` carries the file's
  text in the **index** (`git cat-file blob :./<path>` after `git ls-files`), from the child
  repository holding the file (ADR 0029). Untracked, conflicted, not UTF-8, over 2 MB or outside
  a repository: `None`, and no gutter.
- The UI diffs the editor text with it (`similar`, Myers, 100 ms deadline) on the background
  executor, in the job that re-highlights 150 ms after typing stops; the marks live in
  `highlight::Code` and are drawn only on lines whose text matches that highlight.
- The index, not HEAD: the gutter shows what "Working changes" (unstaged) shows in the Git
  view, so staging a hunk clears its marks.

## Alternatives rejected

- `git.diff` per save: no marks for unsaved text; lines shift while typing.
- A separate `git.base` request: a second round trip and handler for every open.
- HEAD as the base: staged lines stay marked while the Git view calls them staged.

## Consequences

Protocol bump (two `#[serde(default)]` fields). Each `fs.read` of the shown file runs
`git ls-files` + `git cat-file` (Files view only; git still runs only for a view on screen).
Untracked files show the tree's `?` and no bars.
```

- [ ] **Step 11: SPEC §7**

In the §7 table, replace the `fs.read` row with:

```markdown
| `fs.read` | `project_id`, `path`, `git_base?` (bool) | content (≤ 2 MB), binary flag, language, `hash?` (opaque, set with `content`), `git_base?` (asked for: the file's text in the git index, ADR NNNN) |
```

- [ ] **Step 12: Verify and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p mkrew-daemon --lib -j 4`.

```bash
git add crates/mkrew-protocol/src/files.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-daemon/src/git.rs crates/mkrew-daemon/src/undo.rs crates/mkrew-daemon/src/files.rs crates/mkrew-daemon/src/server/fs.rs crates/mkrew-daemon/tests/daemon/fs_ai.rs crates/mkrew-daemon/tests/daemon/worktrees.rs crates/mkrew-ui/src/shell/files.rs crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view/tests.rs crates/mkrew-ui/src/highlight.rs docs/decisions/NNNN-files-change-gutter.md docs/SPEC.md
git commit -m "feat(daemon): fs.read returns the file's index text for the change gutter (ADR NNNN)"
```

---

### Task 2: The change gutter in the Files editor

Spec: §12A Editor, §14 tokens.

**Files:**
- Create: `crates/mkrew-ui/src/gutter.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod gutter;` alphabetically, after `pub mod git_view;`)
- Modify: `crates/mkrew-ui/src/highlight.rs` (`Code.changes`, `Code::new`, `Code::plain`)
- Modify: `crates/mkrew-ui/src/files_view.rs` (`Shown.base`, `show`, `highlight_request`)
- Modify: `crates/mkrew-ui/src/files_view/tests.rs`
- Modify: `crates/mkrew-ui/src/preview.rs` (`RowMarks.change`, `code_row`, new `change_bar`)
- Modify: `crates/mkrew-ui/src/shell/files_render.rs` (`render_editor`: fill `RowMarks.change`)
- Modify: `crates/mkrew-ui/src/theme.rs` (constants + contrast test)
- Modify: `docs/SPEC.md` (§12A), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `FsContent.git_base: Option<String>` (Task 1).
- Produces: `gutter::Change { Added, Modified, Deleted }` (`Copy, Eq, Debug`). `gutter::changes(base: &str, text: &str) -> Vec<Option<Change>>`. `Code.changes: Vec<Option<Change>>`. `files_view::Shown.base: Option<String>`. `preview::RowMarks.change: Option<Change>`.

- [ ] **Step 1: Write the failing tests** (create `crates/mkrew-ui/src/gutter.rs` with this test module only)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use Change::*;

    #[test]
    fn unchanged_text_has_no_marks() {
        assert_eq!(changes("a\nb\n", "a\nb\n"), [None, None]);
    }

    #[test]
    fn added_changed_and_removed_lines_are_marked() {
        // b → B changed, d removed (marked on e, the line after it), f added.
        let marks = changes("a\nb\nc\nd\ne\n", "a\nB\nc\ne\nf\n");
        assert_eq!(marks, [None, Some(Modified), None, Some(Deleted), Some(Added)]);
    }

    #[test]
    fn lines_removed_at_the_end_mark_the_last_line() {
        assert_eq!(changes("a\nb\n", "a\n"), [Some(Deleted)]);
        assert_eq!(changes("a\n", ""), Vec::<Option<Change>>::new());
    }

    #[test]
    fn an_empty_tracked_file_that_got_text_is_all_added() {
        assert_eq!(changes("", "x\ny\n"), [Some(Added), Some(Added)]);
    }

    /// Windows: `core.autocrlf` keeps LF in the index and CRLF in the working file.
    #[test]
    fn crlf_lines_match_their_lf_base() {
        assert_eq!(changes("a\nb\n", "a\r\nb\r\n"), [None, None]);
    }
}
```

Note: `Code::plain` gives a trailing empty line after a final `\n`? Check how `Code::plain` / `Editor::lines` split (`str::lines` drops the last empty line). `changes` must return exactly one mark per row that `render_editor` draws. If the editor shows one more row than `str::lines` (an empty last row after a final newline), the extra row simply has no mark: `code.changes.get(index)` returns `None`. That is correct.

- [ ] **Step 2: Run them to see them fail**

Add `pub mod gutter;` to `crates/mkrew-ui/src/lib.rs`. Run: `cargo test -p mkrew-ui --lib gutter -j 4`
Expected: compile error `cannot find function changes`.

- [ ] **Step 3: Implement `gutter.rs`** (above the test module)

```rust
//! The Files editor's change gutter (SPEC §12A, ADR NNNN): per line of the edited text, how it
//! differs from the file's text in the git index. Pure; slow on big files, so run it off the
//! UI thread.

use std::time::{Duration, Instant};

use similar::{Algorithm, DiffOp, capture_diff_slices_deadline};

/// How one line differs from the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Modified,
    /// Lines were removed just above this one (or, on the last line, after it).
    Deleted,
}

/// A diff taking longer stops here with a coarser (still correct) answer.
// ponytail: fixed budget; make it scale with the file if 2 MB rewrites mark too much.
const DEADLINE: Duration = Duration::from_millis(100);

/// One mark per line of `text` (`str::lines`: `\n` or `\r\n`, so CRLF text matches its LF
/// index text) against `base`.
pub fn changes(base: &str, text: &str) -> Vec<Option<Change>> {
    let old: Vec<&str> = base.lines().collect();
    let new: Vec<&str> = text.lines().collect();
    let mut marks = vec![None; new.len()];
    let Some(last) = new.len().checked_sub(1) else {
        return marks;
    };
    let ops = capture_diff_slices_deadline(
        Algorithm::Myers,
        &old,
        &new,
        Some(Instant::now() + DEADLINE),
    );
    for op in ops {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Insert { new_index, new_len, .. } => {
                marks[new_index..new_index + new_len].fill(Some(Change::Added));
            }
            DiffOp::Replace { new_index, new_len, .. } => {
                marks[new_index..new_index + new_len].fill(Some(Change::Modified));
            }
            DiffOp::Delete { new_index, .. } => {
                marks[new_index.min(last)].get_or_insert(Change::Deleted);
            }
        }
    }
    marks
}
```

`capture_diff_slices_deadline` returns `Equal`/`Insert`/`Delete` ops, and `Replace` only after `Replace` hook grouping. If no `Replace` ever shows (test `added_changed_and_removed_lines_are_marked` fails with `Deleted`+`Added` instead of `Modified`), then a `Delete` directly followed by an `Insert` at the same `new_index` is a modification. Handle it in the loop: remember the last `Delete`'s `new_index`, and if the next op is an `Insert` at that index, fill `Modified` there instead of `Added` (and clear the `Deleted` mark it set).

- [ ] **Step 4: Run the gutter tests**

Run: `cargo test -p mkrew-ui --lib gutter -j 4`
Expected: PASS (5 tests).

- [ ] **Step 5: Carry the marks in `Code`**

`crates/mkrew-ui/src/highlight.rs`, `pub struct Code`, after `syntax`:

```rust
    /// Per line, how it differs from the file's index text (SPEC §12A gutter); empty when the
    /// read had no `git_base` (untracked, no repository) or for Docs code blocks.
    pub changes: Vec<Option<crate::gutter::Change>>,
```

In `Code::plain`, add `changes: Vec::new(),` to the literal. In `Code::new`, after `code.too_large = content.too_large;`:

```rust
        if let Some(base) = &content.git_base {
            code.changes = crate::gutter::changes(base, text);
        }
```

Run `cargo check -p mkrew-ui --all-targets -j 4`. Any other `Code { .. }` literal it reports gets `changes: Vec::new(),`.

- [ ] **Step 6: Write the failing view test** (`crates/mkrew-ui/src/files_view/tests.rs`, after `a_highlight_is_kept_only_for_the_text_it_was_made_of`)

```rust
fn file_with_base(text: &str, base: &str) -> Result<(FsContent, Code), String> {
    let (mut content, _) = file_text(text)?;
    content.git_base = Some(base.into());
    let code = Code::new(&content);
    Ok((content, code))
}

fn marks(view: &FilesView) -> Vec<Option<crate::gutter::Change>> {
    view.shown().map(|shown| shown.code.changes.clone()).unwrap_or_default()
}

/// Staging or committing leaves the file on disk as it is (same hash) but moves the index:
/// the gutter follows, caret and undo stay. With unsaved text the new index text waits for
/// the next typing pause.
#[test]
fn a_new_index_text_moves_the_gutter_and_keeps_the_caret() {
    use crate::gutter::Change::Added;
    let mut view = listed();
    view.open("README.md");
    view.begin_read();
    view.read("README.md", file_with_base("a\nb\n", "a\n"));
    assert_eq!(marks(&view), [None, Some(Added)]);
    view.editor_mut().unwrap().set_cursor(Pos::new(1, 1), false);

    view.begin_read();
    view.read("README.md", file_with_base("a\nb\n", "a\nb\n"));
    assert_eq!(marks(&view), [None, None], "staged: no marks");
    assert_eq!(view.line(), Some(2), "the caret stayed");

    view.editor_mut().unwrap().insert("x");
    view.begin_read();
    view.read("README.md", file_with_base("a\nb\n", "a\n"));
    assert_eq!(text(&view), "a\nxb\n", "unsaved text stays");
    let version = view.shown().unwrap().editor.version();
    let request = view.highlight_request(version).unwrap();
    assert_eq!(request.git_base.as_deref(), Some("a\n"), "the pause diffs with it");
    assert_eq!(Code::new(&request).changes, [None, Some(Added)], "xb is new against a");
}
```

Copy how `opened(..)` opens a file and how the existing tests read the text (a `text(&view)`-like helper, `view.line()`, `highlight_request`'s real signature and return type). Use those exact names.

Run: `cargo test -p mkrew-ui --lib a_new_index_text_moves_the_gutter -j 4`
Expected: FAIL (the second read keeps the old marks, or `request.git_base` is `None`).

- [ ] **Step 7: Keep the index text in `Shown`**

`crates/mkrew-ui/src/files_view.rs`, `pub struct Shown`, after `language`:

```rust
    /// The file's text in the git index from the last read (`fs.read` `git_base`); the text
    /// highlighted again after typing is diffed with it (SPEC §12A gutter).
    pub base: Option<String>,
```

In `fn show` (the function `read` calls to put a read on screen), three changes:
1. In the `if dirty { .. }` block, inside the `if let (Ok((content, _)), Content::Shown(shown))` body, add `shown.base = content.git_base.clone();` after the `disk_changed` assignment.
2. Replace the arm
   `Ok((content, _)) if hash.is_some() && content.hash == hash => return,`
   with a check placed **before** `self.content = match read {`:

```rust
        // The same version again (our own save, a change elsewhere): caret and undo stay; a
        // staged or committed change still moves the gutter.
        if let (Ok((content, code)), Content::Shown(shown)) = (&read, &mut self.content)
            && hash.is_some()
            && content.hash == hash
        {
            if shown.base != content.git_base {
                shown.base = content.git_base.clone();
                shown.code = Rc::new(code.clone());
            }
            return;
        }
```

   Then delete the old arm.
3. In the `Content::Shown(Box::new(Shown { .. }))` literal, add `base: content.git_base,`. Reorder the fields if the borrow checker asks.

In `highlight_request`, replace `git_base: None,` with `git_base: shown.base.clone(),`.

Run `cargo check -p mkrew-ui --all-targets -j 4`. Any other `Shown { .. }` literal in tests gets `base: None,`.

- [ ] **Step 8: Run the view tests**

Run: `cargo test -p mkrew-ui --lib files_view -j 4`
Expected: PASS (the new test and all existing ones).

- [ ] **Step 9: Theme constants and contrast test**

`crates/mkrew-ui/src/theme.rs`, after `CARET_WIDTH`:

```rust
/// The Files editor's change bar (SPEC §12A): its width, and the height of the mark where
/// lines were removed.
pub const GIT_GUTTER_BAR_WIDTH: f32 = 3.0;
pub const GIT_GUTTER_DELETED_HEIGHT: f32 = 6.0;
```

In `mod tests`, after `highlighted_diff_lines_stay_readable`:

```rust
    /// The Files editor's change bars stand out from the editor and its caret row
    /// (WCAG 1.4.11: graphical objects ≥ 3:1).
    #[test]
    fn change_bars_stand_out_from_the_editor() {
        for theme in [Theme::dark(), Theme::light()] {
            for fg in [theme.diff_add_text, theme.diff_mod_text, theme.diff_del_text] {
                for bg in [theme.bg, theme.surface, theme.surface_raised] {
                    let ratio = contrast(fg, bg);
                    assert!(ratio >= 3.0, "{fg:06X} on {bg:06X}: {ratio:.2}");
                }
            }
        }
    }
```

Run: `cargo test -p mkrew-ui --lib change_bars_stand_out -j 4`. Expected: PASS. If it fails, do not change a color. Stop and record the failing pair in HUMAN_CHECKS "Waiting".

- [ ] **Step 10: Draw the bar**

`crates/mkrew-ui/src/preview.rs`, `RowMarks` gets this after `found`:

```rust
    /// The line's change against the git index (SPEC §12A gutter).
    pub change: Option<crate::gutter::Change>,
```

In `code_row`, find the line-number child (the `div().flex_none()` holding `gutter(index + 1, total)` with the accent / `text_muted` color). Make it a row that holds the number and the bar:

```rust
        .child(
            div()
                .flex_none()
                .flex()
                .gap(px(space::XS))
                .text_color(rgb(if current {
                    theme.accent
                } else {
                    theme.text_muted
                }))
                .child(gutter(index + 1, total))
                .child(change_bar(marks.change, theme)),
        )
```

Keep the existing condition that sets `current`.

Add after `code_row`:

```rust
/// The change bar right of a line number (SPEC §12A): the whole row for an added or changed
/// line, a short mark at the top for lines removed above it. Always as wide, so the text
/// never moves.
fn change_bar(change: Option<crate::gutter::Change>, theme: &Theme) -> Div {
    use crate::gutter::Change;
    let bar = div().flex_none().w(px(theme::GIT_GUTTER_BAR_WIDTH));
    match change {
        None => bar,
        Some(Change::Added) => bar.h_full().bg(rgb(theme.diff_add_text)),
        Some(Change::Modified) => bar.h_full().bg(rgb(theme.diff_mod_text)),
        Some(Change::Deleted) => bar
            .h(px(theme::GIT_GUTTER_DELETED_HEIGHT))
            .bg(rgb(theme.diff_del_text)),
    }
}
```

`crates/mkrew-ui/src/shell/files_render.rs`, `render_editor`: in the `preview::RowMarks { .. }` literal, add after `found,`:

```rust
                            // Marks of an older text could sit on moved lines: only when it matches.
                            change: code.changes.get(index).copied().flatten().filter(|_| fresh),
```

Here `fresh` stands for the condition `render_editor` already uses to decide whether the highlight's spans still apply to this row (the line text equals `code.lines[index]`). Reuse that same check under whatever name it has, and do not compute it twice. Any other `RowMarks { .. }` literal gets `change: None,` (or `..Default::default()` if it already uses that).

Run: `cargo build -p mkrew-ui -j 4`. Expected: builds.

- [ ] **Step 11: Smoke run**

Run `cargo run -p mkrew-ui`. Open a workspace that is a git repository and open a tracked file in the Files view. Type a new line, change a line, delete a line, and wait ~150 ms. A green, amber or red mark should appear by each, and the text must not shift. Save, then stage the file in the Git view and come back: the marks are gone. Toggle the theme (Ctrl+Shift+J) and check both themes. If you cannot see the screen, write that in the HUMAN_CHECKS bullets below.

- [ ] **Step 12: SPEC and HUMAN_CHECKS**

`docs/SPEC.md` §12A: insert a new bullet directly after the **Editor** bullet (the one ending "…or press the track to jump there."):

```markdown
- **Change gutter** (ADR NNNN): right of each line number a thin bar marks how the line differs
  from the file's text in the git index (what the Git view calls unstaged): added
  (`diff-add-text`), changed (`diff-mod-text`), and a short `diff-del-text` mark at the top of
  the line after removed lines. It follows unsaved typing (recomputed with the highlight, 150 ms
  after typing stops; marks on lines edited since are hidden until then) and staging/committing
  (the `git.changed` re-read). Untracked files, files outside a repository and conflicted files
  have no bars; the bar column is always there so the text never moves.
```

`docs/HUMAN_CHECKS.md`: create the section header from Global Constraints directly above the first existing `### To verify —` heading, with:

```markdown
### To verify — 2026-10-11 gutter, graph, layouts (plan docs/superpowers/plans/2026-10-11-gutter-graph-layouts.md)

- [ ] **Files change gutter (dark + light):** open a tracked file; add, change and delete lines →
  green / amber bars and a short red mark appear ~150 ms after typing stops; the text never
  shifts; Save + stage the file in Git → the bars go; an untracked file has no bars; a file in a
  child repository (ADR 0029) uses that repository. On Windows with `core.autocrlf=true` an
  unchanged file shows no bars.
```

- [ ] **Step 13: Verify and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p mkrew-ui --lib -j 4`.

```bash
git add crates/mkrew-ui/src/gutter.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/highlight.rs crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view/tests.rs crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/shell/files_render.rs crates/mkrew-ui/src/theme.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): change gutter in the Files editor"
```

---

### Task 3: History graph data (`--topo-order --parents`, `git_graph`)

Spec: §12B History.

**Files:**
- Modify: `crates/mkrew-daemon/src/git.rs` (`GitCli::log`; `mod tests`)
- Create: `crates/mkrew-ui/src/git_graph.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod git_graph;` after `pub mod git_diff;` or wherever it sorts)
- Modify: `crates/mkrew-ui/src/git_history.rs` (`History.graph`, `Default`, `logged`)
- Modify: `crates/mkrew-ui/src/git_view.rs` (`ListRow::Commit.graph`, `list_rows`, new `graph_cell`, new `GitView::graph_lanes`)
- Modify: `crates/mkrew-ui/src/git_view/tests.rs`
- Modify: `crates/mkrew-ui/src/shell/git_rows.rs` (only the pattern `ListRow::Commit { commit, expanded }` → add `..`; rendering is Task 4)

**Interfaces:**
- Produces: `git_graph::GraphRow { lane: usize, up: Vec<(usize, usize)>, down: Vec<(usize, usize)> }` (`Clone, Eq, Debug`) with `fn below(&self) -> Vec<usize>`. `git_graph::GraphCell { Commit(GraphRow), Through(Vec<usize>) }`. `git_graph::layout(&[Commit]) -> Vec<GraphRow>`. `git_graph::lanes(&[GraphRow]) -> usize`. `git_graph::MAX_LANES: usize = 8`. `History.graph: Vec<GraphRow>`. `ListRow::Commit { commit, expanded, graph: GraphRow }`. `git_view::graph_cell(rows: &[ListRow], index: usize) -> Option<GraphCell>`. `GitView::graph_lanes(&self) -> usize`.

Fact checked on 2026-10-11: plain `git log -- <path>` prints unrewritten parents (`%P`) that point at commits the path filter hid. With `--parents` the parents are rewritten to listed commits.

- [ ] **Step 1: Write the failing daemon tests** (`crates/mkrew-daemon/src/git.rs`, `mod tests`, after `index_text_reads_the_staged_version`)

```rust
    /// Commit dates can be wrong (clock skew): the log still lists every commit before its
    /// parents, which the History graph relies on.
    #[test]
    fn log_lists_children_before_their_parents() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        run(root, &["init", "-q", "-b", "main"]);
        run(root, &["config", "core.autocrlf", "false"]);
        let at = |date: &str, args: &[&str]| {
            let output = std::process::Command::new("git")
                .current_dir(root)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .env("GIT_AUTHOR_DATE", date)
                .env("GIT_COMMITTER_DATE", date)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        let change = |name: &str, date: &str| {
            std::fs::write(root.join(name), name).unwrap();
            run(root, &["add", "."]);
            at(date, &["commit", "-q", "-m", name]);
        };
        change("a", "@1000 +0000");
        run(root, &["checkout", "-q", "-b", "side"]);
        change("x", "@500 +0000"); // older than its parent `a`
        run(root, &["checkout", "-q", "main"]);
        change("b", "@2000 +0000");
        at("@3000 +0000", &["merge", "-q", "--no-ff", "--no-edit", "side"]);

        let commits = GitCli.log(root, 50, None).unwrap();
        assert_eq!(commits.len(), 4);
        let index = |hash: &str| commits.iter().position(|commit| commit.hash == hash);
        for (at, commit) in commits.iter().enumerate() {
            for parent in &commit.parents {
                assert!(index(parent).is_some_and(|p| p > at), "{} before a parent", commit.summary);
            }
        }
    }

    /// A project in a subfolder lists only the commits touching it; their parents are the
    /// listed commits (not the hidden ones in between), so graph lanes join.
    #[test]
    fn log_of_a_subfolder_rewrites_parents_to_listed_commits() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let dir = root.join("app");
        std::fs::create_dir(&dir).unwrap();
        run(root, &["init", "-q"]);
        run(root, &["config", "core.autocrlf", "false"]);
        std::fs::write(dir.join("a"), "1").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "one"]);
        std::fs::write(root.join("other"), "x").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "outside"]);
        std::fs::write(dir.join("a"), "2").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "two"]);

        let commits = GitCli.log(&dir, 50, None).unwrap();
        let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(summaries, ["two", "one"]);
        assert_eq!(commits[0].parents, [commits[1].hash.clone()]);
    }
```

The test `run` helper already passes `-c user.name=t -c user.email=t@t -c commit.gpgsign=false`. The `at` closure repeats those flags because it needs the date env vars.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p mkrew-daemon --lib log_ -j 4`
Expected: at least the subfolder test FAILS (`parents` holds the hidden `outside` commit). The topo test may pass by chance with the default order. That is fine, because it guards the flag.

- [ ] **Step 3: Topo order and parent rewriting**

In `impl GitBackend for GitCli`, `fn log`, change

```rust
        command
            .arg(format!("--max-count={limit}"))
            .args(["HEAD", "--", path.unwrap_or(".")]);
```

to

```rust
        // The History graph (SPEC §12B) needs children before parents (`--topo-order`, also
        // under clock skew) and parents among the listed commits (`--parents` rewrites them
        // past commits the path filter hides; the format prints them as `%P`).
        command
            .arg(format!("--max-count={limit}"))
            .args(["--topo-order", "--parents", "HEAD", "--", path.unwrap_or(".")]);
```

Run: `cargo test -p mkrew-daemon --lib log_ -j 4` and `cargo test -p mkrew-core git -j 4`, then `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` and `cargo test -p mkrew-daemon --test daemon git -j 2`
Expected: PASS. `--parents` also prints the parents after the hash on the commit line. Our `--format` replaces that line, so `mkrew_core::git::parse_log` sees no change. If a parse test fails, read `LOG_FORMAT` and fix the parse. Do not drop the flag.

- [ ] **Step 4: Write the failing graph tests** (create `crates/mkrew-ui/src/git_graph.rs`; add `pub mod git_graph;` to `lib.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.into(),
            parents: parents.iter().map(|parent| (*parent).into()).collect(),
            author_name: String::new(),
            author_email: String::new(),
            authored_at: 0,
            summary: String::new(),
            body: String::new(),
        }
    }

    fn row(lane: usize, up: &[(usize, usize)], down: &[(usize, usize)]) -> GraphRow {
        GraphRow {
            lane,
            up: up.to_vec(),
            down: down.to_vec(),
        }
    }

    #[test]
    fn a_straight_history_is_one_lane() {
        let rows = layout(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);
        assert_eq!(
            rows,
            [row(0, &[], &[(0, 0)]), row(0, &[(0, 0)], &[(0, 0)]), row(0, &[(0, 0)], &[])]
        );
        assert_eq!(lanes(&rows), 1);
        assert_eq!(lanes(&[]), 0);
    }

    /// m merges f (a side branch off a) into b:
    ///   m      lane 0, opens lane 1 for f
    ///   | f    lane 1
    ///   b |    lane 0, lane 1 passes by
    ///   a      both lanes end in a
    #[test]
    fn a_merge_opens_a_lane_that_ends_in_the_fork_point() {
        let rows = layout(&[
            commit("m", &["b", "f"]),
            commit("f", &["a"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ]);
        assert_eq!(
            rows,
            [
                row(0, &[], &[(0, 0), (0, 1)]),
                row(1, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)]),
                row(0, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)]),
                row(0, &[(0, 0), (1, 0)], &[]),
            ]
        );
        assert_eq!(rows[0].below(), [0, 1]);
        assert_eq!(lanes(&rows), 2);
    }

    /// The log stops at 50 commits: x's parents are not listed.
    #[test]
    fn lanes_to_unlisted_parents_run_off_the_bottom() {
        let rows = layout(&[commit("y", &["x"]), commit("x", &["p", "q"])]);
        assert_eq!(rows[1], row(0, &[(0, 0)], &[(0, 0), (0, 1)]));
        assert_eq!(rows[1].below(), [0, 1]);
    }

    #[test]
    fn lanes_are_capped() {
        let parents: Vec<String> = (0..12).map(|n| format!("p{n}")).collect();
        let parents: Vec<&str> = parents.iter().map(String::as_str).collect();
        let rows = layout(&[commit("octopus", &parents)]);
        assert_eq!(rows[0].down.len(), 12);
        assert_eq!(lanes(&rows), MAX_LANES);
    }
}
```

Run: `cargo test -p mkrew-ui --lib git_graph -j 4`
Expected: compile error (`layout` not found).

- [ ] **Step 5: Implement `git_graph.rs`** (above the tests)

```rust
//! The lane graph beside the Git History (SPEC §12B): for each listed commit, the lane of its
//! dot and the lines entering it from above and leaving it below. Pure. The commits come
//! newest first from `git log --topo-order --parents`: a commit is listed before its parents.

use mkrew_protocol::Commit;

/// Lanes drawn at most; a wider history is cut at the right (dots sit in the last lane).
// ponytail: fixed cap; scroll the graph sideways if 8 lanes ever hide real work.
pub const MAX_LANES: usize = 8;

/// One commit's row of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// The lane of the commit's dot.
    pub lane: usize,
    /// Lines in the row's top half, `(lane at the top, lane at the dot)`: lanes passing by,
    /// and the lines of this commit's children ending in it.
    pub up: Vec<(usize, usize)>,
    /// Lines in the bottom half, `(lane at the dot, lane at the bottom)`: lanes passing by
    /// and the lines to this commit's parents. Sorted.
    pub down: Vec<(usize, usize)>,
}

impl GraphRow {
    /// The lanes leaving the row at the bottom, each once, in order: they run straight on
    /// through the rows listed under the commit (its files).
    pub fn below(&self) -> Vec<usize> {
        let mut lanes: Vec<usize> = self.down.iter().map(|&(_, to)| to).collect();
        lanes.sort_unstable();
        lanes.dedup();
        lanes
    }
}

/// What the graph column of one History row draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphCell {
    Commit(GraphRow),
    /// A row listed under a commit (a file, a note): these lanes run straight through it.
    Through(Vec<usize>),
}

/// The graph of `commits` (children before parents), one row per commit.
pub fn layout(commits: &[Commit]) -> Vec<GraphRow> {
    // Per lane, the commit its line leads down to; `None` = free.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for commit in commits {
        let hash = commit.hash.as_str();
        let lane = match lanes.iter().position(|waits| *waits == Some(hash)) {
            Some(lane) => lane,
            // A tip no listed commit leads to.
            None => free_lane(&mut lanes),
        };
        let up: Vec<(usize, usize)> = lanes
            .iter()
            .enumerate()
            .filter_map(|(at, waits)| waits.map(|waits| (at, if waits == hash { lane } else { at })))
            .collect();
        let mut down: Vec<(usize, usize)> = up
            .iter()
            .filter(|&&(from, to)| from == to && from != lane)
            .copied()
            .collect();
        for waits in &mut lanes {
            if *waits == Some(hash) {
                *waits = None;
            }
        }
        for (nth, parent) in commit.parents.iter().enumerate() {
            let parent = parent.as_str();
            let to = if nth == 0 {
                lane
            } else if let Some(waiting) = lanes.iter().position(|waits| *waits == Some(parent)) {
                waiting
            } else {
                free_lane(&mut lanes)
            };
            if to == lanes.len() {
                lanes.push(None);
            }
            lanes[to] = Some(parent);
            down.push((lane, to));
        }
        down.sort_unstable();
        down.dedup();
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        rows.push(GraphRow { lane, up, down });
    }
    rows
}

/// Lanes the graph of `rows` needs (0 for none), at most [`MAX_LANES`].
pub fn lanes(rows: &[GraphRow]) -> usize {
    rows.iter()
        .flat_map(|row| {
            let edges = row.up.iter().chain(&row.down).flat_map(|&(from, to)| [from, to]);
            std::iter::once(row.lane).chain(edges)
        })
        .max()
        .map_or(0, |widest| (widest + 1).min(MAX_LANES))
}

/// The first free lane, or a new one at the right.
fn free_lane(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(lane) => lane,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}
```

Trace the merge test by hand before you run it:
- `m` takes lane 0 and pushes b → lane 0, f → lane 1.
- `f` sits in lane 1. `up` = [(0,0), (1,1)] and `down` = [(0,0) pass-by, (1,1) to a].
- `b` sits in lane 0. `up` = [(0,0), (1,1)] and `down` = [(0,0) to a, (1,1) pass-by].
- `a` is waited on by both lanes. It takes lane 0, and `up` = [(0,0), (1,0)].

The `if to == lanes.len()` guard is dead code because `free_lane` pushes. Delete it if clippy or review flags it.

Run: `cargo test -p mkrew-ui --lib git_graph -j 4`
Expected: PASS (4 tests).

- [ ] **Step 6: Write the failing view test** (`crates/mkrew-ui/src/git_view/tests.rs`, after the test that uses `history()` to list commits with the selected one's files)

```rust
/// The files listed under a commit carry its lanes on; the working changes have no graph.
#[test]
fn rows_under_a_commit_carry_its_lanes_on() {
    let mut view = history();
    // `history()` lists b (expanded, files x and y) then a. Give b a second parent.
    let mut b = commit("b");
    b.parents = vec!["a".into(), "side".into()];
    view.logged(Ok(vec![b, commit("a")]));
    assert_eq!(view.graph_lanes(), 2);
    let rows = view.list_rows();
    let commit_at = rows
        .iter()
        .position(|row| matches!(row, ListRow::Commit { commit, .. } if commit.hash == "b"))
        .unwrap();
    assert!(matches!(graph_cell(&rows, commit_at), Some(GraphCell::Commit(row)) if row.lane == 0));
    assert_eq!(graph_cell(&rows, commit_at + 1), Some(GraphCell::Through(vec![0, 1])));
    assert!(
        (0..commit_at).all(|index| graph_cell(&rows, index).is_none()),
        "rows above the History have no graph"
    );
}
```

Read `fn history()` and `history_labels` first. If `logged` drops the selection, so that `b` is no longer expanded, re-select `b` the way `history()` does (`select_commit` + `commit_shown`) before calling `list_rows`. Update `history_labels`: `ListRow::Commit { commit, expanded }` → `ListRow::Commit { commit, expanded, .. }`. Import `crate::git_graph::GraphCell` if `super::*` does not bring it in.

Run: `cargo test -p mkrew-ui --lib rows_under_a_commit -j 4`
Expected: compile error (`graph_cell`, `graph_lanes` not found).

- [ ] **Step 7: Store the graph and expose it per row**

`crates/mkrew-ui/src/git_history.rs`:
- Import: `use crate::git_graph::{self, GraphRow};`
- `pub struct History` gets this after `commits`:

```rust
    /// The lane graph of the listed commits, one row each ([`git_graph::layout`]).
    pub graph: Vec<GraphRow>,
```

- `Default`: `graph: Vec::new(),`
- In the method that stores a `git.log` answer (`logged` or equivalent): on `Err`, `self.graph.clear();`. Before storing `Commits::Loaded(commits)`, set `self.graph = git_graph::layout(&commits);`.

`crates/mkrew-ui/src/git_view.rs`:
- Import: `use crate::git_graph::{self, GraphCell, GraphRow};`
- `ListRow::Commit` becomes:

```rust
    /// A History commit; the selected one is expanded (its files follow it).
    Commit {
        commit: Commit,
        expanded: bool,
        /// Its row of the lane graph (SPEC §12B).
        graph: GraphRow,
    },
```

- In `list_rows`, iterate `history.commits().iter().zip(&history.graph)` instead of the commits alone, and push `ListRow::Commit { commit: commit.clone(), expanded, graph: graph.clone() }`.
- Add to `impl GitView`, after `list_rows`:

```rust
    /// Width of the History's graph column in lanes (0 outside the History).
    pub fn graph_lanes(&self) -> usize {
        self.history.as_ref().map_or(0, |history| git_graph::lanes(&history.graph))
    }
```

- Add a free function after `impl GitView`:

```rust
/// The graph column of row `index` of [`GitView::list_rows`]: a commit's own row; for the
/// files or the note listed under a commit, its lanes running on. `None` outside the History.
pub fn graph_cell(rows: &[ListRow], index: usize) -> Option<GraphCell> {
    match rows.get(index)? {
        ListRow::Commit { graph, .. } => Some(GraphCell::Commit(graph.clone())),
        ListRow::File(..) | ListRow::Note(_) => rows[..index].iter().rev().find_map(|row| match row {
            ListRow::Commit { graph, .. } => Some(GraphCell::Through(graph.below())),
            ListRow::Header(..) => Some(GraphCell::Through(Vec::new())),
            _ => None,
        }),
        ListRow::Header(..) | ListRow::Hint(_) => None,
    }
    .filter(|cell| !matches!(cell, GraphCell::Through(lanes) if lanes.is_empty()))
}
```

The `Header` arm stops the backwards search at a Working-mode group header. That way a Working file never borrows a History commit's lanes. The final `filter` turns an empty `Through` into `None`.

`crates/mkrew-ui/src/shell/git_rows.rs`: every `ListRow::Commit { commit, expanded }` pattern gets `..`. Run `cargo check -p mkrew-ui --all-targets -j 4` and fix any other pattern it reports.

- [ ] **Step 8: Run the UI tests**

Run: `cargo test -p mkrew-ui --lib git_ -j 4`
Expected: PASS.

- [ ] **Step 9: Verify and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings`.

```bash
git add crates/mkrew-daemon/src/git.rs crates/mkrew-ui/src/git_graph.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/git_history.rs crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view/tests.rs crates/mkrew-ui/src/shell/git_rows.rs
git commit -m "feat(ui): lane graph model for the Git History"
```

---

### Task 4: Draw the History graph

Spec: §12B History, §14.

**Files:**
- Modify: `crates/mkrew-ui/src/shell/git_rows.rs` (`git_list`, `git_file_row`, `git_commit_row`, new `graph_column`, `LANE_COLORS`)
- Modify: `crates/mkrew-ui/src/theme.rs` (graph constants)
- Modify: `crates/mkrew-ui/src/main.rs` (imports only)
- Modify: `docs/SPEC.md` (§12B), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `git_view::graph_cell`, `GitView::graph_lanes`, `git_graph::GraphCell` (Task 3).

- [ ] **Step 1: Theme constants**

`crates/mkrew-ui/src/theme.rs`, after `GIT_FILE_ROW_HEIGHT`:

```rust
/// The History's lane graph (SPEC §12B): one lane's width, a commit's dot, a line's width.
pub const GIT_GRAPH_LANE_WIDTH: f32 = 12.0;
pub const GIT_GRAPH_DOT_SIZE: f32 = 8.0;
pub const GIT_GRAPH_LINE_WIDTH: f32 = 2.0;
```

- [ ] **Step 2: The graph column**

`crates/mkrew-ui/src/main.rs`: add `use mkrew_ui::git_graph::GraphCell;` next to `use mkrew_ui::git_history::{self, Commits};`. Make sure the `use gpui::{…}` list contains `Bounds`, `PathBuilder`, `canvas`, `fill`, `point` and `size`, and add any that are missing. Shell files use `use crate::*;`, so names imported in `main.rs` reach them.

`crates/mkrew-ui/src/shell/git_rows.rs`, at the end of the file:

```rust
/// Lane colors of the History graph, from the workspace palette (≥ 3:1 on every surface, ADR
/// 0020); red and yellow are left to deletions and warnings.
const LANE_COLORS: [&str; 6] = ["blue", "purple", "teal", "orange", "pink", "green"];

/// The graph column of a History row (SPEC §12B), `lanes` wide: a commit's lines and dot, or
/// the lanes running straight through a row listed under a commit.
fn graph_column(cell: GraphCell, lanes: usize, theme: &Theme) -> gpui::Div {
    let colors = LANE_COLORS.map(|name| theme.workspace_color(name).unwrap_or(theme.accent));
    let color = move |lane: usize| rgb(colors[lane % colors.len()]);
    div()
        .flex_none()
        .h_full()
        .w(px(lanes as f32 * theme::GIT_GRAPH_LANE_WIDTH))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, (), window, _| {
                    let x = |lane: usize| {
                        bounds.left() + px(theme::GIT_GRAPH_LANE_WIDTH) * (lane as f32 + 0.5)
                    };
                    // Rows meet at their 1 px borders: draw over them so lines join.
                    let top = bounds.top() - px(theme::BORDER_WIDTH);
                    let bottom = bounds.bottom() + px(theme::BORDER_WIDTH);
                    let mid = bounds.center().y;
                    let shown = |lane: usize| lane < lanes;
                    let line = |window: &mut Window, from: Point<Pixels>, to: Point<Pixels>, lane: usize| {
                        let mut path = PathBuilder::stroke(px(theme::GIT_GRAPH_LINE_WIDTH));
                        path.move_to(from);
                        path.line_to(to);
                        if let Ok(path) = path.build() {
                            window.paint_path(path, color(lane));
                        }
                    };
                    match &cell {
                        GraphCell::Through(through) => {
                            for &lane in through.iter().filter(|&&lane| shown(lane)) {
                                line(window, point(x(lane), top), point(x(lane), bottom), lane);
                            }
                        }
                        GraphCell::Commit(row) => {
                            for &(from, to) in &row.up {
                                if shown(from) && shown(to) {
                                    line(window, point(x(from), top), point(x(to), mid), from);
                                }
                            }
                            for &(from, to) in &row.down {
                                if shown(from) && shown(to) {
                                    line(window, point(x(from), mid), point(x(to), bottom), to);
                                }
                            }
                            let lane = row.lane.min(lanes.saturating_sub(1));
                            let dot = px(theme::GIT_GRAPH_DOT_SIZE);
                            let at = point(x(lane) - dot / 2.0, mid - dot / 2.0);
                            window.paint_quad(
                                fill(Bounds::new(at, size(dot, dot)), color(lane))
                                    .corner_radii(dot / 2.0),
                            );
                        }
                    }
                },
            )
            .size_full(),
        )
}

/// Indent of a row listed under a History commit, past the commit's chevron.
fn under_commit() -> gpui::Div {
    div().flex_none().w(px(space::M))
}
```

Check `theme.workspace_color` and `WORKSPACE_COLORS` for the real color names. Use six names that exist, and skip any red or yellow. If a row's height is not fixed (`GIT_FILE_ROW_HEIGHT`), set `.h(px(theme::GIT_FILE_ROW_HEIGHT))` on the column instead of `h_full()`. Read how `git_file_row` sizes its rows.

- [ ] **Step 3: Rows draw it**

In `Shell::git_list`:
- Next to `let now = …;` add `let lanes = view.graph_lanes();`.
- In the `uniform_list` processor, keep each row's `index` and compute

```rust
                        let graph = (lanes > 0)
                            .then(|| git_view::graph_cell(&rows, index))
                            .flatten()
                            .map(|cell| (cell, lanes));
```

  Pass `graph` to `git_file_row` as a new argument after `focused`.

In `git_file_row`:
- Add the parameter `graph: Option<(GraphCell, usize)>,` after `focused: bool,`. Extend its doc comment with "; in the History, `graph` is the row's column of the lane graph".
- `ListRow::Note` arm: when `graph` is `Some((cell, lanes))`, put `graph_column(cell, lanes, theme)` and `under_commit()` as the first two children instead of the left padding the note uses today.
- `ListRow::Commit` arm: pass `graph` on to `git_commit_row`.
- File rows under a commit (`group == git_view::Group::Commit`): keep today's `pl(px(space::S + space::M))` only when `graph.is_none()`. Otherwise use `.when_some(graph, |el, (cell, lanes)| el.child(graph_column(cell, lanes, theme)).child(under_commit()))` before the first child. The `.when(.. graph.is_none() ..)` must come before `.when_some(graph, ..)`, because `when_some` moves `graph`.

In `git_commit_row`:
- Add the parameter `graph: Option<(GraphCell, usize)>,` after `focused: bool,`.
- Before the chevron child, add `.when_some(graph, |el, (cell, lanes)| el.child(graph_column(cell, lanes, theme)))`.
- Doc comment: "One History commit row: its lane graph column, a chevron …".

Run: `cargo build -p mkrew-ui -j 4` and `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: builds clean.

- [ ] **Step 4: Smoke run**

Run `cargo run -p mkrew-ui`. Open the Git view, then History (`h`) on a repository with merges. This repository has them. Check:
- a colored dot per commit;
- lines join across rows without gaps;
- a merge forks into a second lane, and that lane ends at the fork point;
- the expanded commit's files show the lanes running straight through them.

Check light and dark (Ctrl+Shift+J). A linear history should show a single line.

- [ ] **Step 5: SPEC and HUMAN_CHECKS**

`docs/SPEC.md` §12B: replace `- **History:** last 50 commits; select one to see its files and diffs.` with

```markdown
- **History:** last 50 commits (`git log --topo-order --parents`: children before parents,
  parents rewritten past commits a subfolder project hides); select one to see its files and
  diffs. A lane graph left of each commit draws its dot and the lines to its children and
  parents (merges fork into a lane that ends at the fork point; lanes to commits past the 50
  run off the bottom), at most 8 lanes, colored from the workspace palette (no red or yellow);
  the expanded commit's files show its lanes running straight through.
```

`docs/HUMAN_CHECKS.md`, at the end of the section from Task 2:

```markdown
- [ ] **History lane graph (dark + light):** Git → History (`h`) on a repo with merges → dots and
  lanes join across rows with no gaps; a merge opens a second lane that ends at its fork point;
  the expanded commit's files show the lanes running through; a linear repo shows one line; the
  selected row's fill does not hide the lines.
```

- [ ] **Step 6: Verify and commit**

Run: `cargo test -p mkrew-ui --lib -j 4`.

```bash
git add crates/mkrew-ui/src/shell/git_rows.rs crates/mkrew-ui/src/theme.rs crates/mkrew-ui/src/main.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): lane graph in the Git History"
```

---

### Task 5: Layout presets (model, commands, apply)

Spec: §5 split tree, §10 Terminals, §13.

**Files:**
- Modify: `crates/mkrew-ui/src/split.rs` (`Preset`; tests)
- Modify: `crates/mkrew-ui/src/tabs.rs` (`Tabs::arrange`), `crates/mkrew-ui/src/tabs/tests.rs`
- Modify: `crates/mkrew-ui/src/keymap.rs` (`Command::Layout(Preset)`: enum, `all`, `name`, `default_keys`; the spec-defaults test)
- Modify: `crates/mkrew-ui/src/palette.rs` (title, availability), `crates/mkrew-ui/src/palette/tests.rs`
- Modify: `crates/mkrew-ui/src/lifecycle.rs` (new `kind_noun`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (`new_pane` uses `kind_noun`; `run`: two arms)
- Modify: `crates/mkrew-ui/src/shell/tabs.rs` (new `Shell::apply_layout`)
- Modify: `crates/mkrew-ui/src/main.rs` (import `Preset`)
- Modify: `docs/SPEC.md` (§13)

**Interfaces:**
- Produces: `split::Preset { Columns2, Rows2, Columns3, MainAndStack, Grid }` (`Copy, Eq, Debug`) with `ALL: [Preset; 5]`, `label(self) -> &'static str`, `name(self) -> &'static str`, `panes(self) -> usize`, `build(self, &[Ulid]) -> Option<Node>`, `rects(self) -> Vec<Rect>`. `Tabs::arrange(&mut self, tab_id: Ulid, preset: Preset, new: &[Ulid]) -> Option<TabSetLayoutParams>`. `Command::Layout(Preset)`. `Shell::apply_layout(&mut self, preset: Preset, window, cx)`. `lifecycle::kind_noun(SessionKind) -> &'static str`.

Free keys checked on 2026-10-11: no `alt-shift-<digit>` binding exists (`git grep "alt-shift-[0-9]"` matched only `ctrl-alt-shift-v` in the terminal input test).

- [ ] **Step 1: Write the failing split tests** (`crates/mkrew-ui/src/split.rs`, in `mod tests`)

```rust
    #[test]
    fn presets_build_their_trees_in_pane_order() {
        let [a, b, c, d] = [Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new()];
        let half = |axis: &str, first: serde_json::Value, second: serde_json::Value| {
            json!({"split": axis, "ratio": 0.5, "children": [first, second]})
        };
        let pane = |id: Ulid| json!({"session": id.to_string()});
        let json = |preset: Preset, sessions: &[Ulid]| Node::to_json(preset.build(sessions).as_ref());
        assert_eq!(json(Preset::Columns2, &[a, b]), half("vertical", pane(a), pane(b)));
        assert_eq!(json(Preset::Rows2, &[a, b]), half("horizontal", pane(a), pane(b)));
        assert_eq!(
            json(Preset::MainAndStack, &[a, b, c]),
            half("vertical", pane(a), half("horizontal", pane(b), pane(c)))
        );
        assert_eq!(
            json(Preset::Grid, &[a, b, c, d]),
            half("vertical", half("horizontal", pane(a), pane(b)), half("horizontal", pane(c), pane(d)))
        );
        // Re-applied to its own panes (in tree order), a preset keeps every pane in place.
        let grid = Preset::Grid.build(&[a, b, c, d]).unwrap();
        assert_eq!(Preset::Grid.build(&grid.sessions()), Some(grid));
        // Only the preset's number of panes.
        assert_eq!(Preset::Columns3.build(&[a, b]), None);
        assert_eq!(Preset::Columns2.build(&[a, b, c]), None);
    }

    #[test]
    fn three_columns_are_equal() {
        let rects = Preset::Columns3.rects();
        let xs: Vec<f64> = rects.iter().map(|rect| rect.x).collect();
        for (x, want) in xs.iter().zip([0.0, 1.0 / 3.0, 2.0 / 3.0]) {
            assert!((x - want).abs() < 1e-9, "{xs:?}");
        }
        assert!(rects.iter().all(|rect| (rect.w - 1.0 / 3.0).abs() < 1e-9 && rect.h == 1.0));
        assert!(Preset::ALL.iter().all(|preset| preset.rects().len() == preset.panes()));
    }
```

Check the real signatures first: `Node::to_json` (takes `Option<&Node>`), `Node::sessions` (returns `Vec<Ulid>`), and `Node::rects` (what it returns, `Vec<(Ulid, Rect)>` or something else). Use the `json!` import the module's tests already have. The second `Columns3` split gets ratio 0.5 of the remaining two thirds, so every column is one third wide.

Run: `cargo test -p mkrew-ui --lib split -j 4`
Expected: compile error (`Preset` not found).

- [ ] **Step 2: Implement `Preset`** (`crates/mkrew-ui/src/split.rs`, after `pub struct Rect`)

```rust
/// A ready-made layout for a whole tab (SPEC §10, the split button's menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Two panes side by side.
    Columns2,
    /// Two panes stacked.
    Rows2,
    /// Three equal panes side by side.
    Columns3,
    /// One pane left, two stacked right.
    MainAndStack,
    /// Two by two.
    Grid,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Columns2,
        Preset::Rows2,
        Preset::Columns3,
        Preset::MainAndStack,
        Preset::Grid,
    ];

    /// Shown in the menu and the command palette.
    pub fn label(self) -> &'static str {
        match self {
            Preset::Columns2 => "2 columns",
            Preset::Rows2 => "2 rows",
            Preset::Columns3 => "3 columns",
            Preset::MainAndStack => "1 + 2",
            Preset::Grid => "2 × 2",
        }
    }

    /// Its name in `keymap.toml` after `layout_`.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Columns2 => "columns_2",
            Preset::Rows2 => "rows_2",
            Preset::Columns3 => "columns_3",
            Preset::MainAndStack => "main_stack",
            Preset::Grid => "grid",
        }
    }

    pub fn panes(self) -> usize {
        match self {
            Preset::Columns2 | Preset::Rows2 => 2,
            Preset::Columns3 | Preset::MainAndStack => 3,
            Preset::Grid => 4,
        }
    }

    /// The tree with `sessions` as its panes in tree order ([`Node::sessions`]: left to
    /// right, top to bottom within a column); `None` unless there are exactly
    /// [`Self::panes`] of them.
    pub fn build(self, sessions: &[Ulid]) -> Option<Node> {
        if sessions.len() != self.panes() {
            return None;
        }
        let pane = |at: usize| Node::pane(sessions[at]);
        let split = |split: Axis, ratio: f64, first: Node, second: Node| Node::Split {
            split,
            ratio,
            children: Box::new([first, second]),
        };
        let (side, stack) = (Axis::Vertical, Axis::Horizontal);
        Some(match self {
            Preset::Columns2 => split(side, 0.5, pane(0), pane(1)),
            Preset::Rows2 => split(stack, 0.5, pane(0), pane(1)),
            Preset::Columns3 => split(side, 1.0 / 3.0, pane(0), split(side, 0.5, pane(1), pane(2))),
            Preset::MainAndStack => split(side, 0.5, pane(0), split(stack, 0.5, pane(1), pane(2))),
            Preset::Grid => split(
                side,
                0.5,
                split(stack, 0.5, pane(0), pane(1)),
                split(stack, 0.5, pane(2), pane(3)),
            ),
        })
    }

    /// Each pane's area (fractions of the tab), for drawing the layout in the menu.
    pub fn rects(self) -> Vec<Rect> {
        let ids: Vec<Ulid> = (0..self.panes() as u128).map(Ulid).collect();
        self.build(&ids)
            .map(|node| node.rects().into_iter().map(|(_, rect)| rect).collect())
            .unwrap_or_default()
    }
}
```

Adapt `rects` to the real return type of `Node::rects`.

Run: `cargo test -p mkrew-ui --lib split -j 4`
Expected: PASS.

- [ ] **Step 3: Write the failing `arrange` tests** (`crates/mkrew-ui/src/tabs/tests.rs`, after the split/close/persist round-trip test; add `use crate::split::Preset;`)

```rust
#[test]
fn arrange_keeps_the_panes_in_order_and_adds_the_new_ones() {
    let project = Ulid::new();
    let [a, b, c, d] = [Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new()];
    let stacked = json!({"split":"horizontal","ratio":0.7,"children":[pane(a), pane(b)]});
    let tab = wire(project, "main", 1.0, stacked);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    assert!(tabs.focus(b));
    assert!(tabs.toggle_zoom());

    let write = tabs.arrange(tab.id, Preset::Grid, &[c, d]).unwrap();
    assert_eq!(
        write.layout,
        json!({"split":"vertical","ratio":0.5,"children":[
            {"split":"horizontal","ratio":0.5,"children":[pane(a), pane(b)]},
            {"split":"horizontal","ratio":0.5,"children":[pane(c), pane(d)]}
        ]})
    );
    let active = tabs.active().unwrap();
    assert_eq!(active.focused(), Some(b), "the focus stays");
    assert_eq!(active.zoomed(), None, "the zoom ends");
    let again = tabs.arrange(tab.id, Preset::Grid, &[]).unwrap();
    assert_eq!(again.layout, write.layout, "every pane stays where it is");
}

/// The tab may change while the new sessions start (Review Focus 5): nothing is lost or
/// doubled, and the tab need not be the active one.
#[test]
fn arrange_refuses_other_sizes_and_works_on_any_tab() {
    let project = Ulid::new();
    let [a, b, c, e] = [Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new()];
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]});
    let one = wire(project, "one", 1.0, split);
    // Tab two: the daemon placed `c`, the first session started for it, already.
    let two = wire(project, "two", 2.0, pane(c));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    assert_eq!(tabs.arrange(one.id, Preset::Columns3, &[]), None, "a pane short");
    assert_eq!(tabs.arrange(one.id, Preset::Columns2, &[e]), None, "a pane too many");
    assert_eq!(tabs.arrange(one.id, Preset::Columns3, &[c]), None, "c is in tab two");
    assert_eq!(tabs.tab(one.id).unwrap().sessions(), [a, b], "unchanged");

    let write = tabs.arrange(two.id, Preset::Columns2, &[c, e]).unwrap();
    assert_eq!(
        write.layout,
        json!({"split":"vertical","ratio":0.5,"children":[pane(c), pane(e)]})
    );
    assert_eq!(tabs.active().map(|tab| tab.id), Some(one.id), "still on tab one");
}
```

Check the real names of the `Tab` / `Tabs` accessors: `focus`, `toggle_zoom`, `focused()`, `zoomed()` and `sessions()`. `zoomed` may be a private field with no getter. If it has none, assert the zoom through the existing public way (`visible()` lists every pane again).

Run: `cargo test -p mkrew-ui --lib arrange -j 4`
Expected: compile error (`arrange` not found).

- [ ] **Step 4: Implement `Tabs::arrange`** (`crates/mkrew-ui/src/tabs.rs`, after `split_focused`; import `Preset` in `use crate::split::{..}`)

```rust
    /// Lays tab `tab_id` out as `preset` (SPEC §10 layouts): its panes keep their tree
    /// order, then come the `new` sessions it does not show yet. `None` (nothing changes)
    /// unless that makes exactly the preset's panes, or when a new session is shown in
    /// another tab. A zoom ends; the focus stays.
    pub fn arrange(
        &mut self,
        tab_id: Ulid,
        preset: Preset,
        new: &[Ulid],
    ) -> Option<TabSetLayoutParams> {
        if new.iter().any(|id| self.tab_of(*id).is_some_and(|tab| tab.id != tab_id)) {
            return None;
        }
        let tab = self.tab_mut(tab_id)?;
        let mut sessions = tab.sessions();
        for id in new {
            if !sessions.contains(id) {
                sessions.push(*id);
            }
        }
        tab.layout = Some(preset.build(&sessions)?);
        tab.zoomed = None;
        tab.fix_focus();
        Some(tab.write())
    }
```

Use the real names of the private helpers (`fix_focus`, `write`) that `split_focused` / `prune` call.

Run: `cargo test -p mkrew-ui --lib tabs -j 4`
Expected: PASS.

- [ ] **Step 5: The command, its keys and its palette entry**

`crates/mkrew-ui/src/keymap.rs`:
- Import: `use crate::split::{Direction, Preset};`
- `enum Command`, after `SplitShell`:

```rust
    /// Lay the active tab out as a preset (SPEC §10), starting the panes it lacks.
    Layout(Preset),
```

- `Command::all()`: insert `Command::Layout(preset)` for each preset in `Preset::ALL`, directly after `Command::SplitShell`. Keep the existing construction style. If `all()` is one array literal, list the five variants by hand.
- `name()`: `Command::Layout(preset) => format!("layout_{}", preset.name()),` (match how `Workspace(n)` / `Tab(n)` build their names).
- `default_keys()`:

```rust
            Command::Layout(preset) => match preset {
                Preset::Columns2 => "alt-shift-1",
                Preset::Rows2 => "alt-shift-2",
                Preset::Columns3 => "alt-shift-3",
                Preset::MainAndStack => "alt-shift-4",
                Preset::Grid => "alt-shift-5",
            }
            .into(),
```

- The keymap test that checks spec defaults (find it by `"Alt+Shift+S"`): add
  `assert_eq!(keymap.label(Command::Layout(Preset::Grid)), "Alt+Shift+5");`

`crates/mkrew-ui/src/palette.rs`:
- Title: `Command::Layout(preset) => format!("Panes: Layout {}", preset.label()),`
- Availability: put `| Command::Layout(_)` in the same arm as `Command::SplitRight | Command::SplitDown | Command::SplitAgent | Command::SplitShell`.

`crates/mkrew-ui/src/palette/tests.rs`: in the test that checks the split commands are hidden without tabs, assert the same for `Command::Layout(Preset::Columns2)`.

Run: `cargo test -p mkrew-ui --lib keymap -j 4` and `cargo test -p mkrew-ui --lib palette -j 4`
Expected: PASS. If a template or count test fails only because there are more commands, update its expected count.

- [ ] **Step 6: `Shell::apply_layout`**

`crates/mkrew-ui/src/lifecycle.rs`, after `split_kind`:

```rust
/// "an agent" / "a shell", for messages about starting one.
pub fn kind_noun(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Agent => "an agent",
        SessionKind::Shell => "a shell",
    }
}
```

In `Shell::new_pane` (`shell/sessions.rs`), if it has its own `match kind { .. => "an agent", .. }` for its error message, replace that match with `lifecycle::kind_noun(kind)`. If it phrases the message differently, leave it alone and keep `kind_noun` for `apply_layout` only.

`crates/mkrew-ui/src/main.rs`: `use mkrew_ui::split::Axis;` → `use mkrew_ui::split::{Axis, Preset};`.

`crates/mkrew-ui/src/shell/tabs.rs`, after `fn split`:

```rust
    /// Lays the active tab out as `preset` (SPEC §10): its panes stay, the missing ones start
    /// sessions of the focused pane's kind (as Split right) one after another, then the
    /// layout is written once. If a start fails, or the tab changed meanwhile, the sessions
    /// that did start are placed like Split right.
    pub(crate) fn apply_layout(
        &mut self,
        preset: Preset,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some((tab_id, have)) = (self.tabs.get(&project_id))
            .and_then(|tabs| tabs.active())
            .map(|tab| (tab.id, tab.sessions().len()))
        else {
            return;
        };
        if have > preset.panes() {
            self.tab_error = Some(format!(
                "This tab has {have} panes; the {} layout holds {}. Close panes first.",
                preset.label(),
                preset.panes()
            ));
            cx.notify();
            return;
        }
        let missing = preset.panes() - have;
        if missing == 0 {
            let write = (self.tabs.get_mut(&project_id))
                .and_then(|tabs| tabs.arrange(tab_id, preset, &[]));
            if let Some(write) = write {
                self.save_layout(project_id, write, window, cx);
                self.sync_panes(window, cx);
            }
            cx.notify();
            return;
        }
        let focused = (self.focused_session())
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let kind = lifecycle::split_kind(focused);
        if self.starting_pane.is_some() || self.ask_for_default(project_id, kind, window, cx) {
            return;
        }
        self.starting_pane = Some(kind);
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let mut started = Vec::new();
            let mut failed = None;
            for _ in 0..missing {
                let params = lifecycle::spawn_params(project_id, Some(tab_id), kind);
                match connection.call::<SessionSpawnResult>(Request::SessionSpawn(params)).await {
                    Ok(spawned) => started.push(spawned.session_id),
                    Err(error) => {
                        failed = Some(format!("{error:#}"));
                        break;
                    }
                }
            }
            let _ = this.update_in(cx, |shell, window, cx| {
                shell.starting_pane = None;
                let write = (shell.tabs.get_mut(&project_id))
                    .and_then(|tabs| tabs.arrange(tab_id, preset, &started));
                match write {
                    Some(write) => shell.save_layout(project_id, write, window, cx),
                    None => {
                        for session in started {
                            shell.place_session(project_id, session, Axis::Vertical, window, cx);
                        }
                    }
                }
                if let Some(error) = failed {
                    tracing::warn!(%project_id, "session.spawn failed: {error}");
                    let what = lifecycle::kind_noun(kind);
                    shell.tab_error = Some(format!("Cannot start {what}: {error}"));
                }
                shell.sync_panes(window, cx);
                cx.notify();
            });
        })
        .detach();
    }
```

Before pasting, copy the exact spawn call from `Shell::new_pane`:
- the `lifecycle::spawn_params` arguments;
- the result type and its session-id field (e.g. `Session` with `.id`);
- how the error is formatted;
- whether `ask_for_default` returns `true` when it opened a dialog;
- the real `save_layout` / `place_session` / `sync_panes` signatures.

Mirror them exactly. The daemon puts the first session spawned into an empty tab by itself (§10). `arrange` handles that, because a session already in the tab is not added twice.

`crates/mkrew-ui/src/shell/sessions.rs`, `Shell::run`:
- In the arm that switches to the Terminals view for tab and pane commands, add `| Command::Layout(_)` next to `Command::SplitShell`.
- After the `Command::SplitShell => …` action arm: `Command::Layout(preset) => self.apply_layout(preset, window, cx),`

Run: `cargo build -p mkrew-ui -j 4` and `cargo clippy -p mkrew-ui --all-targets -- -D warnings`. Fix any other exhaustive `match command` the compiler reports by putting `Command::Layout(_)` with the Split commands.

- [ ] **Step 7: Smoke run**

Run `cargo run -p mkrew-ui`. In a workspace with a default shell, open a one-pane tab:
- Alt+Shift+5 → four panes in a 2 × 2 grid, the old pane top-left, focus still on it.
- Alt+Shift+1 → "This tab has 4 panes…" shows and nothing changes.
- Close two panes, then Alt+Shift+3 → three equal columns.
- Ctrl+Shift+P, type "Layout" → five entries.
- Restart the UI → the layout persists.

- [ ] **Step 8: SPEC §13**

After the row `| Panes | Split right / split down | Alt+Shift+= / Alt+Shift+- |` add:

```markdown
| Panes | Layout the active tab: 2 columns / 2 rows / 3 columns / 1 + 2 / 2 × 2 (§10) | Alt+Shift+1 / 2 / 3 / 4 / 5 |
```

- [ ] **Step 9: Verify and commit**

Run: `cargo test -p mkrew-ui --lib -j 4`.

```bash
git add crates/mkrew-ui/src/split.rs crates/mkrew-ui/src/tabs.rs crates/mkrew-ui/src/tabs/tests.rs crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/lifecycle.rs crates/mkrew-ui/src/shell/sessions.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/src/main.rs docs/SPEC.md
git commit -m "feat(ui): layout presets for terminal tabs (Alt+Shift+1…5)"
```

---

### Task 6: The split button's layout menu

Spec: §10 Terminals, §14.

**Files:**
- Modify: `crates/mkrew-ui/src/context_menu.rs` (`LayoutMenu`)
- Modify: `crates/mkrew-ui/src/main.rs` (field `layout_menu`, its render call, import)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (init `layout_menu: None`)
- Modify: `crates/mkrew-ui/src/shell/focus.rs` (`close_menus`)
- Modify: `crates/mkrew-ui/src/shell/tabs.rs` (new `open_layout_menu`; `open_tab_menu` closes it)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs` (`render_tab_bar`: the split button opens the menu)
- Modify: `crates/mkrew-ui/src/shell/menus.rs` (new `render_layout_menu`, `layout_thumb`)
- Modify: `crates/mkrew-ui/src/theme.rs` (thumbnail size)
- Modify: `docs/SPEC.md` (§10), `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`

**Interfaces:**
- Consumes: `Preset::{ALL, label, panes, rects, name}`, `Command::Layout` (Task 5).
- Produces: `context_menu::LayoutMenu { tab_id: Ulid, position: Point<Pixels> }`, `Shell.layout_menu: Option<LayoutMenu>`.

This task is GPUI rendering and has no pure logic to unit test. The check is the smoke run in Step 5.

- [ ] **Step 1: State**

`crates/mkrew-ui/src/context_menu.rs`, after `PaneMenu`:

```rust
/// The split button's menu (Split right / down, the layouts) for one tab, at `position`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutMenu {
    pub tab_id: Ulid,
    pub position: Point<Pixels>,
}
```

Update the module doc's first sentence to also name "the split button's layout menu".

`crates/mkrew-ui/src/main.rs`:
- `use mkrew_ui::context_menu::{PaneMenu, TabMenu};` → `use mkrew_ui::context_menu::{LayoutMenu, PaneMenu, TabMenu};`
- In `struct Shell`, after `tab_menu: Option<TabMenu>,`:

```rust
    /// The open menu of the tab strip's split button (Split right / down, layouts).
    layout_menu: Option<LayoutMenu>,
```

- In the render, after the `self.tab_menu` `.children(..)` block, using the same filter:

```rust
                    .children(
                        self.layout_menu
                            .as_ref()
                            .filter(|_| self.view() == View::Terminals && !self.modal_open())
                            .map(|menu| self.render_layout_menu(menu, &theme, cx)),
                    )
```

`shell/connection.rs`: after `tab_menu: None,` add `layout_menu: None,`. `shell/focus.rs` `close_menus`: add `self.layout_menu = None;`.

`shell/tabs.rs`: in `open_tab_menu`, add `self.layout_menu = None;` next to where it closes the other menus. After `open_tab_menu` add:

```rust
    /// Opens the split button's menu for the active tab at `position` (SPEC §10).
    pub(crate) fn open_layout_menu(&mut self, position: Point<Pixels>, cx: &mut Context<'_, Self>) {
        let Some(tab_id) = (self.workspace)
            .and_then(|id| self.tabs.get(&id))
            .and_then(|tabs| tabs.active())
            .map(|tab| tab.id)
        else {
            return;
        };
        self.close_menus();
        self.layout_menu = Some(LayoutMenu { tab_id, position });
        cx.notify();
    }
```

If `close_menus` also closes something the user expects to stay open (the palette, a dialog), reset only `workspace_menu`, `pane_menu`, `tab_menu` and `file_menu` instead.

- [ ] **Step 2: The split button opens it**

`shell/pane_render.rs`, `render_tab_bar`: find the split button (id `"tab-split"` or the button whose `on_click` runs `Command::SplitRight`). Keep its id and icon. Change its tooltip to `"Split or pick a layout"` and its click handler to:

```rust
                    .on_click(cx.listener(|shell, event: &gpui::ClickEvent, _, cx| {
                        shell.open_layout_menu(event.position(), cx);
                    }))
```

Copy the `event.position()` call from the `⋯` button next to it.

- [ ] **Step 3: The menu**

`crates/mkrew-ui/src/theme.rs`, after `MENU_WIDTH`:

```rust
/// The drawing of a layout in the split button's menu (SPEC §10).
pub const LAYOUT_THUMB_WIDTH: f32 = 24.0;
pub const LAYOUT_THUMB_HEIGHT: f32 = 16.0;
```

`shell/menus.rs`, after `render_tab_menu`. The wrapper, panel, separator and close come from `render_tab_menu`. Check four names there before you paste: the keymap field (`self.keymap`), `tooltip(theme, text, shortcut)`, `Run(command)`, and the separator it draws. If any differs, use that function's form.

```rust
    /// The split button's menu (SPEC §10): Split right, Split down, then the layouts, each
    /// drawn small with its shortcut. A layout holding fewer panes than the tab has is greyed.
    /// Floats at the click; a click outside closes it.
    pub(crate) fn render_layout_menu(
        &self,
        menu: &LayoutMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let panes = (self.tabs.values())
            .find_map(|tabs| tabs.tab(menu.tab_id))
            .map_or(0, |tab| tab.sessions().len());
        let shortcut = |command: Command| {
            div()
                .text_size(font_size::META)
                .child(self.keymap.label(command))
        };
        let pick = |command: Command| {
            cx.listener(move |shell: &mut Self, _: &gpui::ClickEvent, window, cx| {
                shell.layout_menu = None;
                shell.run(&Run(command), window, cx);
            })
        };
        let split = |id: &'static str, label: &'static str, command: Command| {
            menu_item(id.into(), theme)
                .child(div().flex_1().text_color(rgb(theme.text)).child(label))
                .child(shortcut(command))
                .on_click(pick(command))
        };
        let mut panel = menu_panel("layout-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.layout_menu = None;
                cx.notify();
            }))
            .child(split("layout-menu-split-right", "Split right", Command::SplitRight))
            .child(split("layout-menu-split-down", "Split down", Command::SplitDown))
            .child(
                div()
                    .h(px(theme::BORDER_WIDTH))
                    .my(px(space::XS))
                    .bg(rgb(theme.border)),
            )
            .child(
                div()
                    .px(px(space::S))
                    .py(px(space::XS))
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child("Layouts"),
            );
        for preset in Preset::ALL {
            let command = Command::Layout(preset);
            let id = SharedString::from(format!("layout-menu-{}", preset.name()));
            let row = if panes <= preset.panes() {
                menu_item(id, theme).on_click(pick(command))
            } else {
                // The same box as a menu item, without its hover: it cannot be picked.
                let why = format!("This tab has {panes} panes; close some first");
                div()
                    .id(id)
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .py(px(space::XS))
                    .rounded(px(radius::SMALL))
                    .text_size(font_size::BODY)
                    .opacity(opacity::DISABLED)
                    .tooltip(tooltip(theme, &why, ""))
            };
            panel = panel.child(
                row.child(layout_thumb(preset, theme))
                    .child(div().flex_1().text_color(rgb(theme.text)).child(preset.label()))
                    .child(shortcut(command)),
            );
        }
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }
```

If the borrow checker rejects `pick` (it borrows `cx` mutably while `split` is also used), build each listener inline at its call site the same way.

At the end of `shell/menus.rs`, as a free function:

```rust
/// A small drawing of `preset`'s panes for the layout menu.
fn layout_thumb(preset: Preset, theme: &Theme) -> gpui::Div {
    div()
        .relative()
        .flex_none()
        .w(px(theme::LAYOUT_THUMB_WIDTH))
        .h(px(theme::LAYOUT_THUMB_HEIGHT))
        .children(preset.rects().into_iter().map(|rect| {
            div()
                .absolute()
                .left(relative(rect.x as f32))
                .top(relative(rect.y as f32))
                .w(relative(rect.w as f32))
                .h(relative(rect.h as f32))
                .p(px(theme::BORDER_WIDTH))
                .child(div().size_full().rounded(px(radius::SMALL)).bg(rgb(theme.subtle)))
        }))
}
```

If `relative` is not imported in `main.rs`'s `use gpui::{…}`, add it.

Run: `cargo build -p mkrew-ui -j 4` and `cargo clippy -p mkrew-ui --all-targets -- -D warnings`. Then check the diff for literals with `git diff -U0 crates/mkrew-ui/src` and look at the added lines. Outside `theme.rs` there must be no `0x……` color and no `px(<digit>`.

- [ ] **Step 4: SPEC §10**

In §10 Terminals, replace the sentence that says the strip's right end holds `+`, a split button (Split right) and `⋯` with:

```markdown
At the right end of the strip: `+` (new tab), the split button and `⋯`. The split button opens
the layout menu: Split right, Split down, then the layouts 2 columns, 2 rows, 3 columns, 1 + 2
and 2 × 2, each with a small drawing and its shortcut (§13). A layout rearranges the active tab:
its panes keep their tree order (left to right, top to bottom within a column) and the panes it
lacks start sessions of the focused pane's kind (as Split right), one after another; then the
layout is saved once. A layout with fewer panes than the tab has is greyed (its tooltip says
why); if a start fails, the sessions that started are placed as Split right places them.
```

- [ ] **Step 5: Smoke run**

Run `cargo run -p mkrew-ui`, in dark and light. Check:
- the split button opens the menu at the button;
- the five thumbnails match their names;
- the shortcuts show on the right;
- Split right and Split down still work from the menu;
- on a tab with 3 panes, "2 columns" and "2 rows" are greyed, and hovering one says why;
- a click outside closes the menu;
- opening `⋯` closes it.

- [ ] **Step 6: HUMAN_CHECKS and PROGRESS**

`docs/HUMAN_CHECKS.md`, at the end of the section from Task 2:

```markdown
- [ ] **Layout menu (dark + light):** the split button opens Split right / Split down / Layouts
  with five drawings (2 columns, 2 rows, 3 columns, 1 + 2, 2 × 2) and Alt+Shift+1…5; picking
  2 × 2 on a one-pane tab starts three panes of the focused kind, the old pane stays top-left and
  keeps the focus; on a 3-pane tab the 2-pane layouts are greyed with a tooltip; the layout
  survives a UI restart. Note: the split button no longer splits on one click (Alt+Shift+= still
  does). Say so if you want the one-click split back.
```

`docs/PROGRESS.md` "Notes for the next session", add one line at the top of the list:

```markdown
- 2026-10-11 gutter, graph, layouts (plan docs/superpowers/plans/2026-10-11-gutter-graph-layouts.md): `fs.read` `git_base` + `GitBackend::index_text` (protocol +1, ADR NNNN), `gutter.rs` marks in `Code.changes`; `git log --topo-order --parents`, `git_graph.rs`, `History.graph`, `ListRow::Commit.graph`, canvas lanes in `git_rows.rs`; `split::Preset`, `Tabs::arrange`, `Command::Layout` (Alt+Shift+1…5), `Shell::apply_layout`, `LayoutMenu` on the split button. Manual checks in HUMAN_CHECKS "2026-10-11 gutter, graph, layouts". Outside the milestone loop.
```

- [ ] **Step 7: Full verify and commit**

Run: `sh scripts/verify.sh`, or the fallback trio from Global Constraints.

```bash
git add crates/mkrew-ui/src/context_menu.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/menus.rs crates/mkrew-ui/src/theme.rs docs/SPEC.md docs/HUMAN_CHECKS.md docs/PROGRESS.md
git commit -m "feat(ui): split button opens a layout menu"
```

---

## Not in this plan (on purpose)

- Branch and tag labels in the History (Tern shows them): this needs `%D` in the log format and a new `Commit` field, so another protocol bump. Add it when the bare lanes are not enough.
- Curved lane lines: lines are straight, which is enough for 50 commits.
- Gutter marks you can click (to show or revert a hunk): the Git view does that already.
- Custom or saved layouts, and layouts with more than 4 panes.
