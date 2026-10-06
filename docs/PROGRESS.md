# Mkrew build progress

<!-- loop-state: machine-readable, keep these exact keys on their own lines -->
status: RUNNING
milestone: M8
<!-- /loop-state -->

Status values: `RUNNING` (loop continues) · `WAITING_HUMAN` (see docs/HUMAN_CHECKS.md, then set back to RUNNING) · `BLOCKED` (no task can progress) · `DONE`.
Task markers: `[ ]` todo · `[~]` in progress · `[x]` done · `[!]` blocked (see note).

## Current milestone: M8 — Polish & packaging (SPEC §18)

### Acceptance criteria
- [ ] Full command palette; keymap customization via `keymap.toml` with duplicate detection.
- [ ] Dev server per workspace: run `dev_command` in its own tab; optional auto-start after daemon start.
- [ ] Windows installer (MSI or similar) and tar.gz for Linux/macOS.
- [ ] README: install, configure agents, uninstall.
- [ ] Non-functional checks from §15 measured and recorded in `docs/decisions/` or the README.

### Task queue
- [x] T8.1 Dev server config: migration `0007` `project.dev_autostart`, protocol `ProjectAddParams.dev_command` + `ProjectUpdateParams.{dev_command (null clears), dev_autostart}` (bump `PROTOCOL_VERSION`), store, CLI `mk project add --dev` / `update --dev "…" | --no-dev`, `--dev-autostart on|off`; SPEC §5/§7/§9 rows updated; covers: dev server (config). (spec: §5, §7, §9) — feat(project): dev server command and auto-start settings
- [x] T8.2 Daemon dev server: `project.dev_start/dev_stop` spawns `dev_command` through the shell profile as a shell session titled "dev" in its own tab (one per project; start twice → the running one), auto-start for `dev_autostart` projects after daemon start; CLI `mk project dev <p> [--stop]`; E2E with a fake command; ADR if the protocol shape is new; covers: dev server (run in own tab, auto-start). (spec: §7, §8, §9, §10) — feat(daemon): dev server start/stop and auto-start
- [x] T8.3 UI dev server: workspace settings get dev command + auto-start fields; sidebar/tab-bar "Start dev server"/"Stop" action and running indicator; dev tab labelled; covers: dev server (UI). (spec: §10, §11, §14) — feat(ui): dev server settings, start/stop and indicator
- [x] T8.4 Command palette model: pure `palette.rs` — entries = every `keymap::Command` with title, shortcut label and availability in the current view/context, plus "Go to" workspace / session / task items (§13 quick open of those), fuzzy filter + ranking, key handling; `Command::CommandPalette` (ctrl-shift-p); unit tests; covers: full command palette. (spec: §13, §14) — feat(ui): command palette model
- [x] T8.5 Command palette UI: dialog (like quick open), runs the picked command through the shell's `run`; palette-only commands for actions without keys (Git fetch/push/discard, task "View changes", dev server start/stop, open keymap settings); covers: full command palette. (spec: §13, §14) — feat(ui): command palette dialog and palette-only actions
- [x] T8.6 Keymap settings: `Command::OpenKeymapSettings` (ctrl-,) creates `keymap.toml` with every command commented at its default when missing and opens it in Zed; palette lists keymap errors (duplicates, unknown, invalid) with the entry; tests for template round trip + duplicate detection; covers: keymap customization with duplicate detection. (spec: §13) — feat(ui): keymap settings template and palette keymap errors
- [x] T8.7 Packaging: release scripts `scripts/package.ps1` (Windows installer — MSI via cargo-wix/WiX or similar, look up via `researcher`; installs `mkrew`, `mk`, daemon, agent profiles) and `scripts/package.sh` (tar.gz for Linux/macOS), CI release job on tags (no publishing); test that the archive layout lists the expected files; covers: installer + tar.gz. (spec: §4, §17) — feat(release): MSI and tar.gz packaging scripts and release workflow
- [ ] T8.8 README: install (MSI / tar.gz / from source), configure agents (profiles TOML, `mk integration install claude|codex`, default agent, dev command), daily use pointers, uninstall (installer, `mk integration uninstall`, data/config dirs); covers: README. (spec: §1, §6, §9)
- [ ] T8.9 §15 measurements: `scripts/measure.*` for UI first frame, idle CPU, memory with 6 fake sessions, flood throughput, git status 10k repo (reuse existing perf tests); record results in ADR `0009-non-functional-measurements.md` + README table; unmeasurable-here items → HUMAN_CHECKS; covers: non-functional checks. (spec: §15, §16)
- [ ] T8.10 M8 manual checks + wrap-up: add M8 visual/manual checks (palette, keymap settings, dev server, installer on a clean Windows, tar.gz on Linux) to HUMAN_CHECKS, map unticked criteria, full verify; covers: all (manual parts). (spec: §14, §18)

### Done (last 10)

_M0–M7 tasks moved to `docs/progress-archive.md` (tags `m1-done` … `m7-done`; unticked criteria are in HUMAN_CHECKS)._

### Carried over

M7 left 9 of 11 criteria unticked; all wait on the T7.7–T7.11b and T7.12 manual checks in
HUMAN_CHECKS (Files view, quick open in a real 10,000-file repo, Git view header/diff/stage/
discard, base menu, commit box, real push/fetch, History, task "View changes", real-agent
diff-comment round trip, §14 design check). If one fails, add a fix task to M8 first. M3–M6's
unticked criteria (T3.4–T3.9, T4.7–T4.10, T5.4–T5.11, T6.8–T6.11 manual checks) still wait too.

## Notes for the next session

- T8.6: `keymap::{template(), ensure_file(path)}` (create_new; never touches an existing file), `Command::OpenKeymapSettings` (`ctrl-,`, global). `palette::keymap_errors(keymap, errors)` (entries → Run(OpenKeymapSettings)) prepended in main.rs `open_palette`; `Shell.{keymap_errors, keymap_file}`, `open_keymap_settings` (failure → `sidebar_error`). Edits apply after restart. Palette/keymap criterion waits for the T8.5 + T8.6 manual checks.
- Palette: `palette.rs` (T8.4 model, T8.5 `Extra` actions); main.rs `open_palette/run_palette_pick/render_palette`. Dev server: `dev_server.rs` (UI), daemon `server/dev.rs` (ADR 0008, `PROTOCOL_VERSION = 16`); criterion waits for the T8.2 + T8.3 manual checks.
- T8.7: `scripts/package.sh` (tar.gz: `bin/`, `agents/*.toml`, README.md; `--no-build --bin-dir --out`), `scripts/package.ps1` + `packaging/mkrew.wxs` (WiX 7, `-AcceptWixEula`), `.github/workflows/release.yml` (tags + manual, artifacts only). Test `crates/mkrew-cli/tests/packaging.rs` (tarball test unix-only; wxs is string-checked, MSI never built here). Installer criterion waits for the T8.7 manual checks + EULA question in HUMAN_CHECKS.
- T8.8 next (README): install sections should match the package layouts above; the daemon seeds profiles into `<config>/mkrew/agents` itself.
- Headless sessions here can't run new scripts directly (approval); run them through a temporary `cargo test` if needed.
- Gotcha: `match self.store().x() { … self.store() … }` deadlocks (the guard lives through the arms).
- Stray locked exes `target/debug/mkrew-daemon.locked-t81.exe`, `mkrew-fakeagent.locked-t711.exe` can be deleted when unlocked.
- Older per-task notes (M7, T8.1–T8.5 details) are in git history (`git log -p docs/PROGRESS.md`).
