# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M11
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M11 — Reviewing per turn (SPEC §18, §20C)

### Task queue

_(empty — the next session plans M11 with `mkrew-plan-milestone`)_

### Done (last 10)

v1 (M0–M8), M9 and M10 are complete (tags `m1-done` … `m10-done`, tasks in `docs/progress-archive.md`).
v2 = M9–M13 in SPEC §20 (ADR 0010 for the git exceptions). The loop ends after M13.

### Carried over

M10 left 5 of 6 criteria unticked and M9 6 of 7; all wait on the T10.10 / T9.11 wrap-up checks
in HUMAN_CHECKS. M3–M8 criteria still wait too (T3.4–T3.9, T4.7–T4.10, T5.4–T5.11, T6.8–T6.11,
T7.7–T7.12, T8.3–T8.10). If one fails, add a fix task to the current milestone first.

## Notes for the next session

- M11 builds on M9 turns (`mkrew_ui::turn::Turns`, daemon `session.turn`, fake agent `edit_files`) and M10's composer (`mkrew_ui::composer::Composers`; main `composer_typed(session, text)` prefills a composer, `send_selection` shows the pattern).
- Protocol is v19 (`attachment.save`). Settings: daemon `crate::settings::SettingsHolder`; `mkrew_core::settings::Settings` holds every §20.0 key. Measurements: `crates/mkrew-ui/tests/measure.rs` (ADR 0009).
- Ideas left from M10 reviewers: Esc/arrows in the Run with… picker, one "…" card menu, visible error for unsupported images, drag-over highlight, multi-line Files picks.
- `cargo insta` is not installed: write accepted `.snap` files by hand.
- GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s.
- Headless sessions can't run new scripts directly (use a temporary `cargo test`). Build "Access is denied" on an exe: `mv` it aside. Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms).
- Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
