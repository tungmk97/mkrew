# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M11
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M11 — Reviewing per turn (SPEC §18, §20C)

### Acceptance criteria
- [x] A turn snapshot is taken at each turn start without changing HEAD, branches, the index or
      any file (real-git test asserts all four); the `hook.report` reply for a turn-start hook waits for it.
- [x] `turn.list`, `turn.restore_preview`, `turn.restore`: preview lists changed/created/deleted
      files; restore only with `confirm` and a matching fingerprint; e2e with the fake agent over 2 turns.
- [x] Snapshot on the generated 10,000-file repo < 300 ms on Windows (perf test like §15).
- [ ] Git view: "Changes since turn n" (`since` accepts a turn's tree) and the "Changed by" session filter.
- [ ] Follow in Zed: debouncer unit-tested; manual check with a real agent.

### Task queue
- [x] T11.1 Daemon git snapshot primitive: `snapshot(dir) -> tree` via a temp copy of the index (`git rev-parse --git-path index`, works in worktrees) + `GIT_INDEX_FILE` + `add -A` + `write-tree`, temp index removed; `update-ref refs/mkrew/turns/<sid>/<n>`; per-project one-at-a-time lock; real-git test asserting HEAD, branch, index and files unchanged (incl. ignored files excluded); perf test on the generated 10,000-file repo < 300 ms (like `git_view.rs` budget test); covers: snapshot, perf. (spec: §20C.1, §15, §20F) — feat(daemon): git turn snapshot primitive
- [x] T11.2 Migration `0008_turns.sql` + store API (start turn, end turn with end_tree/files, list by session, sessions ended > 7 days for ref cleanup); store tests; covers: persistence for turn.list. (spec: §20C protocol, §5, §20.0) — feat(store): persist agent turns
- [x] T11.3 Daemon wiring: snapshot in `spawn_blocking` at each agent turn start (git repos only), end snapshot when the turn ends done/idle/exited, persist the turn row with per-file +/− (numstat start→end tree) and fill `session.turn` counts; `hook.report` for `UserPromptSubmit` replies after the snapshot; startup deletes `refs/mkrew/turns/<sid>/*` of sessions ended > 7 days; tests (fake agent `edit_files`); covers: snapshot at turn start, hook reply waits. (spec: §20C.1, §20.0, §20A.3, §8) — feat(daemon): snapshot and persist agent turns
- [x] T11.4 Protocol `turn.list`, `turn.restore_preview`, `turn.restore` (reuse `undo.rs`: temp-index `read-tree` + `checkout-index -a -f`, delete listed still-new files, fingerprint check, refused while `working`, real index untouched); PROTOCOL_VERSION 20; CLI/UI clients adapt; e2e with the fake agent over 2 turns; covers: turn.* methods. (spec: §20C.2, §20C protocol, §7, ADR 0005) — feat(daemon): list and restore agent turns
- [x] T11.5 `git.status`/`git.diff` `since` accepts a tree id; `session.files {project_id, since_ms}` from turn rows + hook edits; event `session.edited {session_id, path, line?}` for every hook-reported edit; tests; covers: since a turn's tree, Changed-by data, follow data. (spec: §20C.3, §20C.4, §20C.5, §8) — feat(daemon): diff since a turn, session files and edit events
- [ ] T11.6 UI "Turns…" in the agent pane header menu: dialog listing turns (n, time, files, +/−), "Restore before turn n" → preview + confirm reusing `undo_dialog`, disabled while working; "Diff since" entry; covers: restore UI. (spec: §20C.2, §13, §14)
- [ ] T11.7 Git view "Since turn…" mode (base mode with `since` = start tree, header `Since turn 3 of claude #2`) + "Changed by: All | <session>…" filter over `session.files` (last 24 h); pure filter tests + snapshot; covers: Git view since turn, Changed by. (spec: §20C.3, §20C.4, §12B, §14)
- [ ] T11.8 Follow in Zed: pure debouncer (last path wins, `zed.follow_debounce_ms`; unit tests), pane header toggle + palette command (off by default, UI memory only), reacts to `session.edited` and runs `zed <abs path>:<line?>` via the existing Zed launcher; covers: Follow in Zed. (spec: §20C.5, §20.0, §11)
- [ ] T11.9 Add M11 manual checks to HUMAN_CHECKS (Turns dialog + restore with a real agent, Since turn / Changed by, Follow in Zed with a real agent, perf on Windows, §14 design check), then milestone wrap-up. (spec: §18 M11)

### Done (last 10)

v1 (M0–M8), M9 and M10 are complete (tags `m1-done` … `m10-done`, tasks in `docs/progress-archive.md`).
v2 = M9–M11 and M13 in SPEC §20 (ADR 0010 for the git exceptions; M12 dropped, ADR 0013). After
M11 comes M13. The loop ends after M13.

### Carried over

M10 left 5 of 6 criteria unticked and M9 6 of 7; all wait on the T10.10 / T9.11 wrap-up checks
in HUMAN_CHECKS. M3–M8 criteria still wait too (T3.4–T3.9, T4.7–T4.10, T5.4–T5.11, T6.8–T6.11,
T7.7–T7.12, T8.3–T8.10). If one fails, add a fix task to the current milestone first.

## Notes for the next session

- T11.1–T11.3: `crate::snapshot::{Snapshots, turn_ref}` (on `Hub::snapshots`; `Hub::git` is the shared `GitBackend`, also `Daemon::git`); `crate::turns::{record, Job, delete_expired_refs}`: one recorder task per session (in `Hub::tasks`) persists turns via `Store::{start_turn, end_turn, list_turns, get_turn}` and broadcasts `session.turn` with counts. End trees: `refs/mkrew/turns/<sid>/<n>-end`. `Side::Between(a, b)` diffs two trees. `Command::Hook` carries a `reply` oneshot (dropped after handling). Fake agent start output is screen-detected as turn 1: e2e tests hook `done` then `busy` to get a hook-started turn 2 (`a_hook_turn_start_is_snapshotted_before_the_agent_edits`).
- M11 builds on M9 turns (`mkrew_ui::turn::Turns`, daemon `session.turn`, fake agent `edit_files`) and M10's composer (`mkrew_ui::composer::Composers`; main `composer_typed(session, text)` prefills a composer, `send_selection` shows the pattern).
- T11.4: `server/turn.rs`; `turn.restore_preview` returns `UndoPreview` (`checkpoint` = start tree, `added` always empty, deletions in `changed`); `undo::{preview_turn, restore_turn}`; `Snapshots::{tree, checkout}` (unreferenced snapshot of now; checkout scoped to changed paths, ADR 0014). `GitBackend::{changed_files, diff}` take `to: Option<&str>` (a tree). T11.5: `git.status`/`git.diff` `since` = a tree id (`GitBackend::tree_of`; `git_view::{status, diff}` take a `snapshot` closure → `Hub::snapshots.tree`); `merge_base` is then the tree. `session.files` (`server/turn.rs`, `Store::project_turns` + session events `edited`) returns `SessionFiles` (`BTreeMap<Ulid, Vec<String>>`). Event `session.edited {session_id, path, line?}` from `Hub::hook_edits` (`mkrew_core::hook::edited_line`); the UI ignores it so far (sidebar match arm).
- Protocol is v21 (`turn.*`, `session.files`, `session.edited`). Settings: daemon `crate::settings::SettingsHolder`; `mkrew_core::settings::Settings` holds every §20.0 key. Measurements: `crates/mkrew-ui/tests/measure.rs` (ADR 0009).
- Ideas left from M10 reviewers: Esc/arrows in the Run with… picker, one "…" card menu, visible error for unsupported images, drag-over highlight, multi-line Files picks.
- `cargo insta` is not installed: write accepted `.snap` files by hand. GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s.
- Headless sessions can't run new scripts directly (use a temporary `cargo test`). Build "Access is denied" on an exe: `mv` it aside. Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms).
- Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
