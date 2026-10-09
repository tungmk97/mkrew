# Inbox 2026-10-09: Shift+Enter, F11, Files Editor, Several Git Repos, Scrollback, Pane Keys, Default Shell, Rename, Field Keys, Run Stop, Exited Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Handle the twelve new items in `docs/INBOX.md` (lines 27–38):
- Shift+Enter makes a new line in agents.
- F11 toggles full screen.
- The Files viewer becomes an editor, with VS Code-style find and replace.
- The Git view works when a workspace folder holds several repos (`fe/`, `be/`).
- Scrollback survives switching tabs.
- `+ Agent` / `+ Shell` get shortcuts.
- Shells get profiles and a per-workspace default, like agents.
- Panes can be renamed (F2) and name themselves from the terminal title.
- Every text field supports Ctrl+A, Ctrl+Backspace and Ctrl+V.
- Stopping a Run command removes its terminal, and restarting it reuses the same place.
- Exited shells and agents leave the sidebar and their tabs; History keeps them.

**Architecture:**
- Item 27 (bug): `terminal/input.rs::special_key` sends `\r` for Enter with any modifier, so Shift+Enter submits. Mkrew has no kitty or win32-input-mode support, so the only sequence that reaches every agent through ConPTY is `ESC CR` (the Alt+Enter form). Task 1 sends that for Shift+Enter.
- Item 31: GPUI `Window::toggle_fullscreen` is implemented on Windows. Task 2 adds a global `ToggleFullScreen` command on F11.
- Items 29 and 28: the user chose a full in-app editor (Open question 1). This reverses SPEC §1 "Not a code editor", so it needs ADR 0028.
  - Task 3: a daemon write path, `fs.write` with a hash precondition (protocol v27).
  - Task 4: a pure `editor.rs` buffer.
  - Task 5: the viewer edits and saves (Ctrl+S).
  - Task 6: disk changes and unsaved work (never lose data).
  - Task 7: find and replace on Ctrl+F / Ctrl+H.
- Item 30: everything git-related runs on one folder, found by `rev-parse` searching upward. Task 8 adds a `repo` param, `git.repos` discovery (depth 1), nested-repo markers and the watcher fix (ADR 0029). Task 9 adds the repo chooser in the Git header.
- Item 33 (bug): hiding a pane (tab switch, Board, minimize, zoom) drops its UI emulator. Re-attach rebuilds it from a screen-only snapshot, so history above one page is gone. Task 10 makes the snapshot carry the daemon's 200 history lines (ADR 0030).
- Items 34 and 35:
  - Task 11 adds `SplitAgent` (Alt+Shift+A) and `SplitShell` (Alt+Shift+S), the exact actions of the toolbar buttons.
  - Task 12 ships shell profiles (pwsh, powershell, cmd, bash) and a per-workspace `default_shell`. With no default set, the user picks one, as with agents (ADR 0032).
- Item 32:
  - Task 13: F2 renames the focused session through `session.rename`.
  - Task 14: panes name themselves from the OSC terminal title (Claude sets a conversation summary), kept in daemon memory. A user-set name always wins (ADR 0031).
- Item 36: every field is append-only, and each one goes through one of two key mappers. Ctrl+Backspace deletes one character, and Ctrl+A works only in the multi-line fields. Task 15 adds word delete, select-all and paste to the shared mappers and to one single-line field type.
- Item 37: `run_stop` does not wait for the command to stop, and it never updates the run tab. Task 16 adds a hook in `Actor::finish`: when a run session exits while the daemon runs, its pane leaves the run tab. Stop and Restart need no other change, and Restart already puts the new session in the old slot.
- Item 38: exited rows are not clickable and stay forever. Task 17 shows only live sessions in the sidebar and removes a pane from its tab when its session exits. Sessions are not hidden, so History keeps them, and resume after a daemon restart is unaffected (ADR 0033).

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), alacritty_terminal 0.26, serde, insta. No new dependencies.

**Spec:** `docs/SPEC.md` §1, §5, §6, §7, §10, §11, §12A, §12B, §13, §20C/§20E (notes), cited per task. Open questions are at the end, each with the default this plan uses.

**Inbox mapping:**

| Inbox line | Task |
| --- | --- |
| 27 `Terminal and inside agent: add Shift + Enter for wrting in the new line` | 1 |
| 31 `Support F11 to view full screen` | 2 |
| 29 `Files Module: I expected I can edit information there not just read` | 3, 4, 5, 6 |
| 28 `Files module: … find and replace content … find, replate replace all action` | 7 |
| 30 `Git module: … Root folter container FE and BE … 2 diffreence git repo …` | 8, 9 |
| 33 `in terminal agent, sometime I cannot scroll on top over 1 page …` | 10 |
| 34 `Add short cut for '+ Agent' …` | 11 |
| 35 `Add short cut for '+ Shell' and user must to select default shell …` | 11, 12 |
| 32 `Add a short cut to rename pain, good if we can also auto rename …` | 13, 14 |
| 36 `Check all input field in this app should suppport Ctrl A, Ctrl Backspace…` | 15 |
| 37 `Running command, when a server user click stop: should remove this terminal…` | 16 |
| 38 `when a shell or agent is stopped, it still showing in the sidebar…` | 17 |

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "An agent is just a terminal command. No per-agent code in the core." (SPEC §2.1). Every agent difference lives in its TOML profile. There is no `match "claude"` anywhere in Rust code; terminal key bytes stay generic.
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- `thiserror` in libraries, `anyhow` with context in binaries, `tracing` for logs.
- No test may call a real agent or the network.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change. ADR numbers in this plan: 0028 (Task 3), 0029 (Task 8), 0030 (Task 10), 0031 (Tasks 13–14), 0032 (Task 12), 0033 (Task 17).
- **Protocol version:** Tasks 3, 8, 10, 12, 13 and 14 change the wire format. Each one bumps `PROTOCOL_VERSION` (`crates/mkrew-protocol/src/lib.rs:16`, 26 today) by one from its value when the task lands. Numbers quoted inside tasks (27, 28, …) assume plan order.
- Few dependencies: add no new crates.
- GPUI rule: set `.hover()` at most once per element (`button()`/`quiet_button()`/`row_action()`/`menu_item()` already set it). `group_hover` is a separate slot and is safe.
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use; do not run `git commit`.
- Leave uncommitted human edits alone. Today only `docs/INBOX.md` is modified, and the plan's own tick marks are the only change allowed there. Never touch `.github/workflows/*`, `packaging/`, `docs/BUILD_VI.md`, `design/`, `icon/`, or the x11/wayland lines in `crates/mkrew-ui/Cargo.toml`.
- Never write files outside this repo. Edits to the user's own `<config>/mkrew/*` files are **user steps**, listed in HUMAN_CHECKS.
- **Inbox:** when a task passes verification, change its `docs/INBOX.md` line from `[~]` to `[x]` (see the mapping above). Lines with several tasks are ticked after their last task.
- Verify on Windows with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run these three in order:
  1. `cargo fmt --all -- --check`
  2. `cargo clippy --workspace --all-targets -- -D warnings`
  3. `cargo test --workspace -j 4`
- The C: drive is nearly full: no `cargo clean`, no extra target dirs, no downloads.
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions or the daemon.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.
- `crates/mkrew-ui/src/main.rs` is ~16.9k lines: read ranges, never the whole file. Line numbers below are from 2026-10-09 and drift as tasks land. Re-find each function by name (`grep -n "fn render_file"`).

## Review Focus

1. **Unsaved edits when an agent changes the same file.** Expected: the buffer is kept, and a "Changed on disk" banner offers Reload / Keep mine. A save never overwrites a file it has not seen. Pinned by Task 6 `a_change_on_disk_under_unsaved_text_offers_reload_or_keep_mine`, Task 5 `a_reread_follows_the_disk_only_while_nothing_is_unsaved` and Task 3 `write_saves_only_over_the_version_that_was_read`.
2. **Saving a CRLF / tab / no-final-newline file.** Expected: only the edited bytes change; line endings, tabs and a missing final newline are kept. Pinned by Task 4 `text_round_trips_line_endings_tabs_and_empty_files` and Task 3 `write_saves_only_over_the_version_that_was_read`, which reads the file back byte-exact.
3. **Shift+Enter in a plain shell (pwsh, bash, cmd).** Expected: the half-typed command does not run. pwsh and bash receive Alt+Enter, which does nothing. cmd treats it like Enter, the same as Windows Terminal does. Pinned by Task 1 `shift_enter_is_a_newline_for_agents` (bytes) and the HUMAN_CHECKS item.
4. **Git paths in a nested repo; scrollback after re-attach.**
   - Every path in git replies and requests stays project-relative (`fe/src/a.ts`), so Open in Zed, comments and Changed-by keep working. `repo: "../x"` and `repo: "fe/.git"` are refused. Pinned by Task 8 `git_requests_reach_child_repositories_with_project_relative_paths`.
   - After a tab switch, the wheel reaches the daemon's last 200 lines. Pinned by Task 10 `snapshot_history_is_scrollable`.
5. **Replace all where the replacement contains the query** (`a` → `aa`). Expected: one pass, no endless loop, and one undo step restores the text. Pinned by Task 4 `replace_all_is_one_undo_step` and Task 7 `replace_one_moves_on_and_replace_all_is_one_undo_step`.

---

### Task 1: Shift+Enter makes a new line in agents (`ESC CR`)

Inbox: line 27 `Terminal and inside agent: add Shift + Enter for wrting in the new line`. Spec: §13 (:865-872).

**Current state (investigated):**
- `crates/mkrew-ui/src/terminal/input.rs:137` `special_key`: `"enter" => b"\r".to_vec()` ignores Shift and Ctrl. Today Shift+Enter is `\r`, the same as Enter, so every agent submits. Alt+Enter is `\x1b\r` via `with_alt` (test `alt_prefixes_escape` :383).
- Shift+Enter is not bound in the keymap, so it reaches `TerminalView::key_down` (view.rs:214). Ctrl+Shift+Enter is the global OpenComposer and never reaches the PTY.
- Neither emulator enables kitty keyboard mode (`Config::default()`, `kitty_keyboard = false`), and ConPTY's win32-input-mode is ignored. So `CSI 13;2u` is not an option: native Codex prints it literally.
- What each agent does with `ESC CR`:
  - Claude Code: Alt+Enter = newline.
  - Codex: `insert_newline` includes `alt(Enter)`.
  - Copilot CLI: documented Alt+Enter.
  - omp / pi-tui: `editor.ts` treats `"\x1b\r"` as a newline.
- Shells: pwsh / PSReadLine leaves Alt+Enter unbound, and so does bash readline (no-op instead of running the line). cmd treats it like Enter, as Windows Terminal does.

**Files:**
- Modify: `crates/mkrew-ui/src/terminal/input.rs:18-41` (`key_to_bytes`), `:136-137` (`special_key`), tests after `alt_prefixes_escape` (:383-386)
- Modify: `docs/SPEC.md` §13 (:870-871)
- Modify: `docs/HUMAN_CHECKS.md` (new section at the top of "To verify")

**Interfaces:**
- Produces: nothing new; `key_to_bytes` returns `Some(b"\x1b\r")` for `shift-enter` and `alt-shift-enter`.

- [ ] **Step 1: Failing test**

In `terminal/input.rs` `mod tests`, after `alt_prefixes_escape`:

```rust
    /// Shift+Enter is the Alt+Enter form (ESC CR): Claude, Codex, Copilot and omp insert a
    /// new line instead of submitting; plain and Ctrl+Enter still submit.
    #[test]
    fn shift_enter_is_a_newline_for_agents() {
        for mode in [NORMAL, APP] {
            assert_eq!(bytes("shift-enter", None, mode), Some(b"\x1b\r".to_vec()));
            // One ESC, not two (ESC ESC CR would read as Esc, then Alt+Enter).
            assert_eq!(bytes("alt-shift-enter", None, mode), Some(b"\x1b\r".to_vec()));
            assert_eq!(bytes("enter", None, mode), Some(b"\r".to_vec()));
            assert_eq!(bytes("ctrl-enter", None, mode), Some(b"\r".to_vec()));
        }
    }
```

Run: `cargo test -p mkrew-ui --lib terminal::input::tests::shift_enter -j 4`
Expected: FAIL (`left: Some([13])`, `right: Some([27, 13])`).

- [ ] **Step 2: Implement**

`special_key`, replace the `"enter"` arm:

```rust
        // Shift+Enter: the Alt+Enter form, which agents read as "new line" (Claude, Codex,
        // Copilot, omp). Without kitty / win32-input mode nothing else reaches them through
        // ConPTY intact; shells see Alt+Enter, which pwsh and readline leave unbound.
        "enter" if shift => vec![0x1b, b'\r'],
        "enter" => b"\r".to_vec(),
```

`key_to_bytes`, replace the special-key branch:

```rust
    if let Some(bytes) = special_key(&keystroke.key, m.control, m.shift, modifier, mode) {
        // Shift+Enter already starts with ESC: Alt adds nothing.
        let shift_enter = m.shift && keystroke.key == "enter";
        return Some(with_alt(bytes, m.alt && !is_csi(&keystroke.key) && !shift_enter));
    }
```

Run: `cargo test -p mkrew-ui --lib terminal::input -j 4`
Expected: PASS (`editing_keys` and `alt_prefixes_escape` still pass).

- [ ] **Step 3: SPEC §13**

After the Ctrl+V sentence (SPEC.md:870-871) add:

```markdown
Shift+Enter sends `ESC CR` (the Alt+Enter form), which agents read as a new line; Mkrew has no
kitty / win32-input keyboard mode, so this is the one form every agent gets through ConPTY.
```

- [ ] **Step 4: HUMAN_CHECKS**

Add at the top of "To verify (non-blocking)":

```markdown
### To verify — 2026-10-09 inbox: Shift+Enter, F11, Files editor, several repos, scrollback, pane keys, shells, rename (plan docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md)

- [ ] **Shift+Enter:** in claude, codex, copilot and omp panes, type `a`, Shift+Enter, `b` → two
  lines in the prompt, nothing sent; Enter sends. In pwsh and Git Bash, Shift+Enter does not run the
  half-typed command.
```

- [ ] **Step 5: Verify and tick**

Run the full verification. Tick inbox line 27 `[x]`.
Suggested message: `fix(ui): Shift+Enter sends ESC CR so agents insert a new line`

---

### Task 2: F11 toggles full screen

Inbox: line 31 `Support F11 to view full screen`. Spec: §13 (:874-879).

**Current state (investigated):**
- GPUI: `Window::toggle_fullscreen(&self)` (gpui-pre-0.3.7 window.rs:6428) and `is_fullscreen()` (:2731). On Windows (`gpui-pre-windows-0.3.7/src/window.rs:302-361, :955`) the toggle saves the style and rect, removes the caption and frame, covers the monitor, and restores on the second toggle. It runs on the executor, so `is_fullscreen()` lags by a frame. Never derive a label from it in the same frame.
- Mkrew uses the native titlebar (main.rs:137-148, no `appears_transparent`), so no custom-titlebar path applies. Window bounds are not persisted, and neither is fullscreen.
- F11 is unbound. A focused terminal sends it to the PTY as `ESC[23~` (input.rs:194). Once bound, TUIs no longer get F11 (Windows Terminal makes the same trade). Users can rebind it in keymap.toml.
- The wiring to copy is `ToggleTheme`: keymap.rs enum :102-103, `all()` :194, `name()` :256, `default_keys()` :323; palette.rs `title()` :220, `available()` :231-235; main.rs `run` :6317.

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs:102-103, :194, :256, :323`, test `spec_defaults` (:645)
- Modify: `crates/mkrew-ui/src/palette.rs:220, :231-235`, test `availability_follows_the_view` (:777-786)
- Modify: `crates/mkrew-ui/src/main.rs:6317` (`Shell::run`)
- Modify: `docs/SPEC.md` §13 (:879), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces: `keymap::Command::ToggleFullScreen` (name `toggle_full_screen`, default `f11`, context `None`).

- [ ] **Step 1: Failing tests**

`keymap.rs` `spec_defaults`, after the `ctrl-,` asserts:

```rust
        assert_eq!(bound("f11"), Some(Command::ToggleFullScreen));
        assert_eq!(keymap.label(Command::ToggleFullScreen), "F11");
        // Global: also while a terminal has focus (it no longer gets ESC[23~).
        assert_eq!(Command::ToggleFullScreen.context(), None);
```

`palette.rs` `availability_follows_the_view`, in the no-workspace list:

```rust
            [
                Command::AddWorkspace,
                Command::NextInbox,
                Command::Peek,
                Command::OpenKeymapSettings,
                Command::ToggleTheme,
                Command::ToggleFullScreen
            ]
```

Run: `cargo test -p mkrew-ui --lib keymap::tests::spec_defaults -j 4`
Expected: FAIL to compile (`no variant ToggleFullScreen`).

- [ ] **Step 2: Implement**

keymap.rs, enum after `ToggleTheme`:

```rust
    /// Toggle the window filling the whole screen (not remembered across restarts).
    ToggleFullScreen,
```

`all()`: `Command::ToggleFullScreen,` after `Command::ToggleTheme,`. `name()`: `Command::ToggleFullScreen => "toggle_full_screen".into(),`. `default_keys()`: `Command::ToggleFullScreen => "f11".into(),`. `context()` needs no change (`_ => None`).

palette.rs `title()`: `Command::ToggleFullScreen => "Global: Toggle full screen".into(),`. `available()`: add `| Command::ToggleFullScreen` to the always-true arm after `Command::ToggleTheme`.

main.rs `Shell::run`, after the `ToggleTheme` arm:

```rust
            Command::ToggleFullScreen => window.toggle_fullscreen(),
```

Run: `cargo test -p mkrew-ui --lib keymap -j 4` and `cargo test -p mkrew-ui --lib palette -j 4`
Expected: PASS. `every_default_parses_and_is_unique`, `names_round_trip` and `template_parses_to_the_defaults_commented_or_not` cover the new entry through `all()`.

- [ ] **Step 3: SPEC + HUMAN_CHECKS**

SPEC §13, after the Toggle theme row (:879):

```markdown
| Global | Toggle full screen | F11 |
```

HUMAN_CHECKS, in the 2026-10-09 section:

```markdown
- [ ] **F11:** F11 fills the screen (no title bar, taskbar hidden); F11 again restores the previous
  size and position; works with a terminal focused; the palette lists "Global: Toggle full screen".
```

- [ ] **Step 4: Verify and tick**

Run the full verification. Tick inbox line 31 `[x]`.
Suggested message: `feat(ui): F11 toggles full screen`

---

### Task 3: `fs.write` with a conflict check (protocol v27), ADR 0028, SPEC

Inbox: line 29 `Files Module: I expected I can edit information there not just read` (with Tasks 4–6). Spec: §1 (SPEC.md:57, :67), §7 (:527), §12A (:800), M7 (:1165), §2.5 (:98).

**Current state (investigated):**
- SPEC §1 :67 says "Not a code editor: no editor, no LSP. Files and diffs are read-only; editing happens in Zed." §1 :57 says "view them read-only". §12A :800 says "**Viewer** (right): read-only". M7 :1165 says "read-only viewer". The user chose a full in-app editor, so this needs an ADR (latest is 0027) plus SPEC edits in this task.
- Protocol: `FsReadParams` (`crates/mkrew-protocol/src/files.rs:378`) and `FsContent` (:387) have no hash. `Request::FsRead` is at `message.rs:208`, with imports at :20. `PROTOCOL_VERSION = 26` is at `lib.rs:16`. `files.rs` is re-exported with `pub use files::*`. Tests: `every_files_and_git_request_round_trips` (method list :577-593) and `results_and_event_wire_shapes` (an `FsContent` literal at :700).
- Daemon: `fs_read` (`crates/mkrew-daemon/src/server/fs.rs:55-77`) checks `docs::relative_path`, `in_git_dir` and `watched_target`, then calls `files::read` in `spawn_blocking`. Errors go through `read_error` (`server/doc.rs:93`: NotFound → `not_found`, TooLarge/NotUtf8 → `invalid_state`, Io → `internal`). Dispatch is in `server.rs:401`. `files::read` (`crates/mkrew-daemon/src/files.rs:226-256`) builds the only daemon `FsContent`. `docs::hash(&[u8]) -> String` is at `docs.rs:41`, and `docs::MAX_BYTES` (2 MiB) with `ReadError` are at `docs.rs:16-30`. The integration test `fs_list_read_and_find_follow_file_changes` is at `crates/mkrew-daemon/tests/daemon.rs:4859-4958`. Helpers there: `Client::call` :221, `add_project` :256, `error_code` :375.
- Other `FsContent` literals: `crates/mkrew-ui/src/highlight.rs:303` (test helper `read`) and :378. The UI never matches error codes; errors arrive as `anyhow` strings.
- The watcher reports a save as `files_changed` for the single working agent (`session.rs:662`), the same way it reports an edit made in Zed. We accept this and note it in the ADR. No code change.

**Files:**
- Modify: `crates/mkrew-protocol/src/files.rs:385-398` (`FsContent.hash`, `FsWriteParams`, `FsWritten`), tests :557-593, :700-709
- Modify: `crates/mkrew-protocol/src/message.rs:20` (imports), :207-209 (`Request::FsWrite`)
- Modify: `crates/mkrew-protocol/src/lib.rs:16` (`PROTOCOL_VERSION = 27`)
- Modify: `crates/mkrew-daemon/src/files.rs:1-16` (doc, imports), :224-256 (`read` sets `hash`, new `write`), tests after :535
- Modify: `crates/mkrew-daemon/src/server/fs.rs:8-12`, new `fs_write` after `fs_read` (:77)
- Modify: `crates/mkrew-daemon/src/server.rs:401` (dispatch)
- Modify: `crates/mkrew-daemon/tests/daemon.rs:19-20` (imports), after :4930
- Modify: `crates/mkrew-ui/src/highlight.rs:303-310`, :378-385 (`hash: None`)
- Create: `docs/decisions/0028-files-editor.md`
- Modify: `docs/SPEC.md` :57, :67, :527 (+ new row), :1165

**Interfaces:**
- Produces: `FsContent.hash: Option<String>` (`#[serde(default)]`). It is `Some` exactly when `content` is `Some`, and holds `docs::hash` of the bytes on disk.
- Produces: `FsWriteParams { project_id: Ulid, worktree_task_id: Option<Ulid>, path: String, content: String, expected_hash: String }` and `Request::FsWrite` (`"fs.write"`) → `FsWritten { saved: bool, hash: String }`.
- Produces: `mkrew_daemon::files::write(file: &Path, content: &str, expected_hash: &str) -> Result<FsWritten, ReadError>`.

- [ ] **Step 1: Failing protocol tests**

In `crates/mkrew-protocol/src/files.rs` tests, `every_files_and_git_request_round_trips`: after the `Request::FsRead(…)` entry (:557-561) add

```rust
            Request::FsWrite(FsWriteParams {
                project_id,
                worktree_task_id: wt,
                path: "src/a.rs".into(),
                content: "fn a() {}\n".into(),
                expected_hash: "0123456789abcdef".into(),
            }),
```

and add `"fs.write",` after `"fs.read",` in `methods` (:590). In `results_and_event_wire_shapes`, add `hash: None,` to the `FsContent` literal (:700-707). After its round trip (:709), add:

```rust
        // A v26 daemon's answer has no hash: the file shows, but it cannot be saved.
        let old: FsContent = serde_json::from_value(json!({
            "path": "a", "content": "x", "size": 1, "binary": false, "too_large": false,
            "language": null
        }))
        .unwrap();
        assert_eq!(old.hash, None);
        let written = FsWritten {
            saved: false,
            hash: "abc".into(),
        };
        let decoded: FsWritten = Outcome::ok(&written).unwrap().into_result().unwrap();
        assert_eq!(decoded, written);
```

Run: `cargo test -p mkrew-protocol --lib files::tests -j 4`
Expected: FAIL to compile (`FsWriteParams`, `FsWritten` and `hash` are unknown).

- [ ] **Step 2: Protocol types**

In `files.rs`, replace `FsContent` (:385-398) with:

```rust
/// `fs.read` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsContent {
    pub path: String,
    /// `None` when `binary` or `too_large`.
    pub content: Option<String>,
    /// Bytes.
    pub size: u64,
    pub binary: bool,
    /// Over the 2 MB viewer limit.
    pub too_large: bool,
    /// Syntax name guessed from the extension (e.g. `rust`); `None` = plain text.
    pub language: Option<String>,
    /// Opaque hash of the bytes read, `Some` exactly when `content` is: `fs.write` saves only
    /// over this version (ADR 0028).
    #[serde(default)]
    pub hash: Option<String>,
}

/// `fs.write` (ADR 0028): replaces the text of an existing file of the project (or task
/// worktree) if it still has the version that was read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsWriteParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
    pub path: String,
    /// The whole new text, line endings as they should be on disk.
    pub content: String,
    /// `FsContent::hash` of the version the edit started from.
    pub expected_hash: String,
}

/// `fs.write` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsWritten {
    /// `false`: the file on disk no longer has `expected_hash`, so nothing was written.
    pub saved: bool,
    /// The hash of the file on disk now: the new text's if saved, else the other version's
    /// (sending it as `expected_hash` overwrites that version: "Keep mine").
    pub hash: String,
}
```

In `message.rs`, add `FsWriteParams` to the `crate::files::{…}` import (:20). After `FsRead` (:209) add:

```rust
    /// Returns [`FsWritten`] (ADR 0028).
    #[serde(rename = "fs.write")]
    FsWrite(FsWriteParams),
```

In `lib.rs:16`: `pub const PROTOCOL_VERSION: u32 = 27;`

Run: `cargo test -p mkrew-protocol --lib files::tests -j 4`
Expected: PASS.

- [ ] **Step 3: Failing daemon unit test**

In `crates/mkrew-daemon/src/files.rs` tests, after `read_flags_binary_and_large_files_and_guesses_the_language` (:497-535). The module has its own `fn write(root, path, bytes)` helper (:369), so the code under test is called as `super::write`:

```rust
    #[test]
    fn write_saves_only_over_the_version_that_was_read() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.txt", b"one\r\n");
        let read_hash = read(&root.join("a.txt"), "a.txt".into()).unwrap().hash.unwrap();
        assert_eq!(read_hash, crate::docs::hash(b"one\r\n"));

        let saved = super::write(&root.join("a.txt"), "two\r\n", &read_hash).unwrap();
        assert_eq!((saved.saved, saved.hash.clone()), (true, crate::docs::hash(b"two\r\n")));
        assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"two\r\n");

        // Someone wrote meanwhile: nothing is written; the disk's hash comes back.
        std::fs::write(root.join("a.txt"), "agent\n").unwrap();
        let stale = super::write(&root.join("a.txt"), "mine\n", &saved.hash).unwrap();
        assert_eq!((stale.saved, stale.hash.clone()), (false, crate::docs::hash(b"agent\n")));
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "agent\n");
        // "Keep mine": the disk's hash overwrites that version.
        assert!(super::write(&root.join("a.txt"), "mine\n", &stale.hash).unwrap().saved);
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "mine\n");

        // Only existing files, at most MAX_BYTES of text.
        assert!(matches!(
            super::write(&root.join("new.txt"), "x", "h"),
            Err(ReadError::NotFound)
        ));
        assert!(!root.join("new.txt").exists());
        assert!(matches!(super::write(root, "x", "h"), Err(ReadError::NotFound)));
        let big = "a".repeat(MAX_BYTES as usize + 1);
        assert!(matches!(
            super::write(&root.join("a.txt"), &big, &crate::docs::hash(b"mine\n")),
            Err(ReadError::TooLarge(_))
        ));
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "mine\n");
    }
```

Run: `cargo test -p mkrew-daemon --lib files::tests::write_saves_only -j 2`
Expected: FAIL to compile (`files::write` and the `hash` field are missing).

- [ ] **Step 4: Daemon `read` hash and `write`**

`files.rs:16`: `use crate::docs::{self, MAX_BYTES, ReadError};`. Add `FsWritten` to the `mkrew_protocol` import (:11). Update the module doc (:1-4): "…`fs.read` (text, binary flag, language, hash), `fs.write` (ADR 0028) and `fs.find`…".

In `read` (:235-255), hash the bytes before they become the `String`:

```rust
    let (binary, content, hash) = if too_large {
        let mut head = Vec::with_capacity(SNIFF_BYTES);
        std::fs::File::open(file)?
            .take(SNIFF_BYTES as u64)
            .read_to_end(&mut head)?;
        (has_nul(&head), None, None)
    } else {
        let bytes = std::fs::read(file)?;
        let hash = docs::hash(&bytes);
        match String::from_utf8(bytes) {
            Ok(text) if !has_nul(text.as_bytes()) => (false, Some(text), Some(hash)),
            _ => (true, None, None),
        }
    };
    Ok(FsContent {
        language: language(&path).map(str::to_owned),
        path,
        content,
        size,
        binary,
        too_large,
        hash,
    })
```

After `read`:

```rust
/// `fs.write` of `file` (ADR 0028): replaces its text with `content` if its bytes still hash
/// to `expected_hash`; otherwise nothing is written and the disk's hash comes back.
// ponytail: written in place (keeps permissions, symlinks and hard links), not crash-atomic,
// and a writer between the check and the write is overwritten. Write a temp file and rename
// it if torn saves ever show up.
pub fn write(file: &Path, content: &str, expected_hash: &str) -> Result<FsWritten, ReadError> {
    let len = u64::try_from(content.len()).unwrap_or(u64::MAX);
    if len > MAX_BYTES {
        return Err(ReadError::TooLarge(len));
    }
    match file.metadata() {
        Ok(meta) if meta.is_file() => {}
        Ok(_) => return Err(ReadError::NotFound),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(ReadError::NotFound),
        Err(e) => return Err(e.into()),
    }
    let disk = docs::hash(&std::fs::read(file)?);
    if disk != expected_hash {
        return Ok(FsWritten {
            saved: false,
            hash: disk,
        });
    }
    std::fs::write(file, content)?;
    Ok(FsWritten {
        saved: true,
        hash: docs::hash(content.as_bytes()),
    })
}
```

Run: `cargo test -p mkrew-daemon --lib files::tests -j 2`
Expected: PASS.

- [ ] **Step 5: Server handler + dispatch**

`server/fs.rs:8`: add `FsWriteParams` to the `mkrew_protocol` import. Change :12 to `use crate::docs::ReadError;` plus `use crate::{docs, files};`. After `fs_read` (:77):

```rust
    /// `fs.write` (ADR 0028): the same path checks as `fs.read`; the file must exist.
    pub(super) async fn fs_write(&self, params: FsWriteParams) -> Outcome {
        let Some((path, relative)) = docs::relative_path(&params.path) else {
            return outside("path", &params.path);
        };
        if in_git_dir(&path) {
            return outside("path", &params.path);
        }
        let target = match self
            .watched_target(params.project_id, params.worktree_task_id)
            .await
        {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let file = target.dir.join(relative);
        let written = tokio::task::spawn_blocking(move || {
            files::write(&file, &params.content, &params.expected_hash)
        })
        .await;
        match written {
            Ok(Ok(written)) => ok(&written),
            Ok(Err(ReadError::TooLarge(len))) => Outcome::error(
                ErrorCode::InvalidRequest,
                format!("{path}: {len} bytes; at most {} can be saved", docs::MAX_BYTES),
            ),
            Ok(Err(e)) => read_error(target.project.id, &path, e),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("fs.write failed: {e}")),
        }
    }
```

In `server.rs`, after :401:

```rust
            Request::FsWrite(params) => (self.fs_write(params).await, After::Continue),
```

- [ ] **Step 6: Integration test**

In `crates/mkrew-daemon/tests/daemon.rs`, add `FsWriteParams, FsWritten` to the `mkrew_protocol` imports (:19-20). In `fs_list_read_and_find_follow_file_changes`, after the `nope.rs` NotFound assertion (:4927-4930), where `content` (:4914) and `binary` (:4917) are in scope:

```rust
    let save = |path: &str, content: &str, expected_hash: &str| {
        Request::FsWrite(FsWriteParams {
            project_id: id,
            worktree_task_id: None,
            path: path.into(),
            content: content.into(),
            expected_hash: expected_hash.into(),
        })
    };
    assert_eq!(binary.hash, None);
    let read_hash = content.hash.clone().expect("text has a hash");
    let saved: FsWritten = client
        .call(save("src/main.rs", "fn main() { saved() }\n", &read_hash))
        .into_result()
        .unwrap();
    assert!(saved.saved);
    assert_eq!(
        std::fs::read_to_string(repo.join("src/main.rs")).unwrap(),
        "fn main() { saved() }\n"
    );
    let reread: FsContent = client.call(read("src/main.rs")).into_result().unwrap();
    assert_eq!(reread.hash.as_deref(), Some(saved.hash.as_str()));
    // An old hash saves nothing.
    let stale: FsWritten = client
        .call(save("src/main.rs", "lost\n", &read_hash))
        .into_result()
        .unwrap();
    assert_eq!((stale.saved, stale.hash), (false, saved.hash));
    for (path, code) in [
        ("../x", ErrorCode::InvalidRequest),
        (".git/HEAD", ErrorCode::InvalidRequest),
        ("new.rs", ErrorCode::NotFound),
    ] {
        assert_eq!(error_code(client.call(save(path, "x", "h"))), code, "{path}");
    }
```

Run: `cargo test -p mkrew-daemon --test daemon fs_list_read_and_find_follow_file_changes -j 2`
Expected: PASS.

In `crates/mkrew-ui/src/highlight.rs`, add `hash: None,` to the `FsContent` literals at :303 and :378.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean. The UI does not use the hash until Task 5.

- [ ] **Step 7: ADR 0028**

Create `docs/decisions/0028-files-editor.md`:

```markdown
# 0028 — The Files view edits files

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

Inbox items 28 and 29: the user expects to edit the opened file in the Files view, and to find
and replace text with VS Code keys. SPEC §1 said "Not a code editor … editing happens in Zed",
and §12A said "Viewer: read-only". Switching to Zed for a one-word fix in a plan or a config
breaks the flow that the Files view is for.

## Options

1. Keep the viewer read-only and only improve "Open in Zed". No work, but it does not answer
   the inbox items.
2. A plain editor in the viewer: caret, selection, undo and redo, clipboard, find and replace,
   Ctrl+S. Saves go over IPC with a conflict check. No LSP, completion, multi-cursor or folding.
3. Embed Zed's editor crate. It is not published for gpui-pre, it is very large, and it pulls
   in the LSP stack.

## Decision

Option 2.

- **Buffer and screen.** The buffer (`mkrew-ui/src/editor.rs`) is pure and tested. The screen
  reuses the viewer's rows and draws a caret and a selection on the monospace grid.
- **Saving.** Saving uses `fs.write` (protocol 27). It sends the whole text and the `hash`
  that `fs.read` returned (`docs::hash` of the bytes). The daemon writes only if the file still
  hashes to that value. If not, it answers `saved: false` with the disk's hash. The editor then
  shows "Changed on disk — Reload · Keep mine". "Keep mine" saves over that hash.
- **Unsaved text.** A re-read never replaces unsaved text. Leaving it asks "Save / Don't save /
  Cancel" (§2.5). That happens when the user opens another file, picks another worktree or
  closes the window.
- **The write.** The file is written in place (`std::fs::write`). This keeps permissions,
  symlinks and hard links. The write is not crash-atomic, and a writer between the check and the
  write is overwritten.
- **Limits.**
  - Existing files only.
  - At most 2 MB of UTF-8 text.
  - Line endings are kept: the first line's ending is used for the whole file.
  - The final newline is kept.
- **Keys.** The editor handles its own editing keys (arrows, Ctrl+Z/Y/A/C/X/V, …). They are not
  in the keymap, like Ctrl+C in terminals (ADR 0022).
  - Save, Find and Replace are keymap commands: `ctrl-s`, `ctrl-f`, `ctrl-h`. They work only in
    the Files context. No terminal has focus there, so plain Ctrl+letter is safe (§13).
  - Ctrl+Shift+Z stays "Open in Zed", so redo is Ctrl+Y only.
  - VS Code's Ctrl+Shift+1 (replace one) is "Board" here. Replace one is Enter in the replace
    field.

## Consequences

SPEC §1, §7, §12A, §13 and M7 change. A save is a file change like any other. The daemon's
watcher attributes it to the single working agent of the workspace (`files_changed`), exactly
as it does for an edit made in Zed today.

Not done: autosave, creating or renaming files, a crash-safe write.

Known limits:
- AltGr characters on Ctrl+Alt+digit cannot be typed, because those keys jump between tabs (the
  same as in terminals).
- The caret is placed on a monospace grid, so wide CJK glyphs move it out of place.
```

- [ ] **Step 8: SPEC**

- §1 :57: replace "Browse project files and view them read-only;" with "Browse project files, view and edit them (a plain editor with find & replace, ADR 0028);".
- §1 :67: replace the bullet with `- Not an IDE: no LSP, completion or multi-cursor. The Files view has a plain editor (ADR 0028); diffs are read-only; bigger edits happen in Zed.`
- §7 :527: replace the `fs.read` row and add one after it:

```
| `fs.read` | `project_id`, `path` | content (≤ 2 MB), binary flag, language, `hash?` (opaque, set with `content`) |
| `fs.write` | `project_id`, `path`, `content`, `expected_hash` | `saved`, `hash` — writes an existing file (≤ 2 MB text, in place) only if it still hashes to `expected_hash`; else `saved: false`, nothing written, `hash` = the disk's (ADR 0028) |
```

- M7 :1165: replace "read-only viewer with syntax" with "viewer/editor with syntax".

The §12A viewer text changes in Task 5 (editing), Task 6 (unsaved changes) and Task 7 (find).

Run the full verification.
Suggested message: `feat(protocol): fs.write saves over the version read (v27, ADR 0028)`

---

### Task 4: The editor buffer (`editor.rs`)

Inbox: line 29 (with Tasks 3, 5, 6). Spec: §12A (ADR 0028).

**Current state (investigated):**
- `highlight::Code::plain` (`highlight.rs:100-113`) turns tabs into `TAB` (`"    "`, :25), strips `\r` and drops the final newline. It is lossy, so the editor keeps the raw text itself and uses `Code` only for the spans of display lines.
- `text_box.rs` and the dialog fields only append and erase at the end. No caret-in-text buffer exists yet.
- Word semantics in the terminal: Ctrl+Backspace sends ^W (`terminal/input.rs:140`), which deletes the previous word. The HUMAN_CHECKS item says "type `hello world`, Ctrl+Backspace deletes `world` only". The editor matches this, and stops at punctuation the way VS Code does.
- Window bindings the editor must not take:
  - Ctrl+Shift+Z: OpenInZed (`keymap.rs:297`)
  - Alt+arrows: Focus (:282-285)
  - Ctrl+Alt+digit: Tab (:277)
  - Ctrl+Tab / Ctrl+Shift+Tab: Next/PreviousTab (:275-276)

  `Keystroke { modifiers, key, key_char }` is defined in gpui-pre `platform/keystroke.rs:18`.
- Modules live in `crates/mkrew-ui/src/lib.rs` (:14-63), and `files_view.rs` is a lib module. So `editor` is registered there, not in main.rs as the brief said: `files_view` must reach it.

**Files:**
- Create: `crates/mkrew-ui/src/editor.rs`
- Modify: `crates/mkrew-ui/src/lib.rs:24-25` (`pub mod editor;` after `docs`), doc comment :8 ("…the Files view-model with its editor buffer, its syntax highlighting…")

**Interfaces:**
- Produces: `editor::Pos { line: usize, col: usize }` (`col` = byte offset on a char boundary; `Ord`) and `Pos::new`.
- Produces: `editor::Editor`, with these methods:
  - text: `new(&str)`, `text() -> String`, `lines() -> &[String]`
  - caret and selection: `cursor() -> Pos`, `selection() -> Option<Range<Pos>>`, `selected_text() -> Option<String>`, `set_cursor(Pos, extend: bool)`, `select(Range<Pos>)`, `select_all()`
  - edits: `replace(Range<Pos>, &str)`, `replace_all(Vec<(Range<Pos>, String)>)`, `insert(&str)`, `cut() -> Option<String>`, `key(EditKey, page: usize) -> bool`
  - history: `undo() -> bool`, `redo() -> bool`, `version() -> u64`
  - saved state: `is_dirty() -> bool`, `save_point() -> Option<u64>`, `mark_saved(Option<u64>)`
- Produces: `editor::Motion`, `editor::EditKey`, `editor::edit_key(&Keystroke) -> Option<EditKey>`.
- Produces display helpers (tabs drawn as `highlight::TAB`):
  - `display_line(&str) -> Cow<str>`
  - `display_col(&str, byte) -> usize`
  - `display_byte(&str, byte) -> usize`
  - `byte_for_display_col(&str, col) -> usize`
  - `selected_cells(&str, line_index, &Range<Pos>) -> Option<Range<usize>>`

- [ ] **Step 1: Failing tests**

Create `crates/mkrew-ui/src/editor.rs` with only the test module below, and add `pub mod editor;` in lib.rs:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn type_chars(editor: &mut Editor, text: &str) {
        for c in text.chars() {
            editor.key(EditKey::Insert(c.to_string()), 10);
        }
    }

    fn press(editor: &mut Editor, motion: Motion, extend: bool) {
        editor.key(EditKey::Move { motion, extend }, 10);
    }

    #[test]
    fn text_round_trips_line_endings_tabs_and_empty_files() {
        for text in ["", "\n", "a", "a\n", "a\r\nb\r\n", "a\r\nb", "\tx\n\n", "x\r\r\n", "việt\n"] {
            assert_eq!(Editor::new(text).text(), text, "{text:?}");
        }
        // ponytail: mixed endings become the first line's.
        assert_eq!(Editor::new("a\r\nb\nc").text(), "a\r\nb\r\nc");
        assert_eq!(Editor::new("a\r\nb\n").lines(), ["a", "b"]);
    }

    #[test]
    fn typing_and_arrows_stay_on_char_boundaries() {
        let mut editor = Editor::new("việt");
        for _ in 0..3 {
            press(&mut editor, Motion::Right, false);
        }
        assert_eq!(editor.cursor(), Pos::new(0, 5), "past the 3-byte ệ");
        editor.key(EditKey::Backspace { word: false }, 10);
        assert_eq!((editor.text().as_str(), editor.cursor()), ("vit", Pos::new(0, 2)));
        type_chars(&mut editor, "ệ");
        assert_eq!(editor.text(), "việt");
        // A click below the text or inside a char lands on a boundary.
        editor.set_cursor(Pos::new(5, 3), false);
        assert_eq!(editor.cursor(), Pos::new(0, 2));
        editor.set_cursor(Pos::new(0, 99), false);
        assert_eq!(editor.cursor(), Pos::new(0, 6));
        // ← / → and Backspace cross line breaks.
        let mut editor = Editor::new("ab\ncd");
        editor.set_cursor(Pos::new(1, 0), false);
        press(&mut editor, Motion::Left, false);
        assert_eq!(editor.cursor(), Pos::new(0, 2));
        press(&mut editor, Motion::Right, false);
        assert_eq!(editor.cursor(), Pos::new(1, 0));
        editor.key(EditKey::Backspace { word: false }, 10);
        assert_eq!(editor.text(), "abcd");
    }

    #[test]
    fn ctrl_arrows_and_ctrl_delete_go_by_words_and_punctuation() {
        let mut editor = Editor::new("hello world");
        press(&mut editor, Motion::End, false);
        editor.key(EditKey::Backspace { word: true }, 10);
        assert_eq!(editor.text(), "hello ", "like the terminal's Ctrl+Backspace");
        let mut editor = Editor::new("foo.bar(baz)\nnext");
        press(&mut editor, Motion::End, false);
        let mut stops = vec![];
        for _ in 0..6 {
            press(&mut editor, Motion::WordLeft, false);
            stops.push(editor.cursor().col);
        }
        assert_eq!(stops, [11, 8, 7, 4, 3, 0]);
        press(&mut editor, Motion::WordRight, false);
        assert_eq!(editor.cursor(), Pos::new(0, 3));
        // At a line start, Ctrl+Backspace joins the line above.
        editor.set_cursor(Pos::new(1, 0), false);
        editor.key(EditKey::Backspace { word: true }, 10);
        assert_eq!(editor.text(), "foo.bar(baz)next");
        // Ctrl+Delete eats the blanks and the next word.
        let mut editor = Editor::new("a   bc d");
        editor.set_cursor(Pos::new(0, 1), false);
        editor.key(EditKey::Delete { word: true }, 10);
        assert_eq!(editor.text(), "a d");
    }

    #[test]
    fn undo_takes_back_a_typed_run_at_once_and_dirty_follows_the_saved_state() {
        let mut editor = Editor::new("x");
        assert!(!editor.is_dirty());
        type_chars(&mut editor, "abc");
        assert!(editor.is_dirty());
        press(&mut editor, Motion::End, false);
        type_chars(&mut editor, "!");
        assert_eq!(editor.text(), "abcx!");
        assert!(editor.undo());
        assert_eq!(editor.text(), "abcx");
        assert!(editor.undo());
        assert_eq!((editor.text().as_str(), editor.cursor()), ("x", Pos::new(0, 0)));
        assert!(!editor.is_dirty(), "back to what was read");
        assert!(!editor.undo());
        assert!(editor.redo());
        assert_eq!(editor.text(), "abcx");
        // Saved here: undo makes it dirty, redo clean again; a new edit drops the redo.
        let point = editor.save_point();
        editor.mark_saved(point);
        assert!(!editor.is_dirty());
        editor.undo();
        assert!(editor.is_dirty());
        editor.redo();
        assert!(!editor.is_dirty());
        editor.undo();
        type_chars(&mut editor, "z");
        assert!(!editor.redo());
        assert!(editor.is_dirty());
    }

    #[test]
    fn typing_while_a_save_is_in_flight_stays_unsaved() {
        let mut editor = Editor::new("");
        type_chars(&mut editor, "ab");
        let point = editor.save_point();
        type_chars(&mut editor, "c");
        editor.mark_saved(point);
        assert!(editor.is_dirty(), "the c was not saved");
        editor.undo();
        assert_eq!(editor.text(), "ab");
        assert!(!editor.is_dirty());
    }

    #[test]
    fn a_selection_is_replaced_by_typing_and_paste_and_removed_by_cut() {
        let mut editor = Editor::new("one\ntwo\nthree");
        editor.set_cursor(Pos::new(0, 1), false);
        press(&mut editor, Motion::Down, true);
        press(&mut editor, Motion::Right, true);
        assert_eq!(editor.selected_text().as_deref(), Some("ne\ntw"));
        type_chars(&mut editor, "X");
        assert_eq!(editor.text(), "oXo\nthree");
        editor.undo();
        assert_eq!(editor.text(), "one\ntwo\nthree");
        assert_eq!(
            editor.selected_text().as_deref(),
            Some("ne\ntw"),
            "undo brings the selection back"
        );
        // ← collapses the selection to its start.
        press(&mut editor, Motion::Left, false);
        assert_eq!((editor.cursor(), editor.selection()), (Pos::new(0, 1), None));
        editor.key(EditKey::SelectAll, 10);
        assert_eq!(editor.cut().as_deref(), Some("one\ntwo\nthree"));
        assert_eq!(editor.text(), "");
        editor.insert("a\r\nb");
        assert_eq!(
            (editor.lines().len(), editor.cursor()),
            (2, Pos::new(1, 1)),
            "pasted CRLF splits lines"
        );
    }

    #[test]
    fn enter_keeps_the_indent_and_tab_follows_the_file() {
        let mut editor = Editor::new("    fn x() {");
        press(&mut editor, Motion::End, false);
        editor.key(EditKey::Enter, 10);
        assert_eq!(
            (editor.text().as_str(), editor.cursor()),
            ("    fn x() {\n    ", Pos::new(1, 4))
        );
        editor.key(EditKey::Tab, 10);
        assert_eq!(editor.lines()[1], "        ");
        let mut tabs = Editor::new("\tlet a = 1;\n");
        tabs.key(EditKey::Tab, 10);
        assert_eq!(tabs.lines()[0], "\t\tlet a = 1;");
        // Home: to the indent first, then to the line start.
        let mut editor = Editor::new("  x");
        press(&mut editor, Motion::End, false);
        press(&mut editor, Motion::Home, false);
        assert_eq!(editor.cursor(), Pos::new(0, 2));
        press(&mut editor, Motion::Home, false);
        assert_eq!(editor.cursor(), Pos::new(0, 0));
    }

    #[test]
    fn up_and_down_keep_the_column_on_screen_across_tabs() {
        assert_eq!(display_col("\tab", 2), 5);
        assert_eq!(display_byte("\tab", 2), 5);
        assert_eq!(display_line("\tab"), "    ab");
        assert_eq!(byte_for_display_col("\tab", 1), 0, "inside the tab, nearer its start");
        assert_eq!(byte_for_display_col("\tab", 3), 1);
        assert_eq!(byte_for_display_col("việt", 3), 5);
        assert_eq!(byte_for_display_col("ab", 9), 2);
        let selection = Pos::new(0, 1)..Pos::new(2, 1);
        assert_eq!(selected_cells("\tab", 0, &selection), Some(4..7), "with the line break");
        assert_eq!(selected_cells("", 1, &selection), Some(0..1));
        assert_eq!(selected_cells("xy", 2, &selection), Some(0..1));
        assert_eq!(selected_cells("xy", 3, &selection), None);
        let mut editor = Editor::new("\tab\nx\nabcdefgh");
        press(&mut editor, Motion::End, false); // screen column 6
        press(&mut editor, Motion::Down, false);
        assert_eq!(editor.cursor(), Pos::new(1, 1));
        press(&mut editor, Motion::Down, false);
        assert_eq!(editor.cursor(), Pos::new(2, 6), "the column survives a short line");
        press(&mut editor, Motion::PageUp, false);
        assert_eq!(editor.cursor(), Pos::new(0, 3));
    }

    #[test]
    fn replace_all_is_one_undo_step() {
        let mut editor = Editor::new("a-a\na");
        editor.replace_all(vec![
            (Pos::new(0, 0)..Pos::new(0, 1), "bb".into()),
            (Pos::new(0, 2)..Pos::new(0, 3), "bb".into()),
            (Pos::new(1, 0)..Pos::new(1, 1), "bb".into()),
        ]);
        assert_eq!(editor.text(), "bb-bb\nbb");
        assert_eq!(editor.cursor(), Pos::new(0, 2), "after the first replacement");
        assert!(editor.undo());
        assert_eq!(editor.text(), "a-a\na");
        assert!(!editor.is_dirty());
        assert!(editor.redo());
        assert_eq!(editor.text(), "bb-bb\nbb");
    }

    #[test]
    fn keys_follow_vs_code_but_leave_the_app_shortcuts_alone() {
        let key = |keys: &str| edit_key(&Keystroke::parse(keys).unwrap());
        assert_eq!(
            key("ctrl-shift-left"),
            Some(EditKey::Move { motion: Motion::WordLeft, extend: true })
        );
        assert_eq!(
            key("ctrl-end"),
            Some(EditKey::Move { motion: Motion::DocEnd, extend: false })
        );
        assert_eq!(key("ctrl-backspace"), Some(EditKey::Backspace { word: true }));
        assert_eq!(key("ctrl-z"), Some(EditKey::Undo));
        assert_eq!(key("ctrl-y"), Some(EditKey::Redo));
        assert_eq!(key("ctrl-v"), Some(EditKey::Paste));
        // Window shortcuts go on to the keymap: Ctrl+Shift+Z is Open in Zed, Alt+arrows move
        // pane focus, Ctrl+S is Save.
        for keys in ["ctrl-shift-z", "alt-left", "ctrl-s", "ctrl-shift-tab", "ctrl-up"] {
            assert_eq!(key(keys), None, "{keys}");
        }
        // AltGr arrives as Ctrl+Alt with the composed char: it is typed.
        let mut altgr = Keystroke::parse("ctrl-alt-q").unwrap();
        altgr.key_char = Some("@".into());
        assert_eq!(edit_key(&altgr), Some(EditKey::Insert("@".into())));
    }
}
```

Run: `cargo test -p mkrew-ui --lib editor::tests -j 4`
Expected: FAIL to compile (`Editor`, `EditKey` and the rest are missing).

- [ ] **Step 2: Implement `editor.rs`**

Put this code above the tests:

```rust
//! The Files editor's text (SPEC §12A, ADR 0028): the raw lines of the opened file, a caret
//! with an optional selection, and undo / redo. Pure and tested headlessly; `files_view.rs`
//! keeps one per opened file, `main.rs` draws it and feeds it keys.

use std::borrow::Cow;
use std::ops::Range;

use gpui::Keystroke;

use crate::highlight::TAB;

/// A place in the text: a line (0-based) and a byte offset into it, on a char boundary.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Pos {
        Pos { line, col }
    }
}

/// Where a caret key moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    WordLeft,
    WordRight,
    Up,
    Down,
    /// To the indent, then to the line start.
    Home,
    End,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

/// What a key press in the editor does (VS Code keys; not in the keymap, ADR 0028).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditKey {
    Move { motion: Motion, extend: bool },
    Backspace { word: bool },
    Delete { word: bool },
    Enter,
    Tab,
    SelectAll,
    Undo,
    Redo,
    /// The shell does the clipboard: Ctrl+C, Ctrl+X, Ctrl+V.
    Copy,
    Cut,
    Paste,
    Insert(String),
}

/// One change: `removed` at `at` became `inserted` (`\n` line breaks).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    at: Pos,
    removed: String,
    inserted: String,
}

/// One undo step: its edits in the order they were made, and the caret around them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    /// Unique per editor: the saved state is the id of the step on top of the undo stack.
    id: u64,
    edits: Vec<Edit>,
    before: (Pos, Option<Pos>),
    after: Pos,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    /// Without line endings; never empty.
    lines: Vec<String>,
    /// Lines end in `\r\n` (the first line's ending decides).
    crlf: bool,
    final_newline: bool,
    cursor: Pos,
    /// The other end of the selection.
    anchor: Option<Pos>,
    /// The screen column ↑ / ↓ aim for.
    goal: Option<usize>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    next_id: u64,
    /// The undo top that is on disk; `None` = the text it was opened with.
    saved: Option<u64>,
    /// The last edit was a typed char: the next one joins its undo step.
    typing: bool,
    /// Bumped by every text change (re-highlighting, find).
    version: u64,
}

impl Editor {
    pub fn new(text: &str) -> Editor {
        let crlf = text.find('\n').is_some_and(|end| text[..end].ends_with('\r'));
        let final_newline = text.ends_with('\n');
        let body = text.strip_suffix('\n').unwrap_or(text);
        let lines = body
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
            .collect();
        Editor {
            lines,
            crlf,
            final_newline,
            cursor: Pos::default(),
            anchor: None,
            goal: None,
            undo: Vec::new(),
            redo: Vec::new(),
            next_id: 0,
            saved: None,
            typing: false,
            version: 0,
        }
    }

    /// The text to save, with the file's line endings.
    pub fn text(&self) -> String {
        let eol = if self.crlf { "\r\n" } else { "\n" };
        let mut text = self.lines.join(eol);
        if self.final_newline {
            text.push_str(eol);
        }
        text
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> Pos {
        self.cursor
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// The selected range, start first; `None` when nothing is selected.
    pub fn selection(&self) -> Option<Range<Pos>> {
        let anchor = self.anchor.filter(|anchor| *anchor != self.cursor)?;
        Some(anchor.min(self.cursor)..anchor.max(self.cursor))
    }

    pub fn selected_text(&self) -> Option<String> {
        self.selection().map(|range| self.text_in(range))
    }

    /// Whether the text differs from the saved (or opened) one by its undo history.
    pub fn is_dirty(&self) -> bool {
        self.undo.last().map(|step| step.id) != self.saved
    }

    /// The state a save sends. Typing after it starts a new undo step, so that state stays the
    /// saved one even when more is typed while the save is in flight.
    pub fn save_point(&mut self) -> Option<u64> {
        self.typing = false;
        self.undo.last().map(|step| step.id)
    }

    /// The state from [`Self::save_point`] is on disk now.
    pub fn mark_saved(&mut self, point: Option<u64>) {
        self.saved = point;
    }

    /// Moves the caret (clamped into the text); `extend` keeps or starts a selection.
    pub fn set_cursor(&mut self, pos: Pos, extend: bool) {
        let pos = self.clamp(pos);
        self.anchor = extend.then(|| self.anchor.unwrap_or(self.cursor));
        self.cursor = pos;
        self.goal = None;
        self.typing = false;
    }

    pub fn select(&mut self, range: Range<Pos>) {
        self.set_cursor(range.start, false);
        self.set_cursor(range.end, true);
    }

    pub fn select_all(&mut self) {
        self.select(Pos::default()..self.end());
    }

    /// Puts `text` in place of `range` as one undo step; the caret goes after it.
    pub fn replace(&mut self, range: Range<Pos>, text: &str) {
        self.replace_all(vec![(range, text.to_owned())]);
    }

    /// Puts each text in place of its range as ONE undo step (Replace all). The ranges are
    /// sorted and do not overlap; the caret goes after the first replacement.
    pub fn replace_all(&mut self, edits: Vec<(Range<Pos>, String)>) {
        if edits.is_empty() {
            return;
        }
        let before = (self.cursor, self.anchor);
        let mut done = Vec::with_capacity(edits.len());
        let mut after = self.cursor;
        // Last first: the earlier ranges stay where they were.
        for (range, text) in edits.into_iter().rev() {
            let range = self.clamp(range.start)..self.clamp(range.end);
            let text = text.replace("\r\n", "\n").replace('\r', "\n");
            let (removed, end) = self.splice(range.clone(), &text);
            after = end;
            done.push(Edit {
                at: range.start,
                removed,
                inserted: text,
            });
        }
        self.cursor = after;
        self.anchor = None;
        self.goal = None;
        self.typing = false;
        self.push(Step {
            id: 0,
            edits: done,
            before,
            after,
        });
    }

    /// Pasted text: replaces the selection; one undo step.
    pub fn insert(&mut self, text: &str) {
        let range = self.selection().unwrap_or(self.cursor..self.cursor);
        self.replace(range, text);
    }

    /// Ctrl+X: removes the selection and returns it.
    pub fn cut(&mut self) -> Option<String> {
        let range = self.selection()?;
        let text = self.text_in(range.clone());
        self.replace(range, "");
        Some(text)
    }

    pub fn undo(&mut self) -> bool {
        let Some(step) = self.undo.pop() else {
            return false;
        };
        for edit in step.edits.iter().rev() {
            self.splice(edit.at..end_of(edit.at, &edit.inserted), &edit.removed);
        }
        (self.cursor, self.anchor) = step.before;
        self.goal = None;
        self.typing = false;
        self.redo.push(step);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(step) = self.redo.pop() else {
            return false;
        };
        for edit in &step.edits {
            self.splice(edit.at..end_of(edit.at, &edit.removed), &edit.inserted);
        }
        self.cursor = step.after;
        self.anchor = None;
        self.goal = None;
        self.typing = false;
        self.undo.push(step);
        true
    }

    /// Applies `key`. Copy, Cut and Paste belong to the shell and do nothing here. `page` is
    /// the number of lines on screen. Returns whether the text changed.
    pub fn key(&mut self, key: EditKey, page: usize) -> bool {
        let version = self.version;
        match key {
            EditKey::Move { motion, extend } => self.move_caret(motion, extend, page),
            EditKey::Backspace { word } => self.delete(word, false),
            EditKey::Delete { word } => self.delete(word, true),
            EditKey::Enter => {
                let line = &self.lines[self.cursor.line];
                let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
                let text = format!("\n{}", &line[..indent.min(self.cursor.col)]);
                self.insert(&text);
            }
            EditKey::Tab => {
                // ponytail: a tab-indented file gets a tab, other files 4 spaces; no Shift+Tab
                // outdent and no block indent. Add them if asked for.
                let tabs = self.lines.iter().any(|line| line.starts_with('\t'));
                self.insert(if tabs { "\t" } else { TAB });
            }
            EditKey::SelectAll => self.select_all(),
            EditKey::Undo => {
                self.undo();
            }
            EditKey::Redo => {
                self.redo();
            }
            EditKey::Insert(text) => self.type_text(&text),
            EditKey::Copy | EditKey::Cut | EditKey::Paste => {}
        }
        self.version != version
    }

    /// A typed char: like [`Self::insert`], but a run of them is one undo step.
    fn type_text(&mut self, text: &str) {
        let joins = self.typing
            && self.selection().is_none()
            && (self.undo.last())
                .is_some_and(|step| Some(step.id) != self.saved && step.after == self.cursor);
        if !joins {
            self.insert(text);
            self.typing = true;
            return;
        }
        let (_, after) = self.splice(self.cursor..self.cursor, text);
        self.cursor = after;
        self.redo.clear();
        if let Some(step) = self.undo.last_mut() {
            step.after = after;
            if let Some(edit) = step.edits.last_mut() {
                edit.inserted.push_str(text);
            }
        }
    }

    fn delete(&mut self, word: bool, forward: bool) {
        let range = self.selection().unwrap_or_else(|| {
            let to = match (word, forward) {
                (false, false) => self.left(self.cursor),
                (false, true) => self.right(self.cursor),
                (true, false) => self.word_left(self.cursor),
                (true, true) => self.word_right(self.cursor),
            };
            self.cursor.min(to)..self.cursor.max(to)
        });
        if range.start != range.end {
            self.replace(range, "");
        }
    }

    fn move_caret(&mut self, motion: Motion, extend: bool, page: usize) {
        let at = self.cursor;
        let last = self.lines.len() - 1;
        let goal = (self.goal).unwrap_or_else(|| display_col(&self.lines[at.line], at.col));
        let vertical = |line: usize| Pos::new(line, byte_for_display_col(&self.lines[line], goal));
        let to = match (motion, self.selection()) {
            (Motion::Left, Some(selection)) if !extend => selection.start,
            (Motion::Right, Some(selection)) if !extend => selection.end,
            (Motion::Left, _) => self.left(at),
            (Motion::Right, _) => self.right(at),
            (Motion::WordLeft, _) => self.word_left(at),
            (Motion::WordRight, _) => self.word_right(at),
            (Motion::Up, _) if at.line == 0 => Pos::default(),
            (Motion::Up, _) => vertical(at.line - 1),
            (Motion::Down, _) if at.line == last => self.end(),
            (Motion::Down, _) => vertical(at.line + 1),
            (Motion::PageUp, _) => vertical(at.line.saturating_sub(page)),
            (Motion::PageDown, _) => vertical((at.line + page).min(last)),
            (Motion::Home, _) => {
                let line = &self.lines[at.line];
                let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
                Pos::new(at.line, if at.col == indent { 0 } else { indent })
            }
            (Motion::End, _) => Pos::new(at.line, self.lines[at.line].len()),
            (Motion::DocStart, _) => Pos::default(),
            (Motion::DocEnd, _) => self.end(),
        };
        self.set_cursor(to, extend);
        if matches!(
            motion,
            Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
        ) {
            self.goal = Some(goal);
        }
    }

    fn end(&self) -> Pos {
        let line = self.lines.len() - 1;
        Pos::new(line, self.lines[line].len())
    }

    fn clamp(&self, pos: Pos) -> Pos {
        let line = pos.line.min(self.lines.len() - 1);
        let text = &self.lines[line];
        let mut col = pos.col.min(text.len());
        while !text.is_char_boundary(col) {
            col -= 1;
        }
        Pos::new(line, col)
    }

    fn left(&self, pos: Pos) -> Pos {
        match self.lines[pos.line][..pos.col].chars().next_back() {
            Some(c) => Pos::new(pos.line, pos.col - c.len_utf8()),
            None if pos.line > 0 => Pos::new(pos.line - 1, self.lines[pos.line - 1].len()),
            None => pos,
        }
    }

    fn right(&self, pos: Pos) -> Pos {
        match self.lines[pos.line][pos.col..].chars().next() {
            Some(c) => Pos::new(pos.line, pos.col + c.len_utf8()),
            None if pos.line + 1 < self.lines.len() => Pos::new(pos.line + 1, 0),
            None => pos,
        }
    }

    /// Ctrl+←: skips blanks, then one run of word chars or of punctuation (like VS Code). At
    /// a line start it goes to the end of the line above.
    fn word_left(&self, pos: Pos) -> Pos {
        if pos.col == 0 {
            return self.left(pos);
        }
        let before = self.lines[pos.line][..pos.col].trim_end_matches(char::is_whitespace);
        let Some(last) = before.chars().next_back() else {
            return Pos::new(pos.line, 0);
        };
        let kind = class(last);
        Pos::new(pos.line, before.trim_end_matches(|c| class(c) == kind).len())
    }

    /// Ctrl+→: the mirror of [`Self::word_left`].
    fn word_right(&self, pos: Pos) -> Pos {
        let line = &self.lines[pos.line];
        if pos.col == line.len() {
            return self.right(pos);
        }
        let after = line[pos.col..].trim_start_matches(char::is_whitespace);
        let Some(first) = after.chars().next() else {
            return Pos::new(pos.line, line.len());
        };
        let kind = class(first);
        let rest = after.trim_start_matches(|c| class(c) == kind);
        Pos::new(pos.line, line.len() - rest.len())
    }

    fn text_in(&self, range: Range<Pos>) -> String {
        let (start, end) = (range.start, range.end);
        if start.line == end.line {
            return self.lines[start.line][start.col..end.col].to_owned();
        }
        let mut text = self.lines[start.line][start.col..].to_owned();
        for line in &self.lines[start.line + 1..end.line] {
            text.push('\n');
            text.push_str(line);
        }
        text.push('\n');
        text.push_str(&self.lines[end.line][..end.col]);
        text
    }

    /// The only text change: `text` (with `\n` breaks) replaces `range`. Returns the removed
    /// text and the position where the new text ends.
    fn splice(&mut self, range: Range<Pos>, text: &str) -> (String, Pos) {
        let removed = self.text_in(range.clone());
        let (start, end) = (range.start, range.end);
        let tail = self.lines[end.line][end.col..].to_owned();
        let mut new: Vec<String> = text.split('\n').map(str::to_owned).collect();
        new[0].insert_str(0, &self.lines[start.line][..start.col]);
        let last = new.len() - 1;
        let after = Pos::new(start.line + last, new[last].len());
        new[last].push_str(&tail);
        self.lines.splice(start.line..=end.line, new);
        self.version += 1;
        (removed, after)
    }

    fn push(&mut self, mut step: Step) {
        step.id = self.next_id;
        self.next_id += 1;
        self.undo.push(step);
        self.redo.clear();
    }
}

/// Where `text` ends when it starts at `at`.
fn end_of(at: Pos, text: &str) -> Pos {
    match text.rsplit_once('\n') {
        Some((head, last)) => Pos::new(at.line + head.matches('\n').count() + 1, last.len()),
        None => Pos::new(at.line, at.col + text.len()),
    }
}

/// Blanks, word chars, punctuation: Ctrl+arrows stop where the class changes.
fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

/// `line` as drawn: tabs as [`TAB`] (like `highlight::Code`).
pub fn display_line(line: &str) -> Cow<'_, str> {
    if line.contains('\t') {
        Cow::Owned(line.replace('\t', TAB))
    } else {
        Cow::Borrowed(line)
    }
}

/// The screen column (chars from the line start) of byte `byte` of `line`.
// ponytail: one cell per char, so wide CJK glyphs misplace the caret. Measure the shaped line
// if that ever matters.
pub fn display_col(line: &str, byte: usize) -> usize {
    let before = line.get(..byte).unwrap_or(line);
    before.chars().map(|c| if c == '\t' { TAB.len() } else { 1 }).sum()
}

/// The byte offset in [`display_line`] of byte `byte` of `line` (find highlights).
pub fn display_byte(line: &str, byte: usize) -> usize {
    let before = line.get(..byte).unwrap_or(line);
    before.len() + before.matches('\t').count() * (TAB.len() - 1)
}

/// The byte of `line` nearest to screen column `col` (clicks, ↑ / ↓), clamped to its end.
pub fn byte_for_display_col(line: &str, col: usize) -> usize {
    let mut at = 0;
    for (byte, c) in line.char_indices() {
        let width = if c == '\t' { TAB.len() } else { 1 };
        if col < at + width {
            return if col - at <= width / 2 { byte } else { byte + c.len_utf8() };
        }
        at += width;
    }
    line.len()
}

/// The screen cells of line `index` (text `line`) inside `selection`. When the line break is
/// selected too, this includes one cell past the text.
pub fn selected_cells(line: &str, index: usize, selection: &Range<Pos>) -> Option<Range<usize>> {
    if index < selection.start.line || index > selection.end.line {
        return None;
    }
    let start = if index == selection.start.line {
        display_col(line, selection.start.col)
    } else {
        0
    };
    let end = if index == selection.end.line {
        display_col(line, selection.end.col)
    } else {
        display_col(line, line.len()) + 1
    };
    (start < end).then_some(start..end)
}

/// The editor meaning of `keystroke` (VS Code keys). `None` means the key is not the editor's:
/// window shortcuts like Ctrl+Shift+Z, Alt+arrows and Ctrl+S go on to the keymap.
pub fn edit_key(keystroke: &Keystroke) -> Option<EditKey> {
    let m = keystroke.modifiers;
    let ctrl = m.control || m.platform;
    let plain_ctrl = ctrl && !m.alt && !m.shift;
    let caret = |plain, word| {
        let motion = if ctrl { word } else { plain };
        (!m.alt).then_some(EditKey::Move {
            motion,
            extend: m.shift,
        })
    };
    match keystroke.key.as_str() {
        "left" => return caret(Motion::Left, Motion::WordLeft),
        "right" => return caret(Motion::Right, Motion::WordRight),
        "up" if !ctrl => return caret(Motion::Up, Motion::Up),
        "down" if !ctrl => return caret(Motion::Down, Motion::Down),
        "home" => return caret(Motion::Home, Motion::DocStart),
        "end" => return caret(Motion::End, Motion::DocEnd),
        "pageup" if !ctrl => return caret(Motion::PageUp, Motion::PageUp),
        "pagedown" if !ctrl => return caret(Motion::PageDown, Motion::PageDown),
        "backspace" if !m.alt => return Some(EditKey::Backspace { word: ctrl }),
        "delete" if !m.alt => return Some(EditKey::Delete { word: ctrl }),
        "enter" if !ctrl && !m.alt => return Some(EditKey::Enter),
        "tab" if !ctrl && !m.alt && !m.shift => return Some(EditKey::Tab),
        "a" if plain_ctrl => return Some(EditKey::SelectAll),
        "z" if plain_ctrl => return Some(EditKey::Undo),
        "y" if plain_ctrl => return Some(EditKey::Redo),
        "c" if plain_ctrl => return Some(EditKey::Copy),
        "x" if plain_ctrl => return Some(EditKey::Cut),
        "v" if plain_ctrl => return Some(EditKey::Paste),
        _ => {}
    }
    // AltGr arrives as Ctrl+Alt with the composed char; plain Ctrl+key types nothing.
    if m.platform || (m.control && !m.alt) {
        return None;
    }
    keystroke
        .key_char
        .as_deref()
        .filter(|text| !text.is_empty() && !text.chars().any(char::is_control))
        .map(|text| EditKey::Insert(text.to_owned()))
}
```

Run: `cargo test -p mkrew-ui --lib editor::tests -j 4`
Expected: PASS (10 tests).

- [ ] **Step 3: Lint**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean. The module is `pub` in lib.rs, so there is no dead-code warning before Task 5 uses it.

Suggested message: `feat(ui): editor buffer with undo, words and VS Code keys`

---

### Task 5: The viewer becomes the editor (Ctrl+S saves)

Inbox: line 29. Spec: §12A (:800-801, :806), §13 (:865-867, table :903-904). ADR 0028.

**Current state (investigated):**
- `files_view::Content::Shown(Rc<Code>)` (`files_view.rs:27-33`). `FilesView.line: Option<u32>` (:82-83) is the picked line. It is used by:
  - "Open in Zed at line" (`open_file_in_zed` main.rs:1903-1919)
  - Send selection (`picked_lines` main.rs:5792-5805)
  - the header "Line N" (main.rs:8993-8999)
  - `reveal_file_match` (main.rs:1846-1859)
- `FilesView::show` (:334-347) replaces the content unconditionally, `search` (:350-354) searches `code.lines`, and `pick_line` is at :357. Tests that use `code(…)` or `line`:
  - `opening_reads_once_retries_failures_and_coalesces_reads` :610
  - `lines_are_picked_and_cleared` :636
  - `git_changed_relists_open_folders_and_rereads_the_shown_file` :686
  - `find_follows_a_reread_file` :827
- `read_file` (main.rs:1678-1718) runs `Code::new(&content)` on the background executor and drops the `FsContent`.
- `render_file` (main.rs:8924-9156) draws:
  - the header: path, "Line N", Review in Docs, Find, Open in Zed
  - the find bar
  - the body states: Loading, Failed + Retry, the binary / too large notice, "This file is empty." (:9118), and `preview::code_view(…)` with `.track_scroll(&self.file_code_scroll)` (:9123-9145)
- `preview::code_view` (`preview.rs:479-540`) is used only there. `code_line` is at :449 and `gutter` at `docs.rs:299`.
- Focus handles: `files_focus` :300, `file_find_focus` :304, `file_code_scroll` :306. They are created in `Shell::new` at :633-637. The root has `key_context(FILES_CONTEXT)` while Files are shown (:16799).
- IME: `self.typing(focus, on_key, cx)` (:3694) turns platform text into key presses for `on_key`. Composing text is erased with Backspace, which also works with a caret in the middle of a line.
- `run` (:6095) propagates bound commands while `file_find_focus` is focused (:6135-6146). `Command::ToggleIgnored` dispatch is at :6335.
- Cell width: `terminal::view::cell_size(window, theme) -> (Pixels, Pixels)` (`terminal/view.rs:844`, a private `fn`).
- GPUI:
  - `uniform_list(…).with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)` (uniform_list.rs:636) and `.with_width_from_item(Option<usize>)` (:622)
  - `UniformListScrollHandle(pub Rc<RefCell<UniformListScrollState>>)` with `last_item_size: Option<ItemSize { item, contents }>` (:113-132)
  - `Div::on_mouse_move` is hover-gated (div.rs:303-310)
  - `canvas(prepaint, paint)` (canvas.rs:10)
  - `Context::processor` (context.rs:260)
  - `Pixels / Pixels = f32` (geometry.rs:2691)
- Keymap test `agent_keys_are_unbound_and_reach_the_pty` (`keymap.rs:737-758`) forbids every plain Ctrl+letter default. SPEC §13 :918 says "In Files and Git views the terminal never has focus".

**Files:**
- Modify: `crates/mkrew-ui/src/files_view.rs` (`Content`, new `Shown`, `SaveRequest`, `FilesView` fields/methods :66-359, tests :510-842)
- Modify: `crates/mkrew-ui/src/preview.rs` (replace `code_view` :479-540 with `RowMarks` + `code_row`)
- Modify: `crates/mkrew-ui/src/terminal/view.rs:844` (`pub fn cell_size`)
- Modify: `crates/mkrew-ui/src/theme.rs` (`CARET_WIDTH`, `REHIGHLIGHT_DELAY`, after `FIND_INPUT_WIDTH` :129)
- Modify: `crates/mkrew-ui/src/keymap.rs` (`Command::SaveFile`: enum :74-77, `all` :180, `name` :242, `default_keys` :309, `context` :337, doc :8-9, tests :675, :749-757)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title` :207, `available` :270, test :711-721, :757)
- Modify: `crates/mkrew-ui/src/main.rs`:
  - gpui imports (:15-19) and `mkrew_protocol` imports (:28)
  - fields :300-306 and their init :633-637
  - `read_file` :1698-1717
  - `reveal_file_match` :1846-1859
  - `open_file_in_zed` :1907
  - `picked_lines` :5795-5804
  - `run` :6136, :6335
  - `render_file` :8957-9154
  - new methods: `save_file`, `file_editor_key_down`, `click_file_text`, `file_edited`, `file_page_lines`, `render_editor`
- Modify: `docs/SPEC.md` §12A :800-801, :806; §13 :865-867, :904
- Modify: `docs/HUMAN_CHECKS.md` (2026-10-09 section)

**Interfaces:**
- Consumes from Task 3: `FsContent.hash`, `FsWriteParams`, `FsWritten`.
- Consumes from Task 4: `Editor`, `Pos`, `EditKey`, `edit_key`, `display_line`, `display_col`, `display_byte`, `byte_for_display_col`, `selected_cells`.
- Produces in `files_view`:
  - `Content::Shown(Box<Shown>)` and `pub struct Shown { code: Rc<Code>, editor: Editor, hash: Option<String>, language: Option<String>, save_error: Option<String> }`
  - `pub struct SaveRequest { path, text, expected_hash, point }`
  - `FilesView::read(&mut self, path: &str, read: Result<(FsContent, Code), String>) -> Option<String>`
  - accessors: `shown()`, `editor_mut() -> Option<&mut Editor>` (`None` for binary / too large), `unsaved() -> Option<&str>`, `line() -> Option<u32>` (the caret line, 1-based)
  - saving: `begin_save() -> Option<SaveRequest>`, `saved(&mut self, path, point, Result<FsWritten, String>)`
  - re-highlighting: `highlight_request(version) -> Option<FsContent>`, `highlighted(version, Code) -> bool`
- Produces in `preview`: `RowMarks { caret: Option<usize>, selected: Option<Range<usize>>, found: Vec<(Range<usize>, bool)> }` and `code_row(index, total, text, spans, &RowMarks, cell_width, &Rc<Cell<Pixels>>, &Theme) -> Div`.
- Produces in `keymap`: `Command::SaveFile` (`save_file`, `ctrl-s`, `FILES_CONTEXT`).
- Produces on `Shell`: `file_editor_focus: FocusHandle`, `file_text_left: Rc<Cell<Pixels>>`, `save_file(&mut self, project_id: Ulid, window, cx)`. Task 6 adds a `then` parameter.

- [ ] **Step 1: Failing view-model tests**

In the `files_view.rs` tests:
- Replace the `code` helper (:510-512) with the helpers below.
- Replace `Ok(code(…))` with `file_text(…)` in:
  - `opening_reads_once_…` (:620; at :628 use `file_text_at("src/main.rs", "a\nb")`)
  - `git_changed_…` (:697)
  - `find_follows_a_reread_file` (:831, :840)
- Replace the final assertion of `opening_reads_once_…` (:629-632) with `assert_eq!(view.shown().map(|shown| shown.editor.lines().len()), Some(2));`.
- Delete `lines_are_picked_and_cleared` (:635-654): the picked line is gone, and the caret line replaces it (covered below).

```rust
    use crate::editor::{EditKey, Pos};
    use mkrew_protocol::FsWritten;

    fn file_text_at(path: &str, text: &str) -> Result<(FsContent, Code), String> {
        let content = FsContent {
            path: path.into(),
            content: Some(text.into()),
            size: text.len() as u64,
            binary: false,
            too_large: false,
            language: None,
            hash: Some(format!("#{text}")),
        };
        Ok((content, Code::plain(text)))
    }

    fn file_text(text: &str) -> Result<(FsContent, Code), String> {
        file_text_at("README.md", text)
    }

    /// A view showing README.md as read with `text`.
    fn opened(text: &str) -> FilesView {
        let mut view = listed();
        view.open("README.md");
        view.begin_read();
        view.read("README.md", file_text(text));
        view
    }

    fn text(view: &FilesView) -> String {
        view.shown().map(|shown| shown.editor.text()).unwrap_or_default()
    }

    #[test]
    fn a_reread_follows_the_disk_only_while_nothing_is_unsaved() {
        let mut view = opened("a\nbc\n");
        view.editor_mut().unwrap().set_cursor(Pos::new(1, 2), false);
        assert_eq!(view.line(), Some(2));
        // The same text again (our own save, another file changed): caret and undo stay.
        view.editor_mut().unwrap().insert("x");
        view.editor_mut().unwrap().undo();
        view.begin_read();
        view.read("README.md", file_text("a\nbc\n"));
        assert!(view.editor_mut().unwrap().redo(), "the undo history survived");
        view.editor_mut().unwrap().undo();
        // An agent shortened it: the new text shows, the caret stays where it can.
        view.begin_read();
        view.read("README.md", file_text("a\n"));
        assert_eq!((text(&view), view.line()), ("a\n".to_owned(), Some(1)));
        // Unsaved text is never replaced: not by a change on disk, not by a failed read.
        view.editor_mut().unwrap().insert("mine ");
        for read in [file_text("theirs\n"), Err("gone".into())] {
            view.begin_read();
            view.read("README.md", read);
            assert_eq!(text(&view), "amine \n");
        }
        assert_eq!(view.unsaved(), Some("README.md"));
        // Opening another file is still possible (the shell asks first, Task 6).
        assert_eq!(view.open("src/main.rs").as_deref(), Some("src/main.rs"));
        assert_eq!(view.unsaved(), None);
    }

    #[test]
    fn a_save_marks_what_it_sent_and_one_runs_at_a_time() {
        let mut view = opened("a\n");
        assert_eq!(view.begin_save(), None, "nothing to save");
        view.editor_mut().unwrap().insert("b");
        let save = view.begin_save().unwrap();
        assert_eq!(
            (save.text.as_str(), save.expected_hash.as_str()),
            ("ba\n", "#a\n")
        );
        assert_eq!(view.begin_save(), None, "one save at a time");
        // Typed while it was saving: still unsaved afterwards.
        view.editor_mut().unwrap().key(EditKey::Insert("c".into()), 10);
        let written = FsWritten { saved: true, hash: "#ba\n".into() };
        view.saved("README.md", save.point, Ok(written));
        assert_eq!(view.unsaved(), Some("README.md"));
        let save = view.begin_save().unwrap();
        assert_eq!(save.expected_hash, "#ba\n", "the saved text is the new base");
        let written = FsWritten { saved: true, hash: "#bca\n".into() };
        view.saved("README.md", save.point, Ok(written));
        assert_eq!(view.unsaved(), None);
        // Our own save comes back through the watcher: nothing changes.
        view.begin_read();
        view.read("README.md", file_text("bca\n"));
        assert_eq!(text(&view), "bca\n");
        // A failed save says why and keeps the text.
        view.editor_mut().unwrap().insert("d");
        let save = view.begin_save().unwrap();
        view.saved("README.md", save.point, Err("disk full".into()));
        let error = view.shown().and_then(|shown| shown.save_error.clone());
        assert_eq!(error.as_deref(), Some("Not saved: disk full"));
        assert_eq!(view.unsaved(), Some("README.md"));
    }

    #[test]
    fn a_highlight_is_kept_only_for_the_text_it_was_made_of() {
        let mut view = opened("a\n");
        view.editor_mut().unwrap().insert("b");
        let version = view.shown().unwrap().editor.version();
        let request = view.highlight_request(version).unwrap();
        assert_eq!(request.content.as_deref(), Some("ba\n"));
        view.editor_mut().unwrap().insert("c");
        assert_eq!(view.highlight_request(version), None, "typing went on");
        assert!(!view.highlighted(version, Code::plain("ba\n")), "stale");
        let version = view.shown().unwrap().editor.version();
        assert!(view.highlighted(version, Code::plain("bca\n")));
    }
```

Run: `cargo test -p mkrew-ui --lib files_view::tests -j 4`
Expected: FAIL to compile (`Shown`, `editor_mut`, `begin_save` and the rest are missing).

- [ ] **Step 2: View-model**

In `files_view.rs`:
- Imports: `use crate::editor::Editor;` and `use mkrew_protocol::{Event, FsContent, FsEntry, FsWritten};`.
- Module doc (:1-5): "…and the opened file (`fs.read`, highlighted off the UI thread into a [`Code`]) in an [`Editor`], saved with `fs.write`…".

```rust
/// The opened file.
#[derive(Debug, Clone, PartialEq)]
pub enum Content {
    /// No file opened.
    None,
    Loading,
    Shown(Box<Shown>),
    Failed(String),
}

/// The opened file in the editor (SPEC §12A, ADR 0028).
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    /// The last highlight, with the binary / too-large flags. Its spans are drawn only on
    /// lines whose text it still matches; the text is highlighted again once typing pauses.
    pub code: Rc<Code>,
    pub editor: Editor,
    /// `fs.read`'s hash of the version on disk that the text started from (or was last saved
    /// as). `None` = cannot be saved (binary, too large, or an old daemon).
    pub hash: Option<String>,
    pub language: Option<String>,
    /// Why the last save failed.
    pub save_error: Option<String>,
}

/// An `fs.write` of the opened file (SPEC §7): its text, the hash the file must still have
/// on disk, and the undo state it saves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveRequest {
    pub path: String,
    pub text: String,
    pub expected_hash: String,
    pub point: Option<u64>,
}
```

Changes to `FilesView`:
- Delete the `line` field (:82-83) and its init (:114).
- Add `/// An `fs.write` is in flight. saving: bool,` (init `false`).
- Remove `self.line = None;` from `open` (:287).
- Delete `pick_line` (:356-359).

Replace `read` / `show` / `search` (:323-354) with:

```rust
    /// The `fs.read` answer (with its highlight) for `path`; shown if `path` is still the
    /// opened file. Returns the file to read again (asked for meanwhile).
    pub fn read(&mut self, path: &str, read: Result<(FsContent, Code), String>) -> Option<String> {
        self.reading = false;
        let again = std::mem::take(&mut self.read_again);
        if self.file.as_deref() == Some(path) {
            self.show(read);
        }
        if again { self.file.clone() } else { None }
    }

    fn show(&mut self, read: Result<(FsContent, Code), String>) {
        let (dirty, hash, cursor) = match &self.content {
            Content::Shown(shown) => (
                shown.editor.is_dirty(),
                shown.hash.clone(),
                Some(shown.editor.cursor()),
            ),
            _ => (false, None, None),
        };
        // Unsaved text is never replaced by a re-read (SPEC §2.5).
        if dirty {
            return;
        }
        self.content = match read {
            Err(error) => Content::Failed(error),
            // The same version again (our own save, a change elsewhere): caret and undo stay.
            Ok((content, _)) if hash.is_some() && content.hash == hash => return,
            Ok((content, code)) => {
                let mut editor = Editor::new(content.content.as_deref().unwrap_or(""));
                if let Some(cursor) = cursor {
                    editor.set_cursor(cursor, false);
                }
                Content::Shown(Box::new(Shown {
                    code: Rc::new(code),
                    editor,
                    hash: content.hash,
                    language: content.language,
                    save_error: None,
                }))
            }
        };
        self.search();
    }

    /// Runs the find bar's query over the opened file's text again.
    pub fn search(&mut self) {
        if let (Some(find), Content::Shown(shown)) = (&mut self.find, &self.content) {
            find.search(shown.editor.lines());
        }
    }

    pub fn shown(&self) -> Option<&Shown> {
        match &self.content {
            Content::Shown(shown) => Some(shown),
            _ => None,
        }
    }

    /// The editor of an editable file (not binary or too large).
    pub fn editor_mut(&mut self) -> Option<&mut Editor> {
        match &mut self.content {
            Content::Shown(shown) if !shown.code.binary && !shown.code.too_large => {
                Some(&mut shown.editor)
            }
            _ => None,
        }
    }

    /// The opened file while it has unsaved changes.
    pub fn unsaved(&self) -> Option<&str> {
        self.shown()
            .filter(|shown| shown.editor.is_dirty())
            .and(self.file.as_deref())
    }

    /// The caret's line (1-based): "Open in Zed" and Send selection go there.
    pub fn line(&self) -> Option<u32> {
        let shown = self.shown()?;
        u32::try_from(shown.editor.cursor().line + 1).ok()
    }

    /// Save (Ctrl+S). `None` when nothing is unsaved, the file cannot be saved, or a save is
    /// already in flight.
    pub fn begin_save(&mut self) -> Option<SaveRequest> {
        if self.saving {
            return None;
        }
        let path = self.file.clone()?;
        let Content::Shown(shown) = &mut self.content else {
            return None;
        };
        if !shown.editor.is_dirty() {
            return None;
        }
        let expected_hash = shown.hash.clone()?;
        self.saving = true;
        Some(SaveRequest {
            path,
            text: shown.editor.text(),
            expected_hash,
            point: shown.editor.save_point(),
        })
    }

    /// The `fs.write` answer for `path`, which saved the state `point`.
    pub fn saved(&mut self, path: &str, point: Option<u64>, written: Result<FsWritten, String>) {
        self.saving = false;
        if self.file.as_deref() != Some(path) {
            return;
        }
        let Content::Shown(shown) = &mut self.content else {
            return;
        };
        match written {
            Ok(written) if written.saved => {
                shown.editor.mark_saved(point);
                shown.hash = Some(written.hash);
                shown.save_error = None;
            }
            Ok(_) => {
                shown.save_error =
                    Some("Not saved: the file changed on disk since it was opened.".into());
            }
            Err(error) => shown.save_error = Some(format!("Not saved: {error}")),
        }
    }

    /// The text to highlight again once typing paused at `version`; `None` if it changed
    /// since (a later pause does it).
    pub fn highlight_request(&self, version: u64) -> Option<FsContent> {
        let (Some(path), Some(shown)) = (&self.file, self.shown()) else {
            return None;
        };
        (shown.editor.version() == version).then(|| FsContent {
            path: path.clone(),
            content: Some(shown.editor.text()),
            size: shown.code.size,
            binary: false,
            too_large: false,
            language: shown.language.clone(),
            hash: None,
        })
    }

    /// The highlight of `version`; dropped (`false`) when the text changed meanwhile.
    pub fn highlighted(&mut self, version: u64, code: Code) -> bool {
        match &mut self.content {
            Content::Shown(shown) if shown.editor.version() == version => {
                shown.code = Rc::new(code);
                true
            }
            _ => false,
        }
    }
```

Run: `cargo test -p mkrew-ui --lib files_view::tests -j 4`
Expected: PASS.

- [ ] **Step 3: Keymap `SaveFile`, failing then passing**

In the `keymap.rs` tests, `spec_defaults`, after :675:

```rust
        // Files only: plain Ctrl+S saves the opened file (the terminal never has focus there).
        assert_eq!(bound("ctrl-s"), Some(Command::SaveFile));
        assert_eq!(Command::SaveFile.context(), Some(FILES_CONTEXT));
```

Replace the loop at :749-757 of `agent_keys_are_unbound_and_reach_the_pty` with:

```rust
        // No default is a plain Ctrl+<letter>, except in the Files view (ADR 0028): no
        // terminal has focus there (SPEC §13).
        for (command, keys) in keymap.bindings() {
            let m = keys.modifiers;
            let plain_ctrl = m.control && !m.shift && !m.alt && keys.key.len() == 1;
            let letter = keys.key.chars().all(|c| c.is_ascii_alphabetic());
            assert!(
                !plain_ctrl || !letter || command.context() == Some(FILES_CONTEXT),
                "{command:?}"
            );
        }
```

Run: `cargo test -p mkrew-ui --lib keymap::tests -j 4`
Expected: FAIL to compile (`SaveFile`).

Add the command:
- `Command`, after `ToggleIgnored` (:77): `/// Files only: save the opened file (Ctrl+S, ADR 0028).` `SaveFile,`
- `all()`, after `Command::ToggleIgnored,` (:180): `Command::SaveFile,`
- `name()`: `Command::SaveFile => "save_file".into(),`
- `default_keys()`: `Command::SaveFile => "ctrl-s".into(),`
- `context()` (:337): `Command::ToggleIgnored | Command::SaveFile => Some(FILES_CONTEXT),`
- Module doc (:8-9): "…so the defaults avoid plain `Ctrl+<letter>` (Ctrl+C, Ctrl+R… reach the agent), except Files-only commands (ADR 0028)."

`palette.rs`:
- `title`, after :207: `Command::SaveFile => "Files: Save file".into(),`
- `available` (:270): `Command::ToggleIgnored | Command::SaveFile => in_view(View::Files),`
- Test `availability_follows_the_view`: add `Command::SaveFile,` to the "not in terminals" list (:711-721), and after :757 add `assert!(commands(&context(View::Files)).contains(&Command::SaveFile));`

Run: `cargo test -p mkrew-ui --lib keymap::tests -j 4` then `cargo test -p mkrew-ui --lib palette::tests -j 4`
Expected: PASS.

- [ ] **Step 4: `preview::code_row` replaces `code_view`**

In `theme.rs`, after `FIND_INPUT_WIDTH` (:129):

```rust
/// Width of the Files editor's caret.
pub const CARET_WIDTH: f32 = 2.0;

/// The Files editor highlights its text again this long after the last keystroke.
pub const REHIGHLIGHT_DELAY: Duration = Duration::from_millis(150);
```

In `preview.rs`, delete `code_view` (:479-540) and add the code below. Add these imports if they are missing: `std::cell::Cell`, `gpui::{canvas, CursorStyle, Div, Pixels}`.

```rust
/// What one row of the Files editor shows besides its text. Caret and selection are in screen
/// cells (tabs count as `TAB`); `found` is in bytes of the drawn text.
#[derive(Debug, Default)]
pub struct RowMarks {
    pub caret: Option<usize>,
    pub selected: Option<Range<usize>>,
    pub found: Vec<(Range<usize>, bool)>,
}

/// One row of the Files editor (SPEC §12A): its number, then `text` (tabs already spaces) in
/// the syntax colors of `spans`, with the find matches, over the selected cells, with the
/// caret. The caret's row is raised and its number accented. `text_left` learns where the text
/// starts on screen, so a click can be turned into a column.
#[allow(clippy::too_many_arguments)]
pub fn code_row(
    index: usize,
    total: usize,
    text: String,
    spans: &[(Range<usize>, Token)],
    marks: &RowMarks,
    cell_width: Pixels,
    text_left: &Rc<Cell<Pixels>>,
    theme: &Theme,
) -> Div {
    let line_height = theme.terminal_font_size * theme.terminal_line_height;
    let cells = move |n: usize| cell_width * n as f32;
    let current = marks.caret.is_some();
    let text_left = Rc::clone(text_left);
    div()
        .w_full()
        .h(px(line_height))
        .flex()
        .gap(px(space::L))
        .whitespace_nowrap()
        .cursor(CursorStyle::IBeam)
        // A raised band plus an accent number: syntax colors and muted comments stay
        // ≥ 4.5:1 on it.
        .when(current, |el| el.bg(rgb(theme.surface_raised)))
        .child(
            div()
                .flex_none()
                .text_color(rgb(if current { theme.accent } else { theme.text_muted }))
                .child(gutter(index + 1, total)),
        )
        .child(
            div()
                .relative()
                .flex_1()
                .text_color(rgb(theme.text))
                .child(
                    canvas(move |bounds, _, _| text_left.set(bounds.left()), |_, (), _, _| {})
                        .absolute()
                        .size_full(),
                )
                .children(marks.selected.clone().map(|selected| {
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(cells(selected.start))
                        .w(cells(selected.end - selected.start))
                        .bg(rgb(theme.terminal_selection_bg))
                }))
                .child(code_line(text, spans, &marks.found, theme))
                .children(marks.caret.map(|at| {
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(cells(at))
                        .w(px(theme::CARET_WIDTH))
                        .bg(rgb(theme.text))
                })),
        )
}
```

Update the doc comments of `paint` and `code_line` if they mention `code_view`. In `terminal/view.rs:844`, make `cell_size` a `pub fn`.

- [ ] **Step 5: Shell — read, render, keys, mouse, save**

Imports:
- gpui (main.rs :15-19): add `ClipboardItem`, `ListHorizontalSizingBehavior`, `MouseDownEvent`, `MouseMoveEvent` and `Pixels` if missing.
- Add `use mkrew_ui::editor::{self, EditKey, Pos};`.
- Add `FsWriteParams, FsWritten` to the `mkrew_protocol` import (:28).

Fields after `file_code_scroll` (:306), and their init (:637):

```rust
    /// The Files editor (ADR 0028): typing goes there while it has focus.
    file_editor_focus: FocusHandle,
    /// Where the editor's text starts on screen (set while painting), for clicks.
    file_text_left: Rc<Cell<Pixels>>,
```

```rust
            file_editor_focus: cx.focus_handle(),
            file_text_left: Rc::default(),
```

`read_file` (:1699-1717): keep the `FsContent`:

```rust
        cx.spawn_in(window, async move |this, cx| {
            let read = match read.await {
                Ok(content) => Ok(background
                    .spawn(async move {
                        let code = Code::new(&content);
                        (content, code)
                    })
                    .await),
                Err(error) => Err(format!("{error:#}")),
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                if let Err(error) = &read {
                    tracing::warn!(%project_id, path, "fs.read failed: {error}");
                }
                let again = shell
                    .files_at(project_id, target)
                    .and_then(|view| view.read(&path, read));
                if let Some(path) = again {
                    shell.read_file(project_id, path, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
```

New methods (after `close_file_find`, :1870):

```rust
    /// Ctrl+S / "Save" (§12A): `fs.write` of the opened file over the version it was read as.
    fn save_file(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(view) = self.files.get_mut(&project_id) else {
            return;
        };
        let target = view.worktree_task_id;
        let Some(save) = view.begin_save() else {
            return;
        };
        let written = connection.call::<FsWritten>(Request::FsWrite(FsWriteParams {
            project_id,
            worktree_task_id: target,
            path: save.path.clone(),
            content: save.text,
            expected_hash: save.expected_hash,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let written = written.await.map_err(|error| format!("{error:#}"));
            let _ = this.update_in(cx, |shell, _, cx| {
                if let Err(error) = &written {
                    tracing::warn!(%project_id, path = %save.path, "fs.write failed: {error}");
                }
                if let Some(view) = shell.files_at(project_id, target) {
                    view.saved(&save.path, save.point, written);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Keys of the Files editor (§12A, VS Code style; not in the keymap, ADR 0028). Keys the
    /// editor does not take (window shortcuts) bubble on to the keymap.
    fn file_editor_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let (Some(key), Some(project_id)) = (editor::edit_key(&event.keystroke), self.workspace)
        else {
            return;
        };
        cx.stop_propagation();
        let page = self.file_page_lines(cx);
        let pasted = (key == EditKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(editor) = self.files_view(project_id).editor_mut() else {
            return;
        };
        let version = editor.version();
        let copied = match key {
            EditKey::Copy => editor.selected_text(),
            EditKey::Cut => editor.cut(),
            EditKey::Paste => {
                if let Some(text) = pasted {
                    editor.insert(&text);
                }
                None
            }
            key => {
                editor.key(key, page);
                None
            }
        };
        let (line, edited) = (editor.cursor().line, editor.version() != version);
        if let Some(text) = copied {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.file_code_scroll
            .scroll_to_item(line, ScrollStrategy::Nearest);
        if edited {
            self.file_edited(project_id, window, cx);
        }
        cx.notify();
    }

    /// A click (or a drag, `extend`) at window x `x` on line `line` of the editor.
    fn click_file_text(
        &mut self,
        project_id: Ulid,
        line: usize,
        x: Pixels,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.file_editor_focus, cx);
        let cell_width = mkrew_ui::terminal::view::cell_size(window, cx.theme()).0;
        let cells = ((x - self.file_text_left.get()) / cell_width).round().max(0.0) as usize;
        let Some(editor) = self.files_view(project_id).editor_mut() else {
            return;
        };
        let Some(text) = editor.lines().get(line) else {
            return;
        };
        let col = editor::byte_for_display_col(text, cells);
        editor.set_cursor(Pos { line, col }, extend);
        cx.notify();
    }

    /// After an edit, find follows the new text, and the highlight follows once typing pauses.
    fn file_edited(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let view = self.files_view(project_id);
        view.search();
        let Some(version) = view.shown().map(|shown| shown.editor.version()) else {
            return;
        };
        let target = view.worktree_task_id;
        let background = cx.background_executor().clone();
        cx.spawn_in(window, async move |this, cx| {
            background.timer(theme::REHIGHLIGHT_DELAY).await;
            let Ok(Some(content)) = this.update(cx, |shell, _| {
                shell
                    .files_at(project_id, target)
                    .and_then(|view| view.highlight_request(version))
            }) else {
                return;
            };
            let code = background.spawn(async move { Code::new(&content) }).await;
            let _ = this.update(cx, |shell, cx| {
                let view = shell.files_at(project_id, target);
                if view.is_some_and(|view| view.highlighted(version, code)) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Lines the editor shows at once (PageUp / PageDown).
    fn file_page_lines(&self, cx: &App) -> usize {
        let theme = cx.theme();
        let row = px(theme.terminal_font_size * theme.terminal_line_height);
        let shown = self.file_code_scroll.0.borrow().last_item_size;
        shown.map_or(1, |size| (size.item.height / row).floor() as usize).max(1)
    }
```

`reveal_file_match` (:1845-1859): select the match instead of picking its line:

```rust
    /// Scrolls to the current match and selects it in the editor.
    fn reveal_file_match(&mut self, project_id: Ulid, cx: &mut Context<'_, Self>) {
        let view = self.files_view(project_id);
        let found = view.find.as_ref().and_then(TextFind::current).cloned();
        if let (Some(found), Some(editor)) = (found, view.editor_mut()) {
            let (start, end) = (found.range.start, found.range.end);
            editor.select(Pos::new(found.line, start)..Pos::new(found.line, end));
            self.file_code_scroll
                .scroll_to_item(found.line, ScrollStrategy::Center);
        }
        cx.notify();
    }
```

`open_file_in_zed` :1907: `let (Some(path), line) = (view.file.clone(), view.line()) else {`.

`picked_lines` :1795-1804:

```rust
                let files = self.files.get(&project_id)?;
                let (Some(path), Some(line), Some(shown)) =
                    (&files.file, files.line(), files.shown())
                else {
                    return None;
                };
                let text = shown
                    .editor
                    .lines()
                    .get(usize::try_from(line).ok()?.checked_sub(1)?)?;
                Some((path.clone(), line, line, text.clone()))
```

`run`:
- After `ToggleIgnored` (:6335-6341), add:

```rust
            Command::SaveFile => {
                // Only bound while Files are shown.
                if let Some(project_id) = self.workspace {
                    self.save_file(project_id, window, cx);
                }
            }
```

- In the `file_find_focus` branch (:6141), let Save act while the find field has the keyboard: change `if open {` to `if open && *command != Command::SaveFile {`.

`render_file` (:8957-9154):
- `zed_tip`: `match view.line()`.
- Before the header add `let dirty = view.unsaved().is_some();`. The path child (:8991) becomes `.child(format!("{}{path}", if dirty { "● " } else { "" }))`. The `●` and the primary Save button together show the state, so status is never color alone.
- Replace `.children(view.line.map(…))` (:8993-8999) with the caret position:

```rust
            .children(view.shown().map(|shown| {
                let at = shown.editor.cursor();
                let col = editor::display_col(&shown.editor.lines()[at.line], at.col);
                div()
                    .flex_none()
                    .text_size(font_size::META)
                    .text_color(rgb(theme.text_muted))
                    .child(format!("Ln {}, Col {}", at.line + 1, col + 1))
            }))
```

- Before the Find button (:9011), add:

```rust
            .when(
                view.shown()
                    .is_some_and(|shown| !shown.code.binary && !shown.code.too_large),
                |el| {
                    el.child(
                        button("file-save", "Save", theme, dirty)
                            .flex_none()
                            .tooltip(tooltip(
                                theme,
                                "Save this file",
                                self.keymap.label(Command::SaveFile),
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.save_file(project_id, window, cx);
                            })),
                    )
                },
            )
```

- Body:
  - The binary arm becomes `Content::Shown(shown) if shown.code.binary || shown.code.too_large`, using `shown.code.size`.
  - Delete the "This file is empty." arm (:9118-9122): an empty file is editable.
  - The last arm becomes `Content::Shown(shown) => self.render_editor(project_id, shown, window, theme, cx),`.
- Under the `zed_error` notice (:9150-9154), add the same notice for `view.shown().and_then(|shown| shown.save_error.clone())`.

`render_editor` (new, after `render_file`):

```rust
    /// The Files editor (§12A, ADR 0028): the lines with the caret, selection, syntax colors
    /// and find matches. A click places the caret; Shift+click and drag select.
    fn render_editor(
        &self,
        project_id: Ulid,
        shown: &files_view::Shown,
        window: &Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let focused = self.file_editor_focus.is_focused(window);
        let cell_width = mkrew_ui::terminal::view::cell_size(window, theme).0;
        let lines = shown.editor.lines();
        let total = lines.len();
        let longest = (lines.iter().enumerate())
            .max_by_key(|(_, line)| line.len())
            .map(|(index, _)| index);
        let text_left = Rc::clone(&self.file_text_left);
        let row_theme = theme.clone();
        let list = uniform_list(
            "file-code",
            total,
            cx.processor(move |shell, range: std::ops::Range<usize>, _, cx| {
                let Some(view) = shell.files.get(&project_id) else {
                    return Vec::new();
                };
                let Some(shown) = view.shown() else {
                    return Vec::new();
                };
                let (editor, code) = (&shown.editor, &shown.code);
                let hits = view.find.as_ref().map(TextFind::hits);
                let (cursor, selection) = (editor.cursor(), editor.selection());
                range
                    .filter_map(|index| {
                        let line = editor.lines().get(index)?;
                        let text = editor::display_line(line);
                        // Spans of an older text could point past the line: only when it matches.
                        let fresh = code.lines.get(index).is_some_and(|old| *old == *text);
                        let spans = (code.spans.get(index))
                            .filter(|_| fresh)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]);
                        let found = (hits.as_ref())
                            .map(|hits| hits.on_line(index))
                            .unwrap_or_default()
                            .into_iter()
                            .map(|(range, current)| {
                                let start = editor::display_byte(line, range.start);
                                (start..editor::display_byte(line, range.end), current)
                            })
                            .collect();
                        let marks = preview::RowMarks {
                            caret: (focused && cursor.line == index)
                                .then(|| editor::display_col(line, cursor.col)),
                            selected: (selection.as_ref())
                                .and_then(|selection| editor::selected_cells(line, index, selection)),
                            found,
                        };
                        let row = preview::code_row(
                            index,
                            total,
                            text.into_owned(),
                            spans,
                            &marks,
                            cell_width,
                            &text_left,
                            &row_theme,
                        );
                        Some(
                            row.on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                                    let x = event.position.x;
                                    let extend = event.modifiers.shift;
                                    shell.click_file_text(project_id, index, x, extend, window, cx);
                                }),
                            )
                            .on_mouse_move(cx.listener(
                                move |shell, event: &MouseMoveEvent, window, cx| {
                                    let dragging = event.pressed_button == Some(MouseButton::Left)
                                        && shell.file_editor_focus.is_focused(window);
                                    if dragging {
                                        let x = event.position.x;
                                        shell.click_file_text(project_id, index, x, true, window, cx);
                                    }
                                },
                            ))
                            .into_any_element(),
                        )
                    })
                    .collect()
            }),
        )
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(longest)
        .track_scroll(&self.file_code_scroll)
        .flex_1()
        .min_h_0()
        .px(px(space::L))
        .pb(px(space::L))
        .font_family(theme.terminal_font_family.clone())
        .text_size(px(theme.terminal_font_size));
        div()
            .id("file-editor")
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .track_focus(&self.file_editor_focus)
            .on_key_down(cx.listener(Self::file_editor_key_down))
            // Below the last line too: a click there gives the editor the keyboard.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _: &MouseDownEvent, window, cx| {
                    window.focus(&shell.file_editor_focus, cx);
                    cx.notify();
                }),
            )
            .child(self.typing(&self.file_editor_focus, Self::file_editor_key_down, cx))
            .child(list)
            .into_any_element()
    }
```

`on_mouse_move` is hover-gated (div.rs:309), so dragging stops extending once the mouse leaves the rows. That is acceptable (ponytail). The caret is kept in view only vertically (`scroll_to_item`); horizontal scrolling does not follow the caret (see Open questions).

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean. Re-grep `code_view|pick_line|view\.line\b|\.line =` in `crates/mkrew-ui/src`: no hits left.

- [ ] **Step 6: SPEC, HUMAN_CHECKS**

SPEC §12A:
- Replace the viewer bullet (:800-801) with:

```
- **Editor** (right; ADR 0028): the opened file with line numbers and syntax highlighting via
  `syntect` (lazy-loaded syntax set; re-highlighted 150 ms after typing stops). A plain editor:
  click places the caret, Shift+click / drag select; arrows (Ctrl = by word), Home/End
  (Ctrl = file), PgUp/PgDn, Backspace/Delete (Ctrl = word), Enter keeps the indent, Tab, Ctrl+A,
  Ctrl+Z / Ctrl+Y undo/redo, Ctrl+C/X/V. Line endings and the final newline are kept. Unsaved
  changes show `●` before the path; Ctrl+S or "Save" writes the file (`fs.write`). Files > 2 MB
  or binary: a read-only notice and "Open in Zed".
```

- Replace :806 with `- "Open in Zed" opens the file at the caret's line.`

SPEC §13:
- In the intro (:865-867), after "like Windows Terminal." add: "Files-only commands may use plain `Ctrl+<letter>` (no terminal has focus there, ADR 0028); the editor's own keys (arrows, Ctrl+Z/Y/A/C/X/V) are not in the keymap."
- After the table row :904 add `| Files | Save the opened file | Ctrl+S |`.

`docs/HUMAN_CHECKS.md`, in the 2026-10-09 section (`### To verify — 2026-10-09 inbox: Shift+Enter, F11, Files editor, several repos …`), add:

```
- [ ] **Files editor:** restart the daemon (protocol v27). Open a `.rs` file:
  - A click puts the caret there (I-beam pointer).
  - Arrows, Ctrl+arrows, Home (indent, then line start), End, PgUp and PgDn move the caret;
    Shift extends; dragging selects; Ctrl+A selects all.
  - Type Vietnamese in the middle of a line, once with Unikey and once with Windows Telex.
  - Ctrl+C / X / V work with Zed.
  - Enter keeps the indent. Tab in a Makefile inserts a tab.
  - Ctrl+Z undoes a typed word at once; Ctrl+Y redoes; Ctrl+Shift+Z still opens Zed.
  - While there are changes, `●` shows before the path and Save turns primary.
  - Ctrl+S saves, also from the find field: `●` goes away, `git diff` shows only your change,
    and a CRLF file stays CRLF (no `^M` churn in `git diff`).
  - Colors come back ~150 ms after you stop typing.
  - A long line scrolls sideways.
  - In light and dark: the caret, the selection and the raised caret row are readable.
```

Run the full verification.
Suggested message: `feat(ui): edit the opened file in the Files view (Ctrl+S saves)`

---

### Task 6: Never lose unsaved edits — changes on disk, leaving, closing

Inbox: line 29. Spec: §2.5 (:98), §12A. ADR 0028.

**Current state (investigated):**
- After Task 5, `FilesView::show` ignores a re-read while the editor is dirty, and a stale save only sets `save_error`. The user cannot see the disk version or overwrite it.
- Ways to leave the opened file:
  - `click_file` (main.rs:1733-1743) and `files_key_down` Enter (:1746-1768), through `FilesView::click` → `open`
  - quick open (`open_quick_pick` :3387-3404, `view.reveal`)
  - Retry (`render_file` :9093-9099); it only shows for `Content::Failed`, so there is nothing unsaved
  - `set_files_target` (:1596-1610), which replaces the whole `FilesView`
  - `drop_gone_targets` (:1923-1949), which replaces the view when a worktree is gone
  - closing the window

  Workspace and view switches keep every `FilesView` in `self.files`, so they need no prompt.
- Native confirm pattern: `window.prompt(PromptLevel::Warning, msg, Some(detail), &ANSWERS, cx)` → `cx.spawn_in(window, async move |this, cx| { answer.await.ok() … })` (main.rs:3973-4002, `workspace_menu::REMOVE_ANSWERS`). A `PromptButton` from the label "Cancel" becomes `IDCANCEL` on Windows, so Esc picks it (gpui-pre platform.rs:2552-2556, gpui-pre-windows window.rs:718-806).
- Window close:
  - `Window::on_window_should_close(&self, cx: &App, f: impl Fn(&mut Window, &mut App) -> bool + 'static)` (gpui-pre window.rs:6658); returning `false` keeps the window.
  - `Window::remove_window(&mut self)` (:2280).
  - `main.rs:160` quits the app once no window is left.
  - `Shell::new` (:378-403) registers window observers. Use `cx.weak_entity()` there (context.rs:58): `cx.entity()` (:51) panics during `cx.new` if it cannot upgrade.

**Files:**
- Modify: `crates/mkrew-ui/src/files_view.rs` (`Shown.disk_changed`, `show`, `saved`; new `discard`, `reload`, `keep_mine`, `leaves_unsaved`, `LEAVE_ANSWERS`, `Leave`, `leave_choice`; tests)
- Modify: `crates/mkrew-ui/src/main.rs`:
  - `Shell::new` (:402)
  - `save_file` (Task 5) gains `then`
  - `click_file` :1733, `files_key_down` :1756, `open_quick_pick` :3393-3404 (→ new `reveal_file`), `set_files_target` :1603
  - new `leave_file`, `window_should_close`
  - the disk banner in `render_file`
- Modify: `docs/SPEC.md` §12A, `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces in `files_view`:
  - `Shown.disk_changed: Option<String>` (the disk's hash)
  - `FilesView::discard()`, `reload() -> Option<String>`, `keep_mine()`, `leaves_unsaved(&self, path: &str) -> bool`
  - `pub const LEAVE_ANSWERS: [&str; 3]`, `pub enum Leave { Save, DontSave }`, `pub fn leave_choice(Option<usize>) -> Option<Leave>`
- Produces in `main.rs`:
  - `type AfterSave = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>)>`
  - `Shell::save_file(&mut self, project_id: Ulid, then: Option<AfterSave>, window, cx)`
  - `Shell::leave_file(&mut self, project_id: Ulid, then: AfterSave, window, cx)`

- [ ] **Step 1: Failing tests**

In the `files_view.rs` tests:

```rust
    #[test]
    fn a_change_on_disk_under_unsaved_text_offers_reload_or_keep_mine() {
        let mut view = opened("one\n");
        view.editor_mut().unwrap().insert("x");
        let reread = |view: &mut FilesView, text: &str| {
            view.begin_read();
            view.read("README.md", file_text(text));
        };
        let disk = |view: &FilesView| view.shown().and_then(|shown| shown.disk_changed.clone());
        reread(&mut view, "one\n");
        assert_eq!(disk(&view), None, "the same version: nothing to say");
        reread(&mut view, "two\n");
        assert_eq!(
            (text(&view), disk(&view)),
            ("xone\n".to_owned(), Some("#two\n".to_owned()))
        );
        reread(&mut view, "one\n");
        assert_eq!(disk(&view), None, "the agent undid its change");
        // Keep mine: Save overwrites the disk's version.
        reread(&mut view, "two\n");
        view.keep_mine();
        assert_eq!(disk(&view), None);
        let save = view.begin_save().unwrap();
        assert_eq!(save.expected_hash, "#two\n");
        // The save meets yet another version: the banner again, the text stays.
        let written = FsWritten { saved: false, hash: "#three\n".into() };
        view.saved("README.md", save.point, Ok(written));
        assert_eq!(
            (text(&view), disk(&view)),
            ("xone\n".to_owned(), Some("#three\n".to_owned()))
        );
        // Reload: the disk's text replaces it.
        assert_eq!(view.reload().as_deref(), Some("README.md"));
        reread(&mut view, "three\n");
        assert_eq!(
            (text(&view), view.unsaved(), disk(&view)),
            ("three\n".to_owned(), None, None)
        );
    }

    #[test]
    fn only_leaving_unsaved_text_for_another_file_asks() {
        let mut view = opened("a\n");
        assert!(!view.leaves_unsaved("src/main.rs"), "nothing unsaved");
        view.editor_mut().unwrap().insert("b");
        assert!(view.leaves_unsaved("src/main.rs"), "quick open: not a listed row");
        assert!(!view.leaves_unsaved("README.md"), "the same file");
        assert!(!view.leaves_unsaved("src"), "a folder only opens");
        view.discard();
        assert!(!view.leaves_unsaved("src/main.rs"), "Don't save");
        assert_eq!(leave_choice(Some(0)), Some(Leave::Save));
        assert_eq!(leave_choice(Some(1)), Some(Leave::DontSave));
        assert_eq!(leave_choice(Some(2)), None, "Cancel");
        assert_eq!(leave_choice(None), None, "Esc / dialog closed");
    }
```

Run: `cargo test -p mkrew-ui --lib files_view::tests -j 4`
Expected: FAIL to compile (`disk_changed`, `keep_mine`, `leaves_unsaved` and the rest are missing).

- [ ] **Step 2: View-model**

- Add to `Shown`: `/// The file on disk changed under unsaved text: its hash ("Reload · Keep mine"). pub disk_changed: Option<String>,`. Initialize it to `None` in `show`.
- In `show`, replace `if dirty { return; }` with:

```rust
        // Unsaved text is never replaced by a re-read (SPEC §2.5). Another version on disk is
        // offered as "Reload · Keep mine"; the version the text started from clears that.
        if dirty {
            if let (Ok((content, _)), Content::Shown(shown)) = (&read, &mut self.content) {
                shown.disk_changed = (content.hash != hash)
                    .then(|| content.hash.clone().unwrap_or_default());
            }
            return;
        }
```

- In `saved`, replace the `Ok(_)` arm with `Ok(written) => shown.disk_changed = Some(written.hash),`.
- New methods:

```rust
    /// "Don't save" / "Reload": drops the unsaved changes; the next read replaces the text.
    pub fn discard(&mut self) {
        if let Content::Shown(shown) = &mut self.content {
            let point = shown.editor.save_point();
            shown.editor.mark_saved(point);
            shown.disk_changed = None;
            // No version to keep: whatever is read next is shown.
            shown.hash = None;
        }
    }

    /// "Reload": drops the unsaved changes; returns the file to read again.
    pub fn reload(&mut self) -> Option<String> {
        self.discard();
        self.file.clone()
    }

    /// "Keep mine": the text stays and Save overwrites the version on disk.
    pub fn keep_mine(&mut self) {
        if let Content::Shown(shown) = &mut self.content
            && let Some(disk) = shown.disk_changed.take()
        {
            shown.hash = Some(disk);
        }
    }

    /// Whether opening `path` (a click, Enter, quick open) leaves the unsaved changes of
    /// another file.
    pub fn leaves_unsaved(&self, path: &str) -> bool {
        self.unsaved().is_some_and(|file| file != path)
            && self.entry(path).is_none_or(|entry| !entry.is_dir)
    }
```

After the `FilesView` impl:

```rust
/// Answers of "Save changes to …?", in `window.prompt` order ("Cancel" is also Esc).
pub const LEAVE_ANSWERS: [&str; 3] = ["Save", "Don't save", "Cancel"];

/// What to do with unsaved changes before leaving them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    Save,
    DontSave,
}

/// The choice for an answer (an index into [`LEAVE_ANSWERS`]; `None` = the dialog went
/// away). Returns `None` to stay.
pub fn leave_choice(answer: Option<usize>) -> Option<Leave> {
    match answer {
        Some(0) => Some(Leave::Save),
        Some(1) => Some(Leave::DontSave),
        _ => None,
    }
}
```

Run: `cargo test -p mkrew-ui --lib files_view::tests -j 4`
Expected: PASS.

- [ ] **Step 3: Shell — ask before leaving**

Next to `save_file`:

```rust
/// What runs after unsaved Files changes were saved or dropped.
type AfterSave = Box<dyn FnOnce(&mut Shell, &mut Window, &mut Context<'_, Shell>)>;
```

`save_file` gains `then: Option<AfterSave>`. Its callback uses `this.update_in(cx, |shell, window, cx| { … })`, and runs `then` only after a successful save:

```rust
                let done = matches!(&written, Ok(written) if written.saved);
                if let Some(view) = shell.files_at(project_id, target) {
                    view.saved(&save.path, save.point, written);
                }
                if let (true, Some(then)) = (done, then) {
                    then(shell, window, cx);
                }
                cx.notify();
```

`Command::SaveFile` and the Save button pass `None`. If `begin_save` returns `None` while a `then` is given (a save already in flight), `then` is dropped and the user simply asks again.

```rust
    /// Runs `then` once the unsaved changes of `project_id`'s opened file are dealt with:
    /// - at once if there are none;
    /// - after "Save" and a successful save;
    /// - after "Don't save".
    ///
    /// "Cancel" keeps everything (§2.5).
    fn leave_file(
        &mut self,
        project_id: Ulid,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(path) = (self.files.get(&project_id))
            .and_then(FilesView::unsaved)
            .map(str::to_owned)
        else {
            return then(self, window, cx);
        };
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        let detail = format!("{path} — your changes are lost if you don't save them.");
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to {name}?"),
            Some(detail.as_str()),
            &files_view::LEAVE_ANSWERS,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let choice = files_view::leave_choice(answer.await.ok());
            let _ = this.update_in(cx, |shell, window, cx| match choice {
                Some(files_view::Leave::Save) => shell.save_file(project_id, Some(then), window, cx),
                Some(files_view::Leave::DontSave) => {
                    shell.files_view(project_id).discard();
                    then(shell, window, cx);
                }
                None => {}
            });
        })
        .detach();
    }

    /// The window's close button (§2.5): unsaved Files changes ask first, one file at a time,
    /// and the window closes once none are left.
    fn window_should_close(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) -> bool {
        let unsaved = (self.files.values())
            .find(|view| view.unsaved().is_some())
            .map(|view| view.project_id);
        let Some(project_id) = unsaved else {
            return true;
        };
        let close: AfterSave = Box::new(|shell, window, cx| {
            if shell.window_should_close(window, cx) {
                window.remove_window();
            }
        });
        self.leave_file(project_id, close, window, cx);
        false
    }
```

In `Shell::new`, after `observe_window_activation` (:402-403):

```rust
        // Unsaved Files changes (§2.5): the close button asks first.
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            this.update(cx, |shell, cx| shell.window_should_close(window, cx))
                .unwrap_or(true)
        });
```

Callers:
- **`click_file`** (:1740-1742):

```rust
        window.focus(&self.files_focus, cx);
        if self.files_view(project_id).leaves_unsaved(path) {
            let path = path.to_owned();
            return self.leave_file(
                project_id,
                Box::new(move |shell, window, cx| shell.click_file(project_id, &path, window, cx)),
                window,
                cx,
            );
        }
        let requests = self.files_view(project_id).click(path);
        self.files_requests(project_id, requests, window, cx);
```

- **`files_key_down`** (:1756): Enter goes through `click_file`. This is the same action as `FilesView::key`'s `Enter => self.click(…)`, and `click_file` also refocuses the tree. Right after `cx.stop_propagation();` add:

```rust
        if key == files_view::Key::Enter
            && let Some(cursor) = self.files_view(project_id).cursor.clone()
        {
            return self.click_file(project_id, &cursor, window, cx);
        }
```

- **`open_quick_pick`** (:3393-3404): move these lines into a new method `fn reveal_file(&mut self, project_id: Ulid, target: Option<Ulid>, path: String, window: &mut Window, cx: &mut Context<'_, Self>)`, whose body starts at `let Some(view) = self.files_at(project_id, target) else { return; };`. In `open_quick_pick` replace them with:

```rust
        let target = open.worktree_task_id;
        let leaves = (self.files_at(project_id, target)).is_some_and(|view| view.leaves_unsaved(&path));
        if leaves {
            let reveal: AfterSave = Box::new(move |shell, window, cx| {
                shell.reveal_file(project_id, target, path, window, cx);
            });
            return self.leave_file(project_id, reveal, window, cx);
        }
        self.reveal_file(project_id, target, path, window, cx);
```

- **`set_files_target`** (:1603-1605): after the same-target early return, add:

```rust
        if self.files.get(&project_id).and_then(FilesView::unsaved).is_some() {
            return self.leave_file(
                project_id,
                Box::new(move |shell, window, cx| {
                    shell.set_files_target(project_id, target, window, cx);
                }),
                window,
                cx,
            );
        }
```

  The second call goes through, because `discard` or a successful save leaves nothing unsaved.
- **`drop_gone_targets`** and **Retry**: unchanged. The worktree folder is gone, so nothing can be saved there; and Retry only shows when there is no editor.

Disk banner in `render_file`, built before the `panel` chain and added with `.children(disk_banner)` right after `.children(find_bar)` (:9149):

```rust
        let disk_banner = view
            .shown()
            .filter(|shown| shown.disk_changed.is_some())
            .map(|_| {
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::L))
                    .pb(px(space::S))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(font_size::META)
                            .text_color(rgb(theme.text))
                            .child("⚠ This file changed on disk while you were editing it."),
                    )
                    .child(
                        button("file-reload", "Reload", theme, false)
                            .tooltip(tooltip(
                                theme,
                                "Show the file on disk; your changes are dropped",
                                "",
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                let read = shell.files_view(project_id).reload();
                                let requests = Requests { list: vec![], read };
                                shell.files_requests(project_id, requests, window, cx);
                            })),
                    )
                    .child(
                        button("file-keep-mine", "Keep mine", theme, true)
                            .tooltip(tooltip(
                                theme,
                                "Keep your text; Save overwrites the file on disk",
                                "",
                            ))
                            .on_click(cx.listener(move |shell, _, _, cx| {
                                shell.files_view(project_id).keep_mine();
                                cx.notify();
                            })),
                    )
            });
```

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean.

- [ ] **Step 4: SPEC, HUMAN_CHECKS, inbox**

SPEC §12A, after the Editor bullet:

```
- Unsaved text is never replaced (§2.5). If the file changes on disk meanwhile (an agent), or
  Save finds another version on disk, a banner offers "Reload" (drop your changes) or "Keep
  mine" (Save overwrites it). Opening another file, switching to another worktree or closing
  the window with unsaved changes asks "Save / Don't save / Cancel". Switching workspace or view
  keeps them.
```

HUMAN_CHECKS, 2026-10-09 section:

```
- [ ] **Unsaved edits are kept:**
  - Edit a file, then let an agent (or Zed) change it. Your text stays and "⚠ This file changed
    on disk…" shows. Reload shows the agent's text. Keep mine, then Ctrl+S, writes yours.
  - Edit, then click another file, use quick open (Ctrl+Shift+O), or pick a task worktree.
    "Save changes to <name>?" appears:
    - Cancel (and Esc) stays.
    - Don't save opens the other file.
    - Save saves, then opens it.
  - Edit, switch workspace and come back: the edit is still unsaved.
  - Edit and close the window: the same question. Save and Don't save close it; Cancel keeps it
    open.
```

Run the full verification. Line 29 of `docs/INBOX.md` (`[ ]: Files Module: I expected I can edit information there not just read`) changes in two stages:
- Now: mark it `- [~] … → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 3–6`.
- Once the Files editor and the unsaved-edits checks above pass: mark it `[x]`.

Suggested message: `feat(ui): keep unsaved edits across disk changes, file switches and window close`

---

### Task 7: Find & Replace in the Files editor (VS Code keys)

Inbox: line 28 `Files module: when viewing a file need to have a short cut for find and replace content: can follow to vscode style if needed, and also need short cut for find, replate replace all action`. Spec: §12A (:807-809), §13 (:896, :903-904).

**Current state (investigated):**
- `TextFind { query, hits }` (`text_find.rs:36-40`):
  - `search` (:64-85) keeps the place at the current match
  - `step` is at :88
  - `find_in_line` (:101) is case-insensitive and UTF-8 safe
  - there are 3 tests (:143-200)
- Shell (main.rs):
  - `open_file_find` (:1772-1783) opens or focuses the bar
  - `file_find_key_down` (:1787-1821) uses `find_key` (Esc, Enter, Shift+Enter, Backspace, Insert; `terminal/find.rs:22`) and `dialog_key` for Ctrl+V
  - `edit_file_find` is at :1823 and `step_file_find` at :1838
  - `close_file_find` (:1861) gives the focus back to the tree
  - the bar is rendered at :9022-9080
- `Command::Find` (`ctrl-shift-f`, `FIND_CONTEXT`) calls `open_file_find` in Files (:6382-6383). While `file_find_focus` is focused, `run` propagates bound commands (:6135-6146). So Ctrl+F, Ctrl+H and Ctrl+S would not act there without an exception.
- Keys:
  - Ctrl+F and Ctrl+H are free.
  - Ctrl+Shift+H is ToggleIgnored and does not change.
  - Ctrl+Shift+1 (VS Code's replace one) is `ShowBoard` (`keymap.rs:302`), and duplicate detection ignores contexts, so it is not used.
  - Ctrl+Alt+Enter is free. It reaches the field's `on_key_down` because no binding matches it.
- After Task 5, find runs over `editor.lines()` (raw bytes), `render_editor` converts match ranges with `editor::display_byte`, and `reveal_file_match` selects the current match.

**Files:**
- Modify: `crates/mkrew-ui/src/text_find.rs` (`replace` field, `search_from`; doc :1-3; tests)
- Modify: `crates/mkrew-ui/src/files_view.rs` (`open_find`, `replace_one`, `replace_all`; tests)
- Modify: `crates/mkrew-ui/src/keymap.rs` (`FindInFile`, `ReplaceInFile`; `spec_defaults`)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title`, `available`, test)
- Modify: `crates/mkrew-ui/src/main.rs`:
  - field `file_replace_focus` + init
  - `open_file_find` :1770-1783, `file_find_key_down` :1793, `close_file_find` :1868
  - new methods: `file_replace_key_down`, `edit_file_replace`, `replace_in_file`, `replace_all_in_file`
  - `run` :6135-6146, :6382-6389
  - the find bar :9011-9080
- Modify: `docs/SPEC.md` §12A :807-809, §13 after :904; `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Editor::replace`, `Editor::replace_all`, `Editor::select`, `Editor::selected_text`; `FilesView::shown`, `editor_mut`, `file_edited`, `reveal_file_match` (Task 5).
- Produces in `text_find`: `TextFind.replace: Option<String>` (`Some` = the replace row is shown) and `TextFind::search_from(&mut self, lines: &[String], from: (usize, usize))`.
- Produces in `files_view`: `FilesView::open_find(&mut self, replace: bool) -> bool`, `replace_one(&mut self) -> bool`, `replace_all(&mut self) -> usize`.
- Produces in `keymap`: `Command::FindInFile` (`find_in_file`, `ctrl-f`) and `Command::ReplaceInFile` (`replace_in_file`, `ctrl-h`), both in `FILES_CONTEXT`.

- [ ] **Step 1: Failing tests**

In the `files_view.rs` tests:

```rust
    #[test]
    fn find_opens_seeded_with_a_one_line_selection_at_the_caret() {
        let mut view = opened("let a = b;\nlet b = a;\n");
        view.editor_mut().unwrap().select(Pos::new(1, 4)..Pos::new(1, 5));
        assert!(view.open_find(false));
        let find = view.find.as_ref().unwrap();
        assert_eq!((find.query.as_str(), find.replace.as_deref()), ("b", None));
        let at = find.current().map(|m| (m.line, m.range.start));
        assert_eq!(at, Some((1, 4)), "the selected one is current");
        // Ctrl+H adds the replace row; a multi-line selection keeps the query.
        view.editor_mut().unwrap().select(Pos::new(0, 0)..Pos::new(1, 1));
        assert!(view.open_find(true));
        let find = view.find.as_ref().unwrap();
        assert_eq!((find.query.as_str(), find.replace.as_deref()), ("b", Some("")));
        // Without a file there is nothing to find in.
        assert!(!FilesView::new(Ulid::new(), None).open_find(false));
    }

    #[test]
    fn replace_one_moves_on_and_replace_all_is_one_undo_step() {
        let mut view = opened("a1 A2\na3\n");
        assert!(view.open_find(true));
        let find = view.find.as_mut().unwrap();
        find.query = "a".into();
        find.replace = Some("aa".into());
        view.search();
        assert!(view.replace_one());
        assert_eq!(text(&view), "aa1 A2\na3\n");
        let at = view.find.as_ref().and_then(TextFind::current).map(|m| (m.line, m.range.start));
        assert_eq!(at, Some((0, 4)), "the next match, not one inside the replacement");
        assert!(view.replace_one());
        assert!(view.replace_one());
        assert_eq!(text(&view), "aa1 aa2\naa3\n");
        // Replace all: each match once (the replacement holds the query), ignoring case,
        // in UTF-8; one undo step; not saved.
        let mut view = opened("Tiếng việt, VIỆT\nviệt\n");
        view.open_find(true);
        let find = view.find.as_mut().unwrap();
        find.query = "việt".into();
        find.replace = Some("Nam việt".into());
        assert_eq!(view.replace_all(), 3);
        assert_eq!(text(&view), "Tiếng Nam việt, Nam việt\nNam việt\n");
        assert_eq!(view.unsaved(), Some("README.md"));
        assert_eq!(view.find.as_ref().map(TextFind::label).as_deref(), Some("1 of 3"));
        assert!(view.editor_mut().unwrap().undo());
        assert_eq!(text(&view), "Tiếng việt, VIỆT\nviệt\n");
        assert_eq!(view.unsaved(), None);
        // Without the replace row nothing is replaced.
        view.find.as_mut().unwrap().replace = None;
        assert_eq!(view.replace_all(), 0);
        assert!(!view.replace_one());
    }
```

In the `text_find.rs` tests:

```rust
    #[test]
    fn a_search_from_a_place_starts_there_and_wraps() {
        let text = lines(&["ab", "b", "ab"]);
        let mut find = TextFind {
            query: "b".into(),
            ..TextFind::default()
        };
        find.search_from(&text, (1, 1));
        assert_eq!(at(&find), Some((2, 1)));
        find.search_from(&text, (2, 2));
        assert_eq!(at(&find), Some((0, 1)), "past the last: the first");
    }
```

In `keymap.rs` `spec_defaults`, after the SaveFile lines:

```rust
        assert_eq!(bound("ctrl-f"), Some(Command::FindInFile));
        assert_eq!(bound("ctrl-h"), Some(Command::ReplaceInFile));
        for command in [Command::FindInFile, Command::ReplaceInFile] {
            assert_eq!(command.context(), Some(FILES_CONTEXT), "{command:?}");
        }
```

Run: `cargo test -p mkrew-ui --lib files_view::tests -j 4`
Expected: FAIL to compile (`open_find`, the `replace` field and `FindInFile` are missing).

- [ ] **Step 2: `TextFind`**

In `text_find.rs`, change the module doc (:1-3) to "Find & replace in the Files editor (SPEC §12A): the query, the replacement while the replace row is shown, its case-insensitive matches…". Then the struct:

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextFind {
    pub query: String,
    /// The replace row's text; `None` = the row is hidden (Ctrl+H shows it).
    pub replace: Option<String>,
    hits: Hits,
}
```

Split `search` (:62-85):

```rust
    /// Searches `lines` again (after an edit of the query or the text). The current match is
    /// the first one at or after the previous current position.
    pub fn search(&mut self, lines: &[String]) {
        let from = self.current().map_or((0, 0), |m| (m.line, m.range.start));
        self.search_from(lines, from);
    }

    /// Searches `lines`; the current match is the first at or after `from` (line, byte),
    /// else the first one.
    pub fn search_from(&mut self, lines: &[String], from: (usize, usize)) {
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
```

- [ ] **Step 3: `FilesView` find & replace**

In `files_view.rs`, use the imports `std::ops::Range`, `crate::editor::Pos` and `crate::text_find::{self, TextFind}`:

```rust
    /// Ctrl+F / Ctrl+H: opens the find bar, with the replace row when `replace`. A selection
    /// on one line becomes the query, and the current match is the first one at the selection
    /// or caret. Returns `false` when no file is open.
    pub fn open_find(&mut self, replace: bool) -> bool {
        if self.file.is_none() {
            return false;
        }
        let editor = self.shown().map(|shown| &shown.editor);
        let seed = (editor.and_then(Editor::selected_text))
            .filter(|text| !text.is_empty() && !text.contains('\n'));
        let from = editor.map_or(Pos::default(), |editor| {
            editor.selection().map_or(editor.cursor(), |selection| selection.start)
        });
        let find = self.find.get_or_insert_with(TextFind::default);
        if let Some(seed) = seed {
            find.query = seed;
        }
        if replace {
            find.replace.get_or_insert_with(String::new);
        }
        if let (Some(find), Content::Shown(shown)) = (&mut self.find, &self.content) {
            find.search_from(shown.editor.lines(), (from.line, from.col));
        }
        true
    }

    /// Replace (Enter in the replace field): the current match becomes the replacement and the
    /// next match after it is current. One undo step; not saved (like VS Code).
    pub fn replace_one(&mut self) -> bool {
        let (Some(find), Content::Shown(shown)) = (&mut self.find, &mut self.content) else {
            return false;
        };
        let (Some(with), Some(found)) = (find.replace.clone(), find.current().cloned()) else {
            return false;
        };
        let line = found.line;
        shown.editor.replace(
            Pos::new(line, found.range.start)..Pos::new(line, found.range.end),
            &with,
        );
        // Continue after the replacement, so a replacement that holds the query is not
        // replaced again.
        find.search_from(shown.editor.lines(), (line, found.range.start + with.len()));
        true
    }

    /// Replace all (Ctrl+Alt+Enter, "Replace all"): every match at once, as one undo step;
    /// not saved. Returns how many matches were replaced.
    pub fn replace_all(&mut self) -> usize {
        let (Some(find), Content::Shown(shown)) = (&mut self.find, &mut self.content) else {
            return 0;
        };
        let Some(with) = find.replace.clone() else {
            return 0;
        };
        let edits: Vec<(Range<Pos>, String)> = (shown.editor.lines().iter().enumerate())
            .flat_map(|(line, text)| {
                text_find::find_in_line(text, &find.query)
                    .into_iter()
                    .map(move |range| (Pos::new(line, range.start)..Pos::new(line, range.end), line))
            })
            .map(|(range, _)| (range, with.clone()))
            .collect();
        let count = edits.len();
        shown.editor.replace_all(edits);
        find.search(shown.editor.lines());
        count
    }
```

Run: `cargo test -p mkrew-ui --lib files_view::tests -j 4` then `cargo test -p mkrew-ui --lib text_find::tests -j 4`
Expected: PASS.

- [ ] **Step 4: Keymap + palette**

- `Command`, after `SaveFile`:

```rust
    /// Files only: open the find bar over the opened file (Ctrl+F).
    FindInFile,
    /// Files only: open the find bar with its replace row (Ctrl+H).
    ReplaceInFile,
```

- `all()`, after `Command::SaveFile,`: `Command::FindInFile, Command::ReplaceInFile,`
- `name()`: `"find_in_file"`, `"replace_in_file"`
- `default_keys()`: `"ctrl-f"`, `"ctrl-h"`
- `context()`: `Command::ToggleIgnored | Command::SaveFile | Command::FindInFile | Command::ReplaceInFile => Some(FILES_CONTEXT),`
- `palette.rs` `title`: `Command::FindInFile => "Files: Find in file".into(),` and `Command::ReplaceInFile => "Files: Replace in file".into(),`
- `palette.rs` `available`: add both to the `in_view(View::Files)` arm.
- Palette test: add both to the "not in terminals" list, and add `assert!(commands(&context(View::Files)).contains(&Command::ReplaceInFile));`.

Run: `cargo test -p mkrew-ui --lib keymap::tests -j 4` then `cargo test -p mkrew-ui --lib palette::tests -j 4`
Expected: PASS. Task 5's `agent_keys_are_unbound_and_reach_the_pty` allows both, because they are in `FILES_CONTEXT`.

- [ ] **Step 5: Shell**

Field next to `file_find_focus` (:304), with its init (:635):

```rust
    /// The Files find bar's replace field.
    file_replace_focus: FocusHandle,
```

```rust
            file_replace_focus: cx.focus_handle(),
```

`open_file_find` (:1770-1783):

```rust
    /// Ctrl+F / Ctrl+Shift+F / "Find" (or Ctrl+H for `replace`) in the Files view: opens the
    /// find bar over the opened file and gives its field the keyboard.
    fn open_file_find(&mut self, replace: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(project_id) = self.workspace else {
            return;
        };
        if !self.files_view(project_id).open_find(replace) {
            return;
        }
        let field = if replace {
            &self.file_replace_focus
        } else {
            &self.file_find_focus
        };
        window.focus(field, cx);
        self.reveal_file_match(project_id, cx);
    }
```

Callers:
- `Command::Find` (:6383) and the header Find button (:9019) call `open_file_find(false, …)`.
- In `run`'s dispatch, add:

```rust
            Command::FindInFile => self.open_file_find(false, window, cx),
            Command::ReplaceInFile => self.open_file_find(true, window, cx),
```

The focus exception in `run` (:6135-6146) replaces Task 5's `*command != Command::SaveFile` condition. Both fields keep bound keys to themselves, except the Files commands, which act from there:

```rust
        // Same for the Files find / replace fields while they have the keyboard (Esc gives it
        // back), except the Files commands (Ctrl+S, Ctrl+F, Ctrl+H act from there).
        if self.file_find_focus.is_focused(window) || self.file_replace_focus.is_focused(window) {
            let open = self.view() == View::Files
                && (self.workspace)
                    .and_then(|id| self.files.get(&id))
                    .is_some_and(|view| view.find.is_some());
            let files_command = matches!(
                command,
                Command::SaveFile | Command::FindInFile | Command::ReplaceInFile
            );
            if open && !files_command {
                cx.propagate();
                return;
            }
            if !open {
                window.focus(&self.focus_handle, cx);
            }
        }
```

`file_find_key_down`: after `let Some(project_id) …` (:1796), before the paste check, add:

```rust
        let m = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "enter" if m.control && m.alt => {
                return self.replace_all_in_file(project_id, window, cx);
            }
            "tab" => {
                // Tab / Shift+Tab: to the replace field when it is shown.
                let replacing = (self.files_view(project_id).find.as_ref())
                    .is_some_and(|find| find.replace.is_some());
                if replacing {
                    window.focus(&self.file_replace_focus, cx);
                    cx.notify();
                }
                return;
            }
            _ => {}
        }
```

New methods next to it:

```rust
    /// Keys of the replace field:
    /// - typing edits the replacement;
    /// - Enter replaces the current match and goes to the next;
    /// - Ctrl+Alt+Enter replaces all;
    /// - Tab goes back to the find field;
    /// - Esc closes the bar.
    fn file_replace_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(project_id) = self.workspace else {
            return;
        };
        let keystroke = &event.keystroke;
        let m = keystroke.modifiers;
        match keystroke.key.as_str() {
            "enter" if m.control && m.alt => {
                return self.replace_all_in_file(project_id, window, cx);
            }
            "enter" if !m.shift && !m.control => {
                return self.replace_in_file(project_id, window, cx);
            }
            "tab" => {
                window.focus(&self.file_find_focus, cx);
                cx.notify();
                return;
            }
            _ => {}
        }
        if task_dialog::dialog_key(keystroke) == Some(DialogKey::Paste) {
            if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                let text = text.replace(['\r', '\n'], " ");
                self.edit_file_replace(project_id, |with| with.push_str(&text), cx);
            }
            return;
        }
        match find_key(keystroke) {
            Some(FindKey::Close) => self.close_file_find(project_id, window, cx),
            Some(FindKey::Backspace) => self.edit_file_replace(
                project_id,
                |with| {
                    with.pop();
                },
                cx,
            ),
            Some(FindKey::Insert(text)) => {
                self.edit_file_replace(project_id, |with| with.push_str(&text), cx);
            }
            _ => {}
        }
    }

    fn edit_file_replace(
        &mut self,
        project_id: Ulid,
        edit: impl FnOnce(&mut String),
        cx: &mut Context<'_, Self>,
    ) {
        let find = self.files_view(project_id).find.as_mut();
        if let Some(with) = find.and_then(|find| find.replace.as_mut()) {
            edit(with);
            cx.notify();
        }
    }

    fn replace_in_file(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.files_view(project_id).replace_one() {
            self.file_edited(project_id, window, cx);
            self.reveal_file_match(project_id, cx);
        }
    }

    fn replace_all_in_file(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).replace_all() > 0 {
            self.file_edited(project_id, window, cx);
        }
        cx.notify();
    }
```

`close_file_find` (:1868): `window.focus(&self.file_editor_focus, cx);` (Esc goes back to the editor).

Find bar (:9022-9080):
- Rename the existing row to `find_row`, moving its `.px/.pb` there.
- `find_bar` becomes `div().flex_none().flex().flex_col().child(find_row).children(replace_row)`.
- The field placeholder stays "Find in file".
- Before the find field in `find_row`, add this toggle:

```rust
                .child(
                    button(
                        "file-find-toggle-replace",
                        if find.replace.is_some() { "▾" } else { "▸" },
                        theme,
                        false,
                    )
                    .tooltip(tooltip(
                        theme,
                        "Toggle replace",
                        self.keymap.label(Command::ReplaceInFile),
                    ))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        let shown = (shell.files_view(project_id).find.as_ref())
                            .is_some_and(|find| find.replace.is_some());
                        if !shown {
                            return shell.open_file_find(true, window, cx);
                        }
                        if let Some(find) = &mut shell.files_view(project_id).find {
                            find.replace = None;
                        }
                        window.focus(&shell.file_find_focus, cx);
                        cx.notify();
                    })),
                )
```

The replace row:

```rust
            let replacing = self.file_replace_focus.is_focused(window);
            let replace_row = find.replace.as_ref().map(|with| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::L))
                    .pb(px(space::S))
                    .child(
                        text_field("file-replace-field", with, "Replace", replacing, false, theme)
                            .w(px(theme::FIND_INPUT_WIDTH))
                            .track_focus(&self.file_replace_focus)
                            .on_key_down(cx.listener(Self::file_replace_key_down))
                            .relative()
                            .child(self.typing(
                                &self.file_replace_focus,
                                Self::file_replace_key_down,
                                cx,
                            ))
                            .on_click(cx.listener(|shell, _, window, cx| {
                                window.focus(&shell.file_replace_focus, cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        button("file-replace-one", "Replace", theme, false)
                            .tooltip(tooltip(
                                theme,
                                "Replace this match and go to the next",
                                "Enter",
                            ))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.file_replace_focus, cx);
                                shell.replace_in_file(project_id, window, cx);
                            })),
                    )
                    .child(
                        button("file-replace-all", "Replace all", theme, false)
                            .tooltip(tooltip(theme, "Replace every match", "Ctrl+Alt+Enter"))
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                window.focus(&shell.file_replace_focus, cx);
                                shell.replace_all_in_file(project_id, window, cx);
                            })),
                    )
            });
```

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean.

- [ ] **Step 6: SPEC, HUMAN_CHECKS, inbox**

SPEC §12A: replace the find bullet (:807-809) with:

```
- **Find & replace** (VS Code style): `Ctrl+F` (also `Ctrl+Shift+F` or "Find" in the header)
  opens the bar, `Ctrl+H` opens it with the replace row. A selection on one line becomes the
  query. Case-insensitive; matches highlighted, Enter / Shift+Enter move between them and select
  the match, Tab / Shift+Tab switch find ↔ replace. In the replace field Enter replaces the
  current match and goes to the next; Ctrl+Alt+Enter (in either field) or "Replace all" replaces
  every match as one undo step. Replacing does not save. Esc closes the bar and returns to the
  editor.
```

SPEC §13: row :896 stays. After the Save row from Task 5, add `| Files | Find / replace in the opened file (replace all: Ctrl+Alt+Enter in the bar) | Ctrl+F / Ctrl+H |`.

HUMAN_CHECKS, 2026-10-09 section:

```
- [ ] **Find & replace:**
  - Select `foo` on one line and press Ctrl+F: the bar opens with `foo`, and the selected one is
    current.
  - Ctrl+H: the replace row opens and its field has the keyboard. Type `bar`. Enter replaces the
    current match and selects the next one. Ctrl+Alt+Enter (or "Replace all") replaces the rest.
    `●` shows and nothing is saved until Ctrl+S, which also works from the bar.
  - Ctrl+Z brings everything back at once.
  - Replace `a` with `aa`: no endless loop.
  - The Vietnamese query `việt` matches `VIỆT`.
  - Tab / Shift+Tab switch fields; Esc closes the bar and typing goes to the editor.
  - Ctrl+Shift+H still toggles ignored files; Ctrl+Shift+1 still shows the Board.
  - Check light and dark.
```

Run the full verification. Line 28 of `docs/INBOX.md` (`[ ]: Files module: when viewing a file need to have a short cut for find and replace content…`) changes in two stages:
- Now: mark it `- [~] … → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 7`.
- Once the check above passes: mark it `[x]`.

Suggested message: `feat(ui): find and replace in the Files editor (Ctrl+F, Ctrl+H)`

---

### Task 8: Several git repositories in one workspace — protocol, daemon, watcher, Files markers (ADR 0029)

Inbox: `docs/INBOX.md` line 30 today (the brief said 104; re-grep the text) `Git module: also need support a case where I open an Root folter container FE and BE, then git shold dected there are 2 diffreence git repo and also need to support in this case`. Spec: §7 (SPEC.md:526-538), §12A (:797-799), §12B (:827-829), M5 (:1145), §20C.1 (:1424), §20E.1 (:1496-1500, :1531-1532).

**Current state (investigated):**
- Every `fs.*` / `git.*` / `ai.generate` request resolves ONE folder in `Daemon::target` (`server/target.rs:31-79`): `project.path`, or the project's folder in a task worktree. `watched_target` (:83-97) watches `target.dir`.
- Repo discovery is upward only: `GitCli::repo` (`git.rs:316-333`) runs `git rev-parse --absolute-git-dir --show-prefix` in the dir and returns `Ok(None)` outside a repo. `GitCache::repo` (`git_view.rs:101-108`) turns that into `ViewError::NotARepo`, and `view_outcome` (`server/git.rs:294-314`) maps it to `invalid_state` "the project folder is not inside a git repository". So a root holding `fe/.git` and `be/.git` gets an error in every git request.
- `git_view::{status, diff, log, show, push}` and the `git_change` closure (`server/git.rs:248-271`) only take `dir`, so they already work unchanged on `root/fe`. Paths in replies are relative to `dir`.
- Handlers (`server/git.rs`): `git_status` :22, `git_diff` :57, `git_stage` :101, `git_discard` :137, `git_commit` :167, `git_log` :182, `git_show` :210, `git_remote` :231, `git_change` :248, `relative_paths` :318. `ai_generate` (`server/ai.rs:27`) calls `self.target` at :32-38 and runs `prompt` (:125) in `target.dir`. Dispatch: `server.rs:388-402`.
- Files markers: `fs_list` (`server/fs.rs:15-53`) reads markers from `cache.repo(root)` + `git.status` only (:35-40). A non-repo root gets no markers. `files::markers` (`files.rs:108-130`) keys by dir-relative path; `files::list` (:147-222) derives `has_changes` of folders from marker keys (:172-175).
- Watcher: `ALWAYS_IGNORED` (`watch.rs:20`) drops every `.git` component. `Filter::is_git_state` (:95-108) only accepts `<root>/.git/{HEAD,index,packed-refs,refs/…}`, and its test (:361-385) asserts `sub/.git/HEAD` is NOT git state (:383). So a commit in `fe/` from a terminal emits no `git.changed`. `debounce` (:155-156) checks `is_git_state` before the ignore filter, so only `is_git_state` changes. `session.rs:614-626` sends `GitChanged{project_id, worktree_task_id}` for each batch; no change needed there.
- Protocol (`crates/mkrew-protocol/src/files.rs`): `ProjectRef` :12, `GitRemoteParams` :18, `GitStatusParams` :38, `GitDiffParams` :112, `GitStageParams` :187, `GitDiscardParams` :206, `GitCommitParams` :217, `GitLogParams` :231, `GitShowParams` :267, `GitSetBaseParams` :283 (unchanged), `GitChangedEvent` :290 (unchanged), `AiGenerateParams` :310. Wire test `every_files_and_git_request_round_trips` :482-602 (method list :577-593). Requests in `message.rs:171-203`. `PROTOCOL_VERSION` (`lib.rs:16`) is 26 today.
- `docs::relative_path` (`docs.rs:49-57`) normalizes `\` to `/` and refuses empty, absolute and `..` paths.
- Real-git daemon tests use `git(dir, args)` (`tests/daemon.rs:3748`) and `Env::repo()` (:135). The external-commit `git.changed` pattern is at :3320-3326.

**Files:**
- Modify: `crates/mkrew-protocol/src/files.rs` (:18-22, :38-49, :112-129, :187-195, :206-213, :217-222, :231-240, :267-272, :310-324, tests :486-593), `crates/mkrew-protocol/src/message.rs` (:191 add a variant), `crates/mkrew-protocol/src/lib.rs:16`
- Modify: `crates/mkrew-daemon/src/server/target.rs` (whole file, 98 lines), `crates/mkrew-daemon/src/server/git.rs` (:3-19 imports, handlers :22-271, helpers after :330, tests :346-359), `crates/mkrew-daemon/src/server/fs.rs` (:5-6, :24-42, :63, :81), `crates/mkrew-daemon/src/server/ai.rs:32-35`, `crates/mkrew-daemon/src/server.rs:388`, `crates/mkrew-daemon/src/files.rs` (new fn after `markers` :130, test after :390), `crates/mkrew-daemon/src/watch.rs` (:92-108, test :361-385), `crates/mkrew-daemon/src/git_view.rs:608-611` (test literal)
- Modify (struct literals only, `repo: None`, so the workspace compiles after this task): `crates/mkrew-ui/src/git_view.rs:66,380,515,941,987,1211,1237`, `crates/mkrew-ui/src/git_actions.rs:80,152,268,371,379`, `crates/mkrew-ui/src/main.rs:2571,5703`, `crates/mkrew-ui/src/worktree_dialog.rs:158,168,207`, `crates/mkrew-ui/src/fanout.rs:236`. Task 9 replaces the Git view's ones with the shown repo.
- Test: `crates/mkrew-daemon/tests/daemon.rs` (new test after `git_log_show_push_and_fetch_with_a_local_remote`, which ends before :3747; `repo: None` in existing literals)
- Create: `docs/decisions/0029-several-repos-per-workspace.md`
- Modify: `docs/SPEC.md` §7, §12A, §12B, M5, §20C.1, §20E.1

**Interfaces:**
- Consumes: `docs::relative_path(&str) -> Option<(String, PathBuf)>`, `watch::ALWAYS_IGNORED`, `files::markers(&GitStatus) -> HashMap<String, GitMarker>`, `GitCache::repo`, `GitBackend::status`.
- Produces:
  - Protocol: `pub repo: Option<String>` (`#[serde(default)]`) on `GitStatusParams`, `GitDiffParams`, `GitStageParams`, `GitDiscardParams`, `GitCommitParams`, `GitLogParams`, `GitShowParams`, `GitRemoteParams`, `AiGenerateParams`. `Request::GitRepos(ProjectRef)` = `"git.repos"` → `Vec<String>`. `PROTOCOL_VERSION` = next number (28 if Task 3 already landed).
  - Daemon: `Target.repo: Option<String>`; `Daemon::target(&self, project_id: Ulid, worktree_task_id: Option<Ulid>, repo: Option<&str>) -> Result<Target, Outcome>`; `Daemon::watched_target(…, repo: Option<&str>)` (same shape); `target::repo_name(Option<&str>) -> Result<Option<String>, Outcome>`; `files::child_repos(root: &Path) -> Vec<String>`; `Daemon::git_repos(&self, ProjectRef) -> Outcome`; in `server/git.rs`: `fn repo_paths(repo: Option<&str>, paths: Vec<String>) -> Result<Vec<String>, Outcome>` and `fn prefix_files(repo: Option<&str>, files: &mut [ChangedFile])`.
  - Contract kept: request and reply paths stay **project-relative** (`fe/src/a.ts`). The daemon strips `repo/` before git sees a path and adds it back to replies. So Open in Zed, diff comments, Changed-by and Send selection keep working.

- [ ] **Step 1: Failing daemon unit tests (discovery, watcher, path mapping)**

In `crates/mkrew-daemon/src/files.rs` tests (after `markers_prefer_untracked_then_deleted_then_added`):

```rust
    #[test]
    fn child_repos_are_direct_folders_holding_git_sorted() {
        let root = tempfile::tempdir().unwrap();
        let dir = |name: &str| {
            let path = root.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            path
        };
        dir("fe/.git");
        dir("be/.git");
        // A worktree or submodule has a `.git` file.
        std::fs::write(dir("lib").join(".git"), "gitdir: ../x\n").unwrap();
        dir("docs");
        dir(".cache/.git");
        dir("node_modules/pkg/.git");
        dir("target/.git");
        dir("apps/web/.git");
        std::fs::write(root.path().join("file"), "x").unwrap();
        // Hidden, `node_modules`, `target` and depth-2 repos are skipped; a `.git` file counts.
        assert_eq!(child_repos(root.path()), ["be", "fe", "lib"]);
    }
```

In `crates/mkrew-daemon/src/watch.rs`, replace `git_state_is_head_index_and_refs_of_the_root_git_dir` (:361-385) with:

```rust
    #[test]
    fn git_state_is_head_index_and_refs_of_the_root_or_a_child_git_dir() {
        let (dir, filter) = filter("");
        for git in [dir.path().join(".git"), dir.path().join("fe").join(".git")] {
            for state in [
                "HEAD",
                "index",
                "packed-refs",
                "refs/heads/main",
                "refs/tags/v1",
            ] {
                assert!(filter.is_git_state(&git.join(state)), "{}", git.join(state).display());
            }
            for other in [
                "index.lock",
                "refs",
                "objects/ab/cdef",
                "logs/HEAD",
                "HEAD/x",
            ] {
                assert!(!filter.is_git_state(&git.join(other)), "{other}");
            }
        }
        // Only direct child repositories (ADR 0029), never deeper ones or plain files.
        assert!(!filter.is_git_state(&dir.path().join("apps/web/.git/HEAD")));
        assert!(!filter.is_git_state(&dir.path().join("HEAD")));
        assert!(!filter.is_git_state(&dir.path().join("fe/HEAD")));
    }
```

In `crates/mkrew-daemon/src/server/git.rs` tests, change `use super::check_ref;` to `use super::{check_ref, repo_paths};` and add:

```rust
    #[test]
    fn repo_paths_must_lie_in_the_repo_and_lose_its_prefix() {
        let paths = |p: &[&str]| p.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
        assert_eq!(repo_paths(None, paths(&["a/b"])).ok(), Some(paths(&["a/b"])));
        assert_eq!(
            repo_paths(Some("fe"), paths(&["fe/src/a.ts", "fe/x"])).ok(),
            Some(paths(&["src/a.ts", "x"]))
        );
        // The repo folder itself, a sibling sharing its prefix, another repo, the root.
        for bad in ["fe", "fe/", "fex/a", "be/a", "a"] {
            assert!(repo_paths(Some("fe"), paths(&[bad])).is_err(), "{bad}");
        }
        // The repo name is checked too.
        for bad in ["../x", "fe/src", ".git", "FE/.git"] {
            assert!(repo_paths(Some(bad), paths(&["x/a"])).is_err(), "{bad}");
        }
    }
```

Run: `cargo test -p mkrew-daemon --lib files::tests::child_repos -j 4`
Expected: FAIL to compile (`cannot find function child_repos`).

- [ ] **Step 2: Protocol — `repo` on git requests, `git.repos`, version**

In `crates/mkrew-protocol/src/files.rs`, extend the module doc (:3-5) with: `` `repo` (ADR 0029): run in that direct child folder of the project folder, a git repository of its own; paths in requests and replies stay relative to the project folder (`fe/src/a.ts`). ``

Add this field right after `worktree_task_id` in `GitRemoteParams`, `GitStatusParams`, `GitDiffParams`, `GitStageParams`, `GitDiscardParams`, `GitCommitParams`, `GitLogParams`, `GitShowParams` and `AiGenerateParams`:

```rust
    /// A direct child folder of the project folder that is its own git repository (ADR 0029);
    /// `None` = the project folder. Not with `worktree_task_id`.
    #[serde(default)]
    pub repo: Option<String>,
```

Change the `ProjectRef` doc (:10) to `/// Params of requests that only name a project.`

In `message.rs`, after `GitShow` (:191):

```rust
    /// Returns `string[]`: the git repositories of the project folder (ADR 0029). `""` first
    /// when the folder itself is in a repository, then each direct child folder holding `.git`.
    #[serde(rename = "git.repos")]
    GitRepos(ProjectRef),
```

`lib.rs:16`: bump `PROTOCOL_VERSION` to the next number (28 if Task 3 already landed).

Wire test `every_files_and_git_request_round_trips` (files.rs:486-593): add `repo: None,` after `worktree_task_id` in every Git*/AiGenerate literal, except the `GitStatus` one at :487, which gets `repo: Some("fe".into()),` so the field goes over the wire once. After the `GitShow` literal (:538) add `Request::GitRepos(ProjectRef { project_id }),`, and after `"git.show",` (:585) add `"git.repos",`.

Run: `cargo test -p mkrew-protocol --lib files::tests -j 4`
Expected: PASS.

- [ ] **Step 3: `Daemon::target` takes the repo**

Replace the body of `crates/mkrew-daemon/src/server/target.rs` from `use` (:4) to the end with:

```rust
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mkrew_protocol::{ErrorCode, Outcome, Project};
use ulid::Ulid;

use super::{Daemon, store_outcome, task_dir};
use crate::docs;

/// A resolved request target.
pub(super) struct Target {
    pub project: Project,
    /// The project folder or the project's folder inside the task's worktree, then the child
    /// repository folder `repo` in it.
    pub dir: PathBuf,
    /// Echoed in `git.changed`.
    pub worktree_task_id: Option<Ulid>,
    /// The child repository folder (ADR 0029): reply paths get `repo/` in front.
    pub repo: Option<String>,
}

impl Target {
    /// The key of the target's file watcher and `fs.find` list.
    pub fn key(&self) -> Ulid {
        self.worktree_task_id.unwrap_or(self.project.id)
    }
}

/// The child repository folder a request names (ADR 0029): `None` for none or blank;
/// `invalid_request` unless it is one plain folder name other than `.git`.
pub(super) fn repo_name(repo: Option<&str>) -> Result<Option<String>, Outcome> {
    let Some(repo) = repo.map(str::trim).filter(|repo| !repo.is_empty()) else {
        return Ok(None);
    };
    match docs::relative_path(repo) {
        Some((name, _)) if !name.contains('/') && !name.eq_ignore_ascii_case(".git") => {
            Ok(Some(name))
        }
        _ => {
            let message = format!("repo {repo:?} must be a folder directly in the project folder");
            Err(Outcome::error(ErrorCode::InvalidRequest, message))
        }
    }
}

impl Daemon {
    /// Resolves `project_id` + `worktree_task_id` + `repo`. The task must belong to the project
    /// and have a worktree that still exists (`invalid_state` otherwise). `repo` is refused
    /// with a task worktree (ADR 0029: worktrees hold one repository).
    pub(super) async fn target(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        repo: Option<&str>,
    ) -> Result<Target, Outcome> {
        let repo = repo_name(repo)?;
        if repo.is_some() && worktree_task_id.is_some() {
            let message = "repo cannot be combined with worktree_task_id";
            return Err(Outcome::error(ErrorCode::InvalidRequest, message));
        }
        let project = self
            .store()
            .get_project(project_id)
            .map_err(|e| store_outcome::<()>(Err(e)))?;
        let Some(task_id) = worktree_task_id else {
            let mut dir = PathBuf::from(&project.path);
            if let Some(repo) = &repo {
                dir.push(repo);
            }
            return Ok(Target {
                project,
                dir,
                worktree_task_id,
                repo,
            });
        };
        let task = self
            .store()
            .get_task(task_id)
            .map_err(|e| store_outcome::<()>(Err(e)))?;
        if task.project_id != project_id {
            let message = format!("task {task_id} is not in project {project_id}");
            return Err(Outcome::error(ErrorCode::InvalidRequest, message));
        }
        let Some(root) = task.worktree_path.map(PathBuf::from) else {
            let message = format!("task {task_id} has no worktree");
            return Err(Outcome::error(ErrorCode::InvalidState, message));
        };
        let git = Arc::clone(&self.git);
        let project_dir = PathBuf::from(&project.path);
        let dir =
            tokio::task::spawn_blocking(move || task_dir(&*git, &project_dir, Some(&root))).await;
        match dir {
            Ok(Ok(dir)) => Ok(Target {
                project,
                dir,
                worktree_task_id,
                repo: None,
            }),
            Ok(Err(e)) => {
                let message = format!("task {task_id}'s worktree: {e}");
                Err(Outcome::error(ErrorCode::InvalidState, message))
            }
            Err(e) => Err(Outcome::error(
                ErrorCode::Internal,
                format!("resolving task {task_id}'s worktree failed: {e}"),
            )),
        }
    }

    /// [`Daemon::target`], kept watched so its view refreshes on `git.changed` and
    /// `fs.find`'s cached list is invalidated. The project folder is watched, never a child
    /// repository alone: the watch is keyed by the project.
    pub(super) async fn watched_target(
        &self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        repo: Option<&str>,
    ) -> Result<Target, Outcome> {
        let target = self.target(project_id, worktree_task_id, repo).await?;
        match target.worktree_task_id {
            None => self
                .hub
                .keep_watched(target.project.id, Path::new(&target.project.path)),
            Some(task_id) => {
                self.hub
                    .keep_worktree_watched(target.project.id, task_id, &target.dir)
            }
        }
        Ok(target)
    }
}
```

Callers without a repo pass `None`: `fs_list` (fs.rs:25), `fs_read` (:63), `fs_find` (:81) → `.watched_target(params.project_id, params.worktree_task_id, None)`. `ai_generate` (ai.rs:33) → `.target(params.project_id, params.worktree_task_id, params.repo.as_deref())`. The ✦ commit message then reads the staged diff of the chosen repo.

- [ ] **Step 4: git handlers — map paths, add `git.repos`**

In `server/git.rs`:
- Add `ChangedFile` and `ProjectRef` to the `mkrew_protocol` import (:6-10).
- Add `use super::target::{Target, repo_name};` (replacing :15).
- Add `use crate::{docs, files};` (replacing :17).

Helpers after `relative_paths` (:330):

```rust
/// Request paths stay project-relative with `repo` (ADR 0029): each must lie inside it and
/// loses its `repo/` prefix before git sees it; `invalid_request` otherwise.
fn repo_paths(repo: Option<&str>, paths: Vec<String>) -> Result<Vec<String>, Outcome> {
    let Some(repo) = repo_name(repo)? else {
        return Ok(paths);
    };
    paths
        .into_iter()
        .map(|path| {
            match path
                .strip_prefix(repo.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
            {
                Some(inner) if !inner.is_empty() => Ok(inner.to_owned()),
                _ => {
                    let message = format!("path {path:?} is not in repo {repo:?}");
                    Err(Outcome::error(ErrorCode::InvalidRequest, message))
                }
            }
        })
        .collect()
}

/// Reply paths back to project-relative (ADR 0029): `repo/` in front of each.
fn prefix_files(repo: Option<&str>, files: &mut [ChangedFile]) {
    let Some(repo) = repo else {
        return;
    };
    for file in files {
        file.path = format!("{repo}/{}", file.path);
        if let Some(old) = &mut file.old_path {
            *old = format!("{repo}/{old}");
        }
    }
}
```

`git_status` (:22-55): `.watched_target(params.project_id, params.worktree_task_id, params.repo.as_deref())`. In the closure, destructure `repo` too (`let Target { project: Project { id, base_ref, .. }, dir, repo, .. } = target;`) and map the result:

```rust
            git_view::status(
                &*hub.git,
                &cache,
                &dir,
                base_ref.as_deref(),
                params.mode,
                params.since.as_deref(),
                &|| hub.snapshots.tree(&*hub.git, id, &dir),
            )
            .map(|mut status| {
                for files in [
                    &mut status.staged,
                    &mut status.unstaged,
                    &mut status.untracked,
                    &mut status.against_base,
                ] {
                    prefix_files(repo.as_deref(), files);
                }
                status
            })
```

`git_diff` (:57-97): after the `relative_path` check (:58-64) add

```rust
        let path = match repo_paths(params.repo.as_deref(), vec![path]) {
            Ok(mut path) => path.remove(0),
            Err(outcome) => return outcome,
        };
```

Then `.target(params.project_id, params.worktree_task_id, params.repo.as_deref())`. Destructure `repo` from `Target` in the closure, and append to the `git_view::diff(…)` call:

```rust
            .map(|mut diff| {
                if let Some(repo) = &repo {
                    diff.path = format!("{repo}/{}", diff.path);
                    diff.old_path = diff.old_path.map(|old| format!("{repo}/{old}"));
                }
                diff
            })
```

`git_stage` (:101-133):
- `let paths = match relative_paths(&params.paths).and_then(|paths| repo_paths(params.repo.as_deref(), paths)) {`
- In the hunk arm: `match relative_paths(std::slice::from_ref(&path)).and_then(|path| repo_paths(params.repo.as_deref(), path)) {`
- The `git_change` target becomes `(params.project_id, params.worktree_task_id, params.repo)`.

`params.hunk` is moved by the `match`; `params.repo` is a different field, so the partial move compiles.

`git_discard` (:142): `relative_paths(&params.paths).and_then(|paths| repo_paths(params.repo.as_deref(), paths))`; `let target = (params.project_id, params.worktree_task_id, params.repo);`.
`git_commit` (:173) and `git_remote` (:233): `let target = (params.project_id, params.worktree_task_id, params.repo);`.

`git_log` (:182-207): after `path` is computed add

```rust
        let path = match path.map(|path| repo_paths(params.repo.as_deref(), vec![path])) {
            None => None,
            Some(Ok(mut path)) => Some(path.remove(0)),
            Some(Err(outcome)) => return outcome,
        };
```

Then `.target(params.project_id, params.worktree_task_id, params.repo.as_deref())`. `Commit` has no paths, so nothing is prefixed.

`git_show` (:210-227): `.target(…, params.repo.as_deref())`, keep `let (dir, repo) = (target.dir, target.repo);` and

```rust
            git_view::show(&*git, &cache, &dir, &params.commit).map(|mut detail| {
                prefix_files(repo.as_deref(), &mut detail.files);
                detail
            })
```

`git_change` (:248-271): the parameter becomes `(project_id, worktree_task_id, repo): (Ulid, Option<Ulid>, Option<String>)` and the lookup `self.target(project_id, worktree_task_id, repo.as_deref())`. `GitChangedEvent` stays `{project_id, worktree_task_id}`: the UI refreshes the shown repo on any change of the project.

New handler after `git_set_base` (:289):

```rust
    /// `git.repos` (ADR 0029): `""` when the project folder is in a git repository, then each
    /// direct child folder holding `.git`. Read on every call: folders come and go.
    pub(super) async fn git_repos(&self, params: ProjectRef) -> Outcome {
        let target = match self.target(params.project_id, None, None).await {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let git = Arc::clone(&self.git);
        let repos = tokio::task::spawn_blocking(move || {
            let root = target.dir;
            let in_repo = matches!(git.repo(&root), Ok(Some(_)));
            let mut repos = files::child_repos(&root);
            if in_repo {
                repos.insert(0, String::new());
            }
            repos
        })
        .await;
        match repos {
            Ok(repos) => ok(&repos),
            Err(e) => Outcome::error(ErrorCode::Internal, format!("git.repos failed: {e}")),
        }
    }
```

`server.rs` dispatch, after `GitSetBase` (:388):

```rust
            Request::GitRepos(params) => (self.git_repos(params).await, After::Continue),
```

- [ ] **Step 5: Discovery and the watcher**

`crates/mkrew-daemon/src/files.rs`, after `markers` (:130):

```rust
/// The direct child folders of `root` that are git repositories of their own (ADR 0029): a
/// `.git` folder or file (worktree, submodule) inside. Sorted; hidden folders and
/// [`ALWAYS_IGNORED`] ones skipped. `.gitignore` is not asked: parents often ignore them.
// ponytail: depth 1 only; walk deeper (apps/web) when a user asks.
pub fn child_repos(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut repos: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.') && !ALWAYS_IGNORED.contains(&name.as_str()))
        .filter(|name| root.join(name).join(".git").exists())
        .collect();
    repos.sort();
    repos
}
```

`watch.rs` `is_git_state` (:92-108):

```rust
    /// `path` is a file of the root's `.git` dir, or of a direct child folder's (a repository
    /// of its own, ADR 0029), that moves when commits, branches or the index change (`HEAD`,
    /// `index`, `packed-refs`, `refs/…`): the git state changed even when no working file did.
    pub fn is_git_state(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        let in_child = || {
            let mut parts = relative.components();
            parts.next();
            parts.as_path().strip_prefix(".git").ok()
        };
        let Some(relative) = relative.strip_prefix(".git").ok().or_else(in_child) else {
            return false;
        };
        let mut parts = relative.components().map(|c| c.as_os_str());
        match parts.next().and_then(|p| p.to_str()) {
            Some("HEAD" | "index" | "packed-refs") => parts.next().is_none(),
            Some("refs") => parts.next().is_some(),
            _ => false,
        }
    }
```

Update the module doc of `Filter::new` (:29) only if it mentions `.git` (it does not; leave it). `Filter` still reads only `<root>/.gitignore`. So churn under a child repo's own ignored folders (`fe/dist`) is still reported as file changes; this is noted in ADR 0029 as a known limit.

Run: `cargo test -p mkrew-daemon --lib files::tests::child_repos -j 4`, then `cargo test -p mkrew-daemon --lib watch::tests::git_state -j 4`, then `cargo test -p mkrew-daemon --lib server::git::tests -j 4`
Expected: PASS (all three).

- [ ] **Step 6: Files view markers across child repositories**

`server/fs.rs`: add `use std::path::Path;` next to `use std::collections::HashMap;` (:5). Replace the marker lines in `fs_list` (:35-40) with:

```rust
            // Not a repo (or git failing): no markers, the tree still lists.
            let status_of = |dir: &Path| {
                cache
                    .repo(&*git, dir)
                    .ok()
                    .and_then(|repo| git.status(dir, &repo).ok())
            };
            let mut markers = status_of(&root).map_or_else(HashMap::new, |s| files::markers(&s));
            // Child repositories (ADR 0029): only the one the listed folder is in, or all of
            // them when the root is listed (its folder dots). One `git status` each.
            let top = folder.split('/').next().unwrap_or_default();
            for repo in files::child_repos(&root)
                .into_iter()
                .filter(|repo| top.is_empty() || repo == top)
            {
                if let Some(status) = status_of(&root.join(&repo)) {
                    let nested = files::markers(&status).into_iter();
                    markers.extend(nested.map(|(path, marker)| (format!("{repo}/{path}"), marker)));
                }
            }
```

`child_repos` reads the target dir. In a task worktree it finds no child repos (worktrees hold one repository), so nothing changes there.

- [ ] **Step 7: Failing → passing daemon integration test**

First add `repo: None,` after `worktree_task_id` in every existing `GitStatusParams`, `GitDiffParams`, `GitStageParams`, `GitDiscardParams`, `GitCommitParams`, `GitLogParams`, `GitShowParams`, `GitRemoteParams` and `AiGenerateParams` literal. In `crates/mkrew-daemon/tests/daemon.rs` they are today at :2327, :2343, :3059, :3067, :3078, :3086, :3168, :3204, :3215, :3221, :3231, :3287, :3308, :3337, :3372, :3380, :3407, :3434, :3441, :4489, :4503, :4514 and :4967 (:3332 spreads `..by_path`, no change). Also add it in `crates/mkrew-daemon/src/git_view.rs:608-611` (`fn params`) and in the UI literals listed under **Files**. Let `cargo check --workspace --all-targets -j 4` list any left.

New test after `git_log_show_push_and_fetch_with_a_local_remote`:

```rust
/// ADR 0029: a workspace folder that is no repository holds `fe/` and `be/` repositories.
/// `git.repos` finds them; `repo` routes each request; paths stay project-relative; a commit in
/// one leaves the other alone; the watcher follows a child's `.git`; fs.list marks both.
#[test]
fn git_requests_reach_child_repositories_with_project_relative_paths() {
    let env = Env::new();
    let root = env.repo();
    for name in ["fe", "be"] {
        let repo = root.join(name);
        std::fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        git(&repo, &["config", "core.autocrlf", "false"]);
        // The daemon's own `git commit` needs an identity in the repo.
        git(&repo, &["config", "user.name", "t"]);
        git(&repo, &["config", "user.email", "t@t"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.join("x.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "one"]);
        std::fs::write(repo.join("x.txt"), "one\ntwo\n").unwrap();
    }
    std::fs::create_dir(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/a.md"), "# A\n").unwrap();
    let (_daemon, mut client) = env.start();
    let id = client.add_project(&root).id;
    let repos: Vec<String> = client
        .call(Request::GitRepos(ProjectRef { project_id: id }))
        .into_result()
        .unwrap();
    assert_eq!(repos, ["be", "fe"]);
    let status_of = |repo: Option<&str>| {
        Request::GitStatus(GitStatusParams {
            project_id: id,
            worktree_task_id: None,
            repo: repo.map(str::to_owned),
            mode: GitMode::Working,
            since: None,
        })
    };
    let names = |files: &[mkrew_protocol::ChangedFile]| -> Vec<String> {
        files.iter().map(|f| f.path.clone()).collect()
    };
    let fe: GitStatus = client.call(status_of(Some("fe"))).into_result().unwrap();
    assert_eq!(names(&fe.unstaged), ["fe/x.txt"]);
    // The project folder itself is no repository.
    assert_eq!(
        error_code(client.call(status_of(None))),
        ErrorCode::InvalidState
    );
    let diff: FileDiff = client
        .call(Request::GitDiff(GitDiffParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some("fe".into()),
            path: "fe/x.txt".into(),
            mode: GitMode::Working,
            staged: false,
            commit: None,
            since: None,
        }))
        .into_result()
        .unwrap();
    assert_eq!((diff.path.as_str(), diff.added), ("fe/x.txt", 1));

    // Stage and commit in be: fe keeps its change and its history.
    let stage = |repo: &str, path: &str| {
        Request::GitStage(GitStageParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some(repo.into()),
            paths: vec![path.into()],
            hunk: None,
        })
    };
    let _: () = client.call(stage("be", "be/x.txt")).into_result().unwrap();
    let committed: GitCommitResult = client
        .call(Request::GitCommit(GitCommitParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some("be".into()),
            message: "two".into(),
        }))
        .into_result()
        .unwrap();
    assert_eq!(committed.commit, git(&root.join("be"), &["rev-parse", "HEAD"]));
    assert_eq!(git(&root.join("fe"), &["log", "-1", "--format=%s"]), "one");
    let fe: GitStatus = client.call(status_of(Some("fe"))).into_result().unwrap();
    assert!(fe.staged.is_empty());
    assert_eq!(names(&fe.unstaged), ["fe/x.txt"]);
    let shown: CommitDetail = client
        .call(Request::GitShow(GitShowParams {
            project_id: id,
            worktree_task_id: None,
            repo: Some("be".into()),
            commit: committed.commit.clone(),
        }))
        .into_result()
        .unwrap();
    assert_eq!(names(&shown.files), ["be/x.txt"]);

    // fs.list of the root: the repo folders carry the change dot, markers sit under fe/.
    let list = |client: &mut Client, dir: &str| -> Vec<FsEntry> {
        let request = Request::FsList(FsListParams {
            project_id: id,
            worktree_task_id: None,
            dir: dir.into(),
            show_ignored: false,
        });
        client.call(request).into_result().unwrap()
    };
    let top = list(&mut client, "");
    let dotted: Vec<&str> = top
        .iter()
        .filter(|e| e.has_changes)
        .map(|e| e.name.as_str())
        .collect();
    assert_eq!(dotted, ["fe"]);
    let in_fe = list(&mut client, "fe");
    let x = in_fe.iter().find(|e| e.path == "fe/x.txt").unwrap();
    assert_eq!(x.marker, Some(GitMarker::Modified));

    // `git.status` started the watcher: a commit in fe from a terminal is reported.
    std::thread::sleep(Duration::from_millis(500));
    client.call(Request::DaemonStatus);
    client.events.clear();
    git(&root.join("fe"), &["commit", "-q", "--allow-empty", "-m", "outside"]);
    client.wait_event(&Event::GitChanged(GitChangedEvent {
        project_id: id,
        worktree_task_id: None,
    }));

    // Bad repo names, paths outside the repo, repo + task worktree.
    for bad in [
        stage("../x", "x.txt"),
        stage("fe/.git", "fe/.git/x"),
        stage(".git", ".git/x"),
        stage("fe/src", "fe/src/a"),
        stage("fe", "be/x.txt"),
        stage("fe", "x.txt"),
        Request::GitStatus(GitStatusParams {
            project_id: id,
            worktree_task_id: Some(Ulid::new()),
            repo: Some("fe".into()),
            mode: GitMode::Working,
            since: None,
        }),
    ] {
        assert_eq!(error_code(client.call(bad)), ErrorCode::InvalidRequest);
    }
    client.stop();
}
```

`ProjectRef`, `CommitDetail`, `GitMarker`, `FsEntry`, `FsListParams` and `GitChangedEvent` are already in the test imports (daemon.rs:14-36).

Run: `cargo test -p mkrew-daemon --test daemon git_requests_reach_child_repositories -j 2`
Expected: PASS. Before Steps 3-6 the run fails to compile (`no field repo`, `no variant GitRepos`). Then run the touched existing tests: `cargo test -p mkrew-daemon --test daemon git_ -j 2` and `cargo test -p mkrew-daemon --test daemon fs_list -j 2` → PASS.

- [ ] **Step 8: ADR 0029**

Create `docs/decisions/0029-several-repos-per-workspace.md`:

```markdown
# 0029 — Several git repositories in one workspace

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

The inbox asks that a workspace folder holding a front end and a back end, each its own git
repository (`fe/.git`, `be/.git`) with no repository at the root, works in the Git view. Every
git request resolved one folder (`Daemon::target`) and git finds repositories upward only, so
such a root showed "Cannot read the git status", no Files markers, and a commit made in `fe/`
from a terminal never refreshed anything (the watcher only knew the root's `.git`).

## Options

1. Add `fe/` and `be/` as two workspaces — works today, but two sidebar entries and agents
   cannot span both folders; the inbox asks for one root.
2. A `repo` parameter on git requests plus discovery of the direct child repositories, and a
   repository chooser in the Git view — small; every git function already takes a folder.
3. A workspace setting listing repositories (migration, CLI, settings UI) — replaces a cheap
   directory scan and goes stale when folders change.
4. All repositories stacked in one Git view — commit, push, history and base are per
   repository anyway, so most of option 2 is still needed underneath; much larger.

## Decision

Option 2. `git.repos` lists `""` (the project folder, when it is in a repository) and each
direct child folder holding `.git` (folder or file), sorted, skipping hidden folders,
`node_modules` and `target`, not filtered by `.gitignore`, read on every call. `git.status`,
`git.diff`, `git.stage`/`unstage`, `git.discard`, `git.commit`, `git.log`, `git.show`,
`git.fetch`/`push` and `ai.generate` accept `repo?` (one folder name). Paths in requests and
replies stay project-relative (`fe/src/a.ts`): the daemon strips and adds `repo/`, so Open in
Zed, diff comments and "Changed by" work unchanged. The watcher also treats a direct child's
`.git/{HEAD,index,packed-refs,refs/…}` as git state. Files markers merge the child
repositories' status. The Git view shows a "Repo" chooser with two or more repositories and
picks the first one when the project folder is no repository.

## Consequences

- SPEC §7, §12A, §12B, M5, §20C.1 and §20E.1 updated; protocol version bumped.
- Not covered (unchanged behaviour): turn snapshots and turn restore (§20C), task checkpoints
  and Undo (M5), task worktrees, merge back and fan-out (§20E) still work on the project
  folder's repository only and are skipped or refused when it is none; `repo` together with
  `worktree_task_id` is `invalid_request`. `git.set_base` stays per project and applies to the
  shown repository (auto-detection works per repository). Repositories deeper than one level
  (`apps/web`) are not found. A root repository that also has nested ones lists them as
  untracked folders in its own status (no submodule awareness). A child repository's own
  `.gitignore` is not read by the watcher, so churn in its ignored folders still counts as
  file changes.
```

- [ ] **Step 9: SPEC**

- §7 table: after the `git.fetch / git.push` row (SPEC.md:538) add the row
  `| `git.repos` | `project_id` | `string[]`: `""` when the project folder is in a git repository, then each direct child folder holding `.git` (folder or file), sorted; hidden folders, `node_modules` and `target` skipped; not filtered by `.gitignore`; read on every call (ADR 0029) |`
  After the table (before `**Events**`, :541) add the paragraph: `` `git.status`, `git.diff`, `git.stage`/`git.unstage`, `git.discard`, `git.commit`, `git.log`, `git.show`, `git.fetch`/`git.push` and `ai.generate` accept `repo?`: one folder name from `git.repos`; the request runs in that child repository. Paths in requests and replies stay relative to the project folder (`fe/src/a.ts`); a path outside the repo, a bad name or `repo` together with `worktree_task_id` is `invalid_request`. `git.changed` has no repo: clients refresh the shown repo (ADR 0029). ``
- §12A (:797-799): after "a small dot on folders containing changes." add ` In a workspace with child repositories (ADR 0029) the markers come from the project folder's repository and from each child repository the listed folder is in (all of them when the root is listed).`
- §12B Header (:827): change the bullet's start to `- **Header:** with two or more repositories (`git.repos`, ADR 0029) a "Repo" chip row (the workspace name for the project folder, then the child folders; hidden while a task worktree is shown); then current branch, …`. Keep the rest of the bullet. Add a sentence at its end: ` When the project folder is no repository, the first child repository is shown.`
- M5 (:1145): append ` Only the project folder's repository counts (child repositories of ADR 0029 get no checkpoint).` to the first line of the bullet.
- §20C.1 (:1424): after "in a git repo," add ` (the project folder's repository only; child repositories of ADR 0029 are not snapshotted)`.
- §20E.1 (:1496-1497): after "Outside a git repo the task runs in the project folder, saying so in the first output line (ADR 0010)." add ` Child repositories (ADR 0029) are not isolated: a task worktree holds the project folder's repository only, and the Git view's repo chooser is hidden while a worktree is shown.`

- [ ] **Step 10: Verify**

Run: `cargo clippy -p mkrew-protocol -p mkrew-daemon --all-targets -j 4 -- -D warnings`
Expected: clean. Then run the full verification (`"C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`, fallback `cargo test --workspace -j 2`). Do not tick the inbox line yet; Task 9 does.
Suggested message: `feat(daemon): git requests reach child repositories of a workspace (ADR 0029)`

---

### Task 9: Git view — "Repo" chooser for workspaces with several repositories

Inbox: same line as Task 8 (`docs/INBOX.md` line 30 today). Spec: §12B header (as updated in Task 8 Step 9), ADR 0029.

**Current state (investigated):**
- `GitView` (`git_view.rs:426-472`) holds `project_id`, `worktree_task_id`, …; `GitView::new(project_id, worktree_task_id)` :475. Request builders:
  - `begin_status` :507-521 (`GitStatusParams`)
  - `begin_log` :934-947
  - `show_params` :986-992
  - `file_change` :1206-1221 and `hunk_change` :1225-1247 (`GitStageParams`)
  - `discard` :1284-1302 (`Discard {project_id, worktree_task_id, …}` :349, `Discard::params` :379-386)
  - `Target::params(project_id, worktree_task_id)` :64-79 (`GitDiffParams`)
- `apply` (:1135-1147) refreshes on any `git.changed` of the project with the same `worktree_task_id`. That is already right for a repo: the event has no repo field.
- `git_actions.rs`: `CommitBox::begin` builds `GitCommitParams` with `worktree_task_id: None` (:80-84), and `commit_git` (main.rs:2534) fills it in. `Remote::request(project_id, worktree_task_id)` :151-160 has a test at :366-383. The test literal at :268 needs the new field.
- `main.rs`:
  - `Shell.git: HashMap<Ulid, GitView>` :310.
  - `git_view()` :1952.
  - `git_at(project_id, target)` :1960-1964, with callers :2046, :2076, :2140, :2178, :2234, :2383, :2545, :2625.
  - `set_git_target` :1968-1978.
  - `retarget_git(project_id, target)` :1982-1994 (fresh view, keeps the commit draft), with callers :1944, :1975, :4763, :4965, :5168.
  - `refresh_git` :1997-2000 is called when the view is shown (`show_view` :1387, `activate_workspace` :1092) and after commit, discard, fetch/push and set_base.
  - `generate_commit_message` builds `AiGenerateParams` at :2571-2578.
  - `run_git_remote` :2598-2633.
  - `render_git` :9160 shows "Cannot read the git status." + Retry for `Status::Failed` (:9248-9256).
  - `git_header` :9411-9620 renders `chooser` (:9469, `.children(chooser)` :9520) above the branch title.
  - `target_chooser` :9624-9691 is the chip styling to copy (theme tokens `space::S/XS`, `radius::SMALL`, `font_size::META`, `weight::MEDIUM`, `opacity::ACTIVE`, one `.hover` per chip).
  - Workspace names come from `self.sidebar.projects()` (e.g. :971).
- GPUI: `ElementId: From<(&'static str, usize)>` (gpui-pre-0.3.7 window.rs:7500).

**Decision (draft per repo):** switching repo drops nothing silently. `retarget_git` already carries the draft over to the fresh view, and that stays: one draft per workspace, carried across repos the same way it is across worktree targets. Keeping a draft per repo would add a map for a rare case. See Open questions.

**Files:**
- Modify: `crates/mkrew-ui/src/git_view.rs` (field + `new` :426-503, builders :64-79, :349-386, :507-521, :934-947, :986-992, :1206-1247, :1284-1302, new pure fns after `CHANGED_BY_WINDOW_MS` :403, tests)
- Modify: `crates/mkrew-ui/src/git_actions.rs` (:80-84, :151-160, tests :268, :366-383)
- Modify: `crates/mkrew-ui/src/main.rs` (field :310 + init :638, :1944, :1960-2000, :2035-2046, :2067-2076, :2132-2140, :2165-2178, :2213-2235, :2371-2383, :2528-2545, :2571-2578, :2612-2625, :4763, :4965, :5168, `git_header` :9469-9520, new `repo_chooser` after `target_chooser` :9691)
- Modify: `docs/HUMAN_CHECKS.md` (new section under "To verify (non-blocking)", :13)

**Interfaces:**
- Consumes (Task 8): `repo: Option<String>` on the Git*Params and `AiGenerateParams`; `Request::GitRepos(ProjectRef)` → `Vec<String>`.
- Produces:
  - `GitView.repo: Option<String>` (pub)
  - `GitView::at(&self) -> (Option<Ulid>, Option<String>)`
  - `git_view::repo_choices<'a>(repos: &'a [String], workspace: &'a str) -> Vec<(Option<String>, &'a str)>`
  - `git_view::pick_repo(shown: Option<&str>, repos: &[String]) -> Option<String>`
  - `Discard.repo: Option<String>`
  - `Target::params(&self, project_id, worktree_task_id, repo: Option<String>) -> GitDiffParams`
  - `Remote::request(self, project_id, worktree_task_id, repo: Option<String>) -> Request`
  - `Shell.git_repos: HashMap<Ulid, Vec<String>>`
  - `Shell::git_at(&mut self, project_id, at: &(Option<Ulid>, Option<String>))`
  - `Shell::retarget_git(&mut self, project_id, target: Option<Ulid>, repo: Option<String>) -> bool`
  - `Shell::set_git_repo`, `Shell::load_git_repos`, `Shell::repo_chooser`

- [ ] **Step 1: Failing tests for the pure helpers**

In `git_view.rs` tests:

```rust
    fn repos(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn repo_chips_show_with_two_repositories_and_name_the_workspace_folder() {
        assert_eq!(repo_choices(&repos(&[]), "EA"), vec![]);
        // One repository: nothing to choose.
        assert_eq!(repo_choices(&repos(&["fe"]), "EA"), vec![]);
        assert_eq!(repo_choices(&repos(&[""]), "EA"), vec![]);
        assert_eq!(
            repo_choices(&repos(&["", "be", "fe"]), "EA"),
            vec![
                (None, "EA"),
                (Some("be".to_owned()), "be"),
                (Some("fe".to_owned()), "fe"),
            ]
        );
    }

    #[test]
    fn the_shown_repo_is_kept_while_listed_else_the_default_is_picked() {
        // The workspace folder is no repository: the first child one, not an error.
        assert_eq!(pick_repo(None, &repos(&["be", "fe"])), Some("be".to_owned()));
        // Kept when the list refreshes.
        assert_eq!(pick_repo(Some("fe"), &repos(&["be", "fe"])), Some("fe".to_owned()));
        // Gone (folder removed or renamed): back to the default.
        assert_eq!(pick_repo(Some("fe"), &repos(&["", "be"])), None);
        assert_eq!(pick_repo(Some("fe"), &repos(&["be"])), Some("be".to_owned()));
        // The workspace folder is a repository: it stays the default.
        assert_eq!(pick_repo(None, &repos(&["", "fe"])), None);
        // No repository at all: the workspace folder (its error says why).
        assert_eq!(pick_repo(Some("fe"), &repos(&[])), None);
    }
```

Run: `cargo test -p mkrew-ui --lib git_view::tests::repo_chips -j 4`
Expected: FAIL to compile (`cannot find function repo_choices`).

- [ ] **Step 2: Pure helpers and the `repo` of the view**

In `git_view.rs` after `CHANGED_BY_WINDOW_MS` (:403):

```rust
/// The "Repo" chips (ADR 0029): `(repo, label)` per repository of `git.repos`, `""` (the
/// workspace folder) labelled `workspace`; none with fewer than two (the row is hidden).
pub fn repo_choices<'a>(repos: &'a [String], workspace: &'a str) -> Vec<(Option<String>, &'a str)> {
    if repos.len() < 2 {
        return Vec::new();
    }
    repos
        .iter()
        .map(|repo| match repo.as_str() {
            "" => (None, workspace),
            name => (Some(name.to_owned()), name),
        })
        .collect()
}

/// The repository to show once `git.repos` answered: the shown one while it is listed, else
/// the workspace folder when it is one (listed first as `""`), else the first child one.
/// `None` = the workspace folder.
pub fn pick_repo(shown: Option<&str>, repos: &[String]) -> Option<String> {
    if let Some(shown) = shown
        && repos.iter().any(|repo| repo == shown)
    {
        return Some(shown.to_owned());
    }
    repos.first().filter(|first| !first.is_empty()).cloned()
}
```

`GitView`: add after `worktree_task_id` (:429)

```rust
    /// The child repository shown (ADR 0029); `None` = the workspace folder's. Set by
    /// `Shell::retarget_git` only: a repo change is a fresh view.
    pub repo: Option<String>,
```

…and `repo: None,` in `GitView::new` (:478). Keep `new`'s signature: `retarget_git` sets the field, so the ~10 test calls of `GitView::new` stay as they are. Add:

```rust
    /// What the view shows: the task worktree and the repository (late answers for another
    /// are dropped).
    pub fn at(&self) -> (Option<Ulid>, Option<String>) {
        (self.worktree_task_id, self.repo.clone())
    }
```

Replace Task 8's `repo: None,` with `repo: self.repo.clone(),` in `begin_status` (:517), `begin_log` (:943), `show_params` (:989), `file_change` (:1213), `hunk_change` (:1239) and the `Discard` literal in `discard` (:1296; after the new field below exists).

`Discard` (:349-357): add `pub repo: Option<String>,` after `worktree_task_id`; `Discard::params` (:380-383): `repo: self.repo.clone(),`.

`Target::params` (:64): `pub fn params(&self, project_id: Ulid, worktree_task_id: Option<Ulid>, repo: Option<String>) -> GitDiffParams` with `repo,` in the literal. Update its doc: `/// The `git.diff` showing this file (of a task worktree, if `worktree_task_id`; of child repository `repo`).` The test call at :1873 becomes `staged.params(project_id, task_id, None)`.

`git_actions.rs`: `CommitBox::begin` (:80-84) keeps Task 8's `repo: None,` (the caller fills it in, as with the worktree), and so does the test literal at :268.
`Remote::request` (:151-160):

```rust
    /// The request on the project folder, task worktree `worktree_task_id` or child
    /// repository `repo`.
    pub fn request(
        self,
        project_id: Ulid,
        worktree_task_id: Option<Ulid>,
        repo: Option<String>,
    ) -> Request {
        let params = GitRemoteParams {
            project_id,
            worktree_task_id,
            repo,
        };
        match self {
            Remote::Fetch => Request::GitFetch(params),
            Remote::Push => Request::GitPush(params),
        }
    }
```

Test :366-383: `Remote::Fetch.request(project_id, None, None)` with `repo: None` in the expected literal, and `Remote::Push.request(project_id, worktree_task_id, None)` with `repo: None`.

Run: `cargo test -p mkrew-ui --lib git_view::tests -j 4` and `cargo test -p mkrew-ui --lib git_actions::tests -j 4`
Expected: PASS.

- [ ] **Step 3: Shell — target the shown repo, drop late answers for another**

Field (after `git` :310) and init (after :638):

```rust
    /// The `git.repos` of each workspace whose Git view was shown (ADR 0029), for the "Repo"
    /// chooser; kept in memory only.
    git_repos: HashMap<Ulid, Vec<String>>,
```

```rust
            git_repos: HashMap::new(),
```

Replace `git_at` and `retarget_git` (:1958-1964, :1980-1994):

```rust
    /// The Git view of `project_id` while it still shows `at` (task worktree, repository; an
    /// answer sent for another is dropped).
    fn git_at(
        &mut self,
        project_id: Ulid,
        at: &(Option<Ulid>, Option<String>),
    ) -> Option<&mut GitView> {
        self.git
            .get_mut(&project_id)
            .filter(|view| view.worktree_task_id == at.0 && view.repo == at.1)
    }
```

```rust
    /// Puts a fresh Git view of `target` / `repo` in place (the commit draft is kept); `false`
    /// = it shows them already.
    fn retarget_git(&mut self, project_id: Ulid, target: Option<Ulid>, repo: Option<String>) -> bool {
        let old = self.git_view(project_id);
        if old.worktree_task_id == target && old.repo == repo {
            return false;
        }
        let draft = std::mem::take(&mut old.commit.text);
        let mut view = GitView::new(project_id, target);
        view.repo = repo;
        view.commit.text = draft;
        self.git.insert(project_id, view);
        self.git_files_scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.git_diff_scroll.scroll_to_item(0, ScrollStrategy::Top);
        true
    }
```

Callers of `retarget_git` pass `None` as `repo`: :1944, :1975 (in `set_git_target`; a worktree chip, or "Project", resets the repo and `git.repos` picks again), :4763, :4965, :5168.

After `set_git_target`, add:

```rust
    /// The "Repo" chooser picked `repo` (ADR 0029): the Git view starts over on it.
    fn set_git_repo(
        &mut self,
        project_id: Ulid,
        repo: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.retarget_git(project_id, None, repo) {
            self.refresh_git(project_id, window, cx);
        }
    }
```

`refresh_git` (:1997-2000) also asks for the repositories. Every way the view is shown (show_view, activate_workspace, Retry) goes through it:

```rust
    /// Asks the Git view of `project_id` for its repositories, status and shown diff again.
    fn refresh_git(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.load_git_repos(project_id, window, cx);
        let requests = self.git_view(project_id).refresh();
        self.git_requests(project_id, requests, window, cx);
    }

    /// `git.repos` of `project_id` (ADR 0029): the "Repo" chips; a shown repository that is
    /// gone, or a workspace folder that is no repository, moves the view to [`git_view::pick_repo`]
    /// (never while a task worktree is shown: the daemon refuses a repo there).
    fn load_git_repos(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let listed = connection.call::<Vec<String>>(Request::GitRepos(ProjectRef { project_id }));
        cx.spawn_in(window, async move |this, cx| {
            let repos = match listed.await {
                Ok(repos) => repos,
                Err(error) => {
                    tracing::warn!(%project_id, "git.repos failed: {error:#}");
                    return;
                }
            };
            let _ = this.update_in(cx, |shell, window, cx| {
                let view = shell.git_view(project_id);
                let worktree = view.worktree_task_id;
                let picked = git_view::pick_repo(view.repo.as_deref(), &repos);
                shell.git_repos.insert(project_id, repos);
                // ponytail: the first status of a non-repo root may fail before this answer
                // and flash "Cannot read the git status"; send status after git.repos if it shows.
                if worktree.is_none() && shell.retarget_git(project_id, None, picked) {
                    let requests = shell.git_view(project_id).refresh();
                    shell.git_requests(project_id, requests, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
```

`ProjectRef` is already imported in main.rs (:29).

Late answers: every request site keeps the view's `at` and passes `&at` to `git_at`:
- `load_git_log` (:2038): `let at = (params.worktree_task_id, params.repo.clone());` and :2046 `shell.git_at(project_id, &at)`.
- `load_git_show` (:2068 / :2076): same with `params`.
- `load_git_status` (:2132 / :2140): same with `params`.
- `load_session_files` (:2166 / :2178): `let at = view.at();` → `shell.git_at(project_id, &at)`.
- `load_git_diff` (:2217-2235):
  - `let at = view.at();`
  - `connection.call::<FileDiff>(Request::GitDiff(target.params(project_id, at.0, at.1.clone())))`
  - `.git_at(project_id, &at)`
- `stage_git` (:2371 / :2383): `let at = (change.params.worktree_task_id, change.params.repo.clone());` (before `change.request()` consumes `change`).
- `commit_git` (:2528-2545):
  - `let at = view.at();`
  - replace `params.worktree_task_id = target;` (:2534) with `(params.worktree_task_id, params.repo) = at.clone();`
  - `shell.git_at(project_id, &at)`.
- `run_git_remote` (:2612-2625): `let at = view.at();`, `remote.request(project_id, at.0, at.1.clone())`, `shell.git_at(project_id, &at)`.
- `generate_commit_message` (:2574): Task 8's `repo: None,` becomes `repo: view.repo.clone(),`. The ✦ message then describes the shown repo's staged diff.

Every one of these closures is `async move`. `at` moves in as `target` did; `at.clone()` is only needed where it is used twice before the move (commit_git).

- [ ] **Step 4: The "Repo" chips in the header**

After `target_chooser` (:9691):

```rust
    /// "Repo: <workspace> | fe | be" (ADR 0029): which repository of the workspace the Git view
    /// shows; hidden with fewer than two and while a task worktree is shown (one repository).
    fn repo_chooser(
        &self,
        project_id: Ulid,
        view: &GitView,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::Div> {
        if view.worktree_task_id.is_some() {
            return None;
        }
        let repos = self.git_repos.get(&project_id)?;
        let workspace = self
            .sidebar
            .projects()
            .iter()
            .find(|p| p.id == project_id)
            .map_or("Workspace", |p| p.name.as_str());
        let choices = git_view::repo_choices(repos, workspace);
        if choices.is_empty() {
            return None;
        }
        let hover = theme.surface;
        let chips = choices.into_iter().enumerate().map(|(at, (repo, label))| {
            let active = view.repo == repo;
            let tip = match &repo {
                Some(repo) => format!("The git repository in {repo}/"),
                None => "The workspace folder's git repository".to_owned(),
            };
            div()
                .id(("git-repo", at))
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap()
                .px(px(space::S))
                .py(px(space::XS))
                .rounded(px(radius::SMALL))
                .tooltip(tooltip(theme, &tip, ""))
                .when(active, |el| {
                    el.bg(rgb(theme.surface_raised))
                        .font_weight(FontWeight(weight::MEDIUM))
                        .text_color(rgb(theme.text))
                })
                .when(!active, |el| {
                    el.cursor_pointer()
                        .text_color(rgb(theme.text_muted))
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            window.focus(&shell.git_focus, cx);
                            shell.set_git_repo(project_id, repo.clone(), window, cx);
                        }))
                })
                .child(label.to_owned())
        });
        Some(
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(space::XS))
                .text_size(font_size::META)
                .child(div().text_color(rgb(theme.text_muted)).child("Repo"))
                .children(chips),
        )
    }
```

In `git_header`, after `let chooser = …` (:9469) add `let repos = self.repo_chooser(project_id, view, theme, cx);`, and after `.children(chooser)` (:9520) add `.children(repos)`. The two chooser rows never show together (worktree shown → repo row hidden). With only the repo row, it sits where the worktree row would.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean (no "unused" `git_view::pick_repo`/`repo_choices`, no second `.hover` on an element).

- [ ] **Step 5: HUMAN_CHECKS**

Under `## To verify (non-blocking)` (HUMAN_CHECKS.md:13), before the first `### To verify —` section (:18), add:

```markdown
### To verify — 2026-10-09 git: several repositories per workspace (ADR 0029)

Restart the daemon first (the protocol version changed).

- [ ] **Repo chooser:** add a workspace whose folder is no repository and holds `fe/` and `be/`,
  each `git init` with a commit and an edited file. Git view → no "Cannot read the git status"
  (at most a blink): the header shows "Repo  be  fe" with `be` active and lists `be/…` files.
  Click `fe` → its branch, files and diff; the commit draft stays. With the workspace folder
  itself a repository too, the first chip is the workspace name. A workspace with one repository
  shows no Repo row. With a task worktree shown, the Repo row is hidden. Dark + light.
- [ ] **Stage / commit per repository:** in `fe` stage a file and a hunk, Discard another,
  ✦ writes a message about `fe` only, Ctrl+Enter commits → `git -C fe log -1` has it, `be`
  unchanged. History lists `fe`'s commits only; Open in Zed opens `fe/<file>`; a diff comment
  lands on `fe/<file>`. Fetch / Push act on `fe` (its upstream shown).
- [ ] **Terminal commit refreshes:** with `fe` shown, run `git -C fe commit --allow-empty -m x`
  in a terminal → the History / ahead count updates within ~1 s without clicking.
- [ ] **Files markers:** the Files tree shows the change dot on `fe` and `be` and M/A/? letters
  inside them; a plain folder next to them has none.
```

- [ ] **Step 6: Verify and tick**

Run the full verification (`"C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`, fallback `cargo test --workspace -j 2`). Then tick the inbox line `Git module: also need support a case where I open an Root folter container FE and BE…` (`docs/INBOX.md`, line 30 today; the brief said 104) `[x]` once the Step 5 "Repo chooser" and "Stage / commit per repository" checks pass. Until then mark it `[~]`.
Suggested message: `feat(ui): Git view repo chooser for workspaces holding several repositories`

---

### Task 10: Re-attached panes scroll back into the daemon's history (snapshot carries history)

Inbox: line 33 `in terminal agent, sometime I cannot scroll on top over 1 page showing in the screen, please fix`. Spec: §7 Screen snapshot (:569-576), §8 headless emulator (:595-599), §10 Terminal rendering (:695-703).

**Current state (investigated):**
- `crates/mkrew-ui/src/main.rs:3662-3690` `sync_panes`: every wanted session that is not on screen is detached, and `self.panes.remove(&session)` drops its `TerminalPane`, so the UI emulator and its 10,000-line history go with it. This runs on a tab or workspace switch, Board/Docs/Git and back (:1392), minimize (:397), zoom (:6226, :6244), tab changes (:6408, :6538, :6603), `layout_changed` (:7111) and the tab click (:13338). That is the "sometimes".
- `main.rs:3726-3772` `attach` builds the new pane with `TerminalPane::new(session, &snapshot, …)` → `crates/mkrew-ui/src/terminal/view.rs:80-90` → `Emulator::from_snapshot` (`terminal/emulator.rs:130-137`), which is `Self::new` + `advance(paint(snapshot))`. The second trigger is `view.rs:170-175` `load_snapshot` (the daemon's 4 MiB queue overflowed, `crates/mkrew-daemon/src/attach.rs:18`), which does the same.
- `crates/mkrew-daemon/src/emulator.rs:141-196` `snapshot()` reads only `0..screen_lines()`, so the snapshot has no history, although the daemon keeps 200 lines (`SCROLLBACK_LINES`, :16-17). `crates/mkrew-protocol/src/message.rs:1204-1213` defines `ScreenSnapshot { cols, rows, lines, cursor, modes }`; rows are `Vec<CellRun>` (`CellRun` :1215-1224).
- `crates/mkrew-protocol/src/paint.rs:10-49` `paint` writes `ESC[0m`, `?1049h/l`, `ESC[H ESC[2J`, then each non-empty row at an absolute `ESC[row;1H`, then modes and cursor. Callers: UI `from_snapshot` and `mk attach` (`crates/mkrew-cli/src/attach.rs:65, :192`).
- alacritty 0.26 behavior this task relies on (registry `alacritty_terminal-0.26.0`):
  - `term/mod.rs:1423-1433` `linefeed` on the last row calls `scroll_up(1)`. With the scroll region at the top, `grid/mod.rs:252-301` `scroll_up` grows the history (`increase_scroll_limit`) and rotates the top row into it.
  - `term/mod.rs:1788-1801` ED2 on the main screen runs `grid/mod.rs:309-333` `clear_viewport`, which pushes rows down to the last non-blank one into the history. A fresh grid gets one blank history row (this already happens today). The tests below never depend on it.
  - `term/mod.rs:416` builds the alternate grid with 0 history, and `swap_alt` (:714-731) only swaps the grids. On the alternate screen `topmost_line()` is therefore `Line(0)`, and the main-screen history is out of reach (it sits in the private `inactive_grid`).
  - `Dimensions::topmost_line()` = `Line(-(history_size))` (`grid/mod.rs:503-506`). `history_size` counts only initialized rows (`storage.rs:137-142`). It is implemented for `Term` (`term/mod.rs:1042`) and already imported in the daemon emulator (:8).
- `PROTOCOL_VERSION` is 26 today (`crates/mkrew-protocol/src/lib.rs:16`). These `ScreenSnapshot` literals must get the new field: `message.rs:1996` (`snapshot_wire_shape`), `paint.rs:103` (`paints_runs_cursor_and_modes`), `crates/mkrew-ui/src/router.rs:197` (test helper `snapshot`), `crates/mkrew-ui/src/terminal/emulator.rs:696` (`snapshot_restores_screen_cursor_and_modes_then_output_continues`), `crates/mkrew-ui/tests/terminal_runs.rs:63` (`snapshot_and_output_batch_into_runs`). The daemon builds a snapshot only at `emulator.rs:177`.

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs:1204-1213` (`ScreenSnapshot`), test `snapshot_wire_shape` (:1995-2039)
- Modify: `crates/mkrew-protocol/src/lib.rs:16` (`PROTOCOL_VERSION`)
- Modify: `crates/mkrew-protocol/src/paint.rs:18-29` (`paint`), after `sgr` (:73) (new `push_runs`), tests after `paints_runs_cursor_and_modes` (:101-143)
- Modify: `crates/mkrew-daemon/src/emulator.rs:16` (comment), `:140-196` (`snapshot`, new `runs`), tests after `scrolled_output_keeps_the_visible_screen` (:383-387)
- Modify: `crates/mkrew-ui/src/terminal/emulator.rs:128-129` (doc of `from_snapshot`), tests after `snapshot_restores_screen_cursor_and_modes_then_output_continues` (:694-734)
- Modify: `crates/mkrew-ui/src/terminal/view.rs:168-169` (doc of `load_snapshot`)
- Modify (each literal gets `history: vec![]`): `crates/mkrew-ui/src/router.rs:197-207`, `crates/mkrew-ui/tests/terminal_runs.rs:63-104`
- Create: `docs/decisions/0030-snapshot-carries-history.md`
- Modify: `docs/SPEC.md` §7 (:569-576), §8 (:595-597), §10 (:702-703); `docs/HUMAN_CHECKS.md` (2026-10-09 section)

**Interfaces:**
- Produces: `mkrew_protocol::ScreenSnapshot::history: Vec<Vec<CellRun>>`, with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`. It holds the daemon emulator's main-screen history, oldest first, at most `SCROLLBACK_LINES` = 200 rows, and it is empty on the alternate screen.
- Produces: `paint(&ScreenSnapshot) -> String` keeps its signature. It now scrolls `history` into the receiving terminal's scrollback before it paints the screen.
- Produces (private to the daemon): `Emulator::runs(&self, line: Line) -> Vec<CellRun>`.
- Consumes: `alacritty_terminal::grid::Dimensions::topmost_line`.

- [ ] **Step 1: Add the field and bump the protocol (compiles, no behavior yet)**

`message.rs`, replace the struct (:1204-1213):

```rust
/// The visible screen of a session's headless emulator plus the history above it (SPEC §7, §8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSnapshot {
    pub cols: u16,
    pub rows: u16,
    /// One entry per visible row, top to bottom. Trailing default blanks are omitted.
    pub lines: Vec<Vec<CellRun>>,
    /// The main screen's scrollback, oldest first, same row format as `lines` (at most the
    /// daemon's 200 lines). Empty on the alternate screen, which has no history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Vec<CellRun>>,
    pub cursor: CursorSnapshot,
    pub modes: TermModes,
}
```

`lib.rs:16`: bump `PROTOCOL_VERSION` by one from its value when this task lands. (The change is additive and deserializes with old peers, but `paint` behaves differently and the daemon and UI ship together, so bump anyway.)

Add `history: vec![],` after the `lines` field in each of these literals: `message.rs:1996`, `paint.rs:103`, `router.rs:197`, `terminal/emulator.rs:696`, `tests/terminal_runs.rs:63`. In the daemon's `emulator.rs` `snapshot()`, add `history: Vec::new(),` after `lines,` (Step 5 replaces it).

Run: `cargo check -p mkrew-protocol -p mkrew-daemon -p mkrew-ui -p mkrew-cli --all-targets -j 4`
Expected: compiles.

- [ ] **Step 2: Failing UI test: a pane rebuilt from a snapshot can scroll into its history**

`crates/mkrew-ui/src/terminal/emulator.rs` `mod tests`, after `snapshot_restores_screen_cursor_and_modes_then_output_continues`:

```rust
    /// A pane re-attached after a tab switch (or an overflow resnapshot) scrolls back into the
    /// daemon's history; the live screen and the output that follows are unchanged (ADR 0030).
    #[test]
    fn snapshot_history_is_scrollable() {
        let line = |text: &str| {
            vec![CellRun {
                text: text.into(),
                fg: TermColor::Default,
                bg: TermColor::Default,
                flags: 0,
            }]
        };
        let mut snapshot = ScreenSnapshot {
            cols: 10,
            rows: 3,
            lines: vec![line("$ ls"), line("a.txt"), vec![]],
            history: vec![],
            cursor: CursorSnapshot {
                row: 2,
                col: 0,
                visible: true,
            },
            modes: TermModes::default(),
        };
        let screen = Emulator::from_snapshot(&snapshot).grid();
        snapshot.history = (0..5).map(|n| line(&format!("old{n}"))).collect();

        let mut emu = Emulator::from_snapshot(&snapshot);
        assert_eq!(emu.grid(), screen, "the live screen is the same with history");
        // The history sits right above the screen, in order.
        emu.scroll_lines(3);
        assert_eq!(emu.grid().to_text(), "old2\nold3\nold4\n");
        emu.scroll_lines(1_000_000);
        let top = emu.grid().to_text();
        assert!(top.contains("old0\nold1\n"), "{top}");
        // Output continues on the live screen.
        emu.scroll_to_bottom();
        emu.advance(b"b.txt");
        assert_eq!(emu.grid().to_text(), "$ ls\na.txt\nb.txt\n");
    }
```

Run: `cargo test -p mkrew-ui --lib terminal::emulator::tests::snapshot_history_is_scrollable -j 4`
Expected: FAIL at `"old2\nold3\nold4\n"`. `paint` ignores `history`, so the only history row is ED2's blank one and the view shows `\n$ ls\na.txt\n`.

- [ ] **Step 3: `paint` scrolls the history into scrollback**

`paint.rs`: keep `out.push_str("\x1b[H\x1b[2J");` (:18), then replace the row loop (:19-29) with:

```rust
    // History: write each line from the top and let line feeds scroll it off the screen, so the
    // receiving terminal keeps it as scrollback. `rows - 1` extra feeds push the last written
    // line out too; the screen is blank again for the absolute rows below. The alternate
    // screen has no scrollback (the daemon sends none there; skip it anyway).
    if !modes.alt_screen && !snapshot.history.is_empty() {
        for runs in &snapshot.history {
            push_runs(&mut out, runs);
            out.push_str("\r\n");
        }
        for _ in 1..snapshot.rows {
            out.push('\n');
        }
    }
    for (row, runs) in snapshot.lines.iter().enumerate() {
        if runs.is_empty() {
            continue;
        }
        let _ = write!(out, "\x1b[{};1H", row + 1);
        push_runs(&mut out, runs);
    }
```

After `fn sgr` (:55-73) add:

```rust
/// One row's styled runs, then the style reset.
fn push_runs(out: &mut String, runs: &[CellRun]) {
    for run in runs {
        sgr(out, run);
        out.push_str(&run.text);
    }
    out.push_str("\x1b[0m");
}
```

Line counting, checked against alacritty's `linefeed`: the cursor starts on row 0. `h` lines that each end in `\r\n`, plus `rows - 1` more `\n`, make `h + rows - 1` feeds. The first `rows - 1` feeds only move the cursor down, so exactly `h` rows scroll into the history, and every written line ends up above the screen. A line that fills the last column leaves a pending wrap, and the `\r` cancels it (`carriage_return` resets `input_needs_wrap`, term/mod.rs:1412-1418), so the line does not take two rows.

`paint.rs` `mod tests`, after `paints_runs_cursor_and_modes`:

```rust
    /// History goes before the screen and only on the main screen; it never changes how the
    /// screen itself is painted.
    #[test]
    fn history_is_painted_only_on_the_main_screen() {
        let mut snapshot = ScreenSnapshot {
            cols: 20,
            rows: 3,
            lines: vec![vec![run("$ ", TermColor::Default, TermColor::Default, 0)]],
            history: vec![],
            cursor: CursorSnapshot {
                row: 0,
                col: 2,
                visible: true,
            },
            modes: TermModes::default(),
        };
        let without = paint(&snapshot);
        snapshot.history = vec![
            vec![run("old", TermColor::Indexed(1), TermColor::Default, 0)],
            vec![],
        ];
        // Two history lines, then rows - 1 = 2 feeds, then the same screen as before.
        assert_eq!(
            paint(&snapshot),
            without.replacen(
                "\x1b[2J",
                "\x1b[2J\x1b[0;31mold\x1b[0m\r\n\x1b[0m\r\n\n\n",
                1
            )
        );
        snapshot.modes.alt_screen = true;
        let alt = paint(&snapshot);
        snapshot.history.clear();
        assert_eq!(alt, paint(&snapshot));
    }
```

Run: `cargo test -p mkrew-protocol --lib paint -j 4`, `cargo test -p mkrew-ui --lib terminal::emulator -j 4` and `cargo test -p mkrew-ui --test terminal_runs -j 4`
Expected: PASS. `paints_runs_cursor_and_modes` and the `terminal_runs` `.snap` stay byte-identical because they have no history.

- [ ] **Step 4: Failing daemon test: the snapshot carries the main-screen history**

`crates/mkrew-daemon/src/emulator.rs` `mod tests`, after `scrolled_output_keeps_the_visible_screen`:

```rust
    /// Attach snapshots carry what scrolled off the main screen, oldest first; the alternate
    /// screen has none (ADR 0030).
    #[test]
    fn snapshot_carries_main_screen_history() {
        let text = |rows: &[Vec<CellRun>]| -> Vec<String> {
            rows.iter()
                .map(|runs| runs.iter().map(|run| run.text.as_str()).collect())
                .collect()
        };
        let mut emu = Emulator::new(10, 2);
        for n in 0..10 {
            emu.advance(format!("line{n}\r\n").as_bytes());
        }
        let snap = emu.snapshot();
        // 10 lines + the cursor's empty row on a 2-row screen: 9 scrolled off.
        assert_eq!(
            text(&snap.history),
            (0..9).map(|n| format!("line{n}")).collect::<Vec<_>>()
        );
        assert_eq!(text(&snap.lines), ["line9", ""]);

        emu.advance(b"\x1b[?1049halt");
        let snap = emu.snapshot();
        assert!(snap.history.is_empty(), "{:?}", snap.history);
        assert_eq!(text(&snap.lines), ["alt", ""]);
        // Leaving the alternate screen brings the history back.
        emu.advance(b"\x1b[?1049l");
        assert_eq!(emu.snapshot().history.len(), 9);
    }
```

Run: `cargo test -p mkrew-daemon --lib emulator::tests::snapshot_carries_main_screen_history -j 4`
Expected: FAIL (`left: []`, the history is always empty).

- [ ] **Step 5: Daemon `snapshot()` reads the history rows with one row helper**

`emulator.rs:16`, replace the comment:

```rust
/// Small on purpose: long history lives in the session log file (SPEC §8). Attach snapshots
/// carry all of it, so a re-attached pane can scroll back (ADR 0030).
```

Replace `snapshot` (:140-196) with:

```rust
    /// One grid row (screen row ≥ 0, history row < 0) as style runs: wide-char spacers skipped,
    /// trailing default blanks trimmed.
    fn runs(&self, line: Line) -> Vec<CellRun> {
        let row = &self.term.grid()[line];
        let mut runs: Vec<CellRun> = Vec::new();
        for col in 0..self.term.columns() {
            let cell = &row[Column(col)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            let (fg, bg, flags) = (color(cell.fg), color(cell.bg), run_flags(cell.flags));
            match runs.last_mut() {
                Some(run) if (run.fg, run.bg, run.flags) == (fg, bg, flags) => {
                    run.text.push(cell.c);
                }
                _ => runs.push(CellRun {
                    text: cell.c.into(),
                    fg,
                    bg,
                    flags,
                }),
            }
            if let (Some(run), Some(zerowidth)) = (runs.last_mut(), cell.zerowidth()) {
                run.text.extend(zerowidth);
            }
        }
        trim_trailing_blanks(&mut runs);
        runs
    }

    /// The visible screen, the history above it, the cursor and the input modes
    /// (`session.attach`, SPEC §7).
    pub fn snapshot(&self) -> ScreenSnapshot {
        let lines = (0..self.term.screen_lines())
            .map(|row| self.runs(Line(i32::try_from(row).unwrap_or(i32::MAX))))
            .collect();
        // Oldest first. On the alternate screen the active grid has no history (alacritty
        // builds it with 0 lines), so this is empty there without a mode check.
        let history = (self.term.topmost_line().0..0)
            .map(|line| self.runs(Line(line)))
            .collect();
        let point = self.term.grid().cursor.point;
        let mode = self.term.mode();
        ScreenSnapshot {
            cols: to_u16(self.term.columns()),
            rows: to_u16(self.term.screen_lines()),
            lines,
            history,
            cursor: CursorSnapshot {
                row: to_u16(usize::try_from(point.line.0).unwrap_or(0)),
                col: to_u16(point.column.0),
                visible: mode.contains(TermMode::SHOW_CURSOR),
            },
            modes: TermModes {
                alt_screen: mode.contains(TermMode::ALT_SCREEN),
                app_cursor: mode.contains(TermMode::APP_CURSOR),
                app_keypad: mode.contains(TermMode::APP_KEYPAD),
                bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
                mouse: mode.intersects(TermMode::MOUSE_MODE),
                sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
                focus_events: mode.contains(TermMode::FOCUS_IN_OUT),
            },
        }
    }
```

Cost: `snapshot()` now walks at most 200 extra rows per attach or resnapshot (`session.rs:1028, :1043, :1371`). That happens only on those events, never per output chunk. JSON size is at most 200 rows × the run count of a row, so a colorful 80-column history is a few hundred KB, far below the 16 MiB frame limit (§7).

Put this comment above `fn runs`:

```rust
    // ponytail: history rows drop alacritty's WRAPLINE flag (each is painted as its own line), so
    // a wrapped long line copies as two lines and does not re-flow on resize; carry a per-row
    // `wrapped` bit in `ScreenSnapshot` if that matters.
```

Run: `cargo test -p mkrew-daemon --lib emulator -j 4`
Expected: PASS. The old `snapshot_*` tests are unchanged.

- [ ] **Step 6: Wire test and UI doc comments**

`message.rs` `snapshot_wire_shape`, in the literal, replace `history: vec![],` with:

```rust
            history: vec![vec![CellRun {
                text: "old".into(),
                fg: TermColor::Default,
                bg: TermColor::Default,
                flags: 0,
            }]],
```

After the `lines` assert, add:

```rust
        assert_eq!(
            value["data"]["snapshot"]["history"][0][0],
            json!({"text": "old", "fg": "default", "bg": "default", "flags": 0})
        );
```

(The `SessionAttachResult` round trip below it then also covers decoding `history`.)

`terminal/emulator.rs:128-129`, the `from_snapshot` doc becomes:

```rust
    /// A fresh emulator showing `snapshot` (history as scrollback, screen, cursor, modes);
    /// output frames that follow the snapshot continue from it.
```

`view.rs:168-169`, the `load_snapshot` doc becomes:

```rust
    /// Replaces the screen with a fresh daemon snapshot (`session.snapshot` after the output
    /// queue overflowed). The scrollback restarts from the snapshot's history (≤ 200 lines,
    /// ADR 0030). The caller notifies.
```

Run: `cargo test -p mkrew-protocol --lib message::tests::snapshot_wire_shape -j 4` and `cargo test -p mkrew-ui --lib router -j 4`
Expected: PASS.

- [ ] **Step 7: ADR 0030**

Create `docs/decisions/0030-snapshot-carries-history.md`:

```markdown
# 0030 — Attach snapshots carry the daemon's scrollback

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

Sometimes a terminal pane could not scroll back more than one page. Panes that are not visible
(other tab or workspace, Board, minimized, zoomed away) are detached and dropped (SPEC §10). A
re-attach rebuilds the pane from the daemon's snapshot, and that snapshot was the visible screen
only (SPEC §7). An output-queue overflow resnapshot did the same. So after any tab switch the
pane's history started empty again. The daemon emulator keeps 200 lines of history, but it never
sent them.

## Options

1. Send the daemon's history (≤ 200 lines) in the snapshot; `paint` scrolls it into the
   receiving terminal's scrollback.
2. Keep hidden panes' UI emulators (10,000 lines each) instead of dropping them.
3. Replay the tail of the session log file on attach.

## Decision

Option 1. It fixes both triggers (re-attach and overflow) in one place, it works for `mk attach`
too, and it stays inside the §8 memory budget because the daemon already holds those 200 lines.
Option 2 keeps 10k-line grids for every hidden pane (against §10's "not visible = no cost") and
still leaves a gap for output that scrolled while the pane was hidden. Option 3 races the buffered
log writer and adds file I/O to every attach.

## Consequences

`ScreenSnapshot` gets `history` (oldest first, main screen only), and the protocol version goes up
by one. SPEC §7 (snapshot definition), §8 and §10 are updated. After a re-attach a pane scrolls back
at most 200 lines (the daemon scrollback); older output is in the History dialog (session log).
Wrapped history lines come back as separate lines. Raising the daemon scrollback is a separate
memory-budget decision (§8, §15).
```

- [ ] **Step 8: SPEC**

§7 (:569-573), replace the start of the paragraph, up to and including `` `cursor` `{row, col, visible}` and `modes` (`alt_screen`, `app_cursor`, ``, with:

```markdown
**Screen snapshot.** `session.attach` returns `{state, snapshot}`; `session.snapshot` carries
the same `snapshot`. A snapshot is the visible screen of the daemon's emulator plus its history
(ADR 0030): `cols`, `rows`, `lines` (one array per row of style runs `{text, fg, bg, flags}`,
trailing default blanks omitted), `history` (the rows above the screen in the same format, oldest
first, at most the daemon's 200 lines; empty and omitted on the alternate screen), `cursor`
`{row, col, visible}` and `modes` (`alt_screen`, `app_cursor`,
```

Keep the rest of the paragraph, from `` `app_keypad` `` on. After "Output frames sent after a snapshot continue from it." (:576) add: "Painting a snapshot scrolls `history` into the client terminal's scrollback before it draws the screen."

§8 (:596-597): replace "used for (a) state detection on the real screen text and (b) instant attach snapshots." with "used for (a) state detection on the real screen text and (b) instant attach snapshots, which carry those 200 lines (ADR 0030)."

§10 (:702-703), replace the bullet with:

```markdown
    - Panes that are not visible (other tab, other workspace, minimized window) are detached:
      no parsing, no layout, no rendering. Re-attach uses the daemon snapshot, so the pane's
      scrollback restarts from the daemon's ≤ 200 history lines (ADR 0030).
```

- [ ] **Step 9: HUMAN_CHECKS**

In the 2026-10-09 section:

```markdown
- [ ] **Scrollback after a tab switch** (restart the daemon first, because the protocol version
  changed): in a pwsh pane run `1..300 | % { "line $_" }`, then Ctrl+Tab to another tab and back.
  Also try opening the Board and coming back, and minimizing and restoring. Each time, the mouse
  wheel and Shift+PgUp scroll back about 200 lines above the screen (to around `line 100`), the
  screen looks the same as before the switch, and typing returns to the live prompt. Do the same
  in a claude pane after a long answer. `mk attach <session>` in Windows Terminal: its scrollbar
  shows the same lines above the screen.
```

- [ ] **Step 10: Verify and tick**

Run the full verification. Tick inbox line 33 `[x]`.
Suggested message: `fix(terminal): attach snapshots carry the daemon scrollback so re-attached panes scroll back`

No daemon integration assert: the fake `ask` scenario in `attach_streams_output_and_reattach_restores_the_screen` (daemon.rs:1505) fits its 24-row screen, so it has no history to check. The unit tests in Steps 2–4 cover both ends.

---

### Task 11: Alt+Shift+A / Alt+Shift+S open an agent / shell pane (the + Agent / + Shell buttons)

Inbox: line 34 `Add short cut for '+ Agent' which is open new agent by split current tab now`; line 35 `Add short cut for '+ Shell' …` (the shortcut half; Task 12 does the default shell). Spec: §10 (:682-684), §13 (:886-890).

**Current state (investigated):**
- `+ Agent` / `+ Shell` = `new_pane_button` (main.rs:14007-14026). The click runs `show_view(View::Terminals)`, then `new_pane(kind, Axis::Vertical)`. The tooltip shortcut is `""`. `render_toolbar` adds both buttons (:7902-7915).
- `Shell::new_pane` (main.rs:6029-6092) splits the focused pane of the active tab to the right. It needs a connection, a workspace and an active tab. If an agent has no `default_agent`, it sets `tab_error` and opens the workspace menu (:6053-6065).
- No key does this. Ctrl+Shift+A / Ctrl+Shift+T (`NewAgentTab` / `NewShellTab`) open a **new tab** (`new_tab` :6414 → `create_tab` :1133). Alt+Shift+= / - (`SplitRight` / `SplitDown`) split with the **focused pane's kind** (`split` :6423 → `lifecycle::split_kind`, which gives a shell when nothing is focused).
- The empty-tab buttons "New agent" / "New shell" (main.rs:13497-13509) call `new_pane`, but their kbd chips show the new-*tab* keys. The 10-08 plan accepted that mismatch (its option 11a). With the new keys the chips become correct.
- `alt-shift-a` and `alt-shift-s` are free (keymap.rs `default_keys` :267-324). `ctrl-shift-s` (SendComments) has other modifiers, and `same_keys` (:582) compares modifiers. Other tasks of this plan use `f11`, `ctrl-s`, `ctrl-f` and `ctrl-h`, so nothing clashes. The palette test `commands_carry_their_shortcut` (palette.rs:684-695) overrides `new_shell_tab = "ctrl-shift-y"` and is not affected.
- Today an unbound Alt+Shift+A reaches the PTY as `ESC A`. Once it is bound, programs no longer get it. That is the same trade the Alt+Shift+= split keys already make.
- Copy the `SplitRight` / `SplitDown` wiring:
  - keymap.rs: enum :34-35, `all()` :151-152, `name()` :213-214, `default_keys()` :280-281 (`context()` :328 falls through to `_ => None`).
  - palette.rs: `title()` :183-184, `available()` :250.
  - main.rs `run`: the show-Terminals arm :6166-6185, dispatch :6218-6219.

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs:34-35, :151-152, :213-214, :280-281`, test `spec_defaults` (:645-732)
- Modify: `crates/mkrew-ui/src/palette.rs:183-184, :250`, test `availability_follows_the_view` (:698-794)
- Modify: `crates/mkrew-ui/src/main.rs:6174-6175, :6218-6219` (`Shell::run`), :7902-7915 (`render_toolbar`), :13498, :13505 (`render_tab` empty state), :14007-14026 (`new_pane_button`)
- Modify: `docs/SPEC.md` §10 (:683-684), §13 (:890); `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces:
  - `keymap::Command::SplitAgent` (name `split_agent`, default `alt-shift-a`, context `None`).
  - `Command::SplitShell` (`split_shell`, `alt-shift-s`, `None`).
  - Palette titles "Panes: New agent pane" / "Panes: New shell pane".
  - `fn new_pane_button(id: &'static str, label: &'static str, kind: SessionKind, keys: String, theme: &Theme, cx: &mut Context<'_, Shell>) -> gpui::Stateful<gpui::Div>`.
- Consumes: `Shell::new_pane(kind, Axis::Vertical, window, cx)`, unchanged here. Task 12 changes what it does for a shell with no default.

- [ ] **Step 1: Failing tests**

`keymap.rs` `spec_defaults`, after `assert_eq!(bound("alt-shift--"), Some(Command::SplitDown));` (:654):

```rust
        // The + Agent / + Shell buttons (split right with the workspace default).
        assert_eq!(bound("alt-shift-a"), Some(Command::SplitAgent));
        assert_eq!(bound("alt-shift-s"), Some(Command::SplitShell));
        assert_eq!(keymap.label(Command::SplitShell), "Alt+Shift+S");
        // Global like the split keys: a focused terminal no longer gets ESC A / ESC S.
        assert_eq!(Command::SplitAgent.context(), None);
```

`palette.rs` `availability_follows_the_view`, after `assert!(git.contains(&Command::NewShellTab), "shows the terminals");` (:752):

```rust
        // Like the buttons: from any view (they show Terminals first), once a tab exists.
        for command in [Command::SplitAgent, Command::SplitShell] {
            assert!(terminals.contains(&command), "{command:?}");
            assert!(git.contains(&command), "{command:?}");
        }
        let no_tabs = commands(&Context {
            tabs: 0,
            ..context(View::Terminals)
        });
        assert!(!no_tabs.contains(&Command::SplitAgent));
```

Run: `cargo test -p mkrew-ui --lib keymap::tests::spec_defaults -j 4`
Expected: FAIL to compile (`no variant named SplitAgent`).

- [ ] **Step 2: Keymap and palette**

keymap.rs, enum after `SplitDown,` (:35):

```rust
    /// Split the focused pane to the right with the workspace's default agent (+ Agent).
    SplitAgent,
    /// Split the focused pane to the right with the workspace's default shell (+ Shell).
    SplitShell,
```

`all()`: add `Command::SplitAgent, Command::SplitShell,` after `Command::SplitDown,` (:152).
`name()`, after :214:

```rust
            Command::SplitAgent => "split_agent".into(),
            Command::SplitShell => "split_shell".into(),
```

`default_keys()`, after :281:

```rust
            Command::SplitAgent => "alt-shift-a".into(),
            Command::SplitShell => "alt-shift-s".into(),
```

`context()`: no change (`_ => None`).

palette.rs `title()`, after :184:

```rust
        Command::SplitAgent => "Panes: New agent pane".into(),
        Command::SplitShell => "Panes: New shell pane".into(),
```

`available()`, replace :250:

```rust
        Command::SplitRight | Command::SplitDown | Command::SplitAgent | Command::SplitShell => {
            context.workspace && context.tabs > 0
        }
```

Run: `cargo test -p mkrew-ui --lib keymap -j 4` and `cargo test -p mkrew-ui --lib palette -j 4`
Expected: PASS. `every_default_parses_and_is_unique`, `names_round_trip` and `template_parses_to_the_defaults_commented_or_not` cover both new entries through `all()`. A duplicate key would fail the first one.

- [ ] **Step 3: Shell::run, toolbar tooltips, empty-tab chips**

main.rs `Shell::run`, the show-Terminals arm (:6174-6175) becomes:

```rust
                | Command::SplitRight
                | Command::SplitDown
                | Command::SplitAgent
                | Command::SplitShell => self.show_view(View::Terminals, window, cx),
```

Dispatch, after `Command::SplitDown => …` (:6219):

```rust
            // Exactly the + Agent / + Shell buttons.
            Command::SplitAgent => self.new_pane(SessionKind::Agent, Axis::Vertical, window, cx),
            Command::SplitShell => self.new_pane(SessionKind::Shell, Axis::Vertical, window, cx),
```

`new_pane_button` (:14007-14026) takes a `keys` parameter and shows it in the tooltip:

```rust
/// A quiet button starting a new agent or shell pane; `keys` = its shortcut label.
fn new_pane_button(
    id: &'static str,
    label: &'static str,
    kind: SessionKind,
    keys: String,
    theme: &Theme,
    cx: &mut Context<'_, Shell>,
) -> gpui::Stateful<gpui::Div> {
    let tip = match kind {
        SessionKind::Agent => "New pane with the workspace's default agent",
        SessionKind::Shell => "New shell pane in the workspace folder",
    };
    button(id, label, theme, false)
        .tooltip(tooltip(theme, tip, keys))
        .on_click(cx.listener(move |shell, _, window, cx| {
            // The toolbar shows it in every view; the new pane is seen in Terminals.
            shell.show_view(View::Terminals, window, cx);
            shell.new_pane(kind, Axis::Vertical, window, cx);
        }))
}
```

`render_toolbar` (:7902-7915):

```rust
            .child(new_pane_button(
                "new-agent-pane",
                "+ Agent",
                SessionKind::Agent,
                self.keymap.label(Command::SplitAgent),
                theme,
                cx,
            ))
            .child(new_pane_button(
                "new-shell-pane",
                "+ Shell",
                SessionKind::Shell,
                self.keymap.label(Command::SplitShell),
                theme,
                cx,
            ))
```

`render_tab` empty state: these buttons split, so their chips now show the split keys. Change :13498 to `.child(kbd(self.keymap.label(Command::SplitAgent), theme, true))` and :13505 to `.child(kbd(self.keymap.label(Command::SplitShell), theme, false))`.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean. The `run` match is exhaustive, so a missing arm fails to compile.

- [ ] **Step 4: SPEC + HUMAN_CHECKS**

SPEC §13, after the Split right / split down row (:890):

```markdown
| Panes | New agent pane / new shell pane (split right with the workspace default, like + Agent / + Shell) | Alt+Shift+A / Alt+Shift+S |
```

SPEC §10 Toolbar bullet (:683-684): replace `split Run button on the right, in every view (+ Agent / + Shell switch to Terminals first).` with:

```markdown
  split Run button on the right, in every view (+ Agent / + Shell switch to Terminals first; their
  tooltips show Alt+Shift+A / Alt+Shift+S, §13).
```

HUMAN_CHECKS, in the 2026-10-09 section (created by Task 1):

```markdown
- [ ] **Alt+Shift+A / Alt+Shift+S:** with a terminal focused, Alt+Shift+A splits the focused pane to
  the right with the workspace's default agent and Alt+Shift+S with its default shell, the same as
  + Agent / + Shell; from Board / Files / Git they switch to Terminals first. The buttons' tooltips
  and the empty tab's chips show the keys; the palette lists "Panes: New agent pane" / "… shell pane".
```

- [ ] **Step 5: Verify and tick**

Run the full verification. Tick inbox line 34 `[x]` (line 35 waits for Task 12).
Suggested message: `feat(ui): Alt+Shift+A / Alt+Shift+S open an agent / shell pane like + Agent / + Shell`

---

### Task 12: Shell profiles and a per-workspace default shell (ADR 0032)

Inbox: line 35 `Add short cut for '+ Shell' and user must to select default shell user want to use same as agent concept`. Spec: §5 (:316-319), §6 (:421-424), §7 (:498, :499, :507, :521), §10 (:691).

**Current state (investigated):**
- **How a shell pane starts today.**
  - The daemon's `session_spawn` (server.rs:478-491) has `(None, SessionKind::Shell) => "shell"`, a hard-coded profile.
  - `agents/shell.toml` has no `command`. So `launch::program` (launch.rs:49-55) calls `default_shell()` (:57-63), which returns `%COMSPEC%`: cmd.exe on Windows.
  - SPEC §10 :691 promises "pwsh on Windows, … configurable". The code and the spec disagree, and HUMAN_CHECKS :147 lists cmd as a known gap.
  - Profiles already support `kind = "shell"` with any `command` (profile.rs:279: only agents must have a command). `session.spawn` with `agent_profile: Some("pwsh")` already works end to end, because a named profile decides the kind.
- **The default-agent model to copy:**
  - The `project.default_agent` column.
  - Store: `ProjectChanges.default_agent` (store lib.rs:72-82), `update_project` :275-327, `PROJECT_COLUMNS` :34, `RawProject` :1340-1390.
  - Protocol: `Project.default_agent` (message.rs:421) and `ProjectUpdateParams.default_agent: Option<Option<String>>` with `deserialize_with = "present"` (:663-669).
  - Daemon: `project_update` (server.rs:908-947) checks the value with `check_agent_profile` (:949-958). `task_create` :964 and `task_update` :984 call it too.
  - UI:
    - `workspace_menu::agent_choices` (workspace_menu.rs:42-66) keeps only `kind == Agent`.
    - `agent_rows` (main.rs:15144-15228) draws the rows.
    - `render_workspace_menu` (:7419-7548) shows "Default agent for new panes" (:7496-7505).
    - `set_default_agent` (:4005-4016) sends `project.update` through `update_project`.
- **No installed check exists.** `AgentProfileInfo` (message.rs:1287-1298) has no installed flag, and `agent_profiles` (server.rs:1313-1337) only lists profiles. The only detection is `mkrew_core::lookup::find` (lookup.rs:19): PATH + PATHEXT, then `.ps1`; an absolute path is checked as given.
- **Seeding.** `seed_profiles` (daemon lib.rs:90-100) writes every missing file of `SAMPLE_PROFILES` (:80-88) **at every daemon start** and never overwrites. So existing config dirs get new sample files on the next start.
- **Shell lookup on this machine (checked 2026-10-09).**
  - `pwsh` resolves to the WindowsApps alias and `cmd` to System32.
  - The user PATH puts `…\Microsoft\WindowsApps` before Git's `cmd` folder. WindowsApps has a `bash.exe` alias for **WSL**, and Git's `cmd` folder has no `bash.exe`. So `command = "bash"` starts WSL bash, not Git Bash.
  - Git Bash needs its full path (`C:\Program Files\Git\bin\bash.exe`). Its `/etc/profile` has no `cd "$HOME"`, so `--login -i` keeps the project folder as cwd.
- **Run commands** (server/run.rs:124-137) use `load_profile("shell")` and `launch::program` to choose the `typed_line` exit syntax (core run.rs:194-202). They stay on `shell` (see the ADR).
- **One place for the UI check.** `new_pane` refuses only agents without a default (main.rs:6053-6065). Every shell entry point goes through `new_pane`: `create_tab(Some(kind))` (:1176-1180), `split` (:6423-6429), the toolbar, the empty tab and Task 11's keys. So one check there covers all of them.
- **Migration number.** The latest migration is `0013_project_color.sql`, and `MIGRATIONS` (store lib.rs:17-31) is append-only. The next free number is 0014 today; check the folder again when this task lands.

**Files:**
- Create: `agents/pwsh.toml`, `agents/powershell.toml`, `agents/cmd.toml`, `agents/bash.toml`, `agents/git-bash.toml`
- Create: `crates/mkrew-store/migrations/0014_project_default_shell.sql` (next free number)
- Create: `docs/decisions/0032-shell-profiles.md`
- Modify: `agents/shell.toml:1-2` (comment); `crates/mkrew-daemon/src/lib.rs:80-88` (`SAMPLE_PROFILES`)
- Modify: `crates/mkrew-core/src/profile.rs:632-664` (test `repo_sample_profiles_parse`)
- Modify: `crates/mkrew-protocol/src/lib.rs:16`; `crates/mkrew-protocol/src/message.rs:410-433` (`Project`), :657-683 (`ProjectUpdateParams`), :950-956 (`SessionSpawnParams` doc), :1287-1298 (`AgentProfileInfo`), tests :1565, :1835-1887, :1964-1983, :2068-2081
- Modify: `crates/mkrew-store/src/lib.rs:17-31, :34, :72-82, :135-147, :275-299, :1340-1389`; `crates/mkrew-store/src/tests.rs` (new test after :1225)
- Modify: `crates/mkrew-daemon/src/server.rs:478-491` (`session_spawn`), :908-958 (`project_update`, `check_agent_profile`), :964, :984, :1313-1325 (`agent_profiles`)
- Modify: `crates/mkrew-daemon/tests/daemon.rs:730-757` (`agent_profiles_lists_samples_and_broken_files`), :2730-2739 (helpers), :3520-3605 (`spawn_defaults_to_the_project_agent_or_the_shell_and_fills_an_empty_tab`), literals :3469, :3495, :5103, :5609
- Modify: `crates/mkrew-ui/src/workspace_menu.rs:1-2, :42-90`, tests :95-158; `crates/mkrew-ui/src/task_dialog.rs:218, :321-329`; `crates/mkrew-ui/src/lifecycle.rs:19-29` (comments)
- Modify: `crates/mkrew-ui/src/main.rs:4005-4016` (`set_default_shell`), :6029-6065 (`new_pane`), :7419-7506 (`render_workspace_menu`), :14015-14018 (`new_pane_button` tip), :15144-15186 (`agent_rows`), :16073-16084 (Run with…), :16147, :16152 (fan-out)
- Modify (one `default_shell: None,` line each, see Step 2): `crates/mkrew-cli/src/project.rs:58-65`; `crates/mkrew-ui/src/{inbox.rs:195-207, palette.rs:586-598, run.rs:268-279, run_setup.rs:188-195, run_setup.rs:262-273, sidebar.rs:331-343}`; `crates/mkrew-ui/tests/{connect.rs:91-98, lifecycle.rs:126-133}`
- Modify: `docs/SPEC.md` §5, §6, §7, §10; `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces (protocol):
  - `Project.default_shell: Option<String>` (`#[serde(default)]`; `None` = not picked yet).
  - `ProjectUpdateParams.default_shell: Option<Option<String>>`, the same tri-state as `default_agent`: absent = unchanged, `null` = clear.
  - `AgentProfileInfo.installed: bool` (`#[serde(default)]`): the profile's program (`launch::program`) resolves with `lookup::find`.
  - `PROTOCOL_VERSION` + 1.
- Produces (store): `ProjectChanges.default_shell: Option<Option<&'a str>>`.
- Produces (daemon):
  - `session.spawn` without a profile and with `kind: shell` uses `project.default_shell`. Without one it answers `invalid_request` "project X has no default shell; pick one in the workspace menu".
  - `project.update` refuses a `default_shell` that is an agent profile (`invalid_request`) or unknown (`not_found`).
  - `fn check_profile_kind(&self, id: &str, kind: mkrew_core::ProfileKind) -> Result<(), Outcome>` replaces `check_agent_profile`.
- Produces (UI):
  - `workspace_menu::agent_choices(profiles: &[AgentProfileInfo], kind: SessionKind, current: Option<&str>) -> Vec<AgentChoice>`. Shells are listed only when `installed`.
  - `workspace_menu::set_default_shell(project_id: Ulid, shell: &str) -> ProjectUpdateParams`.
  - `fn agent_rows(agents: &Agents, kind: SessionKind, current: Option<&str>, id_prefix: &str, theme, cx, pick)`.
  - `Shell::set_default_shell(&mut self, project_id: Ulid, shell: String, window, cx)`.

- [ ] **Step 1: Ship the shell profiles**

Failing test. In `profile.rs` `repo_sample_profiles_parse`, replace the `ids` assert (:638) and the `shell` kind assert (:653) (`load_dir` sorts by id):

```rust
        assert_eq!(
            ids,
            [
                "bash", "claude", "cmd", "codex", "copilot", "fake", "git-bash", "omp",
                "powershell", "pwsh", "shell"
            ]
        );
```

```rust
        for id in ["bash", "cmd", "git-bash", "powershell", "pwsh", "shell"] {
            assert_eq!(set.get(id).unwrap().kind, ProfileKind::Shell, "{id}");
        }
```

Run: `cargo test -p mkrew-core --lib profile::tests::repo_sample_profiles_parse -j 4`
Expected: FAIL (`left: ["claude", …, "shell"]`).

`agents/pwsh.toml`:

```toml
# PowerShell 7 (`pwsh` on PATH). Pick it as a workspace's default shell in the workspace menu.
name = "PowerShell"
icon = "PS"
kind = "shell"
command = "pwsh"
args = ["-NoLogo"]
prompt_mode = "none"

[state]
mode = "screen"
idle_after_ms = 3000
needs_input_patterns = []
```

`agents/powershell.toml`:

```toml
# Windows PowerShell 5 (built into Windows).
name = "Windows PowerShell"
icon = "PS5"
kind = "shell"
command = "powershell"
args = ["-NoLogo"]
prompt_mode = "none"

[state]
mode = "screen"
idle_after_ms = 3000
needs_input_patterns = []
```

`agents/cmd.toml`:

```toml
# Command Prompt (Windows).
name = "Command Prompt"
icon = "C:>"
kind = "shell"
command = "cmd"
prompt_mode = "none"

[state]
mode = "screen"
idle_after_ms = 3000
needs_input_patterns = []
```

`agents/bash.toml`:

```toml
# The first `bash` on PATH: the system bash on Linux / macOS. On Windows this is often WSL's
# (WindowsApps\bash.exe); use `git-bash` for Git Bash.
name = "Bash"
icon = "$"
kind = "shell"
command = "bash"
args = ["--login", "-i"]
prompt_mode = "none"

[state]
mode = "screen"
idle_after_ms = 3000
needs_input_patterns = []
```

`agents/git-bash.toml` (a literal string, so the backslashes stay as they are):

```toml
# Git Bash from the default Git for Windows install. Installed somewhere else (e.g. per user under
# %LOCALAPPDATA%\Programs\Git)? Change `command` to that bin\bash.exe.
name = "Git Bash"
icon = "G$"
kind = "shell"
command = 'C:\Program Files\Git\bin\bash.exe'
args = ["--login", "-i"]
prompt_mode = "none"

[state]
mode = "screen"
idle_after_ms = 3000
needs_input_patterns = []
```

`agents/shell.toml`: change the header comment (lines 1-2) to:

```toml
# The OS default shell: no `command`, so the daemon uses %COMSPEC% on Windows, $SHELL on Unix.
# Run commands (the Run button) always use this profile (ADR 0032).
```

Daemon lib.rs `SAMPLE_PROFILES` (:81-88), in id order:

```rust
const SAMPLE_PROFILES: &[(&str, &str)] = &[
    ("bash.toml", include_str!("../../../agents/bash.toml")),
    ("claude.toml", include_str!("../../../agents/claude.toml")),
    ("cmd.toml", include_str!("../../../agents/cmd.toml")),
    ("codex.toml", include_str!("../../../agents/codex.toml")),
    ("copilot.toml", include_str!("../../../agents/copilot.toml")),
    ("fake.toml", include_str!("../../../agents/fake.toml")),
    ("git-bash.toml", include_str!("../../../agents/git-bash.toml")),
    ("omp.toml", include_str!("../../../agents/omp.toml")),
    ("powershell.toml", include_str!("../../../agents/powershell.toml")),
    ("pwsh.toml", include_str!("../../../agents/pwsh.toml")),
    ("shell.toml", include_str!("../../../agents/shell.toml")),
];
```

Run: `cargo test -p mkrew-core --lib profile -j 4` and `cargo test -p mkrew-daemon --lib -j 4`
Expected: PASS. Every file parses (icons have at most 3 characters; a shell may have any command). `sample_icon_comes_from_the_shipped_profile_of_the_same_id` still passes.

- [ ] **Step 2: Protocol: `default_shell`, `installed`, version**

Failing test. In message.rs `project_update_tells_missing_from_null`, after the `color` asserts (:1865):

```rust
        assert_eq!(parse(json!({"project_id": id})).default_shell, None);
        assert_eq!(
            parse(json!({"project_id": id, "default_shell": null})).default_shell,
            Some(None)
        );
        assert_eq!(
            parse(json!({"project_id": id, "default_shell": "pwsh"})).default_shell,
            Some(Some("pwsh".into()))
        );
```

In the same test, add `default_shell: None,` to the `keep` literal (:1867-1874), after `default_agent: None,`. The existing wire asserts then prove that an unchanged `default_shell` is left out.

Run: `cargo test -p mkrew-protocol --lib message::tests::project_update_tells_missing_from_null -j 4`
Expected: FAIL to compile (`no field default_shell`).

`Project` (:421), after `pub default_agent: Option<String>,`:

```rust
    /// Shell profile for new shell panes (ADR 0032); `None` = not picked yet, so a new shell
    /// pane is refused until the user picks one.
    #[serde(default)]
    pub default_shell: Option<String>,
```

`ProjectUpdateParams` (:669), after `default_agent`:

```rust
    /// `Some(None)` (JSON `null`) clears the default shell.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub default_shell: Option<Option<String>>,
```

`SessionSpawnParams.agent_profile` doc (:953-954):

```rust
    /// `None` = the default for `kind`: the project's `default_agent` for an agent, its
    /// `default_shell` for a shell. A named profile decides the session kind itself.
```

`AgentProfileInfo` (:1297), after `icon`:

```rust
    /// The profile's program is found (PATH with PATHEXT, or its absolute path); the UI lists
    /// only installed shells (ADR 0032).
    #[serde(default)]
    pub installed: bool,
```

`agent_profiles_result_round_trips` (:1967-1973): add `installed: true,`.

lib.rs:16: bump `PROTOCOL_VERSION` (crates/mkrew-protocol/src/lib.rs:16) by one from its value when this task lands.

Then add `default_shell: None,` right after the `default_agent` field of every other `Project { … }` / `ProjectUpdateParams { … }` literal. `cargo check --workspace --all-targets -j 4` lists them:
- message.rs :1565 (request round trip) and :2075 (`Project` round trip; use `default_shell: Some("pwsh".into()),` there).
- store lib.rs :141 (Step 3 rewrites :1382).
- cli project.rs:61.
- daemon tests daemon.rs:2734, :3472, :3498, :5106, :5612.
- ui src: inbox.rs:201, palette.rs:592, run.rs:273, run_setup.rs:191 and :267, sidebar.rs:337, workspace_menu.rs:73 and :85.
- ui tests: connect.rs:94, lifecycle.rs:129.

Add `installed: true,` to the test `AgentProfileInfo` literals in task_dialog.rs:326 and workspace_menu.rs:104. Step 4 changes server.rs:1318.

Run: `cargo test -p mkrew-protocol --lib -j 4`
Expected: PASS.

- [ ] **Step 3: Store: column and round trip**

Failing test. Store `tests.rs`, after `project_color_persists_and_clears` (:1225):

```rust
#[test]
fn project_default_shell_persists_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert_eq!(p.default_shell, None);
    let pwsh = ProjectChanges {
        default_shell: Some(Some("pwsh")),
        ..ProjectChanges::default()
    };
    let updated = store.update_project(p.id, pwsh).unwrap();
    assert_eq!(updated.default_agent, None, "only the shell changes");
    assert_eq!(
        open(&dir).get_project(p.id).unwrap().default_shell.as_deref(),
        Some("pwsh")
    );
    // Changing another field keeps it.
    let agent = ProjectChanges {
        default_agent: Some(Some("claude")),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, agent).unwrap();
    assert_eq!(
        open(&dir).get_project(p.id).unwrap().default_shell.as_deref(),
        Some("pwsh")
    );
    let none = ProjectChanges {
        default_shell: Some(None),
        ..ProjectChanges::default()
    };
    store.update_project(p.id, none).unwrap();
    assert_eq!(open(&dir).get_project(p.id).unwrap().default_shell, None);
}
```

Run: `cargo test -p mkrew-store --lib tests::project_default_shell_persists_and_clears -j 4`
Expected: FAIL to compile (`ProjectChanges` has no field `default_shell`).

`crates/mkrew-store/migrations/0014_project_default_shell.sql` (next free number; rename it if 0014 is taken when this lands):

```sql
-- SPEC §5, ADR 0032. Never edit: schema changes go in a new numbered migration.
ALTER TABLE project ADD COLUMN default_shell TEXT; -- shell profile id for new shell panes, NULL = ask
```

lib.rs:
- `MIGRATIONS`: append `include_str!("../migrations/0014_project_default_shell.sql"),` after :30.
- `PROJECT_COLUMNS` (:34): append `, default_shell` (index 10).
- `ProjectChanges` (:76), after `default_agent`:

```rust
    /// A shell profile id (checked by the daemon); `Some(None)` clears it.
    pub default_shell: Option<Option<&'a str>>,
```

`update_project`, after the `default_agent` block (:280-282):

```rust
        if let Some(default_shell) = changes.default_shell {
            project.default_shell = default_shell.map(str::to_owned);
        }
```

and the UPDATE (:290-299):

```rust
        tx.execute(
            "UPDATE project SET name = ?2, default_agent = ?3, run_autostart = ?4, color = ?5, default_shell = ?6 WHERE id = ?1",
            params![
                id.to_string(),
                project.name,
                project.default_agent,
                project.run_autostart,
                project.color,
                project.default_shell
            ],
        )?;
```

`RawProject`:
- Add the field `default_shell: Option<String>,` after `color` (:1350).
- `from_row`: `default_shell: row.get(10)?,`.
- `into_project`: `default_shell: self.default_shell,` after `default_agent: self.default_agent,`.

`add_project` (:141) already got `default_shell: None` in Step 2, and its INSERT leaves the column NULL.

Run: `cargo test -p mkrew-store --lib -j 4`
Expected: PASS (the schema-version tests use `MIGRATIONS.len()`).

- [ ] **Step 4: Daemon: refuse without a default shell, validate it, report `installed`**

Failing tests. In daemon.rs, add a helper after `set_default_agent` (:2739):

```rust
fn set_default_shell(project: &Project, shell: Option<&str>) -> Request {
    Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: None,
        default_shell: Some(shell.map(str::to_owned)),
        run_commands: None,
        run_autostart: None,
        color: None,
    })
}
```

In `spawn_defaults_to_the_project_agent_or_the_shell_and_fills_an_empty_tab`, insert before `let shell: SessionSpawnResult = client` (:3570):

```rust
    // No default shell yet: refused like a missing default agent (ADR 0032).
    assert_eq!(
        error_code(client.call(spawn_params(&project, SessionKind::Shell, Some(tab.id)))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(set_default_shell(&project, Some("fake")))),
        ErrorCode::InvalidRequest,
        "an agent is not a shell"
    );
    assert_eq!(
        error_code(client.call(set_default_shell(&project, Some("nope")))),
        ErrorCode::NotFound
    );
    let with_shell: Project = client
        .call(set_default_shell(&project, Some("shell")))
        .into_result()
        .unwrap();
    assert_eq!(with_shell.default_shell.as_deref(), Some("shell"));
    assert_eq!(with_shell.default_agent.as_deref(), Some("fake"), "left unchanged");
```

The existing lines after it (the spawn gives `agent_profile == "shell"`, kind Shell) now prove that the default is used. The later `other` spawn (:3584) is still refused by the tab check, which runs before the profile is resolved.

In `agent_profiles_lists_samples_and_broken_files` (:731-742), add after the `broken` profile write:

```rust
    env.write_profile("ghost", "kind = \"shell\"\ncommand = \"mkrew-no-such-shell\"\n");
```

and replace the ids assert (:738) with:

```rust
    assert_eq!(
        ids,
        [
            "bash", "claude", "cmd", "codex", "copilot", "fake", "ghost", "git-bash", "omp",
            "powershell", "pwsh", "shell"
        ]
    );
    let installed = |id: &str| result.profiles.iter().find(|p| p.id == id).unwrap().installed;
    // The OS default shell always resolves; a shell missing from PATH is marked for the UI.
    assert!(installed("shell"));
    assert!(!installed("ghost"));
```

Run: `cargo test -p mkrew-daemon --test daemon spawn_defaults_to_the_project -j 2`
Expected: FAIL at the first new assert (the shell spawn succeeds with `shell` instead of answering `invalid_request`).

In server.rs `session_spawn`, replace the shell arm (:480):

```rust
            (None, SessionKind::Shell) => match project.default_shell.clone() {
                Some(id) => id,
                None => {
                    let message = format!(
                        "project {} has no default shell; pick one in the workspace menu",
                        project.name
                    );
                    return Outcome::error(ErrorCode::InvalidRequest, message);
                }
            },
```

Replace `check_agent_profile` (:949-958) with:

```rust
    /// `id` must name a profile of `kind`: `default_agent` and task agents an agent,
    /// `default_shell` a shell.
    fn check_profile_kind(&self, id: &str, kind: mkrew_core::ProfileKind) -> Result<(), Outcome> {
        if self.load_profile(id)?.kind == kind {
            return Ok(());
        }
        let message = match kind {
            mkrew_core::ProfileKind::Agent => format!("{id} is a shell profile, not an agent"),
            mkrew_core::ProfileKind::Shell => format!("{id} is an agent profile, not a shell"),
        };
        Err(Outcome::error(ErrorCode::InvalidRequest, message))
    }
```

Its three callers (:912, :965, :985) become `self.check_profile_kind(id, mkrew_core::ProfileKind::Agent)`. In `project_update`, after the `default_agent` check (:915):

```rust
        if let Some(Some(id)) = &params.default_shell
            && let Err(outcome) = self.check_profile_kind(id, mkrew_core::ProfileKind::Shell)
        {
            return outcome;
        }
```

and in its `ProjectChanges` (:937): `default_shell: params.default_shell.as_ref().map(Option::as_deref),`. Change the doc (:908-909) to: "Changes the project; a new `default_agent` must name an agent profile, a new `default_shell` a shell profile; run commands are checked and trimmed (SPEC §10)."

`agent_profiles` (:1318-1324):

```rust
            .map(|p| {
                // What a spawn would run (the OS shell for a shell without `command`).
                let program = crate::launch::program(&p);
                AgentProfileInfo {
                    installed: mkrew_core::lookup::find(std::ffi::OsStr::new(&program)).is_some(),
                    kind: session_kind(p.kind),
                    icon: p.icon.or_else(|| crate::sample_icon(&p.id)),
                    id: p.id,
                    name: p.name,
                    command: p.command,
                }
            })
```

Run: `cargo test -p mkrew-daemon --test daemon spawn_defaults_to_the_project -j 2` and `cargo test -p mkrew-daemon --test daemon agent_profiles_lists -j 2`
Expected: PASS. Also run `cargo test -p mkrew-daemon --test daemon run_ -j 2`: Run commands still start through `shell` and need no default.

- [ ] **Step 5: UI: shell choices**

Failing test. In workspace_menu.rs `mod tests`, `profile()` (:99-107) already has `installed: true,` from Step 2. The four existing `agent_choices(x, y)` calls (:126, :135, :142, :147) become `agent_choices(x, SessionKind::Agent, y)`. New test:

```rust
    #[test]
    fn shells_list_only_installed_shell_profiles() {
        let mut list = profiles();
        list.push(profile("pwsh", "PowerShell", SessionKind::Shell));
        list.push(AgentProfileInfo {
            installed: false,
            ..profile("bash", "Bash", SessionKind::Shell)
        });
        list.push(AgentProfileInfo {
            installed: false,
            ..profile("omp", "Oh My Pi", SessionKind::Agent)
        });
        assert_eq!(
            rows(&agent_choices(&list, SessionKind::Shell, Some("pwsh"))),
            [("shell", "Shell", false), ("pwsh", "PowerShell", true)]
        );
        // Agents stay listed when missing: a spawn then names the PATH it searched.
        assert!(
            agent_choices(&list, SessionKind::Agent, None)
                .iter()
                .any(|choice| choice.id == "omp")
        );
        // A default shell that is no longer found stays visible, like a missing agent.
        assert_eq!(
            rows(&agent_choices(&list, SessionKind::Shell, Some("bash"))).last(),
            Some(&("bash", "bash (profile missing)", true))
        );
    }
```

Run: `cargo test -p mkrew-ui --lib workspace_menu -j 4`
Expected: FAIL to compile (`agent_choices` takes 2 arguments).

`agent_choices` (:42-54) gets a kind parameter (the rest of the body stays the same):

```rust
/// The profiles of `kind` (agents, or shells found on PATH), with `current` selected. A
/// `current` that no longer has a listed profile stays, marked missing, so the setting is never
/// hidden.
pub fn agent_choices(
    profiles: &[AgentProfileInfo],
    kind: SessionKind,
    current: Option<&str>,
) -> Vec<AgentChoice> {
    let mut choices: Vec<AgentChoice> = profiles
        .iter()
        // Shells the machine lacks are hidden (several samples ship); agents always show.
        .filter(|profile| profile.kind == kind && (kind == SessionKind::Agent || profile.installed))
        .map(|profile| AgentChoice {
```

After `set_default_agent` (:78):

```rust
/// `project.update` making `shell` the workspace's default shell (ADR 0032).
pub fn set_default_shell(project_id: Ulid, shell: &str) -> ProjectUpdateParams {
    ProjectUpdateParams {
        project_id,
        name: None,
        default_agent: None,
        default_shell: Some(Some(shell.to_owned())),
        run_commands: None,
        run_autostart: None,
        color: None,
    }
}
```

Other edits in this step:
- workspace_menu.rs module doc (:1-2): "…"Run setup…", the default agent and shell, and removing the workspace."
- task_dialog.rs:218: `choices.extend(agent_choices(profiles, SessionKind::Agent, self.agent.as_deref()));` (import `SessionKind` if it is missing there).
- main.rs fan-out (:16147): `workspace_menu::agent_choices(profiles, SessionKind::Agent, None)`.
- lifecycle.rs comments:
  - :19-20: "an agent from the workspace's default profile or its default shell".
  - :28: "`None` = the project's `default_agent` for an agent, its `default_shell` for a shell."

Run: `cargo test -p mkrew-ui --lib workspace_menu -j 4` and `cargo test -p mkrew-ui --lib task_dialog -j 4`
Expected: PASS.

- [ ] **Step 6: UI: ask for a default shell; pick it in the workspace menu**

`agent_rows` (main.rs:15146-15186): add `kind: SessionKind,` after `agents: &Agents,`. In the `Loaded` arm:

```rust
        Agents::Loaded(profiles) => {
            let choices = workspace_menu::agent_choices(profiles, kind, current);
            if choices.is_empty() {
                let empty = match kind {
                    SessionKind::Agent => "No agent profiles. Add one to the profiles folder.",
                    SessionKind::Shell => {
                        "No shell found on PATH. Add a shell profile to the profiles folder."
                    }
                };
                vec![muted_line(empty.into()).into_any_element()]
            } else {
```

The callers `render_workspace_menu` (:7441), Run with… (:16073) and fan-out (:16152) pass `SessionKind::Agent` as the new second argument.

`set_default_shell`, after `set_default_agent` (:4016):

```rust
    /// Makes `shell` the default shell of `project_id` (ADR 0032); the sidebar picks the change
    /// up from `project.updated`.
    fn set_default_shell(
        &mut self,
        project_id: Ulid,
        shell: String,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = workspace_menu::set_default_shell(project_id, &shell);
        self.update_project(params, "Cannot change the default shell", window, cx);
    }
```

`render_workspace_menu`. After the `agents` binding (:7441-7448):

```rust
        let current_shell = (self.sidebar.projects().iter())
            .find(|project| project.id == project_id)
            .and_then(|project| project.default_shell.as_deref());
        // Loading and errors already show once, under the agents.
        let shells = match &menu.agents {
            Agents::Loaded(_) => agent_rows(
                &menu.agents,
                SessionKind::Shell,
                current_shell,
                "default-shell",
                theme,
                cx,
                move |shell, id, window, cx| shell.set_default_shell(project_id, id, window, cx),
            ),
            _ => Vec::new(),
        };
```

After `.children(agents)` (:7505):

```rust
            .child(separator())
            .child(
                div()
                    .px(px(space::S))
                    .pt(px(space::XS))
                    .text_size(font_size::META)
                    .font_weight(FontWeight(weight::MEDIUM))
                    .text_color(rgb(theme.text_muted))
                    .child("Default shell for new panes"),
            )
            .children(shells)
```

Change the doc comment (:7416-7418) to "…the default agent and the default shell for new panes, the workspace color…".

`new_pane`: change the doc (:6029-6030) to "Starts an agent or a shell (the workspace's default of that kind) in the workspace folder…". Replace :6053-6065 with:

```rust
        let project = (self.sidebar.projects().iter()).find(|project| project.id == project_id);
        let (default, what, button) = match kind {
            SessionKind::Agent => (project.map(|p| &p.default_agent), "agent", "+ Agent"),
            SessionKind::Shell => (project.map(|p| &p.default_shell), "shell", "+ Shell"),
        };
        if default.is_some_and(Option::is_none) {
            // The daemon would refuse; let the user pick one (the menu lists the profiles).
            self.tab_error = Some(format!(
                "Pick a default {what} for this workspace, then press {button} again."
            ));
            self.open_workspace_menu(project_id, window, cx);
            return;
        }
```

`new_pane_button` tip (:14017): `SessionKind::Shell => "New pane with the workspace's default shell",`.

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`
Expected: clean.

- [ ] **Step 7: ADR 0032**

`docs/decisions/0032-shell-profiles.md`:

```markdown
# 0032 — Shells are profiles; each workspace picks its default shell

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

The user wants + Shell to work like + Agent: pick the shell once, then every new shell pane uses
it. Today a shell pane always runs the `shell` profile. That profile has no command, so it gets
`%COMSPEC%` (cmd.exe on Windows). SPEC §10 promised "pwsh on Windows, configurable", but nothing
could be configured. Profiles already allow `kind = "shell"` with any command, and
`session.spawn` with a named shell profile already works.

## Options

1. Shell profiles (TOML files, like agents) plus `project.default_shell`, picked in the workspace
   menu like the default agent. No default = refuse and ask.
2. A global `[terminal] shell = "pwsh"` in settings.toml, with PATH detection in the UI. One value
   for every workspace, new settings and detection code, and not the agent model.
3. Option 1, but fall back to the `shell` profile when nothing is picked.

## Decision

Option 1.

- Mkrew ships `pwsh`, `powershell`, `cmd`, `bash` and `git-bash` profiles next to `shell` (the OS
  default). Seeding adds missing sample files at every daemon start, so existing config dirs get
  them too.
- `agent.profiles` marks each profile `installed` (its program is found the way a spawn would find
  it). The menu lists only installed shells.
- A shell pane without `default_shell` is refused (`invalid_request`), and the UI asks the user to
  pick one, exactly like a missing default agent. The user asked for this instead of option 3's
  silent fallback.
- `bash` is the first bash on PATH, which on Windows is often WSL's. So Git Bash ships as its own
  profile with the default install path.

Run commands keep the `shell` profile. Their typed line and exit syntax depend on that shell's
family (`run::typed_line`), and a run must not change shell when someone picks another default
for interactive panes.

## Consequences

- Migration `0014_project_default_shell.sql` (the next free number when this landed).
- Protocol +1: `Project.default_shell`, `ProjectUpdateParams.default_shell` (absent / null / id),
  `AgentProfileInfo.installed`.
- SPEC §5, §6, §7 and §10 (the "pwsh on Windows, configurable" sentence) are updated.
- Every existing workspace asks once, on its first new shell pane after the upgrade.
- If Git Bash is installed somewhere other than `C:\Program Files\Git`, the user edits `command`
  in `git-bash.toml`.
```

- [ ] **Step 8: SPEC**

§5 (:316-319): append to the migrations paragraph: ``` `0014_project_default_shell.sql` adds `project.default_shell TEXT` (shell profile for new shell panes, NULL = not picked; ADR 0032). ```

§6 (:421-424): replace the first two sentences with:

```markdown
`agents/shell.toml` (kind shell, no command = the OS default shell), the shell profiles
`pwsh.toml`, `powershell.toml`, `cmd.toml`, `bash.toml` (first `bash` on PATH) and `git-bash.toml`
(`C:\Program Files\Git\bin\bash.exe`) (ADR 0032), and `agents/fake.toml` (command =
`mkrew-fakeagent`, see §16) also ship. Seeding writes every missing sample at each daemon start and
never overwrites one. Optional key `kind = "agent" | "shell"` (default `agent`); a shell profile
may omit `command` to use the OS default shell (`%COMSPEC%` / `$SHELL`).
```

§7:
- Row :498: add `default_shell?` to the params, after `default_agent?`. In the returns, after "`default_agent` must be an agent profile;" add "`default_shell` must be a shell profile (same absent / `null` rules);".
- Row :499 (`project.run_start`): after "runs as the project's live `shell`-kind session titled `run:<name>`" add "(always the `shell` profile, not `default_shell`; ADR 0032)".
- Row :507 (`session.spawn`): replace "no `agent_profile` = project `default_agent` (agent) or `shell` (shell);" with "no `agent_profile` = project `default_agent` (agent) or `default_shell` (shell), `invalid_request` when that default is not set;".
- Row :521 (`agent.profiles`): "AgentProfile[] (each with `installed`: its program, or the OS shell for a shell without `command`, is found on PATH) + parse errors".

§10 (:691): replace the sentence with:

```markdown
  Sessions are `agent` (from a profile) or `shell` (a shell profile, §6: the workspace's default
  shell, picked in the workspace menu under "Default shell for new panes" like the default agent;
  a new shell pane without one asks the user to pick; ADR 0032).
```

- [ ] **Step 9: HUMAN_CHECKS**

In the 2026-10-09 section:

```markdown
- [ ] **Default shell (ADR 0032):** after the daemon restart, `<config>\mkrew\agents\` holds
  `pwsh.toml`, `powershell.toml`, `cmd.toml`, `bash.toml` and `git-bash.toml` next to your edited
  files (none overwritten). In a workspace without a default shell, + Shell (or Alt+Shift+S,
  Ctrl+Shift+T) shows "Pick a default shell…" and opens the workspace menu. "Default shell for new
  panes" lists only shells found on this machine. Pick PowerShell → + Shell opens pwsh in the
  workspace folder (`$PSVersionTable` shows 7). Pick Git Bash → `pwd` is the workspace folder.
  The Run button still runs through `shell` (cmd), as before.
```

- [ ] **Step 10: Verify and tick**

Run the full verification. Tick inbox line 35 `[x]`.
Suggested message: `feat: shell profiles and a per-workspace default shell picked like the default agent`

---

### Task 13: Rename a pane (F2, double-click its title) — `session.rename`

Inbox: line 32 `Add a short cut to rename pain, good if we can also auto rename base on what I'm working, if this feature is not too big`. This task is the manual part. Task 14 is the automatic part, and the user can drop it on its own. Spec: §7 (:507-518, :548-550), §11 (:741), §13 (:889-894).

**Current state (investigated):**
- Only tabs can be renamed today. The flow:
  - `Command::RenameTab` (keymap.rs:32, `ctrl-shift-r` :278) → `Shell::start_rename` (main.rs:6432) sets `renaming: Option<(Ulid, String)>` (main.rs:215).
  - `rename_key_down` (:6487): Enter commits, Esc cancels. A blur commits too (`_rename_blur`, main.rs:455).
  - `finish_rename` (:6447) → `Request::TabRename(tabs::rename_params(..))` (tabs.rs:445) → daemon server.rs:273 → `Event::TabUpdated`.
  - The tab bar shows the field through `render_rename` (:13436) at :13300. A double-click on a tab starts it (:13339).
- A pane is one session. Its header (`pane_header`, main.rs:16484) shows the status, the badge and `session.agent_profile`, then `run::pane_title(session, ports)` (run.rs:240). That is `session.title`, or the command name for a run session.
- `Session.title: Option<String>` (message.rs:458) is the only label source. It feeds:
  - sidebar rows (sidebar.rs:290) and the Inbox (inbox.rs:128)
  - palette, broadcast, doc dialog and Git labels (`history::label`, history.rs:121)
  - Peek (main.rs:7597)
  - OS notifications (daemon notify.rs:134)
- The title is set only at spawn: the UI always passes `None` (lifecycle.rs:32), a task run passes `task.title` (server.rs:588), and run commands pass `run:<name>` (server/run.rs:134). No request or event changes it after that.
- The store already writes it: `Store::update_session` (store lib.rs:723) updates `title`. **No migration.** Resume copies it (`title: last.title.clone()`, server/resume.rs:118), so a name survives a daemon restart.
- The session's actor owns a live session's row. `Actor::save` (session.rs:1292) writes the whole `self.session` on every state change, so the next save would undo a store-only rename. A live session must be renamed through a new actor `Command` (enum session.rs:198, loop :1018-1074).
- `run:` is load-bearing: `mkrew_core::run::command_name` (core run.rs:210) finds run sessions by that prefix. Renaming a run session, or giving another session a `run:` title, would break Run (ADR 0018). The daemon refuses both.
- No existing event carries a whole `Session` (Event enum message.rs:305-359), so `session.updated` is new. `sidebar::Sidebar::apply` (sidebar.rs:204) is the only exhaustive match on `Event`. `run::Ports::apply`, the CLI attach loop and the daemon use `_` or `if let`.
- F2 is unbound. A focused terminal sends it as `ESC O Q` (terminal/input.rs:185). Once bound, TUIs no longer get F2 (the same trade as F11 in Task 2). Users can rebind it in keymap.toml. This plan also takes `f11`, `ctrl-s`, `ctrl-f`, `ctrl-h`, `alt-shift-a` and `alt-shift-s`; F2 collides with none of them.

**Files:**
- Modify: `crates/mkrew-protocol/src/lib.rs:16` (`PROTOCOL_VERSION`)
- Modify: `crates/mkrew-protocol/src/message.rs`:
  - :135-136: new request after `SessionRemove`
  - :356-358: new event after `SessionPorts`
  - :976-978: new params after `SessionRef`
  - tests `every_m1_request_round_trips` (:1721) and `event_wire_shape_matches_spec` (:2179-2182)
- Modify: `crates/mkrew-daemon/src/session.rs:198-242` (`Command::Rename`), :1018-1074 (actor loop)
- Modify: `crates/mkrew-daemon/src/server.rs:325-327` (dispatch), new `session_rename` after `session_remove` (:1078-1084), imports :12-20
- Modify: `crates/mkrew-ui/src/keymap.rs:41, :162, :224, :291`, test `spec_defaults` (:660-661)
- Modify: `crates/mkrew-ui/src/palette.rs:188, :251-256`, test `availability_follows_the_view` (:700-750)
- Modify: `crates/mkrew-ui/src/lifecycle.rs:4-7` (imports), new `session_rename_params` after `spawn_params` (:21-36), test after `new_panes_use_the_default_profile_of_their_kind` (:186)
- Modify: `crates/mkrew-ui/src/sidebar.rs:220-249` (`apply`), test after `first_load_expands_workspaces_with_live_sessions_newest_first` (:417-440)
- Modify: `crates/mkrew-ui/src/main.rs`:
  - :214-215: the `renaming` field
  - :6177-6182 and :6207: `Shell::run`
  - :6431-6512: start, finish and key handling
  - :13300: tab bar
  - :13598-13680: `pane_frame`
  - :16478-16524: `pane_header`
- Test: `crates/mkrew-daemon/tests/daemon.rs`:
  - imports :14-36
  - new test after `removed_sessions_stop_and_leave_the_list` (:5573-5601)
  - `restart_resumes_free_agent_panes_in_tabs_once` (:4149-4183)
- Create: `docs/decisions/0031-session-rename-terminal-title.md`
- Modify: `docs/SPEC.md` §7 (:516, :550), §11 (:741), §13 (:894); `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces (protocol):
  - `Request::SessionRename(SessionRenameParams)`, method `session.rename`. Returns `Session`.
  - `pub struct SessionRenameParams { pub session_id: Ulid, #[serde(default)] pub title: Option<String> }`
  - `Event::SessionUpdated(Session)`, event `session.updated`.
  - Errors:
    - `not_found`: unknown id.
    - `invalid_request`: the title starts with `run:`.
    - `invalid_state`: a run session, or a session caught between its exit and its final save.
- Produces (daemon): `session::Command::Rename { title: Option<String>, reply: oneshot::Sender<Session> }`.
- Produces (UI):
  - `keymap::Command::RenamePane` (name `rename_pane`, default `f2`, context `None`)
  - `lifecycle::session_rename_params(session: &Session, draft: &str) -> Option<SessionRenameParams>`
  - in main.rs: `struct Renaming { target: RenameTarget, draft: String }`, `enum RenameTarget { Tab(Ulid), Session(Ulid) }`, and `Shell::start_pane_rename(&mut self, session_id: Ulid, window, cx)`
- Consumes: `Store::get_session` (store lib.rs:791), `Store::update_session` (:723), `Hub::sender` (session.rs:873), `mkrew_core::run::{command_name, SESSION_PREFIX}`.

- [ ] **Step 1: Failing protocol tests**

In `message.rs` `every_m1_request_round_trips`, after `Request::SessionRemove(..)` (:1721):

```rust
            Request::SessionRename(SessionRenameParams {
                session_id: sid(),
                title: Some("Fix login".into()),
            }),
```

In `event_wire_shape_matches_spec`, after the `SessionPorts` round trip (:2179-2182):

```rust
        let updated = round_trip(&Message::Event(Event::SessionUpdated(Session {
            id: sid(),
            task_id: None,
            project_id: sid(),
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: Some("Fix login".into()),
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Screen,
            state_changed_at: 1,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 1,
            ended_at: None,
        })));
        assert_eq!(updated["event"], "session.updated");
        assert_eq!(updated["data"]["title"], "Fix login");
```

Run: `cargo test -p mkrew-protocol --lib message::tests -j 4`
Expected: FAIL to compile (`no variant SessionRename`, `no variant SessionUpdated`).

- [ ] **Step 2: Protocol**

`message.rs`, after `SessionRemove(SessionRef),` (:136):

```rust
    /// Returns the renamed [`Session`], also announced as `session.updated`; a `null` or blank
    /// `title` clears it. Run sessions keep their `run:` name (ADR 0031).
    #[serde(rename = "session.rename")]
    SessionRename(SessionRenameParams),
```

After `SessionPorts(SessionPortsEvent),` in `Event` (:358):

```rust
    /// A session's title changed (`session.rename`, ADR 0031); carries the whole session.
    #[serde(rename = "session.updated")]
    SessionUpdated(Session),
```

After `SessionRef` (:976-978):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRenameParams {
    pub session_id: Ulid,
    /// Trimmed by the daemon; `None` or blank clears the title.
    #[serde(default)]
    pub title: Option<String>,
}
```

In `lib.rs:16`, bump `PROTOCOL_VERSION` by one from its value when this task lands (new request and event).

Run: `cargo test -p mkrew-protocol --lib message::tests -j 4`
Expected: PASS.

- [ ] **Step 3: Failing daemon tests**

In `tests/daemon.rs`, add `SessionRenameParams` to the `mkrew_protocol` import (:14-36). After `removed_sessions_stop_and_leave_the_list` (:5601):

```rust
fn rename(session_id: Ulid, title: Option<&str>) -> Request {
    Request::SessionRename(SessionRenameParams {
        session_id,
        title: title.map(Into::into),
    })
}

/// `session.rename` (ADR 0031): a live session keeps the name through its own later saves
/// (state changes, exit); an exited one is renamed in the store; blank clears; run sessions
/// and `run:` titles are refused.
#[test]
fn sessions_are_renamed_live_and_exited_but_run_sessions_refuse() {
    let env = Env::new();
    env.write_fake_profile();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);

    let renamed: Session = client
        .call(rename(id, Some("  Fix login  ")))
        .into_result()
        .unwrap();
    assert_eq!(renamed.title.as_deref(), Some("Fix login"));
    client.wait_event_where(|e| {
        matches!(e, Event::SessionUpdated(s) if s.id == id && s.title.as_deref() == Some("Fix login"))
    });
    // The actor's final save (exit) keeps the new title.
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    let exited = client.wait_state(id, SessionState::Exited);
    assert_eq!(exited.title.as_deref(), Some("Fix login"));

    // Exited: renamed in the store; blank clears.
    let cleared: Session = client.call(rename(id, Some("   "))).into_result().unwrap();
    assert_eq!(cleared.title, None);
    assert_eq!(client.session(id).title, None);
    client.wait_event_where(
        |e| matches!(e, Event::SessionUpdated(s) if s.id == id && s.title.is_none()),
    );
    assert_eq!(
        error_code(client.call(rename(id, Some("run:web")))),
        ErrorCode::InvalidRequest
    );
    assert_eq!(
        error_code(client.call(rename(Ulid::new(), Some("x")))),
        ErrorCode::NotFound
    );

    // A run session is named by its command.
    let web = run_command("web", ".", &long("echo up", "echo up"));
    client
        .call(set_runs(&project, vec![web], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let web = started.sessions[0].session_id;
    assert_eq!(
        error_code(client.call(rename(web, Some("mine")))),
        ErrorCode::InvalidState
    );
    client
        .call(run_stop(&project, None))
        .into_result::<()>()
        .unwrap();
    client.wait_state(web, SessionState::Exited);
    client.stop();
}
```

In `restart_resumes_free_agent_panes_in_tabs_once`, after the native-id wait loop (:4149-4155), add:

```rust
    client
        .call(rename(shown.session_id, Some("Fix login")))
        .into_result::<Session>()
        .unwrap();
```

After the `(None, "resumable", Some("native-7"))` assert (:4176-4183), add:

```rust
    assert_eq!(
        resumed.title.as_deref(),
        Some("Fix login"),
        "the resumed pane keeps its name"
    );
```

Run: `cargo test -p mkrew-daemon --test daemon sessions_are_renamed -j 2`
Expected: FAIL: the `session.rename` call returns an error from the dispatch's `other =>` arm (server.rs:418).

- [ ] **Step 4: Daemon**

In `session.rs` `Command`, after `TurnFiles(Vec<String>),` (:241):

```rust
    /// `session.rename`: the actor owns the row (its next save would undo a store-only
    /// rename). Replies with the renamed session, also announced as `session.updated`.
    Rename {
        title: Option<String>,
        reply: oneshot::Sender<Session>,
    },
```

In the actor loop, after the `TurnFiles` arm (:1072):

```rust
                    Some(Command::Rename { title, reply }) => {
                        self.session.title = title;
                        self.save();
                        let _ = self.hub.events.send(Event::SessionUpdated(self.session.clone()));
                        // The caller may have gone away; the rename stands.
                        let _ = reply.send(self.session.clone());
                    }
```

In `server.rs`, add `SessionRenameParams` to the `mkrew_protocol` import (:12-20). Dispatch, after the `SessionRemove` arm (:325-327):

```rust
            Request::SessionRename(params) => (self.session_rename(params).await, After::Continue),
```

After `session_remove` (:1078-1084):

```rust
    /// `session.rename` (ADR 0031): a live session through its task, an exited one in the
    /// store. Run sessions are named by their command (`run:<name>`, ADR 0018), so they and
    /// `run:` titles are refused.
    async fn session_rename(&self, params: SessionRenameParams) -> Outcome {
        use mkrew_core::run::{SESSION_PREFIX, command_name};
        let title = params
            .title
            .as_deref()
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned);
        if title.as_deref().is_some_and(|t| t.starts_with(SESSION_PREFIX)) {
            let message = format!("a pane title cannot start with {SESSION_PREFIX:?}");
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let found = self.store().get_session(params.session_id);
        let session = match found {
            Ok(session) => session,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if command_name(&session).is_some() {
            let message = "a run session is named by its command";
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        if let Some(sender) = self.hub.sender(session.id) {
            let (reply, renamed) = oneshot::channel();
            let command = Command::Rename {
                title: title.clone(),
                reply,
            };
            if sender.send(command).await.is_ok()
                && let Ok(renamed) = renamed.await
            {
                return ok(&renamed);
            }
        }
        // Not live (any more): re-read, its task may have saved the exit meanwhile.
        let found = self.store().get_session(params.session_id);
        let mut session = match found {
            Ok(session) => session,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        if session.state != SessionState::Exited {
            // Between leaving `live` and its final save: that save would undo this rename.
            let message = format!("session {} is exiting; try again", session.id);
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        session.title = title;
        let saved = self.store().update_session(&session);
        if let Err(e) = saved {
            return store_outcome::<()>(Err(e));
        }
        let _ = self.hub.events.send(Event::SessionUpdated(session.clone()));
        ok(&session)
    }
```

Each `self.store()` guard is dropped at the end of its own `let`. Never write `match self.store()…` with another `self.store()` inside: that deadlocks.

Run: `cargo test -p mkrew-daemon --test daemon sessions_are_renamed -j 2` and `cargo test -p mkrew-daemon --test daemon restart_resumes_free_agent_panes -j 2`
Expected: PASS.

- [ ] **Step 5: Failing UI tests**

In `keymap.rs` `spec_defaults`, after the `ctrl-shift-m` assert (:661):

```rust
        assert_eq!(bound("f2"), Some(Command::RenamePane));
        assert_eq!(keymap.label(Command::RenamePane), "F2");
        // Global: also while a terminal has focus (it no longer gets ESC O Q).
        assert_eq!(Command::RenamePane.context(), None);
```

In `palette.rs` `availability_follows_the_view`, add `Command::RenamePane,` to the first list (the commands Terminals offers, :700-708). After the `ClosePane` assert in the Git block (:747-750), add:

```rust
        assert!(!git.contains(&Command::RenamePane), "panes are not on screen");
```

In the `lifecycle.rs` tests, after `new_panes_use_the_default_profile_of_their_kind`:

```rust
    #[test]
    fn session_rename_params_trim_clear_and_skip_unchanged() {
        let mut named = session(Ulid::new(), SessionKind::Agent, SessionState::Working);
        let params = session_rename_params(&named, "  Fix login  ").unwrap();
        assert_eq!(params.session_id, named.id);
        assert_eq!(params.title.as_deref(), Some("Fix login"));
        // Blank on an untitled pane: nothing to do.
        assert_eq!(session_rename_params(&named, "   "), None);
        named.title = Some("Fix login".into());
        assert_eq!(session_rename_params(&named, " Fix login "), None);
        // Blank clears an existing title.
        let cleared = session_rename_params(&named, "  ").unwrap();
        assert_eq!(cleared.title, None);
    }
```

In the `sidebar.rs` tests, after `first_load_expands_workspaces_with_live_sessions_newest_first`:

```rust
    #[test]
    fn a_renamed_session_shows_its_new_title_and_an_unknown_one_refreshes() {
        let api = project("api");
        let live = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![live.clone()]);
        let renamed = Session {
            title: Some("Fix login".into()),
            ..live
        };
        assert_eq!(
            sidebar.apply(&Event::SessionUpdated(renamed)),
            Change::Redraw
        );
        let Row::Session(row) = &sidebar.rows()[1] else {
            panic!("expected the session row");
        };
        assert_eq!(row.title.as_deref(), Some("Fix login"));
        let stranger = session(&api, 2, SessionState::Working);
        assert_eq!(
            sidebar.apply(&Event::SessionUpdated(stranger)),
            Change::Refresh
        );
    }
```

Run: `cargo test -p mkrew-ui --lib keymap::tests::spec_defaults -j 4`
Expected: FAIL to compile (`no variant RenamePane`, `cannot find function session_rename_params`, non-exhaustive `Event` in `Sidebar::apply`).

- [ ] **Step 6: Keymap, palette, helper, sidebar**

`keymap.rs`, in the enum after `ClosePane,` (:41):

```rust
    /// Edit the focused pane's (session's) title inline in its header.
    RenamePane,
```

Then:
- `all()`: `Command::RenamePane,` after `Command::ClosePane,` (:162).
- `name()`: `Command::RenamePane => "rename_pane".into(),` after :224.
- `default_keys()`: `Command::RenamePane => "f2".into(),` after :291.
- `context()`: no change (`_ => None`).

`palette.rs`:
- `title()`, after :188: `Command::RenamePane => "Panes: Rename pane".into(),`
- `available()`: add `| Command::RenamePane` to the `on_panes` arm (:251-256).

`lifecycle.rs`: add `SessionRenameParams` to the `mkrew_protocol` import (:4-7). After `spawn_params` (:36):

```rust
/// The `session.rename` for an edited pane title, trimmed; blank clears the title. `None`
/// when nothing changes (ADR 0031).
pub fn session_rename_params(session: &Session, draft: &str) -> Option<SessionRenameParams> {
    let title = Some(draft.trim()).filter(|title| !title.is_empty());
    (title != session.title.as_deref()).then(|| SessionRenameParams {
        session_id: session.id,
        title: title.map(str::to_owned),
    })
}
```

In `sidebar.rs` `apply`, before the `Event::ProjectUpdated` arm (:230):

```rust
            Event::SessionUpdated(updated) => {
                self.update_session(updated.id, |session| *session = updated.clone())
            }
```

Leave it out of the `since_load` exclusion list (:206-216), so a rename that races a reload is replayed.

Run: `cargo test -p mkrew-ui --lib keymap -j 4`, `cargo test -p mkrew-ui --lib palette -j 4`, `cargo test -p mkrew-ui --lib lifecycle -j 4`, `cargo test -p mkrew-ui --lib sidebar -j 4`
Expected: compile errors in main.rs only (`renaming` users, fixed in the next step). After Step 7, all four PASS. `every_default_parses_and_is_unique`, `names_round_trip` and `template_parses_to_the_defaults_commented_or_not` cover the new entry through `all()`.

- [ ] **Step 7: Shell — one inline rename for tabs and panes**

Replace the field (main.rs:214-215):

```rust
    /// The tab or pane title being edited inline.
    renaming: Option<Renaming>,
```

Add these types next to the other private types of main.rs:

```rust
/// What the inline title field edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenameTarget {
    Tab(Ulid),
    /// A pane's session (`session.rename`).
    Session(Ulid),
}

/// The inline title field: what it renames and the draft.
#[derive(Debug, Clone)]
struct Renaming {
    target: RenameTarget,
    draft: String,
}
```

These lines stay as they are: `shell.renaming = None;` (:531), `renaming: None,` (:582), `_rename_blur` (:455-457) and `finish_rename(true, …)` at the top of `Shell::run` (:6097).

`Shell::run`: in the `show_view` guard (:6177-6182), add `| Command::RenamePane` after `Command::RenameTab`. After the `Command::RenameTab` arm (:6207):

```rust
            Command::RenamePane => {
                if let Some(session_id) = self.focused_session() {
                    self.start_pane_rename(session_id, window, cx);
                }
            }
```

Replace `start_rename` through `rename_key_down` (:6431-6512) with:

```rust
    /// Starts editing the active tab's title inline, starting from the current one.
    fn start_rename(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(tab) = self
            .workspace
            .and_then(|id| self.tabs.get(&id))
            .and_then(|tabs| tabs.active())
        else {
            return;
        };
        self.renaming = Some(Renaming {
            target: RenameTarget::Tab(tab.id),
            draft: tab.title.clone(),
        });
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    /// Starts editing `session_id`'s title inline in its pane header (F2, double-click),
    /// from the user's own title. Run sessions are named by their command: left alone.
    fn start_pane_rename(
        &mut self,
        session_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == session_id) else {
            return;
        };
        if mkrew_core::run::command_name(session).is_some() {
            return;
        }
        self.renaming = Some(Renaming {
            target: RenameTarget::Session(session_id),
            draft: session.title.clone().unwrap_or_default(),
        });
        window.focus(&self.rename_focus, cx);
        cx.notify();
    }

    /// Ends the inline edit: `commit` sends `tab.rename` / `session.rename`; the reply shows
    /// at once, the `tab.updated` / `session.updated` event tells other clients. Unchanged
    /// drafts and blank tab titles are dropped (a blank pane title clears it). Focus goes
    /// back to the pane.
    fn finish_rename(&mut self, commit: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(Renaming { target, draft }) = self.renaming.take() else {
            return;
        };
        if commit {
            match target {
                RenameTarget::Tab(tab_id) => self.rename_tab(tab_id, &draft, window, cx),
                RenameTarget::Session(session_id) => {
                    self.rename_session(session_id, &draft, window, cx);
                }
            }
        }
        if self.rename_focus.is_focused(window) {
            self.focus_pane(window, cx);
        }
        cx.notify();
    }

    fn rename_tab(
        &mut self,
        tab_id: Ulid,
        draft: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        // Any workspace: switching workspaces mid-edit commits through the blur.
        let params = self
            .tabs
            .values()
            .find_map(|tabs| tabs.tab(tab_id))
            .and_then(|tab| rename_params(tab, draft));
        let (Some(params), Some(connection)) = (params, self.connection.clone()) else {
            return;
        };
        self.tab_error = None;
        let renamed = connection.call::<TerminalTab>(Request::TabRename(params));
        cx.spawn_in(window, async move |this, cx| {
            let renamed = renamed.await;
            let _ = this.update(cx, |shell, cx| {
                match renamed {
                    // Shown at once, without waiting for the `tab.updated` echo.
                    Ok(tab) => {
                        if let Some(tabs) = shell.tabs.get_mut(&tab.project_id) {
                            tabs.apply(&Event::TabUpdated(tab));
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%tab_id, "tab.rename failed: {error:#}");
                        shell.tab_error = Some(format!("Cannot rename the tab: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn rename_session(
        &mut self,
        session_id: Ulid,
        draft: &str,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let params = self
            .sidebar
            .sessions()
            .iter()
            .find(|s| s.id == session_id)
            .and_then(|session| lifecycle::session_rename_params(session, draft));
        let (Some(params), Some(connection)) = (params, self.connection.clone()) else {
            return;
        };
        self.tab_error = None;
        let renamed = connection.call::<Session>(Request::SessionRename(params));
        cx.spawn_in(window, async move |this, cx| {
            let renamed = renamed.await;
            let _ = this.update(cx, |shell, cx| {
                match renamed {
                    // Shown at once (header, sidebar, Inbox), without waiting for the echo.
                    Ok(session) => {
                        shell.sidebar.apply(&Event::SessionUpdated(session));
                    }
                    Err(error) => {
                        tracing::warn!(%session_id, "session.rename failed: {error:#}");
                        shell.tab_error = Some(format!("Cannot rename the pane: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Keys typed while renaming edit the draft; Enter commits, Esc cancels.
    fn rename_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        let Some(renaming) = self.renaming.as_mut() else {
            return;
        };
        match key {
            FindKey::Close => self.finish_rename(false, window, cx),
            FindKey::Older | FindKey::Newer => self.finish_rename(true, window, cx),
            FindKey::Backspace => {
                renaming.draft.pop();
                cx.notify();
            }
            FindKey::Insert(text) => {
                renaming.draft.push_str(&text);
                cx.notify();
            }
        }
    }
```

Tab bar (:13300):

```rust
                if let Some(renaming) = self
                    .renaming
                    .as_ref()
                    .filter(|r| r.target == RenameTarget::Tab(tab_id))
                {
                    return self.render_rename(&renaming.draft, theme, cx);
                }
```

`render_rename` (:13436-13471) is reused unchanged. In `pane_frame`, inside the `info.map(|info| { … })` closure and before `pane_header(` (:13669), build the title slot:

```rust
                let renaming = self
                    .renaming
                    .as_ref()
                    .filter(|r| r.target == RenameTarget::Session(session));
                let title = match renaming {
                    // Mouse downs stay in the field: the pane's handler would focus the
                    // terminal, and the blur would commit half-typed text.
                    Some(renaming) => div()
                        .flex_1()
                        .min_w_0()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_, _, _, cx| cx.stop_propagation()),
                        )
                        .child(self.render_rename(&renaming.draft, theme, cx))
                        .into_any_element(),
                    None => {
                        let renamable = mkrew_core::run::command_name(info).is_none();
                        div()
                            .id(SharedString::from(format!("pane-title-{session}")))
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(rgb(theme.text_muted))
                            .children(run::pane_title(info, self.ports.get(info.id)))
                            // Also the empty space after the agent name of an untitled pane.
                            .when(renamable, |el| {
                                el.on_click(cx.listener(
                                    move |shell, event: &gpui::ClickEvent, window, cx| {
                                        if event.click_count() == 2 {
                                            shell.start_pane_rename(session, window, cx);
                                        }
                                    },
                                ))
                            })
                            .into_any_element()
                    }
                };
```

In the `pane_header(…)` call (:13673), pass `title` instead of `self.ports.get(info.id)`.

`pane_header` (:16484-16524):
- Replace the parameter `ports: &[u16],` with `title: gpui::AnyElement,`.
- Replace the muted title child (:16515-16524) with `.child(title)`.
- In its doc comment (:16478-16482), add "the title (renamed inline with F2 or a double-click, ADR 0031)".

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`, then `cargo test -p mkrew-ui --lib -j 4`
Expected: no warnings; PASS.

- [ ] **Step 8: ADR 0031**

Create `docs/decisions/0031-session-rename-terminal-title.md`:

```markdown
# 0031 — Rename a pane (session.rename) and name it from its terminal title

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

The user wants a shortcut to rename a pane and, if cheap, an automatic name taken from the
current work. A pane is one session. `session.title` (nullable, persisted) already labels it in
the pane header, sidebar, Inbox, palette, notifications and resume. But it was set only at spawn,
and no request changed it. `run:<name>` titles mark Run sessions (ADR 0018).

## Options

1. Rename the session: `session.rename` + `session.updated`. The name follows the session
   everywhere and survives a daemon restart (resume copies it).
2. A per-pane label in the tab layout JSON: a second naming system that nothing outside the tab
   sees.
3. Reuse the tab rename (Ctrl+Shift+R): it names the tab, not the pane.

## Decision

Option 1. A live session is renamed through its task, because the task owns the row and saves
it on every state change. An exited session is renamed in the store. A blank title clears the
name. Run sessions and `run:` titles are refused, so Run keeps finding its sessions. The
shortcut is F2, the Windows rename key: TUIs lose F2, and users can rebind it. A double-click
on the header title does the same.

## Consequences

Protocol: a `session.rename` request, a `session.updated` event (the whole Session) and a
version bump. SPEC: §7 rows, the §11 header wording and a §13 row. No migration, because the
column exists. The optional `mk session rename` stays out until someone asks for it.
```

- [ ] **Step 9: SPEC**

§7 Requests, after the `session.remove` row (:516):

```markdown
| `session.rename` | `session_id`, `title?` (trimmed; `null`/blank clears) | the Session, also broadcast as `session.updated` — `invalid_state` for a run session (named by its command) or one caught exiting, `invalid_request` for a title starting with `run:` (ADR 0031) |
```

§7 Events, after the `task.updated / project.updated / tab.updated` row (:550):

```markdown
| `session.updated` | a session's title changed (`session.rename`): the whole Session |
```

§11 (:741): replace "session/task title" with:

```markdown
session/task title (F2 or a double-click renames it inline: Enter saves, Esc cancels, blank
clears; run panes keep their command name — ADR 0031)
```

§13, after the `Close pane` row (:894):

```markdown
| Panes | Rename pane (also: double-click its title) | F2 |
```

- [ ] **Step 10: HUMAN_CHECKS**

Add to the `### To verify — 2026-10-09 inbox: …` section (Task 1 creates it):

```markdown
- [ ] **Rename pane:**
  - Focus an agent pane and press F2: the header title becomes a field. Type `Fix login` and
    press Enter. The header, the sidebar row, the Inbox entry (when it needs input) and its OS
    notification all show `Fix login`.
  - F2, then Esc: nothing changes. F2, clear the text, Enter: the pane has no title again.
  - Double-click the header title, or the empty space after the agent name: the same field opens.
  - Stop and restart the daemon (`mk daemon stop`, reopen Mkrew): the resumed pane keeps its name.
  - On a Run pane (`▶ Run`), F2 and double-click do nothing.
```

- [ ] **Step 11: Verify**

Run the full verification. If the user dropped Task 14, tick inbox line 32 `[x]` now. Otherwise, tick it after Task 14.
Suggested message: `feat: rename a pane with F2 or a double-click (session.rename, ADR 0031)`

---

### Task 14: Name a pane from its agent's terminal title (OSC 0/2)

Inbox: line 32 `Add a short cut to rename pain, good if we can also auto rename base on what I'm working, if this feature is not too big`. This task is the automatic part and needs Task 13. Spec: §5 (:240), §7 (:508, :549-550), §11 (:741).

**Current state (investigated):**
- Agents already name their work in the terminal title. Claude Code sets OSC 0 to an auto-generated summary of the conversation, with a spinner or status glyph in front (`✳ Fix login`).
- Mkrew drops the title in both emulators: the UI emulator uses `VoidListener` (terminal/emulator.rs:10), and the daemon's `ReplyListener` (daemon emulator.rs:21-33) keeps only `Event::PtyWrite`. Using the title needs no per-agent code (§2.1). [INFERENCE] Whether Codex, omp and Copilot set titles is unverified; whatever they set is used the same way.
- How alacritty_terminal 0.26 reports titles:
  - OSC 0/2 → `Term::set_title(Some(title))` → `Event::Title(String)` (vte 0.15 ansi.rs:1350-1359). vte trims the text already, and it may be empty.
  - `CSI 23 t` that pops a `None` → `set_title(None)` → `Event::ResetTitle` (term/mod.rs:2221-2256).
  - `Term` has no title getter, so the listener has to record the title.
- A title alone does not change `text_fingerprint` (test `fingerprint_ignores_invisible_output_and_sees_new_text`, daemon emulator.rs:279-290), so state detection is unaffected. Claude repaints its title on every spinner frame, so the daemon dedupes after cleaning.
- [INFERENCE, ConPTY behavior] On Windows, ConPTY passes titles through and gives a new console a title with the program path (`C:\…\node.exe`). Shells (pwsh, cmd) set their title to paths or the running command. Rule chosen:
  - Only agent sessions get an automatic name, so shell headers do not change with every command.
  - A title that contains `\` (a path), or has nothing left after cleaning, is ignored and the current name stays.
  - Only `ResetTitle` or an empty OSC title clears the name.
- Ports are the in-memory pattern to copy: `Hub.ports` (session.rs:98-99, `session_ports` :306) is filled by `Actor::scan_ports` (:1328-1350), which sends `Event::SessionPorts`. Late clients ask `session.ports` per workspace.
- The terminal title instead rides on `session.list` (what every client loads sessions with; sidebar.rs:169) as a field on `Session`. That needs no second request and no second UI store, and Task 13's `session.updated` carries it too.
- `Session` is built as a literal in 26 places (3 of them outside tests). Each gets `terminal_title: None,`:
  - core: run.rs:344
  - daemon: awake.rs:382, notify.rs:309, server.rs:790, server/resume.rs:235, turns.rs:290, tests/daemon.rs:674
  - store: lib.rs:1432 (`into_session`), tests.rs:10
  - ui: board.rs:580, broadcast.rs:246, conflicts.rs:171, doc_dialog.rs:325, fanout.rs:400, git_view.rs:2301, history.rs:209, inbox.rs:211, lifecycle.rs:166, palette.rs:602, run.rs:283, sidebar.rs:347, status.rs:186, tabs.rs:892, turns_dialog.rs:191, undo_dialog.rs:326, usage.rs:185
  - plus the one literal Task 13 adds (message.rs `event_wire_shape_matches_spec`)
- Places that read `session.title` for a label:
  - `history::label` (history.rs:121), used by broadcast.rs:101, doc_dialog.rs:159, git_view.rs:142-145, palette.rs:366 and main.rs:12997/13134
  - `run::pane_title` (run.rs:242)
  - sidebar rows (sidebar.rs:290), the Inbox (inbox.rs:128), Peek (main.rs:7597)
  - notifications (daemon notify.rs:134)

**Files:**
- Modify: `crates/mkrew-protocol/src/lib.rs:16`
- Modify: `crates/mkrew-protocol/src/message.rs:458` (field), :358 (event), :1340-1343 (new struct after `SessionPortsEvent`), test `event_wire_shape_matches_spec`
- Modify: every `Session { … }` literal listed above (`terminal_title: None,` after `title: …,`)
- Modify: `crates/mkrew-daemon/src/emulator.rs:17-65` (listener, `take_title`, `clean_title`), tests after :290
- Modify: `crates/mkrew-daemon/src/session.rs`:
  - :18-21: imports
  - :98-99 and :296: `Hub.terminal_titles`
  - :305-308: `terminal_title`
  - :1103-1113: `on_output`
  - new `on_terminal_title` after `scan_ports` (:1350)
- Modify: `crates/mkrew-daemon/src/server.rs:320-323` (`SessionList`), `session_rename` (Task 13, store path)
- Modify: `crates/mkrew-daemon/src/notify.rs:134`
- Modify: `crates/mkrew-ui/src/history.rs:120-126` (+ test), `run.rs:242`, `sidebar.rs:220-249, :290`, `inbox.rs:128`, `main.rs:7597`
- Test: `crates/mkrew-daemon/tests/daemon.rs` (new test after Task 13's)
- Modify: `docs/decisions/0031-session-rename-terminal-title.md`, `docs/SPEC.md` §5/§7/§11, `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces (protocol):
  - `Session.terminal_title: Option<String>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`. The daemon fills it in `session.list`, `session.rename` and `session.updated`; it is never stored.
  - `Event::SessionTerminalTitle(SessionTerminalTitleEvent)`, event `session.terminal_title`.
  - `pub struct SessionTerminalTitleEvent { pub session_id: Ulid, pub title: Option<String> }`
- Produces (daemon):
  - `emulator::TITLE_MAX_CHARS: usize = 60`
  - `emulator::clean_title(raw: &str) -> Option<String>`
  - `Emulator::take_title(&self) -> Option<Option<String>>`: `Some` means the program set (`Some`) or reset (`None`) its title since the last call; the latest wins.
  - `Hub.terminal_titles: Mutex<HashMap<Ulid, String>>` and `Hub::terminal_title(&self, id: Ulid) -> Option<String>`
- Produces (UI): `history::title(session: &Session) -> Option<&str>` (the user's title, else the terminal title). `history::label` becomes `title`, else the agent profile.
- Consumes: Task 13's `session.updated`, `session_rename` and `rename` test helper.

- [ ] **Step 1: Failing emulator tests**

In the daemon `emulator.rs` tests, after `fingerprint_ignores_invisible_output_and_sees_new_text`:

```rust
    #[test]
    fn osc_titles_are_taken_once_and_the_latest_wins() {
        let mut emu = Emulator::new(20, 4);
        assert_eq!(emu.take_title(), None);
        emu.advance("\x1b]0;✳ Fix login\x07".as_bytes());
        assert_eq!(emu.take_title(), Some(Some("✳ Fix login".to_owned())));
        assert_eq!(emu.take_title(), None, "already taken");
        // Push the (unset) title, set one, pop: the program reset it.
        emu.advance(b"\x1b[22t\x1b]2;x\x07\x1b[23t");
        assert_eq!(emu.take_title(), Some(None));
    }

    #[test]
    fn clean_titles_drop_leading_glyphs_paths_and_overflow() {
        assert_eq!(clean_title("✳ Fix login").as_deref(), Some("Fix login"));
        assert_eq!(
            clean_title("⠋  Sửa lỗi đăng nhập ").as_deref(),
            Some("Sửa lỗi đăng nhập")
        );
        assert_eq!(clean_title("✳ "), None);
        assert_eq!(clean_title(r"C:\Program Files\nodejs\node.exe"), None);
        let long = "a".repeat(80);
        assert_eq!(
            clean_title(&long).map(|t| t.chars().count()),
            Some(TITLE_MAX_CHARS)
        );
    }
```

Run: `cargo test -p mkrew-daemon --lib emulator::tests -j 4`
Expected: FAIL to compile (`no method take_title`, `cannot find function clean_title`).

- [ ] **Step 2: Emulator**

Replace `ReplyListener` (:19-33):

```rust
/// Collects what the terminal reports to its owner: bytes to send back to the program (e.g.
/// DSR replies; ConPTY asks for the cursor position on startup and stalls until it gets an
/// answer) and the last title the program set (OSC 0/2, ADR 0031).
#[derive(Clone, Default)]
struct ReplyListener {
    replies: Arc<Mutex<Vec<u8>>>,
    /// `Some` once the title changed since [`Emulator::take_title`]; `Some(None)` = reset.
    title: Arc<Mutex<Option<Option<String>>>>,
}

impl EventListener for ReplyListener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => {
                let mut replies = self.replies.lock().unwrap_or_else(PoisonError::into_inner);
                replies.extend_from_slice(text.as_bytes());
            }
            Event::Title(title) => {
                *self.title.lock().unwrap_or_else(PoisonError::into_inner) = Some(Some(title));
            }
            Event::ResetTitle => {
                *self.title.lock().unwrap_or_else(PoisonError::into_inner) = Some(None);
            }
            _ => {}
        }
    }
}
```

After `advance` (:57-65):

```rust
    /// The title the program set (`Some(Some)`) or reset (`Some(None)`) since the last call;
    /// `None` when it did neither.
    pub fn take_title(&self) -> Option<Option<String>> {
        self.listener
            .title
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
```

At module level, after `SCROLLBACK_LINES` (:17):

```rust
/// Characters kept of a terminal title used as a pane name.
pub const TITLE_MAX_CHARS: usize = 60;

/// A terminal title as a pane name (ADR 0031): leading spinner and status glyphs (Claude's
/// `✳ `) and blanks dropped, at most [`TITLE_MAX_CHARS`] characters. `None` when nothing
/// usable is left, or for a path (ConPTY and shells title a console with a path).
// ponytail: also drops a leading `(` or `#` of a real title; a glyph list if that bites.
pub fn clean_title(raw: &str) -> Option<String> {
    let title = raw
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_end();
    if title.is_empty() || title.contains('\\') {
        return None;
    }
    Some(title.chars().take(TITLE_MAX_CHARS).collect())
}
```

Run: `cargo test -p mkrew-daemon --lib emulator::tests -j 4`
Expected: PASS (`cursor_position_request_is_answered` still passes).

- [ ] **Step 3: Failing protocol and daemon tests**

In `message.rs` `event_wire_shape_matches_spec`, after Task 13's `session.updated` asserts:

```rust
        let titled = round_trip(&Message::Event(Event::SessionTerminalTitle(
            SessionTerminalTitleEvent {
                session_id: sid(),
                title: Some("Fix login".into()),
            },
        )));
        assert_eq!(titled["event"], "session.terminal_title");
        // Absent on the wire when unset.
        assert!(updated["data"].get("terminal_title").is_none());
```

In `tests/daemon.rs`, add `SessionTerminalTitleEvent` to the import. After Task 13's test:

```rust
/// An agent's OSC title names its session (ADR 0031): cleaned, announced once per change,
/// listed with the session; a user title is kept beside it.
#[test]
fn agent_terminal_titles_name_the_session_once_per_change() {
    let env = Env::new();
    let scenario = env.dir.path().join("titled.toml");
    // Two spinner frames of one title, then the prompt.
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"print\"\ntext = \"\\u001b]0;\\u2733 Fix login\\u0007\"\n\n\
         [[step]]\naction = \"print\"\ntext = \"\\u001b]0;\\u2736 Fix login\\u0007\"\n\n\
         [[step]]\naction = \"ask\"\n",
    )
    .unwrap();
    env.write_fake_profile_as("titled", &format!("['--scenario', '{}']", scenario.display()));
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "titled").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::NeedsInput);
    let titled = |e: &Event| matches!(e, Event::SessionTerminalTitle(t) if t.session_id == id);
    client.wait_event_where(titled);
    let announced: Vec<&Event> = client.events.iter().filter(|e| titled(e)).collect();
    assert_eq!(
        announced,
        [&Event::SessionTerminalTitle(SessionTerminalTitleEvent {
            session_id: id,
            title: Some("Fix login".into()),
        })]
    );
    assert_eq!(client.session(id).terminal_title.as_deref(), Some("Fix login"));

    // The user's name is stored beside it (clients show it first).
    let renamed: Session = client
        .call(rename(id, Some("Mine")))
        .into_result()
        .unwrap();
    assert_eq!(
        (renamed.title.as_deref(), renamed.terminal_title.as_deref()),
        (Some("Mine"), Some("Fix login"))
    );
    client.stop();
}
```

Run: `cargo test -p mkrew-protocol --lib message::tests -j 4`
Expected: FAIL to compile (`no variant SessionTerminalTitle`).

- [ ] **Step 4: Protocol**

In `Session` (:458), after `pub title: Option<String>,`:

```rust
    /// The title the session's agent gave its terminal (OSC 0/2), cleaned; daemon memory
    /// only, filled in `session.list` / `session.rename` / `session.updated`. Shown when
    /// `title` is unset (ADR 0031).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_title: Option<String>,
```

In `Event`, after Task 13's `SessionUpdated(Session),`:

```rust
    /// An agent session's terminal title changed, cleaned (`None` = cleared; ADR 0031).
    #[serde(rename = "session.terminal_title")]
    SessionTerminalTitle(SessionTerminalTitleEvent),
```

After `SessionPortsEvent` (:1340-1343):

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTerminalTitleEvent {
    pub session_id: Ulid,
    pub title: Option<String>,
}
```

Add `terminal_title: None,` after `title: …,` in every `Session { … }` literal listed under Current state. This includes the store's `into_session` (lib.rs:1432), because there is no such column. In `lib.rs:16`, bump `PROTOCOL_VERSION` by one from its value when this task lands.

Run: `cargo test -p mkrew-protocol --lib message::tests -j 4`
Expected: PASS. The daemon test still fails: nothing sets the title yet.

- [ ] **Step 5: Daemon**

In `session.rs`, import `SessionTerminalTitleEvent` (:18-21). In `Hub`, after `ports` (:99):

```rust
    /// Cleaned titles agent sessions gave their terminal (ADR 0031); memory only, kept until
    /// the daemon stops, so exited sessions keep their name in `session.list`.
    // ponytail: one ≤ 60-char string per titled agent session; prune on exit if it grows.
    pub terminal_titles: Mutex<HashMap<Ulid, String>>,
```

In `Hub::new` (:296), add `terminal_titles: Mutex::default(),`. After `session_ports` (:305-308):

```rust
    /// The cleaned terminal title of agent session `id`, if its program set one.
    pub fn terminal_title(&self, id: Ulid) -> Option<String> {
        lock(&self.terminal_titles).get(&id).cloned()
    }
```

In `Actor::on_output`, after the DSR reply block (:1111-1113):

```rust
        if let Some(title) = self.emulator().take_title() {
            self.on_terminal_title(title.as_deref());
        }
```

After `scan_ports` (:1350):

```rust
    /// Keeps and announces an agent's new terminal title (ADR 0031). Shell titles (paths,
    /// running commands) are ignored; so are unusable titles (a path, only glyphs), which
    /// keep the current name. A reset or empty title clears it. Spinner frames clean to the
    /// same name: announced once.
    fn on_terminal_title(&mut self, raw: Option<&str>) {
        if self.session.kind != SessionKind::Agent {
            return;
        }
        let title = match raw.map(str::trim) {
            None | Some("") => None,
            Some(raw) => match crate::emulator::clean_title(raw) {
                Some(title) => Some(title),
                None => return,
            },
        };
        if title == self.session.terminal_title {
            return;
        }
        self.session.terminal_title.clone_from(&title);
        {
            let mut titles = lock(&self.hub.terminal_titles);
            match &title {
                Some(title) => titles.insert(self.session.id, title.clone()),
                None => titles.remove(&self.session.id),
            };
        }
        let _ = self
            .hub
            .events
            .send(Event::SessionTerminalTitle(SessionTerminalTitleEvent {
                session_id: self.session.id,
                title,
            }));
    }
```

`save()` ignores the new field, because `update_session` writes only named columns.

`server.rs` `SessionList` (:320-323):

```rust
            Request::SessionList(params) => {
                let sessions = self.store().list_sessions(params.project_id);
                let sessions = sessions.map(|mut sessions| {
                    for session in &mut sessions {
                        session.terminal_title = self.hub.terminal_title(session.id);
                    }
                    sessions
                });
                (store_outcome(sessions), After::Continue)
            }
```

In Task 13's `session_rename`, on the store path, add this line before the `session.updated` send:

```rust
        session.terminal_title = self.hub.terminal_title(session.id);
```

(On the live path, the actor's `self.session` already carries it.)

`notify.rs:134`:

```rust
    // The user's name, else the agent's own title, else the profile (ADR 0031).
    let name = session
        .title
        .or_else(|| hub.terminal_title(session_id))
        .unwrap_or(session.agent_profile);
```

Lock order: `terminal_titles` is never held while another lock is taken. Code may take it while it holds `store` (notify), but never the other way round.

Run: `cargo test -p mkrew-daemon --test daemon agent_terminal_titles -j 2` and `cargo test -p mkrew-daemon --lib -j 4`
Expected: PASS.

- [ ] **Step 6: Failing UI precedence test**

In the `history.rs` tests, after `finished_keeps_the_workspaces_exited_sessions_newest_first`:

```rust
    #[test]
    fn the_users_title_wins_then_the_terminal_title_then_the_agent() {
        let mut s = session(Ulid::new(), SessionState::Working, None);
        assert_eq!((title(&s), label(&s).as_str()), (None, "claude"));
        s.terminal_title = Some("Fix login".into());
        assert_eq!(label(&s), "Fix login");
        s.title = Some("Mine".into());
        assert_eq!(title(&s), Some("Mine"));
        assert_eq!(label(&s), "Mine");
        // Clearing the user's title shows the terminal title again.
        s.title = None;
        assert_eq!(title(&s), Some("Fix login"));
    }
```

Run: `cargo test -p mkrew-ui --lib history::tests -j 4`
Expected: FAIL to compile (`cannot find function title`).

- [ ] **Step 7: UI labels**

`history.rs` (:120-126):

```rust
/// A session's name (ADR 0031): the user's title, else the title its agent gave the terminal.
pub fn title(session: &Session) -> Option<&str> {
    session
        .title
        .as_deref()
        .or(session.terminal_title.as_deref())
}

/// The row's title: [`title`], else its agent profile.
pub fn label(session: &Session) -> String {
    title(session)
        .unwrap_or(&session.agent_profile)
        .to_owned()
}
```

`run.rs:242`: `return crate::history::title(session).map(str::to_owned);`

`sidebar.rs:290`: `title: crate::history::title(session).map(str::to_owned),`. In `Sidebar::apply`, after Task 13's `SessionUpdated` arm:

```rust
            Event::SessionTerminalTitle(event) => self.update_session(event.session_id, |session| {
                session.terminal_title.clone_from(&event.title);
            }),
```

`inbox.rs:128`: `title: crate::history::title(session).map(str::to_owned),`

main.rs `render_peek` (:7597): `let name = match history::title(session) {`. The arms stay the same; `title` is now a `&str`.

The rename field still starts from the user's title only (Task 13 `start_pane_rename`). So Enter on an untouched field never freezes the automatic name into a user title.

Run: `cargo test -p mkrew-ui --lib -j 4` and `cargo clippy --workspace --all-targets -j 4 -- -D warnings`
Expected: PASS; no warnings.

- [ ] **Step 8: ADR, SPEC, HUMAN_CHECKS**

In ADR 0031, append to `## Decision`:

```markdown
Automatic name: the daemon's emulator records OSC 0/2 titles. For agent sessions, the actor
drops leading glyphs (Claude's spinner), caps the title at 60 characters, dedupes it and
announces `session.terminal_title`. It ignores paths and glyph-only titles (the current name
stays); a reset or empty title clears the name. The title is kept in daemon memory only and
returned as `terminal_title` in `session.list`. Display order everywhere: user title > terminal
title > agent profile. A rename always wins, and clearing the name shows the terminal title
again. Shells are left out, because their titles are paths and commands. Rejected: the first
prompt (typed straight into the PTY, so Mkrew never sees it) and an `ai.generate` summary (one
CLI run per pane).
```

Append to `## Consequences`: "`Session` gains `terminal_title` (on the wire only, never stored), and there is a new event `session.terminal_title` with a second protocol bump."

SPEC §5: on the `title TEXT,` line of the session table (:240), add the comment `-- user's pane name (session.rename); the agent's terminal title is memory-only (ADR 0031)`.

SPEC §7: in the `session.list` row (:508), the Returns column becomes `Session[] (with \`terminal_title?\` from daemon memory)`. In Events, after Task 13's `session.updated` row:

```markdown
| `session.terminal_title` | an agent session's terminal title (OSC 0/2) changed: `session_id`, `title?` — leading glyphs stripped, paths ignored, ≤ 60 chars, once per change; memory only (ADR 0031) |
```

SPEC §11: append to Task 13's header sentence: `Without one, an agent pane shows the title its agent gave the terminal (e.g. Claude's conversation summary). Everywhere a session is named, the order is: user title > terminal title > agent name.`

HUMAN_CHECKS, in the same section:

```markdown
- [ ] **Auto pane name:**
  - Start a claude pane and ask it to fix something. Within a turn, the header, the sidebar row
    and the palette's "Go to session" show Claude's conversation title without the spinner glyph,
    and it does not flicker while Claude works.
  - F2, type `Mine`, Enter: `Mine` shows everywhere and stays while Claude keeps working.
  - F2, clear the text, Enter: Claude's title is back.
  - A pwsh pane never gets an automatic name.
```

- [ ] **Step 9: Verify and tick**

Run the full verification. Tick inbox line 32 `[x]`.
Suggested message: `feat: name agent panes from their terminal title (session.terminal_title, ADR 0031)`

---

### Task 15: Every input field takes Ctrl+A, Ctrl+Backspace and Ctrl+V

Inbox: line 36 `Check all input field in this app should suppport Ctrl A, Ctrl Backspace... same as standard for input field so that can easy to update the value....`. Spec: §13 (:869-871, table :895), §20B.1. No ADR: no spec decision changes (ADR 0028 already says editor keys are not in the keymap; field keys follow the same stance).

**Current state (investigated):**
- Two key mappers feed every field. `task_dialog::dialog_key` (task_dialog.rs:45-67) serves the dialogs, drafts, composer, commit box, run setup, broadcast, peek, base menu, quick open, palette and Docs filter (the last three through `quick_open::key`, quick_open.rs:30-46). `terminal::find::find_key` (find.rs:22-40) serves the terminal find bar (view.rs:405 `find_key_down`), the Files find bar (main.rs:1787 `file_find_key_down`) and the inline rename (main.rs:6487 `rename_key_down`).
- **Ctrl+Backspace deletes one char everywhere.** `dialog_key` L53 and `find_key` L28 match `"backspace"` whatever the modifiers. The terminal PTY already maps it to ^W (input.rs:140); that is not a field.
- **Ctrl+A works only in the `TextBox` fields** (text_box.rs:21-25 `TextBox { text, all_selected }`): task prompt, run-setup env, composer, card follow-up, commit box. It does nothing in every `String` field:
  - task title (task_dialog.rs:82, :164-177, `SelectAll` arm only handles the prompt)
  - run setup name / folder / command / env file (run_setup.rs:14-17, :99-116, `line()` :245)
  - base menu (base_dialog.rs:38; the mapper returns `None` for `SelectAll`, :29)
  - comment draft (comments.rs:142, :167)
  - broadcast (broadcast.rs:76, :154) and peek reply (peek.rs:54, :125)
  - quick open (quick_open.rs:63, :44), palette (palette.rs:445, :473-479), Docs filter (docs.rs:105)
  - Files find bar (`TextFind.query`, text_find.rs:38), terminal find bar (`FindBar.query`, find.rs:66), rename draft (main.rs:215)
- **Ctrl+V** works in every `dialog_key` field (the handlers read the clipboard on `DialogKey::Paste`) but **not** in the terminal find bar and the rename: `find_key` has no paste (the test pins `ctrl-a` => `None`, find.rs:169). The Files find bar special-cases it (main.rs:1797).
- Every field only appends and erases at the end (ime.rs:99-100 says so). IME / Unikey text arrives as synthetic `backspace` keystrokes with default modifiers, then one key press per char (ime.rs:47-67). So a plain Backspace must stay one char; Ctrl+Backspace never comes from the IME.
- Rendering: `text_field(id, text, placeholder, focused, selected, theme)` (main.rs:14251-14310) highlights every line with `theme.terminal_selection_bg` while `selected` and hides the caret. All the `String` call sites pass `false` today. The terminal find bar (view.rs:442-467) and the rename (main.rs:13436-13471) draw their own text.
- Word rule: Task 4's `editor.rs` has a private `fn class(c: char) -> u8` (blank 0, word char `alphanumeric` or `_` 1, other 2). Ctrl+Backspace there deletes one run of a class, like VS Code. This task puts that function in `text_box.rs` and the editor imports it, so there is one word rule.
- **After Tasks 7 and 13 land** (this task is written on top of them): `TextFind { query, replace: Option<String> }` and `file_replace_key_down` / `edit_file_replace` exist (Task 7); `renaming: Option<Renaming { target, draft: String }>` and `rename_key_down` use `self.renaming.as_mut()` (Task 13). The edits below name the functions, not the line numbers, for those two.

**Decisions (defaults in the open questions):**
- Option A, append-only. One reusable type `text_box::Input { text: String, all_selected: bool }` replaces every single-line `String` field. `TextBox` (multi-line) wraps one `Input`, so the select-all, backspace and word rules live in one place.
- Ctrl+Backspace (Cmd+Backspace on macOS) = `DeleteWord`. **Alt+Backspace stays a plain Backspace** (Windows has no Alt+Backspace field key; the terminal keeps ESC DEL).
- Word rule = the Files editor's: delete trailing blanks, then one run of the last char's class (word chars, or punctuation). It is **not** the terminal's ^W (readline deletes up to whitespace): a path like `src/main.rs` goes `rs`, `.`, `main`, `/` in four presses, the way every Windows and VS Code text field behaves. A trailing `\n` (multi-line boxes) is deleted alone, like the editor.
- Ctrl+A selects the whole text; the next typed or pasted text replaces it; Backspace or Ctrl+Backspace clears it. No arrows, Home/End, Shift selection, Ctrl+Z, Ctrl+C or Ctrl+X in fields: **out of scope** (a follow-up that moves every field onto `editor.rs`).

**Files:**
- Modify: `crates/mkrew-ui/src/text_box.rs` (`Input`, `delete_word`, `class`; `TextBox` wraps `Input`)
- Modify: `crates/mkrew-ui/src/task_dialog.rs:26-42, :82, :116-129, :164-193`, tests :331-345, :364-372
- Modify: `crates/mkrew-ui/src/terminal/find.rs:8-40, :64-133`, tests :161-171, :195-216
- Modify: `crates/mkrew-ui/src/quick_open.rs:14-46, :63, :78, :92, :114-133`, tests :250-276
- Modify: `crates/mkrew-ui/src/base_dialog.rs:11-31, :38, :56-62, :70-84`, test :169-178
- Modify: `crates/mkrew-ui/src/palette.rs:445, :456, :464-491`, new test at the end of `mod tests`
- Modify: `crates/mkrew-ui/src/docs.rs:105, :125`
- Modify: `crates/mkrew-ui/src/run_setup.rs:12-20, :84-139, :158-178, :220-252`, new test after :400
- Modify: `crates/mkrew-ui/src/comments.rs:142, :155, :162-185`, new test
- Modify: `crates/mkrew-ui/src/broadcast.rs:76, :109, :136-179`, new test
- Modify: `crates/mkrew-ui/src/peek.rs:54, :72, :111-140`, new test
- Modify: `crates/mkrew-ui/src/text_find.rs:36-40` (and Task 7's `replace`), `crates/mkrew-ui/src/editor.rs` (Task 4's private `class`)
- Modify: `crates/mkrew-ui/src/ime.rs` (one test)
- Modify: `crates/mkrew-ui/src/terminal/view.rs:405-420, :442-467`
- Modify: `crates/mkrew-ui/src/main.rs:78` (import), `:215/:582/:6440` (rename draft, as reshaped by Task 13), `:1471-1489` (`docs_filter_key_down`), `:1787-1836` (`file_find_key_down`, `edit_file_find`; Task 7's `file_replace_key_down`, `edit_file_replace`), `:2678` (`git_base_key_down`), `:3358` (`quick_open_key_down`), `:3514` (`palette_key_down`), `:5997` (`prompt_picker_key_down`), `rename_key_down` + `render_rename`, and the `text_field(..., selected, ...)` call sites listed in Step 5
- Modify: `docs/SPEC.md` §13 (after :871), `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces (`text_box.rs`):
  - `pub struct Input` (`Debug, Clone, Default, PartialEq, Eq`), with `Deref<Target = str>`, `From<&str>`, `From<String>`, `PartialEq<&str>`, `PartialEq<str>`
  - `Input::as_str(&self) -> &str`, `all_selected(&self) -> bool`, `select_all(&mut self)`, `clear(&mut self)`
  - `Input::insert(&mut self, text: &str) -> bool` (types over a selection; `false` and no change when `text` is empty)
  - `Input::backspace(&mut self) -> bool`, `delete_word(&mut self) -> bool`, `edit(&mut self, key: &DialogKey) -> bool` (Backspace / DeleteWord / SelectAll; `true` = text changed)
  - `pub fn delete_word(text: &mut String) -> bool`, `pub fn class(c: char) -> u8`
- Produces (mappers): `DialogKey::DeleteWord`; `FindKey::{DeleteWord, SelectAll, Paste}` and `FindKey::edit(&self, input: &mut Input) -> bool`; `quick_open::Key::{DeleteWord, SelectAll}`; `base_dialog::Key::{DeleteWord, SelectAll}`.
- Produces (state): `FindBar::selected() -> bool`; `Palette::query_selected() -> bool`; `run_setup::Card::all_selected(&self, field: Field) -> bool`.
- Type changes `String` → `Input`: `TaskDialog.title`, `run_setup::Card.{name, dir, command, env_file}`, `BaseDialog.text`, `comments::Draft.text`, `Broadcast.text`, `Peek.text`, `QuickOpen.query`, `Palette.query` (private), `Docs.filter`, `TextFind.query` and `TextFind.replace` (now `Option<Input>`), `FindBar.query` (private), `Renaming.draft`. Reads keep working through `Deref<Target = str>` (`.trim()`, `.is_empty()`, `&field` where a `&str` is wanted), and `assert_eq!(field, "text")` keeps working through `PartialEq<&str>`.
- Consumes: `DialogKey` (task_dialog.rs), `terminal::emulator::Emulator`, Task 4 `editor.rs` `fn class`, Task 7 `TextFind.replace`, Task 13 `Renaming`.

- [ ] **Step 1: Failing tests for `Input`, the word rule and `TextBox`**

In `text_box.rs` `mod tests`, after `select_all_makes_the_next_edit_replace_everything` (:143), append:

```rust
    #[test]
    fn delete_word_removes_the_last_word_like_a_text_field() {
        // (before, after)
        let cases = [
            ("hello world", "hello "),
            ("hello world   ", "hello "),
            ("hello", ""),
            ("snake_case x", "snake_case "),
            ("call(a, b)", "call(a, b"),
            ("foo(), ", "foo"),
            ("src/main.rs", "src/main."),
            ("việt nam", "việt "),
            ("việt ", ""),
            ("   ", ""),
            ("one\ntwo", "one\n"),
            ("one\n", "one"),
            ("one\n  ", "one\n"),
            ("", ""),
        ];
        for (before, after) in cases {
            let mut text = before.to_owned();
            assert_eq!(delete_word(&mut text), before != after, "{before:?}");
            assert_eq!(text, after, "{before:?}");
        }
    }

    #[test]
    fn select_all_then_typing_replaces_and_backspace_clears() {
        let mut input = Input::from("old name");
        input.select_all();
        assert!(input.all_selected());
        assert!(input.insert("new"));
        assert_eq!((input.as_str(), input.all_selected()), ("new", false));

        input.select_all();
        assert!(input.backspace());
        assert_eq!(input, "");
        input.select_all();
        assert!(!input.all_selected(), "nothing to select in an empty field");
        assert!(!input.backspace());
    }

    #[test]
    fn ctrl_backspace_deletes_a_word_or_the_selection() {
        let mut input = Input::from("fix the việt nam");
        assert!(input.delete_word());
        assert_eq!(input, "fix the việt ");
        input.select_all();
        assert!(input.delete_word());
        assert_eq!(input, "");
        assert!(!input.delete_word());
    }

    #[test]
    fn plain_backspace_erases_one_char_and_an_empty_insert_keeps_the_selection() {
        let mut input = Input::from("việt");
        assert!(input.backspace());
        assert_eq!(input, "việ");
        input.select_all();
        assert!(!input.insert(""), "typing nothing changes nothing");
        assert!(input.all_selected());
        assert_eq!(input, "việ");
    }

    #[test]
    fn a_text_box_deletes_words_line_by_line_and_clears_a_selection() {
        let mut text = TextBox::default();
        text.insert("fix the\nlogin bug");
        text.key(DialogKey::DeleteWord);
        assert_eq!(text.text(), "fix the\nlogin ");
        text.key(DialogKey::DeleteWord);
        assert_eq!(text.text(), "fix the\n");
        text.key(DialogKey::DeleteWord);
        assert_eq!(text.text(), "fix the", "the line break goes alone");
        text.key(DialogKey::SelectAll);
        text.key(DialogKey::DeleteWord);
        assert_eq!(text.text(), "");
    }
```

Run: `cargo test -p mkrew-ui --lib text_box -j 4`
Expected: FAIL to compile (`cannot find type Input`, `cannot find function delete_word`, `no variant DeleteWord`). `DialogKey::DeleteWord` is added in Step 3, so this step is run again after it; the first green run is the end of Step 3.

- [ ] **Step 2: Implement `Input`, `delete_word`, `class` and the `TextBox` wrapper**

`text_box.rs`: replace the module doc (:1-4) with:

```rust
//! The text inputs (SPEC §20B.1, §13): [`Input`] is one line, [`TextBox`] many (Enter types a
//! new line, Ctrl+Enter submits, Esc cancels). Text is typed and deleted at the end, like the
//! other fields; Ctrl+A selects all (the next edit replaces it) and Ctrl+Backspace deletes
//! the word before the end. Pure and tested headlessly; keys come from
//! [`crate::task_dialog::dialog_key`], `main.rs` renders the text.
```

Replace the imports (:6) and the `TextBox` struct and impl (:19-76) with:

```rust
use std::ops::Deref;

use crate::task_dialog::DialogKey;

/// What a key press in the box does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Keep it open (the text may have changed).
    Edited,
    /// Esc.
    Cancel,
    /// Ctrl+Enter (Cmd+Enter on macOS).
    Submit,
}

/// Blanks, word chars, punctuation: Ctrl+Backspace (and the editor's Ctrl+arrows) work on one
/// run of the same class.
// ponytail: a decomposed (NFD) Vietnamese letter has a combining mark, which counts as
// punctuation, so Ctrl+Backspace stops at it; Windows IMEs type precomposed letters. Upgrade
// path: unicode-segmentation word boundaries.
pub fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

/// Ctrl+Backspace: removes the trailing blanks, then the run of word chars (or of
/// punctuation) before them; a trailing line break goes alone. `true` when `text` changed.
pub fn delete_word(text: &mut String) -> bool {
    let before = text.len();
    while text
        .chars()
        .next_back()
        .is_some_and(|c| c != '\n' && c.is_whitespace())
    {
        text.pop();
    }
    match text.chars().next_back() {
        Some('\n') => {
            if text.len() == before {
                text.pop();
            }
        }
        Some(last) => {
            let kind = class(last);
            while text.chars().next_back().is_some_and(|c| class(c) == kind) {
                text.pop();
            }
        }
        None => {}
    }
    text.len() != before
}

/// One field's text. Typing and deleting happen at the end; Ctrl+A selects everything and the
/// next edit replaces it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    /// Ctrl+A was pressed: the next edit replaces the whole text.
    all_selected: bool,
}

impl Input {
    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// Whether the whole text is selected (drawn highlighted; the next edit replaces it).
    pub fn all_selected(&self) -> bool {
        self.all_selected
    }

    /// Ctrl+A: nothing to select in an empty field.
    pub fn select_all(&mut self) {
        self.all_selected = !self.text.is_empty();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.all_selected = false;
    }

    /// Types `text` (already shaped for the field: one line, no control chars), replacing a
    /// select-all. `false`, and nothing changes, when `text` is empty.
    pub fn insert(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if std::mem::take(&mut self.all_selected) {
            self.text.clear();
        }
        self.text.push_str(text);
        true
    }

    /// Backspace: the selection, else the last char. `true` when the text changed.
    pub fn backspace(&mut self) -> bool {
        if std::mem::take(&mut self.all_selected) {
            return !std::mem::take(&mut self.text).is_empty();
        }
        self.text.pop().is_some()
    }

    /// Ctrl+Backspace: the selection, else the last word. `true` when the text changed.
    pub fn delete_word(&mut self) -> bool {
        if std::mem::take(&mut self.all_selected) {
            return !std::mem::take(&mut self.text).is_empty();
        }
        delete_word(&mut self.text)
    }

    /// Applies Backspace, Ctrl+Backspace or Ctrl+A; every other key does nothing. `true` when
    /// the text changed (Ctrl+A only moves the highlight).
    pub fn edit(&mut self, key: &DialogKey) -> bool {
        match key {
            DialogKey::Backspace => self.backspace(),
            DialogKey::DeleteWord => self.delete_word(),
            DialogKey::SelectAll => {
                self.select_all();
                false
            }
            _ => false,
        }
    }
}

impl Deref for Input {
    type Target = str;

    fn deref(&self) -> &str {
        &self.text
    }
}

impl From<&str> for Input {
    fn from(text: &str) -> Self {
        text.to_owned().into()
    }
}

impl From<String> for Input {
    fn from(text: String) -> Self {
        Self {
            text,
            all_selected: false,
        }
    }
}

impl PartialEq<&str> for Input {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl PartialEq<str> for Input {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

/// The text of a multi-line box; line breaks are always `\n`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextBox {
    input: Input,
}

impl TextBox {
    pub fn text(&self) -> &str {
        self.input.as_str()
    }

    /// Whether nothing but blanks was typed.
    pub fn is_blank(&self) -> bool {
        self.input.trim().is_empty()
    }

    /// Whether the whole text is selected (drawn highlighted; the next edit replaces it).
    pub fn all_selected(&self) -> bool {
        self.input.all_selected()
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    /// Applies `key`; the caller inserts the clipboard text on [`DialogKey::Paste`].
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Cancel,
            DialogKey::Save => return Outcome::Submit,
            DialogKey::Enter => self.insert("\n"),
            DialogKey::Insert(text) => self.insert(&text),
            key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll) => {
                self.input.edit(&key);
            }
            DialogKey::SwitchField | DialogKey::Paste => {}
        }
        Outcome::Edited
    }

    /// Appends typed or pasted text (replacing a select-all), turning `\r\n` and `\r` line
    /// breaks into `\n`.
    pub fn insert(&mut self, text: &str) {
        self.input
            .insert(&text.replace("\r\n", "\n").replace('\r', "\n"));
    }
}
```

(The old `Outcome` enum at :8-17 is kept: it is part of the replaced block above, so delete the original, not both.)

Add the new variant now so the file compiles, in `task_dialog.rs` `enum DialogKey` after `Backspace` (:36):

```rust
    /// Ctrl+Backspace (Cmd+Backspace on macOS): delete the word before the end.
    DeleteWord,
```

and map it in `dialog_key`, replacing the `"backspace"` arm (:53):

```rust
        "backspace" if command && !m.alt => return Some(DialogKey::DeleteWord),
        "backspace" => return Some(DialogKey::Backspace),
```

(`command` is defined at :47. Alt+Backspace and the IME's synthetic Backspace have no Ctrl, so they stay `Backspace`.)

Run: `cargo test -p mkrew-ui --lib text_box -j 4`
Expected: PASS for `text_box` only if the rest of the crate compiles; it does not yet (the non-exhaustive `DialogKey` matches are fixed in Step 4). Expected now: FAIL to compile in `base_dialog.rs`, `broadcast.rs`, `comments.rs`, `peek.rs`, `quick_open.rs`, `run_setup.rs`, `task_dialog.rs` (`non-exhaustive patterns: DialogKey::DeleteWord`). That is the list Step 4 works through; run the same command at the end of Step 4.

- [ ] **Step 3: Failing tests for the mappers and the field models**

`task_dialog.rs` `keys_map_to_dialog_actions` (:331), after the `ctrl-a`/`cmd-a` asserts (:342-343):

```rust
        assert_eq!(key("ctrl-backspace"), Some(DialogKey::DeleteWord));
        assert_eq!(key("cmd-backspace"), Some(DialogKey::DeleteWord));
        assert_eq!(
            key("alt-backspace"),
            Some(DialogKey::Backspace),
            "Alt+Backspace is not a Windows field key"
        );
```

Same file, after `pasting_keeps_the_title_on_one_line` (:364-372):

```rust
    #[test]
    fn the_title_takes_select_all_and_ctrl_backspace_and_the_prompt_keeps_its_own() {
        let mut dialog = TaskDialog::edit(task());
        assert_eq!(dialog.title, "Fix login");
        dialog.key(DialogKey::SelectAll);
        dialog.insert("Add tests");
        assert_eq!(dialog.title, "Add tests", "typing replaces the selection");
        dialog.key(DialogKey::DeleteWord);
        assert_eq!(dialog.title, "Add ");
        dialog.key(DialogKey::SelectAll);
        dialog.key(DialogKey::Backspace);
        assert_eq!(dialog.title, "");
        assert_eq!(dialog.prompt.text(), "Fix the login bug", "untouched");

        dialog.key(DialogKey::SwitchField);
        dialog.key(DialogKey::DeleteWord);
        assert_eq!(dialog.prompt.text(), "Fix the login ");
    }
```

`terminal/find.rs`, in `keys_map_to_find_actions` replace the line `assert_eq!(key("ctrl-a"), None);` (:169) with:

```rust
        assert_eq!(key("ctrl-a"), Some(FindKey::SelectAll));
        assert_eq!(key("ctrl-v"), Some(FindKey::Paste));
        assert_eq!(key("ctrl-backspace"), Some(FindKey::DeleteWord));
        assert_eq!(key("alt-backspace"), Some(FindKey::Backspace));
        assert_eq!(key("ctrl-b"), None, "other Ctrl+letters still type nothing");
```

and after `invalid_and_missing_patterns_are_reported` (:216):

```rust
    #[test]
    fn select_all_and_ctrl_backspace_edit_the_query_and_search_again() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"foo bar");
        let mut bar = FindBar::default();
        bar.insert("foo bar", &mut emu);
        assert!(bar.key(FindKey::DeleteWord, &mut emu));
        assert_eq!(bar.query(), "foo ");
        assert_eq!(bar.status(), FindStatus::Found);

        bar.key(FindKey::SelectAll, &mut emu);
        assert!(bar.selected());
        bar.key(FindKey::Insert("bar".into()), &mut emu);
        assert_eq!((bar.query(), bar.selected()), ("bar", false));
        assert_eq!(bar.status(), FindStatus::Found);

        bar.key(FindKey::SelectAll, &mut emu);
        bar.key(FindKey::Backspace, &mut emu);
        assert_eq!(bar.query(), "");
        assert_eq!(bar.status(), FindStatus::Empty);
        // Paste is the caller's (it reads the clipboard), like in the dialogs.
        assert!(bar.key(FindKey::Paste, &mut emu));
        assert_eq!(bar.query(), "");
    }
```

`quick_open.rs` `keys` (:264), after the `ctrl-v` assert (:272):

```rust
        assert_eq!(parse("ctrl-backspace"), Some(Key::DeleteWord));
        assert_eq!(parse("ctrl-a"), Some(Key::SelectAll));
```

and after `editing_the_query` (:261):

```rust
    #[test]
    fn select_all_and_ctrl_backspace_edit_the_query() {
        let mut open = QuickOpen::new(Ulid::new(), None);
        open.insert("src/main.rs");
        assert!(open.key(Key::DeleteWord), "the query changed: search again");
        assert_eq!(open.query, "src/main.");
        assert!(!open.key(Key::SelectAll), "only the highlight changes");
        assert!(open.query.all_selected());
        assert!(open.insert("lib"));
        assert_eq!(open.query, "lib", "typing replaces the selection");
        open.key(Key::SelectAll);
        assert!(open.key(Key::Backspace));
        assert_eq!(open.query, "");
    }
```

`base_dialog.rs` `keys` (:170), after `ctrl-v` (:175):

```rust
        assert_eq!(parse("ctrl-backspace"), Some(Key::DeleteWord));
        assert_eq!(parse("ctrl-a"), Some(Key::SelectAll));
```

and after `typing_edits_one_line_and_enter_sends_the_trimmed_ref` (:157):

```rust
    #[test]
    fn the_ref_takes_select_all_and_ctrl_backspace() {
        let mut dialog = BaseDialog::new(Ulid::new(), Some(&status(None, true)));
        dialog.insert("origin/release");
        assert!(dialog.key(&Key::DeleteWord));
        assert_eq!(dialog.text, "origin/");
        dialog.key(&Key::SelectAll);
        assert!(dialog.insert("main"));
        assert_eq!(dialog.text, "main");
    }
```

`palette.rs`, at the end of `mod tests` (:1021):

```rust
    #[test]
    fn the_query_takes_select_all_and_ctrl_backspace() {
        let mut palette = Palette::new(Vec::new());
        palette.insert("git push");
        palette.key(Key::DeleteWord);
        assert_eq!(palette.query(), "git ");
        palette.key(Key::SelectAll);
        assert!(palette.query_selected());
        palette.key(Key::Insert("view".into()));
        assert_eq!((palette.query(), palette.query_selected()), ("view", false));
    }
```

`run_setup.rs`, after `backspace_edits_the_focused_field_only` (:400):

```rust
    #[test]
    fn single_line_fields_and_the_env_box_take_select_all_and_ctrl_backspace() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.cards[0].command = "npm run dev".into();
        setup.focus = Focus::Field(0, Field::Command);
        setup.key(DialogKey::DeleteWord);
        assert_eq!(setup.cards[0].command, "npm run ");
        setup.key(DialogKey::SelectAll);
        assert!(setup.cards[0].all_selected(Field::Command));
        assert!(!setup.cards[0].all_selected(Field::Name), "per field");
        setup.insert("make");
        assert_eq!(setup.cards[0].command, "make");
        assert_eq!(setup.cards[0].name, "BE", "other fields untouched");

        setup.focus = Focus::Field(0, Field::Env);
        setup.insert("A=1\nB=2");
        setup.key(DialogKey::DeleteWord);
        assert_eq!(setup.cards[0].env.text(), "A=1\nB=");
        setup.key(DialogKey::SelectAll);
        assert!(setup.cards[0].all_selected(Field::Env));
    }
```

`comments.rs`, after the test at :498 (end of that test, before the next `#[test]`):

```rust
    #[test]
    fn a_draft_takes_select_all_and_ctrl_backspace_unless_it_is_saving() {
        let mut draft = Draft::new(Target::New(Selection { start: 1, end: 1 }), String::new(), 1);
        draft.insert("why this");
        draft.key(DialogKey::DeleteWord);
        assert_eq!(draft.text, "why ");
        draft.key(DialogKey::SelectAll);
        draft.key(DialogKey::Insert("ok".into()));
        assert_eq!(draft.text, "ok");

        draft.busy = true;
        draft.key(DialogKey::SelectAll);
        draft.key(DialogKey::DeleteWord);
        assert_eq!(draft.text, "ok", "frozen while saving");
        assert!(!draft.text.all_selected());
    }
```

`broadcast.rs`, after `keys_move_the_cursor_toggle_rows_and_edit_the_text` (:335):

```rust
    #[test]
    fn the_text_takes_select_all_and_ctrl_backspace() {
        let mut dialog = two();
        dialog.insert("run the tests");
        assert_eq!(press(&mut dialog, "ctrl-backspace"), Outcome::Edited);
        assert_eq!(dialog.text, "run the ");
        assert_eq!(press(&mut dialog, "ctrl-a"), Outcome::Edited);
        assert!(dialog.text.all_selected());
        dialog.insert("go");
        assert_eq!(dialog.text, "go");
    }
```

`peek.rs`, after `keys_edit_the_reply_send_cycle_and_close` (:320):

```rust
    #[test]
    fn the_reply_takes_select_all_and_ctrl_backspace() {
        let mut peek = Peek::new(Ulid::new(), Place::Inbox, true);
        peek.insert("go on now");
        assert_eq!(peek.key(&keystroke("ctrl-backspace")), Outcome::Edited);
        assert_eq!(peek.text, "go on ");
        assert_eq!(peek.key(&keystroke("ctrl-a")), Outcome::Edited);
        peek.insert("y");
        assert_eq!(peek.text, "y");
    }
```

`text_find.rs` (:189): the typing test now types through `Input`, replace

```rust
        find.query.push('p');
```

with

```rust
        find.query.insert("p");
```

`ime.rs`, after `composition_steps_end_as_the_committed_text_in_a_text_box` (:206):

```rust
    /// Ctrl+A, then Telex: the first composing char replaces the selection, the re-typing
    /// Backspaces only erase composing chars (plain Backspace stays one char).
    #[test]
    fn composition_after_select_all_replaces_the_selection_once() {
        use crate::text_box::Input;
        let marked = Cell::new(0);
        let mut input = Input::from("tôi cũ");
        input.select_all();
        for keystroke in telex(&marked) {
            match dialog_key(&keystroke) {
                Some(DialogKey::Insert(text)) => {
                    input.insert(&text);
                }
                Some(key) => {
                    input.edit(&key);
                }
                None => {}
            }
        }
        assert_eq!(input, "việt");
        assert_eq!(marked.get(), 0);
    }
```

Run: `cargo test -p mkrew-ui --lib -j 4`
Expected: FAIL to compile (`no variant FindKey::SelectAll`, `no variant quick_open::Key::DeleteWord`, `no method all_selected on String`, `no method query_selected`, `no method selected on FindBar`).

- [ ] **Step 4: Implement the mappers and the field models (lib side)**

Every file gets `use crate::text_box::Input;` (merge it into an existing `use crate::text_box::…` line where there is one: task_dialog.rs:8, run_setup.rs:10).

`task_dialog.rs`:
- `TaskDialog.title` (:82): `pub title: Input,`
- `open` (:116-129): in the `Self { … }` literal `title: title.into(),`
- `key` (:164-178): replace the `Backspace` arm, the `Insert` arm and the `SelectAll` arm and the `Paste` arm with:

```rust
            DialogKey::Insert(text) => self.insert(&text),
            key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll) => {
                match self.field {
                    Field::Title => {
                        self.title.edit(&key);
                    }
                    Field::Prompt => {
                        self.prompt.key(key);
                    }
                }
            }
            DialogKey::Paste => {}
```

- `insert` (:184-193): replace the `Field::Title` arm with:

```rust
            Field::Title => {
                let line: String = text
                    .replace("\r\n", "\n")
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                self.title.insert(&line);
            }
```

(`save_request` :240-263 reads `self.title.trim()` and `title != task.title` with `title: &str`: unchanged. main.rs:5924 `dialog.title.trim()`: unchanged.)

`terminal/find.rs`: imports `use crate::text_box::Input;`. Enum (:9-19) and mapper:

```rust
pub enum FindKey {
    /// Esc: close the bar, back to the terminal.
    Close,
    /// Enter: next match towards older output.
    Older,
    /// Shift+Enter: previous match (towards newer output).
    Newer,
    Backspace,
    /// Ctrl+Backspace (Cmd+Backspace on macOS): delete the word before the end.
    DeleteWord,
    /// Ctrl+A (Cmd+A): the next edit replaces the whole text.
    SelectAll,
    /// Ctrl+V (Cmd+V): the caller inserts the clipboard text.
    Paste,
    Insert(String),
}

impl FindKey {
    /// Applies an editing key to `input` (the terminal bar, the Files bars, the rename);
    /// every other key does nothing. `true` when the text changed.
    pub fn edit(&self, input: &mut Input) -> bool {
        match self {
            FindKey::Backspace => input.backspace(),
            FindKey::DeleteWord => input.delete_word(),
            FindKey::SelectAll => {
                input.select_all();
                false
            }
            _ => false,
        }
    }
}

/// The find-bar meaning of `keystroke`; `None` = ignored (it never reaches the PTY).
pub fn find_key(keystroke: &Keystroke) -> Option<FindKey> {
    let m = keystroke.modifiers;
    let command = m.platform || m.control;
    match keystroke.key.as_str() {
        "escape" => return Some(FindKey::Close),
        "enter" if m.shift => return Some(FindKey::Newer),
        "enter" => return Some(FindKey::Older),
        "backspace" if command && !m.alt => return Some(FindKey::DeleteWord),
        "backspace" => return Some(FindKey::Backspace),
        "v" if command && !m.alt && !m.shift => return Some(FindKey::Paste),
        "a" if command && !m.alt && !m.shift => return Some(FindKey::SelectAll),
        _ => {}
    }
    // (the AltGr / plain Ctrl+key block and the Insert mapping stay as they are)
```

`FindBar` (:64-133): `query: Input,` (L66); `Default` `query: Input::default(),`; add after `query()` (:80-82):

```rust
    /// Whether Ctrl+A selected the whole query (drawn highlighted).
    pub fn selected(&self) -> bool {
        self.query.all_selected()
    }
```

`FindBar::key` (:90-106), replace the `Backspace` and `Insert` arms:

```rust
            FindKey::Insert(text) => self.insert(&text, emulator),
            // The caller reads the clipboard and calls `insert`.
            FindKey::Paste => {}
            key => {
                if key.edit(&mut self.query) {
                    self.search(emulator);
                }
            }
```

`FindBar::insert` (:109-115):

```rust
    pub fn insert(&mut self, text: &str, emulator: &mut Emulator) {
        let line = text.lines().next().unwrap_or_default();
        if self.query.insert(line) {
            self.search(emulator);
        }
    }
```

(`reapply`, `search` read `&self.query` as `&str`: unchanged. The `matches!`-free `FindStatus` code is unchanged.)

`quick_open.rs`: enum (:16-27), add after `Backspace`:

```rust
    /// Ctrl+Backspace: delete the word before the end.
    DeleteWord,
    /// Ctrl+A: the next edit replaces the whole query.
    SelectAll,
```

`key()` (:38-45):

```rust
    match dialog_key(keystroke)? {
        DialogKey::Cancel => Some(Key::Close),
        DialogKey::Enter | DialogKey::Save => Some(Key::Open),
        DialogKey::Backspace => Some(Key::Backspace),
        DialogKey::DeleteWord => Some(Key::DeleteWord),
        DialogKey::SelectAll => Some(Key::SelectAll),
        DialogKey::Paste => Some(Key::Paste),
        DialogKey::Insert(text) => Some(Key::Insert(text)),
        DialogKey::SwitchField => None,
    }
```

`QuickOpen.query: Input` (:63); `new` `query: Input::default(),` (:78); `search` (:92) `query: self.query.as_str().to_owned(),`; `key` (:114-124):

```rust
            Key::Backspace => self.query.backspace(),
            Key::DeleteWord => self.query.delete_word(),
            Key::SelectAll => {
                self.query.select_all();
                false
            }
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => false,
```

`insert` (:128-133):

```rust
    pub fn insert(&mut self, text: &str) -> bool {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        self.query.insert(&line)
    }
```

`base_dialog.rs`: enum `Key` (:13-20) gets `DeleteWord,` and `SelectAll,` after `Backspace`; `key()` (:23-30):

```rust
        DialogKey::Backspace => Some(Key::Backspace),
        DialogKey::DeleteWord => Some(Key::DeleteWord),
        DialogKey::SelectAll => Some(Key::SelectAll),
        DialogKey::Paste => Some(Key::Paste),
        DialogKey::Insert(text) => Some(Key::Insert(text)),
        DialogKey::SwitchField => None,
```

`BaseDialog.text: Input` (:38); `new` (:58-62):

```rust
            text: if auto {
                Input::default()
            } else {
                base.unwrap_or_default().into()
            },
```

`key` (:70-76) and `insert` (:79-84):

```rust
    pub fn key(&mut self, key: &Key) -> bool {
        match key {
            Key::Backspace => self.text.backspace(),
            Key::DeleteWord => self.text.delete_word(),
            Key::SelectAll => {
                self.text.select_all();
                false
            }
            Key::Insert(text) => self.insert(text),
            Key::Close | Key::Save | Key::Paste => false,
        }
    }

    /// Appends typed or pasted text (a ref is one line). `true` when it changed.
    pub fn insert(&mut self, text: &str) -> bool {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        self.text.insert(&line)
    }
```

`palette.rs`: `query: Input` (:445), `query: Input::default(),` (:456), after `query()` (:464-466):

```rust
    /// Whether Ctrl+A selected the whole query (drawn highlighted).
    pub fn query_selected(&self) -> bool {
        self.query.all_selected()
    }
```

`query()` body becomes `self.query.as_str()`. `key` (:470-481):

```rust
            Key::Move(by) => self.step(by),
            Key::Backspace => {
                if self.query.backspace() {
                    self.filter();
                }
            }
            Key::DeleteWord => {
                if self.query.delete_word() {
                    self.filter();
                }
            }
            Key::SelectAll => self.query.select_all(),
            Key::Insert(text) => self.insert(&text),
            Key::Close | Key::Open | Key::Paste => {}
```

`insert` (:484-491):

```rust
    pub fn insert(&mut self, text: &str) {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        if self.query.insert(&line) {
            self.filter();
        }
    }
```

`docs.rs`: `pub filter: Input,` (:105); `filter: Input::default(),` (:125); `palette::fuzzy(&self.filter, …)` (:263) unchanged (deref).

`run_setup.rs`: `Card` (:12-20) fields `name`, `dir`, `command`, `env_file` become `Input`; add after the `Field` enum (:39):

```rust
impl Card {
    /// Whether Ctrl+A selected the whole text of `field` (drawn highlighted).
    pub fn all_selected(&self, field: Field) -> bool {
        match field {
            Field::Name => self.name.all_selected(),
            Field::Dir => self.dir.all_selected(),
            Field::Command => self.command.all_selected(),
            Field::EnvFile => self.env_file.all_selected(),
            Field::Env => self.env.all_selected(),
        }
    }
}
```

`key` (:84-124): replace the `Backspace` arm (:99-110) and the `SelectAll` arm (:111-116) with one arm:

```rust
            key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll) => {
                match self.focus {
                    Focus::Field(i, Field::Env) => {
                        self.cards[i].env.key(key);
                    }
                    Focus::Field(i, field) => {
                        line(&mut self.cards[i], field).edit(&key);
                    }
                    Focus::Autostart => {}
                }
                Outcome::Edited
            }
```

`insert` (:135-137), the single-line arm:

```rust
            Focus::Field(i, field) => {
                let text: String = text.chars().filter(|c| !c.is_control()).collect();
                line(&mut self.cards[i], field).insert(&text);
            }
```

`begin` (:171-177): `name: card.name.as_str().to_owned(), dir: card.dir.as_str().to_owned(), command: card.command.as_str().to_owned(), env_file: Some(card.env_file.as_str().to_owned()),`. `empty_card` (:222): `dir: ".".into(),`. `card()` (:235-241): `name: command.name.clone().into(), dir: command.dir.clone().into(), command: command.command.clone().into(), env_file: command.env_file.clone().unwrap_or_default().into(),`. `line` (:245): `fn line(card: &mut Card, field: Field) -> &mut Input {`.

`comments.rs`: `Draft.text: Input` (:142); `Draft::new` `text: Input::default(),` (:155); `key` (:162-176) and `insert` (:179-185):

```rust
    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => return Outcome::Cancel,
            DialogKey::Enter | DialogKey::Save => return Outcome::Submit,
            // The caller inserts the clipboard text.
            DialogKey::SwitchField | DialogKey::Paste => {}
            key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll) => {
                if !self.busy {
                    self.text.edit(&key);
                }
            }
            DialogKey::Insert(text) => self.insert(&text),
        }
        Outcome::Edited
    }

    /// Appends typed or pasted text (pasted line breaks are kept); ignored while saving.
    pub fn insert(&mut self, text: &str) {
        if self.busy {
            return;
        }
        self.text.insert(&text.replace("\r\n", "\n"));
        self.error = None;
    }
```

`broadcast.rs`: `pub text: Input,` (:76), `text: Input::default(),` (:109); in `key` (:136-155) replace the `Backspace` arm (:144-149) and the last arm:

```rust
            Some(key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll)) => {
                if !self.busy() {
                    self.text.edit(&key);
                }
                Outcome::Edited
            }
            Some(DialogKey::Insert(text)) => {
                self.insert(&text);
                Outcome::Edited
            }
            None => Outcome::Edited,
```

(the `Insert` arm already exists at :150-153; keep one copy.) `insert` (:171-179):

```rust
        let line: String = text
            .chars()
            .map(|c| if c == '\r' || c == '\n' { ' ' } else { c })
            .collect();
        self.text.insert(&line);
```

`peek.rs`: `pub text: Input,` (:54), `text: Input::default(),` (:72); `key` (:111-126) arms:

```rust
            Some(key @ (DialogKey::Backspace | DialogKey::DeleteWord | DialogKey::SelectAll)) => {
                if !self.busy {
                    self.text.edit(&key);
                }
                Outcome::Edited
            }
            Some(DialogKey::Insert(text)) => {
                self.insert(&text);
                Outcome::Edited
            }
            Some(DialogKey::SwitchField) | None => Outcome::Edited,
```

and `insert` (:135-138) with the same `line` collect as broadcast. (`send_params` reads `self.text.trim()`; `self.text.clear()` at :165; `self.text.is_empty()` at :174: all unchanged.)

`text_find.rs` (:36-40) and Task 7's `replace`: `pub query: Input,` and `pub replace: Option<Input>,` (import `use crate::text_box::Input;`). The four Task 7 spots the compiler lists, with their fixes:
- `files_view.rs` `open_find`: `find.query = seed;` becomes `find.query = seed.into();`
- `files_view.rs` `open_find`: `find.replace.get_or_insert_with(String::new);` becomes `find.replace.get_or_insert_with(Input::default);`
- `replace_one` and `replace_all`: `let (Some(with), Some(found)) = (find.replace.clone(), find.current().cloned())` and `let Some(with) = find.replace.clone() else` each take `find.replace.as_ref().map(|with| with.as_str().to_owned())` instead of `find.replace.clone()` (the edits and `with.len()` want a `String`)
- the `view.find` literal in `files_view.rs` tests (`find.query = "todo".into()`) and Task 7's tests keep compiling (`From<&str>`, `Option<Input>::as_deref`)

`editor.rs` (Task 4): delete its private `fn class(c: char) -> u8` (with its doc line) and add `use crate::text_box::class;` to the imports. Task 4's tests (`word_motions_…`, `ctrl_backspace_…`) keep passing: same function.

Run: `cargo test -p mkrew-ui --lib -j 4`
Expected: PASS (`text_box`, `task_dialog`, `find`, `quick_open`, `base_dialog`, `palette`, `run_setup`, `comments`, `broadcast`, `peek`, `text_find`, `ime`, `editor`). `composition_steps_end_as_the_committed_text_in_a_text_box` and `composition_steps_reach_the_terminal_as_the_committed_text` pass unchanged: IME Backspaces carry no Ctrl.

- [ ] **Step 5: Wire `main.rs` and the terminal view**

`main.rs:78`: `use mkrew_ui::text_box::{self, Input};`

`docs_filter_key_down`, replace the `Backspace`, `Paste`, `Insert` arms (:1481-1488):

```rust
            quick_open::Key::Backspace => {
                docs.filter.backspace();
            }
            quick_open::Key::DeleteWord => {
                docs.filter.delete_word();
            }
            quick_open::Key::SelectAll => docs.filter.select_all(),
            quick_open::Key::Paste => {
                docs.filter
                    .insert(&paste.unwrap_or_default().replace(['\r', '\n'], " "));
            }
            quick_open::Key::Insert(text) => {
                docs.filter.insert(&text);
            }
```

`file_find_key_down`: delete the paste block (`if task_dialog::dialog_key(&event.keystroke) == Some(DialogKey::Paste) { … return; }`, :1797-1803) and replace the `match find_key(&event.keystroke)` (:1804-1820) with:

```rust
        match find_key(&event.keystroke) {
            Some(FindKey::Close) => self.close_file_find(project_id, window, cx),
            // The terminal's names: Enter = "older" = the next match here.
            Some(FindKey::Older) => self.step_file_find(project_id, true, cx),
            Some(FindKey::Newer) => self.step_file_find(project_id, false, cx),
            Some(FindKey::Paste) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let text = text.replace(['\r', '\n'], " ");
                    self.edit_file_find(
                        project_id,
                        |query| {
                            query.insert(&text);
                        },
                        cx,
                    );
                }
            }
            Some(FindKey::Insert(text)) => self.edit_file_find(
                project_id,
                |query| {
                    query.insert(&text);
                },
                cx,
            ),
            // Backspace, Ctrl+Backspace, Ctrl+A.
            Some(key) => self.edit_file_find(
                project_id,
                |query| {
                    key.edit(query);
                },
                cx,
            ),
            None => {}
        }
```

`edit_file_find` (:1823): `edit: impl FnOnce(&mut Input),`. Task 7's `edit_file_replace` likewise `edit: impl FnOnce(&mut Input)`, and in `file_replace_key_down` the clipboard block and the `match find_key(keystroke)` are replaced by the same shape (`edit_file_replace(project_id, |with| { … }, cx)`, no `Older` / `Newer` arms: `_ => {}` stays for them):

```rust
        match find_key(keystroke) {
            Some(FindKey::Close) => self.close_file_find(project_id, window, cx),
            Some(FindKey::Paste) => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    let text = text.replace(['\r', '\n'], " ");
                    self.edit_file_replace(
                        project_id,
                        |with| {
                            with.insert(&text);
                        },
                        cx,
                    );
                }
            }
            Some(FindKey::Insert(text)) => self.edit_file_replace(
                project_id,
                |with| {
                    with.insert(&text);
                },
                cx,
            ),
            Some(key @ (FindKey::Backspace | FindKey::DeleteWord | FindKey::SelectAll)) => {
                self.edit_file_replace(
                    project_id,
                    |with| {
                        key.edit(with);
                    },
                    cx,
                );
            }
            _ => {}
        }
```

`git_base_key_down` (:2678): `base_dialog::Key::Backspace | base_dialog::Key::DeleteWord | base_dialog::Key::SelectAll | base_dialog::Key::Insert(_) => {`.

`quick_open_key_down` (:3358): `quick_open::Key::Backspace | quick_open::Key::DeleteWord | quick_open::Key::SelectAll | quick_open::Key::Insert(_) => {`.

`palette_key_down` (:3514): `quick_open::Key::Move(_) | quick_open::Key::Backspace | quick_open::Key::DeleteWord | quick_open::Key::SelectAll | quick_open::Key::Insert(_) => {`.

`prompt_picker_key_down` (:5997) has no text input; the new keys are ignored:

```rust
            quick_open::Key::Backspace
            | quick_open::Key::DeleteWord
            | quick_open::Key::SelectAll
            | quick_open::Key::Paste
            | quick_open::Key::Insert(_) => {}
```

Rename (after Task 13): `Renaming.draft: Input`; `start_rename` `draft: tab.title.clone().into(),`; `start_pane_rename` `draft: session.title.clone().unwrap_or_default().into(),`; `finish_rename` passes `&draft` to `rename_tab` / `rename_session` (`&str` parameters: deref coercion). `rename_key_down`, replace the `Backspace` and `Insert` arms:

```rust
            FindKey::Paste => {
                if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                    renaming.draft.insert(&text.replace(['\r', '\n'], " "));
                    cx.notify();
                }
            }
            FindKey::Insert(text) => {
                renaming.draft.insert(&text);
                cx.notify();
            }
            // Backspace, Ctrl+Backspace, Ctrl+A.
            key => {
                key.edit(&mut renaming.draft);
                cx.notify();
            }
```

`render_rename` (:13436-13471): the parameter is the `Input` (callers pass `&renaming.draft`), and the selection is drawn like `text_field` does:

```rust
    fn render_rename(
        &self,
        draft: &Input,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let selected = draft.all_selected();
        div()
            // … (every style call up to `.child(self.typing(...))` stays)
            .child(
                div()
                    .when(selected, |el| el.bg(rgb(theme.terminal_selection_bg)))
                    .child(draft.as_str().to_owned()),
            )
            .when(!selected, |el| {
                el.child(
                    div()
                        .flex_none()
                        .w(px(theme::BORDER_WIDTH))
                        .h(font_size::BODY)
                        .bg(rgb(theme.text)),
                )
            })
            .into_any_element()
    }
```

The old call in `render_tab_bar` (:13300-13301) `self.render_rename(draft, theme, cx)` now gets the `Input` (`renaming` tuple/struct field, whichever Task 13 left).

`text_field` call sites: replace the literal `false` of the `selected` argument (the 5th argument) with the field's flag:

| Field | main.rs today | selected argument |
| --- | --- | --- |
| peek reply | :7648-7652 | `peek.text.all_selected()` |
| docs filter | :8526-8531 | `docs.filter.all_selected()` |
| Files find | :9032-9037 | `find.query.all_selected()` |
| Files replace (Task 7) | the `file-replace-field` call | `with.all_selected()` |
| comment draft | :10583-10588 | `draft.text.all_selected()` |
| base ref | :12041-12045 | `dialog.text.all_selected()` |
| broadcast | :12262-12266 | `dialog.text.all_selected()` |
| quick open | :12626-12630 | `open.query.all_selected()` |
| palette | :12775-12780 | `palette.query_selected()` |

Task dialog `field` closure (:10702-10711), replace the `selected` line:

```rust
            let selected = match which {
                Field::Title => dialog.title.all_selected(),
                Field::Prompt => dialog.prompt.all_selected(),
            };
```

Run setup `field` closure (:8146-8153), replace the fifth argument `which == Field::Env && setup.cards[i].env.all_selected(),` with `setup.cards[i].all_selected(which),`.

`terminal/view.rs`: add `FindKey` to the existing `find` import. Replace `find_key_down` (:405-420):

```rust
    fn find_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let Some(key) = find_key(&event.keystroke) else {
            return;
        };
        if key == FindKey::Paste {
            let text = cx.read_from_clipboard().and_then(|item| item.text());
            if let (Some(find), Some(text)) = (self.find.as_mut(), text) {
                find.insert(&text, &mut self.emulator);
                cx.notify();
            }
            return;
        }
        let Some(find) = self.find.as_mut() else {
            return;
        };
        if find.key(key, &mut self.emulator) {
            cx.notify();
        } else {
            self.close_find(window, cx);
        }
    }
```

`render_find_bar` (:456-467), replace with (the highlight uses the same token as `text_field`):

```rust
        // While typing, the caret sits after the query; the placeholder comes after the caret.
        // Ctrl+A highlights the query instead of showing the caret.
        let selected = find.selected();
        let query = div().flex().items_center().child(
            div()
                .text_color(rgb(color))
                .when(selected, |el| el.bg(rgb(theme.terminal_selection_bg)))
                .child(text),
        );
        let query = if !focused {
            query
        } else if find.query().is_empty() {
            div().flex().items_center().child(caret).child(query)
        } else if selected {
            query
        } else {
            query.child(caret)
        };
```

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings` (this compiles `main.rs`)
Expected: clean. A leftover `String`-typed use shows as `expected String, found Input` or `no method push_str`; the only expected uses are the ones listed above (`grep -n "\.query\b\|\.filter\b\|\.draft\b" crates/mkrew-ui/src/main.rs`).
Run: `cargo test -p mkrew-ui --lib -j 4` and `cargo test -p mkrew-ui --tests -j 4`
Expected: PASS.

- [ ] **Step 6: SPEC + HUMAN_CHECKS**

SPEC §13, after the sentence about plain Ctrl+V (:870-871) add:

```markdown
Every text field of the app (task title and prompt, composer, commit box, run setup, comments,
broadcast, peek reply, quick open, command palette, Docs filter, the find bars and the inline
rename) edits at the end of its text and takes Ctrl+A (select all: the next key replaces it),
Ctrl+Backspace (delete the word before the end: blanks, then a run of letters / digits / `_` or of
punctuation, like the Files editor) and Ctrl+V. Arrows, Home / End, Shift selection and Ctrl+Z / C /
X work only in the Files editor (§12A).
```

HUMAN_CHECKS, in the 2026-10-09 section (`### To verify — 2026-10-09 inbox: …`, created by Task 1):

```markdown
- [ ] **Field keys (Ctrl+A, Ctrl+Backspace, Ctrl+V):** in each of these, type `hello world`,
  press Ctrl+Backspace → `hello ` (again → empty); type `one two`, press Ctrl+A (the text is
  highlighted, no caret), type `x` → just `x`; Ctrl+A then Backspace → empty; Ctrl+V pastes the
  clipboard text (one line in single-line fields). Fields: task dialog title and prompt, card
  composer (Ctrl+Shift+Enter), commit box, Run setup (name and env), comment box, broadcast,
  peek reply, quick open (Ctrl+Shift+O), command palette (the list re-filters), Docs filter, base
  branch menu, terminal find (Ctrl+Shift+F; Ctrl+V pastes, matches follow), Files find (Ctrl+F),
  and the tab / pane rename (F2 / Ctrl+Shift+R). In the Vietnamese IME (Unikey, Telex): type
  `việt nam`, Ctrl+Backspace → `việt `; Ctrl+A, then type `tieengs` → only the new word; plain
  Backspace still removes one letter. Ctrl+Backspace in a terminal pane still deletes a word of
  the shell line (^W).
```

- [ ] **Step 7: Verify and tick**

Run the full verification. Tick inbox line 36 `[x]` (today the line starts `[ ]:`; the plan's mapping turns it into `[x]`).
Suggested message: `feat(ui): Ctrl+A, Ctrl+Backspace and Ctrl+V in every text field (text_box::Input)`

---

### Task 16: Run ■ Stop removes the terminal, ↻ Restart leaves one (a pane leaves its tab when its session exits)

Inbox: line 37 `Running command, when a server user click stop: should remove this terminal, user click restart: don't create new one stop exisitng terminal then start again`. Spec: §7 (SPEC.md:500-501), §10 Run button (:713-715), §8 (:609-612). ADR 0018.

**Current state (investigated):**
- `Daemon::run_stop` (`crates/mkrew-daemon/src/server/run.rs:62-67`) is fire-and-forget: `stop_runs` (:170-190) sends `Command::Kill { force: false }` to the live `run:<name>` shells and returns `ok`. It never touches the `run` tab. The session exits, but its `{"session": id}` leaf stays in the tab layout, so the pane shows `pane_placeholder` "✕ Session exited (code N)." (`main.rs:13910-13914`; `sync_panes` :3684 does not attach exited sessions). This is the visible bug.
- A run command that ends by itself (`<cmd>; exit`, `typed_line` `crates/mkrew-core/src/run.rs:194-202`) has the same dead pane. Run sessions are found by `run::command_name(&Session)` (core run.rs:210-215: `Shell` kind, title `run:<name>`).
- `run_restart` (:32-59) = validate → `stop_runs` → `wait_exited(.., RESTART_WAIT)` (:195-217, watches the store, 10 s) → `start_runs` → `show_run_tab` (:232-260), which writes `run::layout(run::ordered_live(..))` (live sessions only, config order) in one store lock and emits `TabUpdated`. So the old pane is already replaced **in place** by the new one. Test `run_commands_start_in_their_folders_…` (`crates/mkrew-daemon/tests/daemon.rs:5245-5259`) pins `pair(new_be, fe_id)`. The "second terminal" the user sees is the old **session row** in the sidebar: the old run session stays there as `✕ exited`. Task 17 removes that row. Restart keeps a NEW session id on purpose: an id is a row with its log, turns and History entry, and `Actor::finish` (`session.rs:1359`), the exited invariant and `session.history` all assume one process per id. Reusing an id would mix two runs in one log.
- Daemon: `Actor::finish` (`crates/mkrew-daemon/src/session.rs:1359-1400`) is the one place every session exit goes through. It sets `Exited`, sends `Event::SessionExited` and never edits tabs. Daemon stop = `Hub::shutdown` (:887-902) sets `Hub.stopping` first; `finish` reads it (:1384) to mark the session `interrupted`. Startup `mark_stale_sessions_exited` (`lib.rs:58-63`) only writes the store, before the endpoint opens: no events, no tab edits.
- `resume_sessions` (`server/resume.rs:22-75`) resumes an interrupted agent pane only if `in_a_tab` (:179-190) still finds its leaf. **Trap:** a hook in `finish` that removes leaves must skip `stopping`, or a daemon restart loses every pane that should resume (ADR 0017, `restart_resumes_free_agent_panes_in_tabs_once` daemon.rs:4091-4211 pins this).
- Tab layouts are edited by the daemon as opaque JSON already (`replace_session`, resume.rs:203-226; one store lock so a concurrent `tab.set_layout` is not overwritten). The UI mirrors `tab.updated` (`tabs.rs:162-194`); `Tab::fix_focus` (:99-106) moves focus and ends a zoom when the focused pane leaves. No protocol change.

**Decision (hook point and races):**
- One hook, in `Actor::finish`: after the state is written as `Exited` and when the daemon is not stopping, `Hub::leave_tabs(project_id, session_id)` removes the leaf from every tab of the project and sends `tab.updated`. Task 16 calls it for run sessions only; Task 17 drops that condition.
- It runs **after** `set_state(Exited)` on purpose. `show_run_tab` reads sessions and writes the layout under one store lock and lists live sessions only, so it either ran before the exit was stored (then the hook removes what it wrote) or after (then it never listed the session). The other order (hook first) could leave a stale leaf.
- `run_stop` needs no change: Stop → kill → exit → the hook removes the pane. The reply stays fire-and-forget (nothing for the UI to wait on; the pane just disappears). The scout's draft had `run_stop` await `wait_exited` + `show_run_tab`; the hook makes that redundant and also covers self-ending commands, so it is dropped.
- Restart: `wait_exited` returns when the store says `exited`; the hook and `start_runs` → `show_run_tab` then race. Both orders end with `pair(new_be, fe)`: the hook only removes the **old** id, and `show_run_tab` rewrites from live sessions only. In between, the tab may briefly show only the other command. Accepted (a few ms, no input is lost). `show_run_tab` still rewrites the whole tab (a non-run pane the user split into it is dropped, as today; unchanged).
- Last run pane gone → layout `null`; the `run` tab stays (empty), like any empty tab. Run ▶ fills it again.

**Files:**
- Create: `crates/mkrew-daemon/src/tabs.rs` (`without_session`, `Hub::leave_tabs`, unit test)
- Modify: `crates/mkrew-daemon/src/lib.rs:6-29` (add `mod tabs;` after `mod snapshot;`), `crates/mkrew-daemon/src/session.rs:1383-1399` (`Actor::finish`)
- Modify tests: `crates/mkrew-daemon/tests/daemon.rs` (`run_commands_start_in_their_folders_…` :5174-5273, `run_commands_auto_start_after_a_daemon_restart_and_end_with_their_command` :5384-5417, new helper `wait_layout` after `list_tabs` :2749-2756)
- Modify: `docs/SPEC.md` §7 (:500-501), §8 (:609-612), §10 (:713-715); `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Produces: `fn without_session(layout: &serde_json::Value, session: Ulid) -> Option<serde_json::Value>` (private to `tabs.rs`; `None` = not in the layout; `Some(Value::Null)` = it was the last pane; the sibling of a removed pane takes the split's place, ratios of the rest kept; same result as the UI's `Node::close`).
- Produces: `impl Hub { pub fn leave_tabs(&self, project_id: Ulid, session_id: Ulid) }`.
- Consumes: `Hub::store()` (`session.rs:300`), `Hub.events` (`broadcast::Sender<Event>`), `Store::list_tabs`/`set_tab_layout` (store lib.rs:341, :413), `mkrew_core::run::command_name`.

- [ ] **Step 1: Failing tests**

Create `crates/mkrew-daemon/src/tabs.rs` with the test first (the functions arrive in Step 3):

```rust
#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use ulid::Ulid;

    use super::without_session;

    fn leaf(id: Ulid) -> Value {
        json!({"session": id.to_string()})
    }

    #[test]
    fn a_leaving_pane_gives_its_place_to_its_sibling() {
        let (a, b, c) = (Ulid::new(), Ulid::new(), Ulid::new());
        let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
            leaf(a),
            {"split": "horizontal", "ratio": 0.6, "children": [leaf(b), leaf(c)]}
        ]});
        // An inner pane: its split collapses to the sibling, the outer ratio stays.
        assert_eq!(
            without_session(&layout, c),
            Some(json!({"split": "vertical", "ratio": 0.5, "children": [leaf(a), leaf(b)]}))
        );
        // An outer pane: the other subtree becomes the whole layout.
        assert_eq!(
            without_session(&layout, a),
            Some(json!({"split": "horizontal", "ratio": 0.6, "children": [leaf(b), leaf(c)]}))
        );
        // The last pane leaves an empty tab (`null`, like a new tab).
        assert_eq!(without_session(&leaf(a), a), Some(Value::Null));
        // Not in the layout: nothing to write.
        assert_eq!(without_session(&layout, Ulid::new()), None);
        assert_eq!(without_session(&Value::Null, a), None);
    }
}
```

In `daemon.rs`, after `list_tabs` (:2749-2756), add the helper:

```rust
/// Waits for a `tab.updated` of `tab_id` carrying exactly `layout`. A pane leaves its tab a
/// moment after the session's exit is listed (the daemon edits the tab right after storing it).
fn wait_layout(client: &mut Client, tab_id: Ulid, layout: &serde_json::Value) {
    client
        .wait_event_where(|e| matches!(e, Event::TabUpdated(t) if t.id == tab_id && t.layout == *layout));
}
```

`run_commands_start_in_their_folders_…`, replace the tail (:5261-5272):

```rust
    // A command removed from the setup still stops with "stop all", and a stopped command's
    // pane leaves the run tab (the last one leaves an empty tab).
    client
        .call(set_runs(&project, Vec::new(), None))
        .into_result::<Project>()
        .unwrap();
    client
        .call(run_stop(&project, None))
        .into_result::<()>()
        .unwrap();
    client.wait_state(new_be, SessionState::Exited);
    client.wait_state(fe_id, SessionState::Exited);
    wait_layout(&mut client, first.tab_id.unwrap(), &serde_json::Value::Null);
    assert_eq!(
        list_tabs(&mut client, &project)[0].layout,
        serde_json::Value::Null
    );
    client.stop();
```

Also, in the same test after the restart assert (:5256-5259), pin the middle step so the old pane is not shown next to the new one:

```rust
    // The old BE's exit must not take the new BE's pane with it.
    wait_layout(&mut client, first.tab_id.unwrap(), &pair(new_be, fe_id));
```

`run_commands_auto_start_after_a_daemon_restart_and_end_with_their_command`, after `client.wait_state(sessions[0].id, SessionState::Exited);` (:5406):

```rust
    // The command ended by itself: its pane left the run tab.
    let tab = list_tabs(&mut client, &project)[0].id;
    wait_layout(&mut client, tab, &serde_json::Value::Null);
```

Run: `cargo test -p mkrew-daemon --lib tabs::tests -j 2`
Expected: FAIL to compile (`unresolved import super::without_session`).
Run: `cargo test -p mkrew-daemon --test daemon run_commands_start_in_their_folders -j 2`
Expected: FAIL (timeout in `wait_layout`: the layout is still `pair(new_be, fe_id)`).

- [ ] **Step 2: Implement `tabs.rs`**

Above the test module in `crates/mkrew-daemon/src/tabs.rs`:

```rust
//! Edits of tab layouts the daemon makes itself (SPEC §5: the layout is the UI's split tree,
//! stored as opaque JSON; only leaves and the splits around them are touched here).

use mkrew_protocol::Event;
use serde_json::Value;
use ulid::Ulid;

use crate::session::Hub;

/// What removing a pane did to one node of the tree.
enum Cut {
    /// The node was the pane itself.
    Gone,
    /// The node now looks like this (a split collapsed to its other child, or a child changed).
    Became(Value),
}

fn cut(node: &Value, session: &str) -> Option<Cut> {
    if node.get("session").and_then(Value::as_str) == Some(session) {
        return Some(Cut::Gone);
    }
    let children = node.get("children")?.as_array()?;
    // A split always has two children (SPEC §5); anything else is not ours to edit.
    let [first, second] = children.as_slice() else {
        return None;
    };
    for (index, (child, sibling)) in [(first, second), (second, first)].into_iter().enumerate() {
        match cut(child, session) {
            None => {}
            // The sibling takes the split's place, like closing a pane in the UI.
            Some(Cut::Gone) => return Some(Cut::Became(sibling.clone())),
            Some(Cut::Became(new)) => {
                let mut node = node.clone();
                node["children"][index] = new;
                return Some(Cut::Became(node));
            }
        }
    }
    None
}

/// `layout` without the pane of `session`; `None` if it is not in it, `Some(Value::Null)` if
/// it was the last pane.
fn without_session(layout: &Value, session: Ulid) -> Option<Value> {
    match cut(layout, &session.to_string())? {
        Cut::Gone => Some(Value::Null),
        Cut::Became(layout) => Some(layout),
    }
}

impl Hub {
    /// Takes the pane of `session_id` out of every tab of its project and announces each
    /// changed tab as `tab.updated` (a session that exited has nothing to show; its output
    /// stays reachable in History). One store lock, so a concurrent `tab.set_layout` is not
    /// overwritten.
    pub fn leave_tabs(&self, project_id: Ulid, session_id: Ulid) {
        let updated = {
            let store = self.store();
            store.list_tabs(project_id).and_then(|tabs| {
                tabs.into_iter()
                    .filter_map(|tab| {
                        let layout = without_session(&tab.layout, session_id)?;
                        Some(store.set_tab_layout(tab.id, &layout))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
        };
        match updated {
            Ok(tabs) => {
                for tab in tabs {
                    let _ = self.events.send(Event::TabUpdated(tab));
                }
            }
            Err(e) => tracing::warn!(%session_id, "pane not removed from its tab: {e}"),
        }
    }
}
```

`lib.rs`, add `mod tabs;` after `mod snapshot;` (:23). `Hub.events` and `Hub::store` are `pub` (used as `self.hub.events` in `server.rs`), so no visibility change.

- [ ] **Step 3: Hook `Actor::finish`**

`session.rs:1382-1399`, replace the interrupted block and the tail:

```rust
        // Cut off by the daemon stop, not ended by the user or the agent: resume it at the
        // next start (SPEC §8). A reboot leaves the row live instead; startup marks those.
        let stopping = self.hub.stopping.load(Ordering::SeqCst);
        if stopping && let Err(e) = self.hub.store().mark_interrupted(self.session.id) {
            tracing::warn!(id = %self.session.id, "not marked interrupted: {e}");
        }
        self.session.exit_code = exit_code;
        self.session.ended_at = Some(crate::server::now_ms());
        self.set_state(SessionState::Exited, StateSource::Daemon);
        let _ = self
            .hub
            .events
            .send(Event::SessionExited(SessionExitedEvent {
                session_id: self.session.id,
                exit_code,
            }));
        // A run command that ended (stopped, or by its own `; exit`) leaves the run tab
        // (ADR 0018). After `Exited` is stored: `show_run_tab` lists live sessions only, so it
        // cannot write the pane back. Never while the daemon stops: the pane must stay for the
        // resume at the next start (SPEC §8, ADR 0017).
        if !stopping && mkrew_core::run::command_name(&self.session).is_some() {
            self.hub
                .leave_tabs(self.session.project_id, self.session.id);
        }
        tracing::info!(id = %self.session.id, ?exit_code, "session exited");
```

Run: `cargo test -p mkrew-daemon --lib tabs::tests -j 2`
Expected: PASS.
Run: `cargo test -p mkrew-daemon --test daemon run_commands -j 2`
Expected: PASS (all four `run_commands_*` tests; the restart assert still ends in `pair(new_be, fe_id)`).
Run: `cargo test -p mkrew-daemon --test daemon restart_resumes -j 2`
Expected: PASS (run sessions only here; Task 17 relies on these for the trap).

- [ ] **Step 4: SPEC**

§7 (:501), replace the `project.run_stop` result cell with:

```markdown
ok — asks the run sessions to terminate (without `name`: every live run session of the project, also one whose name left the config); ok when nothing runs. Each one leaves the `run` tab when it exits (§10)
```

§7 (:500), after "then starts them like `run_start`" add: "; the new session (a new id: a run is one session with its own log and History entry) takes the old pane's place in the `run` tab".

§8 (:609-612), after "so each session is resumed at most once." add: "A session that exits while the daemon runs leaves its tab layout; one cut off by the daemon stop keeps its pane, so the resume above finds it (ADR 0033)."

§10 (:714-715), replace "Restart and per-command Run keep the view." with:

```markdown
Restart and per-command Run keep the view. A stopped command, and one that ends by itself, leaves
the `run` tab at once (the tab stays, empty after the last one). Restart puts the new session where
the old pane was, so the tab shows one terminal per command, never an old one beside a new one.
```

- [ ] **Step 5: HUMAN_CHECKS**

In the 2026-10-09 section:

```markdown
- [ ] **Run ■ / ↻:** set up two commands (a dev server and one more) and press ▶. ■ on one (the
  Run menu row) → its pane leaves the `run` tab at once and the other keeps running; ■ on the last
  → the `run` tab is empty. ▶ again → panes are back. ↻ on one → still one pane per command, in the
  same place, with fresh output (no "Session exited" pane beside it). A command that ends by itself
  (`echo hi`) → its pane leaves the tab.
```

- [ ] **Step 6: Verify and tick**

Run: `cargo clippy -p mkrew-daemon --all-targets -j 4 -- -D warnings`, then the full verification. Tick inbox line 37 `[x]`.
Suggested message: `fix(run): a stopped or finished run command leaves the run tab`

---

### Task 17: Exited sessions leave the sidebar and their panes (ADR 0033)

Inbox: line 38 `when a shell or agent is stopped, it still showing in the sidebar cannot clickable can just remove, so need auto clean up the workspace also, there're no meaning when juz show to remove because cannot click able anymore juz to close`. Spec: §8 (:609-612), §10 (:674-678, :692-693), §11 (:738, :745-747). ADR 0019 stays as it is. Needs Task 16 (`Hub::leave_tabs`, the `finish` hook).

**Current state (investigated):**
- `Sidebar::rows` (`crates/mkrew-ui/src/sidebar.rs:253-299`) lists every session of the workspace, exited ones too (no state filter, :256-260). `session_row` (`main.rs:15393-15474`) makes an exited row non-clickable on purpose (`let live = row.state != SessionState::Exited;` :15408-15409, comment "Run again / Resume come later"; click, hover and pointer only `.when(live, …)` :15421-15430). The only action is the hover ✕ → `remove_session` (:6630) → `session.remove`.
- `session.remove` = **hide** (ADR 0019, daemon `session_remove` server.rs:1078): the session leaves `session.list` for good, so it also leaves History (`history::finished` reads `session.list`). That is why auto-hiding on exit is wrong: the exit code and log would be gone from History. Exited sessions must stay in `Sidebar::sessions()` (History, Inbox, `is_exited` :1253-1258, the pane placeholder :13910, `close_action` lifecycle.rs) and only leave the sidebar **rows**.
- Pane close today: `close_action` (`lifecycle.rs`): exited → `Remove` (pane only); live shell → `Kill` (the pane is removed, the row stays as `✕ exited`: this is the inbox complaint for shells); live agent → `Ask` (Kill / Hide pane / Cancel; Hide keeps the session running in the sidebar).
- An exited pane stays in its tab (`sync_panes` does not attach it, so it is only a "✕ Session exited (code N)" placeholder). Nothing prunes it.
- Zoom and History: a zoomed pane is a local `Tab.zoomed` that `Tab::fix_focus` (tabs.rs:99-106) ends when the pane leaves the layout, so the other panes come back. The History replay is a dialog (`history_pane`, `main.rs:3706`), not a tab pane: unaffected.
- The resume trap (Task 16, "Current state"): the hook skips `Hub.stopping`; `mark_stale_sessions_exited` sends no events. So a client-side prune on `SessionExited` would be wrong (the UI also receives `SessionExited` for every session cut off by a daemon stop while still connected, and `tab.set_layout` would then erase the panes that must resume). The removal is therefore daemon-side, in the same hook (Task 16), where `stopping` is known.
- Existing tests that pin the old behaviour: `exit_shows_the_code_and_unknown_sessions_ask_for_a_refresh` (`sidebar.rs:514-538`, asserts the row text `exited (1)`). `first_load_expands_workspaces_with_live_sessions_newest_first` (:416-440) already puts an exited session in a collapsed workspace: unchanged. UI `tests/lifecycle.rs:112-194` reads `session()` states, not rows after the kill: unchanged.

**Decision:**
- **Sidebar:** `rows()` shows live sessions only. Workspace counts (`working`, `needs_input`) are unchanged (exited sessions never counted). A workspace whose sessions all exited shows `No sessions`. `sessions()` stays unfiltered. No setting, no timer, no hide.
- **Panes:** the Task 16 hook applies to every session, not only run ones: a shell or agent that exits while the daemon runs (typed `exit`, `/exit`, crash, ✕ close of a live shell, Kill) leaves its tab. Panes of sessions cut off by a daemon stop stay (resume, and as before a non-resumable one is a placeholder until the user closes it). Panes that already exited before this version stay too, and so do panes of exited sessions in a layout after a daemon restart (the user closes them with ✕; a startup sweep would delete panes of archived workspaces that "resume when added again", `archived_workspaces_are_not_resumed_until_added_again` daemon.rs:5500). Zoom ends by itself.
- The exit code and the output stay in History (the History dialog lists exited sessions).
- Not changed: `session_row`'s ✕ (it still kills a live session and hides it, ADR 0019), `close_action`.

**Files:**
- Modify: `crates/mkrew-daemon/src/session.rs:1397-1408` (drop the run-only condition of the Task 16 hook)
- Modify: `crates/mkrew-ui/src/sidebar.rs:256-260` (`rows`), test `exit_shows_the_code_…` (:514-538)
- Modify: `crates/mkrew-ui/src/main.rs:15408-15430` (`session_row`: every row is live now)
- Modify tests: `crates/mkrew-daemon/tests/daemon.rs` (new `an_exited_session_leaves_its_tab_but_a_daemon_stop_keeps_the_pane` after `restart_resumes_free_agent_panes_in_tabs_once` :4211)
- Create: `docs/decisions/0033-exited-sessions-leave-the-sidebar-and-tabs.md`
- Modify: `docs/SPEC.md` §8, §10, §11; `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `Hub::leave_tabs` (Task 16), `wait_layout` (Task 16, daemon.rs), `spawn_params` (daemon.rs:2718), `create_tab`/`list_tabs`.
- Produces: no new API.

- [ ] **Step 1: Failing tests**

`sidebar.rs`, replace `exit_shows_the_code_and_unknown_sessions_ask_for_a_refresh` (:514-538) with:

```rust
    /// An exited session cannot be opened, so it has no row; History, the Inbox and the pane
    /// placeholder still read it from `sessions()`. A session this list never saw asks for a
    /// refresh.
    #[test]
    fn an_exited_session_leaves_the_rows_but_stays_listed() {
        let api = project("api");
        let s = session(&api, 1, SessionState::Working);
        let mut sidebar = Sidebar::default();
        sidebar.load(vec![api.clone()], vec![s.clone()]);
        assert_eq!(session_ids(&sidebar.rows()), vec![s.id]);

        let exited = Event::SessionExited(SessionExitedEvent {
            session_id: s.id,
            exit_code: Some(1),
        });
        assert_eq!(sidebar.apply(&exited), Change::Redraw);
        assert_eq!(
            sidebar.rows()[1],
            Row::NoSessions { project_id: api.id }
        );
        let kept = &sidebar.sessions()[0];
        assert_eq!((kept.state, kept.exit_code), (SessionState::Exited, Some(1)));

        let spawned = Event::SessionState(SessionStateEvent {
            session_id: Ulid::new(),
            state: SessionState::Starting,
            source: StateSource::Daemon,
            changed_at: 1,
        });
        assert_eq!(sidebar.apply(&spawned), Change::Refresh);
    }
```

`daemon.rs`, after `restart_resumes_free_agent_panes_in_tabs_once` (:4211):

```rust
/// ADR 0033: a session that exits while the daemon runs leaves its tab (shell or agent
/// alike, the sibling takes its place); a daemon stop does not, so the pane is still there
/// for the resume (ADR 0017) after the restart.
#[test]
fn an_exited_session_leaves_its_tab_but_a_daemon_stop_keeps_the_pane() {
    let env = Env::new();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let mut shell = |client: &mut Client| -> Ulid {
        client
            .call(spawn_params(&project, SessionKind::Shell, None))
            .into_result::<SessionSpawnResult>()
            .unwrap()
            .session_id
    };
    let (a, b) = (shell(&mut client), shell(&mut client));
    let tab: TerminalTab = client
        .call(create_tab(&project, "shells"))
        .into_result()
        .unwrap();
    client
        .call(Request::TabSetLayout(TabSetLayoutParams {
            tab_id: tab.id,
            layout: pair(a, b),
        }))
        .into_result::<()>()
        .unwrap();

    // `a` ends while the daemon runs: its pane goes, `b` takes the whole tab.
    client
        .call(Request::SessionKill(SessionKillParams {
            session_id: a,
            force: true,
        }))
        .into_result::<()>()
        .unwrap();
    client.wait_state(a, SessionState::Exited);
    let alone = json!({"session": b.to_string()});
    wait_layout(&mut client, tab.id, &alone);
    assert_eq!(list_tabs(&mut client, &project)[0].layout, alone);
    // The session itself is still listed: History keeps its exit code and output.
    assert_eq!(client.session(a).state, SessionState::Exited);

    // The daemon stop ends `b` too, but its pane stays for the next start.
    client.stop();
    assert!(daemon.wait_exit().success());
    let (_daemon, mut client) = env.start();
    assert_eq!(list_tabs(&mut client, &project)[0].layout, alone);
    client.stop();
}
```

Run: `cargo test -p mkrew-ui --lib sidebar::tests::an_exited_session -j 4`
Expected: FAIL (`left: Row::Session(..)`, `right: Row::NoSessions { .. }`).
Run: `cargo test -p mkrew-daemon --test daemon an_exited_session_leaves_its_tab -j 2`
Expected: FAIL (timeout in `wait_layout`: Task 16's hook skips non-run sessions).

- [ ] **Step 2: Implement**

`session.rs` (the Task 16 hook), replace the condition and comment:

```rust
        // A session that exited while the daemon runs has nothing to show: its pane leaves
        // the tab (ADR 0033; its output stays in History). After `Exited` is stored, so
        // `show_run_tab`, which lists live sessions only, cannot write a run pane back. Never
        // while the daemon stops: the pane must stay for the resume at the next start
        // (SPEC §8, ADR 0017).
        if !stopping {
            self.hub
                .leave_tabs(self.session.project_id, self.session.id);
        }
```

`sidebar.rs:256-260`, `rows`:

```rust
            let mut sessions: Vec<&Session> = self
                .sessions
                .iter()
                // An exited session cannot be opened: no row (History and the Inbox read it
                // from `sessions()`; ADR 0033).
                .filter(|session| {
                    session.project_id == project.id && session.state != SessionState::Exited
                })
                .collect();
```

`rows()` doc (:252): `/// Rows to show: each workspace, then (if expanded) its live sessions, newest first.`

`main.rs` `session_row`, delete :15408-15409 (the comment and `let live`) and replace the `.when(live, |el| {…})` wrapper (:15421-15430) by its body directly:

```rust
        .cursor_pointer()
        .hover(move |style| style.bg(rgb(hover)))
        .on_click(cx.listener(move |shell, _, window, cx| {
            shell.open_session(session_id, window, cx);
        }))
        .on_hover(cx.listener(move |shell, hovered: &bool, window, cx| {
            shell.hover_row(Place::Sidebar, session_id, *hovered, window, cx);
        }))
```

(`.hover()` is still called once on this element.) `status_color`/`Exited` arms stay: History and the pane placeholder use them.

Run: `cargo test -p mkrew-ui --lib sidebar -j 4`
Expected: PASS.
Run: `cargo test -p mkrew-daemon --test daemon an_exited_session_leaves_its_tab -j 2`
Expected: PASS.
Run: `cargo test -p mkrew-daemon --test daemon archived_workspaces -j 2`
Expected: PASS (it does not depend on exits while running).

`restart_resumes_free_agent_panes_in_tabs_once` (daemon.rs:4091-4211) needs one change, or it becomes racy: it writes the layout `pair(shown, quit)` right after spawning, while `quit` exits by itself a moment later, and the hook now removes `quit`'s leaf if the exit comes after the write. Its point is "a pane whose session exited before the stop is not resumed" (a pane left over from before this version, or after a restart). Move the `TabSetLayout` call (:4142-4148) to just after `client.wait_state(quit.session_id, SessionState::Exited);` (:4156) and keep the rest. The leaf of an already exited session is then written after its exit, so it stays, and the test still pins that it is not resumed and that the `stopping` side keeps the resumed pane (`layout(resumed.id)` asserts :4188-4191, :4206-4209 stay as they are).

Run: `cargo test -p mkrew-daemon --test daemon restart_resumes -j 2`
Expected: PASS (the trap: `Hub.stopping` keeps the panes that resume).

- [ ] **Step 3: ADR 0033**

Create `docs/decisions/0033-exited-sessions-leave-the-sidebar-and-tabs.md`:

```markdown
# 0033 — Exited sessions leave the sidebar and their tabs

- Date: 2026-10-09
- Status: accepted
- Milestone: inbox 2026-10-09

## Context

A shell or agent that exited stayed in the sidebar as `✕ exited` (not clickable, only a hover ✕ to
remove it) and its pane stayed in its tab as "Session exited". A stopped run command did the same
in the `run` tab. The row had no use, and ✕ = `session.remove` hides the session for good (ADR
0019), which also drops it from History. The daemon cannot just hide on exit for the same reason.
A daemon stop also exits every session; those panes must stay in their tabs, because the next
start resumes them from there (ADR 0017).

## Options

1. Filter exited sessions out of the sidebar rows only; the daemon removes the pane of a session
   from its tabs when it exits while the daemon runs (not while it stops).
2. Hide the session on exit (`session.remove`).
3. Make exited rows clickable (open a replay) and keep them.
4. A setting to keep or drop them.

## Decision

Option 1. The session stays in `session.list`, so History keeps its exit code, log and replay
(option 2 would delete that, option 3 keeps the clutter the user wants gone, option 4 is a knob
nobody asked for). The daemon does it in `Actor::finish`, where it knows whether it is stopping;
a UI-side prune on `session.exited` would also fire for the exits of a daemon stop and erase the
panes that must resume. A restarted run command is a new session in the old pane's place; the old
one is no longer in the sidebar.

## Consequences

SPEC §8, §10 and §11 change; no protocol change. After a daemon stop the exited panes of
non-resumable sessions (shells, agents without an id) stay in their tab as a placeholder until the
user closes them, as before (a startup sweep would also remove the panes of archived workspaces,
which resume when added again). Closing a live shell with ✕ now also removes its sidebar row.
```

- [ ] **Step 4: SPEC**

§8 (:609-612) — the sentence added in Task 16 already says this; no further change.

§10, sidebar bullet (:674-675), replace its first sentence with: "**Sidebar:** workspaces; expanding one lists its live sessions with status (§11); a session that exited leaves the list and its pane leaves its tab (ADR 0033; History keeps it)."

§10 Remove bullet (:676-678), replace with:

```markdown
- **Remove:** the workspace menu's "Remove workspace…" archives it after a confirmation
  (`project.archive`, ADR 0019); a session row's ✕ (on hover) removes a live session from the
  sidebar (`session.remove`; it asks first and the session is stopped). A session that exits needs
  no removing (ADR 0033).
```

§10 Terminals (:692-693), after "Closing a pane with a running agent asks: kill the session, or just hide it (still reachable from the sidebar)." add: "A pane whose session exits closes itself."

§11 table (:738), replace the exited row with `| exited | ✕ | exit code (History); its sidebar row and pane close (ADR 0033) |`, and :745-746 "Workspace row: …" stays.

- [ ] **Step 5: HUMAN_CHECKS**

In the 2026-10-09 section:

```markdown
- [ ] **Exited sessions clean up (ADR 0033):** `exit` in a shell pane → the pane and its sidebar row
  disappear (the sibling pane fills the space; a zoomed pane unzooms); quit an agent (`/exit`) →
  the same; ✕ on a live shell pane → no `✕ exited` row left behind. History (toolbar) still lists
  them with their exit code and replays their output. A workspace with no live session shows
  "No sessions". **Daemon restart:** with a live agent pane (one with a resumable conversation) open,
  `mk daemon stop`, start Mkrew again → the pane resumes in the same place (it was not removed).
```

- [ ] **Step 6: Verify and tick**

Run: `cargo clippy -p mkrew-ui --all-targets -j 4 -- -D warnings`, `cargo clippy -p mkrew-daemon --all-targets -j 4 -- -D warnings`, then the full verification. Tick inbox line 38 `[x]`.
Suggested message: `feat: exited sessions leave the sidebar and their panes`

---

## Open questions (defaults used above)

| # | Area | Question | Options | Default in this plan |
| --- | --- | --- | --- | --- |
| 1 | Files editor | How far editing goes (items 28, 29) | a) full in-app editor · b) find/replace only · c) read-only + Edit in Zed | **a** (anh chose it, 2026-10-09) |
| 2 | Files editor + find/replace | Replace one shortcut | VS Code Ctrl+Shift+1 (taken by Board; duplicate detection is context-blind) / Enter in the replace field | Enter in the replace field (+ "Replace" button) |
| 3 | Files editor + find/replace | Redo keys | Ctrl+Y and Ctrl+Shift+Z (= Open in Zed) / Ctrl+Y only | Ctrl+Y only |
| 4 | Files editor + find/replace | How Save writes | in place `std::fs::write` (keeps permissions and links, not crash-atomic) / temp file + rename | in place (ponytail comment in `files::write`) |
| 5 | Files editor + find/replace | Undo grouping | a typed run until the caret moves / one word | a typed run |
| 6 | Files editor + find/replace | Ctrl+C without a selection | copy nothing / copy the line (VS Code) | copy nothing |
| 7 | Files editor + find/replace | Tab key | `\t` in tab-indented files, else 4 spaces, no outdent / Shift+Tab outdent and block indent | insert only |
| 8 | Files editor + find/replace | Caret column | monospace cell grid (CJK glyphs misplace it) / shaped-line positions | cell grid |
| 9 | Files editor + find/replace | Long lines | horizontal scrollbar; the caret is not followed sideways / follow the caret | not followed |
| 10 | Files editor + find/replace | Worktree removed with unsaved edits (`drop_gone_targets`) | drop them silently (the folder is gone) / ask | drop |
| 11 | Files editor + find/replace | Where `mod editor` lives | `lib.rs` (the Files view-model is a lib module and needs it) / `main.rs` as the brief said | `lib.rs` |
| 12 | Files editor + find/replace | AltGr chars on Ctrl+Alt+digit (tab jump) | cannot be typed in the editor, like in terminals / rebind | as in terminals |
| 13 | Several git repos | Commit draft when switching repo | (a) one draft per workspace, carried over (as worktree switches do today); (b) one draft per repo (`HashMap<repo, TextBox>`); (c) drop it | **(a)**: no new state; the draft is visible, so the user sees what they would commit |
| 14 | Several git repos | Discovery depth | (a) direct children only; (b) depth 2 (`apps/web`) | **(a)**: the inbox case; depth 2 later if asked |
| 15 | Several git repos | Root repo + nested repos in the root's own status | (a) leave `fe/` listed as an untracked folder in the root repo; (b) hide child repo folders from the root status | **(a)**: no submodule logic; the chooser reaches `fe` |
| 16 | Several git repos | Turn snapshots / checkpoints / worktrees for child repos | (a) out of scope, documented in ADR 0029 and SPEC; (b) per-repo snapshot refs and checkpoint maps (migration) | **(a)**: large; the Git view covers the inbox ask |
| 17 | Several git repos | Base branch per repo | (a) keep `project.base_ref`, applied to the shown repo (auto-detect works per repo); (b) per-repo base (migration) | **(a)** |
| 18 | Several git repos | First-show flash on a non-repo root (status sent before `git.repos` answers) | (a) accept a brief "Cannot read the git status"; (b) delay the first status until `git.repos` answers | **(a)**; switch to (b) if the blink is visible in the HUMAN_CHECKS run |
| 19 | Scrollback | Is 200 lines after a tab switch enough? | **Keep the daemon's 200 (SPEC §8 budget)**; raise it to e.g. 2,000 (≈ 10× daemon grid memory per session, §15 limits); also keep hidden panes' UI emulators | Keep 200; revisit if the user still hits the limit |
| 20 | Scrollback | The wheel does nothing in full-screen apps that turn on mouse mode (alt screen + `?1000`/`?1006`), because no SGR wheel reports are sent | **Out of scope here**; encode `ESC[<64;col;rowM` / `65` in `TerminalView::scroll_wheel` (view.rs:345-371) | Out of scope; a new inbox item if it shows up with a real agent |
| 21 | Scrollback | An overflow resnapshot still cuts the UI's 10k-line history down to 200 | **Accept** (output was dropped anyway, so there is a gap); paint into the existing emulator (duplicates the old screen) | Accept |
| 22 | Pane keys + default shell | Existing workspaces have no default shell after the upgrade | a) ask on the first new shell pane (the user's "must select") · b) the migration sets `default_shell = 'shell'`, so nothing changes until they pick | **a** |
| 23 | Pane keys + default shell | On this machine, the `bash` on PATH is WSL's (WindowsApps comes before Git's `cmd` folder) | a) ship both `bash.toml` (PATH) and `git-bash.toml` (full default install path) · b) only `git-bash.toml` · c) only `bash.toml`, and the user edits it | **a** |
| 24 | Pane keys + default shell | Run commands and the default shell | a) keep the `shell` profile (the exit syntax stays stable) · b) use `default_shell` when set | **a** |
| 25 | Pane keys + default shell | Also hide agents that are not installed | a) no, only shells (a missing agent fails loudly and names PATH) · b) hide both | **a** |
| 26 | Pane keys + default shell | Keys for the two panes | a) Alt+Shift+A / Alt+Shift+S (the split-key family) · b) Ctrl+Shift+D / Ctrl+Shift+G · c) make Ctrl+Shift+A/T split and move new-tab elsewhere | **a** |
| 27 | Rename + auto-name | Rename-pane key | **F2** (Windows rename key; TUIs lose F2) / Alt+Shift+R / Ctrl+Shift+R on a focused pane, tab otherwise | **F2** |
| 28 | Rename + auto-name | `mk session rename <id> <title>` CLI | add now / **skip until asked** | **skip** |
| 29 | Rename + auto-name | Keep Task 14 (automatic name) | **yes** / drop it (Task 13 ships alone and ticks line 32) | **yes** |
| 30 | Rename + auto-name | Which sessions get an automatic name | **agent sessions only** / every session (shells show paths and commands) | **agents only** |
| 31 | Rename + auto-name | Where the rename field starts | **the user's title only** / the shown title, including the automatic one (Enter would freeze it) | **user's title** |
| 32 | Field keys | Word rule of Ctrl+Backspace | **(a) editor / VS Code rule: blanks, then one run of word chars or of punctuation** · (b) terminal ^W rule (up to whitespace) | **(a)**: `src/main.rs` goes one piece at a time, the same in every field and in the Files editor; (b) would delete a whole path per press |
| 33 | Field keys | Alt+Backspace | **(a) stays a plain Backspace** · (b) also delete a word | **(a)**: Windows has no Alt+Backspace field key; the terminal keeps ESC DEL |
| 34 | Field keys | Cmd+Backspace on macOS | **(a) same as Ctrl (delete a word)** · (b) delete to the start | **(a)**: Windows first; revisit with the caret-model follow-up |
| 35 | Field keys | One type for every single-line field | **(a) new `text_box::Input`, `TextBox` wraps it** · (b) reuse `TextBox` with `Enter` overrides per field · (c) a `all_selected: bool` next to each `String` | **(a)**: ~20 lines, one place for the select-all and word rules; (b) drags newline handling into 12 fields; (c) copies the logic 12 times |
| 36 | Field keys | Select the whole name when a rename starts (Windows Explorer does) | (a) yes · **(b) no** | **(b)**: not asked; it changes Task 13's start state and its tests |
| 37 | Field keys | Caret movement (arrows, Home / End), Shift selection, Ctrl+Z, Ctrl+C / X in fields | **(a) out of scope: a follow-up moves every field onto `editor.rs`** (caret-aware `text_field`, `ime.rs` selection) · (b) add them now | **(a)**: the inbox names Ctrl+A and Ctrl+Backspace; (b) needs a caret model, a new renderer and an IME rewrite |
| 38 | Field keys | Decomposed (NFD) Vietnamese letters | **(a) accept: Ctrl+Backspace stops at a combining mark (`// ponytail:` in `text_box::class`)** · (b) add unicode-segmentation | **(a)**: Windows IMEs type precomposed letters; no new crate |
| 39 | Run stop + exited cleanup | `run_stop` waits for the exit and calls `show_run_tab` (scout's draft) or the `finish` hook does it? | (a) hook in `Actor::finish` for all exits; (b) `run_stop` + `wait_exited` + `show_run_tab` plus a separate hook for self-ending commands | (a): one place, covers Stop, `; exit` and later Task 17 |
| 40 | Run stop + exited cleanup | Panes that exited before this version, or after a daemon restart (not resumable), stay as "Session exited" placeholders? | (a) leave them, user closes with ✕; (b) sweep exited leaves at startup after `resume_sessions` | (a): (b) would also strip panes of archived workspaces that "resume when added again" |
| 41 | Run stop + exited cleanup | Restart reuses the session id? | (a) new id (current); (b) respawn into the same id | (a): an id is one process with one log/History entry |
| 42 | Run stop + exited cleanup | Brief single-pane flash in the run tab during ↻ of one of two commands? | (a) accept; (b) make `run_restart` skip the hook (needs a per-session flag) | (a): a few ms, no input lost |
