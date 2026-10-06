# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M9
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M9 — Attention (SPEC §18, §20.0, §20A)

_Queue empty: the next session plans M9 with the `mkrew-plan-milestone` skill._

v1 (M0–M8) is complete (tags `m1-done` … `m8-done`, tasks in `docs/progress-archive.md`).
v2 = M9–M13 in SPEC §20 (ADR 0010 for the git exceptions). The loop ends after M13.

### Carried over

M3–M8 left criteria unticked; all wait on manual checks in HUMAN_CHECKS (T3.4–T3.9, T4.7–T4.10,
T5.4–T5.11, T6.8–T6.11, T7.7–T7.12, T8.3–T8.10). If one fails, add a fix task to the current
milestone first.

## Notes for the next session

- Measurements: `crates/mkrew-ui/tests/measure.rs` (`#[ignore]`, real window; first `cargo build --release -p mkrew-ui --example idle_floor`, then `cargo test --release -p mkrew-ui --test measure -- --ignored --nocapture`); results in ADR 0009.
- Headless sessions here can't run new scripts directly (approval); run them through a temporary `cargo test` if needed.
- Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms).
- Stray locked exes `target/debug/mkrew-daemon.locked-t81.exe`, `mkrew-fakeagent.locked-t711.exe` can be deleted when unlocked.
- Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
