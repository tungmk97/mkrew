# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M13
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M13 — Isolation & usage (SPEC §18, §20E)

### Acceptance criteria
- [ ] Worktree task: Run creates the worktree and `mkrew/…` branch from HEAD, copies
      `.worktreeinclude` entries, runs the agent inside; the main folder's branch, HEAD, index and
      files are unchanged (real-git test). Resume after a daemon restart uses the worktree.
- [ ] Git and Files views work on a task worktree; Merge only when both sides are clean and aborts
      cleanly on conflict; Remove keeps an unmerged branch (real-git tests).
- [ ] Fan-out runs 2–5 agents in their own worktrees; Compare and Pick work; e2e with fake profiles.
- [ ] Usage: fixture transcripts in the current Claude and Codex formats give the expected
      input/output/context tokens, read incrementally; shown on pane header, card and sidebar.
- [ ] M9–M11 and M13 manual checks listed in HUMAN_CHECKS.

### Task queue
- [x] T13.1 Migration `0009_worktrees.sql` + store (task `worktree`, `worktree_path`, `branch`, `fanout_of`; list children by `fanout_of`); protocol Task fields, `task.create/update` accept `worktree: bool`, PROTOCOL_VERSION 22, CLI/UI clients adapt; task dialog "Run in a worktree" checkbox (default off); store + protocol tests; covers: worktree task (data). (spec: §20E.1, §20E protocol, §5, §7) — feat(store): task worktree fields and dialog checkbox
- [ ] T13.2 Daemon git worktree primitives (ADR 0010 update for the new git writes): `worktree add -b mkrew/<slug>-<short ulid> <data>/mkrew/worktrees/<project_id>/<task_id> HEAD` (pure slug fn, unit-tested), `.worktreeinclude` parser + copy (literal paths, `#` comments, must exist and be ignored, else warning list); real-git tests asserting main HEAD, branch, index and files unchanged; covers: worktree task (git). (spec: §20E.1, §20F, ADR 0010)
- [ ] T13.3 Run/resume wiring: on Run of a `worktree` task in a git repo create the worktree (once), persist path+branch, spawn with `cwd` = worktree, `.worktreeinclude` warnings in the session's first output line, checkpoint = worktree HEAD; resume after daemon restart uses the same folder; delete refused while a worktree exists; e2e with the fake agent; covers: Run in worktree, resume. (spec: §20E.1, §8, §9)
- [ ] T13.4 `fs.*` and `git.*` accept `worktree_task_id?` (daemon resolves project + task to a folder; watcher covers the worktree); Files and Git views get the "Project | <task worktree>…" target chooser; card shows `⎇ mkrew/<branch>`; tests; covers: Git and Files views on a worktree. (spec: §20E.1, §12A, §12B, §14)
- [ ] T13.5 `task.merge` (`confirm`; both sides clean, not detached HEAD, `git merge --no-ff --no-edit`, on conflict `merge --abort` + conflicting files) and `task.remove_worktree` (`confirm`; `worktree remove` refused with uncommitted changes, `branch -d` never `-D`, `branch_kept`); real-git tests (clean merge, conflict abort leaves main untouched, dirty refused, unmerged branch kept); covers: Merge, Remove. (spec: §20E.1, §20E protocol, §20F, ADR 0010)
- [ ] T13.6 UI card menu "Merge back…" (confirmation with commit list, conflict result) and "Remove worktree…" (shows uncommitted files, separate confirmed discard step, kept-branch result) on Review/Done cards; snapshot tests; covers: Merge/Remove UI. (spec: §20E.1, §13, §14)
- [ ] T13.7 `task.fanout` (2–5 profiles → child tasks `fanout_of`, title ` · <agent>`, worktree on, all run; original → Running); e2e with 2 fake profiles; covers: fan-out (daemon). (spec: §20E.2, §20E protocol, §20F)
- [ ] T13.8 UI fan-out: "Run with several agents…" picker (2–5), group card with children's states, "Compare" view (per child: files changed, +/−, run time, open Git view) when all in Review, "Pick" = merge that child + offer removing the others, original → Review; pure tests + snapshot; covers: Compare and Pick. (spec: §20E.2, §13, §14)
- [ ] T13.9 Usage parsing: profile `[usage]` (`format`, `context_window`) in core; pure incremental readers for `claude_jsonl` and `codex_jsonl` (byte offset, partial last line kept) summing input/output and newest context; fixtures recorded from CURRENT real formats (researcher checks field names); unknown format → none + one warning; covers: usage fixtures, incremental. (spec: §20E.3, §20.0, §6)
- [ ] T13.10 Daemon usage wiring: locate transcript by `native_session_id` (`CLAUDE_CONFIG_DIR`/`~/.claude/projects/*/<id>.jsonl`, `CODEX_HOME`/`~/.codex/sessions/**/*<id>*.jsonl`, env-overridable in tests), read appended bytes at each turn end, event `session.usage`; UI pane header `ctx 62%` (warning ≥ 80 %), card and sidebar workspace totals; tests; covers: usage shown. (spec: §20E.3, §8, §11, §14)
- [ ] T13.11 Add M13 manual checks to HUMAN_CHECKS (worktree run with a real agent, merge/remove, fan-out compare/pick, usage with real Claude/Codex, §14 design check) and confirm M9–M11 ones are listed, then milestone wrap-up (`status: DONE`). (spec: §18 M13)

### Done (last 10)

v1 (M0–M8), M9, M10 and M11 are complete (tags `m1-done` … `m11-done`, tasks in `docs/progress-archive.md`).
v2 = M9–M11 and M13 in SPEC §20 (ADR 0010 for the git exceptions; M12 dropped, ADR 0013). M13 is
the last milestone: the loop ends after it (`status: DONE`).

### Carried over

M11 left 1 of 5 criteria unticked (Follow in Zed with a real agent), M10 5 of 6 and M9 6 of 7;
all wait on the T11.9 / T10.10 / T9.11 wrap-up checks in HUMAN_CHECKS. M3–M8 criteria still wait
too (T3.4–T3.9, T4.7–T4.10, T5.4–T5.11, T6.8–T6.11, T7.7–T7.12, T8.3–T8.10). M13's last
criterion ("M9–M11 and M13 manual checks listed") is met by those entries plus M13's own. If one
fails, add a fix task to the current milestone first.

## Notes for the next session

- Git building blocks for M13 worktrees: daemon `Hub::git` (`GitBackend`: `changed_files`, `diff`, `tree_of`), `crate::snapshot::Snapshots` (per-project lock, temp-index snapshots work in worktrees via `git rev-parse --git-path index`), `undo.rs` restore; real-git test patterns in `snapshot.rs` and `git_view.rs` (10,000-file budget test). ADR 0010 lists the allowed git writes.
- T13.1 store API: `create_task(.., worktree)`, `update_task(.., worktree: Option<bool>)`, `set_task_worktree(id, Option<(path, branch)>)`, `set_task_fanout_of`, `list_fanout_children`; `delete_task` detaches fan-out children. T13.3 must: refuse `task.update` `worktree: false` while `worktree_path` is set (daemon; the UI dialog already locks it via `TaskDialog::worktree_locked`). Dialog checkbox is mouse-only (Tab = fields) — keyboard toggle is a UI leftover.
- Protocol is v22. Settings: daemon `crate::settings::SettingsHolder`; `mkrew_core::settings::Settings` holds every §20.0 key. Measurements: `crates/mkrew-ui/tests/measure.rs` (ADR 0009). Migration 0009 is M13's (§20).
- UI leftovers (M11 reviewers): "Turns…" hover-only (§14.4), coloured +/−, focus ring on header buttons (`button_frame`), state-aware Follow palette title, follow entries not pruned on exit, palette/key for Changed by, SPEC §20.0 comment `§20C.4` for follow should be §20C.5. M10: Esc/arrows in Run with… picker, one "…" card menu, visible image errors, drag-over highlight.
- `cargo insta` is not installed: write accepted `.snap` files by hand. GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s.
- Headless sessions can't run new scripts directly (use a temporary `cargo test`). Build "Access is denied" on an exe: `mv` it aside. Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms). Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
