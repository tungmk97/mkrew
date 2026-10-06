# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: DONE
milestone: M8
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## All milestones complete (M0–M8, SPEC §18)

_M0–M8 tasks moved to `docs/progress-archive.md` (tags `m1-done` … `m8-done`; unticked criteria are in HUMAN_CHECKS)._

The build loop is finished. What is left is the human's: the "To verify" list in
`docs/HUMAN_CHECKS.md` (every milestone's wrap-up entry, T3.x … T8.10) and the
"Questions with assumed defaults". If a check fails, add a fix task here (e.g. a new
"M9 — Fixes" section with a queue) and set `status: RUNNING`.

### Carried over

M8 left 4 of 5 criteria unticked; all wait on the T8.3–T8.9 and T8.10 manual checks in
HUMAN_CHECKS (command palette + keymap settings, dev server with a real command, MSI on a clean
Windows, tar.gz on Linux, key → glyph latency, §14 design check). M3–M7's unticked criteria
(T3.4–T3.9, T4.7–T4.10, T5.4–T5.11, T6.8–T6.11, T7.7–T7.12 manual checks) still wait too.

## Notes for the next session

- Measurements: `crates/mkrew-ui/tests/measure.rs` (`#[ignore]`, real window; first `cargo build --release -p mkrew-ui --example idle_floor`, then `cargo test --release -p mkrew-ui --test measure -- --ignored --nocapture`); results in ADR 0009.
- Headless sessions here can't run new scripts directly (approval); run them through a temporary `cargo test` if needed.
- Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms).
- Stray locked exes `target/debug/mkrew-daemon.locked-t81.exe`, `mkrew-fakeagent.locked-t711.exe` can be deleted when unlocked.
- Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
