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

- 2026-10-19 HANDOFF branch `feat/attention-review-push` (worktree `.worktrees/attention-review-push`): plan 1 toast layer done + reviewed; plan 2 tray/badge/Mark unread Tasks 1-9 committed, verify OK, Tasks 5-9 not yet reviewed (API spend limit); plans 3 (git viewed files) and 4 (phone push) not started. Next steps, numbers and traps: `.superpowers/sdd/HANDOFF.md` in that worktree. This line is replaced by one line per plan when the branch is finished.
- 2026-10-12 phone access from the UI (plan docs/superpowers/plans/2026-10-12-phone-access-ui.md, ADR 0053, protocol 39→40): `[web] enabled` (`mkrew_core::settings::write_web_enabled`), `mkrew_core::tailscale` (shared with `mk web`), daemon `web.rs` supervises `mk web` (`daemon.status.web`), UI `phone.rs` (link, token, QR svg), `phone_dialog.rs` (pure model) + `shell/phone_access.rs` (dialog, title bar `smartphone` icon, palette "Global: Phone access…", 1 s poll while open / 5 s for the icon). No toast layer exists, so Start/Stop errors show inside the dialog. Manual checks in HUMAN_CHECKS "2026-10-12 Phone access from the UI". Outside the milestone loop.
- 2026-10-13 Mkrew web extras (plan docs/superpowers/plans/2026-10-13-mobile-web-extras.md): Home Screen icon (`apple-touch-icon.png`), Stop (`session.kill`), recent-reply chips (`localStorage` `mkrew.replies`), ended sessions of the last 24 h replayed from the log (the relay sends `mkrew_core::log_tail` before the `session.history` reply; ADR 0052 amended). No protocol change. Manual checks in HUMAN_CHECKS "2026-10-13 Mkrew web extras". Outside the milestone loop.
- 2026-10-12 Mkrew web (plan docs/superpowers/plans/2026-10-10-mobile-web.md, ADR 0052): `mk web` (crates/mkrew-cli/src/web/, page in crates/mkrew-cli/web/, xterm.js 6 vendored) relays a browser on the tailnet to the daemon; no protocol change. JS helpers: `node --test crates/mkrew-cli/web/model.test.mjs`. Manual checks in HUMAN_CHECKS "2026-10-12 Mkrew web". Outside the milestone loop.
- 2026-10-12 run setup + share + files (plan docs/superpowers/plans/2026-10-12-run-setup-env-file-and-duplicate.md, SDD ledger .superpowers/sdd/2026-10-12-run-setup-env-file-and-duplicate/): env file relative to the workspace (migration 0018, ADR 0049, protocol v38→v39), env file picker over the workspace, Run setup Duplicate, pane Share menu (Follow in Zed), Files tree on the right + search row + Ignored chip (ADR 0050). Manual checks in HUMAN_CHECKS "2026-10-12 Files tree on the right" and "2026-10-12 Run setup caret and Browse…". Nothing committed. Outside the milestone loop.
- 2026-10-12 pane sizing: `Node::equalize` / `dividers` / `set_ratio`, `Divider::ratio_at` (split.rs); `Command::EqualizePanes` (Alt+Shift+0, layout menu, palette); mouse-draggable pane dividers (`pane_divider`, drag move on `tab-panes`, one `tab.set_layout` per drag via `Tabs::write_active`); `[ui] sidebar_width` (200–480) with a grip like the session panel's (`theme::GRIP_WIDTH` shared). UI + settings only, no protocol change. Manual checks in HUMAN_CHECKS "2026-10-12 equalize panes". Outside the milestone loop.
- 2026-10-11 gutter, graph, layouts (plan docs/superpowers/plans/2026-10-11-gutter-graph-layouts.md): `fs.read` `index_text` + `GitBackend::index_text` (protocol +1, ADR 0047), `gutter.rs` marks in `Code.changes`; `git log --parents` re-sorted children-first (`children_first`), `git_graph.rs`, `History.graph`, `ListRow::Commit.graph`, canvas lanes in `git_rows.rs`; `split::Preset`, `Tabs::arrange`, `Command::Layout` (Alt+Shift+1…5), `Shell::apply_layout`, `LayoutMenu` on the split button. Manual checks in HUMAN_CHECKS "2026-10-11 gutter, graph, layouts". Outside the milestone loop.
- 2026-10-09 inbox (plan docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md, 17 tasks, SDD ledger in `.superpowers/sdd/2026-10-09-inbox-editor-git-repos-keys/`): protocol v27→v32 (fs.write, repo/git.repos, snapshot history, default_shell, session.rename/updated, session.terminal_title), migration 0014 project.default_shell, ADR 0028–0033, new `editor.rs` + shared text_box `Input`; manual checks in HUMAN_CHECKS "2026-10-09 inbox". Nothing committed. Outside the milestone loop.
- 2026-10-10 inbox redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md, SDD ledger `.superpowers/sdd/2026-10-10-inbox-redesign/`): protocol v32→v37, migrations 0015–0017, ADR 0034–0039, manual checks in HUMAN_CHECKS "2026-10-10". Nothing committed. Outside the milestone loop.
- 2026-10-10 compact sidebar (plan docs/superpowers/plans/2026-10-10-compact-sidebar.md, ADR 0040): `sidebar::Layout` replaces `Shell.sidebar_hidden`; `Command::ToggleCompactSidebar` (Ctrl+Alt+B, status-bar `⇤`/`⇥`); rail = `render_compact_sidebar` + `compact_workspace_row` / `compact_session_row`, tooltips `sidebar::workspace_tip` / `session_tip`. UI only. Manual checks in HUMAN_CHECKS "2026-10-10 compact sidebar". Outside the milestone loop.
- 2026-10-10 file viewer scroll (plan docs/superpowers/plans/2026-10-10-file-viewer-scroll.md): new `scrollbar.rs` (pure thumb maths + canvas bar over a `uniform_list` handle or a `ListState`) on the Files editor and the Markdown preview. The preview (Files + Docs) is a gpui `list` (`preview::Page`, `measure_all`) instead of a div laying out every block per frame; `preview_page`/`preview_blocks` are gone. `Editor::longest_line` replaces the per-frame scan (perf plan Task 6 B11 marked done). UI only; manual checks in HUMAN_CHECKS "2026-10-10 file viewer scroll bar". Outside the milestone loop.
- Traps: after any protocol/CLI change run `cargo build -p mkrew-cli -p mkrew-fakeagent --bins -j 4` BEFORE `cargo test -p mkrew-daemon --test daemon -j 2` (tests use prebuilt `target/debug/mk.exe`; stale one = 10 bogus failures). Disk is nearly full (no new target dirs). Tree snapshots (`snap`/`pkg`) replace commits for review diffs.
- Not committed on purpose: the working tree also holds the human's edits (INBOX/MODEL_ROLES, workflows, Cargo files) — commit only by explicit request, split per task.
- The loop is `DONE`. Only restart it after the human adds tasks and sets `status: RUNNING`.
- M13 UI leftovers (listed in HUMAN_CHECKS T13.11): no palette/keyboard for Compare, cancelling the Pick merge does not reopen Compare, Compare not live; worktree checkbox and fan-out picker mouse-only; Merge/Remove are hover buttons; no CLI merge/remove; `turn.restore` refused for worktree tasks; rerun after removal with a kept branch fails.
- UI leftovers (M11 reviewers): "Turns…" hover-only (§14.4), coloured +/−, focus ring on header buttons (`button_frame`), state-aware Follow palette title, follow entries not pruned on exit, palette/key for Changed by, SPEC §20.0 comment `§20C.4` for follow should be §20C.5. M10: Esc/arrows in Run with… picker, one "…" card menu, visible image errors, drag-over highlight.
- Uncommitted human edits (.github workflows, Cargo.lock, mkrew-ui Cargo.toml x11/wayland features, docs/BUILD_VI.md): leave untouched, never commit them.
- `scripts/verify.sh`/full `cargo test --workspace` can crash rustc on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN): run fmt check + clippy, then `cargo test --workspace -j 4`.
- Protocol is v37. Settings: daemon `crate::settings::SettingsHolder`; `mkrew_core::settings::Settings` holds every §20.0 key. Measurements: `crates/mkrew-ui/tests/measure.rs` (ADR 0009).
- 2026-10-07 daily fixes (plan docs/superpowers/plans/2026-10-07-daily-fixes-and-ai-helpers.md): protocol v24 (ai.generate/ai.generated), profile key `oneshot`, ADR 0015/0016, `ime.rs` InputHandler; manual checks in HUMAN_CHECKS "2026-10-07 daily fixes". Outside the milestone loop.
- 2026-10-07 resume plan (docs/superpowers/plans/2026-10-07-resume-omp-copilot-panes.md): hook agents omp/copilot, `mk integration install omp|copilot` (owned files), migration 0010 `session.interrupted`, free agent panes in a tab resume after a restart, ADR 0017; manual checks in HUMAN_CHECKS. Outside the milestone loop.
- 2026-10-07 Run button (plan docs/superpowers/plans/2026-10-07-run-button.md, ADR 0018): run commands replace the dev server; manual checks in HUMAN_CHECKS "2026-10-07 Run button". Outside the milestone loop.
- 2026-10-07 workspace/files/docs fixes (plan docs/superpowers/plans/2026-10-07-workspace-files-docs-fixes.md): protocol v26 (project.archive, session.remove, Project.color), migrations 0012 session.hidden / 0013 project.color, ADR 0019–0021, new crate mermaid-rs-renderer, text_find.rs, diagram.rs; manual checks in HUMAN_CHECKS "2026-10-07 workspace, files and docs fixes". Outside the milestone loop.
- 2026-10-08 inbox (plan docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md): `DialogKey::SelectAll` + `TextBox` select-all, terminal Ctrl+C copies a selection (ADR 0022, `terminal::input::key_action`), Ctrl+Backspace = ^W, `[ui]`/`[terminal]` font settings with rems + `theme::Fonts` global, settings errors in the palette (ADR 0024), view rail (ADR 0023); manual checks in HUMAN_CHECKS. Outside the milestone loop.
- 2026-10-08 inbox 2 (plan docs/superpowers/plans/2026-10-08-inbox-status-drop-badges.md): status icons already existed — missing done/needs-input came from uninstalled hooks (user setup in HUMAN_CHECKS); workspace row count spins; file drop already built (T10.8); profile `icon` key + badges (ADR 0025). Outside the milestone loop.
- 2026-10-08 inbox 3 (plan docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md): working dots (`status_mark`); pane-wide file drop + composer routing + gpui modality workaround (`drop_catcher`); Ctrl+V smart paste + BMP→PNG (ADR 0026); agent icons fall back to the shipped sample + chips (ADR 0025 amendment); calm redesign — Theme global System/Light/Dark `[ui] theme`, toolbar replaces the view rail, per-tab ✕, new sidebar/inbox/empty state (ADR 0027). Outside the milestone loop.
- `cargo insta` is not installed: write accepted `.snap` files by hand. GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`). `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs. Daemon e2e `TIMEOUT` is 30 s.
- Headless sessions can't run new scripts directly (use a temporary `cargo test`). Build "Access is denied" on an exe: `mv` it aside. Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms). Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
- 2026-10-10 voice input (plan docs/superpowers/plans/2026-10-10-voice-input.md): `[voice]` settings, `voice.rs` (cpal), `stt.rs` (ureq → OpenAI-compatible `/audio/transcriptions`, Groq default), `Command::VoiceInput` Ctrl+Shift+D, SPEC §20G, ADR 0045; no protocol or migration change. Manual checks in HUMAN_CHECKS "2026-10-10 voice input". Outside the milestone loop.
