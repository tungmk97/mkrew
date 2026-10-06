# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M10
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M10 — Instructing (SPEC §18, §20B)

_No task queue yet: the next session plans M10 (`mkrew-plan-milestone`)._

### Done (last 10)

v1 (M0–M8) and M9 are complete (tags `m1-done` … `m9-done`, tasks in `docs/progress-archive.md`).
v2 = M9–M13 in SPEC §20 (ADR 0010 for the git exceptions). The loop ends after M13.

### Carried over

M9 left 6 of 7 criteria unticked; all wait on the T9.11 wrap-up checks in HUMAN_CHECKS (peek +
quick reply, turn summary, sound, `powercfg /requests` + unix compile, broadcast with real agents,
Inbox last line, §14 design check). M3–M8 criteria still wait too (T3.4–T3.9, T4.7–T4.10,
T5.4–T5.11, T6.8–T6.11, T7.7–T7.12, T8.3–T8.10). If one fails, add a fix task to the current
milestone first.

## Notes for the next session

- M10 must use the real `queued` from `session.send` → `{send_id, queued}` in broadcast (`mkrew_ui::broadcast::Broadcast::sent` drops its `working` param; Shell `send_broadcast`). PROTOCOL_VERSION is 17 after M9.
- Settings: daemon `crate::settings::SettingsHolder` (`Daemon.settings: Arc<_>`, `current()` = last good); `mkrew_core::settings::Settings` holds every §20.0 key (M10's `ComposerSettings` is already there). Measurements: `crates/mkrew-ui/tests/measure.rs` (how to run: ADR 0009).
- `session.read` (`Shell::read_last_lines`, `Shell::read_peek`), turns (`mkrew_ui::turn::Turns`), peek (`mkrew_ui::peek`) are the M9 building blocks. `cargo insta` is not installed: write accepted `.snap` files by hand.
- GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s (parallel load).
- Headless sessions here can't run new scripts directly (approval); run them through a temporary `cargo test` if needed.
- Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms).
- Stray locked exes `target/debug/*.locked-*.exe` can be deleted when unlocked. A build failing with "Access is denied" on an exe: `mv` it aside.
- Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
