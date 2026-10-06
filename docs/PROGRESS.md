# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M10
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M10 — Instructing (SPEC §18, §20B)

### Acceptance criteria
- [ ] Composer: a multi-line send arrives as one bracketed paste; `@` inserts `file_ref`-formatted
      paths; drafts survive switching panes.
- [x] Send queue: `session.send` returns `{send_id, queued}`, `session.queue` events, `session.unqueue`;
      e2e with the fake agent: a text queued while working and removed is never typed.
- [ ] Prompt library: samples seeded on first run, variables rendered (unit tests), insertable from
      the palette, the composer and the task dialog.
- [ ] Send selection from Files, Git diff and a terminal opens the right session's composer, prefilled.
- [ ] Dropping files types their paths; pasting an image stores it via `attachment.save` and types
      the path; attachments of sessions ended > 7 days ago are deleted.
- [ ] "Run with…" on a Todo/Review card updates the task's agent and runs it.

### Task queue
- [x] T10.1 Daemon send queue: `session.send` → `{send_id, queued}`, `session.unqueue` (`not_found` once written), event `session.queue` after every change, PROTOCOL_VERSION 18; CLI + UI client adapt; broadcast rows use the real `queued`; e2e: fake agent working, queued text removed is never typed; covers: send queue. (spec: §20B.2, §7, §8) — feat(daemon): visible send queue with unqueue
- [x] T10.2 `mkrew_core::prompt`: `render(template, vars)` (unknown/unavailable vars → empty + listed), library load from `<config>/mkrew/prompts/*.md` (stem = name, optional `# Title`), seed `review.md`/`explain.md`/`fix-tests.md` when the dir is missing; unit tests; covers: prompt library (seed, render). (spec: §20B.3, §20.0) — feat(core): prompt library with render and seeded samples
- [x] T10.3 UI multi-line text box (Enter newline, Ctrl+Enter submit, Esc) + composer docked at a focused agent pane's bottom via `Ctrl+Shift+Enter`, send = `session.send` with submit, drafts per session in UI memory, keys local to the composer; covers: composer (multi-line send, drafts). (spec: §20B.1, §13, §14) — feat(ui): prompt composer docked on agent panes
- [x] T10.4 Composer `@` opens quick-open and inserts `composer.file_ref` (pure formatter: relative, `/`, quoted if spaces; unit tests); composer lists queued texts with ✕ (`session.unqueue`); pane header `⧗ N queued`; covers: composer `@`, send queue UI. (spec: §20B.1, §20B.2, §12A) — feat(ui): composer @ file refs and send queue list
- [ ] T10.5 Reuse the text box in task create/edit and Board follow-up; prompt entry points: palette "Insert prompt…", composer "Prompts" menu, task dialog "From prompt…", preview with muted unknown-vars note; covers: prompt library (insertable). (spec: §20B.1, §20B.3)
- [ ] T10.6 `Ctrl+Shift+L` send selection from Files (picked lines), Git diff (new-side lines) and terminal (selected text): session picker with the §20B.4 default, opens that session's composer prefilled (pure prefill formatter, unit tests); covers: send selection. (spec: §20B.4, §12, §12A, §12B)
- [ ] T10.7 Daemon `attachment.save` (ext whitelist, base64 ≤ 10 MiB, `<data>/mkrew/attachments/<session_id>/<ulid>.<ext>`) + delete attachments of sessions ended > 7 days ago in the hourly backup check; tests; covers: attachments (save, cleanup). (spec: §20B.5, §7, §8)
- [ ] T10.8 UI: dropping OS files (`ExternalPaths`) on an agent pane types quoted absolute paths without Enter; `Ctrl+Shift+V` with a clipboard image → `attachment.save` → types the path; pure path-joining formatter tested; covers: drop files, paste image. (spec: §20B.5, §13)
- [ ] T10.9 Board card menu "Run with…" (profile picker) for Todo/Review cards without a live session: update the task's agent, then run; test on the update+run sequence; covers: Run with. (spec: §20B.6, §10)
- [ ] T10.10 Add M10 manual checks to HUMAN_CHECKS (composer feel, `@` picker, send selection, drop/paste, Run with…, §14 design check), then milestone wrap-up. (spec: §18 M10)

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

- T10.1: PROTOCOL_VERSION 18. `session.send` returns `SessionSendResult {send_id, queued}`; `session.unqueue`; `Event::SessionQueue`. Daemon: `Command::Send`/`Command::Unqueue`, `Actor::announce_queue`.
- T10.3/T10.4: `mkrew_ui::text_box::TextBox` (append-only, keys via `task_dialog::dialog_key`) — T10.5 reuses it. `mkrew_ui::composer::Composers` (`open/hide/shown/key/insert/begin_send/sent`, `apply(event)`/`queue(session)` for `session.queue`, pure `file_ref`/`first_line`). main: `composer_session()`, `focused_agent()`, `open_composer`, `composer_panel`, `composer_typed(session, text)` (insert + refocus — T10.5/T10.6 prefill can use it), `open_file_ref_picker`; `QuickOpen.composer` = `@` mode; `Shell.settings_file` (UI reads `mkrew_core::settings::load` on demand).
- T10.2: `mkrew_core::prompt::{load_or_seed(&paths.prompts_dir()), render(&body, &PromptVars{file,selection,task,project})}` → `Rendered {text, missing}`; `Prompt {name, title, body}` (title line stripped). Only `{ident}` braces are variables; others stay literal. UI (T10.5) calls `load_or_seed` directly.
- Settings: daemon `crate::settings::SettingsHolder` (`Daemon.settings: Arc<_>`, `current()` = last good); `mkrew_core::settings::Settings` holds every §20.0 key (M10's `ComposerSettings` is already there). Measurements: `crates/mkrew-ui/tests/measure.rs` (how to run: ADR 0009).
- `session.read` (`Shell::read_last_lines`, `Shell::read_peek`), turns (`mkrew_ui::turn::Turns`), peek (`mkrew_ui::peek`) are the M9 building blocks. `cargo insta` is not installed: write accepted `.snap` files by hand.
- GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s (parallel load).
- Headless sessions can't run new scripts directly (approval; use a temporary `cargo test`). Build "Access is denied" on an exe: `mv` it aside (stray `target/debug/*.locked-*.exe` can be deleted). Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms).
- Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
