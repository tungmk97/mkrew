# Data dir retention (session logs, files_changed, old events) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `%APPDATA%\mkrew` stops growing without bound: old session logs and old noisy events are deleted, and each watcher batch is stored once instead of once per live session.

**Architecture:**
1. **One row per batch.** `Hub::files_changed` inserts the batch once, on the project's newest live session (the session `doc.list` already picks when several sessions share a change). No schema change. Conflict detection (`record_edits`, in memory) and `TurnFiles` are unchanged.
2. **Quieter watcher.** `watch::Filter` also honours nested `.gitignore` files and `.git/info/exclude`, and never reports `.worktrees/`. Markdown stays reported even when ignored (SPEC: `doc.list` lists plans in ignored folders).
3. **Retention** (new `crates/mkrew-daemon/src/retention.rs`), with the same 7-day window as attachments (`attachment::KEEP_MS`): delete the log files of sessions that ended more than 7 days ago and that no tab shows; delete `files_changed`, `hook` and `edited` events older than 7 days. Session rows, `native_session_id`, `interrupted`, turns and `usage` events stay, so resume and usage totals are untouched. Runs at daemon startup and in the existing hourly backup tick.
4. **Reclaim space** with one conditional `VACUUM` at startup, when free pages exceed a quarter of the file.

**Tech Stack:** Rust (rusqlite, ignore 0.4 `gitignore`, tokio).

**Spec:** the prompt below (2026-10-14 chat, "Prompt 3"), SPEC §5 (`docs/SPEC.md:205`), §8 (`:618`, session logs `:654`), ADR 0019. This plan amends ADR 0019 with ADR 0056 and updates SPEC §5/§8.

<details><summary>Spec (verbatim)</summary>

```text
Fix: %APPDATA%\mkrew grows without bound.

Evidence (2026-10-14, 7 days of use)
- sessions\*.log: 569.9 MB in 255 files, 60–140 MB/day since 10-10. Rotation (SPEC §8) caps one
  session at ~10 MB (.log + .log.1, 64 sessions already rotated) but nothing ever deletes old
  sessions' logs. `hide_session` (mkrew-store/src/lib.rs ~847, ADR 0019) keeps row, events, turns
  and log forever.
- mkrew.db 23 MB, of which session_event `files_changed` ≈ 14 MB (14,329 rows). Causes:
  1. session.rs `files_changed` (~716-735) inserts the same batch once PER LIVE SESSION of the project
     (N live sessions → N copies).
  2. Paths include noise: `.worktrees/` (one event 156 KB), generated run logs under
     `docs/jira/runs/` (one session: 1021 events, 4.7 MB), a log file rewritten over and over
     (`pr571-karate-run2.log`, ~80 events in each of 4 sessions). The watcher skips only .git,
     node_modules and target (files.rs ~70).
- backups\ (7 daily copies, 61 MB) is already bounded; leave it.

Goal
1. Retention for session logs: delete logs of sessions that are hidden or exited longer than a limit
   (and/or cap total size), at daemon startup and daily. Live sessions, sessions a tab still shows,
   and anything resume needs are never touched. `session.history` returns nothing for a purged
   session instead of failing.
2. files_changed: store each batch once per project (or attribute it to one session) and read it the
   same way. docs.rs `doc.list` and conflict detection must keep working (read docs.rs ~104 and
   server/doc.rs). Skip paths that git ignores (the `ignore` crate is already used in files.rs), plus
   `.worktrees/`.
3. Old session_event rows: prune by age with the same retention, then reclaim space (incremental
   vacuum or VACUUM at startup; pick one and justify it).

Constraints
- This amends ADR 0019 ("history stays") → new ADR (check the latest number) + SPEC 5/8 update in
  the same commit. If you add a setting for the limits, put it in settings.rs with a default and
  document it in the generated settings comment. If not, hard-code with a `ponytail:` comment.
- Schema change → new migration (check the latest number in crates/mkrew-store/migrations).
- Follow AGENTS.md: tests, no network, `sh scripts/verify.sh`, Conventional Commit, own hunks only.
  Do not delete anything in the user's real %APPDATA%\mkrew during development; tests use temp dirs.

Acceptance
- Tests: an old hidden/exited session's log is purged, a live and a tab-shown one are kept;
  one batch with 3 live sessions writes one row; ignored paths are not stored; doc.list still lists a
  changed .md file.
- Report the expected size after one startup on a copy of a large data dir (optional, if time allows).
```
</details>

## Decisions

| Question | Decision | Why |
|---|---|---|
| Limit | 7 days after `ended_at`, the existing `attachment::KEEP_MS`; no setting, no size cap | One retention for attachments, logs and events. `ponytail:` comment names the upgrade (a `[storage]` setting or a total-size cap) if 7 days proves wrong. |
| "Hidden" | No separate rule: a hidden session is either live (kept) or ended (`ended_at` set → aged like any other) | `session` has no `hidden_at`; hiding a live session stops it, which sets `ended_at`. |
| What resume needs | Session rows, `native_session_id`, `interrupted`, tabs — never deleted; only log files and three event kinds are | Resume (`server/resume.rs`) reads rows and tabs, never logs or events. |
| Event kinds pruned | `files_changed`, `hook`, `edited` | `usage` is read with `since = 0` (`server/turn.rs:143`, usage totals): kept. Readers of the pruned kinds look back 24 h (`doc.list`) or to the running turn (`session.files`). |
| `files_changed` storage | One row per batch on the newest live session; no new table, no migration | `doc_files` already takes the greatest `(at, session)` among equal changes, so `doc.list` output is the same; `record_edits` never read the DB. |
| Space | Startup `VACUUM` when `freelist_count * 4 > page_count`; no `auto_vacuum` | SQLite reuses free pages for new rows, so after the first big prune the file stays bounded; only that one shrink needs a rewrite. `auto_vacuum = INCREMENTAL` on an existing DB needs a full `VACUUM` first anyway, plus periodic `incremental_vacuum` calls. |
| Ignore rules | Root + nested `.gitignore`, `.git/info/exclude`, `.worktrees/`; not the global `core.excludesFile` | The global file is the user's machine config: tests would depend on it (a global `*.log` breaks `paths_outside_the_root…`). `ponytail:` comment. `.worktrees` only in the watcher, not `ALWAYS_IGNORED` (the file tree and Run setup browse it). |
| Schedule | Startup + the hourly backup tick | The tick exists (its first tick runs at startup); hourly is at least daily and costs a few `stat`s. |

## Global Constraints

- No migration (no schema change). Latest migration stays `0019`.
- ADR number: latest is `0054` → this plan uses **0056** (0055 is reserved by the hooks plan `2026-10-14-hooks-use-daemon-mk.md`). Re-check `docs/decisions/` first and take the next free number if needed; use it everywhere below.
- `PROTOCOL_VERSION` stays 41.
- Never touch the user's real `%APPDATA%\mkrew` (no daemon with it as data dir, no delete). Tests use temp dirs. The optional size report reads a COPY of `mkrew.db` and only `stat`s the real log files.
- No `unwrap`/`expect` outside tests; `thiserror` in `mkrew-store`, `anyhow` in the daemon binary paths that already use it.
- Shell: in omp `bash` may be WSL; run `cargo`/`git` directly, `sh scripts/verify.sh` (Git Bash). Build with `-j 4`. Rebuild before daemon integration tests: `cargo build -j 4 -p mkrew-daemon -p mkrew-fakeagent`.
- Conventional Commits; explicit paths, `git add -p` where others have WIP; never `git add -A`, never push.

## Review Focus

1. **A session ended >7 days ago that a tab still shows** (pane kept open after exit, or an interrupted pane waiting to resume) → log kept. Pinned by `run_purges_old_logs_but_keeps_live_and_tab_shown_sessions` (Task 3).
2. **Log file locked or already gone** (Windows sharing violation, user deleted it) → the run logs a warning, continues with the next file, retries next tick; no panic, the count says what was removed. Pinned by the same test (a session whose files are already gone counts 0 and does not fail).
3. **`usage` events older than 7 days** → kept (usage totals read from 0). Pinned by `prune_session_events_drops_only_old_rows_of_the_given_kinds` (Task 3).
4. **Nested `.gitignore` negation and anchored patterns** (`docs/jira/.gitignore` with `/runs/` and `!runs/keep.txt`) → evaluated relative to that folder, deepest file wins. Pinned by `honours_nested_gitignore_and_info_exclude` (Task 2).
5. **Several live sessions, one change** → one row, and `doc.list` still shows the `.md` with a session and a time. Pinned by `one_batch_is_stored_once_for_all_live_sessions` (Task 1).

---

### Task 1: Store each watcher batch once

**Files:**
- Modify: `crates/mkrew-daemon/src/session.rs:716-735` (`Hub::files_changed`)
- Test: `crates/mkrew-daemon/tests/daemon/states.rs` (helpers in `tests/daemon/main.rs`: `Env`, `write_fake_profile_as`, `doc_list`, `changed_files`)

**Interfaces:**
- Produces: `files_changed` rows: one per batch, `session_id` = greatest live session ULID of the project.

- [ ] **Step 1: Write the failing test** in `states.rs`, after `file_watcher_records_changed_files_of_live_sessions_except_ignored_ones`:

```rust
/// One watcher batch is one `files_changed` row, however many sessions are live, and
/// `doc.list` still names the changed Markdown file (ADR 0056).
#[test]
fn one_batch_is_stored_once_for_all_live_sessions() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::create_dir_all(repo.join("docs")).unwrap();
    let writer = env.dir.path().join("writer.toml");
    std::fs::write(
        &writer,
        "[[step]]\naction = \"write_file\"\npath = \"docs/plan.md\"\ncontent = \"# Plan\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    let idle = env.dir.path().join("idle.toml");
    std::fs::write(&idle, "[[step]]\naction = \"ask\"\n").unwrap();
    env.write_fake_profile_as("writer", &format!("['--scenario', '{}']", writer.display()));
    env.write_fake_profile_as("idle", &format!("['--scenario', '{}']", idle.display()));
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    for profile in ["idle", "idle"] {
        let spawned: SessionSpawnResult = client.spawn(&project, profile).into_result().unwrap();
        client.wait_state(spawned.session_id, SessionState::NeedsInput);
    }
    let spawned: SessionSpawnResult = client.spawn(&project, "writer").into_result().unwrap();
    client.wait_state(spawned.session_id, SessionState::NeedsInput);

    let db = env.data_dir().join("mkrew.db");
    let rows = || {
        Store::open(&db).unwrap().project_session_events(project.id, "files_changed", 0).unwrap()
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !rows().iter().any(|(_, e)| e.payload["paths"].to_string().contains("docs/plan.md")) {
        assert!(Instant::now() < deadline, "docs/plan.md never recorded");
        std::thread::sleep(Duration::from_millis(50));
    }
    let rows = rows();
    let mut times: Vec<i64> = rows.iter().map(|(_, e)| e.at).collect();
    times.dedup();
    assert_eq!(times.len(), rows.len(), "a batch was stored more than once: {rows:?}");

    let docs = doc_list(&mut client, &project);
    let plan = docs.iter().find(|d| d.file_path == "docs/plan.md").expect("plan listed");
    assert!(plan.last_session_id.is_some() && plan.last_changed_at.is_some(), "{plan:?}");
    client.stop();
}
```

(The `SessionEvent` payload field may be named differently — read `mkrew_store::SessionEvent`; `changed_files` in `main.rs` shows how paths are read from it. Rows come ordered by `at`, so `dedup` on `at` finds copies.)

- [ ] **Step 2: Run it to see it fail**

Run: `cargo build -j 4 -p mkrew-daemon -p mkrew-fakeagent && cargo test -j 4 -p mkrew-daemon --test daemon one_batch_is_stored_once`
Expected: FAIL `a batch was stored more than once` (3 rows with the same `at`).

- [ ] **Step 3: Implement** — in `Hub::files_changed` (`session.rs`), replace the per-session insert loop:

```rust
        let working: Vec<Ulid> = {
            let store = self.store();
            // One row per batch (ADR 0056), on the newest live session: the one `doc.list`
            // picks anyway when sessions share a change.
            if let Some(&newest) = sessions.iter().max() {
                if let Err(e) = store.insert_session_event(newest, "files_changed", &payload, at) {
                    tracing::warn!(%newest, "files_changed not recorded: {e}");
                }
            }
            match store.list_sessions(Some(project_id)) {
                // … unchanged
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -j 4 -p mkrew-daemon --test daemon file_watcher && cargo test -j 4 -p mkrew-daemon --test daemon one_batch && cargo test -j 4 -p mkrew-daemon --test daemon docs`
Expected: all pass. If a test in `docs.rs`/`main.rs` expected a row on a specific older session, change it to expect the newest live session (the new contract), not the old copy.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/src/session.rs crates/mkrew-daemon/tests/daemon/states.rs
git commit -m "fix(daemon): store each changed-files batch once, not once per live session"
```

### Task 2: Watcher honours nested `.gitignore`, `.git/info/exclude`, `.worktrees/`

**Files:**
- Modify: `crates/mkrew-daemon/src/watch.rs:19-90` (`Filter`)
- Test: `crates/mkrew-daemon/src/watch.rs` `mod tests` (helper `filter(gitignore)`)

**Interfaces:**
- Produces: `Filter::relative` unchanged signature; new rules: nested `.gitignore` (deepest decides), `.git/info/exclude`, `WATCH_IGNORED = [".worktrees"]`.

- [ ] **Step 1: Write the failing tests** in `watch.rs` `mod tests`:

```rust
    #[test]
    fn honours_nested_gitignore_and_info_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs/jira/runs")).unwrap();
        std::fs::create_dir_all(root.join(".git/info")).unwrap();
        std::fs::write(root.join("docs/jira/.gitignore"), "/runs/\n!runs/keep.txt\n").unwrap();
        std::fs::write(root.join(".git/info/exclude"), "pr*-run*.log\n").unwrap();
        let filter = Filter::new(root);
        let rel = |p: &str| filter.relative(&root.join(p), Some(false));
        assert_eq!(rel("docs/jira/runs/out.txt"), None, "nested, anchored to its folder");
        assert_eq!(rel("runs/out.txt"), Some("runs/out.txt".into()), "not anchored at the root");
        assert_eq!(rel("api/pr571-karate-run2.log"), None, "info/exclude");
        assert_eq!(rel("docs/jira/runs/plan.md"), Some("docs/jira/runs/plan.md".into()), "md stays");
        assert_eq!(rel("src/lib.rs"), Some("src/lib.rs".into()));
    }

    #[test]
    fn never_reports_worktrees() {
        let (dir, filter) = filter("");
        let root = dir.path();
        assert_eq!(filter.relative(&root.join(".worktrees/fix/src/a.rs"), Some(false)), None);
        assert_eq!(filter.relative(&root.join(".worktrees/fix/README.md"), Some(false)), None);
    }
```

(`!runs/keep.txt` cannot re-include a file under an ignored directory in git either; it is there to show the nested file parses with negations — do not assert on it.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -j 4 -p mkrew-daemon --lib watch::tests`
Expected: the two new tests FAIL; the old ones pass.

- [ ] **Step 3: Implement** in `watch.rs`:

```rust
/// Never reported by the watcher (Mkrew and users put git worktrees there). The file tree
/// still shows it, so it is not in [`ALWAYS_IGNORED`].
const WATCH_IGNORED: [&str; 1] = [".worktrees"];

/// Which changed paths of one project are reported.
// ponytail: the global `core.excludesFile` is not read (machine config would leak into
// tests); add it via `GitignoreBuilder` if users report noise it would remove.
pub struct Filter {
    root: PathBuf,
    /// The root `.gitignore` plus `.git/info/exclude` (`.gitignore` lines win, added last).
    gitignore: Gitignore,
    /// Nested `.gitignore` files by folder relative to the root, read on first use. Never
    /// re-read, like the root one: the watcher restarts with its project.
    nested: std::sync::Mutex<std::collections::HashMap<PathBuf, Gitignore>>,
}

impl Filter {
    pub fn new(root: &Path) -> Self {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
        for file in [root.join(".git/info/exclude"), root.join(".gitignore")] {
            if let Some(e) = builder.add(&file).filter(|_| file.exists()) {
                tracing::debug!(file = %file.display(), "partly invalid ignore file: {e}");
            }
        }
        let gitignore = builder.build().unwrap_or_else(|e| {
            tracing::debug!(root = %root.display(), "ignore rules dropped: {e}");
            Gitignore::empty()
        });
        Self { root: root.to_path_buf(), gitignore, nested: Default::default() }
    }

    /// Git's precedence: the deepest `.gitignore` with a matching line decides, then the
    /// root rules. `relative` is relative to the root.
    fn ignored(&self, relative: &Path, is_dir: bool) -> bool {
        let mut nested = match self.nested.lock() {
            Ok(nested) => nested,
            Err(poisoned) => poisoned.into_inner(),
        };
        for dir in relative.ancestors().skip(1) {
            if dir.as_os_str().is_empty() {
                break;
            }
            let rules = nested
                .entry(dir.to_path_buf())
                .or_insert_with(|| Gitignore::new(self.root.join(dir).join(".gitignore")).0);
            let Ok(inner) = relative.strip_prefix(dir) else { continue };
            let found = rules.matched_path_or_any_parents(inner, is_dir);
            if !found.is_none() {
                return found.is_ignore();
            }
        }
        self.gitignore.matched_path_or_any_parents(relative, is_dir).is_ignore()
    }
```

In `relative_with`: the component check becomes `if ALWAYS_IGNORED.contains(&part) || WATCH_IGNORED.contains(&part) { return None; }`, and the ignore closure becomes `let ignored = |is_dir| self.ignored(relative, is_dir);` (the rest unchanged). Update the `relative` doc comment: "…or git-ignored (root and nested `.gitignore`, `.git/info/exclude`)…, or under `.worktrees`". Check `Gitignore::new` returns `(Gitignore, Option<Error>)` and `GitignoreBuilder::add` returns `Option<Error>` in the locked `ignore` version (docs.rs); `Filter` must stay `Send` (it is moved into the debounce task) — `Mutex` keeps it `Send + Sync`.

Note on `matched_path_or_any_parents` with a nested matcher: the path passed is relative to that matcher's folder, so it never leaves its root (the root-only panic in the existing comment does not apply).

- [ ] **Step 4: Run the tests**

Run: `cargo test -j 4 -p mkrew-daemon --lib watch::tests && cargo test -j 4 -p mkrew-daemon --test daemon file_watcher`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-daemon/src/watch.rs
git commit -m "fix(daemon): watcher skips nested-gitignored, info/exclude and .worktrees paths"
```

### Task 3: Retention of logs and old events

**Files:**
- Modify: `crates/mkrew-store/src/lib.rs` (new `sessions_ended_before`, `prune_session_events`; doc of `hide_session`)
- Test: `crates/mkrew-store/src/tests.rs` (helpers `open(&dir)`, `session(project_id, started_at)`)
- Create: `crates/mkrew-daemon/src/retention.rs`
- Modify: `crates/mkrew-daemon/src/lib.rs` (`mod retention;`)
- Modify: `crates/mkrew-daemon/src/server/resume.rs:193` (`replace_session` → `pub(crate)`) and `crates/mkrew-daemon/src/server.rs` (`pub(crate) use resume::replace_session;`)
- Modify: `crates/mkrew-daemon/src/attachment.rs:14` (doc: the window is shared)

**Interfaces:**
- Consumes: `attachment::KEEP_MS: i64` (7 days); `session_log::{path, files}`; `Store::{list_projects, list_tabs}`; `replace_session(&Value, Ulid, Ulid) -> Option<Value>`.
- Produces: `Store::sessions_ended_before(&self, before: i64) -> Result<Vec<Ulid>>`; `Store::prune_session_events(&self, before: i64, kinds: &[&str]) -> Result<usize>`; `retention::PRUNED_EVENT_KINDS: [&str; 3]`; `retention::run(store: &Store, sessions_dir: &Path, now_ms: i64) -> Result<Purged, StoreError>`; `pub struct Purged { pub log_files: usize, pub events: usize }`.

- [ ] **Step 1: Write the failing store tests** in `crates/mkrew-store/src/tests.rs`:

```rust
#[test]
fn sessions_ended_before_lists_ended_sessions_hidden_or_not() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let ended = |started, ended_at| Session { ended_at, ..session(p.id, started) };
    let (old, old_hidden, recent, live) =
        (ended(1, Some(10)), ended(2, Some(20)), ended(3, Some(500)), ended(4, None));
    for s in [&old, &old_hidden, &recent, &live] {
        store.insert_session(s).unwrap();
    }
    store.hide_session(old_hidden.id).unwrap();
    let mut got = store.sessions_ended_before(100).unwrap();
    got.sort();
    let mut want = vec![old.id, old_hidden.id];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn prune_session_events_drops_only_old_rows_of_the_given_kinds() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    let v = serde_json::json!({});
    for (kind, at) in [("files_changed", 5), ("hook", 6), ("usage", 7), ("files_changed", 200)] {
        store.insert_session_event(s.id, kind, &v, at).unwrap();
    }
    assert_eq!(store.prune_session_events(100, &["files_changed", "hook"]).unwrap(), 2);
    assert_eq!(store.session_events(s.id, "usage").unwrap().len(), 1, "usage kept");
    let left = store.session_events(s.id, "files_changed").unwrap();
    assert_eq!(left.iter().map(|e| e.at).collect::<Vec<_>>(), [200]);
    assert!(store.session_events(s.id, "hook").unwrap().is_empty());
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -j 4 -p mkrew-store sessions_ended_before prune_session_events`
Expected: compile error, missing methods.

- [ ] **Step 3: Implement** in `crates/mkrew-store/src/lib.rs`, after `hide_session`:

```rust
    /// Sessions (hidden ones too) that ended before `before` (unix ms): retention candidates
    /// (SPEC §8, ADR 0056). Live sessions have no `ended_at` and never show up.
    pub fn sessions_ended_before(&self, before: i64) -> Result<Vec<Ulid>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM session WHERE ended_at IS NOT NULL AND ended_at < ?1")?;
        let ids = stmt.query_map([before], |row| row.get::<_, String>(0))?;
        ids.map(|id| {
            parse_ulid("session.id", id?)
        })
        .collect()
    }

    /// Deletes `session_event` rows of `kinds` older than `before`; returns how many.
    pub fn prune_session_events(&self, before: i64, kinds: &[&str]) -> Result<usize> {
        let mut deleted = 0;
        for kind in kinds {
            deleted += self.conn.execute(
                "DELETE FROM session_event WHERE kind = ?1 AND at < ?2",
                params![kind, before],
            )?;
        }
        Ok(deleted)
    }
```

`parse_ulid` (lib.rs ~1371) is the store's existing ULID-column parser. Update `hide_session`'s doc: "…its row, turns and usage stay; its log and old events follow retention (ADR 0056)."

- [ ] **Step 4: Run the store tests**

Run: `cargo test -j 4 -p mkrew-store`
Expected: all pass.

- [ ] **Step 5: Write the failing retention test** — create `crates/mkrew-daemon/src/retention.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{Session, SessionKind, SessionState, StateSource};

    const DAY: i64 = 24 * 60 * 60 * 1000;

    #[test]
    fn run_purges_old_logs_but_keeps_live_and_tab_shown_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let project = store.add_project(dir.path(), None).unwrap();
        let now = 100 * DAY;
        let make = |ended_at: Option<i64>| {
            let mut s = test_session(project.id);
            s.ended_at = ended_at;
            s.state = if ended_at.is_some() { SessionState::Exited } else { SessionState::Working };
            store.insert_session(&s).unwrap();
            s.id
        };
        let old = make(Some(now - 8 * DAY));
        let old_hidden = make(Some(now - 30 * DAY));
        let old_in_tab = make(Some(now - 30 * DAY));
        let recent = make(Some(now - DAY));
        let live = make(None);
        let gone = make(Some(now - 30 * DAY)); // never had a log
        store.hide_session(old_hidden).unwrap();
        let tab = store.create_tab(project.id, "T", None).unwrap();
        store
            .set_tab_layout(tab.id, &serde_json::json!({"session": old_in_tab.to_string()}))
            .unwrap();
        for id in [old, old_hidden, old_in_tab, recent, live] {
            let log = session_log::path(&sessions_dir, id);
            std::fs::write(&log, "x").unwrap();
            let mut rotated = log.into_os_string();
            rotated.push(".1");
            std::fs::write(rotated, "y").unwrap();
        }
        let v = serde_json::json!({});
        store.insert_session_event(live, "files_changed", &v, now - 8 * DAY).unwrap();
        store.insert_session_event(live, "usage", &v, now - 8 * DAY).unwrap();

        let purged = run(&store, &sessions_dir, now).unwrap();

        let has_log = |id| !session_log::files(&session_log::path(&sessions_dir, id)).is_empty();
        assert!(!has_log(old) && !has_log(old_hidden), "old logs purged");
        assert!(has_log(old_in_tab) && has_log(recent) && has_log(live), "kept");
        assert!(!has_log(gone));
        assert_eq!(purged.log_files, 4, "two files each of `old` and `old_hidden`");
        assert_eq!(purged.events, 1);
        assert_eq!(store.session_events(live, "usage").unwrap().len(), 1);
        // `session.history` of a purged session: the row is still there, `files` is empty.
        assert!(store.get_session(old_hidden).is_ok());
        assert_eq!(run(&store, &sessions_dir, now).unwrap().log_files, 0, "idempotent");
    }

    fn test_session(project_id: Ulid) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "fake".into(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            pinned: false,
            position: 0.0,
            native_session_id: None,
            state: SessionState::Exited,
            state_source: StateSource::Daemon,
            state_changed_at: 1,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 1,
            ended_at: None,
            plan_path: None,
        }
    }
}
```
(The literal mirrors `server/resume.rs` tests' `session(…)`; if `Session` gained a field since, the compiler names it.)

Add `mod retention;` to `crates/mkrew-daemon/src/lib.rs`.

- [ ] **Step 6: Run it to see it fail**

Run: `cargo test -j 4 -p mkrew-daemon --lib retention`
Expected: compile error (`run`, `Purged` missing).

- [ ] **Step 7: Implement** `retention.rs` above the tests:

```rust
//! Retention (SPEC §8, ADR 0056): the log files of sessions that ended more than
//! [`attachment::KEEP_MS`] ago and that no tab shows, and older `files_changed`, `hook` and
//! `edited` events, are deleted. Session rows, turns and `usage` stay (resume, usage totals).

use std::collections::HashSet;
use std::path::Path;

use mkrew_store::{Store, StoreError};
use ulid::Ulid;

use crate::{attachment, server::replace_session, session_log};

/// Event kinds pruned by age. `usage` is not: usage totals read every row (`since` = 0).
pub const PRUNED_EVENT_KINDS: [&str; 3] = ["files_changed", "hook", "edited"];

/// What one run deleted.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Purged {
    pub log_files: usize,
    pub events: usize,
}

/// One retention pass at `now_ms`. A file that cannot be deleted is logged and retried next
/// time.
// ponytail: one fixed window (7 days, shared with attachments) and no total-size cap; add a
// `[storage]` setting or a cap if users need longer history or still run out of disk.
pub fn run(store: &Store, sessions_dir: &Path, now_ms: i64) -> Result<Purged, StoreError> {
    let before = now_ms - attachment::KEEP_MS;
    let ended = store.sessions_ended_before(before)?;
    let shown = shown_in_tabs(store, &ended)?;
    let mut purged = Purged::default();
    for id in ended.iter().filter(|id| !shown.contains(id)) {
        for file in session_log::files(&session_log::path(sessions_dir, id)) {
            match std::fs::remove_file(&file) {
                Ok(()) => purged.log_files += 1,
                Err(e) => tracing::warn!(file = %file.display(), "session log not deleted: {e}"),
            }
        }
    }
    purged.events = store.prune_session_events(before, &PRUNED_EVENT_KINDS)?;
    Ok(purged)
}

/// Those of `ids` that a tab of any project (archived ones too) still shows.
fn shown_in_tabs(store: &Store, ids: &[Ulid]) -> Result<HashSet<Ulid>, StoreError> {
    let mut shown = HashSet::new();
    for project in store.list_projects()? {
        for tab in store.list_tabs(project.id)? {
            shown.extend(
                ids.iter().filter(|id| replace_session(&tab.layout, **id, **id).is_some()),
            );
        }
    }
    Ok(shown)
}
```

Make `replace_session` `pub(crate)` in `server/resume.rs` and add `pub(crate) use resume::replace_session;` in `server.rs` (next to its `mod resume;`). Check `mkrew_store` exports `StoreError` and `Store` at the crate root (it does for `Store`; adjust the import if `StoreError` lives elsewhere). In `attachment.rs:14` change the doc to "Attachments, session logs and old events (`retention`) are kept this long after the session ended: 7 days."

- [ ] **Step 8: Run the tests**

Run: `cargo test -j 4 -p mkrew-daemon --lib retention && cargo test -j 4 -p mkrew-daemon --lib resume`
Expected: pass.

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-store/src/lib.rs crates/mkrew-store/src/tests.rs crates/mkrew-daemon/src/retention.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/server/resume.rs crates/mkrew-daemon/src/server.rs crates/mkrew-daemon/src/attachment.rs
git commit -m "feat(daemon): delete session logs and noisy events 7 days after a session ended"
```

### Task 4: Run retention at startup and hourly; VACUUM when sparse

**Files:**
- Modify: `crates/mkrew-store/src/lib.rs` (new `vacuum_if_sparse`)
- Test: `crates/mkrew-store/src/tests.rs`
- Modify: `crates/mkrew-daemon/src/lib.rs` (`run`, ~lines 58-83: after `mark_stale_sessions_exited`)
- Modify: `crates/mkrew-daemon/src/backup.rs:28-60` (`start` takes the sessions dir; the tick runs retention)
- Modify: `crates/mkrew-daemon/src/server.rs` (`Daemon::serve`, the `backup::start(…)` call ~line 130)

**Interfaces:**
- Consumes: `retention::run` (Task 3).
- Produces: `Store::vacuum_if_sparse(&self) -> Result<bool>`; `backup::start(hub, dir, attachments, sessions: PathBuf)`.

- [ ] **Step 1: Write the failing test** in `crates/mkrew-store/src/tests.rs`:

```rust
#[test]
fn vacuum_if_sparse_shrinks_the_file_only_when_mostly_free() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let s = session(p.id, 1);
    store.insert_session(&s).unwrap();
    let big = serde_json::json!({ "paths": ["x".repeat(2000)] });
    for at in 0..2000 {
        store.insert_session_event(s.id, "files_changed", &big, at).unwrap();
    }
    assert!(!store.vacuum_if_sparse().unwrap(), "nothing free yet");
    store.prune_session_events(i64::MAX, &["files_changed"]).unwrap();
    store.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    let before = std::fs::metadata(dir.path().join("mkrew.db")).unwrap().len();
    assert!(store.vacuum_if_sparse().unwrap());
    // WAL mode: the rewritten pages reach the main file at the checkpoint.
    store.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    let after = std::fs::metadata(dir.path().join("mkrew.db")).unwrap().len();
    assert!(after * 4 < before, "{before} -> {after}");
    assert!(!store.vacuum_if_sparse().unwrap(), "compact now");
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -j 4 -p mkrew-store vacuum_if_sparse`
Expected: compile error.

- [ ] **Step 3: Implement** in `crates/mkrew-store/src/lib.rs`:

```rust
    /// Rewrites the file (`VACUUM`) when over a quarter of its pages are free (ADR 0056).
    /// Freed pages are reused by later inserts, so only a large prune (the first retention
    /// run) needs this; it runs at daemon startup, before any client connects.
    pub fn vacuum_if_sparse(&self) -> Result<bool> {
        let pages: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let free: i64 = self.conn.query_row("PRAGMA freelist_count", [], |r| r.get(0))?;
        if free * 4 <= pages {
            return Ok(false);
        }
        self.conn.execute_batch("VACUUM")?;
        Ok(true)
    }
```

- [ ] **Step 4: Run the store tests**

Run: `cargo test -j 4 -p mkrew-store`
Expected: pass.

- [ ] **Step 5: Startup** — `crates/mkrew-daemon/src/lib.rs` `run`, right after `store.mark_stale_sessions_exited()`:

```rust
    // Retention before anyone connects, then reclaim what it freed (ADR 0056).
    match retention::run(&store, &paths.sessions_dir(), server::now_ms()) {
        Ok(purged) => tracing::info!(?purged, "retention at startup"),
        Err(e) => tracing::warn!("retention at startup failed: {e}"),
    }
    match store.vacuum_if_sparse() {
        Ok(true) => tracing::info!("database compacted"),
        Ok(false) => {}
        Err(e) => tracing::warn!("database not compacted: {e}"),
    }
```

(Use whatever `now_ms` path compiles from `lib.rs`: `server::now_ms` is `pub(crate)` or make it so.)

- [ ] **Step 6: Hourly** — `backup.rs`: `pub fn start(hub: Arc<Hub>, dir: PathBuf, attachments: PathBuf, sessions: PathBuf)`; inside the `spawn_blocking` closure, after `attachment::cleanup(…)`:

```rust
                let retained = crate::retention::run(&hub.store(), &sessions, now);
```

return it in the tuple and log it next to the other results (`Ok(p) if p != Purged::default() → info!(?p, "retention")`, `Err(e) → warn!`). Clone `sessions` per tick like `dir`/`attachments`. Update the doc comment of `start` ("…and runs retention (ADR 0056)"). In `server.rs` `Daemon::serve`, pass `self.paths.sessions_dir()` as the new argument.

- [ ] **Step 7: Run the daemon tests**

Run: `cargo test -j 4 -p mkrew-daemon --lib backup && cargo test -j 4 -p mkrew-daemon --test daemon basics`
Expected: pass (startup still answers `hello`).

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-store/src/lib.rs crates/mkrew-store/src/tests.rs crates/mkrew-daemon/src/lib.rs crates/mkrew-daemon/src/backup.rs crates/mkrew-daemon/src/server.rs
git commit -m "feat(daemon): run retention at startup and hourly, compact a sparse database"
```

### Task 5: ADR 0056, SPEC §5/§8, verify, size report

**Files:**
- Create: `docs/decisions/0056-retention-of-logs-and-events.md`
- Modify: `docs/decisions/0019-archive-workspaces-hide-sessions.md` (Status line: "amended by ADR 0056")
- Modify: `docs/SPEC.md` §5 (schema notes near `session_event`, ~line 208+), §8 (startup `:620`, session logs `:654`), §7 `session.history` row (`:532`)

- [ ] **Step 1: ADR** — `docs/decisions/0056-retention-of-logs-and-events.md`:

```markdown
# 0056 — Session logs and noisy events are deleted 7 days after a session ended

- Date: 2026-10-14
- Status: accepted (amends ADR 0019 "history stays"; SPEC §5, §8)
- Milestone: none (bug fix, user report 2026-10-14)

## Context

In 7 days the data dir grew to 570 MB of session logs (255 files) and a 23 MB database, 14 MB
of it `files_changed` events: each batch was stored once per live session, and the watcher
reported nested-gitignored files, `.git/info/exclude` matches and `.worktrees/`. Nothing deleted
old logs; ADR 0019 kept hidden sessions' logs and events forever.

## Decision

- Retention window: 7 days after `ended_at`, the attachments' window (`attachment::KEEP_MS`).
- At daemon startup and in the hourly backup tick: delete `<id>.log` and `<id>.log.1` of every
  session that ended before the window and that no tab shows; delete `files_changed`, `hook` and
  `edited` events older than the window.
- Kept: session rows (resume, `get_session`), `native_session_id`, turns, `usage` events (usage
  totals), comments, tabs. `session.history` of a purged session answers `files: []`.
- `files_changed` is stored once per batch, on the project's newest live session.
- The watcher also honours nested `.gitignore` files and `.git/info/exclude`, and skips
  `.worktrees/`. Markdown stays reported (doc review).
- Startup runs `VACUUM` when free pages exceed a quarter of the database.

## Alternatives

- `auto_vacuum = INCREMENTAL`: needs a full `VACUUM` on existing databases first and periodic
  `incremental_vacuum`; freed pages are reused anyway. Rejected.
- A `[storage]` setting or a total-size cap: not needed yet (`ponytail:` in `retention.rs`).
- A project-level `files_changed` table: a migration for the same `doc.list` result. Rejected.

## Consequences

- History of a session older than 7 days is gone unless its pane is still in a tab.
- First startup after the upgrade deletes most old logs and compacts the database (seconds).
```

- [ ] **Step 2: SPEC** — edit in place:
  - §8 `- **Session logs:**` line → `- **Session logs:** <data>/mkrew/sessions/<session_id>.log, rotated at 5 MB; deleted 7 days after the session ended unless a tab still shows it (ADR 0056).`
  - §8 startup bullet: append `; run retention (ADR 0056) and VACUUM the database when over a quarter of it is free`.
  - §8 add `- **Retention:** hourly (and at startup): logs as above; session_event rows of kind files_changed, hook, edited older than 7 days. Session rows, turns and usage events stay.`
  - §8 watcher rules (where `.git`, `node_modules`, `target` are listed): add nested `.gitignore`, `.git/info/exclude`, `.worktrees/`; one `files_changed` row per batch on the newest live session.
  - §5 next to `session_event`: `-- files_changed/hook/edited rows are pruned after 7 days (ADR 0056)`.
  - §7 `session.history` row: append `; empty after retention deleted the log (ADR 0056)`.
  - ADR 0019 status line: append `; amended by ADR 0056 (logs and events expire)`.

- [ ] **Step 3: Full verify**

Run: `sh scripts/verify.sh`
Expected: `verify: OK`.

- [ ] **Step 4: Commit**

```bash
git add docs/decisions/0056-retention-of-logs-and-events.md
git add -p docs/SPEC.md docs/decisions/0019-archive-workspaces-hide-sessions.md
git commit -m "docs(spec): retention of session logs and events (ADR 0056)"
```

- [ ] **Step 5 (optional): Size report on a copy.** Never start a daemon on the real or copied data dir (it would resume agents). Copy only the database, apply the same SQL with `sqlite3`, and `stat` the real logs read-only (PowerShell):

```powershell
$t = (New-Item -ItemType Directory "$env:TEMP\mkrew-retention-$(Get-Date -Format yyyyMMddHHmmss)").FullName
Copy-Item "$env:APPDATA\mkrew\mkrew.db" "$t\mkrew.db"
$cut = [DateTimeOffset]::UtcNow.AddDays(-7).ToUnixTimeMilliseconds()
$ids = sqlite3 "$t\mkrew.db" "select id from session where ended_at is not null and ended_at < $cut"
$shown = sqlite3 "$t\mkrew.db" "select layout from terminal_tab" | Out-String
$purge = $ids | Where-Object { $shown -notmatch $_ }
$logs = Get-ChildItem "$env:APPDATA\mkrew\sessions" -Filter *.log*
$gone = ($logs | Where-Object { $purge -contains ($_.Name -replace '\.log(\.1)?$','') } | Measure-Object Length -Sum).Sum
"logs now: {0:N1} MB, deleted: {1:N1} MB" -f (($logs | Measure-Object Length -Sum).Sum/1MB), ($gone/1MB)
sqlite3 "$t\mkrew.db" "delete from session_event where kind in ('files_changed','hook','edited') and at < $cut; vacuum;"
"db: {0:N1} MB -> {1:N1} MB" -f ((Get-Item "$env:APPDATA\mkrew\mkrew.db").Length/1MB), ((Get-Item "$t\mkrew.db").Length/1MB)
Remove-Item -Recurse $t
```

Expected: two lines with sizes; put them in the final report as "expected after one startup". Duplicate `files_changed` rows newer than 7 days stay until they age out (no backfill dedup). Skip this step if `sqlite3` is not on `PATH` or C: has under 1 GB free.

Final report: test names that cover each acceptance item, and the size numbers if Step 5 ran.
