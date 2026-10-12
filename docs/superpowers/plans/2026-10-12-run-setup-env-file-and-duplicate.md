# Run setup: env file independent of Folder, Duplicate command — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A run command's Env file no longer depends on its Folder (it is relative to the workspace
root), so a copy of `BE` can run in `.worktrees/<name>/EA-BE` with the main `EA-BE/.env`; and each
Run setup card gets a `Duplicate` button. Separately, an agent pane header's "Follow in Zed" button
becomes a `Share` button opening a small menu whose one item, for now, is "Follow in Zed". And the
Files view puts its tree on the right with a cleaner search row and a compact "Ignored" toggle.

**Architecture:** `RunCommand.env_file` changes meaning from "relative to `dir`" to "relative to
the workspace folder". Migration 0018 rewrites saved rows (`dir/env_file`) so existing setups keep
working. The daemon's `prepare` joins the env file to the project folder. The Run setup picker for
Env file starts in the card's folder but may go up to the workspace root and returns a
workspace-relative path. `RunSetup::duplicate_card` clones a card under a free name.

**Tech Stack:** Rust, rusqlite migrations (`crates/mkrew-store/migrations`), GPUI (`crates/mkrew-ui`).

**Spec:** `docs/SPEC.md` §5 (`run_command` table), §7 (`project.update`, `project.run_start`),
§10 (Run setup dialog); ADR 0018, ADR 0048. Deviation recorded in new ADR 0049.

## Background (why)

Workspace EA (`C:\Users\NguyenTung\Workspace\SourceCode\EA`) has `BE` = folder `./EA-BE`,
command `.venv\Scripts\python.exe main.py`, env file `.env`. The user keeps git worktrees in
`EA/.worktrees/<name>/EA-BE`; some (e.g. `fix-alembic-heads`) have no `.env` and no `.venv`.
Today a copy of `BE` with folder `.worktrees/fix-alembic-heads/EA-BE` cannot use `EA-BE/.env`:
`env_file` is joined to the folder (`prepare`: `dir.join(file)`), and `normalize` rejects `..`.

The **command** is typed into the shell started in the folder, so it is already free: an absolute
path (`C:\…\EA\EA-BE\.venv\Scripts\python.exe main.py`) or a folder-relative one
(`..\..\..\EA-BE\.venv\Scripts\python.exe main.py`) works today. No change for the command.

## Global Constraints

- Paths stay inside the workspace: `dir` and `env_file` are relative, no `..`, no absolute
  (`mkrew_core::run::normalize` / `inside`, unchanged).
- Max 8 commands per workspace (`MAX_COMMANDS`), name 1–20 chars without `/` `:`, unique
  (`MAX_NAME_CHARS`).
- No protocol shape change: `RunCommand` fields stay; only the meaning of `env_file` changes, so
  `PROTOCOL_VERSION` is not bumped.
- Migrations are append-only; never edit `0011_run_commands.sql`.
- UI: every color/spacing/size from `crates/mkrew-ui/src/theme.rs`; reuse `quiet_button`.
- Tests never call a real agent or the network. Verify with `sh scripts/verify.sh` (omp shell).

## Review Focus

1. Saved env files with folder `.` stay `.env` after the migration (no `./.env`, no `/.env`) —
   migration test, Task 1.
2. Folders saved with Windows separators / trailing slash (`.\EA-FE\`) migrate to a path the
   daemon can open — migration test, Task 1.
3. Rows with no env file (`NULL`) stay `NULL` — migration test, Task 1.
4. Duplicating a card whose name is already 20 chars, or whose `<name> copy` exists, still yields
   a valid unique name — Task 3.
5. Duplicate when 8 commands exist: no 9th card, the error says why — Task 3.

---

### Task 1: Env file relative to the workspace (store, daemon, spec)

**Files:**
- Create: `crates/mkrew-store/migrations/0018_run_env_file_from_workspace.sql`
- Modify: `crates/mkrew-store/src/lib.rs:17-35` (append to `MIGRATIONS`)
- Test: `crates/mkrew-store/src/tests.rs` (new test after `the_dev_command_becomes_a_run_command_named_dev`)
- Modify: `crates/mkrew-daemon/src/server/run.rs:294-317` (`prepare`)
- Modify: `crates/mkrew-daemon/tests/daemon/runs.rs:73,84-88,112,244`
- Modify: `crates/mkrew-protocol/src/message.rs:486-497` (doc comment of `RunCommand`)
- Modify: `crates/mkrew-core/src/run.rs:42-44` (doc comment of `normalize`)
- Modify: `docs/SPEC.md` (§5 line ~311, §7 `project.run_start` row ~510, §10 lines ~866-870)
- Create: `docs/decisions/0049-run-env-file-from-workspace.md`

**Interfaces:**
- Produces: `RunCommand.env_file` = path relative to the workspace folder (was: to `dir`).

- [ ] **Step 1: Write the failing migration test** in `crates/mkrew-store/src/tests.rs`:

```rust
#[test]
fn env_files_move_from_the_folder_to_the_workspace_root() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mkrew.db");
    let project = Ulid::new();
    {
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..17] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 17).unwrap();
        conn.execute(
            "INSERT INTO project (id, name, path, created_at) VALUES (?1, 'ea', '/ea', 0)",
            [project.to_string()],
        )
        .unwrap();
        let rows = [
            ("root", ".", Some(".env")),
            ("be", "./EA-BE", Some(".env")),
            ("win", r".\EA-FE\", Some(r"config\.env")),
            ("none", "web", None),
        ];
        for (position, (name, folder, env_file)) in rows.into_iter().enumerate() {
            conn.execute(
                "INSERT INTO run_command (project_id, position, name, dir, command, env_file)
                 VALUES (?1, ?2, ?3, ?4, 'x', ?5)",
                rusqlite::params![project.to_string(), position as i64, name, folder, env_file],
            )
            .unwrap();
        }
    }
    let store = Store::open(&path).unwrap();
    let files: Vec<Option<String>> = store
        .get_project(project)
        .unwrap()
        .run_commands
        .into_iter()
        .map(|c| c.env_file)
        .collect();
    assert_eq!(
        files,
        [
            Some(".env".to_owned()),
            Some("./EA-BE/.env".to_owned()),
            Some(r".\EA-FE/config\.env".to_owned()),
            None,
        ]
    );
}
```

(`Connection`, `MIGRATIONS`, `Store`, `Ulid` are already used by
`the_dev_command_becomes_a_run_command_named_dev`; use the file's import style for `params!`.)

- [ ] **Step 2: Run it, expect FAIL** (env files unchanged):
  `cargo test -p mkrew-store env_files_move_from_the_folder`

- [ ] **Step 3: Write the migration** `crates/mkrew-store/migrations/0018_run_env_file_from_workspace.sql`:

```sql
-- ADR 0049: a run command's env file is relative to the workspace folder, no longer to its
-- folder. Never edit: schema changes go in a new numbered migration.
UPDATE run_command
   SET env_file = rtrim(dir, '/\') || '/' || env_file
 WHERE env_file IS NOT NULL
   AND rtrim(dir, '/\') NOT IN ('', '.');
```

and append `include_str!("../migrations/0018_run_env_file_from_workspace.sql"),` to `MIGRATIONS`
in `crates/mkrew-store/src/lib.rs`.

- [ ] **Step 4: Run it, expect PASS:** `cargo test -p mkrew-store`

- [ ] **Step 5: Daemon reads the env file from the workspace.** In
  `crates/mkrew-daemon/src/server/run.rs` `prepare`, change

```rust
        let path = dir.join(file);
```

to

```rust
        let path = project_dir.join(file);
```

and its doc comment to:
`/// The folder and extra environment of `command` (the env file is relative to the project folder, ADR 0049), or why it cannot start.`

- [ ] **Step 6: Update the daemon tests** in `crates/mkrew-daemon/tests/daemon/runs.rs`:
  - line 73 (`BE`, folder `api`): `env_file: Some("api/.env".into()),`
  - line 244 (`api`, folder `api`): `env_file: Some("api/.env".into()),`
  - prove the decoupling in `run_commands_start_in_their_folders_with_env_side_by_side_restart_and_stop`:
    `FE` (folder `web`) reads the env file of another folder:

```rust
    let fe = RunCommand {
        env_file: Some("api/.env".into()),
        ..run_command(
            "FE",
            "web",
            &long("echo web %FROM_FILE%> web.txt", "echo web $FROM_FILE > web.txt"),
        )
    };
```

    and line 112 becomes `assert_eq!(wait_text(&repo.join("web/web.txt")), "web file");`.

- [ ] **Step 7: Run:** `cargo test -p mkrew-daemon --test daemon runs` — expect PASS.

- [ ] **Step 8: Docs.**
  - `crates/mkrew-protocol/src/message.rs` `RunCommand` doc: "typed into its own shell in `dir`,
    with `env_file` (relative to the project folder, not `dir`) then `env` added to the environment."
  - `crates/mkrew-core/src/run.rs` `normalize` doc: append "`env_file` is relative to the project
    folder, not to `dir` (ADR 0049)."
  - `docs/SPEC.md` §5: `env_file TEXT, -- relative to the project folder (ADR 0049), e.g. "EA-BE/.env"`.
  - `docs/SPEC.md` §7 `project.run_start` row: grep the row for `env file`; say "the env file
    (relative to the project folder)".
  - `docs/SPEC.md` §10 Run setup dialog: replace "for Env file it starts in (and never leaves) the
    card's folder and a click on a file takes it, relative to that folder." with "for Env file it
    starts in the card's folder (the root when that is not a folder of the workspace), can go up to
    the workspace root, and a click on a file takes it relative to the workspace (ADR 0049)."
  - ADR `docs/decisions/0049-run-env-file-from-workspace.md`:

```markdown
# 0049 — A run command's env file is relative to the workspace, not to its folder

- Date: 2026-10-12
- Status: accepted

## Context

ADR 0018/0048 resolved `env_file` from the command's folder and kept the picker inside it. A
copy of a command pointed at a git worktree of the same repo (`.worktrees/x/EA-BE`) could not
use the main checkout's `EA-BE/.env`: the worktree has none, and `..` is refused.

## Decision

- `env_file` is relative to the workspace folder; `dir` only sets the shell's working folder.
- Migration 0018 rewrites saved rows to `<dir>/<env_file>` (unchanged when `dir` is `.`).
- The Env file picker starts in the card's folder and may go up to the workspace root.
- The command is untouched: it is typed into the shell in `dir`, so an absolute path or a
  `..\` path already reaches a `.venv` elsewhere.

## Alternatives rejected

- Allow `..` and absolute env file paths, still relative to `dir`: `..\..\..\EA-BE\.env` breaks
  as soon as the folder changes; the coupling stays.
- A `MKREW_WORKSPACE` variable for commands: shell-specific syntax (`$env:` / `%…%` / `$`);
  left for when asked.

## Consequences

No protocol shape change. Saved setups show `EA-BE/.env` where they showed `.env`.
```

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-store/migrations/0018_run_env_file_from_workspace.sql crates/mkrew-store/src/lib.rs crates/mkrew-store/src/tests.rs crates/mkrew-daemon/src/server/run.rs crates/mkrew-daemon/tests/daemon/runs.rs crates/mkrew-protocol/src/message.rs crates/mkrew-core/src/run.rs docs/SPEC.md docs/decisions/0049-run-env-file-from-workspace.md
git commit -m "feat(daemon): run env file is relative to the workspace, not the folder"
```

### Task 2: Env file picker over the whole workspace (UI)

**Files:**
- Modify: `crates/mkrew-ui/src/run_setup.rs:55-74` (`Picker`), `:176-199` (`open_picker`),
  `:209-217` (`browse_up`), `:231-257` (`pick`), test at `:686-709`
- Modify: `crates/mkrew-ui/src/shell/run.rs:563-571,593` (placeholder, tooltip, label) and any
  `picker.root` use
- Modify: `docs/HUMAN_CHECKS.md` (one manual check line)

**Interfaces:**
- Consumes: Task 1's meaning of `env_file`.
- Produces: `RunSetup::open_picker(card, Field::EnvFile)` returns the card's folder (or `""`);
  `pick(Some(path))` stores the workspace-relative `path` as is.

- [ ] **Step 1: Replace the picker test** `the_env_file_picker_starts_in_the_cards_folder_and_stays_inside_it` with:

```rust
    #[test]
    fn the_env_file_picker_starts_in_the_cards_folder_and_can_leave_it() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.cards[0].dir = r".worktrees\fix\EA-BE\".into();
        assert_eq!(
            setup.open_picker(0, Field::EnvFile).as_deref(),
            Some(".worktrees/fix/EA-BE")
        );
        assert_eq!(setup.browse_up().as_deref(), Some(".worktrees/fix"));
        assert_eq!(setup.browse_up().as_deref(), Some(".worktrees"));
        assert_eq!(setup.browse_up().as_deref(), Some(""));
        assert_eq!(setup.browse_up(), None, "never above the workspace");
        setup.browse("EA-BE".into());
        setup.key(DialogKey::Enter);
        assert!(setup.picker.is_some(), "Enter picks only folders");
        setup.pick(Some("EA-BE/.env"));
        assert_eq!(setup.cards[0].env_file, "EA-BE/.env");
        assert_eq!(setup.cards[0].dir, r".worktrees\fix\EA-BE\", "the folder stays");
        assert_eq!(setup.focus, Focus::Field(0, Field::EnvFile));

        // A folder outside the workspace: the picker starts at the root.
        setup.cards[0].dir = "../other".into();
        assert_eq!(setup.open_picker(0, Field::EnvFile).as_deref(), Some(""));
        assert_eq!(setup.error, None);
    }
```

- [ ] **Step 2: Run, expect FAIL:** `cargo test -p mkrew-ui run_setup::tests::the_env_file_picker`

- [ ] **Step 3: Implement.** Both pickers now stop at the workspace root, so delete
  `Picker::root`; `at_root` becomes `self.dir.is_empty()`. `open_picker`:

```rust
    /// Opens the picker for `field` (`Dir` or `EnvFile`) of card `card`; returns the folder to
    /// `fs.list`. Both browse the whole workspace; the env file picker starts in the card's
    /// folder (the root when that folder is not one of the workspace).
    pub fn open_picker(&mut self, card: usize, field: Field) -> Option<String> {
        let dir = match field {
            Field::Dir => String::new(),
            Field::EnvFile => workspace_dir(self.cards.get(card)?.dir.as_str()).unwrap_or_default(),
            _ => return None,
        };
        self.focus = Focus::Field(card, field);
        self.picker = Some(Picker {
            card,
            field,
            dir: dir.clone(),
            entries: None,
        });
        Some(dir)
    }
```

  `pick`, env file arm:

```rust
            (Field::EnvFile, Some(path)) => card.env_file = path.into(),
```

  Grep `\.root` in `crates/mkrew-ui/src/shell/run.rs` and `run_setup.rs`; drop the uses. Update
  the `Picker` and `browse_up` doc comments ("never above the workspace root"). Run
  `cargo test -p mkrew-ui run_setup` — `the_folder_picker_lists_folders_and_takes_the_shown_one`
  must still pass unchanged.

- [ ] **Step 4: UI text** in `crates/mkrew-ui/src/shell/run.rs`:
  - placeholder `"e.g. .env"` → `"e.g. EA-BE/.env"`
  - tooltip `"Choose a file in the command's folder"` → `"Choose a file in the workspace"`
  - label `"Env file (optional, relative to the folder)"` → `"Env file (optional, relative to the workspace)"`

- [ ] **Step 5: Run:** `cargo test -p mkrew-ui` — PASS; if an insta snapshot shows the label,
  `cargo insta review` and accept only the label change.

- [ ] **Step 6: HUMAN_CHECKS** — add: "Run setup (EA): Env file Browse… starts in the card's
  folder and ↑ Up reaches the workspace root; a card with folder
  `.worktrees/fix-alembic-heads/EA-BE` and env file `EA-BE/.env` starts there with the main
  `.env` (dark and light)."

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/run_setup.rs crates/mkrew-ui/src/shell/run.rs docs/HUMAN_CHECKS.md
git commit -m "feat(ui): env file picker browses the whole workspace"
```

### Task 3: Duplicate a run command

**Files:**
- Modify: `crates/mkrew-ui/src/run_setup.rs:6` (imports), new `duplicate_card` after
  `remove_card` (`:280-289`), tests
- Modify: `crates/mkrew-ui/src/shell/run.rs:597-606` (button row of each card)
- Modify: `docs/SPEC.md` §10 Run setup dialog
- Modify: `docs/HUMAN_CHECKS.md` (one line)

**Interfaces:**
- Consumes: `mkrew_core::run::{RunError, MAX_COMMANDS, MAX_NAME_CHARS}` (already `pub`).
- Produces: `RunSetup::duplicate_card(&mut self, index: usize)`.

- [ ] **Step 1: Write the failing tests** in `run_setup.rs` tests:

```rust
    #[test]
    fn duplicating_a_card_copies_it_below_under_a_free_name() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.duplicate_card(0);
        assert_eq!(setup.cards.len(), 2);
        assert_eq!(setup.cards[1].name, "BE copy");
        assert_eq!(setup.cards[1].dir, setup.cards[0].dir);
        assert_eq!(setup.cards[1].command, setup.cards[0].command);
        assert_eq!(setup.cards[1].env_file, setup.cards[0].env_file);
        assert_eq!(setup.cards[1].env.text(), setup.cards[0].env.text());
        assert_eq!(setup.focus, Focus::Field(1, Field::Name));

        setup.duplicate_card(0);
        let names: Vec<&str> = setup.cards.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["BE", "BE copy 2", "BE copy"]);
        assert!(setup.begin().is_some(), "the copies save");
    }

    #[test]
    fn a_long_name_is_cut_to_fit_and_a_full_list_refuses() {
        let mut long = be();
        long.name = "abcdefghijklmnopqrst".into(); // 20 chars, the most
        let mut setup = RunSetup::new(&project(vec![long]), false);
        setup.duplicate_card(0);
        assert_eq!(setup.cards[1].name, "abcdefghijklmno copy");

        while setup.cards.len() < MAX_COMMANDS {
            setup.duplicate_card(0);
        }
        assert!(setup.begin().is_some(), "every generated name is valid and unique");
        setup.busy = false;
        setup.duplicate_card(0);
        assert_eq!(setup.cards.len(), MAX_COMMANDS);
        assert_eq!(
            setup.error.as_deref(),
            Some(format!("at most {MAX_COMMANDS} run commands").as_str())
        );
    }
```

- [ ] **Step 2: Run, expect FAIL** (no `duplicate_card`): `cargo test -p mkrew-ui run_setup::tests`

- [ ] **Step 3: Implement.** Imports at the top of `run_setup.rs`:

```rust
use mkrew_core::run::{MAX_COMMANDS, MAX_NAME_CHARS, RunError, normalize, parse_env};
```

  In `impl RunSetup`, after `remove_card`:

```rust
    /// Inserts a copy of card `index` right below it, named `<name> copy` (then `copy 2`, …,
    /// the name cut so it fits) and focuses its name. Refused at [`MAX_COMMANDS`].
    pub fn duplicate_card(&mut self, index: usize) {
        let Some(original) = self.cards.get(index) else {
            return;
        };
        if self.cards.len() >= MAX_COMMANDS {
            self.error = Some(RunError::TooMany.to_string());
            return;
        }
        let mut copy = original.clone();
        copy.name = self.free_copy_name(original.name.as_str().trim()).into();
        self.cards.insert(index + 1, copy);
        self.focus = Focus::Field(index + 1, Field::Name);
    }

    /// `<base> copy`, `<base> copy 2`, …: the first name no card has, `base` cut so the whole
    /// name fits in [`MAX_NAME_CHARS`].
    fn free_copy_name(&self, base: &str) -> String {
        (1..)
            .map(|n| {
                let suffix = if n == 1 { " copy".to_owned() } else { format!(" copy {n}") };
                let room = MAX_NAME_CHARS.saturating_sub(suffix.chars().count());
                let base: String = base.chars().take(room).collect();
                format!("{}{suffix}", base.trim_end())
            })
            .find(|name| self.cards.iter().all(|c| c.name.as_str().trim() != name.as_str()))
            // An endless range: `find` always returns; ≤ MAX_COMMANDS cards take ≤ 8 names.
            .unwrap_or_default()
    }
```

- [ ] **Step 4: Run, expect PASS:** `cargo test -p mkrew-ui run_setup`

- [ ] **Step 5: Button.** In `crates/mkrew-ui/src/shell/run.rs` replace the card's last child
  (`.child(div().flex().justify_end().child(quiet_button("run-remove", …)))`) with:

```rust
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(space::XS))
                            .child(
                                quiet_button("run-duplicate", "Duplicate", theme, theme.surface_raised)
                                    .on_click(cx.listener(move |shell, _, _, cx| {
                                        if let Some(setup) = shell.run_setup.as_mut() {
                                            setup.duplicate_card(i);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                quiet_button("run-remove", "Remove", theme, theme.surface_raised)
                                    .on_click(cx.listener(move |shell, _, _, cx| {
                                        if let Some(setup) = shell.run_setup.as_mut() {
                                            setup.remove_card(i);
                                        }
                                        cx.notify();
                                    })),
                            ),
                    ),
```

  (the card's `.id(("run-card", i))` already scopes button ids.)

- [ ] **Step 6: Docs.** SPEC §10: "command cards (…), `+ Add command`, `Duplicate` (a copy
  right below, named `<name> copy`, refused at 8), `Remove`, …". HUMAN_CHECKS: "Run setup:
  Duplicate on BE adds `BE copy` below with the same fields and focuses its name; at 8 commands it
  shows the limit (dark and light)."

- [ ] **Step 7: Full verify:** `sh scripts/verify.sh` — fmt, clippy `-D warnings`, all tests PASS.

- [ ] **Step 8: Smoke run** (build the app, workspace EA → Run setup): Duplicate `BE`; rename the
  copy `BE fix`, folder `.worktrees/fix-alembic-heads/EA-BE`, command
  `C:\Users\NguyenTung\Workspace\SourceCode\EA\EA-BE\.venv\Scripts\python.exe main.py`, Env file
  `EA-BE/.env` via Browse…; Save; Run `BE fix` from the Run menu: the pane starts in the worktree
  folder with the main `.env` loaded. Also check `BE` now shows `./EA-BE/.env` (migrated) and still
  starts. (`BE` and `BE fix` share the app port — override it in the copy's Env lines to run both.)

- [ ] **Step 9: Commit**

```bash
git add crates/mkrew-ui/src/run_setup.rs crates/mkrew-ui/src/shell/run.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): duplicate a run command in Run setup"
```

### Task 4: Pane header "Follow in Zed" → `Share` menu

An agent pane header shows a "Follow in Zed" / "Following in Zed" toggle
(`crates/mkrew-ui/src/shell/pane_render.rs:429-458`). Replace it with one `Share` button; a click
opens a menu at the click with "Follow in Zed" (✓ while on). More share targets get added to this
menu later — only this one item now, no registry or trait.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs:360-361` (new field `share_menu`) and `:716-721` (render it
  next to `pane_menu`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs:274` (`share_menu: None` in the initializer)
- Modify: `crates/mkrew-ui/src/shell/focus.rs:25-36` (`close_menus` clears it)
- Modify: `crates/mkrew-ui/src/shell/pane_render.rs:416-458` (button), `:715-717` (doc comment)
- Modify: `crates/mkrew-ui/src/shell/menus.rs` (new `render_share_menu` after `render_pane_menu`)
- Modify: `crates/mkrew-ui/src/palette.rs:119-120` (comment wording only)
- Modify: `docs/SPEC.md` §10 (~line 905) and §20C.5 (~line 1743); `docs/HUMAN_CHECKS.md` lines that
  say the header shows "Follow in Zed" (~86, ~362, ~1405) plus one new check

**Interfaces:**
- Consumes: `context_menu::PaneMenu { session_id, position }` (reused as is: same shape),
  `Shell::toggle_follow(session_id, cx)`, `self.follow.is_on(session)`, `menu_panel`, `menu_item`.
- Produces: `Shell.share_menu: Option<PaneMenu>`, `Shell::render_share_menu(&self, &PaneMenu,
  &Theme, &mut Context<Self>) -> gpui::AnyElement`.

No unit test: this is rendering and wiring (follow logic stays in `follow.rs`, already tested).
Proof is the smoke run in Step 6.

- [ ] **Step 1: State.** `main.rs`, after `pane_menu`:

```rust
    /// The open Share menu of an agent pane (Follow in Zed), at the click.
    share_menu: Option<PaneMenu>,
```

  `connection.rs` initializer: `share_menu: None,` after `pane_menu: None,`. `focus.rs`
  `close_menus`: `self.share_menu = None;` after `self.pane_menu = None;`.

- [ ] **Step 2: Button.** In `pane_render.rs` replace the `button_frame("pane-follow", …)` child
  (and the `follow_tip` binding above it) with a `Share` button that shows the follow state:

```rust
                        .child(
                            button_frame("pane-share", "Share")
                                .px(px(space::XS))
                                .py_0()
                                .text_size(font_size::META)
                                .active(|style| style.opacity(opacity::ACTIVE))
                                .when(following, |el| {
                                    el.bg(rgb(theme.surface_raised))
                                        .font_weight(FontWeight(weight::MEDIUM))
                                        .text_color(rgb(theme.text))
                                })
                                .when(!following, |el| {
                                    let hover = theme.surface_raised;
                                    el.text_color(rgb(theme.text_muted))
                                        .hover(move |style| style.bg(rgb(hover)))
                                })
                                .tooltip(tooltip(
                                    theme,
                                    if following {
                                        "Share: following this agent's edits in Zed"
                                    } else {
                                        "Share this agent's work"
                                    },
                                    "",
                                ))
                                .on_click(cx.listener(move |shell, event: &gpui::ClickEvent, _, cx| {
                                    shell.close_menus();
                                    shell.share_menu = Some(PaneMenu {
                                        session_id: session,
                                        position: event.position(),
                                    });
                                    cx.notify();
                                })),
                        )
```

  (`event.position()` is how the split button and tab `⋯` open their menus,
  `pane_render.rs:183-193`.) Keep the highlighted look while following so the state stays
  visible without opening the menu.

- [ ] **Step 3: Menu.** In `menus.rs` after `render_pane_menu`:

```rust
    /// The Share menu of an agent pane: "Follow in Zed" (✓ while on). Floats at the click.
    pub(crate) fn render_share_menu(
        &self,
        menu: &PaneMenu,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let session_id = menu.session_id;
        let following = self.follow.is_on(session_id);
        let panel = menu_panel("share-menu", theme)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.share_menu = None;
                cx.notify();
            }))
            .child(
                menu_item("share-follow-zed".into(), theme)
                    .child(
                        div()
                            .flex_none()
                            .w(px(space::M))
                            .text_color(rgb(theme.accent_text))
                            .child(if following { "✓" } else { "" }),
                    )
                    .child(div().flex_1().text_color(rgb(theme.text)).child("Follow in Zed"))
                    .on_click(cx.listener(move |shell, _, _, cx| {
                        shell.share_menu = None;
                        shell.toggle_follow(session_id, cx);
                    })),
            );
        deferred(
            anchored()
                .position(menu.position)
                .snap_to_window_with_margin(px(space::S))
                .child(panel),
        )
        .into_any_element()
    }
```

  `toggle_follow` must call `cx.notify()` (check `shell/history.rs:8`; add `cx.notify()` in the
  listener if it does not).

- [ ] **Step 4: Render.** `main.rs`, right after the `pane_menu` `.children(…)` block:

```rust
                    .children(
                        self.share_menu
                            .as_ref()
                            .filter(|_| self.view() == View::Terminals && !self.modal_open())
                            .map(|menu| self.render_share_menu(menu, &theme, cx)),
                    )
```

- [ ] **Step 5: Docs.**
  - `pane_render.rs:715-717` doc: "…and on the right an agent's `Share` menu (Follow in Zed) and
    "Turns…"".
  - `palette.rs:119-120` comment: "the focused agent pane's "Turns…" and Share → "Follow in Zed"".
    The palette command `Agent: Follow in Zed (on/off)` stays.
  - SPEC §10 (~905): "and an agent's `Share` menu (for now one item, "Follow in Zed", ✓ while on;
    the button is highlighted while following) and "Turns…"". SPEC §20C.5 (~1743): "Pane header
    `Share` menu → "Follow in Zed" (also a palette command)".
  - HUMAN_CHECKS: reword the three existing mentions to `Share` and add: "Agent pane header:
    `Share` opens a menu at the click with "Follow in Zed"; clicking it turns following on (✓
    next time, button highlighted), again turns it off; a click outside closes the menu (dark and
    light)."

- [ ] **Step 6: Verify and smoke.** `sh scripts/verify.sh` — PASS. Build and open the app, in an
  agent pane: header shows `Share` (no "Follow in Zed" text); click → menu at the click with
  "Follow in Zed"; click it → menu closes, button highlighted; reopen → ✓; click → off. Esc /
  click outside closes the menu. Check dark and light theme.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/pane_render.rs crates/mkrew-ui/src/shell/menus.rs crates/mkrew-ui/src/palette.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): pane Share menu holds Follow in Zed"
```

### Task 5: Files view — tree on the right, cleaner search row and Ignored toggle

Today (`crates/mkrew-ui/src/shell/files_render.rs:33-171`): the tree column (280 px,
`theme::FILE_TREE_WIDTH`) sits on the **left**, right next to the app sidebar, so two lists stand
side by side. Its header is a plain text field "Search files…" (click opens Quick open) plus a
ghost text button whose label flips between "Show ignored" and "Hide ignored" (the width jumps,
and it reads as an action, not a state).

Target:

```
┌ app sidebar ┐┌──────────── file panel ─────────────┐┌──── tree (280) ────┐
│             ││ src/main.rs        Ln 3, Col 1  Zed  ││ ⌕ Search files…  Ctrl+P │ Ignored │
│             ││ 1  fn main() {                       ││ ▸ crates                │
│             ││ …                                    ││   main.rs            M  │
```

- Tree column moves to the **right** of the file panel (same width, same keys, same right-click
  menu, same target chooser and "New file…" row).
- Search row: one field-looking button — `⌕` glyph, muted "Search files…", the Quick open
  shortcut right-aligned inside it (`self.keymap.label(Command::QuickOpen)`), subtle surface +
  1 px border, hover lifts to `surface_raised`. Click opens Quick open, as today.
- Ignored: a compact toggle chip with a fixed label `Ignored`: off = muted text; on = pressed
  look (`surface_raised` bg, `text` color, medium weight) — the same on/off look the pane's Share
  button uses. Tooltip says what a click does + the `ToggleIgnored` shortcut.
- The empty-tree state keeps its primary "Show ignored" button (design rule 6).

**Files:**
- Modify: `crates/mkrew-ui/src/shell/files_render.rs:27-71` (header), `:132-171` (column order),
  `:234-244` (`render_file` panel margins)
- Modify: `docs/SPEC.md:972` ("**Tree** (left)" → "(right)") and the §12A sentence about the
  ignored toggle
- Create: `docs/decisions/0050-files-tree-on-the-right.md`
- Modify: `docs/HUMAN_CHECKS.md` (one check)

**Interfaces:** none new; reuses `button_frame`, `tooltip`, `Shell::open_quick_open`,
`FilesView::toggle_ignored`, `Shell::files_requests`. No new theme tokens (the skill's hard rule 1:
only existing `theme.rs` tokens).

No unit test: layout and styling only (toggle logic is covered by
`files_view/tests.rs`). Proof is the smoke run in Step 5.

- [ ] **Step 1: Header.** Replace `ignored_label`/`ignored_tip` and the `header` (lines 27-71) with:

```rust
        let ignored_keys = self.keymap.label(Command::ToggleIgnored);
        let showing_ignored = view.show_ignored;
        let ignored_tip = if showing_ignored {
            "Showing git-ignored files: click to hide them"
        } else {
            "Show git-ignored files"
        };
        let search_hover = theme.surface_raised;
        let search = div()
            .id("files-search")
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .py(px(space::XS))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface))
            .cursor_pointer()
            .text_size(font_size::BODY)
            .text_color(rgb(theme.text_muted))
            .hover(move |style| style.bg(rgb(search_hover)))
            .active(|style| style.opacity(opacity::ACTIVE))
            .child(div().flex_none().child("⌕"))
            .child(div().flex_1().min_w_0().truncate().child("Search files…"))
            .child(
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .child(self.keymap.label(Command::QuickOpen)),
            )
            .tooltip(tooltip(theme, "Find a file of this workspace", self.keymap.label(Command::QuickOpen)))
            .on_click(cx.listener(|shell, _, window, cx| shell.open_quick_open(window, cx)));
        let ignored = button_frame("files-ignored", "Ignored")
            .flex_none()
            .text_size(font_size::META)
            .active(|style| style.opacity(opacity::ACTIVE))
            .when(showing_ignored, |el| {
                el.bg(rgb(theme.surface_raised))
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text))
            })
            .when(!showing_ignored, |el| {
                let hover = theme.surface_raised;
                el.text_color(rgb(theme.text_muted))
                    .hover(move |style| style.bg(rgb(hover)))
            })
            .tooltip(tooltip(theme, ignored_tip, ignored_keys.clone()))
            .on_click(cx.listener(move |shell, _, window, cx| {
                let requests = shell.files_view(project_id).toggle_ignored();
                shell.files_requests(project_id, requests, window, cx);
            }));
        let header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(space::XS))
            .px(px(space::S))
            .pb(px(space::S))
            .child(search)
            .child(ignored);
```

  Check `theme.surface` / `surface_raised` / `border` / `text_muted` exist under these names in
  `theme.rs` (they are used in this file and `widgets.rs`); `ignored_keys` is still used by the
  empty-state button below. `text_field` stays in `widgets.rs` (other views use it); clippy flags
  any import here that becomes unused.

- [ ] **Step 2: Column order.** In the outer `div()` (lines 132-171) put
  `.child(self.render_file(project_id, view, window, theme, cx))` **before** the
  `files-tree` child. In `render_file`'s `panel` (line 240) change `.mr(px(space::S))` to
  `.ml(px(space::S))` so the panel keeps a gap on the side of the app sidebar and touches the tree
  side like it touched the window edge before. Check the tree column's first row is not flush with
  the window edge; if it is, add `.pr(px(space::XS))` to `files-tree` (rows already have `px(S)`).

- [ ] **Step 3: Docs.**
  - SPEC §12A line 972: "**Tree** (right of the file; ADR 0050): lazily loaded per folder,
    respects `.gitignore` (an `Ignored` toggle beside the search shows ignored files), …".
  - ADR `docs/decisions/0050-files-tree-on-the-right.md`:

```markdown
# 0050 — The Files tree sits on the right; search and Ignored are redrawn

- Date: 2026-10-12
- Status: accepted

## Context

The Files tree stood left of the file, beside the app sidebar: two lists side by side and the
file pushed to the far right. Its "Show ignored" / "Hide ignored" text button changed width and
read as an action rather than a state. The human asked for the tree on the right and a cleaner
search and ignored control.

## Decision

- Tree column right of the file panel; same width, keys, menus.
- The search row is one field-looking button (`⌕`, placeholder, Quick open shortcut) that opens
  Quick open, as before.
- Ignored is a fixed-label toggle chip with an on (pressed) and off look; the tooltip says what a
  click does.

## Alternatives rejected

- A setting for the tree side: a choice nobody asked for.
- An icon-only ignored toggle: no glyph says "git-ignored" without reading the tooltip.

## Consequences

Layout only; no protocol, store or key change.
```

  - HUMAN_CHECKS: "Files view: tree on the right of the file; the search row shows `⌕ Search
    files…` with the Quick open shortcut and opens Quick open on click; `Ignored` toggles ignored
    files, looks pressed while on, its width never changes; ←/→/↑/↓, right-click, New file… still
    work (dark and light)."

- [ ] **Step 4: Verify:** `sh scripts/verify.sh` — PASS; grep the diff for hex colors or raw pixel
  numbers outside `theme.rs` (skill `mkrew-ui-design`), fix any hit.

- [ ] **Step 5: Smoke.** Build and open the app → a workspace → Files: tree on the right, file
  on the left; click the search row → Quick open; click `Ignored` twice → ignored files appear
  then hide, chip pressed only while on; keyboard in the tree still moves; both themes.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/shell/files_render.rs docs/SPEC.md docs/decisions/0050-files-tree-on-the-right.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): Files tree on the right with a cleaner search and Ignored toggle"
```

## Not in this plan

- Command paths relative to the workspace (`MKREW_WORKSPACE` variable): an absolute path already
  works; add when asked.
- Running Run commands inside mkrew's own task worktrees (`worktree_task_id` on
  `project.run_start`): separate feature.
