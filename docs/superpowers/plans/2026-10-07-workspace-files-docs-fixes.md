# Workspace, Files and Docs Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix or add six things the user reported on 2026-10-07: removing a workspace and removing an agent row; a color per workspace; full-width selectable rows in the Files tree; find-in-file in the Files viewer; a readable Docs preview that draws Mermaid diagrams; a filter for the Docs file list.

**Architecture:** Removal was never built. There is no protocol request, no store method and no UI for it. It becomes `project.archive` and a new `session.remove`:
- `project.archive` is already named in SPEC §7 and half-wired. `project.archived` is read everywhere, and the sidebar, palette and inbox already hide archived projects.
- `session.remove` hides a session row for good (migration adds `session.hidden`). History is kept, as ADR 0004 decided for task deletion.

The other five items:
- **Workspace color:** a new nullable `project.color` that holds a palette name. The UI maps each name to a per-appearance theme token.
- **Files rows:** a GPUI/taffy layout fact. A `display: flex` root item in a `uniform_list` shrinks to its content, so each row needs `.w_full()`.
- **Find-in-file:** a small pure `text_find` module, drawn by the existing `preview::code_view`.
- **Docs preview:**
  - doc typography tokens;
  - monospace inline code;
  - syntect-highlighted code blocks;
  - Mermaid diagrams, drawn to SVG by the pure-Rust `mermaid-rs-renderer` crate and shown with `gpui::img`.

  All of this is parsed once, off the UI thread.
- **Docs filter:** reuses `palette::fuzzy` locally.

**Tech Stack:** Rust workspace, GPUI (`gpui-pre =0.3.7`), SQLite (rusqlite, bundled), syntect 5.3, pulldown-cmark 0.13.4, new `mermaid-rs-renderer 0.3.1` (`default-features = false`). Tests: `cargo test`, daemon end-to-end tests with `mkrew-fakeagent`.

**Spec:** `docs/SPEC.md`, sections cited per task: §4, §5, §7, §10, §12, §12A, §13, §14. There is no separate design doc. This plan comes from the user's request of 2026-10-07 plus the investigation summarized under each task. Read the cited SPEC sections along with each task.

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md).
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14): "No hard-coded values in components."
- SPEC §14 design check: "status never conveyed by color alone". The workspace color is decoration only.
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- Use `thiserror` in libraries, `anyhow` with context in binaries, and `tracing` for logs.
- No test may call a real agent or the network. Use `mkrew-fakeagent`.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change.
  - This plan writes ADRs 0018 (removal), 0019 (workspace colors) and 0020 (Docs preview).
  - **Before creating each one, list `docs/decisions/`.** Another plan in flight (`2026-10-07-resume-omp-copilot-panes.md`) claims 0017.
  - If a number is taken, use the next free one and fix every reference in this plan's edits.
- Migrations are append-only (`-- Never edit` headers).
  - This plan adds `0011_session_hidden.sql` and `0012_project_color.sql`.
  - **Before creating each one, list `crates/mkrew-store/migrations/`.** `0010_session_interrupted.sql` already exists, uncommitted, from the resume plan.
  - If a number is taken, use the next free one.
- `PROTOCOL_VERSION` goes 24 → 25 once, in Task 1 (new requests). Task 3 adds an optional field under the same version.
- One new dependency only: `mermaid-rs-renderer = { version = "0.3.1", default-features = false }` (Task 8).
  - SPEC §4 says: "When adding a crate, check its latest version and current API". Checked on 2026-10-07: 0.3.1 is the latest version.
  - A local spike compiled it, and gpui's `usvg 0.46` parsed its SVG.
- GPUI rules:
  - Set `.hover()` at most once per element. `button()`, `quiet_button()`, `menu_item()` and `row_action()` already set it.
  - An element with `track_focus` grabs focus on ANY mouse down, before `on_click`.
- Daemon gotcha: `match self.store().x() { … self.store() … }` deadlocks, because the guard lives through the arms. Bind the result in a block first, as `task_delete` does.
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use. Do not run `git commit`.
- Leave the existing uncommitted human edits alone:
  - `.github/workflows/*`
  - the x11/wayland lines in `crates/mkrew-ui/Cargo.toml`
  - `docs/BUILD_VI.md`
  - `packaging/`
  - `scripts/package-setup.ps1`
- Leave the uncommitted work of the resume plan alone too:
  - `agents/*.toml`
  - `crates/mkrew-cli/*`
  - `crates/mkrew-core/*`
  - migration 0010
  - the edits in `crates/mkrew-store/src/{lib,tests}.rs` and `crates/mkrew-daemon/tests/daemon.rs`

  Edit those shared files only where this plan says, and find code **by name**, not by line number.
- Task 8 must change `Cargo.lock`. That change is expected.
- **The Run button plan (`2026-10-07-run-button.md`) lands before this one** and replaces the dev server (`dev_command`, `dev_autostart`, `dev_server_item`, `ProjectDevStart/Stop`) with `run_commands`. Before each task, re-check every name this plan uses that it may have renamed or removed (`ProjectUpdateParams` fields, `ProjectChanges`, `PROJECT_COLUMNS`/project `SELECT`s, `render_workspace_menu` items, migration and ADR numbers, `PROTOCOL_VERSION`). Adapt the code to the current names; bump `PROTOCOL_VERSION` by one from whatever it is.
- `crates/mkrew-ui/src/main.rs` is ~15k lines. Never read it whole: grep for the named function, then read a range. Line numbers below are approximate (as of 2026-10-07).
- Verify on Windows with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`.
  - If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run instead, in order:
    1. `cargo fmt --all -- --check`
    2. `cargo clippy --workspace --all-targets -- -D warnings`
    3. `cargo test --workspace -j 4`
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions.
- `cargo insta` is not installed. Write accepted `.snap` files by hand (none are expected in this plan).

## Review Focus

1. **Removing the workspace that is on screen, or one with a running dev server or agents.**
   - Its sessions must stop, and the sidebar must move to another workspace.
   - Adding the same folder later must bring it back with its tasks and history. It must not fail with "a project already exists".
   - Pinned by Task 1: `archived_projects_come_back_when_their_folder_is_added_again` (store) and `archived_workspaces_stop_their_sessions_and_come_back_when_added_again` (daemon).
2. **Removing a running agent row.**
   - The process must stop, with no orphaned PTY.
   - The row must not come back on the next refresh or daemon restart.
   - Its output log must stay readable.
   - Pinned by Task 1: `hidden_sessions_leave_the_list_but_keep_their_history` (store) and `removed_sessions_stop_and_leave_the_list` (daemon).
3. **Find with Vietnamese or other non-ASCII text**, where case folding changes byte lengths (`Ệ`/`ệ`, `İ`).
   - Match ranges must sit on char boundaries. Otherwise GPUI's `StyledText` debug-asserts and panics.
   - Pinned by Task 5 `matches_ignore_case_and_keep_byte_ranges_of_the_line`.
4. **The opened file changes while the find bar is open, or the query stops matching.** For example, an agent edits the file and it is re-read.
   - The count and the current match must follow.
   - "No matches" and the empty state must be clear.
   - Pinned by Task 5 `find_follows_a_reread_file` and `a_new_search_stays_at_or_after_the_current_match`.
5. **A ```mermaid block that does not parse, or uses a diagram type the renderer lacks.**
   - The rest of the doc must still render.
   - That block must show its source plus the reason.
   - Pinned by Task 8 `flowcharts_draw_in_both_appearances_and_bad_sources_say_why`.

---

## File Structure

| File | Change | Responsibility |
| --- | --- | --- |
| `crates/mkrew-protocol/src/{lib,message}.rs` | modify | T1: `project.archive`, `session.remove`, v25. T3: `Project.color`, `ProjectUpdateParams.color`, `WORKSPACE_COLORS` |
| `crates/mkrew-store/migrations/0011_session_hidden.sql` | create | T1: `session.hidden` |
| `crates/mkrew-store/migrations/0012_project_color.sql` | create | T3: `project.color` |
| `crates/mkrew-store/src/{lib,tests}.rs` | modify | T1: `archive_project`, revive on re-add, `hide_session`, `PROJECT_COLUMNS`. T3: color |
| `crates/mkrew-daemon/src/server.rs` | modify | T1: `project_archive`, `session_remove`. T3: color validation |
| `crates/mkrew-daemon/tests/daemon.rs` | modify | end-to-end tests: removal (T1), color (T3) |
| `crates/mkrew-ui/src/sidebar.rs` | modify | T2: `remove_session`. T3: `WorkspaceRow.color` |
| `crates/mkrew-ui/src/workspace_menu.rs` | modify | T2: `REMOVE_ANSWERS`. T3: `set_color` |
| `crates/mkrew-ui/src/theme.rs` | modify | T3: workspace colors. T7: `doc` typography. T8: `dark`, `DIAGRAM_FONT` |
| `crates/mkrew-ui/src/text_find.rs` | create | T5: find-in-file query and matches |
| `crates/mkrew-ui/src/files_view.rs` | modify | T5: `find` state follows re-reads |
| `crates/mkrew-ui/src/preview.rs` | modify | T5: match painting in `code_line`/`code_view`. T7: doc typography, highlighted code blocks. T8: diagrams |
| `crates/mkrew-ui/src/keymap.rs`, `palette.rs` | modify | `Command::Find` also in Files (T5) and Docs (T6) |
| `crates/mkrew-ui/src/docs.rs` | modify | T6: list filter. T7: `Shown` highlighted code, parsed off-thread. T8: diagrams |
| `crates/mkrew-ui/src/markdown.rs` | modify | T7: `walk` over nested blocks |
| `crates/mkrew-ui/src/highlight.rs` | modify | T7: `fenced` code block highlighting |
| `crates/mkrew-ui/src/diagram.rs` | create | T8: Mermaid → themed SVG images |
| `crates/mkrew-ui/src/lib.rs`, `Cargo.toml` | modify | T5: `pub mod text_find`. T8: `pub mod diagram` + dependency |
| `crates/mkrew-ui/src/main.rs` | modify | T2: removal UI. T3: color dot + swatches. T4: full-width rows. T5: find bar. T6: docs filter. T7: preview layout + off-thread parse |
| `crates/mkrew-ui/tests/docs_view.rs` | modify | T7: `Docs::read` takes a parsed `Shown` |
| `docs/SPEC.md`, `docs/decisions/0018-*.md`, `0019-*.md`, `0020-*.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md` | modify/create | spec, ADRs, manual checks, notes |

---

### Task 1: `project.archive` and `session.remove` in protocol, store and daemon

**Current state (verified):** The user reports "cannot remove a workspace or an agent". The feature was never built:
- The `Request` enum (`crates/mkrew-protocol/src/message.rs` ~42-217) has no archive or remove request.
- The store has no `archive_project`, `delete_project` or hide-session method. Nothing ever writes `project.archived = 1`.
- SPEC §7 lists `project.list/add/update/archive`. So `archive` was specified but never implemented.

A hard `DELETE` is not an option:
- `PRAGMA foreign_keys` is ON (store `open`).
- `task`, `session`, `terminal_tab`, `doc_comment`, `turn` and `session_event` all reference `project` or `session` without `ON DELETE`.
- ADR 0004 already rejected deleting session history.

Decision:
- Archive the workspace: stop its live sessions and hide it. Re-adding the folder un-archives it with its tasks and history.
- Hide a session row for good with a new `session.hidden` column. Stop the session first if it is live. Its log, turns and comments stay.

**Files:**
- Modify: `crates/mkrew-protocol/src/lib.rs:16` (version)
- Modify: `crates/mkrew-protocol/src/message.rs` (`Request` ~42-217; round-trip request list ~1500-1650)
- Create: `crates/mkrew-store/migrations/0011_session_hidden.sql`
- Modify: `crates/mkrew-store/src/lib.rs`: `MIGRATIONS`, `add_project_with_dev`, `list_projects`, `get_project`, `list_sessions`, and new `archive_project`, `project_at`, `project_where`, `hide_session`
- Modify: `crates/mkrew-store/src/tests.rs` (two new tests)
- Modify: `crates/mkrew-daemon/src/server.rs`: dispatch (~241-330), new `project_archive`, new `session_remove` (near `session_kill`, ~993)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (two new tests)
- Create: `docs/decisions/0018-archive-workspaces-hide-sessions.md`
- Modify: `docs/SPEC.md` §5, §7

**Interfaces:**
- Produces (protocol):
  - `Request::ProjectArchive(ProjectRef)` (`"project.archive"`): returns the `Project` with `archived: true`, also announced as `Event::ProjectUpdated`.
  - `Request::SessionRemove(SessionRef)` (`"session.remove"`): returns `null`.
  - `PROTOCOL_VERSION = 25`.
- Produces (store):
  - `Store::archive_project(&self, id: Ulid) -> Result<Project>`
  - `Store::hide_session(&self, id: Ulid) -> Result<()>`
  - `add_project*` un-archives an archived project at the same path and returns it.
  - `list_sessions` leaves hidden sessions out. `get_session` still finds them.
- Consumes: `ProjectRef { project_id }` (protocol `files.rs`), `SessionRef { session_id }`, `Command::Kill { force: false }`, `store_outcome`, `ok`.

- [ ] **Step 1: Write the failing store tests.** Append to `crates/mkrew-store/src/tests.rs`:

```rust
#[test]
fn archived_projects_come_back_when_their_folder_is_added_again() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), Some("demo")).unwrap();
    let task = store.create_task(p.id, "t", "", None, None, false).unwrap();

    let archived = store.archive_project(p.id).unwrap();
    assert!(archived.archived);
    assert!(store.get_project(p.id).unwrap().archived);
    // Adding the folder again un-archives the same project: settings and tasks stay.
    let back = store.add_project(dir.path(), None).unwrap();
    assert_eq!(
        (back.id, back.name.as_str(), back.archived),
        (p.id, "demo", false)
    );
    assert_eq!(store.get_task(task.id).unwrap(), task);
    // Only an archived one comes back; an active one is still a duplicate.
    assert!(matches!(
        store.add_project(dir.path(), None),
        Err(StoreError::DuplicateProject(_))
    ));
    assert!(matches!(
        store.archive_project(Ulid::new()),
        Err(StoreError::ProjectNotFound(_))
    ));
}

#[test]
fn hidden_sessions_leave_the_list_but_keep_their_history() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    let mut gone = session(p.id, 1);
    gone.state = SessionState::Exited;
    store.insert_session(&gone).unwrap();
    let kept = session(p.id, 2);
    store.insert_session(&kept).unwrap();
    store.start_turn(gone.id, 1, 1, None).unwrap();

    store.hide_session(gone.id).unwrap();
    assert_eq!(store.list_sessions(Some(p.id)).unwrap(), vec![kept.clone()]);
    assert_eq!(store.list_sessions(None).unwrap(), vec![kept]);
    assert_eq!(store.get_session(gone.id).unwrap(), gone);
    assert_eq!(store.list_turns(gone.id).unwrap().len(), 1);
    assert!(matches!(
        store.hide_session(Ulid::new()),
        Err(StoreError::SessionNotFound(_))
    ));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mkrew-store archived_projects_come_back hidden_sessions_leave`
Expected: FAIL to compile with "no method named `archive_project`" / "`hide_session`".

- [ ] **Step 3: Migration.** List `crates/mkrew-store/migrations/` first (see Global Constraints). Create `0011_session_hidden.sql`:

```sql
-- SPEC §5, ADR 0018. Never edit: schema changes go in a new numbered migration.
ALTER TABLE session ADD COLUMN hidden INTEGER NOT NULL DEFAULT 0; -- removed from the sidebar (session.remove); history kept
```

Append `include_str!("../migrations/0011_session_hidden.sql"),` as the last entry of `MIGRATIONS`.

- [ ] **Step 4: Store code.** All edits below are in `crates/mkrew-store/src/lib.rs`.

**(a)** Above `impl Store`, add one column list for every project `SELECT` (Task 3 extends it):

```rust
/// Columns of [`RawProject::from_row`], in its order.
const PROJECT_COLUMNS: &str = "id, name, path, dev_command, default_agent, review_globs, \
                               created_at, archived, base_ref, dev_autostart";
```

**(b)** Replace the bodies of `list_projects` and `get_project`. Add `project_at`, `project_where` and `archive_project` next to them:

```rust
    /// All projects (archived ones too: clients hide them), oldest first.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {PROJECT_COLUMNS} FROM project ORDER BY created_at, id"
        ))?;
        let rows = stmt.query_map([], RawProject::from_row)?;
        rows.map(|r| r?.into_project()).collect()
    }

    pub fn get_project(&self, id: Ulid) -> Result<Project> {
        self.project_where("id = ?1", &id.to_string())?
            .ok_or(StoreError::ProjectNotFound(id))
    }

    /// The project at `path` (as stored: canonical).
    fn project_at(&self, path: &str) -> Result<Option<Project>> {
        self.project_where("path = ?1", path)
    }

    fn project_where(&self, condition: &str, value: &str) -> Result<Option<Project>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {PROJECT_COLUMNS} FROM project WHERE {condition}"
        ))?;
        let mut rows = stmt.query_map([value], RawProject::from_row)?;
        rows.next().map(|row| row?.into_project()).transpose()
    }

    /// Archives the project (SPEC §7 `project.archive`, ADR 0018): clients leave it out; its
    /// tasks, sessions and comments stay. Adding its folder again un-archives it.
    pub fn archive_project(&self, id: Ulid) -> Result<Project> {
        let mut project = self.get_project(id)?;
        project.archived = true;
        self.conn.execute(
            "UPDATE project SET archived = 1 WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(project)
    }
```

**(c)** In `add_project_with_dev`, replace the `UNIQUE` arm of `match inserted`:

```rust
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                // A removed (archived) workspace comes back with its tasks and history.
                let revived = self.conn.execute(
                    "UPDATE project SET archived = 0 WHERE path = ?1 AND archived = 1",
                    [&project.path],
                )?;
                match self.project_at(&project.path)? {
                    Some(project) if revived > 0 => Ok(project),
                    _ => Err(StoreError::DuplicateProject(project.path)),
                }
            }
```

**(d)** Add `hide_session` after `get_session`:

```rust
    /// Leaves the session out of [`Store::list_sessions`] from now on (SPEC §7
    /// `session.remove`, ADR 0018); its row, events, turns and log stay as history.
    pub fn hide_session(&self, id: Ulid) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE session SET hidden = 1 WHERE id = ?1",
            [id.to_string()],
        )?;
        if changed == 0 {
            return Err(StoreError::SessionNotFound(id));
        }
        Ok(())
    }
```

**(e)** In `list_sessions`:
- Change `FROM session WHERE ?1 IS NULL OR project_id = ?1` to `FROM session WHERE (?1 IS NULL OR project_id = ?1) AND hidden = 0`.
- Change its doc comment to `/// Sessions not removed (\`hidden\`), oldest first.` Read the current comment first, and keep any other facts it states.

- [ ] **Step 5: Run the store tests**

Run: `cargo test -p mkrew-store`
Expected: PASS. This covers all tests: the two new ones and the existing ones.

- [ ] **Step 6: Write the failing daemon tests.** Append to `crates/mkrew-daemon/tests/daemon.rs`. Add `SessionHistory` to the `mkrew_protocol` import list if it is missing.

```rust
#[test]
fn archived_workspaces_stop_their_sessions_and_come_back_when_added_again() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    client.wait_state(spawned.session_id, SessionState::NeedsInput);

    let archive = |project_id| Request::ProjectArchive(ProjectRef { project_id });
    let archived: Project = client.call(archive(project.id)).into_result().unwrap();
    assert!(archived.archived);
    client.wait_event(&Event::ProjectUpdated(archived.clone()));
    client.wait_state(spawned.session_id, SessionState::Exited);
    // Still listed (clients hide archived projects).
    let listed: Vec<Project> = client.call(Request::ProjectList).into_result().unwrap();
    assert_eq!(listed, vec![archived]);

    // Adding the folder again brings back the same workspace.
    let back = client.add_project(&env.repo());
    assert_eq!((back.id, back.archived), (project.id, false));
    client.wait_event(&Event::ProjectUpdated(back));
    assert_eq!(
        error_code(client.call(archive(Ulid::new()))),
        ErrorCode::NotFound
    );
    client.stop();
}

#[test]
fn removed_sessions_stop_and_leave_the_list() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    let remove = |session_id| Request::SessionRemove(SessionRef { session_id });
    client.call(remove(id)).into_result::<()>().unwrap();
    client.wait_event_where(|e| matches!(e, Event::SessionExited(x) if x.session_id == id));
    let listed: Vec<Session> = client
        .call(Request::SessionList(SessionListParams::default()))
        .into_result()
        .unwrap();
    assert!(listed.iter().all(|s| s.id != id), "{listed:?}");
    // Its output log stays readable.
    let history: SessionHistory = client
        .call(Request::SessionHistory(SessionRef { session_id: id }))
        .into_result()
        .unwrap();
    assert!(!history.files.is_empty());
    assert_eq!(
        error_code(client.call(remove(Ulid::new()))),
        ErrorCode::NotFound
    );
    client.stop();
}
```

- [ ] **Step 7: Run them to verify they fail**

Run: `cargo test -p mkrew-daemon --test daemon archived_workspaces removed_sessions`
Expected: FAIL to compile with "no variant `ProjectArchive`" / "`SessionRemove`".

- [ ] **Step 8: Protocol.** In `crates/mkrew-protocol/src/lib.rs`, set `pub const PROTOCOL_VERSION: u32 = 25;`.

In `message.rs` `Request`, add after `ProjectDevStop(ProjectRef)`:

```rust
    /// Returns the [`Project`] (`archived: true`), also announced as `project.updated`: its
    /// live sessions are asked to stop and clients hide it; `project.add` of its folder
    /// brings it back (SPEC §7, ADR 0018).
    #[serde(rename = "project.archive")]
    ProjectArchive(ProjectRef),
```

Add after `SessionKill(SessionKillParams)`:

```rust
    /// Returns nothing (`null`): leaves the session out of `session.list` for good (its log
    /// and turns stay); a live one is asked to stop first (SPEC §7, ADR 0018).
    #[serde(rename = "session.remove")]
    SessionRemove(SessionRef),
```

Find the test that round-trips every request. It is the list that contains `Request::ProjectDevStop(ProjectRef { project_id: sid() })` and `Request::SessionKill(...)`. Add to it:
- `Request::ProjectArchive(ProjectRef { project_id: sid() }),`
- `Request::SessionRemove(SessionRef { session_id: sid() }),`

- [ ] **Step 9: Daemon.** In the dispatch of `crates/mkrew-daemon/src/server.rs`, add after the `Request::ProjectDevStop` arm:

```rust
            Request::ProjectArchive(params) => {
                (self.project_archive(params.project_id), After::Continue)
            }
```

Add after the `Request::SessionKill` arm:

```rust
            Request::SessionRemove(params) => {
                (self.session_remove(params.session_id), After::Continue)
            }
```

Add next to `project_update`:

```rust
    /// `project.archive` (ADR 0018): hides the project and stops its live sessions (the dev
    /// server too); tasks, history and comments stay.
    fn project_archive(&self, project_id: Ulid) -> Outcome {
        let archived = {
            // One lock for archive and list.
            let store = self.store();
            store.archive_project(project_id).and_then(|project| {
                let sessions = store.list_sessions(Some(project_id))?;
                Ok((project, sessions))
            })
        };
        let (project, sessions) = match archived {
            Ok(archived) => archived,
            Err(e) => return store_outcome::<Project>(Err(e)),
        };
        for session in sessions.iter().filter(|s| s.state != SessionState::Exited) {
            self.hub.command(session.id, Command::Kill { force: false });
        }
        tracing::info!(%project_id, "project archived");
        let _ = self.hub.events.send(Event::ProjectUpdated(project.clone()));
        ok(&project)
    }
```

Add after `session_kill`:

```rust
    /// `session.remove` (ADR 0018): hidden from `session.list` for good; a live session is
    /// asked to stop (its exit still goes out as `session.exited`).
    fn session_remove(&self, session_id: Ulid) -> Outcome {
        let hidden = self.store().hide_session(session_id);
        if hidden.is_ok() {
            self.hub.command(session_id, Command::Kill { force: false });
        }
        store_outcome(hidden)
    }
```

`hub.command` returns `false` for an exited session. That is fine here.

- [ ] **Step 10: Run the daemon tests**

Run: `cargo test -p mkrew-daemon --test daemon archived_workspaces removed_sessions projects_are_added` then `cargo test -p mkrew-protocol`
Expected: PASS.

- [ ] **Step 11: ADR 0018.** List `docs/decisions/` first, and follow `0000-template.md`. Title: "Archive workspaces, hide sessions".
  - **Context:**
    - The user could not remove a workspace or an agent row.
    - SPEC §7 named `project.archive`, but it was never built.
    - A hard delete fails on foreign keys (there is no `ON DELETE` anywhere). It would also lose history that ADR 0004 keeps.
  - **Options:**
    1. Archive and hide flags (chosen).
    2. Cascade delete through a table-rebuild migration. This loses logs, turns and comments, and needs a big migration.
    3. Hide in the UI only. The rows come back on every restart and on other clients.
  - **Consequences:**
    - Archived projects stay in `project.list`, and clients filter them out. The sidebar, palette, inbox and dev autostart already do.
    - `project.add` of an archived folder un-archives it.
    - Hidden sessions stay in the DB and on disk. The existing log and attachment cleanup still applies.
    - There is no CLI command yet.

- [ ] **Step 12: SPEC.**
  - **§7 table, first project row, Returns cell.** Append:
    `; \`archive\`: \`project_id\` → Project (\`archived: true\`, also \`project.updated\`): its live sessions are asked to stop, clients leave archived projects out, \`add\` of an archived project's folder un-archives it (same id, tasks and history) — ADR 0018`
  - **§7 table, new row after `session.kill`:**
    `| \`session.remove\` | \`session_id\` | ok — leaves the session out of \`session.list\` for good (row, log, turns kept); a live one is asked to stop first (ADR 0018) |`
  - **§5.** After the sentence about the v2 migrations, add:
    `\`0011_session_hidden.sql\` adds \`session.hidden INTEGER NOT NULL DEFAULT 0\` (ADR 0018).`
  - **Version rule (§7, "bump on incompatible changes").** If a current version number is written there, change it to 25.

- [ ] **Step 13: Record (no commit)**

Suggested message: `feat(daemon): archive workspaces and remove sessions (project.archive, session.remove, protocol v25)`

---

### Task 2: "Remove workspace…" and ✕ on agent rows in the sidebar

**Current state (verified):**
- `render_workspace_menu` (main.rs ~6749) offers Open in Zed, the dev server items and the default agent.
- `session_row` (~13982) has no remove action. Exited rows stay forever: HUMAN_CHECKS T2.4 left pruning for later.

Patterns to reuse:
- the `delete_task` confirmation (`window.prompt` + `task_dialog::confirms_delete`, ~3796);
- `set_default_agent` (~3365), which sends errors to `sidebar_error`;
- the hover-revealed `row_action`: the stage button of `git_file_row`, with `group_hover` (~13397).

**Files:**
- Modify: `crates/mkrew-ui/src/workspace_menu.rs` (constant)
- Modify: `crates/mkrew-ui/src/sidebar.rs` (`remove_session`)
- Modify: `crates/mkrew-ui/src/main.rs`: `render_workspace_menu`, `session_row`, new `archive_workspace`, new `remove_session`
- Modify: `docs/SPEC.md` §10

**Interfaces:**
- Consumes:
  - from Task 1: `Request::ProjectArchive(ProjectRef)`, `Request::SessionRemove(SessionRef)`;
  - `task_dialog::confirms_delete(Option<usize>) -> bool`;
  - `Shell::refresh`, `Shell::remove_pane`, `Shell::is_exited`;
  - `row_action(id, label, theme)`, `menu_item(id, theme)`.
- Produces:
  - `workspace_menu::REMOVE_ANSWERS: [&str; 2]`;
  - `Sidebar::remove_session(&mut self, session_id: Ulid)`.

No new permanent test:
- Task 1 pins the daemon behavior.
- The rest is wiring, which the repo rules forbid testing, and GPUI rendering.

Proof comes from the build plus the GUI check in Task 9.

- [ ] **Step 1: Pure pieces.** In `workspace_menu.rs`, add after `set_default_agent`:

```rust
/// Answers of the "Remove …?" confirmations; index 0 removes ([`crate::task_dialog::confirms_delete`]).
pub const REMOVE_ANSWERS: [&str; 2] = ["Remove", "Cancel"];
```

Change the module doc's first line so it mentions removal:
`//! The workspace menu (SPEC §10): "Open in Zed", the dev server, the default agent, and removing the workspace.`

In `sidebar.rs`, add after `toggle`:

```rust
    /// A session the user removed (`session.remove` answered ok): gone from the rows now.
    pub fn remove_session(&mut self, session_id: Ulid) {
        self.sessions.retain(|session| session.id != session_id);
    }
```

- [ ] **Step 2: Remove a workspace.** In `main.rs`, add next to `set_default_agent`:

```rust
    /// "Remove workspace…": asks, then `project.archive` (its sessions stop; its folder, tasks
    /// and history stay; adding the folder again brings it back). The sidebar drops it on
    /// `project.updated`; a refresh moves to another workspace if it was shown.
    fn archive_workspace(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(name) = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .map(|project| project.name.clone())
        else {
            return;
        };
        let live = (self.sidebar.sessions().iter())
            .filter(|s| s.project_id == project_id && s.state != SessionState::Exited)
            .count();
        let stops = match live {
            0 => String::new(),
            1 => "Its running session stops. ".to_owned(),
            n => format!("Its {n} running sessions stop. "),
        };
        let detail = format!(
            "{stops}The folder, its tasks and history stay. Add the folder again to bring it back."
        );
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Remove “{name}” from Mkrew?"),
            Some(detail.as_str()),
            &workspace_menu::REMOVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if !task_dialog::confirms_delete(answer.await.ok()) {
                return;
            }
            let archived = connection
                .call::<Project>(Request::ProjectArchive(ProjectRef { project_id }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match archived {
                    Ok(_) => {
                        shell.sidebar_error = None;
                        shell.refresh(window, cx);
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "project.archive failed: {error:#}");
                        shell.sidebar_error = Some(format!("Cannot remove the workspace: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
```

In `render_workspace_menu`, add after `.children(agents)` (before `panel` closes):

```rust
            .child(separator())
            .child(
                menu_item("remove-workspace".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            // The destructive color of `danger_button`.
                            .text_color(rgb(theme.diff_del_text))
                            .child("Remove workspace…"),
                    )
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.archive_workspace(project_id, window, cx);
                    })),
            )
```

- [ ] **Step 3: Remove an agent row.** In `main.rs`, add next to `kill`:

```rust
    /// ✕ on a sidebar session: a running one asks first (removing stops it); then
    /// `session.remove` hides it for good and its pane closes. Its log stays on disk.
    fn remove_session(&mut self, session_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let answer = (!self.is_exited(session_id)).then(|| {
            window.prompt(
                PromptLevel::Warning,
                "This session is still running.",
                Some("Removing it from the sidebar stops it."),
                &workspace_menu::REMOVE_ANSWERS,
                cx,
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            if let Some(answer) = answer
                && !task_dialog::confirms_delete(answer.await.ok())
            {
                return;
            }
            let removed = connection
                .call::<()>(Request::SessionRemove(SessionRef { session_id }))
                .await;
            let _ = this.update_in(cx, |shell, window, cx| match removed {
                Ok(()) => {
                    shell.sidebar.remove_session(session_id);
                    shell.remove_pane(session_id, window, cx);
                }
                Err(error) => {
                    tracing::warn!(%session_id, "session.remove failed: {error:#}");
                    shell.sidebar_error = Some(format!("Cannot remove the session: {error:#}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }
```

In `session_row`:
1. Replace `.id(SharedString::from(format!("session-{session_id}")))` with a named group:

```rust
    let row_id = SharedString::from(format!("session-{session_id}"));
    div()
        .id(row_id.clone())
        .group(row_id.clone())
        // … existing chain unchanged …
```

2. Add the ✕ action as the last child, after the status text child:

```rust
        .child(
            row_action(SharedString::from(format!("session-remove-{session_id}")), "✕", theme)
                // Shown on hover (SPEC §14 progressive disclosure).
                .opacity(opacity::HIDDEN)
                .group_hover(row_id.clone(), |style| style.opacity(opacity::FULL))
                .tooltip(tooltip(theme, "Remove from the sidebar", ""))
                .on_click(cx.listener(move |shell, _, window, cx| {
                    // Not a click on the row (which would open the session).
                    cx.stop_propagation();
                    shell.remove_session(session_id, window, cx);
                })),
        )
```

`row_action` already sets `.hover` once. Do not add another.

- [ ] **Step 4: SPEC §10.** In the workspaces part of §10, add:

`- **Remove:** the workspace menu's "Remove workspace…" archives it after a confirmation (\`project.archive\`, ADR 0018); a session row's ✕ (on hover) removes it from the sidebar (\`session.remove\`; a running one asks first and is stopped).`

- [ ] **Step 5: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings` then `cargo test -p mkrew-ui --lib sidebar workspace_menu`
Expected: no warnings; PASS.

- [ ] **Step 6: Record (no commit)**

Suggested message: `feat(ui): remove a workspace from its menu and a session from the sidebar`

> Skipped: CLI commands `mk project archive` / `mk session remove`. Add them when someone scripts this.

---

### Task 3: A color per workspace

**Current state (verified):**
- `Project` has no color field.
- SPEC §14 allows "no saturated colors besides the accent and status colors". So this needs ADR 0019 and a §14 token row.
- The field pattern to copy is `dev_autostart`:
  - migration;
  - `#[serde(default)]` field;
  - `ProjectChanges`;
  - `ProjectUpdateParams` double-`Option` with `present`;
  - `workspace_menu` helper;
  - menu item.

Decision: store the color as a palette **name** (`"teal"`), not a hex value.
- The daemon validates the name.
- The UI maps it to a per-appearance token.
- This keeps both themes readable and keeps components free of hard-coded colors.

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs`: `Project`, `ProjectUpdateParams`, `WORKSPACE_COLORS`, tests (~1753-1795 and ~1961)
- Create: `crates/mkrew-store/migrations/0012_project_color.sql`
- Modify: `crates/mkrew-store/src/lib.rs` (`PROJECT_COLUMNS`, `RawProject`, `ProjectChanges`, `update_project`, the `add_project_with_dev` literal) and `src/tests.rs`
- Modify: `crates/mkrew-daemon/src/server.rs` (`project_update`) and `tests/daemon.rs`
- Modify: `crates/mkrew-ui/src/theme.rs`, `sidebar.rs`, `workspace_menu.rs`, `main.rs` (`workspace_row`, `render_workspace_menu`, `set_default_agent`)
- Modify, struct literals only (add `color: None`). The compiler lists each one.
  - `crates/mkrew-cli/src/project.rs`
  - test helpers in `crates/mkrew-ui/src/{dev_server,inbox,palette,sidebar}.rs`
  - `crates/mkrew-ui/tests/{connect,lifecycle}.rs`
  - `crates/mkrew-daemon/tests/daemon.rs`
- Create: `docs/decisions/0019-workspace-colors.md`
- Modify: `docs/SPEC.md` §5, §7, §10, §14

**Interfaces:**
- Produces (protocol):
  - `pub const WORKSPACE_COLORS: [&str; 8] = ["red", "orange", "yellow", "green", "teal", "blue", "purple", "pink"];`
  - `Project.color: Option<String>` (`#[serde(default)]`)
  - `ProjectUpdateParams.color: Option<Option<String>>`: absent = unchanged, `null` = none.
- Produces (store): `ProjectChanges.color: Option<Option<&'a str>>`.
- Produces (ui):
  - `Theme.workspace_colors: [u32; 8]`
  - `Theme::workspace_color(&self, name: &str) -> Option<u32>`
  - `theme::WORKSPACE_DOT_SIZE`, `theme::WORKSPACE_SWATCH_SIZE`
  - `WorkspaceRow.color: Option<String>`
  - `workspace_menu::set_color(project_id: Ulid, color: Option<&str>) -> ProjectUpdateParams`

- [ ] **Step 1: Failing tests.**

Store: append to `tests.rs`:

```rust
#[test]
fn project_color_persists_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert_eq!(p.color, None);
    let teal = ProjectChanges {
        color: Some(Some("teal")),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, teal).unwrap();
    assert_eq!(open(&dir).get_project(p.id).unwrap().color.as_deref(), Some("teal"));
    let none = ProjectChanges {
        color: Some(None),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, none).unwrap();
    assert_eq!(open(&dir).get_project(p.id).unwrap().color, None);
}
```

Daemon: append to `tests/daemon.rs`:

```rust
#[test]
fn workspace_colors_are_set_cleared_and_checked() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let color = |color: Option<&str>| {
        Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            dev_command: None,
            dev_autostart: None,
            color: Some(color.map(Into::into)),
        })
    };
    let teal: Project = client.call(color(Some("teal"))).into_result().unwrap();
    assert_eq!(teal.color.as_deref(), Some("teal"));
    client.wait_event(&Event::ProjectUpdated(teal));
    assert_eq!(
        error_code(client.call(color(Some("#ff0000")))),
        ErrorCode::InvalidRequest
    );
    let cleared: Project = client.call(color(None)).into_result().unwrap();
    assert_eq!(cleared.color, None);
    client.stop();
}
```

Protocol: in `project_update_tells_missing_from_null`, add after the `autostart` assertions:

```rust
        assert_eq!(parse(json!({"project_id": id})).color, None);
        assert_eq!(
            parse(json!({"project_id": id, "color": null})).color,
            Some(None)
        );
        assert_eq!(
            parse(json!({"project_id": id, "color": "teal"})).color,
            Some(Some("teal".into()))
        );
```

Also add `color: None,` to the `keep` literal.

Theme: append to the `theme.rs` tests:

```rust
    #[test]
    fn workspace_colors_stand_out_from_the_surfaces() {
        for theme in [Theme::dark(), Theme::light()] {
            for color in theme.workspace_colors {
                // WCAG 1.4.11: graphical objects ≥ 3:1.
                for bg in [theme.bg, theme.surface, theme.surface_raised] {
                    let ratio = contrast(color, bg);
                    assert!(ratio >= 3.0, "{color:06X} on {bg:06X}: {ratio:.2}");
                }
            }
            assert_eq!(theme.workspace_color("teal"), Some(theme.workspace_colors[4]));
            assert_eq!(theme.workspace_color("#ff0000"), None);
        }
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run:
- `cargo test -p mkrew-store project_color`
- `cargo test -p mkrew-protocol project_update_tells`
- `cargo test -p mkrew-ui --lib workspace_colors_stand_out`

Expected: FAIL to compile (no field `color`).

- [ ] **Step 3: Protocol.** In `message.rs`, add above `pub struct Project`:

```rust
/// Workspace color names (SPEC §14, ADR 0019); clients map them to theme tokens.
pub const WORKSPACE_COLORS: [&str; 8] = [
    "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink",
];
```

In `Project`, add after `base_ref`:

```rust
    /// One of [`WORKSPACE_COLORS`]; `None` = no color.
    #[serde(default)]
    pub color: Option<String>,
```

In `ProjectUpdateParams`, add after `dev_autostart`:

```rust
    /// One of [`WORKSPACE_COLORS`]; `Some(None)` (JSON `null`) clears it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub color: Option<Option<String>>,
```

Add `color: Some("teal".into()),` to the `Project` literal in the `ProjectUpdated` round trip (~1961).

- [ ] **Step 4: Store.** List the migrations folder first, then create `0012_project_color.sql`:

```sql
-- SPEC §5, ADR 0019. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN color TEXT; -- a WORKSPACE_COLORS name, NULL = none
```

Then, in `crates/mkrew-store/src/lib.rs`:
- Append the migration to `MIGRATIONS`.
- Append `, color` to the end of `PROJECT_COLUMNS` (from Task 1).
- `RawProject`: add the field `color: Option<String>,`.
- `from_row`: add `color: row.get(10)?,`.
- `into_project`: add `color: self.color,`.
- The `Project` literal in `add_project_with_dev`: add `color: None,`.
- `ProjectChanges`: add the field

  ```rust
      /// A `WORKSPACE_COLORS` name (checked by the daemon); `Some(None)` clears it.
      pub color: Option<Option<&'a str>>,
  ```

- `update_project`: add before the `UPDATE`:

  ```rust
          if let Some(color) = changes.color {
              project.color = color.map(str::to_owned);
          }
  ```

  Then make the statement `"UPDATE project SET name = ?2, default_agent = ?3, dev_command = ?4, dev_autostart = ?5, color = ?6 WHERE id = ?1"`, with `project.color` as the sixth parameter.

- [ ] **Step 5: Daemon.** In `project_update`, add before `let updated`:

```rust
        if let Some(Some(color)) = &params.color
            && !WORKSPACE_COLORS.contains(&color.as_str())
        {
            let message = format!(
                "unknown color {color:?}; use one of {}",
                WORKSPACE_COLORS.join(", ")
            );
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
```

Add `color: params.color.as_ref().map(Option::as_deref),` to the `ProjectChanges` literal. Import `WORKSPACE_COLORS` from `mkrew_protocol`.

- [ ] **Step 6: Fix every struct literal.** Run `cargo check --workspace --all-targets`. Add `color: None,` to each `Project { … }` and `ProjectUpdateParams { … }` literal it reports:
  - the update in CLI `project.rs`;
  - `workspace_menu::set_default_agent`;
  - `dev_server.rs`;
  - the UI and daemon tests;
  - the test helpers in `inbox.rs`, `palette.rs`, `sidebar.rs` and `dev_server.rs`.

- [ ] **Step 7: Theme.** In `theme.rs`, add `use mkrew_protocol::WORKSPACE_COLORS;`.

Add the constants near `FILE_CHANGES_DOT_SIZE`:

```rust
/// The workspace color dot in front of a sidebar row's name (ADR 0019).
pub const WORKSPACE_DOT_SIZE: f32 = space::S;

/// A color choice in the workspace menu.
pub const WORKSPACE_SWATCH_SIZE: f32 = space::L;
```

Add the field to `Theme`, after `ansi`:

```rust
    /// Workspace colors, in [`WORKSPACE_COLORS`] order (≥ 3:1 on every surface, ADR 0019).
    pub workspace_colors: [u32; 8],
```

Set its values:
- In `dark()`: `workspace_colors: [0xE5676B, 0xF0955A, 0xE3C14E, 0x4CB782, 0x5EC4D6, 0x7C8CFF, 0xC28CF0, 0xF08CC0],`
- In `light()`: `workspace_colors: [0xC53030, 0xC05621, 0x975A16, 0x2F855A, 0x1F8A9E, 0x4F5BD5, 0x8A3FB8, 0xB83280],`
- In `shared()`: `workspace_colors: [0; 8],`

These values were checked against `bg`, `surface` and `surface_raised`: dark ≥ 4.66:1, light ≥ 3.64:1.

Add to `impl Theme`:

```rust
    /// The token of a workspace color name; `None` for an unknown name.
    pub fn workspace_color(&self, name: &str) -> Option<u32> {
        let index = WORKSPACE_COLORS.iter().position(|color| *color == name)?;
        self.workspace_colors.get(index).copied()
    }
```

- [ ] **Step 8: Sidebar row model.**
  - `WorkspaceRow`: add `pub color: Option<String>,` with the doc `/// A \`WORKSPACE_COLORS\` name.`
  - `rows()`: set `color: project.color.clone(),`.
  - Add `color: None,` to the `WorkspaceRow` literal in `added_and_updated_workspaces_show_up_and_archived_ones_leave`.

- [ ] **Step 9: Menu helper.** In `workspace_menu.rs`:

```rust
/// `project.update` setting the workspace's color (`None` = no color).
pub fn set_color(project_id: Ulid, color: Option<&str>) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        name: None,
        default_agent: None,
        dev_command: None,
        dev_autostart: None,
        color: Some(color.map(str::to_owned)),
    }
}
```

- [ ] **Step 10: UI.** All edits are in `main.rs`. Import `ProjectUpdateParams` and `WORKSPACE_COLORS` from `mkrew_protocol`.

**(a)** Move the request part of `set_default_agent` into a shared helper, `update_project`, and add `set_workspace_color`:

```rust
    /// Makes `agent` the default agent of `project_id`; the sidebar picks the change up from
    /// `project.updated`.
    fn set_default_agent(
        &mut self,
        project_id: Ulid,
        agent: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_default_agent(project_id, &agent);
        self.update_project(params, "Cannot change the default agent", window, cx);
    }

    /// Sets (or clears) the workspace color from the menu.
    fn set_workspace_color(
        &mut self,
        project_id: Ulid,
        color: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_color(project_id, color);
        self.update_project(params, "Cannot change the color", window, cx);
    }

    /// Closes the workspace menu and sends `project.update`; a failure shows in the sidebar
    /// after `failure`.
    fn update_project(
        &mut self,
        params: ProjectUpdateParams,
        failure: &'static str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.workspace_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let project_id = params.project_id;
        let updated = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.spawn_in(window, async move |this, cx| {
            let updated = updated.await;
            let _ = this.update(cx, |shell, cx| {
                match updated {
                    Ok(_) => shell.sidebar_error = None,
                    Err(error) => {
                        tracing::warn!(%project_id, "project.update failed: {error:#}");
                        shell.sidebar_error = Some(format!("{failure}: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
```

**(b)** Add a free function next to `agent_rows`:

```rust
/// One color choice of the workspace menu: a filled dot (`None` = an empty ring = no
/// color); the current one is ringed in the text color.
fn color_swatch(
    project_id: Ulid,
    color: Option<&'static str>,
    current: Option<&str>,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::AnyElement {
    let fill = color.and_then(|name| theme.workspace_color(name));
    let ring = if color == current {
        theme.text
    } else {
        fill.unwrap_or(theme.text_muted)
    };
    div()
        .id(SharedString::from(format!(
            "workspace-color-{}",
            color.unwrap_or("none")
        )))
        .flex_none()
        .size(px(theme::WORKSPACE_SWATCH_SIZE))
        .rounded_full()
        .cursor_pointer()
        .border_2()
        .border_color(rgb(ring))
        .when_some(fill, |el, fill| el.bg(rgb(fill)))
        .hover(|style| style.opacity(opacity::HOVER))
        .tooltip(tooltip(theme, color.unwrap_or("No color"), ""))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.set_workspace_color(project_id, color, window, cx);
        }))
        .into_any_element()
}
```

**(c)** In `render_workspace_menu`, read the current color next to `current`:

```rust
        let current_color = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .and_then(|project| project.color.clone());
```

Then add this between `.children(agents)` and Task 2's separator + "Remove workspace…":

```rust
            .child(separator())
            .child(
                div()
                    .px(px(space::S))
                    .pt(px(space::XS))
                    .text_size(px(font_size::META))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child("Color"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .py(px(space::S))
                    .child(color_swatch(project_id, None, current_color.as_deref(), theme, cx))
                    .children(WORKSPACE_COLORS.iter().map(|name| {
                        color_swatch(project_id, Some(*name), current_color.as_deref(), theme, cx)
                    })),
            )
```

**(d)** In `workspace_row`, add a child before the name child. It is always present, so names line up:

```rust
        .child(
            div()
                .flex_none()
                .size(px(theme::WORKSPACE_DOT_SIZE))
                .rounded_full()
                .when_some(
                    row.color.as_deref().and_then(|name| theme.workspace_color(name)),
                    |el, color| el.bg(rgb(color)),
                ),
        )
```

- [ ] **Step 11: Run the tests**

Run:
- `cargo test -p mkrew-store project_color`
- `cargo test -p mkrew-protocol`
- `cargo test -p mkrew-daemon --test daemon workspace_colors`
- `cargo test -p mkrew-ui --lib theme sidebar workspace_menu`
- `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS, no warnings.

- [ ] **Step 12: ADR 0019 + SPEC.**

ADR "Workspace colors" (list `docs/decisions/` first):
- **Context:** the user wants to tell workspaces apart at a glance. §14 allowed only the accent and status colors.
- **Decision:**
  - 8 named colors, stored as names.
  - Per-appearance tokens, each ≥ 3:1 on every surface.
  - Shown as a dot before the sidebar name, and as swatches in the workspace menu.
  - Never used for state.
- **Options rejected:**
  - Free hex values: unreadable in one of the themes, and they hard-code colors.
  - Coloring the whole row: fights the selection fill.

SPEC:
- **§5:** add `0012_project_color.sql adds project.color TEXT (ADR 0019)`.
- **§7 project row:**
  - "Main params" cell: add `color`.
  - Returns cell: add `\`color\` must be one of red, orange, yellow, green, teal, blue, purple, pink (null = none)`.
- **§10:** add `- **Color:** the workspace menu's swatches give a workspace one of 8 colors, shown as a dot before its name (decoration only, ADR 0019).`
- **§14 principle 2:** append `Workspace colors (ADR 0019) are the one exception: a small dot per workspace.`
- **§14 tokens:** add the line `- Workspace colors (red … pink): dark #E5676B #F0955A #E3C14E #4CB782 #5EC4D6 #7C8CFF #C28CF0 #F08CC0 / light #C53030 #C05621 #975A16 #2F855A #1F8A9E #4F5BD5 #8A3FB8 #B83280 (≥ 3:1 on every surface).`

- [ ] **Step 13: Record (no commit)**

Suggested message: `feat: a color per workspace (dot in the sidebar, swatches in the workspace menu)`

> Skipped: `mk project update --color`, and coloring tabs or inbox rows. Add them if the dot alone is not enough.

---

### Task 4: Full-width rows in the Files tree and the Git list

**Current state (verified):**
- In `gpui-pre 0.3.7` `uniform_list` (elements/uniform_list.rs ~499-509), each item is laid out with `layout_as_root(Definite(width), …)`.
- In taffy 0.13, a root fills the available width only when it is `display: block`. A `.flex()` root shrinks to its content.
- `file_row` (main.rs ~13115), `git_file_row` (~13257, all row kinds) and `git_commit_row` (~13435) have no `.w_full()`.
- Result:
  - The selection fill, hover and click target only hug the name.
  - The git marker sits right after the name instead of at the right edge.
- Rows that already work use `.w_full()`: `preview::code_view`, `preview::source`, `git_diff_row`.
- `doc_row` lives in a flex column, which stretches it, so it is fine.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (`file_row`, `git_file_row`, `git_commit_row`)

**Interfaces:** none.

No automated test: this is GPUI layout, and the repo has no render tests. The bug was confirmed in the gpui/taffy source (see above). Proof is the GUI check in Task 9.

- [ ] **Step 1: `file_row`.**
  - Add `.w_full()` right after `.id(SharedString::from(format!("file-{}", entry.path)))`.
  - In the `Row::Note` early return, add `.w_full()` as the first call after `div()`.

  The indent stays as padding (`pl`) inside the row. So the highlight spans the full width, as in other editors.

- [ ] **Step 2: `git_file_row`.**
  - Add `.w_full()` after `div()` in the `Header` and `Note` returns.
  - Add `.w_full()` after `.group(id.clone())` in the file row.
  - Replace

    ```rust
            .when(group == git_view::Group::Commit, |el| el.ml(px(space::M)))
    ```

    with

    ```rust
            // Indented under its commit inside the full-width row (a margin would overflow it).
            .when(group == git_view::Group::Commit, |el| el.pl(px(space::S + space::M)))
    ```

    It comes after `.px(px(space::S))`, so the left padding wins.

- [ ] **Step 3: `git_commit_row`.** Add `.w_full()` after its `.id(…)`.

- [ ] **Step 4: Build**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Record (no commit)**

Suggested message: `fix(ui): Files tree and Git list rows select and highlight across the full width`

---

### Task 5: Find in the opened file (Files view)

**Current state (verified):**
- `render_file` (main.rs ~7718) has a header (path, "Line N", Review in Docs, Open in Zed) and a body from `preview::code_view`, a `uniform_list` with no scroll handle. There is no find.
- The terminal `FindBar` (`terminal/find.rs`) is tied to the emulator's regex search. Its key parsing (`find_key`/`FindKey`) is generic, though.
- `Command::Find` (Ctrl+Shift+F) is limited to `TERMINAL_CONTEXT`, and `Shell::run` treats it as a no-op.

Decision:
- Add a small pure `text_find` module: a case-insensitive literal search over `Code.lines`.
- Reuse `Command::Find` in the Files view by widening its key context. One shortcut then finds everywhere.
- Draw matches with the existing `terminal_match_bg`/`terminal_match_current_bg` tokens. Text on them is drawn in `text`, which is already ≥ 4.5:1 (`selected_text_stays_readable`).

**Files:**
- Create: `crates/mkrew-ui/src/text_find.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod text_find;`)
- Modify: `crates/mkrew-ui/src/files_view.rs` (`find` field, `search`, `show`)
- Modify: `crates/mkrew-ui/src/preview.rs` (`Paint`, `paint`, `code_line`, `code_view`)
- Modify: `crates/mkrew-ui/src/keymap.rs` (`FIND_CONTEXT`, the `Command::Find` context, test ~686)
- Modify: `crates/mkrew-ui/src/palette.rs` (`Command::Find` label and availability)
- Modify: `crates/mkrew-ui/src/main.rs`: fields, `run`, `render_file`, the git diff `code_line` call (~13663), new find methods
- Modify: `docs/SPEC.md` §12A, §13

**Interfaces:**
- Produces `text_find::{TextFind, Hits, Match, find_in_line}`:
  - `TextFind { pub query: String, .. }` with:
    - `search(&mut self, lines: &[String])`
    - `step(&mut self, forward: bool) -> Option<&Match>`
    - `current(&self) -> Option<&Match>`
    - `hits(&self) -> Hits`
    - `label(&self) -> String`
  - `Hits::on_line(&self, line: usize) -> Vec<(Range<usize>, bool)>`
  - `Match { pub line: usize /* 0-based */, pub range: Range<usize> }`
- Produces on `FilesView`: `FilesView.find: Option<TextFind>` and `FilesView::search(&mut self)`.
- Produces in `preview`:
  - `preview::code_line(text: String, spans: &[(Range<usize>, Token)], found: &[(Range<usize>, bool)], theme: &Theme) -> StyledText`. The new `found` parameter is also used by Task 7.
  - `preview::code_view(id, code, selected, found: Option<Hits>, on_line, theme)`.
- Produces `keymap::FIND_CONTEXT: &str = "Terminal || Files"`. Task 6 extends it.
- Consumes: `terminal::find::{find_key, FindKey}`, `task_dialog::{dialog_key, DialogKey}`, `text_field`, `Shell::typing`, `tooltip`, `button`.

- [ ] **Step 1: Write the failing tests.** Create `crates/mkrew-ui/src/text_find.rs` with only the tests for now, and add `pub mod text_find;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|line| (*line).to_owned()).collect()
    }

    fn at(find: &TextFind) -> Option<(usize, usize)> {
        find.current().map(|m| (m.line, m.range.start))
    }

    #[test]
    fn matches_ignore_case_and_keep_byte_ranges_of_the_line() {
        // "Tiếng " is 8 bytes, "VIỆT" 6; lowercase 'ệ' is as long as 'Ệ'.
        assert_eq!(find_in_line("Tiếng VIỆT, việt", "việt"), [8..14, 16..22]);
        assert_eq!(find_in_line("aaaa", "aa"), [0..2, 2..4], "not overlapping");
        // 'İ' lowercases to two chars: no match, and ranges stay on char boundaries.
        let line = "İİi";
        let found = find_in_line(line, "i");
        assert_eq!(found, [4..5]);
        assert!(found.iter().all(|r| line.get(r.clone()).is_some()));
        assert!(find_in_line("abc", "").is_empty());
    }

    #[test]
    fn enter_steps_through_matches_and_wraps() {
        let text = lines(&["fn a() {}", "let a = 1;", "", "a"]);
        let mut find = TextFind::default();
        find.query = "a".into();
        find.search(&text);
        assert_eq!(find.label(), "1 of 3");
        assert_eq!(at(&find), Some((0, 3)));
        find.step(true);
        assert_eq!(at(&find), Some((1, 4)));
        find.step(true);
        assert_eq!(at(&find), Some((3, 0)));
        find.step(true);
        assert_eq!(at(&find), Some((0, 3)), "wraps to the first");
        find.step(false);
        assert_eq!(at(&find), Some((3, 0)), "and back to the last");
        assert_eq!(find.hits().on_line(1), [(4..5, false)]);
        assert_eq!(find.hits().on_line(3), [(0..1, true)]);
        assert!(find.hits().on_line(2).is_empty());
    }

    #[test]
    fn a_new_search_stays_at_or_after_the_current_match() {
        let text = lines(&["alpha", "beta", "alphabet"]);
        let mut find = TextFind::default();
        find.query = "al".into();
        find.search(&text);
        find.step(true);
        assert_eq!(at(&find), Some((2, 0)));
        find.query.push('p');
        find.search(&text);
        assert_eq!(find.label(), "2 of 2", "typing on keeps the place");
        find.query = "zz".into();
        find.search(&text);
        assert_eq!(find.label(), "No matches");
        assert_eq!(find.current(), None);
        assert_eq!(find.step(true), None);
        find.query.clear();
        find.search(&text);
        assert_eq!(find.label(), "");
    }
}
```

In the `files_view.rs` tests, add:

```rust
    #[test]
    fn find_follows_a_reread_file() {
        let mut view = listed();
        view.open("README.md");
        view.begin_read();
        view.read("README.md", Ok(code("todo\nok")));
        let mut find = TextFind::default();
        find.query = "todo".into();
        view.find = Some(find);
        view.search();
        let label = |view: &FilesView| view.find.as_ref().map(TextFind::label);
        assert_eq!(label(&view).as_deref(), Some("1 of 1"));
        // An agent edits the file: the re-read is searched again.
        view.begin_read();
        view.read("README.md", Ok(code("todo\ntodo")));
        assert_eq!(label(&view).as_deref(), Some("1 of 2"));
    }
```

In the `preview.rs` tests, add:

```rust
    #[test]
    fn matches_paint_over_syntax_colors() {
        let spans = [(0..2, Token::Keyword), (3..8, Token::String)];
        let found = [(1..4, false), (6..7, true)];
        assert_eq!(
            paint(&spans, &found),
            [
                (0..1, Paint::Token(Token::Keyword)),
                (1..2, Paint::Match { current: false }),
                (2..3, Paint::Match { current: false }),
                (3..4, Paint::Match { current: false }),
                (4..6, Paint::Token(Token::String)),
                (6..7, Paint::Match { current: true }),
                (7..8, Paint::Token(Token::String)),
            ]
        );
        assert_eq!(
            paint(&spans, &[]),
            [
                (0..2, Paint::Token(Token::Keyword)),
                (3..8, Paint::Token(Token::String))
            ]
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mkrew-ui --lib text_find find_follows matches_paint`
Expected: FAIL to compile (`TextFind`, `find_in_line`, `paint`, `Paint` not found).

- [ ] **Step 3: `text_find.rs`.** Put this above the tests:

```rust
//! Find in the Files viewer (SPEC §12A): the query, its case-insensitive matches in the
//! opened file's lines, and the current one. Pure and tested headlessly; `main.rs` draws the
//! bar and `preview::code_view` the highlights.

use std::ops::Range;
use std::rc::Rc;

/// One match: the line (0-based) and the byte range in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    pub range: Range<usize>,
}

/// The matches to draw; cheap to clone into a render closure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hits {
    /// Sorted by line, then position.
    matches: Rc<[Match]>,
    current: Option<usize>,
}

impl Hits {
    /// The matches on `line` (byte ranges, left to right) and whether each is the current one.
    pub fn on_line(&self, line: usize) -> Vec<(Range<usize>, bool)> {
        let start = self.matches.partition_point(|m| m.line < line);
        self.matches[start..]
            .iter()
            .take_while(|m| m.line == line)
            .enumerate()
            .map(|(i, m)| (m.range.clone(), self.current == Some(start + i)))
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextFind {
    pub query: String,
    hits: Hits,
}

impl TextFind {
    pub fn hits(&self) -> Hits {
        self.hits.clone()
    }

    pub fn current(&self) -> Option<&Match> {
        self.hits.current.and_then(|index| self.hits.matches.get(index))
    }

    /// "3 of 12", "No matches", or nothing while the query is empty.
    pub fn label(&self) -> String {
        match (self.query.is_empty(), self.hits.current) {
            (true, _) => String::new(),
            (false, None) => "No matches".to_owned(),
            (false, Some(index)) => format!("{} of {}", index + 1, self.hits.matches.len()),
        }
    }

    /// Searches `lines` again (after an edit of the query or a re-read of the file). The
    /// current match is the first one at or after the previous current position.
    pub fn search(&mut self, lines: &[String]) {
        let from = self.current().map_or((0, 0), |m| (m.line, m.range.start));
        let matches: Vec<Match> = lines
            .iter()
            .enumerate()
            .flat_map(|(line, text)| {
                find_in_line(text, &self.query)
                    .into_iter()
                    .map(move |range| Match { line, range })
            })
            .collect();
        let current = (!matches.is_empty()).then(|| {
            matches
                .iter()
                .position(|m| (m.line, m.range.start) >= from)
                .unwrap_or(0)
        });
        self.hits = Hits {
            matches: matches.into(),
            current,
        };
    }

    /// Enter / Shift+Enter: the next or previous match, wrapping around.
    pub fn step(&mut self, forward: bool) -> Option<&Match> {
        let len = self.hits.matches.len();
        let current = self.hits.current?;
        self.hits.current = Some(if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        });
        self.current()
    }
}

/// The byte ranges of `query` in `line`, ignoring case, left to right, not overlapping.
pub fn find_in_line(line: &str, query: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    if query.is_empty() {
        return found;
    }
    let mut from = 0;
    while let Some(c) = line[from..].chars().next() {
        match match_len(&line[from..], query) {
            Some(len) => {
                found.push(from..from + len);
                from += len;
            }
            None => from += c.len_utf8(),
        }
    }
    found
}

/// The byte length of the start of `text` that equals `query` ignoring case.
fn match_len(text: &str, query: &str) -> Option<usize> {
    let mut chars = text.char_indices();
    for want in query.chars() {
        let (_, got) = chars.next()?;
        if !got.to_lowercase().eq(want.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or(text.len(), |(index, _)| index))
}
```

- [ ] **Step 4: `FilesView`.** In `files_view.rs`:
  - Import `crate::text_find::TextFind`.
  - Add the field after `zed_error`:

    ```rust
        /// The find bar over the opened file (SPEC §12A), `None` = closed. Kept when another
        /// file opens; searched again whenever the content changes.
        pub find: Option<TextFind>,
    ```

  - Set `find: None,` in `new`.
  - Add the method:

    ```rust
        /// Runs the find bar's query over the opened file again.
        pub fn search(&mut self) {
            if let (Some(find), Content::Shown(code)) = (&mut self.find, &self.content) {
                find.search(&code.lines);
            }
        }
    ```

  - Call `self.search();` at the end of `show`, after `self.content = …`.

- [ ] **Step 5: `preview.rs` painting.** Add above `code_line`:

```rust
/// How a piece of a code line is drawn: in a token color, or as a find match.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    Token(Token),
    Match { current: bool },
}

/// The non-overlapping pieces of a line to draw: find matches (`found`, with "is current")
/// cover the syntax `spans`; text in neither keeps the parent's color.
pub fn paint(
    spans: &[(Range<usize>, Token)],
    found: &[(Range<usize>, bool)],
) -> Vec<(Range<usize>, Paint)> {
    if found.is_empty() {
        return spans
            .iter()
            .map(|(range, token)| (range.clone(), Paint::Token(*token)))
            .collect();
    }
    let mut cuts: Vec<usize> = spans
        .iter()
        .map(|(range, _)| range)
        .chain(found.iter().map(|(range, _)| range))
        .flat_map(|range| [range.start, range.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let covers = |range: &Range<usize>, piece: &Range<usize>| {
        range.start <= piece.start && piece.end <= range.end
    };
    cuts.windows(2)
        .filter_map(|pair| {
            let piece = pair[0]..pair[1];
            let matched = found
                .iter()
                .find(|(range, _)| covers(range, &piece))
                .map(|(_, current)| Paint::Match { current: *current });
            let token = || {
                spans
                    .iter()
                    .find(|(range, _)| covers(range, &piece))
                    .map(|(_, token)| Paint::Token(*token))
            };
            matched.or_else(token).map(|paint| (piece, paint))
        })
        .collect()
}
```

Replace `code_line`:

```rust
/// One line of code with its syntax `spans` colored and its find matches (`found`, with "is
/// current") on the match backgrounds in the text color; other text keeps the parent's color.
pub fn code_line(
    text: String,
    spans: &[(Range<usize>, Token)],
    found: &[(Range<usize>, bool)],
    theme: &Theme,
) -> StyledText {
    let highlights: Vec<_> = paint(spans, found)
        .into_iter()
        .map(|(range, paint)| {
            let style = match paint {
                Paint::Token(token) => HighlightStyle {
                    color: Some(Hsla::from(rgb(token_color(token, theme)))),
                    ..HighlightStyle::default()
                },
                Paint::Match { current } => HighlightStyle {
                    color: Some(Hsla::from(rgb(theme.text))),
                    background_color: Some(Hsla::from(rgb(if current {
                        theme.terminal_match_current_bg
                    } else {
                        theme.terminal_match_bg
                    }))),
                    ..HighlightStyle::default()
                },
            };
            (range, style)
        })
        .collect();
    StyledText::new(text).with_highlights(highlights)
}
```

Then update `code_view` and its callers:
- Import `crate::text_find::Hits`.
- Add the parameter `found: Option<Hits>` after `selected: Option<u32>`. Update its doc to say find matches are highlighted.
- Inside the row closure, compute `let marks = found.as_ref().map(|hits| hits.on_line(index)).unwrap_or_default();` and call `code_line(line.to_owned(), spans, &marks, &theme)`.
- In `main.rs`, the git diff row's `preview::code_line(…)` call (~13663) gets `&[]` as its new third argument.

- [ ] **Step 6: Run the tests**

Run: `cargo test -p mkrew-ui --lib text_find find_follows matches_paint`
Expected: PASS.

- [ ] **Step 7: Keymap and palette.**

In `keymap.rs`, add after `COMMENTS_CONTEXT`:

```rust
/// Find: in a terminal's scrollback, and in the Files view's opened file.
pub const FIND_CONTEXT: &str = "Terminal || Files";
```

Then:
- In `Command::context`, take `Command::Find` out of the `TERMINAL_CONTEXT` arm, and add `Command::Find => Some(FIND_CONTEXT),`.
- In the test (~686), remove `Command::Find` from the `TERMINAL_CONTEXT` loop, and add `assert_eq!(Command::Find.context(), Some(FIND_CONTEXT));`.

In `palette.rs`:
- Change the label to `Command::Find => "Find: in scrollback or the open file".into(),`.
- Take `Command::Find` out of the `Copy | Paste | PageUp | PageDown` arm, and add `Command::Find => (on_panes && context.terminal) || in_view(View::Files),`.

- [ ] **Step 8: Shell state and keys.** In `main.rs`, add fields next to `files_scroll`:

```rust
    /// The Files viewer's find field (outside `FILES_CONTEXT`'s arrow keys).
    file_find_focus: FocusHandle,
    /// Keeps the current find match of the Files viewer in view.
    file_code_scroll: UniformListScrollHandle,
```

Initialize them with `cx.focus_handle()` and `UniformListScrollHandle::new()`. Import `mkrew_ui::text_find::TextFind` and `mkrew_ui::terminal::find::{FindKey, find_key}`, the same way `main.rs` already imports other `mkrew_ui` modules.

Add the methods next to `files_key_down`:

```rust
    /// Ctrl+Shift+F / "Find" in the Files view: opens the find bar over the opened file, or
    /// gives it the keyboard again.
    fn open_file_find(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let view = self.files_view(project_id);
        if view.file.is_none() {
            return;
        }
        view.find.get_or_insert_with(TextFind::default);
        window.focus(&self.file_find_focus, cx);
        cx.notify();
    }

    /// Typing edits the query; Enter / Shift+Enter go to the next / previous match; Esc
    /// closes the bar.
    fn file_find_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(project_id) = self.workspace else {
            return;
        };
        if task_dialog::dialog_key(&event.keystroke) == Some(DialogKey::Paste) {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let text = text.replace(['\r', '\n'], " ");
                self.edit_file_find(project_id, |query| query.push_str(&text), cx);
            }
            return;
        }
        match find_key(&event.keystroke) {
            Some(FindKey::Close) => self.close_file_find(project_id, window, cx),
            // The terminal's names: Enter = "older" = the next match here.
            Some(FindKey::Older) => self.step_file_find(project_id, true, cx),
            Some(FindKey::Newer) => self.step_file_find(project_id, false, cx),
            Some(FindKey::Backspace) => self.edit_file_find(
                project_id,
                |query| {
                    query.pop();
                },
                cx,
            ),
            Some(FindKey::Insert(text)) => {
                self.edit_file_find(project_id, |query| query.push_str(&text), cx);
            }
            None => {}
        }
    }

    fn edit_file_find(
        &mut self,
        project_id: Ulid,
        edit: impl FnOnce(&mut String),
        cx: &mut Context<'_, Self>,
    ) {
        let view = self.files_view(project_id);
        let Some(find) = &mut view.find else {
            return;
        };
        edit(&mut find.query);
        view.search();
        self.reveal_file_match(project_id, cx);
    }

    fn step_file_find(&mut self, project_id: Ulid, forward: bool, cx: &mut Context<'_, Self>) {
        if let Some(find) = &mut self.files_view(project_id).find {
            find.step(forward);
        }
        self.reveal_file_match(project_id, cx);
    }

    /// Scrolls to the current match and selects its line (so "Open in Zed" goes there).
    fn reveal_file_match(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        let view = self.files_view(project_id);
        let line = view.find.as_ref().and_then(TextFind::current).map(|m| m.line);
        if let Some(line) = line {
            view.line = u32::try_from(line + 1).ok();
            self.file_code_scroll
                .scroll_to_item(line, ScrollStrategy::Center);
        }
        cx.notify();
    }

    fn close_file_find(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.files_view(project_id).find = None;
        window.focus(&self.files_focus, cx);
        cx.notify();
    }
```

In `Shell::run`, add next to the git commit box guard:

```rust
        // Same for the Files find field while it has the keyboard (Esc gives it back).
        if self.file_find_focus.is_focused(window) {
            let open = self.view() == View::Files
                && (self.workspace)
                    .and_then(|id| self.files.get(&id))
                    .is_some_and(|view| view.find.is_some());
            if open {
                cx.propagate();
                return;
            }
            window.focus(&self.focus_handle, cx);
        }
```

In the final `match *command`, take `Command::Find` out of the `Copy | Paste | PageUp | PageDown` no-op arm, and add:

```rust
            // A focused terminal pane handles it itself.
            Command::Find => {
                if self.view() == View::Files {
                    self.open_file_find(window, cx);
                }
            }
```

- [ ] **Step 9: Render.** Give `render_file` a `window: &Window` parameter, after `view`. Pass `window` at its call site in `render_files`.

In its header, add before `.child(zed_button("file-zed", false))`:

```rust
            .child(
                button("file-find", "Find", theme, false)
                    .flex_none()
                    .tooltip(tooltip(
                        theme,
                        "Find in this file",
                        self.keymap.label(Command::Find),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| shell.open_file_find(window, cx))),
            )
```

Build the bar before `let body`:

```rust
        let find_bar = view.find.as_ref().map(|find| {
            let focused = self.file_find_focus.is_focused(window);
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::L))
                .pb(px(space::S))
                .child(
                    text_field("file-find-field", &find.query, "Find in file", focused, theme)
                        .w(px(theme::FIND_INPUT_WIDTH))
                        .track_focus(&self.file_find_focus)
                        .on_key_down(cx.listener(Self::file_find_key_down))
                        .relative()
                        .child(self.typing(&self.file_find_focus, Self::file_find_key_down, cx))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            window.focus(&shell.file_find_focus, cx);
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(px(font_size::META))
                        .text_color(rgb(theme.text_muted))
                        .child(find.label()),
                )
                .child(
                    button("file-find-previous", "↑", theme, false)
                        .tooltip(tooltip(theme, "Previous match", "Shift+Enter"))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.step_file_find(project_id, false, cx);
                        })),
                )
                .child(
                    button("file-find-next", "↓", theme, false)
                        .tooltip(tooltip(theme, "Next match", "Enter"))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.step_file_find(project_id, true, cx);
                        })),
                )
                .child(
                    button("file-find-close", "✕", theme, false)
                        .tooltip(tooltip(theme, "Close", "Esc"))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.close_file_find(project_id, window, cx);
                        })),
                )
        });
```

In the `Content::Shown(code)` arm, pass the hits and the scroll handle:

```rust
                preview::code_view(
                    "file-code",
                    Rc::clone(code),
                    view.line,
                    view.find.as_ref().map(TextFind::hits),
                    on_line,
                    theme,
                )
                .track_scroll(&self.file_code_scroll)
                .flex_1()
                // … rest unchanged
```

In the final `panel` chain, add `.children(find_bar)` right after `.child(header)`.

- [ ] **Step 10: SPEC.**
  - **§12A:** add `- **Find in file** (\`Ctrl+Shift+F\` or "Find" in the viewer header): case-insensitive text search in the opened file; matches highlighted, Enter / Shift+Enter move between them (the line is selected for "Open in Zed"), Esc closes.`
  - **§13 table:** change the row `| Terminal | Find in scrollback | Ctrl+Shift+F |` to `| Terminal / Files | Find in scrollback / in the opened file | Ctrl+Shift+F |`.

- [ ] **Step 11: Build and test**

Run: `cargo test -p mkrew-ui --lib text_find files_view preview keymap palette` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 12: Record (no commit)**

Suggested message: `feat(ui): find text in the file opened in the Files view`

> Skipped: regex and match-case toggles. Add them when a literal, case-insensitive search falls short.

---

### Task 6: Filter the Docs file list

**Current state (verified):**
- `render_docs` (main.rs ~7302) maps `Files::Loaded(files)` straight to `doc_row`s. There is no filter, no focus handle and no field.
- `Docs::listed` replaces `files` on every `doc.changed`/`comment.updated`. So a filter must live in `Docs` and be applied when the list is drawn.

Pieces to reuse:
- `palette::fuzzy(query, text) -> Option<(i64, Vec<u32>)>` scores locally. An empty query matches everything.
- `quick_open::match_ranges` turns its char indices into byte ranges for highlighting.
- `quick_open::key` parses the keys.

Decision:
- A "Filter files…" field at the top of the list.
- ↑/↓ in the field move the selection over the shown files.
- Ctrl+Shift+F (`Command::Find`) focuses the field in the Docs view.

**Files:**
- Modify: `crates/mkrew-ui/src/docs.rs` (`filter`, `shown_files`, `step_selection`, tests)
- Modify: `crates/mkrew-ui/src/keymap.rs`, `palette.rs` (Find also in Docs)
- Modify: `crates/mkrew-ui/src/main.rs` (field, `render_docs`, `doc_row`, `docs_filter_key_down`, `run`)
- Modify: `docs/SPEC.md` §12, §13

**Interfaces:**
- Produces:
  - `Docs.filter: String`
  - `Docs::shown_files(&self) -> Vec<(&DocFile, Vec<u32>)>`
  - `Docs::step_selection(&self, offset: isize) -> Option<String>`
- Consumes: `palette::fuzzy`, `quick_open::{key, Key, match_ranges}`, `FIND_CONTEXT` (Task 5).

- [ ] **Step 1: Failing tests.** In the `docs.rs` tests:

```rust
    fn paths(docs: &Docs) -> Vec<String> {
        docs.shown_files()
            .into_iter()
            .map(|(file, _)| file.file_path.clone())
            .collect()
    }

    fn three_files() -> Docs {
        let mut docs = Docs::new(Ulid::new());
        docs.begin_list();
        docs.listed(Ok(vec![
            file("PLAN.md", 0),
            file("docs/plan-v2.md", 0),
            file("docs/notes.md", 1),
        ]));
        docs
    }

    #[test]
    fn the_filter_ranks_paths_and_survives_relisting() {
        let mut docs = three_files();
        assert_eq!(paths(&docs), ["PLAN.md", "docs/plan-v2.md", "docs/notes.md"]);
        docs.filter = "notes".into();
        assert_eq!(paths(&docs), ["docs/notes.md"]);
        assert_eq!(docs.shown_files()[0].1, [5, 6, 7, 8, 9]);
        // Equal scores keep the daemon's order.
        docs.filter = "md".into();
        assert_eq!(paths(&docs), ["PLAN.md", "docs/plan-v2.md", "docs/notes.md"]);
        // A relist (doc.changed / comment.updated) keeps the filter.
        docs.filter = "v2".into();
        docs.begin_list();
        docs.listed(Ok(vec![file("docs/plan-v2.md", 3), file("docs/other.md", 0)]));
        assert_eq!(paths(&docs), ["docs/plan-v2.md"]);
        docs.filter = "zz".into();
        assert!(paths(&docs).is_empty());
    }

    #[test]
    fn arrows_in_the_filter_move_over_the_shown_files() {
        let mut docs = three_files();
        docs.filter = "docs".into();
        assert_eq!(
            docs.step_selection(1).as_deref(),
            Some("docs/plan-v2.md"),
            "the selected PLAN.md is filtered out: the first shown file"
        );
        docs.select("docs/plan-v2.md");
        assert_eq!(docs.step_selection(1).as_deref(), Some("docs/notes.md"));
        docs.select("docs/notes.md");
        assert_eq!(docs.step_selection(1).as_deref(), Some("docs/notes.md"), "stays at the end");
        assert_eq!(docs.step_selection(-1).as_deref(), Some("docs/plan-v2.md"));
        docs.filter = "zz".into();
        assert_eq!(docs.step_selection(1), None);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mkrew-ui --lib the_filter_ranks arrows_in_the_filter`
Expected: FAIL to compile (no field `filter`).

- [ ] **Step 3: `Docs`.** In `docs.rs`:
  - Add `use crate::palette;`.
  - Add the field after `mode`:

    ```rust
        /// The list filter (fuzzy over the path, SPEC §12); empty = every file. Kept across
        /// relists.
        pub filter: String,
    ```

  - Initialize it with `filter: String::new(),`.
  - Add after `selected_file`:

```rust
    /// The listed files the filter keeps, best match first (the daemon's order on ties and
    /// with an empty filter), each with its matched char indices in `file_path`.
    pub fn shown_files(&self) -> Vec<(&DocFile, Vec<u32>)> {
        let Files::Loaded(files) = &self.files else {
            return Vec::new();
        };
        let mut shown: Vec<(i64, &DocFile, Vec<u32>)> = files
            .iter()
            .filter_map(|file| {
                let (score, indices) = palette::fuzzy(&self.filter, &file.file_path)?;
                Some((score, file, indices))
            })
            .collect();
        // Stable: ties keep the listed order.
        shown.sort_by_key(|(score, _, _)| std::cmp::Reverse(*score));
        shown
            .into_iter()
            .map(|(_, file, indices)| (file, indices))
            .collect()
    }

    /// ↑ / ↓ in the filter: the shown file `offset` places from the selected one, clamped
    /// (the first shown one when the selection is filtered out); `None` without shown files.
    pub fn step_selection(&self, offset: isize) -> Option<String> {
        let shown = self.shown_files();
        let last = shown.len().checked_sub(1)?;
        let at = shown
            .iter()
            .position(|(file, _)| Some(file.file_path.as_str()) == self.selected.as_deref());
        let next = at.map_or(0, |at| at.saturating_add_signed(offset).min(last));
        shown.get(next).map(|(file, _)| file.file_path.clone())
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui --lib docs`
Expected: PASS.

- [ ] **Step 5: Find also in Docs.**
  - **`keymap.rs`:** set `pub const FIND_CONTEXT: &str = "Terminal || Files || Docs";`. Update its doc to "…, the Files view's opened file, and the Docs list filter."
  - **`palette.rs`:**
    - label: `"Find: in scrollback, the open file or the Docs list"`;
    - availability: `(on_panes && context.terminal) || in_view(View::Files) || in_view(View::Docs)`.
  - **SPEC §13 row:** `| Terminal / Files / Docs | Find in scrollback / in the opened file / filter the Docs list | Ctrl+Shift+F |`.

- [ ] **Step 6: Shell.** Add a field `docs_filter_focus: FocusHandle` (doc: `/// The Docs list filter field.`), initialized with `cx.focus_handle()`.

Add next to `select_doc`:

```rust
    /// The Docs filter field: typing filters, ↑ / ↓ show the previous / next shown file,
    /// Esc clears the filter (then leaves the field), Enter leaves it.
    fn docs_filter_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(project_id), Some(key)) = (self.workspace, quick_open::key(&event.keystroke))
        else {
            return;
        };
        let paste = (key == quick_open::Key::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(docs) = self.docs.get_mut(&project_id) else {
            return;
        };
        match key {
            quick_open::Key::Close if !docs.filter.is_empty() => docs.filter.clear(),
            quick_open::Key::Close | quick_open::Key::Open => {
                window.focus(&self.focus_handle, cx);
            }
            quick_open::Key::Move(by) => {
                if let Some(path) = docs.step_selection(by as isize) {
                    self.select_doc(project_id, &path, window, cx);
                }
            }
            quick_open::Key::Backspace => {
                docs.filter.pop();
            }
            quick_open::Key::Paste => {
                docs.filter.push_str(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => docs.filter.push_str(&text),
        }
        cx.notify();
    }
```

In `Shell::run`, add a guard for the filter field next to Task 5's guard:

```rust
        // Same for the Docs filter while it has the keyboard.
        if self.docs_filter_focus.is_focused(window) {
            if self.view() == View::Docs {
                cx.propagate();
                return;
            }
            window.focus(&self.focus_handle, cx);
        }
```

Then make the `Command::Find` arm:

```rust
            // A focused terminal pane handles it itself.
            Command::Find => match self.view() {
                View::Files => self.open_file_find(window, cx),
                View::Docs => {
                    window.focus(&self.docs_filter_focus, cx);
                    cx.notify();
                }
                _ => {}
            },
```

- [ ] **Step 7: Render.** In `render_docs`, replace the `doc-list` child with a column that holds the field and the list:

```rust
        let shown_files = docs.shown_files();
        let filter_focused = self.docs_filter_focus.is_focused(window);
        let list = div()
            .flex_none()
            .w(px(theme::DOC_LIST_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .child(
                div().px(px(space::S)).pb(px(space::XS)).child(
                    text_field("docs-filter", &docs.filter, "Filter files…", filter_focused, theme)
                        .track_focus(&self.docs_filter_focus)
                        .on_key_down(cx.listener(Self::docs_filter_key_down))
                        .relative()
                        .child(self.typing(&self.docs_filter_focus, Self::docs_filter_key_down, cx))
                        .tooltip(tooltip(theme, "Filter the files", self.keymap.label(Command::Find)))
                        .on_click(cx.listener(|shell, _, window, cx| {
                            window.focus(&shell.docs_filter_focus, cx);
                            cx.notify();
                        })),
                ),
            )
            .child(
                div()
                    .id("doc-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .px(px(space::S))
                    .pb(px(space::S))
                    .when(shown_files.is_empty(), |el| {
                        el.child(
                            div()
                                .px(px(space::S))
                                .text_size(px(font_size::META))
                                .text_color(rgb(theme.text_muted))
                                .child("No files match the filter."),
                        )
                    })
                    .children(shown_files.into_iter().map(|(file, matched)| {
                        let shown = selected == Some(file.file_path.as_str());
                        doc_row(project_id, file, &matched, shown, theme, cx)
                    })),
            );
```

Then use `.child(list)` in place of the old list child.

`doc_row` gets a new parameter `matched: &[u32]` after `file`. Import `HighlightStyle`, `Hsla` and `StyledText` from `gpui` in `main.rs` if any is missing. Inside `doc_row`, after computing `(folder, name)`, add:

```rust
    // `matched` indexes chars of the whole path: split it over the folder and the name.
    let name_from = folder.map_or(0, |folder| folder.chars().count() + 1);
    let name_from = u32::try_from(name_from).unwrap_or(u32::MAX);
    let name_hits: Vec<u32> = matched.iter().filter_map(|i| i.checked_sub(name_from)).collect();
    let folder_hits: Vec<u32> = matched.iter().copied().filter(|i| *i < name_from).collect();
    let marked = |text: &str, hits: &[u32]| {
        let style = HighlightStyle {
            color: Some(Hsla::from(rgb(theme.accent))),
            font_weight: Some(FontWeight(weight::SEMIBOLD)),
            ..HighlightStyle::default()
        };
        StyledText::new(text.to_owned()).with_highlights(
            quick_open::match_ranges(text, hits)
                .into_iter()
                .map(move |range| (range, style)),
        )
    };
```

Use `.child(marked(name, &name_hits))` and `.child(marked(folder, &folder_hits))` instead of the plain `name.to_owned()` and `folder.to_owned()` children.

The `/` between folder and name is never shown. A hit on it has index `name_from - 1`. `match_ranges` drops indices past the end of the text, so that hit is ignored.

- [ ] **Step 8: SPEC §12.** In the File list bullet, append: `A filter field at the top (\`Ctrl+Shift+F\`) fuzzy-filters the list by path; ↑ / ↓ in it show the previous / next file.`

- [ ] **Step 9: Build and test**

Run: `cargo test -p mkrew-ui --lib docs keymap palette` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 10: Record (no commit)**

Suggested message: `feat(ui): filter the Docs file list`

---

### Task 7: A readable Docs preview (typography, monospace inline code, highlighted code blocks)

**Current state (verified):** In `preview.rs`:
- h3–h6 use the body size (13 px). Only h1 (20) and h2 (15) differ, and headings have no spacing below.
- Body text is 13 px with GPUI's default line height and a uniform 12 px gap.
- The column is left-aligned, not centered (main.rs `render_doc` Preview arm, ~7512).
- Inline code only gets a background. It keeps the same proportional font.
- Code blocks drop their language, have no syntax colors, and use 12 px.
- Number markers have no fixed width, so items do not line up.
- Quotes are muted text.

Other constraints:
- SPEC §12 says "No syntax highlighting in v1". This task changes that, so it needs ADR 0020. Task 8 adds diagrams to the same ADR.
- Parsing currently runs on the UI thread (`Docs::read` → `Shown::new`). Highlighting and drawing make that slow. So it moves to the background executor, as `read_file` already does for `Code::new`.

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs` (`pub mod doc`)
- Modify: `crates/mkrew-ui/src/markdown.rs` (`walk`)
- Modify: `crates/mkrew-ui/src/highlight.rs` (`fenced`)
- Modify: `crates/mkrew-ui/src/docs.rs` (`Shown.code`, `Docs::read` takes `Shown`, tests)
- Modify: `crates/mkrew-ui/src/preview.rs`: `blocks`/`block` take `&Shown`; headings, lists, quote, `code_block`, inline monospace
- Modify: `crates/mkrew-ui/src/main.rs` (`read_doc` off-thread, `preview_blocks`, the Preview container)
- Modify: `crates/mkrew-ui/tests/docs_view.rs` (`read` parses before `Docs::read`)
- Create: `docs/decisions/0020-docs-preview-rendering.md`
- Modify: `docs/SPEC.md` §12, §14

**Interfaces:**
- Produces:
  - `theme::doc::{BODY, CODE, HEADINGS, LINE_HEIGHT, HEADING_LINE_HEIGHT, CODE_LINE_HEIGHT, MARKER_WIDTH}`
  - `markdown::walk(blocks: &[Block], f: &mut impl FnMut(&Block))`
  - `highlight::fenced(lang: Option<&str>, text: &str) -> Code`
  - `Shown.code: HashMap<u32, Code>`, keyed by a code block's first source line
  - `Docs::read(&mut self, file_path: &str, read: Result<Shown, String>)`
  - `preview::blocks(blocks: &[Block], shown: &Shown, theme: &Theme)`
  - `preview::block(block: &Block, shown: &Shown, theme: &Theme)`
- Consumes: `preview::code_line(text, spans, found, theme)` (Task 5).

- [ ] **Step 1: Failing test.** In the `docs.rs` tests, add a helper and a test:

```rust
    fn shown(path: &str, text: &str, hash: &str) -> Shown {
        Shown::new(&content(path, text, hash))
    }

    #[test]
    fn code_blocks_are_highlighted_once_also_inside_lists() {
        let doc = "```rust\nfn main() {}\n```\n\n- item\n\n  ```\n  plain\n  ```\n";
        let shown = shown("a.md", doc, "h");
        let rust = &shown.code[&1];
        assert_eq!(rust.syntax.as_deref(), Some("Rust"));
        assert!(!rust.spans[0].is_empty(), "colored");
        let nested = shown.code.get(&7).expect("the code block in the list item");
        assert_eq!(nested.lines, ["plain"]);
        assert_eq!(nested.syntax, None);
    }
```

`Docs::read` will take a parsed `Shown`. So switch every existing `docs.read(…, Ok(content(…)))` call in these tests to `Ok(shown(…))`, with the same arguments.

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p mkrew-ui --lib code_blocks_are_highlighted`
Expected: FAIL to compile (no field `code`; `read` expects `DocContent`).

- [ ] **Step 3: Pure pieces.**

**`markdown.rs`:** add after `parse`:

```rust
/// Calls `f` on every block, nested ones (list items, quotes) included, in document order.
pub fn walk<'a>(blocks: &'a [Block], f: &mut impl FnMut(&'a Block)) {
    for block in blocks {
        f(block);
        match &block.kind {
            BlockKind::List { items, .. } => {
                for item in items {
                    walk(&item.blocks, f);
                }
            }
            BlockKind::Quote(inner) => walk(inner, f),
            _ => {}
        }
    }
}
```

Change the `Code` variant's doc comment from "no syntax highlighting in v1" to "highlighted by the preview (`highlight::fenced`)".

**`highlight.rs`:** add after the `impl` block of `Code::plain`:

```rust
/// A fenced code block of the Docs preview (SPEC §12): `text` split like a file, highlighted
/// by its fence language (`rust`, `ts`, `sh`, …); unknown languages and blocks over the
/// highlighting limits stay plain. Slow on big blocks: run it off the UI thread.
pub fn fenced(lang: Option<&str>, text: &str) -> Code {
    let mut code = Code::plain(text);
    let long = code.lines.iter().any(|line| line.len() > HIGHLIGHT_MAX_LINE);
    if text.len() > HIGHLIGHT_MAX_BYTES || long {
        return code;
    }
    if let Some(syntax) = lang.and_then(|lang| find_syntax("", Some(lang))) {
        code.syntax = Some(syntax.name.clone());
        code.spans = highlight(&code.lines, syntax);
    }
    code
}
```

**`docs.rs`:** import `std::collections::HashMap`, `crate::highlight::{self, Code}` and `crate::markdown::{self, Block, BlockKind}`. Change `Shown`:

```rust
/// A read file, parsed once (off the UI thread: highlighting is slow).
#[derive(Debug, PartialEq)]
pub struct Shown {
    pub hash: String,
    pub blocks: Vec<Block>,
    /// Source lines without their line ending; line `n` (1-based) is `lines[n - 1]`.
    pub lines: Vec<String>,
    /// Every code block, highlighted, by its first source line (ADR 0020).
    pub code: HashMap<u32, Code>,
}

impl Shown {
    pub fn new(content: &DocContent) -> Shown {
        let blocks = markdown::parse(&content.content);
        let mut code = HashMap::new();
        markdown::walk(&blocks, &mut |block| {
            if let BlockKind::Code { lang, text } = &block.kind {
                code.insert(block.lines.start, highlight::fenced(lang.as_deref(), text));
            }
        });
        Shown {
            hash: content.hash.clone(),
            lines: source_lines(&content.content),
            blocks,
            code,
        }
    }
}
```

Change `Docs::read` to take the parsed file:

```rust
    /// A `doc.read` answer, parsed; ignored when another file got selected meanwhile, or when
    /// it predates the last `doc.changed` (the read that change asked for follows).
    pub fn read(&mut self, file_path: &str, read: Result<Shown, String>) {
        if self.selected.as_deref() != Some(file_path) {
            return;
        }
        if let (Ok(shown), Some(changed_to)) = (&read, &self.changed_to)
            && shown.hash != *changed_to
        {
            return;
        }
        self.doc = match read {
            Ok(shown) => Doc::Shown(Rc::new(shown)),
            Err(error) => Doc::Failed(error),
        };
    }
```

- [ ] **Step 4: Run the docs tests**

Run: `cargo test -p mkrew-ui --lib docs markdown highlight`
Expected: PASS.

- [ ] **Step 5: Off-thread parse.** In `main.rs` `read_doc`, parse in the background, the way `read_file` builds `Code`:

```rust
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            let read = match read.await {
                Ok(content) => Ok(background.spawn(async move { Shown::new(&content) }).await),
                Err(error) => Err(format!("{error:#}")),
            };
            // … the existing `this.update(…)` body, unchanged (`docs.read(&file_path, read)`)
```

Import `mkrew_ui::docs::Shown` if it is not imported yet.

In `crates/mkrew-ui/tests/docs_view.rs` `read`, map the answer before handing it over: `let read = read.map(|content| Shown::new(&content));`. Import `Shown` there.

- [ ] **Step 6: Typography tokens.** In `theme.rs`, add after `DOC_TASK_BOX_SIZE`:

```rust
/// Docs preview typography (SPEC §12, ADR 0020): long-form reading, a step above the UI
/// scale.
pub mod doc {
    pub const BODY: f32 = 14.0;
    pub const CODE: f32 = 13.0;
    /// h1–h6.
    pub const HEADINGS: [f32; 6] = [24.0, 20.0, 17.0, 15.0, 14.0, 14.0];
    /// Line heights as multiples of the font size.
    pub const LINE_HEIGHT: f32 = 1.6;
    pub const HEADING_LINE_HEIGHT: f32 = 1.3;
    pub const CODE_LINE_HEIGHT: f32 = 1.45;
    /// The marker column of list items (bullets, numbers, check boxes line up).
    pub const MARKER_WIDTH: f32 = 20.0;
}
```

- [ ] **Step 7: Preview rendering.** In `preview.rs`, import `gpui::relative`, `crate::docs::Shown` and `crate::highlight::Code`. Then:

**(a) Pass `shown` through.** `blocks` and `block` take `shown: &Shown` after the block(s) and pass it on. So does `list_item(item, number, shown, theme)`. Quote calls `blocks(inner, shown, theme)`.

**(b) Headings.** The heading arm becomes:

```rust
        BlockKind::Heading { level, runs } => {
            let size = (theme::doc::HEADINGS.get(usize::from(*level).saturating_sub(1)))
                .copied()
                .unwrap_or(theme::doc::BODY);
            let major = *level <= 2;
            div()
                .pt(px(if major { space::L } else { space::S }))
                .text_size(px(size))
                .line_height(relative(theme::doc::HEADING_LINE_HEIGHT))
                .font_weight(FontWeight(weight::SEMIBOLD))
                .text_color(rgb(theme.text))
                .when(major, |el| {
                    el.pb(px(space::XS))
                        .border_b_1()
                        .border_color(rgb(theme.border))
                })
                .child(text(runs, theme))
                .into_any_element()
        }
```

**(c) Quotes.** The inner column uses `.text_color(rgb(theme.text))`, so it is readable. The bar uses `theme.pane_focus_border` (a faint accent) instead of `theme.border`.

**(d) Code blocks.** The code arm becomes:

```rust
        BlockKind::Code { lang, text } => match shown.code.get(&block.lines.start) {
            Some(highlighted) => code_block(lang.as_deref(), highlighted, theme).into_any_element(),
            None => code(text, theme.text, theme).into_any_element(),
        },
```

**(e) List markers.** In `list_item`, give the marker a fixed column. Replace `.child(marker.flex_none())` with:

```rust
        .child(
            div()
                .flex_none()
                .w(px(theme::doc::MARKER_WIDTH))
                .flex()
                .justify_end()
                .child(marker),
        )
```

**(f) `code_block`.** Add after `code`:

```rust
/// A fenced code block: its language muted above, then its lines with syntax colors in the
/// monospaced font on a raised surface (long lines wrap).
fn code_block(lang: Option<&str>, code: &Code, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .bg(rgb(theme.surface_raised))
        .children(lang.map(|lang| {
            div()
                .text_size(px(font_size::META))
                .text_color(rgb(theme.text_muted))
                .child(lang.to_owned())
        }))
        .child(
            div()
                .flex()
                .flex_col()
                .font_family(theme.terminal_font_family)
                .text_size(px(theme::doc::CODE))
                .line_height(relative(theme::doc::CODE_LINE_HEIGHT))
                .text_color(rgb(theme.text))
                .children(code.lines.iter().zip(&code.spans).map(|(line, spans)| {
                    // An empty line still takes a line's height.
                    let line = if line.is_empty() { " ".to_owned() } else { line.clone() };
                    div().child(code_line(line, spans, &[], theme))
                })),
        )
}
```

**(g) Inline code.** Make it monospace in `text`. Build the overrides before `styled` is moved, and chain them on:

```rust
    let mono: Vec<(Range<usize>, SharedString)> = styled
        .iter()
        .filter(|(_, style, _)| style.code)
        .map(|(range, _, _)| (range.clone(), SharedString::from(theme.terminal_font_family)))
        .collect();
    // … existing `highlights` …
    StyledText::new(text)
        .with_highlights(highlights)
        .with_font_family_overrides(mono)
```

The overrides must be sorted and non-overlapping. `inline` yields its runs left to right, already merged, so they are.

- [ ] **Step 8: Preview container.** In `main.rs`:
  - `preview_blocks` takes `shown: &Shown` instead of `blocks: &[Block]`. It iterates `shown.blocks` and calls `preview::block(block, shown, theme)`.
  - Its caller in `render_doc` passes `shown`.
  - Replace the Preview arm's inner column with a centered, readable column:

```rust
                Mode::Preview => div()
                    .id("doc-preview")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(space::XL))
                    .py(px(space::L))
                    .child(
                        div()
                            .w_full()
                            .max_w(px(theme::DOC_PREVIEW_MAX_WIDTH))
                            .mx_auto()
                            .flex()
                            .flex_col()
                            .gap(px(space::M))
                            .text_size(px(theme::doc::BODY))
                            .line_height(relative(theme::doc::LINE_HEIGHT))
                            .children(preview_blocks(
                                project_id,
                                shown,
                                docs.comments.selection,
                                theme,
                                cx,
                            )),
                    )
                    .into_any_element(),
```

- [ ] **Step 9: ADR 0020 + SPEC.** List `docs/decisions/` first.

ADR "Docs preview rendering":
- **Context:** the user found the preview hard to read and wants Mermaid diagrams.
- **Decision (this task):**
  - doc typography tokens: body 14/1.6, headings 24/20/17/15/14/14, a centered 720 px column;
  - monospace inline code;
  - fenced code highlighted by syntect, with the same tokens as the Files viewer;
  - parsing off the UI thread.
- Task 8 appends the Mermaid decision.

SPEC:
- **§12 Rendering bullet:** replace "No syntax highlighting in v1." with "Code blocks are highlighted by their fence language (syntect, as in the Files viewer); the text uses the doc typography of §14 (ADR 0020)."
- **§14:** under the font sizes line, add `- Docs preview (ADR 0020): body 14 px, line height 1.6, headings 24/20/17/15/14/14 px, code 13 px, a centered column of at most 720 px.`

- [ ] **Step 10: Build and test**

Run:
- `cargo test -p mkrew-ui --lib docs markdown preview highlight`
- `cargo test -p mkrew-ui --test docs_view`
- `cargo clippy -p mkrew-ui --all-targets -- -D warnings`

Expected: PASS, no warnings. The markdown snapshots do not change, because the model did not change.

- [ ] **Step 11: Record (no commit)**

Suggested message: `feat(ui): a readable Docs preview with highlighted code blocks`

> Skipped: clickable links, inline images, per-depth bullet glyphs, horizontal scrolling for code. Add them when a doc needs them.

---

### Task 8: Mermaid diagrams in the Docs preview

**Current state (verified):**
- A ```mermaid fence parses as `BlockKind::Code { lang: Some("mermaid") }` and is shown as source. Nothing in the tree renders Mermaid.
- `gpui-pre 0.3.7` already ships resvg/usvg 0.46:
  - `gpui::img(Arc<gpui::Image>)` with `Image::from_bytes(ImageFormat::Svg, bytes)` draws a **full-color** SVG through gpui's svg renderer.
  - `gpui::svg()` is monochrome and meant for icons. Do not use it here.
- `mermaid-rs-renderer 0.3.1` is pure Rust. It draws flowchart, sequence, class, state, ER, pie, gantt, timeline, journey, mindmap, gitGraph and more.
- A spike on this machine with `default-features = false` showed:
  - it compiles;
  - usvg 0.46 parses its SVG;
  - a bad source returns `ParseError` "unexpected token … expected invalid flowchart edge syntax: A -->";
  - the first render takes ~3.6 s in a debug build, because it loads the system fonts once; later renders take ~2 ms.

  So the render runs in `Shown::new`, which Task 7 moved to the background executor.
- Its pipeline (`parse_mermaid_strict` → `compute_layout` → `render_svg`) lets one layout be drawn in two color themes. Dark and light are both ready, so switching the appearance needs no re-render.

**Files:**
- Modify: `crates/mkrew-ui/Cargo.toml` (dependency line only)
- Create: `crates/mkrew-ui/src/diagram.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod diagram;`)
- Modify: `crates/mkrew-ui/src/theme.rs` (`Theme.dark`, `DIAGRAM_FONT`)
- Modify: `crates/mkrew-ui/src/docs.rs` (`Shown.diagrams`, test)
- Modify: `crates/mkrew-ui/src/preview.rs` (code arm, `diagram_view`)
- Modify: `docs/decisions/0020-docs-preview-rendering.md`, `docs/SPEC.md` §4, §12

**Interfaces:**
- Produces:
  - `diagram::Diagram { pub light: Arc<gpui::Image>, pub dark: Arc<gpui::Image>, pub width: f32, pub height: f32 }`
  - `Diagram::image(&self, theme: &Theme) -> Arc<gpui::Image>`
  - `diagram::render(source: &str) -> Result<Diagram, String>`
  - `Shown.diagrams: HashMap<u32, Result<Diagram, String>>`
  - `Theme.dark: bool`
  - `theme::DIAGRAM_FONT: &str`
- Consumes: `Shown`, `markdown::walk`, `preview::code_block` (Task 7).

- [ ] **Step 1: Dependency.** In `crates/mkrew-ui/Cargo.toml` `[dependencies]`, add in its alphabetical place. Keep the user's x11/wayland edits untouched.

```toml
# Mermaid diagrams of the Docs preview (ADR 0020): pure Rust, SVG only (no CLI, no PNG).
mermaid-rs-renderer = { version = "0.3.1", default-features = false }
```

- [ ] **Step 2: Failing tests.** Create `crates/mkrew-ui/src/diagram.rs` with the tests, and add `pub mod diagram;` to `lib.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowcharts_draw_in_both_appearances_and_bad_sources_say_why() {
        let drawn = render("flowchart LR\n  A[Plan] --> B{Review}\n  B -->|ok| C[Ship]\n").unwrap();
        assert!(drawn.width > 0.0 && drawn.height > 0.0);
        let svg = |image: &Arc<Image>| String::from_utf8(image.bytes.clone()).unwrap();
        let surface = |theme: Theme| format!("#{:06X}", theme.surface);
        assert!(svg(&drawn.dark).contains(&surface(Theme::dark())));
        assert!(svg(&drawn.light).contains(&surface(Theme::light())));
        assert_ne!(drawn.dark, drawn.light);
        let bad = render("flowchart LR\n  A -->").unwrap_err();
        assert!(bad.contains("A -->"), "{bad}");
    }
}
```

In the `docs.rs` tests:

```rust
    #[test]
    fn mermaid_blocks_are_drawn_and_other_code_is_not() {
        let doc = "```mermaid\ngraph TD\n  A --> B\n```\n\n```sh\nls\n```\n";
        let shown = shown("a.md", doc, "h");
        assert!(matches!(shown.diagrams.get(&1), Some(Ok(_))));
        assert!(!shown.diagrams.contains_key(&6));
        assert!(shown.code.contains_key(&1), "the source stays for a failed drawing");
    }
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -p mkrew-ui --lib flowcharts_draw mermaid_blocks_are_drawn`
Expected: FAIL to compile (`render`, `Diagram`, `diagrams` not found).

- [ ] **Step 4: Theme.** In `theme.rs`, add the field `pub dark: bool,` to `Theme`, with the doc `/// Dark appearance (picks diagram images, ADR 0020).` Set it to `true` in `dark()`, and to `false` in `light()` and `shared()`.

Add after `UI_FONT`:

```rust
/// Font family list of Mermaid diagram text (an SVG font-family value; the first installed
/// one is used).
pub const DIAGRAM_FONT: &str = "Segoe UI, Helvetica Neue, DejaVu Sans, sans-serif";
```

- [ ] **Step 5: `diagram.rs`.** Put this above the tests:

```rust
//! Mermaid diagrams of the Docs preview (SPEC §12, ADR 0020): ```mermaid blocks drawn to SVG
//! by `mermaid-rs-renderer` (pure Rust, no browser) in the theme's colors, both appearances
//! at once, shown with `gpui::img`. Slow the first time (it loads the system fonts): run it
//! off the UI thread.

use std::sync::Arc;

use gpui::{Image, ImageFormat};
use mermaid_rs_renderer::{
    LayoutConfig, compute_layout, measure_svg_dimensions, parse_mermaid_strict, render_svg,
};

use crate::theme::{self, Theme};

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub light: Arc<Image>,
    pub dark: Arc<Image>,
    /// Natural size, in px.
    pub width: f32,
    pub height: f32,
}

impl Diagram {
    /// The drawing for `theme`'s appearance.
    pub fn image(&self, theme: &Theme) -> Arc<Image> {
        Arc::clone(if theme.dark { &self.dark } else { &self.light })
    }
}

/// Draws `source`; `Err` says why it cannot be drawn (shown above its source).
pub fn render(source: &str) -> Result<Diagram, String> {
    let parsed = parse_mermaid_strict(source).map_err(|error| error.to_string())?;
    let config = LayoutConfig::default();
    let light = colors(&Theme::light());
    // Both themes share fonts and sizes, so one layout fits both.
    let layout = compute_layout(&parsed.graph, &light, &config);
    let size = measure_svg_dimensions(&layout, &config, None);
    let image = |colors: &mermaid_rs_renderer::Theme| {
        let svg = render_svg(&layout, colors, &config);
        Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes()))
    };
    Ok(Diagram {
        light: image(&light),
        dark: image(&colors(&Theme::dark())),
        width: size.width,
        height: size.height,
    })
}

/// The renderer's theme in Mkrew's tokens (SPEC §14).
fn colors(theme: &Theme) -> mermaid_rs_renderer::Theme {
    let hex = |color: u32| format!("#{color:06X}");
    mermaid_rs_renderer::Theme {
        font_family: theme::DIAGRAM_FONT.to_owned(),
        font_size: theme::doc::BODY,
        background: hex(theme.surface),
        primary_color: hex(theme.surface_raised),
        primary_text_color: hex(theme.text),
        primary_border_color: hex(theme.text_muted),
        line_color: hex(theme.text_muted),
        secondary_color: hex(theme.surface_raised),
        tertiary_color: hex(theme.surface),
        edge_label_background: hex(theme.surface),
        cluster_background: hex(theme.bg),
        cluster_border: hex(theme.border),
        text_color: hex(theme.text),
        sequence_actor_fill: hex(theme.surface_raised),
        sequence_actor_border: hex(theme.text_muted),
        sequence_actor_line: hex(theme.text_muted),
        sequence_note_fill: hex(theme.terminal_match_bg),
        sequence_note_border: hex(theme.status_needs_input),
        sequence_activation_fill: hex(theme.surface_raised),
        sequence_activation_border: hex(theme.text_muted),
        pie_title_text_color: hex(theme.text),
        pie_section_text_color: hex(theme.text),
        pie_legend_text_color: hex(theme.text),
        ..mermaid_rs_renderer::Theme::modern()
    }
}
```

The field names come from `mermaid-rs-renderer 0.3.1` `src/theme.rs`. If the compiler disagrees, check `~/.cargo/registry/src/*/mermaid-rs-renderer-0.3.1/src/theme.rs`. Do not guess.

- [ ] **Step 6: `Shown.diagrams`.** In `docs.rs`, import `crate::diagram::{self, Diagram}` and add the field:

```rust
    /// Every ```mermaid block drawn (or why not), by its first source line (ADR 0020).
    pub diagrams: HashMap<u32, Result<Diagram, String>>,
```

Add `let mut diagrams = HashMap::new();` next to `code` in `Shown::new`. Fill it in the same `walk` as `code`:

```rust
            if let BlockKind::Code { lang, text } = &block.kind {
                code.insert(block.lines.start, highlight::fenced(lang.as_deref(), text));
                if lang.as_deref().is_some_and(|lang| lang.eq_ignore_ascii_case("mermaid")) {
                    diagrams.insert(block.lines.start, diagram::render(text));
                }
            }
```

- [ ] **Step 7: Preview.** In `preview.rs`, import `gpui::img` and `crate::diagram::Diagram`. Make the code arm:

```rust
        BlockKind::Code { lang, text } => {
            let start = block.lines.start;
            let source = || match shown.code.get(&start) {
                Some(highlighted) => code_block(lang.as_deref(), highlighted, theme),
                None => code(text, theme.text, theme),
            };
            match shown.diagrams.get(&start) {
                Some(Ok(diagram)) => diagram_view(diagram, theme).into_any_element(),
                Some(Err(error)) => div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .child(
                        div()
                            .text_size(px(font_size::META))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("Cannot draw this diagram: {error}")),
                    )
                    .child(source())
                    .into_any_element(),
                None => source().into_any_element(),
            }
        }
```

Add:

```rust
/// A drawn Mermaid diagram on a bordered card, at its natural size, at most as wide as the
/// preview.
fn diagram_view(diagram: &Diagram, theme: &Theme) -> gpui::Div {
    div()
        .flex()
        .justify_center()
        .p(px(space::M))
        .rounded(px(radius::MEDIUM))
        .border_1()
        .border_color(rgb(theme.border))
        .bg(rgb(theme.surface))
        .child(img(diagram.image(theme)).w(px(diagram.width)).max_w_full())
}
```

- [ ] **Step 8: Run the tests**

Run: `cargo test -p mkrew-ui --lib diagram docs preview` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS, no warnings. The first diagram test takes a few seconds, because it loads the system fonts.

- [ ] **Step 9: ADR 0020 + SPEC.**

Append to ADR 0020:
- **Decision:** Mermaid via `mermaid-rs-renderer` 0.3.1 (`default-features = false`; pure Rust, MIT).
- **Alternatives rejected:**
  - mermaid-cli/Node: needs a browser, which breaks "lightweight" and §1.
  - A WebView: not GPUI.
  - Showing the source only: that is what the user complained about.
- **Consequences:**
  - One more crate, with its own fontdb. It loads the system fonts once, which takes seconds in debug. Measure the UI memory against the 150 MB budget in the manual check.
  - Unsupported diagram types show their source with the reason.

SPEC:
- **§4 table:** add the row `| Diagrams | \`mermaid-rs-renderer\` (no default features) | Mermaid in the Docs preview (ADR 0020) |`.
- **§12 Rendering:** add "```mermaid blocks are drawn as diagrams (pure Rust renderer, theme colors); a block that cannot be drawn shows its source and the reason."

- [ ] **Step 10: Record (no commit)**

Suggested message: `feat(ui): draw Mermaid diagrams in the Docs preview`

---

### Task 9: Full verification, manual checks, notes

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (new "To verify" entry)
- Modify: `docs/PROGRESS.md` ("Notes for the next session")

- [ ] **Step 1: Full verify**

Run (PowerShell): `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`

If rustc crashes on mkrew-ui test builds, run instead, in order:
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo test --workspace -j 4`

Expected: fmt clean, no clippy warnings, all tests pass.

- [ ] **Step 2: HUMAN_CHECKS entry.** Add a "To verify — 2026-10-07 workspace, files and docs fixes" section. It lists the checks the user runs with `cargo run -p mkrew-ui`. The protocol is now v25, so stop the old daemon first: `target\debug\mk daemon stop`.
  1. **Remove a workspace.** Workspace menu → "Remove workspace…" on a workspace with a running agent and its dev server. Expected:
     - a confirmation that names the sessions;
     - after Remove, the row is gone, the agents stop, and another workspace is shown.

     Then add the same folder again. It returns with its tasks and history.
  2. **Remove an agent row.**
     - Hover an exited agent row: ✕ removes it.
     - On a running one, ✕ asks first, then stops it and closes its pane.
     - Restart the app: removed rows stay gone.
  3. **Workspace color.** Workspace menu → Color: pick teal, then "No color". The dot before the name follows, in the dark and light themes.
  4. **Full-width rows.** In the Files tree and the Git list, click anywhere on a row, also right of the name. The whole row width highlights, and the markers sit at the right edge.
  5. **Find in file.** In Files, open a file, press Ctrl+Shift+F (or "Find"), and type `việt` / `TODO`. Expected:
     - matches are highlighted;
     - Enter, Shift+Enter, ↑ and ↓ move between them and scroll;
     - "N of M" updates;
     - Esc closes the bar.

     Let an agent edit the open file: the count follows.
  6. **Docs filter.** In Docs, Ctrl+Shift+F focuses "Filter files…". Expected:
     - typing filters the list, with the matched letters bold;
     - ↑/↓ open the previous/next file;
     - Esc clears the filter.
  7. **Docs preview.** Open a plan in dark and light. It should contain:
     - headings h1–h4;
     - nested and numbered lists, a table and a quote;
     - `inline code`;
     - ```rust, ```ts and ```mermaid blocks (a flowchart and a sequenceDiagram).

     Expected:
     - a readable, centered column;
     - colored code;
     - diagrams drawn in theme colors;
     - a broken mermaid block shows its source and the reason.

     Retake the §14 screenshots.
  8. **Memory.** With a doc that contains 3 diagrams open:
     - the UI process stays under 150 MB (Task Manager);
     - in a release build (`cargo run --release -p mkrew-ui`), the first diagram shows within ~1 s.

- [ ] **Step 3: PROGRESS note.** Under "Notes for the next session", add one bullet:

`- 2026-10-07 workspace/files/docs fixes (plan docs/superpowers/plans/2026-10-07-workspace-files-docs-fixes.md): protocol v25 (project.archive, session.remove, Project.color), migrations 0011 session.hidden / 0012 project.color, ADR 0018–0020, new crate mermaid-rs-renderer, text_find.rs, diagram.rs; manual checks in HUMAN_CHECKS "2026-10-07 workspace, files and docs fixes". Outside the milestone loop.`

Use the real ADR and migration numbers if they shifted. Change the existing "Protocol is v24" mention to v25. Keep `status: DONE`.

- [ ] **Step 4: Record (no commit)**

Suggested message: `docs: manual checks and notes for the workspace, files and docs fixes`
