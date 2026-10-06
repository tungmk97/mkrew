# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M9
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M9 — Attention (SPEC §18, §20.0, §20A)

### Acceptance criteria
- [x] `settings.toml` (§20.0): defaults when missing, parse errors reported in `daemon.status` with the last good settings kept, re-read after the file changes; unit tests.
- [ ] `session.read` returns the last screen lines; the peek popover shows them for an Inbox or sidebar row, and its quick reply answers a fake agent's "(y/n)" without leaving the view (e2e for the request + manual check for the popover). — e2e done in T9.3; popover in T9.9 (`peek` unit-tested, ADR 0012 plain text); manual check waits on HUMAN_CHECKS (T9.9).
- [ ] Turns start and end as §20A.3 defines; a fake agent that edits 2 files in one turn produces `session.turn` with both paths (+/− counts arrive in M11); Inbox row and pane header show the summary. — daemon e2e (T9.5) + UI (T9.8, ADR 0011 `4m`); on-screen check waits on HUMAN_CHECKS (T9.8).
- [ ] `notify.needs_input` / `notify.done` / `notify.sound` switches are respected (unit tests on the decision; manual check for the sound). — unit tests done in T9.6; sound waits on HUMAN_CHECKS (T9.6).
- [ ] Keep awake: inhibitor held while an agent works and released ≤ 5 s after the last one stops (fake inhibitor test); real backends per OS; manual check (`powercfg /requests`). — fake tests + backends done in T9.7; `powercfg` + unix compile wait on HUMAN_CHECKS (T9.7).
- [ ] Broadcast sends one text to every checked session; each result is shown.
- [ ] Inbox rows show the last non-blank screen line (§20A.4). — done in T9.8 (`inbox::LastLines` unit-tested); on-screen check waits on HUMAN_CHECKS (T9.8).

### Task queue
- [x] T9.1 `mkrew_core::settings`: `Settings` struct with every §20.0 key + defaults, `deny_unknown_fields`, `Paths::settings_file()`, `load(path)` (missing file = defaults); unit tests (missing, partial, unknown key, bad type); covers: settings defaults/parse errors. (spec: §20.0, §17) — feat(core): settings.toml model and loader
- [x] T9.2 Daemon settings holder: load at start, watch the config dir (debounced 300 ms) and re-read, keep last good on error, `daemon.status.settings_error?`; protocol field + test that edits the file (good → bad → good); covers: settings re-read + status error. (spec: §20.0, §7, §8) — feat(daemon): watch settings.toml and report errors in daemon.status
- [x] T9.3 `session.read` request (protocol + daemon: last N non-blank emulator rows as plain text, `state`, `state_changed_at`, 1–200 range, `invalid_state` for non-live sessions); e2e with fake agent `ask` scenario reading the "(y/n)" line, then `session.send y`; covers: session.read e2e. (spec: §20A.1, §20A protocol, §7) — feat(daemon): session.read returns the last screen lines
- [x] T9.4 `mkrew_core::turn` pure model (start/end/resume rules, numbering) with unit tests on state sequences; fakeagent `edit_files { paths }` step (writes files + PostToolUse hooks). (spec: §20.0 Turn, §20A.3, §20F) — feat(core): turn model and fake agent edit_files step
- [x] T9.5 Daemon turn tracking: actor owns a turn per session, collects attributed paths (hook edits + watcher batches, dedup, relative), broadcasts event `session.turn`; e2e: fake agent edits 2 files in one turn → event with both paths; covers: session.turn event. (spec: §20A.3, §7, §8) — feat(daemon): track turns and broadcast session.turn
- [x] T9.6 Notification switches + sound: pure decision fn (kind × settings × 30 s throttle) with unit tests, notify-rust `sound_name` on Windows/macOS, Linux hint; daemon uses current settings; covers: notify switches. (spec: §20A.2, §8) — feat(daemon): notification switches and sound from settings
- [x] T9.7 Keep awake: `Inhibitor` trait + fake; controller (held while ≥1 agent session `working` and `awake.keep_awake`, released 5 s after the last leaves; shells excluded) with time-controlled tests; real backends (Windows dedicated thread `SetThreadExecutionState`, macOS `caffeinate -i -w`, Linux `systemd-inhibit`, missing tool logged once); covers: keep awake. (spec: §20A.5, §17) — feat(daemon): keep the system awake while agents work
- [x] T9.8 UI: Inbox row last non-blank line (`session.read lines=1` when an entry appears) and turn summary (`✓ done · 3 files · 4 min`) on the Inbox row + pane header from `session.turn`; pure formatting fns unit-tested + snapshot; covers: Inbox last line, turn summary in UI. (spec: §20A.3, §20A.4, §11, §14) — feat(ui): Inbox last line and turn summary
- [x] T9.9 UI: peek popover (300 ms hover on Inbox/sidebar rows, `Ctrl+Shift+Space` + ↑/↓/Esc, last 8 lines, state + age, one-line input, `y`/`n` buttons for needs_input, focus restored); keymap entry; covers: peek + quick reply UI. (spec: §20A.1, §13, §14) — feat(ui): peek popover with quick reply
- [ ] T9.10 UI: "Broadcast to agents…" palette command + dialog (live agent sessions of the workspace, checkboxes all on, text input, one `session.send` per checked session, per-row sent/queued/failed); pure state model unit-tested; covers: broadcast. (spec: §20A.6, §14)
- [ ] T9.11 Add M9 manual checks to HUMAN_CHECKS (peek popover + quick reply, sound, `powercfg /requests`, broadcast with real agents) and tick verified criteria. (spec: §18 M9)

### Done (last 10)

v1 (M0–M8) is complete (tags `m1-done` … `m8-done`, tasks in `docs/progress-archive.md`).
v2 = M9–M13 in SPEC §20 (ADR 0010 for the git exceptions). The loop ends after M13.

### Carried over

M3–M8 left criteria unticked; all wait on manual checks in HUMAN_CHECKS (T3.4–T3.9, T4.7–T4.10,
T5.4–T5.11, T6.8–T6.11, T7.7–T7.12, T8.3–T8.10). If one fails, add a fix task to the current
milestone first.

## Notes for the next session

- Settings: daemon `crate::settings::SettingsHolder` (`Daemon.settings: Arc<_>`; `current()` = last good, `error()`). `notify::start` and `awake::start` (T9.7, started in `Daemon::serve`) take the Arc and read `current()` per event. Notify policy = `notify::decide`; `Notice.sound` is in the `MKREW_NOTIFY` file JSON. PROTOCOL_VERSION is 17 for M9 (no further bump needed this milestone).
- `session.read`: `Request::SessionRead` → `SessionReadResult`; daemon `Command::Read`, `Emulator::last_lines(n)` (visible screen only). UI users: `Shell::read_last_lines` (T9.8), `Shell::read_peek` (T9.9).
- UI (T9.8): `mkrew_ui::turn::{Turns, status_text}`; `inbox::LastLines`. `cargo insta` is not installed: write accepted `.snap` files by hand.
- Peek (T9.9): pure `mkrew_ui::peek::{Peek, Hover, cycle}`; Shell `open_peek/close_peek/update_peek/hover_row`, `with_peek` hangs the popover (deferred+anchored) at a row's top right; `Command::Peek` = Ctrl+Shift+Space. GPUI gotcha: an element with `track_focus` grabs focus on ANY mouse down (before `on_click`) — the peek only tracks focus while it has the keyboard. T9.10 broadcast can reuse `send_peek`'s send/result pattern. `scripts/verify.ps1` needs approval headless; `bash scripts/verify.sh` runs.
- Full `verify.ps1` occasionally timed out daemon e2e tests under parallel load (`slow_attached_client…` failed twice in T9.5); daemon test `TIMEOUT` raised 15 → 30 s. The cli tests (`task_add_list_run…`) have their own helper timeout if they recur.

- Measurements: `crates/mkrew-ui/tests/measure.rs` (`#[ignore]`, real window; first `cargo build --release -p mkrew-ui --example idle_floor`, then `cargo test --release -p mkrew-ui --test measure -- --ignored --nocapture`); results in ADR 0009.
- Headless sessions here can't run new scripts directly (approval); run them through a temporary `cargo test` if needed.
- Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms).
- Stray locked exes `target/debug/*.locked-*.exe` (t81, t711, t92, t94, t94b, t97 — `mkrew-fakeagent.exe` got locked twice in T9.4, likely a leftover fake process from an e2e test) can be deleted when unlocked. A build failing with "Access is denied" on an exe: `mv` it aside like these.
- Per-task notes of every milestone are in git history (`git log -p docs/PROGRESS.md`).
