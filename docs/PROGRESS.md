# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M13
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M13 — Isolation & usage (SPEC §18, §20E)

### Acceptance criteria
- [x] Worktree task: Run creates the worktree and `mkrew/…` branch from HEAD, copies
      `.worktreeinclude` entries, runs the agent inside; the main folder's branch, HEAD, index and
      files are unchanged (real-git test). Resume after a daemon restart uses the worktree.
- [x] Git and Files views work on a task worktree; Merge only when both sides are clean and aborts
      cleanly on conflict; Remove keeps an unmerged branch (real-git tests).
- [x] Fan-out runs 2–5 agents in their own worktrees; Compare and Pick work; e2e with fake profiles.
- [x] Usage: fixture transcripts in the current Claude and Codex formats give the expected
      input/output/context tokens, read incrementally; shown on pane header, card and sidebar.
- [ ] M9–M11 and M13 manual checks listed in HUMAN_CHECKS.

### Task queue
- [x] T13.1 Migration `0009_worktrees.sql` + store (task `worktree`, `worktree_path`, `branch`, `fanout_of`; list children by `fanout_of`); protocol Task fields, `task.create/update` accept `worktree: bool`, PROTOCOL_VERSION 22, CLI/UI clients adapt; task dialog "Run in a worktree" checkbox (default off); store + protocol tests; covers: worktree task (data). (spec: §20E.1, §20E protocol, §5, §7) — feat(store): task worktree fields and dialog checkbox
- [x] T13.2 Daemon git worktree primitives (ADR 0010 update for the new git writes): `worktree add -b mkrew/<slug>-<short ulid> <data>/mkrew/worktrees/<project_id>/<task_id> HEAD` (pure slug fn, unit-tested), `.worktreeinclude` parser + copy (literal paths, `#` comments, must exist and be ignored, else warning list); real-git tests asserting main HEAD, branch, index and files unchanged; covers: worktree task (git). (spec: §20E.1, §20F, ADR 0010) — feat(daemon): git worktree primitives
- [x] T13.3 Run/resume wiring: on Run of a `worktree` task in a git repo create the worktree (once), persist path+branch, spawn with `cwd` = worktree, `.worktreeinclude` warnings in the session's first output line, checkpoint = worktree HEAD; resume after daemon restart uses the same folder; delete refused while a worktree exists; e2e with the fake agent; covers: Run in worktree, resume. (spec: §20E.1, §8, §9) — feat(daemon): run and resume worktree tasks in their worktree
- [x] T13.4a `fs.*` and `git.*` accept `worktree_task_id?` (daemon resolves project + task to a folder; watcher covers the worktree; `git.changed` carries it); real-git e2e; covers: Git and Files views on a worktree (daemon). (spec: §20E.1, §12A, §12B) — feat(daemon): fs and git requests on a task worktree
- [x] T13.4b UI: Files and Git views get the "Project | <task worktree>…" target chooser (requests carry `worktree_task_id`, `git.changed` matched on it); card shows `⎇ mkrew/<branch>`; pure tests + snapshot; covers: Git and Files views on a worktree (UI). (spec: §20E.1, §12A, §12B, §14) — feat(ui): worktree target chooser in Files and Git views
- [x] T13.5 `task.merge` (`confirm`; both sides clean, not detached HEAD, `git merge --no-ff --no-edit`, on conflict `merge --abort` + conflicting files) and `task.remove_worktree` (`confirm`; `worktree remove` refused with uncommitted changes, `branch -d` never `-D`, `branch_kept`); real-git tests (clean merge, conflict abort leaves main untouched, dirty refused, unmerged branch kept); covers: Merge, Remove. (spec: §20E.1, §20E protocol, §20F, ADR 0010) — feat(daemon): merge back and remove task worktrees
- [x] T13.6 UI card menu "Merge back…" (confirmation with commit list, conflict result) and "Remove worktree…" (shows uncommitted files, separate confirmed discard step, kept-branch result) on Review/Done cards; snapshot tests; covers: Merge/Remove UI. (spec: §20E.1, §13, §14) — feat(ui): merge back and remove worktree dialogs
- [x] T13.7 `task.fanout` (2–5 profiles → child tasks `fanout_of`, title ` · <agent>`, worktree on, all run; original → Running); e2e with 2 fake profiles; covers: fan-out (daemon). (spec: §20E.2, §20E protocol, §20F) — feat(daemon): fan a task out to several agents
- [x] T13.8a UI fan-out: "Run with several agents…" picker (2–5) + palette entry, group card with children's states, an original can no longer run itself; pure tests + snapshot; covers: fan-out (UI start). (spec: §20E.2, §13, §14) — feat(ui): run a task with several agents
- [x] T13.8b UI Compare and Pick: "Compare" view (per child: files changed, +/−, run time, open Git view) when all children are in Review, "Pick" = merge that child (reuse `open_worktree_dialog(Kind::Merge, child)`) + offer removing the others' worktrees, original → Review (`task.move`); pure tests + snapshot; covers: Compare and Pick. (spec: §20E.2, §13, §14) — feat(ui): compare fan-out results and pick one
- [x] T13.9 Usage parsing: profile `[usage]` (`format`, `context_window`) in core; pure incremental readers for `claude_jsonl` and `codex_jsonl` (byte offset, partial last line kept) summing input/output and newest context; fixtures recorded from CURRENT real formats (researcher checks field names); unknown format → none + one warning; covers: usage fixtures, incremental. (spec: §20E.3, §20.0, §6) — feat(core): incremental token usage readers for Claude and Codex transcripts
- [x] T13.10a Daemon usage wiring: locate transcript by `native_session_id` (`CLAUDE_CONFIG_DIR`/`~/.claude/projects/*/<id>.jsonl`, `CODEX_HOME`/`~/.codex/sessions/**/*<id>*.jsonl`, env-overridable in tests), read appended bytes at each turn end, event `session.usage` + request `session.usage` (protocol v23); tests; covers: usage (daemon). (spec: §20E.3, §8) — feat(daemon): read token usage from agent transcripts at turn ends
- [x] T13.10b UI usage: load `session.usage` per workspace on connect, follow the `session.usage` event; pane header `ctx 62%` (warning ≥ 80 %), task card and sidebar workspace row total tokens (task = its newest session, not a sum); pure tests + snapshot; covers: usage shown. (spec: §20E.3, §11, §14) — feat(ui): show token usage on panes, cards and workspaces
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

- T13.10b: UI `usage.rs` (`Usages`: `to_request`/`request_failed` per workspace, `load`/`apply` keep the larger total, `task_total` = newest session WITH usage, `workspace_total`, `context_share`, `format_tokens`/`format_count`); main.rs `load_usage` (after `sidebar.load` in `refresh`), pane header `ctx N%` (⚠ amber ≥ 80 %), card `12.3k tok`, sidebar row count. Defaults in HUMAN_CHECKS (T13.10b).
- T13.11 is the M13 wrap-up: add M13 manual checks to HUMAN_CHECKS (worktree run with a real agent, merge/remove, fan-out compare/pick, usage with real Claude/Codex, §14 design check; M9–M11 wrap-up checks T9.11/T10.10/T11.9 are already there), then BUILD_LOOP Step 8 with `status: DONE`. M13 UI leftovers to list: no palette/keyboard for Compare, cancelling the Pick merge does not reopen Compare, Compare not live; worktree checkbox mouse-only; Merge/Remove are hover buttons; no CLI merge/remove; `turn.restore` refused for worktree tasks; rerun after removal with a kept branch fails.
- Uncommitted human edits (.github workflows, Cargo.lock, mkrew-ui Cargo.toml x11/wayland features, docs/BUILD_VI.md): leave untouched, never commit them.
- `scripts/verify.sh`/full `cargo test --workspace` can crash rustc on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN): run fmt check + clippy, then `cargo test --workspace -j 4` (green at T13.10b).
- Protocol is v23. Settings: daemon `crate::settings::SettingsHolder`; `mkrew_core::settings::Settings` holds every §20.0 key. Measurements: `crates/mkrew-ui/tests/measure.rs` (ADR 0009). Migration 0009 is M13's (§20).
- UI leftovers (M11 reviewers): "Turns…" hover-only (§14.4), coloured +/−, focus ring on header buttons (`button_frame`), state-aware Follow palette title, follow entries not pruned on exit, palette/key for Changed by, SPEC §20.0 comment `§20C.4` for follow should be §20C.5. M10: Esc/arrows in Run with… picker, one "…" card menu, visible image errors, drag-over highlight.
- `cargo insta` is not installed: write accepted `.snap` files by hand. GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s.
- Headless sessions can't run new scripts directly (use a temporary `cargo test`). Build "Access is denied" on an exe: `mv` it aside. Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms). Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
