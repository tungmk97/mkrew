# Git review "Mark as viewed" Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the Git view the user marks each changed file as viewed (key `v`, a box on the file row, a toolbar button, a palette command); viewed files dim in the list and collapse in the diff, the header shows `Viewed N/M`, and a mark clears by itself when that file's diff changes.

**Architecture:** The daemon stores a mark as (project, task worktree, repo, list, path, hash of the file's diff text) in a new SQLite table (migration 0021). `git.set_viewed` hashes the diff as it is now and stores/removes the mark; `git.viewed` (asked by the UI after every `git.status` answer) re-hashes only the marked files through the existing diff cache, deletes marks whose text changed or whose file no longer changes, and returns the files that still stand. The UI keeps that answer in a small pure `ViewedSet` inside `GitView` (counter, row flag, diff collapse) and only toggles optimistically.

**Tech Stack:** Rust 2024, SQLite (`rusqlite`, numbered migrations), the daemon's `GitBackend` diff cache, GPUI (UI), FNV-1a 64 written by hand (no new dependency).

**Spec:** `docs/SPEC.md` §12B (Git view: two modes, changed-file list, diff viewer), §13 (Git single keys), §7 (`git.*` requests), §5 (migrations); design decided in chat 2026-10-19, recorded in the "Design" section of this plan.

## Global Constraints

- Numbers are reserved for planning: ADR **0065**, migration **0021**, `PROTOCOL_VERSION` **45** (now 43 in `crates/mkrew-protocol/src/lib.rs:16`). Before creating anything, re-check the newest ADR in `docs/decisions`, the newest file in `crates/mkrew-store/migrations` and `PROTOCOL_VERSION`, and take the next free number if another plan landed first (the tray plan reserves 0063 / 0020 / 44). A renamed migration keeps its place at the END of `MIGRATIONS` in `crates/mkrew-store/src/lib.rs`.
- Deviating from or adding to the spec needs an ADR in `docs/decisions/` AND a `docs/SPEC.md` update in the same commit: Task 3 does both, together with the code that implements it.
- UI work follows `.claude/skills/mkrew-ui-design/SKILL.md`: every color, spacing, radius, font size and opacity comes from `crates/mkrew-ui/src/theme.rs` tokens (spacing 4 · 8 · 12 · 16 · 24 · 32, radius 4 / 8 / 12); dark and light both work; status never by color alone; every list keeps its empty state; tooltips show shortcuts; the command palette lists every action. A "To verify" entry goes into `docs/HUMAN_CHECKS.md` (Task 6).
- Tests never call a real agent or the network; git tests run real `git` on temp repos like the existing ones in `crates/mkrew-daemon/src/git_view/tests.rs`.
- No `unwrap` / `expect` outside tests (SPEC §17); `thiserror` in libraries; `tracing` for logs.
- Windows: run `cargo` directly; the verify script is `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (fmt check, clippy `-D warnings`, tests). Per-crate loop: `cargo test -p <crate> <filter> -j 4`. After a protocol change run `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` before the daemon integration tests. Disk C: is nearly full: never create a second `target/`.
- `cargo insta` is not installed (this plan adds no snapshot).
- Never kill or restart the user's running daemon or UI. Never build the installer.
- Commits: Conventional Commits, `git -c user.email=tungnv975@gmail.com commit`, stage explicit paths only (never `add -A` or `.`), never push. Other agents have uncommitted work in the tree: never stage their files, never reset / stash / clean.

## Review Focus

The five failure modes the spec implies but a happy-path test would not hit, most likely first. Each has its pinning test in the task that owns the code.

1. **The agent edits a viewed file again, reverts it, or the user stages it.** Expected: an edit or a revert clears the mark (and deleting it is permanent: typing the old text back does not resurrect it); staging a whole viewed file keeps it viewed because the staged diff reads the same. Pinned by Task 1 (`resolve` tests), Task 3 (`a_mark_stands_until_its_diff_text_changes`, `staging_a_viewed_file_keeps_it_viewed`) and the integration test `git_viewed_marks_follow_the_diff_and_clear_themselves`.
2. **A binary file, a file whose diff is over 1 MB, an untracked file.** Expected: a changed binary clears its mark (git's `index` line carries the blob ids, so the text changes); an untracked file can be marked (its all-new diff is hashed); a file with no diff at all cannot (`invalid_state`, no empty-text mark). Pinned by Task 3 (`binary_and_untracked_files_can_be_marked`) and the integration test.
3. **Marks of another list must survive.** Expected: looking at Working changes never deletes the marks made in Against base or in a task's "since" changes (and the reverse); each list prunes only itself. Pinned by Task 2 (`viewed_marks_are_kept_per_scope_and_list`) and the integration test.
4. **A workspace with many marked files while an agent saves constantly.** Expected: a status refresh costs no `git diff` for an unchanged marked file, and the staged side is only hashed for a mark the unstaged side did not explain. Pinned by Task 1 (`the_staged_side_is_only_read_for_marks_the_unstaged_side_did_not_explain`) and Task 3 (`viewed_hashes_are_cached_until_the_file_changes`).
5. **Several repositories and task worktrees.** Expected: `fe/x.txt` marked in repo `fe` is not marked in repo `be`; a task worktree has its own marks and deleting the task deletes them. Pinned by Task 2 (scope + `delete_task` test) and the integration test (`repo: Some("fe")` / `Some("be")`, paths come back project-relative).

## Design

Verified in the code first (2026-10-19):

- **Against-base mode exists** (`GitMode::Base`, `Section::Base`, `Group::Base`) and so does the task / turn "since" list (`Section::Since`, `Group::Since`, `GitView::since`): marks must work in both. Staged / Unstaged / Untracked are the Working groups; the History (`Group::Commit`) is never marked.
- **The remembered diff mode lives in `settings.toml` `[ui] diff_mode`** (`mkrew_core::settings::write_ui_value`, `crates/mkrew-core/src/settings.rs:338`, called from `crates/mkrew-ui/src/shell/workspace_actions.rs:428`). That store is unsuitable for marks, so they go to daemon SQLite: it writes one scalar of a hand-edited, commented config file (no lists, no per-workspace data); every write re-triggers the hot reload of the settings in the UI (`shell/connection.rs:73`) and in the daemon's settings watcher; and the number of marks is unbounded.
- **Free key:** §13 and `keymap.rs` bind in the Git context only `b h n p s u c d` (+ `j k` / arrows in `git_view::key`): `v` is free.
- The UI has no per-file diff hash and reading every diff to get one is out of the question (§12B: diffs are computed on demand, one per file). So the daemon owns hashing, through the diff cache it already has.

Decisions and defaults (change only with a reason):

- **Mark key** = (repo, path, hash of that file's diff text) plus the *list* it was made in (`working` = Staged + Unstaged + Untracked, `base`, or `since:<checkpoint>`) and the task worktree. The list keeps one list's pruning from deleting another's marks (Review Focus 3). The hash is FNV-1a 64 of the text `git diff` prints (hand-written: stable across Rust versions, no dependency). Hashing git's text, not the parsed `FileDiff`, makes binary and mode-only changes count.
- **Staged vs unstaged:** a `working` mark stands on whichever side's diff hashes the same, so staging a whole viewed file keeps it viewed; the answer says which side (`ViewedFile.staged`), the UI shows the Staged and the other groups separately.
- **Prune** = the check that runs with every `git.viewed`: a mark whose hash no longer equals the file's current diff hash is deleted (this covers "the file no longer changes": an empty diff has no hash). Marks of lists that are not shown are untouched. Rows of old `since:` lists of finished tasks or turns stay (a few bytes each; `delete_task` removes its worktree's marks). `// ponytail:` upgrade path = a retention pass.
- **UI:** a box on every file row (not in the History), dimmed row (`opacity::VIEWED`, a new token), the selected viewed file's diff collapses to "Marked as viewed." with a **Show diff** button, a "Viewed" toolbar button, the key `v` (`Command::GitToggleViewed`, Git context, palette "Git: Mark file as viewed / not viewed"), and `Viewed N/M` in the side-panel header (N viewed of M = every file of the shown list, ignoring "Changed by" and the file filter). Marking does not move the selection.
- **The click races the diff:** the daemon hashes the diff as it is when the request arrives; if the agent edited the file in the ~300 ms between the screen and the click, the mark is for the newer text. Accepted; stated in ADR 0065.
- **Optimistic toggle:** the UI flips the mark at once, sends `git.set_viewed`, then always re-asks `git.viewed` (the daemon has the truth, a failure just logs a warning). Answers to older asks are dropped by a serial.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `docs/decisions/0065-git-viewed-files.md` (new) | The decision. |
| `docs/SPEC.md` | §5, §7, §12B, §13, protocol note. |
| `crates/mkrew-protocol/src/files.rs`, `message.rs`, `lib.rs` | `GitSetViewedParams`, `ViewedFile`, `git.viewed`, `git.set_viewed`, protocol 45. |
| `crates/mkrew-core/src/viewed.rs` (new) | Pure rules: `text_hash`, `list_key`, `Mark`, `Standing`, `resolve` (prune). |
| `crates/mkrew-store/migrations/0021_viewed_files.sql` (new), `src/lib.rs` | Table `viewed_file` + `ViewedScope`, `viewed_marks`, `add_viewed`, `remove_viewed`, `delete_task` cleanup. |
| `crates/mkrew-daemon/src/git_view.rs`, `server/git.rs`, `server.rs` | Diff key refactor, hash cache, `viewed()`, the two handlers. |
| `crates/mkrew-ui/src/git_viewed.rs` (new) | Pure `ViewedSet` (`contains`, `set`, `count`). |
| `crates/mkrew-ui/src/git_view.rs` | `GitView` state: `viewed`, toggle, collapse, counter, serial. |
| `crates/mkrew-ui/src/keymap.rs`, `palette.rs`, `shell/sessions.rs` | `Command::GitToggleViewed`. |
| `crates/mkrew-ui/src/shell/git_viewed.rs` (new), `git.rs`, `git_rows.rs`, `git_render.rs`, `git_diff.rs`, `theme.rs`, `main.rs` | Requests, row box, counter line, toolbar button, collapsed diff. |
| `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md` | Manual checks, next-session note. |

---

### Task 1: Pure rules in `mkrew-core`: hash, list key, prune

**Files:**
- Create: `crates/mkrew-core/src/viewed.rs`
- Modify: `crates/mkrew-core/src/lib.rs` (add `pub mod viewed;` after `pub mod usage;`, :25)

**Interfaces:**
- Consumes: `mkrew_protocol::GitMode` (already exists).
- Produces:
  - `pub fn text_hash(diff_text: &str) -> Option<String>` (16 hex digits; `None` for empty text).
  - `pub fn list_key(mode: GitMode, since: Option<&str>) -> String` (`"working"`, `"base"`, `"since:<checkpoint>"`).
  - `pub struct Mark { pub path: String, pub hash: String }` (`Debug, Clone, PartialEq, Eq`).
  - `pub struct Resolved { pub viewed: Vec<Standing>, pub stale: Vec<Mark> }`; `pub struct Standing { pub path: String, pub staged: bool }` (`Debug, Clone, PartialEq, Eq`; the daemon turns it into the protocol's `ViewedFile`)` (`Debug, Default, PartialEq, Eq`).
  - `pub fn resolve<E>(marks: Vec<Mark>, sides: &[bool], now: impl FnMut(&str, bool) -> Result<Option<String>, E>) -> Result<Resolved, E>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-core/src/viewed.rs` with only the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn mark(path: &str, hash: &str) -> Mark {
        Mark {
            path: path.into(),
            hash: hash.into(),
        }
    }

    fn file(path: &str, staged: bool) -> Standing {
        Standing {
            path: path.into(),
            staged,
        }
    }

    /// `now` answering from a table of (path, staged, hash).
    fn table<'a>(
        rows: &'a [(&'a str, bool, Option<&'a str>)],
    ) -> impl FnMut(&str, bool) -> Result<Option<String>, String> + 'a {
        move |path, staged| {
            Ok(rows
                .iter()
                .find(|(p, s, _)| *p == path && *s == staged)
                .and_then(|(_, _, hash)| hash.map(str::to_owned)))
        }
    }

    #[test]
    fn text_hash_is_fnv1a_64_and_none_for_nothing() {
        // The published FNV-1a 64 test vectors.
        assert_eq!(text_hash("a").as_deref(), Some("af63dc4c8601ec8c"));
        assert_eq!(text_hash("foobar").as_deref(), Some("85944171f73967e8"));
        // Any byte changes it; an empty diff has no hash (nothing to mark).
        assert_ne!(text_hash("+x\n"), text_hash("+y\n"));
        assert_eq!(text_hash(""), None);
    }

    #[test]
    fn list_keys_tell_the_lists_apart() {
        assert_eq!(list_key(GitMode::Working, None), "working");
        // `since` is Against-base only; Working ignores it, like `git.status`.
        assert_eq!(list_key(GitMode::Working, Some("abc")), "working");
        assert_eq!(list_key(GitMode::Base, None), "base");
        assert_eq!(list_key(GitMode::Base, Some("abc")), "since:abc");
    }

    #[test]
    fn a_mark_stands_while_its_hash_is_a_current_hash_and_goes_stale_otherwise() {
        let now = [
            ("a.rs", false, Some("h1")),
            ("a.rs", true, Some("h2")),
            ("b.rs", false, Some("new")),
            ("c.rs", false, None),
        ];
        let marks = vec![
            mark("a.rs", "h1"),
            mark("a.rs", "h2"),
            mark("b.rs", "old"),
            mark("c.rs", "h3"),
            mark("gone.rs", "h4"),
        ];
        let resolved = resolve(marks, &[false, true], table(&now)).unwrap();
        // a.rs is marked on both its sides; b.rs was edited, c.rs no longer changes, gone.rs
        // is not listed at all.
        assert_eq!(
            resolved.viewed,
            vec![file("a.rs", false), file("a.rs", true)]
        );
        assert_eq!(
            resolved.stale,
            vec![mark("b.rs", "old"), mark("c.rs", "h3"), mark("gone.rs", "h4")]
        );
    }

    #[test]
    fn staging_a_viewed_file_moves_its_mark_to_the_staged_side() {
        // Marked unstaged with hash h1; now the unstaged diff is empty and the index holds h1.
        let now = [("a.rs", false, None), ("a.rs", true, Some("h1"))];
        let resolved = resolve(vec![mark("a.rs", "h1")], &[false, true], table(&now)).unwrap();
        assert_eq!(resolved.viewed, vec![file("a.rs", true)]);
        assert!(resolved.stale.is_empty());
    }

    #[test]
    fn the_staged_side_is_only_read_for_marks_the_unstaged_side_did_not_explain() {
        let mut asked = Vec::new();
        let resolved = resolve(vec![mark("a.rs", "h1")], &[false, true], |path, staged| {
            asked.push((path.to_owned(), staged));
            Ok::<_, ()>(Some("h1".to_owned()))
        })
        .unwrap();
        assert_eq!(resolved.viewed, vec![file("a.rs", false)]);
        assert_eq!(asked, [("a.rs".to_owned(), false)]);
        // One list side only (Against base): the staged side is never read.
        let mut asked = Vec::new();
        resolve(vec![mark("a.rs", "zzz")], &[false], |path, staged| {
            asked.push((path.to_owned(), staged));
            Ok::<_, ()>(None)
        })
        .unwrap();
        assert_eq!(asked, [("a.rs".to_owned(), false)]);
    }

    #[test]
    fn a_failing_read_fails_the_whole_check_so_nothing_is_deleted() {
        let result = resolve(
            vec![mark("a.rs", "h1"), mark("b.rs", "h2")],
            &[false],
            |path, _| {
                if path == "b.rs" {
                    Err("git failed")
                } else {
                    Ok(None)
                }
            },
        );
        assert_eq!(result, Err("git failed"));
    }
}
```

Register the module: in `crates/mkrew-core/src/lib.rs` add `pub mod viewed;` after `pub mod usage;`.

- [ ] **Step 2: Run to verify the tests fail**

Run: `cargo test -p mkrew-core viewed -j 4`
Expected: FAIL to compile (`text_hash`, `Mark`, `resolve` not found).

- [ ] **Step 3: Write the implementation** (above `mod tests` in `viewed.rs`)

```rust
//! "Mark as viewed" in the Git view (SPEC §12B, ADR 0065): the pure rules. The daemon hashes
//! the text of a file's diff when the user marks it; the mark stands only while the file's
//! diff still hashes the same. The storage is `mkrew-store`, the git reads are the daemon's.

use std::collections::BTreeMap;

use mkrew_protocol::GitMode;

/// One stored mark: the file (relative to its repository) and the hash of its diff text when
/// it was marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub path: String,
    pub hash: String,
}

/// A mark that still stands, on the staged (index) side or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub path: String,
    pub staged: bool,
}

/// What [`resolve`] found: the files whose mark stands (project-relative once the caller
/// prefixes the repository) and the marks to delete.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    pub viewed: Vec<Standing>,
    pub stale: Vec<Mark>,
}

/// FNV-1a 64 of the text `git diff` printed for a file, as 16 hex digits; `None` for an empty
/// diff (nothing to mark). Hand-written so the stored value never changes with a Rust or
/// dependency upgrade.
pub fn text_hash(diff_text: &str) -> Option<String> {
    if diff_text.is_empty() {
        return None;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in diff_text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    Some(format!("{hash:016x}"))
}

/// The list a mark belongs to: `working` (Staged, Unstaged and Untracked share it), `base`
/// (Against base), or `since:<checkpoint>` (a task's or turn's changes). Marks of one list are
/// never pruned by looking at another.
pub fn list_key(mode: GitMode, since: Option<&str>) -> String {
    match (mode, since) {
        (GitMode::Working, _) => "working".to_owned(),
        (GitMode::Base, None) => "base".to_owned(),
        (GitMode::Base, Some(since)) => format!("since:{since}"),
    }
}

/// Which `marks` of one list still stand. `sides` are the diffs a file can have in that list
/// (`[false, true]` = unstaged then staged for Working changes, `[false]` otherwise) and
/// `now(path, staged)` the hash of that side's diff text now (`None` = no diff). A mark stands
/// on every side that hashes the same (so staging a viewed file keeps it viewed); a mark no
/// side explains is stale, which also covers a file that no longer changes. Sides are read in
/// order and only while a mark of the path is still unexplained. A failing `now` fails the
/// whole check: nothing may be deleted on a git error.
pub fn resolve<E>(
    marks: Vec<Mark>,
    sides: &[bool],
    mut now: impl FnMut(&str, bool) -> Result<Option<String>, E>,
) -> Result<Resolved, E> {
    let mut by_path: BTreeMap<String, Vec<Mark>> = BTreeMap::new();
    for mark in marks {
        by_path.entry(mark.path.clone()).or_default().push(mark);
    }
    let mut resolved = Resolved::default();
    for (path, mut pending) in by_path {
        for &staged in sides {
            if pending.is_empty() {
                break;
            }
            let hash = now(&path, staged)?;
            pending.retain(|mark| {
                let stands = hash.as_deref() == Some(mark.hash.as_str());
                if stands {
                    resolved.viewed.push(Standing {
                        path: path.clone(),
                        staged,
                    });
                }
                !stands
            });
        }
        resolved.stale.extend(pending);
    }
    Ok(resolved)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo fmt --all && cargo test -p mkrew-core viewed -j 4`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-core/src/viewed.rs crates/mkrew-core/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(core): viewed-file hash, list key and prune rules (ADR 0065)"
```

---

### Task 2: Store: migration 0021 and the mark queries

**Files:**
- Create: `crates/mkrew-store/migrations/0021_viewed_files.sql`
- Modify: `crates/mkrew-store/src/lib.rs` (`MIGRATIONS` :17-37; new type before `pub struct Store` :94; new methods after `forget_turn_trees` :1066-1072; `delete_task` :700-722)
- Test: `crates/mkrew-store/src/tests.rs`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces (`mkrew_store`):
  - `pub struct ViewedScope<'a> { pub project_id: Ulid, pub worktree_task_id: Option<Ulid>, pub repo: &'a str, pub list: &'a str }` (`Debug, Clone, Copy`).
  - `Store::viewed_marks(&self, scope: &ViewedScope<'_>) -> Result<Vec<(String, String)>>` — `(path, hash)` ordered by path then hash.
  - `Store::add_viewed(&self, scope: &ViewedScope<'_>, path: &str, hash: &str) -> Result<()>`.
  - `Store::remove_viewed(&self, scope: &ViewedScope<'_>, path: &str, hash: Option<&str>) -> Result<()>` — `None` removes every hash of the path.

- [ ] **Step 1: Write the failing test**

Append to `crates/mkrew-store/src/tests.rs`:

```rust
#[test]
fn viewed_marks_are_kept_per_scope_and_list() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let project = store.add_project(dir.path(), None).unwrap();
    let other_dir = tempfile::tempdir().unwrap();
    let other = store.add_project(other_dir.path(), None).unwrap();
    let task = Ulid::new();
    let scope = |repo: &'static str, list: &'static str| ViewedScope {
        project_id: project.id,
        worktree_task_id: None,
        repo,
        list,
    };
    let marks = |scope: &ViewedScope<'_>| store.viewed_marks(scope).unwrap();
    let pair = |path: &str, hash: &str| (path.to_owned(), hash.to_owned());

    // Two sides of one file (staged / unstaged texts) are two rows; marking again replaces.
    store.add_viewed(&scope("", "working"), "a.rs", "h1").unwrap();
    store.add_viewed(&scope("", "working"), "a.rs", "h1").unwrap();
    store.add_viewed(&scope("", "working"), "a.rs", "h2").unwrap();
    store.add_viewed(&scope("", "working"), "b.rs", "h3").unwrap();
    assert_eq!(
        marks(&scope("", "working")),
        [pair("a.rs", "h1"), pair("a.rs", "h2"), pair("b.rs", "h3")]
    );

    // Another list, repository, task worktree and project are separate.
    store.add_viewed(&scope("", "base"), "a.rs", "h9").unwrap();
    store.add_viewed(&scope("fe", "working"), "a.rs", "h8").unwrap();
    let in_task = ViewedScope {
        worktree_task_id: Some(task),
        ..scope("", "working")
    };
    store.add_viewed(&in_task, "a.rs", "h7").unwrap();
    let elsewhere = ViewedScope {
        project_id: other.id,
        ..scope("", "working")
    };
    store.add_viewed(&elsewhere, "a.rs", "h6").unwrap();
    assert_eq!(marks(&scope("", "base")), [pair("a.rs", "h9")]);
    assert_eq!(marks(&scope("fe", "working")), [pair("a.rs", "h8")]);
    assert_eq!(marks(&in_task), [pair("a.rs", "h7")]);
    assert_eq!(marks(&elsewhere), [pair("a.rs", "h6")]);
    assert_eq!(marks(&scope("", "working")).len(), 3);

    // One hash, or every hash of the path.
    store.remove_viewed(&scope("", "working"), "a.rs", Some("h1")).unwrap();
    assert_eq!(
        marks(&scope("", "working")),
        [pair("a.rs", "h2"), pair("b.rs", "h3")]
    );
    store.remove_viewed(&scope("", "working"), "a.rs", None).unwrap();
    assert_eq!(marks(&scope("", "working")), [pair("b.rs", "h3")]);
    assert_eq!(marks(&scope("", "base")).len(), 1);
}

#[test]
fn deleting_a_task_deletes_the_marks_of_its_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let project = store.add_project(dir.path(), None).unwrap();
    let task = store
        .create_task(project.id, "t", "", None, None, false)
        .unwrap();
    let scope = |worktree_task_id| ViewedScope {
        project_id: project.id,
        worktree_task_id,
        repo: "",
        list: "working",
    };
    store.add_viewed(&scope(Some(task.id)), "a.rs", "h1").unwrap();
    store.add_viewed(&scope(None), "a.rs", "h2").unwrap();
    store.delete_task(task.id).unwrap();
    assert_eq!(store.viewed_marks(&scope(Some(task.id))).unwrap(), []);
    assert_eq!(store.viewed_marks(&scope(None)).unwrap().len(), 1);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-store viewed -j 4`
Expected: FAIL to compile (`ViewedScope` not found).

- [ ] **Step 3: Write the migration**

`crates/mkrew-store/migrations/0021_viewed_files.sql`:

```sql
-- SPEC §5, §12B, ADR 0065. Never edit: schema changes go in a new numbered migration.
CREATE TABLE viewed_file (
  project_id       TEXT NOT NULL REFERENCES project(id),
  worktree_task_id TEXT NOT NULL DEFAULT '', -- '' = the project folder, else the task whose worktree is shown
  repo             TEXT NOT NULL DEFAULT '', -- '' = the workspace folder's own repository, else a child repository folder
  list             TEXT NOT NULL,            -- 'working' | 'base' | 'since:<checkpoint>'
  path             TEXT NOT NULL,            -- relative to the repository
  hash             TEXT NOT NULL,            -- FNV-1a 64 of the file's diff text when it was marked
  marked_at        INTEGER NOT NULL,         -- Unix ms
  PRIMARY KEY (project_id, worktree_task_id, repo, list, path, hash)
);
```

- [ ] **Step 4: Implement**

In `crates/mkrew-store/src/lib.rs`:

1. `MIGRATIONS`: after the `0019_session_position.sql` line add `    include_str!("../migrations/0021_viewed_files.sql"),` (keep it the last entry; if the tray plan landed `0020`, that line sits just above it).
2. Before `pub struct Store {` add:

```rust
/// Whose viewed marks (ADR 0065): one repository's list of a project folder or of a task's
/// worktree.
#[derive(Debug, Clone, Copy)]
pub struct ViewedScope<'a> {
    pub project_id: Ulid,
    pub worktree_task_id: Option<Ulid>,
    /// `""` = the workspace folder's own repository.
    pub repo: &'a str,
    /// `working`, `base` or `since:<checkpoint>` (`mkrew_core::viewed::list_key`).
    pub list: &'a str,
}
```

3. After `forget_turn_trees` (ends :1072) add:

```rust
    /// The viewed marks of `scope` as `(path, hash)`, in path order (ADR 0065).
    pub fn viewed_marks(&self, scope: &ViewedScope<'_>) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, hash FROM viewed_file
             WHERE project_id = ?1 AND worktree_task_id = ?2 AND repo = ?3 AND list = ?4
             ORDER BY path, hash",
        )?;
        let rows = stmt.query_map(
            params![
                scope.project_id.to_string(),
                scope
                    .worktree_task_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                scope.repo,
                scope.list
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Marks `path` viewed with the diff `hash` it has now; marking the same text again just
    /// refreshes the row.
    pub fn add_viewed(&self, scope: &ViewedScope<'_>, path: &str, hash: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO viewed_file
               (project_id, worktree_task_id, repo, list, path, hash, marked_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                scope.project_id.to_string(),
                scope
                    .worktree_task_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                scope.repo,
                scope.list,
                path,
                hash,
                now_ms()
            ],
        )?;
        Ok(())
    }

    /// Clears the mark of `path` with `hash`, or (`None`) every mark of the path.
    pub fn remove_viewed(
        &self,
        scope: &ViewedScope<'_>,
        path: &str,
        hash: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "DELETE FROM viewed_file
             WHERE project_id = ?1 AND worktree_task_id = ?2 AND repo = ?3 AND list = ?4
               AND path = ?5 AND (?6 IS NULL OR hash = ?6)",
            params![
                scope.project_id.to_string(),
                scope
                    .worktree_task_id
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                scope.repo,
                scope.list,
                path,
                hash
            ],
        )?;
        Ok(())
    }
```

4. In `delete_task` (:715-719) add one statement before `tx.execute("DELETE FROM task WHERE id = ?1", ...)`:

```rust
        tx.execute(
            "DELETE FROM viewed_file WHERE worktree_task_id = ?1",
            [id.to_string()],
        )?;
```

- [ ] **Step 5: Run the store tests**

Run: `cargo fmt --all && cargo test -p mkrew-store -j 4`
Expected: PASS (including `open_runs_migrations_and_enables_wal`, which compares against `MIGRATIONS.len()`).

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-store/migrations/0021_viewed_files.sql crates/mkrew-store/src/lib.rs crates/mkrew-store/src/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(store): viewed_file table and mark queries (migration 0021, ADR 0065)"
```

---

### Task 3: Protocol 45, daemon handlers, ADR 0065 and SPEC

**Files:**
- Create: `docs/decisions/0065-git-viewed-files.md`
- Modify: `docs/SPEC.md` (§5 near line 317, §7 table after line 567, protocol notes after line 632, §12B after line 1146, §13 table after line 1249)
- Modify: `crates/mkrew-protocol/src/files.rs` (new types after `FileDiff`, :146-158; tests :567-745)
- Modify: `crates/mkrew-protocol/src/message.rs` (import :21-23; variants after `GitDiff` :196-198)
- Modify: `crates/mkrew-protocol/src/lib.rs:16`
- Modify: `crates/mkrew-daemon/src/git_view.rs` (`GitCache` :42-47, consts :19-23, `diff` :216-287, new fns)
- Modify: `crates/mkrew-daemon/src/server/git.rs` (imports :6-18; two handlers after `git_diff` :73-120)
- Modify: `crates/mkrew-daemon/src/server.rs` (dispatch after :415)
- Test: `crates/mkrew-daemon/src/git_view/tests.rs`, `crates/mkrew-daemon/tests/daemon/git.rs`, `crates/mkrew-daemon/tests/daemon/main.rs` (import list :14-39)

**Interfaces:**
- Consumes: `GitDiffParams`, `GitStatusParams`, `GitMode` (`files.rs`); Task 1 `mkrew_core::viewed::{Mark, Resolved, Standing, resolve, text_hash, list_key}`; Task 2 `mkrew_store::ViewedScope` and `Store::{viewed_marks, add_viewed, remove_viewed}`.
- Produces: `GitSetViewedParams { diff: GitDiffParams (flattened), viewed: bool }`, `ViewedFile { path: String, staged: bool }`, `Request::GitViewed(GitStatusParams)` = `git.viewed` → `ViewedFile[]`, `Request::GitSetViewed(GitSetViewedParams)` = `git.set_viewed` → `null`; `PROTOCOL_VERSION` = 45; `mkrew_daemon::git_view::{diff_hash, viewed}`:
  - `pub fn diff_hash(git: &dyn GitBackend, cache: &GitCache, dir: &Path, configured: Option<&str>, params: &GitDiffParams, path: &str, snapshot: &dyn Fn() -> io::Result<String>) -> Result<Option<String>, ViewError>`
  - `pub fn viewed(git: &dyn GitBackend, cache: &GitCache, dir: &Path, configured: Option<&str>, list: (GitMode, Option<&str>), marks: Vec<Mark>, snapshot: &dyn Fn() -> io::Result<String>) -> Result<Resolved, ViewError>`
  - `diff()` keeps its signature and behavior.

- [ ] **Step 1: Write the ADR**

Create `docs/decisions/0065-git-viewed-files.md`:

```markdown
# 0065 — Git view: mark files as viewed

- Date: 2026-10-19
- Status: accepted (extends SPEC §12B, §13, §7, §5)
- Milestone: none (competitor study: GitHub / Conductor "viewed")

## Context

Reviewing a large agent diff file by file loses track of what was read. GitHub and Conductor
let the reviewer tick a file as viewed; the tick must not outlive a new change to the file.
The Git view lists files from `git.status` and computes each diff on demand (SPEC §12B), so the
UI cannot tell by itself that a file's diff changed. The remembered diff mode lives in
`settings.toml` `[ui]`; that file is a hand-edited config written one scalar at a time, every
write reloads settings in the UI and the daemon, and marks are an unbounded per-workspace list.

## Decision

- A mark is a row `viewed_file(project_id, worktree_task_id, repo, list, path, hash, marked_at)`
  in the daemon's SQLite (migration 0021). `list` = `working` (Staged, Unstaged, Untracked),
  `base`, or `since:<checkpoint>`; `worktree_task_id` and `repo` are `''` for the project
  folder / the workspace's own repository. `hash` = FNV-1a 64 of the text `git diff` prints for
  the file when it was marked (binary changes count: the text holds the blob ids).
- `git.set_viewed { <git.diff params, commit must be none>, viewed }` hashes the diff as it is
  now and stores or removes the mark (`invalid_state` when the file has no diff).
- `git.viewed` (the params of `git.status`) re-hashes the marked files of that list through the
  diff cache and returns `ViewedFile { path, staged }` for each mark that still stands. Marks
  whose hash no longer matches (the diff changed, or the file no longer changes) are deleted
  there. A `working` mark stands on the staged or the unstaged side, whichever hashes the same:
  staging a whole viewed file keeps it viewed. Protocol 45.
- UI: a box on each file row, `v` in the Git view (SPEC §13), a toolbar button, a palette
  command; viewed rows dim, the selected viewed file's diff collapses ("Show diff" opens it),
  the header shows `Viewed N/M`. Not in the History.

## Alternatives

- `settings.toml` `[ui]`, like `diff_mode`: see Context. Rejected.
- The UI hashing diffs it already holds: only the selected file's diff is loaded, so a mark
  could not clear until the file is opened. Rejected.
- Keying marks by the file's mtime / size or `added` / `removed` counts: an edit that keeps
  the counts would keep the mark. Rejected for the diff text.
- No list in the key (a mark valid in any list whose diff hashes the same): pruning in
  Working changes would delete marks made in Against base. Rejected.

## Consequences

- The daemon hashes the diff as it is when the request arrives: an agent edit between the
  screen and the click marks the newer text.
- Each `git.viewed` costs one `git diff` per marked file whose file or index changed (cached
  by the existing diff key, so none otherwise); a refresh with many marks after a big change
  is slower.
- Marks of finished tasks' `since:` lists stay until `delete_task` or never; a retention pass
  can drop them if the table ever grows.
- Marks are per machine (the daemon's database), not in the repository.
```

- [ ] **Step 2: Update `docs/SPEC.md`**

Six edits (use `read` on each anchor first; line numbers are today's):

1. §5, after the sentence ending `... (the linked plan `.md`: ... NULL = none; ADR 0039).` (the `0017_session_plan.sql` line, ~317) add:

```markdown
`0021_viewed_files.sql` adds table `viewed_file(project_id, worktree_task_id, repo, list, path, hash, marked_at)`, one row per file the user marked as viewed in the Git view (§12B, ADR 0065).
```

2. §7 table, after the `git.diff` row (line 567) add:

```markdown
| `git.viewed` | the params of `git.status` | `ViewedFile[]` (`path`, `staged`): the files of that list (`working` / `base` / the `since` checkpoint) whose viewed mark still stands (ADR 0065). Re-hashes the marked files through the diff cache and deletes marks whose diff text changed |
| `git.set_viewed` | the params of `git.diff` (no `commit`), `viewed` (bool) | `null`; hashes the file's diff now and stores (`viewed: true`) or removes the mark; `invalid_state` for a file with no diff, `invalid_request` with `commit` |
```

3. Protocol notes, after `Protocol 41: `session.move` and `Session.position`.` add:

```markdown
Protocol 45 (ADR 0065): `git.viewed`, `git.set_viewed`.
```

4. §12B, after the **Diff modes** bullet (ends `... remembered as `[ui] diff_mode` (§20.0).`, line 1146) add:

```markdown
- **Viewed files** (ADR 0065): each file row has a box (not in the History), the diff toolbar a
  "Viewed" button, key `v` (§13) and the palette "Git: Mark file as viewed / not viewed". A viewed
  row is dimmed and the selected viewed file's diff collapses to "Marked as viewed." with a
  **Show diff** button. The side panel header shows `Viewed N/M` (M = every file of the shown
  list, whatever "Changed by" and the file filter hide). The daemon keeps each mark in SQLite
  with a hash of the file's diff text; it clears by itself as soon as that text changes or the
  file stops changing (checked with every status refresh). Staging a whole viewed file keeps the
  mark; marks of Against base, a task's changes and Working changes are separate lists.
```

5. §13 table, after `| Git | Toggle Unified / Split diff | d |` add:

```markdown
| Git | Mark the selected file viewed / not viewed | v |
```

6. §7, the paragraph under the table that lists the requests accepting `repo?` (line 579, starts ``git.status`, `git.diff`, `git.stage`/`git.unstage`, …``): add `` `git.viewed`, `git.set_viewed` `` to that list, so it reads `` … `git.fetch`/`git.push`, `git.viewed`, `git.set_viewed` and `ai.generate` accept `repo?`: …``.

- [ ] **Step 3: Write the failing protocol tests**

In `crates/mkrew-protocol/src/files.rs` `mod tests`, in `every_files_and_git_request_round_trips` insert after the `Request::GitDiff(...)` entry (:580-589):

```rust
            Request::GitViewed(GitStatusParams {
                project_id,
                worktree_task_id: wt,
                repo: Some("fe".into()),
                mode: GitMode::Working,
                since: None,
            }),
            Request::GitSetViewed(GitSetViewedParams {
                diff: GitDiffParams {
                    project_id,
                    worktree_task_id: wt,
                    repo: None,
                    path: "src/a.rs".into(),
                    mode: GitMode::Base,
                    staged: false,
                    commit: None,
                    since: Some("abc".into()),
                },
                viewed: true,
            }),
```

and in `methods` insert `"git.viewed", "git.set_viewed",` right after `"git.diff",`. Then add a test after `optional_params_default_when_missing`:

```rust
    #[test]
    fn set_viewed_flattens_the_diff_params_and_a_viewed_file_defaults_to_the_unstaged_side() {
        let id = sid().to_string();
        let set: GitSetViewedParams =
            serde_json::from_value(json!({"project_id": id, "path": "a", "viewed": true})).unwrap();
        assert_eq!(
            (set.diff.path.as_str(), set.diff.mode, set.diff.staged, set.viewed),
            ("a", GitMode::Working, false, true)
        );
        assert_eq!(serde_json::to_value(&set).unwrap()["path"], "a");
        let file: ViewedFile = serde_json::from_value(json!({"path": "a"})).unwrap();
        assert!(!file.staged);
    }
```

- [ ] **Step 4: Run them to see them fail**

Run: `cargo test -p mkrew-protocol viewed -j 4`
Expected: FAIL to compile (`GitSetViewedParams`, `Request::GitViewed` not found).

- [ ] **Step 5: Implement the protocol**

`files.rs`, after `pub struct FileDiff { ... }` (ends :158):

```rust
/// `git.set_viewed` (ADR 0065): marks the file of `diff` as viewed, or clears the mark, for the
/// diff `diff` describes (`mode`, `staged`, `since` as in `git.diff`; `commit` must be `None`:
/// a commit's files are never marked). The daemon hashes that diff's text as it is now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitSetViewedParams {
    #[serde(flatten)]
    pub diff: GitDiffParams,
    pub viewed: bool,
}

/// One file of a `git.viewed` answer whose mark still stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewedFile {
    /// Project-relative, like [`ChangedFile::path`].
    pub path: String,
    /// `true` = the staged (index) side of Working changes; `false` = every other list (the
    /// unstaged side, untracked, against base, since a checkpoint), as in `git.diff`.
    #[serde(default)]
    pub staged: bool,
}
```

`message.rs`: add `GitSetViewedParams` to the `use crate::files::{...}` list (:21-23, after `GitSetBaseParams`), and after the `GitDiff(GitDiffParams),` variant (:198) add:

```rust
    /// Returns `ViewedFile[]`: the files of the listed changes whose viewed mark still stands;
    /// marks that no longer match are deleted (ADR 0065). Same params as `git.status`.
    #[serde(rename = "git.viewed")]
    GitViewed(GitStatusParams),
    /// Returns nothing (`null`): marks the file as viewed (or clears the mark) for the diff the
    /// params describe (ADR 0065).
    #[serde(rename = "git.set_viewed")]
    GitSetViewed(GitSetViewedParams),
```

`lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 45;`

- [ ] **Step 6: Run the protocol tests**

Run: `cargo fmt --all && cargo test -p mkrew-protocol -j 4`
Expected: PASS. (The daemon's exhaustive `match` on `Request` is completed in Step 10 of this same task; do not build the daemon before them.)

- [ ] **Step 7: Write the failing daemon unit tests**

Append to `crates/mkrew-daemon/src/git_view/tests.rs` (add `use mkrew_core::viewed::{Mark, Resolved, Standing};` to the imports at the top):

```rust
fn viewed_in(
    cache: &GitCache,
    dir: &Path,
    list: (GitMode, Option<&str>),
    marks: &[&Mark],
) -> Resolved {
    let marks = marks.iter().map(|m| (*m).clone()).collect();
    super::viewed(&GitCli, cache, dir, None, list, marks, &unused).unwrap()
}

/// The mark `git.set_viewed` stores for `path` on `side` of `list` as it is now.
fn mark_now(
    cache: &GitCache,
    dir: &Path,
    path: &str,
    mode: GitMode,
    staged: bool,
    since: Option<&str>,
) -> Mark {
    let mut p = params(mode, staged, None);
    p.since = since.map(str::to_owned);
    let hash = diff_hash(&GitCli, cache, dir, None, &p, path, &unused)
        .unwrap()
        .unwrap_or_else(|| panic!("{path} has no diff"));
    Mark {
        path: path.into(),
        hash,
    }
}

fn file(path: &str, staged: bool) -> Standing {
    Standing {
        path: path.into(),
        staged,
    }
}

#[test]
fn a_mark_stands_until_its_diff_text_changes() {
    let (_repo, dir) = repo();
    std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    let cache = GitCache::default();
    let working = (GitMode::Working, None);
    let mark = mark_now(&cache, &dir, "a.txt", GitMode::Working, false, None);

    let resolved = viewed_in(&cache, &dir, working, &[&mark]);
    assert_eq!(resolved.viewed, [file("a.txt", false)]);
    assert!(resolved.stale.is_empty());

    // The agent edits it again: stale.
    std::fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    let resolved = viewed_in(&cache, &dir, working, &[&mark]);
    assert_eq!(resolved.viewed, []);
    assert_eq!(resolved.stale, [mark.clone()]);

    // The same text again stands again (the caller deletes stale rows, so a deleted mark is
    // gone for good; this pins that the hash is the text and nothing else).
    std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    assert_eq!(
        viewed_in(&cache, &dir, working, &[&mark]).viewed,
        [file("a.txt", false)]
    );

    // Reverted to HEAD: the file no longer changes, the mark is stale.
    std::fs::write(dir.join("a.txt"), "one\ntwo\n").unwrap();
    assert_eq!(viewed_in(&cache, &dir, working, &[&mark]).stale, [mark]);
}

#[test]
fn staging_a_viewed_file_keeps_it_viewed() {
    let (_repo, dir) = repo();
    std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    let cache = GitCache::default();
    let working = (GitMode::Working, None);
    let mark = mark_now(&cache, &dir, "a.txt", GitMode::Working, false, None);

    git(&dir, &["add", "a.txt"]);
    let resolved = viewed_in(&cache, &dir, working, &[&mark]);
    assert_eq!(resolved.viewed, [file("a.txt", true)]);
    assert!(resolved.stale.is_empty());

    // A further edit shows as unstaged: that diff was never viewed, the staged one still is.
    std::fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    let resolved = viewed_in(&cache, &dir, working, &[&mark]);
    assert_eq!(resolved.viewed, [file("a.txt", true)]);
}

#[test]
fn binary_and_untracked_files_can_be_marked() {
    let (_repo, dir) = repo();
    std::fs::write(dir.join("img.bin"), [0u8, 1, 2, 3]).unwrap();
    git(&dir, &["add", "img.bin"]);
    git(&dir, &["commit", "-q", "-m", "bin"]);
    std::fs::write(dir.join("img.bin"), [0u8, 1, 2, 4]).unwrap();
    std::fs::write(dir.join("new.txt"), "n1\n").unwrap();
    let cache = GitCache::default();
    let working = (GitMode::Working, None);
    let binary = mark_now(&cache, &dir, "img.bin", GitMode::Working, false, None);
    let untracked = mark_now(&cache, &dir, "new.txt", GitMode::Working, false, None);
    let both = [&binary, &untracked];
    assert_eq!(
        viewed_in(&cache, &dir, working, &both).viewed,
        [file("img.bin", false), file("new.txt", false)]
    );

    // Another byte in the binary (a different size, so the cache cannot reuse the entry) and
    // another line in the new file clear both.
    std::fs::write(dir.join("img.bin"), [0u8, 1, 2, 5, 6]).unwrap();
    std::fs::write(dir.join("new.txt"), "n1\nn2\n").unwrap();
    let resolved = viewed_in(&cache, &dir, working, &both);
    assert_eq!(resolved.viewed, []);
    assert_eq!(resolved.stale.len(), 2);

    // A file with no diff has no hash: nothing to mark.
    let p = params(GitMode::Working, false, None);
    assert_eq!(
        diff_hash(&GitCli, &cache, &dir, None, &p, "b.txt", &unused).unwrap(),
        None
    );
}

#[test]
fn marks_follow_the_list_they_are_read_in() {
    let (repo, dir) = repo();
    let checkpoint = git(repo.path(), &["rev-parse", "HEAD"]);
    std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    git(repo.path(), &["commit", "-q", "-am", "task"]);
    std::fs::write(dir.join("b.txt"), "b\nmore\n").unwrap();
    let cache = GitCache::default();

    // Against a checkpoint: a.txt (committed after it) and b.txt (working tree) have diffs.
    let since = (GitMode::Base, Some(checkpoint.as_str()));
    let a = mark_now(&cache, &dir, "a.txt", GitMode::Base, false, Some(checkpoint.as_str()));
    let b = mark_now(&cache, &dir, "b.txt", GitMode::Base, false, Some(checkpoint.as_str()));
    assert_eq!(
        viewed_in(&cache, &dir, since, &[&a, &b]).viewed,
        [file("a.txt", false), file("b.txt", false)]
    );
    // In Working changes only b.txt changes: a.txt's mark is not explained there (the caller
    // only ever asks a list about its own marks).
    assert_eq!(
        viewed_in(&cache, &dir, (GitMode::Working, None), &[&a]).stale,
        [a.clone()]
    );
    // Against base on `main` itself: no merge-base difference, same texts as Working.
    let base = (GitMode::Base, None);
    assert_eq!(
        viewed_in(&cache, &dir, base, &[&b]).viewed,
        [file("b.txt", false)]
    );
}

#[test]
fn viewed_hashes_are_cached_until_the_file_changes() {
    let (_repo, dir) = repo();
    std::fs::write(dir.join("a.txt"), "one\nTWO\n").unwrap();
    let cache = GitCache::default();
    let mark = mark_now(&cache, &dir, "a.txt", GitMode::Working, false, None);
    let working = (GitMode::Working, None);
    assert_eq!(lock(&cache.hashes).len(), 1);
    // Unchanged file: the next checks read only the cache (the staged side is not read: the
    // unstaged side explained the mark).
    for _ in 0..3 {
        viewed_in(&cache, &dir, working, &[&mark]);
    }
    assert_eq!(lock(&cache.hashes).len(), 1);
    // A new text is a new entry.
    std::fs::write(dir.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    viewed_in(&cache, &dir, working, &[&mark]);
    assert!(lock(&cache.hashes).len() >= 2);
}
```

- [ ] **Step 8: Run to verify they fail**

Run: `cargo test -p mkrew-daemon --lib viewed -j 4`
Expected: FAIL to compile (`diff_hash`, `viewed`, `cache.hashes` not found). The crate does not compile as a whole until Step 10 adds the two `Request` arms to the daemon's exhaustive `match`.

- [ ] **Step 9: Refactor `diff` and add the hash path in `git_view.rs`**

Imports: add `use mkrew_core::viewed::{Mark, Resolved, resolve, text_hash};` (module `mkrew_core::viewed`) after the `mkrew_protocol` import.

Consts, after `MAX_CACHED_DIFFS` (:23):

```rust
/// Cached diff hashes kept before the cache starts over (a hash is a few bytes).
const MAX_CACHED_HASHES: usize = 4096;
```

`GitCache` (:42-47) gains the field:

```rust
    /// The hash of each diff's text (`git.viewed`, ADR 0065), keyed like `diffs`; `None` = an
    /// empty diff. Separate from `diffs` so checking many marked files never evicts the diff
    /// on screen.
    hashes: Mutex<HashMap<DiffKey, Option<String>>>,
```

Replace the whole `pub fn diff(...)` (:216-287, from its doc comment to the closing brace) with:

```rust
/// `git.diff` of one file. `path` is already checked to stay inside `dir`. Cached by target,
/// HEAD, the file's mtime/size and the index's mtime. A `since` tree is compared with
/// `snapshot()` of the working state (taken on a cache miss only).
pub fn diff(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    params: &GitDiffParams,
    path: &str,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<FileDiff, ViewError> {
    let key = diff_key(git, cache, dir, configured, params, path)?;
    if let Some(cached) = lock(&cache.diffs).get(&key) {
        return Ok(cached.clone());
    }
    let diff = file_diff(&diff_text(git, dir, &key, snapshot)?, path);
    let mut diffs = lock(&cache.diffs);
    if diffs.len() >= MAX_CACHED_DIFFS {
        diffs.clear();
    }
    diffs.insert(key, diff.clone());
    Ok(diff)
}

/// The hash of the text `git.diff` of `params` would show (ADR 0065): what a viewed mark
/// stores. `None` = the file has no diff. Cached like [`diff`].
pub fn diff_hash(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    params: &GitDiffParams,
    path: &str,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<Option<String>, ViewError> {
    let key = diff_key(git, cache, dir, configured, params, path)?;
    cached_hash(git, cache, dir, key, snapshot)
}

/// `git.viewed`: which of `marks` (one repository's stored marks of `list` = mode and `since`,
/// ADR 0065) still stand, and which are stale. HEAD and the targets are resolved once, and each
/// file's hash comes from the cache unless the file, the index or HEAD changed; Working
/// changes read the staged side only for a mark the unstaged side did not explain.
pub fn viewed(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    list: (GitMode, Option<&str>),
    marks: Vec<Mark>,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<Resolved, ViewError> {
    let repo = cache.repo(git, dir)?;
    let head = git.head(dir)?;
    let (mode, since) = list;
    let unstaged = target_of(git, cache, dir, configured, (mode, false, since), head.as_deref())?;
    let staged = (mode == GitMode::Working)
        .then(|| target_of(git, cache, dir, configured, (mode, true, since), head.as_deref()))
        .transpose()?;
    let sides: &[bool] = if staged.is_some() {
        &[false, true]
    } else {
        &[false]
    };
    resolve(marks, sides, |path, is_staged| {
        let target = match (&staged, is_staged) {
            (Some(staged), true) => staged.clone(),
            _ => unstaged.clone(),
        };
        let key = DiffKey {
            dir: dir.to_owned(),
            path: path.to_owned(),
            target,
            stamp: Some(stamp_of(&repo, dir, head.clone(), path)),
        };
        cached_hash(git, cache, dir, key, snapshot)
    })
}

/// The cache key of the diff `params` asks for (the same for [`diff`] and [`diff_hash`]).
fn diff_key(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    params: &GitDiffParams,
    path: &str,
) -> Result<DiffKey, ViewError> {
    let repo = cache.repo(git, dir)?;
    let (target, stamp) = match &params.commit {
        // Resolved first: a moving name (`HEAD~1`) must not hit a stale entry, and ranges or
        // trees are refused.
        Some(commit) => (Target::Commit(commit_id(git, dir, commit)?), None),
        None => {
            let head = git.head(dir)?;
            let want = (params.mode, params.staged, params.since.as_deref());
            let target = target_of(git, cache, dir, configured, want, head.as_deref())?;
            (target, Some(stamp_of(&repo, dir, head, path)))
        }
    };
    Ok(DiffKey {
        dir: dir.to_owned(),
        path: path.to_owned(),
        target,
        stamp,
    })
}

/// What a working-tree diff of `want` = (mode, staged side, `since`) compares; `head` is the
/// current HEAD.
fn target_of(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    configured: Option<&str>,
    want: (GitMode, bool, Option<&str>),
    head: Option<&str>,
) -> Result<Target, ViewError> {
    Ok(match want {
        (GitMode::Working, false, _) => Target::Unstaged,
        (GitMode::Working, true, _) => Target::Staged,
        (GitMode::Base, _, Some(since)) => since_target(git, dir, since)?,
        (GitMode::Base, _, None) => {
            let head = head.ok_or(ViewError::NoMergeBase)?;
            let (_, base) = resolve_base(git, dir, configured)?.ok_or(ViewError::NoMergeBase)?;
            let merge_base = cache
                .merge_base(git, dir, head, &base)?
                .ok_or(ViewError::NoMergeBase)?;
            Target::Since(merge_base)
        }
    })
}

/// What a cached working-tree diff of `path` depends on besides its target.
fn stamp_of(repo: &Repo, dir: &Path, head: Option<String>, path: &str) -> Stamp {
    Stamp {
        head,
        file: std::fs::symlink_metadata(dir.join(path))
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len()))),
        index: modified(&repo.git_dir.join("index")),
    }
}

/// What git prints for the diff `key` names; an untracked file shows as all-new.
fn diff_text(
    git: &dyn GitBackend,
    dir: &Path,
    key: &DiffKey,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<String, ViewError> {
    let now = match key.target {
        Target::Tree(_) => snapshot()?,
        _ => String::new(),
    };
    let mut text = git.file_diff(dir, &key.path, key.target.side(&now))?;
    if text.is_empty()
        && matches!(key.target, Target::Unstaged | Target::Since(_))
        && dir.join(&key.path).is_file()
        && !git.is_tracked(dir, &key.path)?
    {
        text = git.untracked_diff(dir, &key.path)?;
    }
    Ok(text)
}

/// [`text_hash`] of the diff `key` names, cached.
fn cached_hash(
    git: &dyn GitBackend,
    cache: &GitCache,
    dir: &Path,
    key: DiffKey,
    snapshot: &dyn Fn() -> io::Result<String>,
) -> Result<Option<String>, ViewError> {
    if let Some(hash) = lock(&cache.hashes).get(&key) {
        return Ok(hash.clone());
    }
    let hash = text_hash(&diff_text(git, dir, &key, snapshot)?);
    let mut hashes = lock(&cache.hashes);
    if hashes.len() >= MAX_CACHED_HASHES {
        hashes.clear();
    }
    hashes.insert(key, hash.clone());
    Ok(hash)
}
```

- [ ] **Step 10: Add the handlers and dispatch**

`crates/mkrew-daemon/src/server/git.rs`: imports — add `GitSetViewedParams`, `ViewedFile` to the `mkrew_protocol::{...}` list, and add `use mkrew_core::viewed::{self, Mark};` and `use mkrew_store::ViewedScope;`. After `git_diff` (ends :120) add:

```rust
    /// `git.viewed` (ADR 0065): which files of the listed changes (`params` as in
    /// `git.status`) are still marked as viewed. A mark whose file's diff text changed, or
    /// whose file no longer changes, is deleted here; marks of other lists are not touched.
    pub(super) async fn git_viewed(&self, params: GitStatusParams) -> Outcome {
        if let Some(since) = &params.since
            && let Err(reason) = check_ref(since)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let target = match self
            .target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let repo_key = target.repo.clone().unwrap_or_default();
        let list = viewed::list_key(params.mode, params.since.as_deref());
        let scope = ViewedScope {
            project_id: params.project_id,
            worktree_task_id: params.worktree_task_id,
            repo: &repo_key,
            list: &list,
        };
        let stored = self.store().viewed_marks(&scope);
        let marks: Vec<Mark> = match stored {
            Ok(stored) => stored
                .into_iter()
                .map(|(path, hash)| Mark { path, hash })
                .collect(),
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if marks.is_empty() {
            return ok(&Vec::<ViewedFile>::new());
        }
        let (hub, cache) = (Arc::clone(&self.hub), Arc::clone(&self.git_cache));
        let Target {
            project: Project { id, base_ref, .. },
            dir,
            repo,
            ..
        } = target;
        let (mode, since) = (params.mode, params.since);
        let resolved = tokio::task::spawn_blocking(move || {
            git_view::viewed(
                &*hub.git,
                &cache,
                &dir,
                base_ref.as_deref(),
                (mode, since.as_deref()),
                marks,
                &|| hub.snapshots.tree(&*hub.git, id, &dir),
            )
        })
        .await;
        let result = resolved.map(|inner| {
            inner.map(|resolved| {
                let store = self.store();
                for stale in &resolved.stale {
                    let hash = Some(stale.hash.as_str());
                    if let Err(e) = store.remove_viewed(&scope, &stale.path, hash) {
                        tracing::warn!(
                            project_id = %scope.project_id,
                            "stale viewed mark not deleted: {e}"
                        );
                    }
                }
                drop(store);
                resolved
                    .viewed
                    .into_iter()
                    .map(|file| ViewedFile {
                        path: in_repo(repo.as_deref(), &file.path),
                        staged: file.staged,
                    })
                    .collect::<Vec<_>>()
            })
        });
        view_outcome("git.viewed", result)
    }

    /// `git.set_viewed` (ADR 0065): hashes the file's diff as it is now and stores the mark
    /// (`invalid_state` for a file with no diff), or removes it: the mark of that exact text,
    /// else (no diff any more) every mark of the file in the list.
    pub(super) async fn git_set_viewed(&self, params: GitSetViewedParams) -> Outcome {
        let GitSetViewedParams { diff: params, viewed: on } = params;
        if params.commit.is_some() {
            let message = "a commit's files cannot be marked as viewed";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        if let Some(since) = &params.since
            && let Err(reason) = check_ref(since)
        {
            return Outcome::error(ErrorCode::InvalidRequest, reason);
        }
        let path = match project_path(params.repo.as_deref(), &params.path) {
            Ok(path) => path,
            Err(outcome) => return outcome,
        };
        let target = match self
            .target(
                params.project_id,
                params.worktree_task_id,
                params.repo.as_deref(),
            )
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let repo_key = target.repo.clone().unwrap_or_default();
        let list = viewed::list_key(params.mode, params.since.as_deref());
        let (hub, cache) = (Arc::clone(&self.hub), Arc::clone(&self.git_cache));
        let Target {
            project: Project { id, base_ref, .. },
            dir,
            ..
        } = target;
        let hashed = {
            let (params, path) = (params.clone(), path.clone());
            tokio::task::spawn_blocking(move || {
                let snapshot = || hub.snapshots.tree(&*hub.git, id, &dir);
                git_view::diff_hash(
                    &*hub.git,
                    &cache,
                    &dir,
                    base_ref.as_deref(),
                    &params,
                    &path,
                    &snapshot,
                )
            })
            .await
        };
        let hash = match hashed {
            Ok(Ok(hash)) => hash,
            other => return view_outcome("git.set_viewed", other),
        };
        let scope = ViewedScope {
            project_id: params.project_id,
            worktree_task_id: params.worktree_task_id,
            repo: &repo_key,
            list: &list,
        };
        let stored = match (on, hash) {
            (true, Some(hash)) => self.store().add_viewed(&scope, &path, &hash),
            (true, None) => {
                let message = format!("{path} has no changes to mark as viewed");
                return Outcome::error(ErrorCode::InvalidState, message);
            }
            (false, hash) => self.store().remove_viewed(&scope, &path, hash.as_deref()),
        };
        store_outcome(stored)
    }
```

`crates/mkrew-daemon/src/server.rs`: after the `Request::GitDiff(params) => ...` arm (:415) add:

```rust
            Request::GitViewed(params) => (self.git_viewed(params).await, After::Continue),
            Request::GitSetViewed(params) => {
                (self.git_set_viewed(params).await, After::Continue)
            }
```

- [ ] **Step 11: Run the daemon unit tests**

Run: `cargo fmt --all && cargo test -p mkrew-daemon --lib git_view -j 4`
Expected: PASS, including the existing `diffs_of_each_side_are_cached_until_the_file_or_index_changes` (the refactor must not change `diff`'s behavior; it reads `lock(&cache.diffs).len()`).

If a `clippy::too_many_arguments` fires on `viewed` (7 params) it does not: the limit is 7. `target_of` takes 6.

- [ ] **Step 12: Write the failing integration test**

In `crates/mkrew-daemon/tests/daemon/main.rs` add `GitSetViewedParams` and `ViewedFile` to the `mkrew_protocol` import list (:14-39, keep it sorted: `GitSetBaseParams, GitSetViewedParams, GitShowParams` and `UndoPreview` … `ViewedFile, WORKSPACE_COLORS`).

Append to `crates/mkrew-daemon/tests/daemon/git.rs`:

```rust
#[test]
fn git_viewed_marks_follow_the_diff_and_clear_themselves() {
    let env = Env::new();
    let repo = env.repo();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(repo.join("b.txt"), "b\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("a.txt"), "one\nTWO\n").unwrap();
    std::fs::write(repo.join("new.txt"), "n\n").unwrap();
    let (mut daemon, mut client) = env.start();
    let id = client.add_project(&repo).id;
    let diff = |path: &str, mode, since: Option<&str>| GitDiffParams {
        project_id: id,
        worktree_task_id: None,
        repo: None,
        path: path.into(),
        mode,
        staged: false,
        commit: None,
        since: since.map(str::to_owned),
    };
    let list = |mode, since: Option<&str>| {
        Request::GitViewed(GitStatusParams {
            project_id: id,
            worktree_task_id: None,
            repo: None,
            mode,
            since: since.map(str::to_owned),
        })
    };
    let set = |params: GitDiffParams, viewed| {
        Request::GitSetViewed(GitSetViewedParams {
            diff: params,
            viewed,
        })
    };
    let file = |path: &str| ViewedFile {
        path: path.into(),
        staged: false,
    };
    let working = |client: &mut Client| -> Vec<ViewedFile> {
        client
            .call(list(GitMode::Working, None))
            .into_result()
            .unwrap()
    };

    assert_eq!(working(&mut client), []);
    let _: () = client
        .call(set(diff("a.txt", GitMode::Working, None), true))
        .into_result()
        .unwrap();
    let _: () = client
        .call(set(diff("new.txt", GitMode::Working, None), true))
        .into_result()
        .unwrap();
    assert_eq!(working(&mut client), [file("a.txt"), file("new.txt")]);

    // Marks survive a daemon restart; Against base is another list.
    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(working(&mut client), [file("a.txt"), file("new.txt")]);
    let base: Vec<ViewedFile> = client
        .call(list(GitMode::Base, None))
        .into_result()
        .unwrap();
    assert_eq!(base, []);

    // A mark in Against base does not disturb the Working one, and the other way round.
    let _: () = client
        .call(set(diff("a.txt", GitMode::Base, None), true))
        .into_result()
        .unwrap();
    std::fs::write(repo.join("a.txt"), "one\nTWO\nthree\n").unwrap();
    // Working: a.txt was edited, so its mark is gone (and deleted for good); new.txt stays.
    assert_eq!(working(&mut client), [file("new.txt")]);
    std::fs::write(repo.join("a.txt"), "one\nTWO\n").unwrap();
    assert_eq!(working(&mut client), [file("new.txt")]);
    // Never read while it was edited: the Base mark still matches the restored text.
    let base: Vec<ViewedFile> = client
        .call(list(GitMode::Base, None))
        .into_result()
        .unwrap();
    assert_eq!(base, [file("a.txt")]);

    // Unmarking: the file in Working changes, then a file with nothing to mark.
    let _: () = client
        .call(set(diff("new.txt", GitMode::Working, None), false))
        .into_result()
        .unwrap();
    assert_eq!(working(&mut client), []);
    assert_eq!(
        error_code(client.call(set(diff("b.txt", GitMode::Working, None), true))),
        ErrorCode::InvalidState
    );
    let mut commit = diff("a.txt", GitMode::Working, None);
    commit.commit = Some("HEAD".into());
    assert_eq!(
        error_code(client.call(set(commit, true))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set(diff("../x", GitMode::Working, None), true))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(list(GitMode::Base, Some("--force")))),
        ErrorCode::InvalidRequest
    );

    // Staging a viewed file keeps it viewed, on the staged side.
    let _: () = client
        .call(set(diff("a.txt", GitMode::Working, None), true))
        .into_result()
        .unwrap();
    git(&repo, &["add", "a.txt"]);
    let staged: Vec<ViewedFile> = client
        .call(list(GitMode::Working, None))
        .into_result()
        .unwrap();
    assert_eq!(
        staged,
        [ViewedFile {
            path: "a.txt".into(),
            staged: true
        }]
    );
    client.stop();
}

#[test]
fn git_viewed_marks_are_per_repository_with_project_relative_paths() {
    let env = Env::new();
    let root = env.repo();
    for name in ["fe", "be"] {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        git(&dir, &["config", "core.autocrlf", "false"]);
        std::fs::write(dir.join("x.txt"), "one\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "one"]);
        std::fs::write(dir.join("x.txt"), "one\nchanged\n").unwrap();
    }
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&root).id;
    let set = |repo: &str| {
        Request::GitSetViewed(GitSetViewedParams {
            diff: GitDiffParams {
                project_id: id,
                worktree_task_id: None,
                repo: Some(repo.into()),
                path: format!("{repo}/x.txt"),
                mode: GitMode::Working,
                staged: false,
                commit: None,
                since: None,
            },
            viewed: true,
        })
    };
    let list = |repo: &str| {
        Request::GitViewed(GitStatusParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some(repo.into()),
            mode: GitMode::Working,
            since: None,
        })
    };
    let _: () = client.call(set("fe")).into_result().unwrap();
    let fe: Vec<ViewedFile> = client.call(list("fe")).into_result().unwrap();
    assert_eq!(
        fe,
        [ViewedFile {
            path: "fe/x.txt".into(),
            staged: false
        }]
    );
    let be: Vec<ViewedFile> = client.call(list("be")).into_result().unwrap();
    assert_eq!(be, []);
    // A task worktree that does not exist cannot be asked about, and not with a repo.
    let bad = Request::GitViewed(GitStatusParams {
        project_id: id,
        worktree_task_id: Some(Ulid::new()),
        repo: Some("fe".into()),
        mode: GitMode::Working,
        since: None,
    });
    assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    client.stop();
}
```


- [ ] **Step 13: Run the integration tests**

Run:
```bash
cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4
cargo test -p mkrew-daemon --test daemon git_viewed -j 4
```
Expected: PASS (2 tests). If the first test fails on the staged assertion, check that `git add` changed the index mtime: the diff cache key includes it.

- [ ] **Step 14: Commit**

```bash
git add docs/decisions/0065-git-viewed-files.md docs/SPEC.md crates/mkrew-protocol/src/files.rs crates/mkrew-protocol/src/message.rs crates/mkrew-protocol/src/lib.rs crates/mkrew-daemon/src/git_view.rs crates/mkrew-daemon/src/git_view/tests.rs crates/mkrew-daemon/src/server/git.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/tests/daemon/git.rs crates/mkrew-daemon/tests/daemon/main.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(daemon): git.viewed and git.set_viewed, protocol 45 (ADR 0065)"
```

---

### Task 4: UI state, daemon requests and the `v` command

**Files:**
- Create: `crates/mkrew-ui/src/git_viewed.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod git_viewed;` after `pub mod git_view;`, :41)
- Modify: `crates/mkrew-ui/src/git_view.rs` (imports :14-26; `GitView` :560-614 and `new` :617-648; `list` :925-939; `files` :944-990; `select` :1179-1189; new methods)
- Create: `crates/mkrew-ui/src/shell/git_viewed.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (add `mod git_viewed;` after `pub(crate) mod git_rows;`, :25)
- Modify: `crates/mkrew-ui/src/main.rs` (add `ViewedFile` to the `mkrew_protocol` import, between `UndoPreview,` and `WORKSPACE_COLORS`, :42-43)
- Modify: `crates/mkrew-ui/src/shell/git.rs` (`load_git_status` closure :347-352)
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum after :110; `all()` after :235; `name()` after :312; `default_keys()` after :401; `context()` :431-438; test :854-866)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title` after :237; `available` :314-321)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (after :379)
- Test: `crates/mkrew-ui/src/git_viewed.rs` (inline), `crates/mkrew-ui/src/git_view/tests.rs`

**Interfaces:**
- Consumes: `ViewedFile`, `GitSetViewedParams`, `Request::GitViewed`, `Request::GitSetViewed` (Task 3).
- Produces:
  - `pub struct ViewedSet` with `pub fn new(files: Vec<ViewedFile>) -> ViewedSet`, `pub fn contains(&self, group: Group, path: &str) -> bool`, `pub fn set(&mut self, group: Group, path: &str, on: bool)`, `pub fn count(&self, files: &[(Target, ChangedFile)]) -> (usize, usize)`.
  - `GitView`: `pub viewed: Rc<ViewedSet>`, `pub fn wants_viewed(&self) -> bool`, `pub fn begin_viewed(&mut self) -> (GitStatusParams, u64)`, `pub fn viewed_loaded(&mut self, serial: u64, asked: (GitMode, Option<String>), files: Vec<ViewedFile>)`, `pub fn toggle_viewed(&mut self, target: &Target) -> Option<GitSetViewedParams>`, `pub fn diff_collapsed(&self) -> bool`, `pub fn unfold(&mut self)`, `pub fn viewed_counter(&self) -> Option<(usize, usize)>`.
- `Shell::load_git_viewed(project_id, window, cx)`, `Shell::toggle_git_viewed(project_id, target, window, cx)`, `Shell::toggle_selected_viewed(window, cx)`; `Command::GitToggleViewed` (name `git_toggle_viewed`, default `v`, context `GIT_CONTEXT`), palette title `Git: Mark file as viewed / not viewed`.
- The type is named `ViewedSet` because `crate::inbox::Viewed` already exists and `main.rs` imports it.

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-ui/src/git_viewed.rs` with the tests only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::ChangeKind;

    fn viewed(path: &str, staged: bool) -> ViewedFile {
        ViewedFile {
            path: path.into(),
            staged,
        }
    }

    fn listed(group: Group, path: &str) -> (Target, ChangedFile) {
        let target = Target {
            group,
            path: path.into(),
            commit: None,
        };
        let file = ChangedFile {
            path: path.into(),
            old_path: None,
            kind: ChangeKind::Modified,
            added: Some(1),
            removed: Some(0),
        };
        (target, file)
    }

    #[test]
    fn the_staged_group_is_told_apart_from_every_other_group() {
        let set = ViewedSet::new(vec![viewed("a.rs", true), viewed("b.rs", false)]);
        assert!(set.contains(Group::Staged, "a.rs"));
        assert!(!set.contains(Group::Unstaged, "a.rs"));
        for group in [Group::Unstaged, Group::Untracked, Group::Base, Group::Since] {
            assert!(set.contains(group, "b.rs"), "{group:?}");
        }
        assert!(!set.contains(Group::Staged, "b.rs"));
    }

    #[test]
    fn a_commits_files_are_never_viewed() {
        let mut set = ViewedSet::new(vec![viewed("a.rs", false)]);
        assert!(!set.contains(Group::Commit, "a.rs"));
        set.set(Group::Commit, "c.rs", true);
        assert!(!set.contains(Group::Commit, "c.rs"));
        assert!(!set.contains(Group::Unstaged, "c.rs"));
    }

    #[test]
    fn set_marks_and_clears_one_group_only() {
        let mut set = ViewedSet::default();
        set.set(Group::Staged, "a.rs", true);
        assert!(set.contains(Group::Staged, "a.rs"));
        assert!(!set.contains(Group::Unstaged, "a.rs"));
        set.set(Group::Staged, "a.rs", false);
        assert_eq!(set, ViewedSet::default());
    }

    #[test]
    fn the_counter_counts_viewed_over_every_listed_file() {
        let set = ViewedSet::new(vec![viewed("a.rs", true)]);
        let files = [
            listed(Group::Staged, "a.rs"),
            listed(Group::Unstaged, "a.rs"),
            listed(Group::Untracked, "n.txt"),
        ];
        // Only the staged copy of a.rs is viewed.
        assert_eq!(set.count(&files), (1, 3));
        assert_eq!(set.count(&[]), (0, 0));
    }
}
```

Append to `crates/mkrew-ui/src/git_view/tests.rs` (add `GitSetViewedParams, ViewedFile` to its `use mkrew_protocol::{...}` at :3):

```rust
fn viewed_file(path: &str, staged: bool) -> ViewedFile {
    ViewedFile {
        path: path.into(),
        staged,
    }
}

/// A [`loaded`] view whose daemon answered that only the staged `a.rs` is viewed.
fn with_viewed() -> GitView {
    let mut view = loaded();
    let (params, serial) = view.begin_viewed();
    view.viewed_loaded(serial, (params.mode, params.since), vec![viewed_file("a.rs", true)]);
    view
}

#[test]
fn the_viewed_counter_counts_every_listed_file_whatever_the_filters_hide() {
    let view = loaded();
    // a.rs staged + a.rs unstaged + b.rs + new.txt.
    assert_eq!(view.viewed_counter(), Some((0, 4)));
    let mut view = with_viewed();
    assert_eq!(view.viewed_counter(), Some((1, 4)));
    assert!(view.viewed.contains(Group::Staged, "a.rs"));
    assert!(!view.viewed.contains(Group::Unstaged, "a.rs"));
    // The file filter narrows the list, not the counter.
    view.filter.insert("new");
    assert_eq!(view.files().len(), 1);
    assert_eq!(view.viewed_counter(), Some((1, 4)));
    // No files: no counter; the History never has one.
    let mut empty = GitView::new(Ulid::new(), None);
    empty.begin_status();
    empty.status_loaded(Ok(GitStatus::default()));
    assert_eq!(empty.viewed_counter(), None);
    let _ = view.show(Section::History);
    assert_eq!(view.viewed_counter(), None);
    assert!(!view.wants_viewed());
}

#[test]
fn an_answer_for_older_asks_or_other_changes_is_dropped() {
    let mut view = loaded();
    assert!(view.wants_viewed());
    let (params, first) = view.begin_viewed();
    assert_eq!((params.mode, params.since.clone()), (GitMode::Working, None));
    let (_, second) = view.begin_viewed();
    // The first ask answers late: dropped. The second counts.
    view.viewed_loaded(first, (GitMode::Working, None), vec![viewed_file("b.rs", false)]);
    assert!(!view.viewed.contains(Group::Unstaged, "b.rs"));
    // An answer for Against base while Working is listed: dropped.
    view.viewed_loaded(second, (GitMode::Base, None), vec![viewed_file("b.rs", false)]);
    assert!(!view.viewed.contains(Group::Unstaged, "b.rs"));
    view.viewed_loaded(second, (GitMode::Working, None), vec![viewed_file("b.rs", false)]);
    assert!(view.viewed.contains(Group::Unstaged, "b.rs"));
    // Another list starts without the marks of the first.
    view.show(Section::Base);
    assert!(!view.viewed.contains(Group::Unstaged, "b.rs"));
    assert!(!view.wants_viewed(), "its status is loading");
}

#[test]
fn toggling_marks_at_once_and_describes_the_diff_to_send() {
    let mut view = loaded();
    let staged = target(Group::Staged, "a.rs");
    let params = view.toggle_viewed(&staged).unwrap();
    assert!(params.viewed);
    assert_eq!(params.diff, staged.params(view.project_id, None, None));
    assert!(params.diff.staged);
    assert!(view.viewed.contains(Group::Staged, "a.rs"));
    assert!(!view.viewed.contains(Group::Unstaged, "a.rs"));
    // Again: cleared.
    assert!(!view.toggle_viewed(&staged).unwrap().viewed);
    assert!(!view.viewed.contains(Group::Staged, "a.rs"));
    // A toggle makes an older in-flight answer stale.
    let (_, serial) = view.begin_viewed();
    view.toggle_viewed(&staged);
    view.viewed_loaded(serial, (GitMode::Working, None), vec![]);
    assert!(view.viewed.contains(Group::Staged, "a.rs"));
    // A History file has nothing to mark.
    assert_eq!(view.toggle_viewed(&commit_target("abc", "x")), None);
}

#[test]
fn a_viewed_selected_file_collapses_until_show_diff() {
    let mut view = with_viewed();
    // a.rs staged is selected by `loaded()` and viewed.
    assert!(view.diff_collapsed());
    view.unfold();
    assert!(!view.diff_collapsed());
    // Marking it again, or selecting another file, starts collapsed / open again.
    let staged = target(Group::Staged, "a.rs");
    view.toggle_viewed(&staged);
    assert!(!view.diff_collapsed(), "not viewed any more");
    view.toggle_viewed(&staged);
    assert!(view.diff_collapsed());
    view.unfold();
    view.select(target(Group::Unstaged, "b.rs"));
    view.select(staged);
    assert!(view.diff_collapsed(), "selecting resets Show diff");
}
```

(`commit_target` exists in this test file: it is used at :1046; if its signature differs, build the `Target { group: Group::Commit, path, commit: Some(hash) }` literal instead.)

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui git_view viewed -j 4` and `cargo test -p mkrew-ui git_viewed -j 4`
Expected: FAIL to compile (`ViewedSet`, `viewed_counter`, … not found).

- [ ] **Step 3: Implement `ViewedSet`**

Prepend to `crates/mkrew-ui/src/git_viewed.rs` (above the tests; and add `pub mod git_viewed;` to `lib.rs`):

```rust
//! The files of a Git view marked as viewed (SPEC §12B, ADR 0065), as the daemon last
//! confirmed them. Pure, so it is unit-tested; `GitView` owns one.

use std::collections::HashSet;

use mkrew_protocol::{ChangedFile, ViewedFile};

use crate::git_view::{Group, Target};

/// Viewed paths of the listed changes. The Staged group has its own set (a path can be staged
/// and unstaged with different diffs); every other group shares the other one. A History
/// commit's files (`Group::Commit`) are never viewed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewedSet {
    staged: HashSet<String>,
    other: HashSet<String>,
}

impl ViewedSet {
    /// The set a `git.viewed` answer describes.
    pub fn new(files: Vec<ViewedFile>) -> ViewedSet {
        let mut set = ViewedSet::default();
        for file in files {
            set.side_mut(file.staged).insert(file.path);
        }
        set
    }

    fn side_mut(&mut self, staged: bool) -> &mut HashSet<String> {
        if staged {
            &mut self.staged
        } else {
            &mut self.other
        }
    }

    /// Whether `path` of `group` is marked viewed.
    pub fn contains(&self, group: Group, path: &str) -> bool {
        match group {
            Group::Commit => false,
            Group::Staged => self.staged.contains(path),
            _ => self.other.contains(path),
        }
    }

    /// Marks or clears `path` of `group` (a no-op for a commit's files).
    pub fn set(&mut self, group: Group, path: &str, on: bool) {
        if group == Group::Commit {
            return;
        }
        let side = self.side_mut(group == Group::Staged);
        if on {
            side.insert(path.to_owned());
        } else {
            side.remove(path);
        }
    }

    /// `(viewed, listed)` over `files`: the header's `Viewed N/M`.
    pub fn count(&self, files: &[(Target, ChangedFile)]) -> (usize, usize) {
        let viewed = files
            .iter()
            .filter(|(target, _)| self.contains(target.group, &target.path))
            .count();
        (viewed, files.len())
    }
}
```

- [ ] **Step 4: Implement the `GitView` state**

In `crates/mkrew-ui/src/git_view.rs`:

1. Imports: add `GitSetViewedParams` and `ViewedFile` to the `mkrew_protocol::{...}` list (:15-20) and `use crate::git_viewed::ViewedSet;` after `use crate::git_history::{Detail, History};` (:26).
2. `GitView` fields (after `collapsed`, :596):

```rust
    /// The files marked as viewed (SPEC §12B, ADR 0065): the daemon's last answer, plus the
    /// user's toggles since. Shared with the list's render closure.
    pub viewed: Rc<ViewedSet>,
    /// The viewed file whose collapsed diff "Show diff" opened.
    unfolded: Option<Target>,
    /// Serial of the last `git.viewed` asked for or toggle made: an older answer is dropped.
    viewed_asked: u64,
```

and in `GitView::new` (after `collapsed: HashSet::new(),`): `viewed: Rc::new(ViewedSet::default()), unfolded: None, viewed_asked: 0,`.

3. `list()` (:931-933): replace

```rust
        if listed != self.listed() {
            self.status = Status::Loading;
        }
```
with
```rust
        if listed != self.listed() {
            self.status = Status::Loading;
            // Another list has its own marks; the answer for it is asked after its status.
            self.viewed = Rc::new(ViewedSet::default());
            self.viewed_asked += 1;
        }
```

4. `files()` (:944-990): keep the History branch, replace everything after it (from `let Status::Loaded(status) = &self.status else {` to the end of the function) with `self.status_files(true)\n    }` and add the new function right after:

```rust
    /// The changed files of the loaded status in list order. `narrowed`: only those that pass
    /// "Changed by" and the "Filter files" text (what the list shows); else every file of the
    /// section (what `Viewed N/M` counts).
    fn status_files(&self, narrowed: bool) -> Vec<(Target, ChangedFile)> {
        let Status::Loaded(status) = &self.status else {
            return vec![];
        };
        let checkpoint = self.since.as_ref().map(|since| since.checkpoint.clone());
        let kept = self.kept();
        let group = |group: Group, files: &[ChangedFile]| {
            files
                .iter()
                .filter(|file| !narrowed || kept.is_none_or(|kept| kept.contains(&file.path)))
                .filter(|file| !narrowed || self.matches(&file.path))
                .map(|file| {
                    let target = Target {
                        group,
                        path: file.path.clone(),
                        commit: checkpoint.clone().filter(|_| group == Group::Since),
                    };
                    (target, file.clone())
                })
                .collect::<Vec<_>>()
        };
        match (self.mode, &self.since) {
            (GitMode::Working, _) => [
                group(Group::Staged, &status.staged),
                group(Group::Unstaged, &status.unstaged),
                group(Group::Untracked, &status.untracked),
            ]
            .concat(),
            (GitMode::Base, None) => group(Group::Base, &status.against_base),
            (GitMode::Base, Some(_)) => group(Group::Since, &status.against_base),
        }
    }
```

5. `select()` (:1183-1188): add `self.unfolded = None;` after `self.hunk = None;`.
6. New methods, in a new `impl GitView` block placed right after `begin_stage`/`staged` (any spot inside the existing `impl GitView`):

```rust
    /// Whether the shown list can ask which files are viewed: loaded changes, not the
    /// History (a commit's files are never marked).
    pub fn wants_viewed(&self) -> bool {
        self.history.is_none() && matches!(self.status, Status::Loaded(_))
    }

    /// A `git.viewed` is about to be sent: the params of the shown `git.status`, and its serial
    /// (given back with the answer).
    pub fn begin_viewed(&mut self) -> (GitStatusParams, u64) {
        self.viewed_asked += 1;
        let (mode, since) = self.listed();
        let params = GitStatusParams {
            project_id: self.project_id,
            worktree_task_id: self.worktree_task_id,
            repo: self.repo.clone(),
            mode,
            since,
        };
        (params, self.viewed_asked)
    }

    /// The `git.viewed` answer: kept unless a newer ask or a toggle came meanwhile, or other
    /// changes (`asked` = mode and checkpoint) are listed now. A failed ask keeps the last
    /// set (the shell never calls this for it).
    pub fn viewed_loaded(
        &mut self,
        serial: u64,
        asked: (GitMode, Option<String>),
        files: Vec<ViewedFile>,
    ) {
        if serial != self.viewed_asked || asked != self.listed() {
            return;
        }
        self.viewed = Rc::new(ViewedSet::new(files));
    }

    /// Marks `target` viewed, or clears the mark: shown at once; returns the `git.set_viewed`
    /// to send. `None` for a History file.
    pub fn toggle_viewed(&mut self, target: &Target) -> Option<GitSetViewedParams> {
        if target.group == Group::Commit || self.history.is_some() {
            return None;
        }
        let viewed = !self.viewed.contains(target.group, &target.path);
        Rc::make_mut(&mut self.viewed).set(target.group, &target.path, viewed);
        // An answer to an older `git.viewed` no longer holds.
        self.viewed_asked += 1;
        self.unfolded = None;
        Some(GitSetViewedParams {
            diff: target.params(self.project_id, self.worktree_task_id, self.repo.clone()),
            viewed,
        })
    }

    /// The selected file is viewed and its diff is shown collapsed ("Show diff" opens it).
    pub fn diff_collapsed(&self) -> bool {
        self.selected.as_ref().is_some_and(|selected| {
            self.viewed.contains(selected.group, &selected.path)
                && self.unfolded.as_ref() != Some(selected)
        })
    }

    /// "Show diff": opens the selected viewed file's diff (until another file is selected).
    pub fn unfold(&mut self) {
        self.unfolded = self.selected.clone();
    }

    /// `(viewed, listed)` for the header's `Viewed N/M`: every file of the shown list, whatever
    /// "Changed by" and the file filter hide; `None` in the History or with nothing listed.
    pub fn viewed_counter(&self) -> Option<(usize, usize)> {
        if !self.wants_viewed() {
            return None;
        }
        let (viewed, listed) = self.viewed.count(&self.status_files(false));
        (listed > 0).then_some((viewed, listed))
    }
```

- [ ] **Step 5: Run the UI tests**

Run: `cargo fmt --all && cargo test -p mkrew-ui git_view -j 4 && cargo test -p mkrew-ui git_viewed -j 4`
Expected: PASS (the existing git_view tests compare `Requests::default()`: `Requests` was not changed).

- [ ] **Step 6: Import the type in `main.rs`**

In the `mkrew_protocol` import (:42-43) change `UndoPreview,\n    WORKSPACE_COLORS,` to `UndoPreview, ViewedFile,\n    WORKSPACE_COLORS,` (run `cargo fmt`).

- [ ] **Step 7: Create `shell/git_viewed.rs` and register it**

`shell.rs`: add `mod git_viewed;` after `pub(crate) mod git_rows;` (:25).

```rust
//! Git view "Mark as viewed" (SPEC §12B, ADR 0065): asking the daemon which marks still stand,
//! and toggling one.

use crate::*;

impl Shell {
    /// `git.viewed` of the Git view of `project_id`: which listed files are still viewed. Asked
    /// after every status answer and after every toggle; a failure keeps the last set.
    pub(crate) fn load_git_viewed(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        if !view.wants_viewed() {
            return;
        }
        let at = view.at();
        let (params, serial) = view.begin_viewed();
        let asked = (params.mode, params.since.clone());
        let loaded = connection.call::<Vec<ViewedFile>>(Request::GitViewed(params));
        cx.spawn_in(window, async move |this, cx| {
            let files = match loaded.await {
                Ok(files) => files,
                Err(error) => {
                    tracing::warn!(%project_id, "git.viewed failed: {error:#}");
                    return;
                }
            };
            let _ = this.update_in(cx, |shell, _, cx| {
                if let Some(view) = shell.git_at(project_id, &at) {
                    view.viewed_loaded(serial, asked, files);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Marks `target` viewed or not (the `v` key, a row's box, the toolbar button): shown at
    /// once, then sent; the daemon is asked again afterwards, so a failure just puts its truth
    /// back.
    pub(crate) fn toggle_git_viewed(
        &mut self,
        project_id: Ulid,
        target: git_view::Target,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(params) = self.git_view(project_id).toggle_viewed(&target) else {
            return;
        };
        cx.notify();
        let sent = connection.request(Request::GitSetViewed(params));
        cx.spawn_in(window, async move |this, cx| {
            let failure = match sent.await {
                Ok(Outcome::Result(_)) => None,
                Ok(Outcome::Error(body)) => Some(body.message),
                Err(error) => Some(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Some(error) = failure {
                    tracing::warn!(%project_id, path = target.path, "git.set_viewed failed: {error}");
                }
                shell.load_git_viewed(project_id, window, cx);
            });
        })
        .detach();
    }

    /// The `v` key and the palette command: the selected file of the shown Git view.
    pub(crate) fn toggle_selected_viewed(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let Some(target) = self
            .git
            .get(&project_id)
            .and_then(|view| view.selected.clone())
        else {
            return;
        };
        self.toggle_git_viewed(project_id, target, window, cx);
    }
}
```

- [ ] **Step 8: Ask after every status answer**

`shell/git.rs`, in `load_git_status`'s closure (:347-348), after `shell.git_requests(project_id, requests, window, cx);` add:

```rust
                // Which of the listed files are still marked as viewed (no-op in the History).
                shell.load_git_viewed(project_id, window, cx);
```

- [ ] **Step 9: Write the failing test**

In `keymap.rs` test `spec_defaults`' Git loop (:854-863) add `("v", Command::GitToggleViewed),` after `("d", Command::GitToggleDiffMode),`.

- [ ] **Step 10: Run to verify it fails**

Run: `cargo test -p mkrew-ui keymap -j 4`
Expected: FAIL to compile (`GitToggleViewed` not found).

- [ ] **Step 11: Implement**

`keymap.rs`:
- enum, after `GitToggleDiffMode,` (:110):

```rust
    /// Git only: mark the selected file as viewed, or clear the mark (SPEC §12B, ADR 0065).
    GitToggleViewed,
```
- `all()`, after `Command::GitToggleDiffMode,` (:235): `            Command::GitToggleViewed,`
- `name()`, after the `GitToggleDiffMode` arm (:312): `            Command::GitToggleViewed => "git_toggle_viewed".into(),`
- `default_keys()`, after `Command::GitToggleDiffMode => "d".into(),` (:401): `            Command::GitToggleViewed => "v".into(),`
- `context()`: add `| Command::GitToggleViewed` to the arm that ends in `| Command::GitFocusCommit => Some(GIT_CONTEXT),` (:431-438), after `| Command::GitToggleDiffMode`.

`palette.rs`: after the `GitToggleDiffMode` title (:237): `        Command::GitToggleViewed => "Git: Mark file as viewed / not viewed".into(),`; in `available` add `| Command::GitToggleViewed` to the `in_view(View::Git)` arm (:314-321), after `| Command::GitToggleDiffMode`.

`shell/sessions.rs`, after the `Command::GitToggleDiffMode => ...` arm (:379):

```rust
            Command::GitToggleViewed => self.toggle_selected_viewed(window, cx),
```

- [ ] **Step 12: Run the tests**

Run: `cargo fmt --all && cargo test -p mkrew-ui keymap palette -j 4`
Expected: PASS (the keymap template and palette tests iterate `Command::all()`).

- [ ] **Step 13: Run, lint, commit**

Run: `cargo fmt --all && cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings && cargo test -p mkrew-ui -j 4`
Expected: no warnings, PASS (the shell methods are used by the status hook and the `v` command, so nothing is dead code).

```bash
git add crates/mkrew-ui/src/git_viewed.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view/tests.rs crates/mkrew-ui/src/shell/git_viewed.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/git.rs crates/mkrew-ui/src/keymap.rs crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/shell/sessions.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): viewed state, git.viewed requests and the v command (ADR 0065)"
```

---

### Task 5: Rendering: row box, counter, toolbar button, collapsed diff

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`opacity` mod, after `DISABLED` :138)
- Modify: `crates/mkrew-ui/src/shell/git_rows.rs` (`git_list` :16-51, `git_file_row` :116-266, new `viewed_box`, `git_viewed_line`)
- Modify: `crates/mkrew-ui/src/shell/git_render.rs` (`git_header` :228-235)
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs` (body match :61-114, toolbar :351-415)

**Interfaces:**
- Consumes: Task 4 `GitView` API (`viewed`, `viewed_counter`, `diff_collapsed`, `unfold`) and `Shell::toggle_git_viewed`.
- Produces: `theme::opacity::VIEWED`, `git_rows::viewed_box(on, theme)`, `git_rows::git_viewed_line(view, theme)`.

There is no headless UI test for rendering (GPUI): this task's check is `cargo clippy`, the existing tests, and the HUMAN_CHECKS entry (Task 6).

- [ ] **Step 1: Theme token**

`theme.rs`, in `pub mod opacity`, after `pub const DISABLED: f32 = 0.5;` (:138):

```rust
    /// A changed-file row marked as viewed (SPEC §12B): dimmed, still readable.
    pub const VIEWED: f32 = 0.6;
```

- [ ] **Step 2: The row: box and dimming**

`shell/git_rows.rs`:

1. In `git_list`, before `uniform_list(` add `let viewed = Rc::clone(&view.viewed);` and inside the `.map(|(index, row)| {` closure, before `git_file_row(`:

```rust
                        let is_viewed = matches!(
                            row,
                            git_view::ListRow::File(target, _)
                                if viewed.contains(target.group, &target.path)
                        );
```
and pass `is_viewed,` as a new argument after `folded,` in the `git_file_row(` call.

2. `git_file_row` signature: add `viewed: bool,` after `folded: bool,`; update the doc comment with "a viewed file is dimmed and its box is checked".

3. In the file row builder: after `.when(!picked, |el| el.hover(move |style| style.bg(rgb(hover))))` add:

```rust
        // A viewed row steps back; the selected one stays fully readable.
        .when(viewed && !picked, |el| el.opacity(opacity::VIEWED))
```

4. Between the counts child (`.children(git_view::counts(file)...)`) and the stage action (`// Shown on hover and on the selected file ...`), add:

```rust
        // Not for a History commit's files (never marked).
        .when(group != git_view::Group::Commit, |el| {
            let target = target.clone();
            el.child(
                viewed_box(viewed, theme)
                    .id(SharedString::from(format!("{id}-viewed")))
                    .cursor_pointer()
                    .tooltip(tooltip(
                        theme,
                        if viewed {
                            "Mark as not viewed"
                        } else {
                            "Mark as viewed"
                        },
                        "",
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        // Not a click on the row (which would select it).
                        cx.stop_propagation();
                        window.focus(&shell.git_focus, cx);
                        shell.toggle_git_viewed(project_id, target.clone(), window, cx);
                    })),
            )
        })
```

5. New free functions at the end of the file's non-impl code (after `row_action`):

```rust
/// The "viewed" box of a file: a small square, ✓ in the accent color while viewed (the look of
/// the broadcast dialog's checkboxes).
pub(crate) fn viewed_box(on: bool, theme: &Theme) -> gpui::Div {
    div()
        .flex_none()
        .size(px(space::M))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::SMALL))
        .border(px(theme::BORDER_WIDTH))
        .border_color(rgb(theme.border))
        .text_size(font_size::META)
        .text_color(rgb(theme.accent))
        .child(if on { "✓" } else { "" })
}

/// `Viewed 3/10` in the side panel header (✓ once every file is viewed); none in the History
/// or while nothing is listed.
pub(crate) fn git_viewed_line(view: &GitView, theme: &Theme) -> Option<gpui::Stateful<gpui::Div>> {
    let (viewed, listed) = view.viewed_counter()?;
    Some(
        div()
            .id("git-viewed-count")
            .px(px(space::M))
            .flex()
            .items_center()
            .gap(px(space::XS))
            .tooltip(tooltip(
                theme,
                "Files you marked as viewed; a mark clears when the file's diff changes",
                "",
            ))
            .child(meta_line(theme, format!("Viewed {viewed}/{listed}").into()))
            .when(viewed == listed, |el| {
                el.child(
                    div()
                        .text_size(font_size::META)
                        .text_color(rgb(theme.accent))
                        .child("✓"),
                )
            }),
    )
}
```

- [ ] **Step 3: The header counter**

`shell/git_render.rs` `git_header` (:234-235): after `.children(self.git_base_line(project_id, view, status, theme, cx))` add:

```rust
            .children(git_viewed_line(view, theme))
```
(Free widget fns of `shell/*.rs` are re-exported at the crate root by `main.rs:121` `use shell::git_rows::*;`, so `git_viewed_line` and `viewed_box` are reached through `use crate::*;`, as `git_diff.rs` reaches `change_counts`.)

- [ ] **Step 4: The diff: collapsed body and toolbar button**

`shell/git_diff.rs`:

1. In the `body` match (:61-114), insert this arm **before** the `git_view::Diff::Shown(shown) if shown.diff.binary || shown.diff.too_large =>` arm (:87):

```rust
            git_view::Diff::Shown(_) if view.diff_collapsed() => message(
                theme,
                None,
                "Marked as viewed.".into(),
                Some("Its diff has not changed since you marked it.".into()),
            )
            .flex_1()
            .child(
                button("git-viewed-show", "Show diff", theme, true).on_click(cx.listener(
                    move |shell, _, _, cx| {
                        shell.git_view(project_id).unfold();
                        cx.notify();
                    },
                )),
            )
            .into_any_element(),
```

2. In `git_diff_toolbar`, before `let discard = ...` (:351) add:

```rust
        let viewed = (selected.group != git_view::Group::Commit).then(|| {
            let on = view.viewed.contains(selected.group, &selected.path);
            let target = selected.clone();
            quiet_button(
                "git-viewed",
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::XS))
                    .child(viewed_box(on, theme))
                    .child("Viewed"),
                theme,
                theme.hover,
            )
            .flex_none()
            .tooltip(tooltip(
                theme,
                if on {
                    "Mark as not viewed"
                } else {
                    "Mark as viewed: its diff collapses until it changes"
                },
                self.keymap.label(Command::GitToggleViewed),
            ))
            .on_click(cx.listener(move |shell, _, window, cx| {
                window.focus(&shell.git_focus, cx);
                shell.toggle_git_viewed(project_id, target.clone(), window, cx);
            }))
        });
```

and in the toolbar's child chain, right after the 1 px divider `div()` (:408-414) and before `.child(self.zed_button(...))`, add `.children(viewed)`.

- [ ] **Step 5: Build, lint, test**

Run:
```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings
cargo test -p mkrew-ui -j 4
```
Expected: no warnings, PASS. Then grep your diff for hex colors or raw pixel numbers outside `theme.rs`: `git diff -- crates/mkrew-ui/src/shell | grep -nE '#[0-9A-Fa-f]{6}|px\([0-9]'` must print nothing.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/shell/git_viewed.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/git.rs crates/mkrew-ui/src/shell/git_rows.rs crates/mkrew-ui/src/shell/git_render.rs crates/mkrew-ui/src/shell/git_diff.rs crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): viewed boxes, Viewed N/M and collapsed diffs in the Git view (ADR 0065)"
```

---

### Task 6: HUMAN_CHECKS, PROGRESS note, full verify

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (new entry at the top of "To verify", above the 2026-10-15 Ask AI entry, :18)
- Modify: `docs/PROGRESS.md` ("Notes for the next session", :31)

- [ ] **Step 1: HUMAN_CHECKS entry**

Insert as the first `### To verify` section:

```markdown
### To verify — 2026-10-19 Mark files as viewed in the Git view (ADR 0065, plan docs/superpowers/plans/2026-10-19-git-viewed-files.md)

- [ ] **Restart the daemon first** (protocol 45, migration 0021).
- [ ] **Mark and dim (dark + light):** Git view on a workspace with ≥ 5 changed files. Each file row
  has a small box before the Stage button. Click one: ✓ in the accent color, the row dims (still
  readable), the header shows `Viewed 1/5` under the Working / Vs. base / History tabs, selection
  does not move. Select that file: the diff panel says "Marked as viewed." with **Show diff**; the
  toolbar shows a checked "Viewed" button. Show diff opens it; selecting another file and coming
  back collapses it again.
- [ ] **Keyboard:** with the file list focused, `v` toggles the selected file (also from the
  palette: "Git: Mark file as viewed / not viewed"); the tooltip of the toolbar button shows V. Mark
  every file: the counter reads `Viewed 5/5 ✓`. `v` is not bound while the commit box or the
  file filter has the keyboard (typing "v" there types a v).
- [ ] **A change clears the mark:** mark a file, then let an agent (or an editor) change it →
  within a second its box unchecks, the row un-dims and the diff shows. Revert the file by hand
  to the marked text afterwards: it stays unviewed (the mark was deleted).
- [ ] **Staging:** mark an unstaged file, press `s`: in Staged it is still checked and dimmed.
  Edit it again: the Staged copy stays viewed, the new Changes copy is not.
- [ ] **Other lists:** mark files in Vs. base, switch to Working and back: the Vs. base marks are
  still there. Open a task's "View changes": its own marks. In a task worktree (chooser) marks
  are separate from the project folder's.
- [ ] **Repositories:** a workspace with `fe/` and `be/` repos: marking `fe/x.txt` leaves `be/x.txt`
  unmarked; the Repo chips keep each repo's marks.
- [ ] **History:** no boxes, no counter, `v` does nothing.
- [ ] **Restart Mkrew and the daemon:** the marks come back.
- [ ] **Many files:** with ~50 viewed files and an agent saving files, the list keeps updating
  without freezing.
```

- [ ] **Step 2: PROGRESS note**

Add one line at the top of "Notes for the next session" (:31):

```markdown
- 2026-10-19 viewed files in the Git view (plan docs/superpowers/plans/2026-10-19-git-viewed-files.md, ADR 0065, migration 0021, protocol 45): `git.viewed` / `git.set_viewed`, marks = hash of the file's diff text per list (`mkrew_core::viewed`, table `viewed_file`), `v` / box / toolbar button, `Viewed N/M`, collapsed diff. Manual checks in HUMAN_CHECKS "2026-10-19 Mark files as viewed".
```

- [ ] **Step 3: Full verification**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: `verify: OK` (fmt check, clippy `-D warnings`, all tests). Fix what it reports in the owning task's files, never by loosening a lint.

- [ ] **Step 4: Commit**

```bash
git add docs/HUMAN_CHECKS.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: manual checks and progress note for viewed files (ADR 0065)"
```

---

## Self-Review

**1. Spec coverage** (the A5 brief, line by line):

| Requirement | Where |
| --- | --- |
| "Mark as viewed" per file, collapse/dim in list and diff | Task 5 (`opacity::VIEWED`, row, `diff_collapsed` body), Task 4 (state) |
| `Viewed N/M` counter in the header | Task 4 (`viewed_counter`), Task 5 (header line) |
| Mark clears when the file's diff changes | Tasks 1 + 3 (`resolve` / `viewed` delete stale), integration test |
| 1 or N repos (ADR 0029) | Task 2 `repo` column, Task 3 `git_viewed_marks_are_per_repository_with_project_relative_paths` |
| Staged and unstaged lists | Task 1 `sides`, Task 3 `staging_a_viewed_file_keeps_it_viewed`, Task 4 `ViewedSet` staged / other |
| Compare-against-base mode (verify) | Verified in Design: `GitMode::Base` + `Group::Base` / `Group::Since`; list keys `base`, `since:<checkpoint>`; Task 3 `marks_follow_the_list_they_are_read_in` |
| Keyboard: free key + checkbox + palette | Design (free `v`), Task 4 (command), Task 5 (row box, toolbar button) |
| Key = (repo, path, hash of the diff text) | Task 2 schema, Task 1 `text_hash`, Task 3 `diff_text` |
| Reuse remembered-prefs store or justify daemon SQLite | Design + ADR 0065 Context (settings.toml unsuitable), Task 3 |
| Prune marks whose file no longer changes | Task 1 `resolve`, Task 3 handler deletes `stale` |
| Pure logic unit-tested; UI in HUMAN_CHECKS | Tasks 1, 4 tests; Task 6 |
| SPEC §12B update; ADR 0065 | Task 3 |

Gap found and closed while writing: the first draft keyed the mark without a list, which let looking at Working changes delete Against-base marks; the `list` column and Review Focus 3 fix that. Task order keeps every commit compiling: pure core (1), store (2), protocol + daemon in one task so the exhaustive `Request` match stays complete (3), UI state with its requests and command so no shell method is dead code (4), rendering (5), docs (6).

**2. Placeholder scan:** no "TBD", "TODO", "similar to Task N" or code-less code steps; no task needs a later task's step first.

**3. Type consistency:** `Standing { path, staged }` (Task 1) is what `resolve` pushes; Task 3 handlers turn it into the protocol's `ViewedFile { path, staged }` (with the repo prefix), which Task 4 `ViewedSet::new` consumes and the shell's `Vec<ViewedFile>` call decodes. `Mark { path, hash }` (Task 1) is built from store tuples in Task 3 `git_viewed`. `ViewedScope` fields (Task 2) match both handlers. `resolve(marks, sides, now)` (Task 1) is called with `(marks, &[bool], closure)` in Task 3 `viewed`. `GitView::toggle_viewed -> Option<GitSetViewedParams>` (Task 4) is sent by `Request::GitSetViewed` in the same task. `begin_viewed -> (GitStatusParams, u64)` and `viewed_loaded(serial, (GitMode, Option<String>), Vec<ViewedFile>)` match the shell call. The UI type is `ViewedSet` everywhere (never `Viewed`, which `inbox` owns).

**4. Review Focus:** each of the five lines names its pinning tests above and each of those tests exists in Tasks 1-3 (`the_staged_side_is_only_read_for_marks_the_unstaged_side_did_not_explain`, `a_mark_stands_until_its_diff_text_changes`, `staging_a_viewed_file_keeps_it_viewed`, `binary_and_untracked_files_can_be_marked`, `viewed_marks_are_kept_per_scope_and_list`, `viewed_hashes_are_cached_until_the_file_changes`, `deleting_a_task_deletes_the_marks_of_its_worktree`, the two integration tests).

**Open risks (not blockers):** (a) a viewed file's diff is hashed from the file at request time, so a click racing an agent edit marks the newer text (ADR consequence); (b) a `since:` turn-tree list calls `snapshot()` on each hash cache miss (the cost HUMAN_CHECKS T11.5 already describes for status); (c) `since:` rows of finished tasks / turns are never pruned (`// ponytail:` retention pass if the table grows); (d) the GPUI rendering has no automated test, only HUMAN_CHECKS.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-10-19-git-viewed-files.md`. Two execution options: **Subagent-driven** (a fresh subagent per task, review between tasks) or **Native** (inline, one session). Recommended: **Native / executing-plans**: tasks 1-3 are a straight chain through shared interfaces (core → store → protocol + daemon), tasks 4-5 share `git_view.rs` and the shell, so little is parallel. Every task ends with a compiling tree and its own tests.
