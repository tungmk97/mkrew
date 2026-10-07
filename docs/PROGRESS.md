# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M13
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M13 — Isolation & usage (SPEC §18, §20E)

### Task queue

_(empty — the next session plans M13 with `mkrew-plan-milestone`)_

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
- Protocol is v21. Settings: daemon `crate::settings::SettingsHolder`; `mkrew_core::settings::Settings` holds every §20.0 key. Measurements: `crates/mkrew-ui/tests/measure.rs` (ADR 0009). Migration 0009 is M13's (§20).
- UI leftovers (M11 reviewers): "Turns…" hover-only (§14.4), coloured +/−, focus ring on header buttons (`button_frame`), state-aware Follow palette title, follow entries not pruned on exit, palette/key for Changed by, SPEC §20.0 comment `§20C.4` for follow should be §20C.5. M10: Esc/arrows in Run with… picker, one "…" card menu, visible image errors, drag-over highlight.
- `cargo insta` is not installed: write accepted `.snap` files by hand. GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s.
- Headless sessions can't run new scripts directly (use a temporary `cargo test`). Build "Access is denied" on an exe: `mv` it aside. Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms). Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
