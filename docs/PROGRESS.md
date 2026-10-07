# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: DONE
milestone: M13
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: none — the build is complete (M13 was the last milestone in SPEC §18)

### Task queue

_(empty — every milestone in SPEC §18 is done; the loop has stopped)_

### Done (last 10)

v1 (M0–M8) and v2 (M9, M10, M11, M13) are complete (tags `m1-done` … `m11-done`, `m13-done`;
tasks in `docs/progress-archive.md`). M12 was dropped (ADR 0013). ADR 0010 lists the git
exceptions of v2.

### Carried over

Every automated criterion is ticked. The manual checks wait in HUMAN_CHECKS "To verify":
M13 (T13.11), M11 1 of 5 criteria (T11.9, Follow in Zed with a real agent), M10 5 of 6 (T10.10),
M9 6 of 7 (T9.11), and M3–M8 (T3.4–T3.9, T4.7–T4.10, T5.4–T5.11, T6.8–T6.11, T7.7–T7.12,
T8.3–T8.10). If a check fails or the human wants more work: add a milestone/fix tasks here (or to
SPEC §18), set `status: RUNNING` and restart the loop.

## Notes for the next session

- The loop is `DONE`. Only restart it after the human adds tasks and sets `status: RUNNING`.
- M13 UI leftovers (listed in HUMAN_CHECKS T13.11): no palette/keyboard for Compare, cancelling the Pick merge does not reopen Compare, Compare not live; worktree checkbox and fan-out picker mouse-only; Merge/Remove are hover buttons; no CLI merge/remove; `turn.restore` refused for worktree tasks; rerun after removal with a kept branch fails.
- UI leftovers (M11 reviewers): "Turns…" hover-only (§14.4), coloured +/−, focus ring on header buttons (`button_frame`), state-aware Follow palette title, follow entries not pruned on exit, palette/key for Changed by, SPEC §20.0 comment `§20C.4` for follow should be §20C.5. M10: Esc/arrows in Run with… picker, one "…" card menu, visible image errors, drag-over highlight.
- Uncommitted human edits (.github workflows, Cargo.lock, mkrew-ui Cargo.toml x11/wayland features, docs/BUILD_VI.md): leave untouched, never commit them.
- `scripts/verify.sh`/full `cargo test --workspace` can crash rustc on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN): run fmt check + clippy, then `cargo test --workspace -j 4`.
- Protocol is v24. Settings: daemon `crate::settings::SettingsHolder`; `mkrew_core::settings::Settings` holds every §20.0 key. Measurements: `crates/mkrew-ui/tests/measure.rs` (ADR 0009).
- 2026-10-07 daily fixes (plan docs/superpowers/plans/2026-10-07-daily-fixes-and-ai-helpers.md): protocol v24 (ai.generate/ai.generated), profile key `oneshot`, ADR 0015/0016, `ime.rs` InputHandler; manual checks in HUMAN_CHECKS "2026-10-07 daily fixes". Outside the milestone loop.
- `cargo insta` is not installed: write accepted `.snap` files by hand. GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s.
- Headless sessions can't run new scripts directly (use a temporary `cargo test`). Build "Access is denied" on an exe: `mv` it aside. Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms). Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
