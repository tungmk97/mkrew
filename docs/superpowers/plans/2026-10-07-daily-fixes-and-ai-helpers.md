# Daily Fixes and AI Helpers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix eight problems the user reported in daily use: Vietnamese typing, finding files, the `working` timer counting while agents sit idle, Ctrl+Shift+digit shortcuts, Git list placement, missing terminal icons. Also add two ✦ buttons: an AI commit message and an AI prompt rewrite.

**Architecture:** Most items are local fixes in `mkrew-ui` (keymap, theme/terminal font, Git/Files layout, a new `ime.rs` input handler). The `working` timer fix is in the daemon's session actor. The two AI buttons share one new daemon request, `ai.generate`. It runs the profile's new `oneshot` command (the user's own CLI agent, e.g. `claude -p`) with the prompt on stdin, and it broadcasts `ai.generated`. Mkrew still never calls an LLM API itself.

**Tech Stack:** Rust workspace, GPUI (`gpui-pre =0.3.7`, Windows backend `gpui-pre-windows 0.3.7`), alacritty_terminal 0.26, tokio, portable-pty, SQLite. Tests: `cargo test`, daemon end-to-end tests with `mkrew-fakeagent`.

**Spec:** `docs/SPEC.md` (sections cited per task: §1, §2, §6, §7, §12A, §12B, §13, §14, §20B.1). There is no separate design doc. This plan comes from the user's request of 2026-10-07 plus the investigation summarized under each task. Read the cited SPEC sections along with each task.

## Global Constraints

- Windows native is first-class: ConPTY, named pipes, `PATHEXT`, `dunce` paths (AGENTS.md, SPEC §2.4).
- "Mkrew never calls LLM APIs itself; it only runs CLI agents the user installed." (SPEC §1)
- "An agent is just a terminal command. No per-agent code in the core. Every agent difference lives in its TOML profile." (SPEC §2.1)
- "Never auto-commit in the user's projects." The AI buttons only fill a text box. The user still presses Commit or Send (SPEC §2.5).
- Every color, spacing, radius and font size comes from `crates/mkrew-ui/src/theme.rs` (SPEC §14).
- No `unwrap`/`expect` outside tests unless provably infallible, with a comment saying why.
- Use `thiserror` in libraries, `anyhow` with context in binaries, and `tracing` for logs.
- No test may call a real agent or the network. Use `mkrew-fakeagent` or plain OS commands.
- Deviating from the spec requires an ADR in `docs/decisions/` and a SPEC update in the same change.
- Bump `PROTOCOL_VERSION` on incompatible protocol changes: 23 → 24 (Task 7).
- Few dependencies: add no new crates. Task 7 only enables tokio's `process` feature.
- GPUI rule: set `.hover()` at most once per element (`button()`/`quiet_button()` already set it).
- **User preference: never commit.** Leave every change uncommitted. The last step of each task prints the Conventional Commit message the user may use; do not run `git commit`.
- Leave the existing uncommitted human edits alone (`.github/workflows/*`, `Cargo.lock`, `crates/mkrew-ui/Cargo.toml`, `docs/BUILD_VI.md`).
- Verify on Windows with Git Bash: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`. If rustc crashes on mkrew-ui test builds (STATUS_STACK_BUFFER_OVERRUN), run `cargo fmt --all -- --check`, then `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace -j 4`.
- If a build fails with "Access is denied" on an `.exe` (a running daemon or fake agent holds it), rename it to `*.locked-old.exe`. Never kill the user's sessions.
- `cargo insta` is not installed. Write accepted `.snap` files by hand.

## Review Focus

1. **Profiles created before this change have no `oneshot` key** (the user's `%APPDATA%\mkrew\agents\*.toml` are never overwritten). ✦ must report an error that names the key, an example and the file. It must not fail silently. Pinned by Task 7 `ai_generate_reports_what_is_missing`.
2. **The agent CLI hangs, exits non-zero or prints nothing.** The ✦ button must leave its busy state and show the reason. Pinned by Task 7 `run_stops_an_agent_that_does_not_answer` and `ai_generate_reports_what_is_missing`, and by the `generated(Err)` / `enhanced(Err)` tests in Tasks 8–9.
3. **Nothing staged but files modified or untracked, or nothing changed at all.** The commit message uses the unstaged diff plus the new-file list. With no changes it says "No changes to describe." Pinned by Task 7 `ai_generate_reports_what_is_missing` and `commit_prompt_falls_back_to_unstaged_and_new_files`.
4. **The user types, commits, sends or clicks ✦ again while a generation runs.** There is no second request and no silently lost text: input is ignored until the answer arrives. Pinned by Task 8 `a_generated_message_replaces_the_draft_and_typing_waits_for_it` and Task 9 `an_improved_prompt_replaces_only_its_draft`.
5. **IME composition that revises earlier characters** (Windows Vietnamese Telex: `v` → `vi` → `vie` → `viê` → `việ` → commit `việt`). The final text must be exactly `việt` in text boxes and in the bytes sent to the terminal. Pinned by Task 6 `composition_steps_end_as_the_committed_text_in_a_text_box` and `composition_steps_reach_the_terminal_as_the_committed_text`.

---

## File Structure

| File | Change | Responsibility |
| --- | --- | --- |
| `crates/mkrew-ui/src/keymap.rs` | modify | key bindings built with the platform keyboard mapper (Task 1) |
| `crates/mkrew-ui/src/main.rs` | modify | rebind on layout change (T1), font init (T3), Git column order (T4), Files search entry (T5), IME registration (T6), ✦ buttons + `ai.generated` handling (T8, T9) |
| `crates/mkrew-daemon/src/emulator.rs` | modify | `text_fingerprint()` (Task 2) |
| `crates/mkrew-daemon/src/session.rs` | modify | only visible screen changes count as activity (Task 2) |
| `crates/mkrew-ui/src/theme.rs` | modify | terminal font preference list, Nerd Font glyph fallbacks, `pick_family` (Task 3) |
| `crates/mkrew-ui/src/terminal/view.rs` | modify | glyph fallbacks (T3), terminal + find-bar IME handler (T6) |
| `crates/mkrew-ui/src/quick_open.rs` | modify | searches the Files view's worktree target (Task 5) |
| `crates/mkrew-ui/src/ime.rs` | create | platform text input → synthetic key presses (Task 6) |
| `crates/mkrew-ui/src/lib.rs` | modify | `pub mod ime;` (Task 6) |
| `crates/mkrew-core/src/profile.rs` (+ snapshot) | modify | `oneshot` profile key (Task 7) |
| `agents/{claude,codex,omp}.toml` | modify | default `oneshot` commands (Task 7) |
| `crates/mkrew-protocol/src/{lib,message}.rs` | modify | `ai.generate` / `ai.generated`, protocol v24 (Task 7) |
| `crates/mkrew-daemon/src/server.rs`, `server/ai.rs` (new), `Cargo.toml` | modify/create | runs the one-shot command, broadcasts the answer (Task 7) |
| `crates/mkrew-daemon/tests/daemon.rs` | modify | end-to-end `ai.generate` tests (Task 7) |
| `crates/mkrew-ui/src/git_actions.rs` | modify | `CommitBox` generation state (Task 8) |
| `crates/mkrew-ui/src/composer.rs` | modify | `Draft` rewrite state (Task 9) |
| `docs/SPEC.md`, `docs/decisions/0015-*.md`, `0016-*.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md` | modify/create | spec, ADRs, manual checks, notes |

---

### Task 1: Ctrl+Shift+digit and Alt+Shift+=/- shortcuts work on Windows

**Root cause (verified in source):** `Keymap::key_bindings` (keymap.rs:521-531) builds bindings with `gpui::KeyBinding::new`. That always uses `DummyKeyboardMapper` (gpui-pre binding.rs:33-44), so the binding stays `{ctrl+shift, "2"}`. gpui-pre-windows folds Shift into digits and OEM keys when it reports a key press (keyboard.rs:148-158 `get_keystroke_key`): Ctrl+Shift+2 arrives as `{ctrl, "@"}` on a US layout. `Keystroke::should_match` compares modifiers and key exactly, so Ctrl+Shift+1…5 (views) and Alt+Shift+= / Alt+Shift+- (splits) never fire. In a terminal the unmatched key even sends NUL to the agent. The fix is to build bindings with `KeyBinding::load(..., use_key_equivalents = true, cx.keyboard_mapper())`. Its `WindowsKeyboardMapper::map_key_equivalent` folds the binding the same way. The shifted char depends on the layout (US `@`, Vietnamese differs), so bind again on every layout change.

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs:520-531` (`key_bindings`), tests module (~line 820 `every_binding_becomes_a_gpui_key_binding`)
- Modify: `crates/mkrew-ui/src/main.rs:113-114`
- Modify: `docs/HUMAN_CHECKS.md` (the notes near lines 108-110 and 1128-1130 that say shifted digits are unverified)

**Interfaces:**
- Consumes: `gpui::KeyBinding::load(keystrokes: &str, action: Box<dyn Action>, context_predicate: Option<Rc<KeyBindingContextPredicate>>, use_key_equivalents: bool, action_input: Option<SharedString>, keyboard_mapper: &dyn PlatformKeyboardMapper) -> Result<KeyBinding, InvalidKeystrokeError>`; `App::keyboard_mapper(&self) -> &Rc<dyn PlatformKeyboardMapper>`; `App::on_keyboard_layout_change(FnMut(&mut App)) -> Subscription`; `App::clear_key_bindings()`.
- Produces: `Keymap::key_bindings(&self, mapper: &dyn gpui::PlatformKeyboardMapper) -> Vec<gpui::KeyBinding>` (signature change; the only caller is main.rs:114).

- [ ] **Step 1: Write the failing test** (append to `mod tests` in `crates/mkrew-ui/src/keymap.rs`)

```rust
    /// gpui-pre-windows reports Ctrl+Shift+2 as `ctrl-@` (Shift folded into the digit, layout
    /// dependent; keyboard.rs `get_keystroke_key`). A binding kept as `ctrl-shift-2` never
    /// matches, so every shifted digit / `=` / `-` binding must be folded the same way.
    #[cfg(windows)]
    #[test]
    fn shifted_digit_bindings_are_folded_like_windows_reports_them() {
        let platform = gpui_platform::current_platform(true);
        let mapper = platform.keyboard_mapper();
        let keymap = Keymap::defaults();
        let bindings = keymap.key_bindings(mapper.as_ref());
        let mut checked = 0;
        for ((command, keystroke), binding) in keymap.bindings().iter().zip(&bindings) {
            let folded = keystroke.modifiers.shift
                && (keystroke.key.chars().all(|c| c.is_ascii_digit())
                    || keystroke.key == "="
                    || keystroke.key == "-");
            if !folded {
                continue;
            }
            let inner = binding.keystrokes()[0].inner();
            assert!(!inner.modifiers.shift, "{command:?} still needs Shift: {inner:?}");
            assert_ne!(inner.key, keystroke.key, "{command:?} kept the unshifted key");
            checked += 1;
        }
        assert!(checked >= 7, "Ctrl+Shift+1..5 and Alt+Shift+=/- are shifted defaults");
    }
```

Also change the existing test `every_binding_becomes_a_gpui_key_binding`:

```rust
    #[test]
    fn every_binding_becomes_a_gpui_key_binding() {
        let (keymap, _) = Keymap::parse("split_down = \"ctrl-alt--\"\n");
        let bindings = keymap.key_bindings(&gpui::DummyKeyboardMapper);
        assert_eq!(bindings.len(), Command::all().len());
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p mkrew-ui --lib keymap`
Expected: FAIL to compile: `key_bindings` takes 0 arguments but 1 was supplied.

- [ ] **Step 3: Implement**

Replace `key_bindings` in `crates/mkrew-ui/src/keymap.rs`:

```rust
    /// GPUI key bindings for [`Keymap::bindings`], folded by `mapper` the way the platform
    /// reports typed keys (on Windows Shift folds into digits and punctuation: Ctrl+Shift+2
    /// arrives as `ctrl-@` on a US layout). Rebuild them when the keyboard layout changes.
    pub fn key_bindings(
        &self,
        mapper: &dyn gpui::PlatformKeyboardMapper,
    ) -> Vec<gpui::KeyBinding> {
        self.bindings
            .iter()
            .map(|(command, keystroke)| {
                // Cannot fail: the keystroke was parsed and is unparsed back; contexts are plain
                // identifiers or `COMMENTS_CONTEXT` (parsed in the tests).
                let context = command.context().map(|context| {
                    gpui::KeyBindingContextPredicate::parse(context)
                        .expect("keymap contexts are valid predicates")
                        .into()
                });
                gpui::KeyBinding::load(
                    &keystroke.unparse(),
                    Box::new(Run(*command)),
                    context,
                    true,
                    None,
                    mapper,
                )
                .expect("parsed keystrokes unparse to valid keystrokes")
            })
            .collect()
    }
```

In `crates/mkrew-ui/src/main.rs`, replace line 114 (`cx.bind_keys(keymap.key_bindings());`):

```rust
        let bindings = keymap.key_bindings(cx.keyboard_mapper().as_ref());
        cx.bind_keys(bindings);
        // Which char a shifted digit reports depends on the layout: bind again after a switch
        // (e.g. US ↔ Vietnamese), or Ctrl+Shift+1..5 stop working until a restart.
        let rebind = keymap.clone();
        cx.on_keyboard_layout_change(move |cx| {
            let bindings = rebind.key_bindings(cx.keyboard_mapper().as_ref());
            cx.clear_key_bindings();
            cx.bind_keys(bindings);
        })
        .detach();
```

(`Keymap` derives `Clone`, keymap.rs:409. `main.rs` binds keys only here. It never rebinds at runtime.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib keymap`
Expected: PASS (all keymap tests, including `spec_defaults` and the new test).

- [ ] **Step 5: Update HUMAN_CHECKS**

In `docs/HUMAN_CHECKS.md`, change the notes that say "whether Windows reports shifted digits that way is part of the T4.10 manual board check" (~1128-1130) and the Alt+Shift+= note (~108-110) to say: fixed by mapping bindings through the platform keyboard mapper; check Ctrl+Shift+1…5 and Alt+Shift+=/- with a US and a Vietnamese layout, including after switching the layout while the app runs, and with a terminal focused (no NUL must reach the agent).

- [ ] **Step 6: Record (no commit)**

Suggested message for the user: `fix(ui): map shifted-digit shortcuts like Windows reports them`

---

### Task 2: `working` only while the visible screen changes

**Root cause (verified, with live evidence):** `Actor::on_output` (daemon session.rs:1050-1074) calls `StateMachine::on_output` for every PTY batch. That call sets `last_output = now` and moves the state to `Working` (core state.rs:66-82). `tick` only reaches `Idle` after `idle_after_ms` (3 s) with no bytes at all. omp/claude TUIs keep sending invisible bytes at the idle prompt: cursor show/hide, OSC title/progress, sync markers, identical repaints. So the session stays `working` forever. The user's DB had 3 omp sessions `working` (source `screen`) for 882 s, all sitting at a static prompt. The UI counter (status.rs:48-51) is correct: it counts from `state_changed_at`. Fix: a batch counts as activity only if the visible text changed. The state machine itself stays as it is. The single caller skips it when nothing changed.

**Files:**
- Modify: `crates/mkrew-daemon/src/emulator.rs` (new method next to `lines_to_cursor`, line ~69; test next to `last_lines_skip_blank_rows_and_keep_order`, line ~249)
- Modify: `crates/mkrew-daemon/src/session.rs` (`struct Actor` ~880-912, its constructor, `Actor::on_output` ~1050-1074)
- Modify: `docs/SPEC.md` §6 "Session states" table (rows `working`, `idle`)
- Create: `docs/decisions/0015-visible-change-is-activity.md`

**Interfaces:**
- Consumes: `Emulator` grid access already used in `lines_to_cursor` (`self.term.grid()`, `screen_lines()`, `columns()`, `Line`, `Column`).
- Produces: `Emulator::text_fingerprint(&self) -> u64`; `Actor.screen_print: Option<u64>`.

- [ ] **Step 1: Write the failing test** (in `crates/mkrew-daemon/src/emulator.rs` `mod tests`)

```rust
    /// TUIs at an idle prompt keep sending bytes that change nothing visible (cursor show/hide,
    /// OSC title and progress, synchronized-update markers, identical repaints): those must
    /// not look like work (SPEC §6, ADR 0015).
    #[test]
    fn fingerprint_ignores_invisible_output_and_sees_new_text() {
        let mut emu = Emulator::new(20, 4);
        emu.advance(b"hello");
        let print = emu.text_fingerprint();
        emu.advance(b"\x1b[?25l\x1b[?25h\x1b]0;title\x07\x1b]9;4;3\x07");
        emu.advance(b"\x1b[?2026h\x1b[H\x1b[2Jhello\x1b[?2026l");
        emu.advance(b"\x1b[31m\x1b[H\x1b[1Khello\x1b[0m");
        assert_eq!(emu.text_fingerprint(), print, "nothing visible changed");
        emu.advance(b"!");
        assert_ne!(emu.text_fingerprint(), print);
    }
```

(The third `advance` repaints the same text in red. Colors are not text, so the blink/fade timers of omp's status line do not count.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p mkrew-daemon --lib emulator::tests::fingerprint`
Expected: FAIL to compile: no method `text_fingerprint`.

- [ ] **Step 3: Implement the fingerprint** (in `impl Emulator`, after `lines_to_cursor`)

```rust
    /// Hash of the visible screen's characters (no colors, no cursor): unchanged when output
    /// only moved the cursor, set a title or repainted the same text. The whole screen, not
    /// [`Self::lines_to_cursor`]: spinners and status rows can sit below the cursor.
    pub fn text_fingerprint(&self) -> u64 {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let grid = self.term.grid();
        let mut hasher = DefaultHasher::new();
        for row in 0..self.term.screen_lines() {
            let line = &grid[Line(i32::try_from(row).unwrap_or(i32::MAX))];
            for column in 0..self.term.columns() {
                line[Column(column)].c.hash(&mut hasher);
            }
        }
        hasher.finish()
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p mkrew-daemon --lib emulator`
Expected: PASS.

- [ ] **Step 5: Gate the state machine in `Actor::on_output`**

Add a field to `struct Actor` (next to `machine: StateMachine`, session.rs ~895):

```rust
    /// [`Emulator::text_fingerprint`] after the last output; output that leaves it unchanged
    /// is not activity (ADR 0015).
    screen_print: Option<u64>,
```

Initialise it with `screen_print: None,` where the `Actor { … }` literal is built (search session.rs for `machine,` / `machine:` inside the `Actor {` literal).

In `Actor::on_output`, replace:

```rust
        let lines = self.screen_lines();
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        if let Some(state) = self.machine.on_output(Instant::now(), &lines) {
            self.set_state(state, StateSource::Screen);
        }
```

with:

```rust
        // Only visible changes are work: idle TUIs keep sending cursor/title/progress bytes,
        // which would otherwise hold the session in `working` forever (ADR 0015).
        let print = self.emulator().text_fingerprint();
        if self.screen_print.replace(print) != Some(print) {
            let lines = self.screen_lines();
            let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
            if let Some(state) = self.machine.on_output(Instant::now(), &lines) {
                self.set_state(state, StateSource::Screen);
            }
        }
```

Leave the `self.queue.on_output()` / `deliver()` lines after it unchanged: the send queue still sees every byte, so deliveries are not delayed. The first batch always counts (`None != Some(_)`), so hook-only profiles still leave `starting`.

- [ ] **Step 6: Run the daemon tests**

Run: `cargo test -p mkrew-daemon`
Expected: PASS, including `fake_agents_go_through_every_state_transition` in tests/daemon.rs (its fake omp spinner frames change the text).

- [ ] **Step 7: SPEC + ADR**

In `docs/SPEC.md` §6 "Session states", change the rows:

```markdown
| working | the visible screen text changed in the last `idle_after_ms`, or hook says busy | hook / screen |
| idle | the visible screen text unchanged for `idle_after_ms` (cursor, title, color or identical-redraw output does not count), no pattern match | screen |
```

Create `docs/decisions/0015-visible-change-is-activity.md` from `docs/decisions/0000-template.md`:
- Date 2026-10-07, Status accepted, Milestone: post-M13 fixes.
- Context: omp/claude at an idle prompt send invisible bytes at least every 3 s. Sessions stayed `working` for 15+ minutes (user DB evidence above). The timer counted from the first output.
- Options: 1) count only output that changes the screen text (hash of the visible cells), cheap, deterministic, no profile change; 2) alacritty damage tracking, which also flags cursor/mode changes; 3) per-agent ignore patterns, which break §2.1 (no per-agent code) and need constant tuning.
- Decision: option 1, a gate in `Actor::on_output`.
- Consequences: typing at a prompt echoes, so a session shows `working` for ≤ 3 s. A frozen screen during a long silent tool call shows `idle`, as it already did with true silence. A status-line clock that ticks once a minute causes a 3 s blip.

- [ ] **Step 8: Record (no commit)**

Suggested message: `fix(daemon): count only visible screen changes as agent activity`

---

### Task 3: Terminal icons render (Nerd Font glyph fallbacks, installed family)

**Root cause (verified in source):** `terminal_font()` (view.rs:802-808) gives the terminal font one glyph fallback, "Consolas". Nerd Font icons used by omp are private-use code points (U+E000–F8FF, U+F0000+). DirectWrite's system fallback has no font for them, so they draw as empty boxes (fallback order: listed families first, then system fallback, gpui-pre-windows direct_write.rs:388-443; missing listed families are skipped). This machine has `MesloLG* Nerd Font Mono` installed per user. Also, when the primary family is not visible to DirectWrite, gpui's `resolve_font` (text_system.rs:370-388) falls through to its own stack (Segoe UI, proportional) and drops the fallbacks. So the family is picked once from `all_font_names()`.

**Files:**
- Modify: `crates/mkrew-ui/src/theme.rs:202-214` (constants), fields ~254-257, `dark()`/`light()` field init ~378-379, tests module
- Modify: `crates/mkrew-ui/src/terminal/view.rs:802-808` (`terminal_font`)
- Modify: `crates/mkrew-ui/src/main.rs` (one line before `cx.open_window`, ~line 126)
- Modify: `docs/SPEC.md` §14 terminal font line (~847)

**Interfaces:**
- Consumes: `gpui::TextSystem::all_font_names(&self) -> Vec<String>` (pub, text_system.rs:287); `App::text_system(&self) -> &Arc<TextSystem>`; `gpui::FontFallbacks::from_fonts(Vec<String>)`.
- Produces: `theme::TERMINAL_FONTS: &[&str]`, `theme::TERMINAL_GLYPH_FALLBACKS: &[&str]`, `theme::pick_family(preferred: &[&'static str], installed: &[String]) -> &'static str`, `theme::init_terminal_font(installed: &[String])`. Field `Theme::terminal_font_fallback` is removed.

- [ ] **Step 1: Write the failing test** (in `crates/mkrew-ui/src/theme.rs` `mod tests`)

```rust
    #[test]
    fn the_first_installed_terminal_family_wins() {
        let installed = |names: &[&str]| names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>();
        let preferred = &["Cascadia Code", "Cascadia Mono", "Consolas"];
        assert_eq!(pick_family(preferred, &installed(&["Arial", "Consolas"])), "Consolas");
        assert_eq!(
            pick_family(preferred, &installed(&["Consolas", "Cascadia Mono"])),
            "Cascadia Mono",
            "preference order, not installation order"
        );
        assert_eq!(pick_family(preferred, &installed(&[])), "Cascadia Code");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p mkrew-ui --lib theme::tests::the_first_installed`
Expected: FAIL to compile: cannot find function `pick_family`.

- [ ] **Step 3: Implement in theme.rs**

Replace the six `TERMINAL_FONT` / `TERMINAL_FONT_FALLBACK` constants (lines 202-214) with:

```rust
/// Terminal font families by preference; the first one installed is used (SPEC §14).
#[cfg(windows)]
pub const TERMINAL_FONTS: &[&str] = &["Cascadia Code", "Cascadia Mono", "Consolas"];
#[cfg(target_os = "macos")]
pub const TERMINAL_FONTS: &[&str] = &["SF Mono", "Menlo"];
#[cfg(not(any(windows, target_os = "macos")))]
pub const TERMINAL_FONTS: &[&str] = &["JetBrains Mono", "DejaVu Sans Mono"];

/// Per-glyph fallbacks of the terminal font, tried before the system fallback. Nerd Font
/// icons (private-use code points, used by omp and shell prompts) are in no system font.
/// Missing families are skipped; "Mono" variants keep icons one cell wide.
pub const TERMINAL_GLYPH_FALLBACKS: &[&str] = &[
    "Symbols Nerd Font Mono",
    "CaskaydiaCove Nerd Font Mono",
    "CaskaydiaMono Nerd Font Mono",
    "JetBrainsMono Nerd Font Mono",
    "MesloLGS Nerd Font Mono",
    "MesloLGM Nerd Font Mono",
    "Hack Nerd Font Mono",
    "FiraCode Nerd Font Mono",
];

/// The terminal family picked from the installed fonts at startup.
static TERMINAL_FONT: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// The first of `preferred` that is `installed`, else the first of `preferred`.
pub fn pick_family(preferred: &[&'static str], installed: &[String]) -> &'static str {
    preferred
        .iter()
        .copied()
        .find(|family| installed.iter().any(|name| name == family))
        .or_else(|| preferred.first().copied())
        .unwrap_or_default()
}

/// Picks the terminal family once; call before the first window opens. A family DirectWrite
/// / CoreText cannot find would silently become a proportional UI font.
pub fn init_terminal_font(installed: &[String]) {
    let _ = TERMINAL_FONT.set(pick_family(TERMINAL_FONTS, installed));
}

fn terminal_font_family() -> &'static str {
    TERMINAL_FONT
        .get()
        .copied()
        .unwrap_or_else(|| pick_family(TERMINAL_FONTS, &[]))
}
```

In `struct Theme`, delete the field and its doc comment:

```rust
    /// Used if `terminal_font_family` is not installed.
    pub terminal_font_fallback: &'static str,
```

In the shared field init (~378-379), replace `terminal_font_family: TERMINAL_FONT,` with `terminal_font_family: terminal_font_family(),` and delete `terminal_font_fallback: TERMINAL_FONT_FALLBACK,`. If `dark()` and `light()` each set these fields, change both.

- [ ] **Step 4: Use the fallbacks in the terminal** (`crates/mkrew-ui/src/terminal/view.rs`)

Replace `terminal_font`:

```rust
/// Glyph fallbacks built once: the terminal paints every frame.
static GLYPH_FALLBACKS: std::sync::LazyLock<gpui::FontFallbacks> = std::sync::LazyLock::new(|| {
    gpui::FontFallbacks::from_fonts(
        theme::TERMINAL_GLYPH_FALLBACKS
            .iter()
            .map(|family| (*family).to_owned())
            .collect(),
    )
});

fn terminal_font(theme: &Theme) -> Font {
    let mut base = font(theme.terminal_font_family);
    base.fallbacks = Some(GLYPH_FALLBACKS.clone());
    base
}
```

(Add `use crate::theme;` if `theme::` is not already in scope in view.rs. `FontFallbacks` is an `Arc` wrapper, so `clone()` is cheap. If the compiler reports that `FontFallbacks` is not `Sync`, use `thread_local!` with the same body instead.)

In `crates/mkrew-ui/src/main.rs`, right after the key-binding lines from Task 1 and before `let options = WindowOptions {`, add:

```rust
        theme::init_terminal_font(&cx.text_system().all_font_names());
```

(Use the import path main.rs already uses for `theme`: `mkrew_ui::theme`.)

- [ ] **Step 5: Run tests and the theme snapshot tests**

Run: `cargo test -p mkrew-ui --lib theme terminal`
Expected: PASS. `Theme::for_appearance(true) == Theme::dark()` still holds. If a snapshot contains `terminal_font_fallback`, edit the `.snap` by hand to drop that line.

- [ ] **Step 6: SPEC**

In `docs/SPEC.md` §14, replace the "Terminal font defaults" bullet with:

```markdown
- Terminal font defaults: Cascadia Code → Cascadia Mono → Consolas (Windows), SF Mono → Menlo
  (macOS), JetBrains Mono → DejaVu Sans Mono (Linux): the first installed one, 13 px. Icons of
  Nerd Font prompts/TUIs (private-use code points) come from an installed Nerd Font "Mono"
  family (`theme::TERMINAL_GLYPH_FALLBACKS`), then the system fallback. ANSI palette
  harmonized with the theme.
```

(The old "configurable" is dropped because no setting ever existed. A font setting is backlog.)

- [ ] **Step 7: Record (no commit)**

Suggested message: `fix(ui): render Nerd Font icons in terminals and pick an installed terminal font`

---

### Task 4: Git view changed-file column on the right

**Current state (verified):** The "Git sidebar" is the Git view's 300 px column: header, mode switch, changed-file list and commit box. It is built in `render_git` (main.rs:7730-7932). Its final flex row adds the column first (left) and `render_git_diff` second. The diff panel has `.mr(px(space::S))` (main.rs ~8469). No test covers this layout.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs:7728-7729` (doc comment), `7913-7931` (row), `~8469` (diff margin)
- Modify: `crates/mkrew-ui/src/theme.rs:174` (doc comment of `GIT_FILE_LIST_WIDTH`)
- Modify: `docs/SPEC.md` §12B "Changed-file list" bullet (~721)

**Interfaces:** none (layout only).

- [ ] **Step 1: Swap the children** in `render_git`'s final row:

```rust
        div()
            .flex_1()
            .min_h_0()
            .flex()
            // The list column stays at the right edge even when no diff is shown.
            .justify_end()
            .font_family(theme::UI_FONT)
            .when(any_files, |el| {
                el.child(self.render_git_diff(project_id, view, theme, window, cx))
            })
            .child(
                // The commit box sits outside the `GIT_CONTEXT` column: typing never stages.
                div()
                    .flex_none()
                    .w(px(theme::GIT_FILE_LIST_WIDTH))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(column.child(list))
                    .when_some(status.filter(|_| view.commit_box_shown()), |el, status| {
                        el.child(self.git_commit_box(project_id, view, status, theme, window, cx))
                    }),
            )
            .into_any_element()
```

- [ ] **Step 2: Move the diff panel's gap to its left side.** In `render_git_diff`, change `.mr(px(space::S))` to `.ml(px(space::S))`. Keep `.mb(px(space::S))`.

- [ ] **Step 3: Fix the comments.** main.rs:7728-7729 becomes `/// The workspace's changes (SPEC §12B): the selected file's unified diff, and on its right the branch header and mode toggle over the changed-file list and the commit box.` theme.rs:174 becomes `/// Width of the Git view's header and changed-file list (right of the diff).`

- [ ] **Step 4: SPEC.** §12B: `- **Changed-file list** (right of the diff, with the header and commit box) with +/− counts; selecting one shows its diff.`

- [ ] **Step 5: Build**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings. (Manual check for the user is in Task 10.)

- [ ] **Step 6: Record (no commit)**

Suggested message: `feat(ui): show the Git changed-file list on the right`

---

### Task 5: Find files from the Files view; quick open follows the Files target

**Current state (verified):** Go-to-file already exists: `Command::QuickOpen`, **Ctrl+Shift+O**, global, scoped to the active workspace, ranked by the daemon (`fs.find`: `ignore` + `nucleo-matcher`). The palette lists it as "Files: Quick open file". The Files view itself has no search entry or hint, so the user never finds it, and SPEC §13 mislabels the shortcut as "Quick open workspace / session / task". Real bug: `QuickOpen::search` hardcodes `worktree_task_id: None` (quick_open.rs:87). When the Files view shows a task worktree, quick open searches the main folder and reveals the wrong copy. Decision: no second inline finder. The tree loads lazily per folder, so filtering it cannot find files in folders that were never opened. A field-look "Search files…" entry at the top of the tree opens the same quick open. Ctrl+Shift+O stays the shortcut.

**Files:**
- Modify: `crates/mkrew-ui/src/quick_open.rs:60-95` (`QuickOpen` field, `new`, `search`)
- Modify: `crates/mkrew-ui/src/main.rs` `open_quick_open` (~2630), the composer `@` picker (~2655, `QuickOpen::new` call), `open_quick_pick` (~2769), `render_files` header (~7466-7490), empty viewer hint (~7600)
- Modify: `docs/SPEC.md` §12A (~694) and §13 table row (~756)

**Interfaces:**
- Produces: `QuickOpen::new(project_id: Ulid, worktree_task_id: Option<Ulid>) -> QuickOpen`; field `pub worktree_task_id: Option<Ulid>`.
- Consumes: `FilesView.worktree_task_id: Option<Ulid>` (`self.files.get(&project_id)`); `Shell::files_at(project_id, target)` (main.rs ~1172); helpers `text_field(id, text, placeholder, focused, theme)`, `tooltip(theme, text, keys)`, `self.keymap.label(Command::QuickOpen)`.

No new permanent test: the scope fix is forwarding, and the repo's rules forbid wiring tests. The daemon side of `fs.find` with a worktree is already covered (tests/daemon.rs ~4409). Proof comes from the build plus the GUI check in Task 10.

- [ ] **Step 1: QuickOpen carries the target.** In `crates/mkrew-ui/src/quick_open.rs`, add the field after `project_id`:

```rust
    /// The task worktree the Files view shows (`None` = the project folder): searched and
    /// revealed there.
    pub worktree_task_id: Option<Ulid>,
```

Change `new` to `pub fn new(project_id: Ulid, worktree_task_id: Option<Ulid>) -> Self` and set the field. In `search`, use `worktree_task_id: self.worktree_task_id,`. Update the tests in quick_open.rs that call `QuickOpen::new(x)` to `QuickOpen::new(x, None)`.

- [ ] **Step 2: Callers.** In `open_quick_open`:

```rust
        let target = self.files.get(&project_id).and_then(|view| view.worktree_task_id);
        self.quick_open = Some(QuickOpen::new(project_id, target));
```

In the composer `@` picker, use `QuickOpen::new(project_id, None)`: the composer types project-relative paths, so its behavior does not change. In `open_quick_pick`, reveal the file only when the Files view still shows the searched target. Use `self.files_at(open.project_id, open.worktree_task_id)` instead of `self.files_view(project_id)` for the `reveal` call, and return early when it is `None`. Read `files_at` first to match its exact return type.

- [ ] **Step 3: Search entry in the Files header.** In `render_files`, replace the muted `"Files"` label child of `header` with:

```rust
            .child(
                div()
                    .id("files-search")
                    .flex_1()
                    .min_w_0()
                    .cursor_pointer()
                    .child(text_field("files-search-field", "", "Search files…", false, theme))
                    .tooltip(tooltip(
                        theme,
                        "Find a file of this workspace",
                        self.keymap.label(Command::QuickOpen),
                    ))
                    .on_click(cx.listener(|shell, _, window, cx| shell.open_quick_open(window, cx))),
            )
```

(Only the outer div sets the tooltip. Check `text_field` at main.rs:12735: if it already sets `.hover(…)`, do not add one.)

- [ ] **Step 4: Hint in the empty viewer** (~7600). Replace the static hint with:

```rust
                    Some(format!(
                        "Use ↑ ↓ to move, → to open a folder, Enter to open a file · {} to find a file.",
                        self.keymap.label(Command::QuickOpen)
                    )),
```

If the label is empty (the user unbound it), the sentence still reads fine, because the header entry still works. Do not special-case it.

- [ ] **Step 5: SPEC.** §12A quick-open bullet: `- **Quick open** (`Ctrl+Shift+O`, or the "Search files…" entry at the top of the tree) fuzzy-finds files of the current workspace, in the folder the Files view shows (project or task worktree) (`ignore` to walk, `nucleo-matcher` to rank).` §13 row: `| Global | Quick open a file of the active workspace | Ctrl+Shift+O |`.

- [ ] **Step 6: Build and run the quick-open tests**

Run: `cargo test -p mkrew-ui --lib quick_open` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS, no warnings.

- [ ] **Step 7: Record (no commit)**

Suggested message: `feat(ui): search files from the Files view; quick open follows its worktree`

> Skipped: a Ctrl+P alias. SPEC §13 keeps plain Ctrl+letter free for the agents, and Ctrl+Shift+O already does this job. Add a Files-scoped `GoToFile` command only if the user asks.

---

### Task 6: Vietnamese (IME and Unikey) input in every text field and the terminal

**Root cause (verified in source):** mkrew-ui never registers a GPUI input handler. There is no `InputHandler`/`handle_input` anywhere. Every field reads only `KeyDownEvent.keystroke.key_char`. On Windows, gpui-pre-windows sends text that arrives as WM_CHAR to the input handler only: Unikey/EVKey send accented chars via SendInput Unicode = `VK_PACKET` (events.rs:1519 `VK_PACKET => None`, no KeyDown), and `handle_char_msg` (events.rs:477-484) drops the char when there is no handler. WM_IME_COMPOSITION (Windows Vietnamese Telex) is also routed only to the handler. With no handler, `update_ime_enabled` even detaches the IME from the window on every paint (events.rs:699-727). Result: Unikey's real Backspace keydowns delete chars, but the replacement `ệ` never arrives, so text gets eaten. Windows Telex types raw `vieejt`. Known gap: HUMAN_CHECKS.md ~809-811, ~1139 and ADR 0001 ("must add an InputHandler for IME/dead keys").

**Design:** One small generic `InputHandler` (`ime::Typing<V>`). It turns committed and composing text into synthetic key presses for the field's existing key handler: chars become `Insert`, and Backspace erases the previous composing text. All field models and key mappers stay as they are, since they append at the end and delete with Backspace. The composing text is shown inline by typing it and erasing it when it changes. `Window::handle_input` only registers for the focused handle and must be called in the paint phase (window.rs:5205-5219), so each surface registers through a `canvas` paint callback.

**Files:**
- Create: `crates/mkrew-ui/src/ime.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod ime;` in alphabetical order, and mention "platform text input" in the crate doc list)
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (`TerminalPane` field, `TerminalElement::paint` ~703-787, `render_find_bar` ~460)
- Modify: `crates/mkrew-ui/src/main.rs` (`Shell` field + `typing` helper + call sites)
- Modify: `docs/HUMAN_CHECKS.md` (~809-811, ~1010, ~1021, ~1139), `docs/decisions/0001-ui-framework.md` (~217: say the InputHandler now exists)

**Interfaces:**
- Consumes: `gpui::InputHandler` trait (platform.rs:1923-2060; required: `selected_text_range`, `marked_text_range`, `text_for_range`, `replace_text_in_range`, `replace_and_mark_text_in_range`, `unmark_text`, `bounds_for_range`, `character_index_for_point`); `Window::handle_input(&mut self, focus: &FocusHandle, handler: impl InputHandler, cx: &App)`; `gpui::canvas(prepaint, paint)`; `crate::task_dialog::dialog_key`; `crate::terminal::input::{key_to_bytes, InputMode}`.
- Produces: `ime::keys(marked: &Cell<usize>, text: &str, mark: bool) -> Vec<gpui::Keystroke>`; `ime::OnKey<V>`; `ime::Typing<V> { view, on_key, marked, caret }`; `Shell::typing(&self, focus: &FocusHandle, on_key: ime::OnKey<Shell>, cx: &Context<'_, Shell>) -> impl IntoElement`.

- [ ] **Step 1: Write the failing tests** (create `crates/mkrew-ui/src/ime.rs` with only the tests module and `pub mod ime;` in lib.rs)

```rust
#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::keys;
    use crate::task_dialog::dialog_key;
    use crate::terminal::input::{InputMode, key_to_bytes};
    use crate::text_box::TextBox;

    /// Windows Vietnamese Telex composing "việt": each step replaces the composing text,
    /// revising chars typed before; the last step commits.
    fn telex(marked: &Cell<usize>) -> Vec<gpui::Keystroke> {
        let mut presses = Vec::new();
        for composing in ["v", "vi", "vie", "viê", "việ"] {
            presses.extend(keys(marked, composing, true));
        }
        presses.extend(keys(marked, "việt", false));
        presses
    }

    #[test]
    fn composition_steps_end_as_the_committed_text_in_a_text_box() {
        let marked = Cell::new(0);
        let mut text = TextBox::default();
        text.insert("tôi ");
        for keystroke in telex(&marked) {
            if let Some(key) = dialog_key(&keystroke) {
                text.key(key);
            }
        }
        assert_eq!(text.text(), "tôi việt", "earlier text is never erased");
        assert_eq!(marked.get(), 0);
    }

    #[test]
    fn composition_steps_reach_the_terminal_as_the_committed_text() {
        let marked = Cell::new(0);
        // What a shell line editor does with the bytes: DEL (0x7f) erases one char.
        let mut line = String::new();
        for keystroke in telex(&marked) {
            let bytes = key_to_bytes(&keystroke, InputMode::default()).expect("typed key");
            if bytes == [0x7f] {
                line.pop();
            } else {
                line.push_str(std::str::from_utf8(&bytes).expect("utf-8"));
            }
        }
        assert_eq!(line, "việt");
    }

    #[test]
    fn committed_text_types_each_char_and_drops_control_chars() {
        let marked = Cell::new(0);
        let typed: Vec<String> = keys(&marked, "ệ\u{8}a\r", false)
            .into_iter()
            .map(|k| k.key_char.unwrap_or(k.key))
            .collect();
        assert_eq!(typed, ["ệ", "a"]);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui --lib ime`
Expected: FAIL to compile: unresolved import `super::keys`.

- [ ] **Step 3: Implement `ime.rs`** (above the tests module)

```rust
//! Platform text input for the hand-rolled fields and the terminal (SPEC §10, §13): text from
//! an IME (Windows Vietnamese Telex, CJK) or sent as Unicode input (Unikey, EVKey) arrives here
//! instead of as key presses. It becomes key presses for the field's own key handler: chars
//! are typed, and composing text is typed in place and erased with Backspace when it changes.

use std::cell::Cell;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    App, Bounds, Context, Entity, InputHandler, KeyDownEvent, Keystroke, Pixels, Point,
    UTF16Selection, Window,
};

/// Key presses that turn the `marked` composing chars into `text`; `text` is composing again
/// (and counted in `marked`) when `mark`. Control chars are dropped: Enter, Tab and Backspace
/// come as real key presses.
pub fn keys(marked: &Cell<usize>, text: &str, mark: bool) -> Vec<Keystroke> {
    let backspace = Keystroke {
        key: "backspace".into(),
        ..Keystroke::default()
    };
    let mut presses: Vec<Keystroke> = (0..marked.replace(0)).map(|_| backspace.clone()).collect();
    let mut typed = 0;
    for c in text.chars().filter(|c| !c.is_control()) {
        let s = c.to_string();
        presses.push(Keystroke {
            key: s.clone(),
            key_char: Some(s),
            ..Keystroke::default()
        });
        typed += 1;
    }
    if mark {
        marked.set(typed);
    }
    presses
}

/// A field's key handler, e.g. `Shell::composer_key_down`.
pub type OnKey<V> = fn(&mut V, &KeyDownEvent, &mut Window, &mut Context<'_, V>);

/// The input handler of one focused field: feeds `view`'s `on_key`.
pub struct Typing<V: 'static> {
    pub view: Entity<V>,
    pub on_key: OnKey<V>,
    /// Chars of the composing text typed into the field so far.
    pub marked: Rc<Cell<usize>>,
    /// Where the IME shows its candidate window.
    pub caret: Bounds<Pixels>,
}

impl<V: 'static> Typing<V> {
    fn send(&self, text: &str, mark: bool, window: &mut Window, cx: &mut App) {
        for keystroke in keys(&self.marked, text, mark) {
            let event = KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            };
            let on_key = self.on_key;
            self.view.update(cx, |view, cx| on_key(view, &event, window, cx));
        }
    }
}

// The fields only append and erase at the end, so ranges are ignored and the "document" is
// just the composing text: selection = end of it.
impl<V: 'static> InputHandler for Typing<V> {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<UTF16Selection> {
        let end = self.marked.get();
        Some(UTF16Selection {
            range: end..end,
            reversed: false,
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Range<usize>> {
        let len = self.marked.get();
        (len > 0).then_some(0..len)
    }

    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<String> {
        None
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.send(text, false, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.send(text, true, window, cx);
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut App) {
        self.marked.set(0);
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        Some(self.caret)
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Option<usize> {
        None
    }
}
```

(`view.update` inside the handler is safe: gpui calls input handlers outside any entity update, the same way `ElementInputHandler` does, gpui input.rs:174-184. Check that the `Keystroke` field names `key`/`key_char`/`modifiers` and `Keystroke: Default` match gpui-pre keystroke.rs:17.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui --lib ime`
Expected: PASS (3 tests).

- [ ] **Step 5: Register the terminal's handler** (`crates/mkrew-ui/src/terminal/view.rs`)

Add to `struct TerminalPane`:

```rust
    /// Composing IME chars typed into the PTY so far (see `crate::ime`).
    ime_marked: Rc<Cell<usize>>,
```

Initialise it with `ime_marked: Rc::default(),` in the constructor. At the end of `TerminalElement::paint` (after the row loop, before `let _ = self.painted.try_send(());`), register the handler with the cursor cell as caret:

```rust
        let pane = self.pane.read(cx);
        let (focus, marked) = (pane.focus_handle.clone(), pane.ime_marked.clone());
        let caret = Bounds::new(
            point(
                origin.x + cell_width * cursor.col as f32,
                origin.y + line_height * cursor.row as f32,
            ),
            size(cell_width, line_height),
        );
        let typing = crate::ime::Typing {
            view: self.pane.clone(),
            on_key: TerminalPane::key_down,
            marked,
            caret,
        };
        window.handle_input(&focus, typing, cx);
```

Use the names `paint` already has for the grid origin, the cursor (`let cursor = grid.cursor;` ~730) and the cell size. If `origin` or `cursor` are scoped inside the loop, copy them out before it. `TerminalPane::key_down` must have the exact `OnKey<TerminalPane>` signature (`&mut self, &KeyDownEvent, &mut Window, &mut Context<'_, Self>`). Rename its `_window` parameter only if clippy asks.

In `render_find_bar` (~460), add this child to the find bar's root div, which has `track_focus(&find_focus)` and `on_key_down(…find_key_down)`:

```rust
            .child({
                let (focus, view, marked) = (find_focus.clone(), cx.entity(), self.ime_marked.clone());
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        let typing = crate::ime::Typing { view, on_key: TerminalPane::find_key_down, marked, caret: bounds };
                        window.handle_input(&focus, typing, cx);
                    },
                )
                .absolute()
                .size_full()
            })
```

(Use the find bar's real focus-handle variable name. Make the find bar root `.relative()` if it is not already, so the canvas covers it.)

- [ ] **Step 6: Register the Shell fields' handlers** (`crates/mkrew-ui/src/main.rs`)

Add to `struct Shell`: `ime_marked: Rc<Cell<usize>>,` (doc: `/// Composing IME chars typed into the focused field so far (see mkrew_ui::ime).`), initialised with `Rc::default()`. Add the helper to `impl Shell`:

```rust
    /// Platform text input (IME, Unikey) for the field focused by `focus`, fed to `on_key`.
    /// Put it inside the field; only the focused field's handler is registered.
    fn typing(
        &self,
        focus: &FocusHandle,
        on_key: ime::OnKey<Self>,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement {
        let (focus, view, marked) = (focus.clone(), cx.entity(), self.ime_marked.clone());
        canvas(
            |_, _, _| (),
            move |bounds, (), window, cx| {
                let typing = ime::Typing { view, on_key, marked, caret: bounds };
                window.handle_input(&focus, typing, cx);
            },
        )
        .absolute()
        .size_full()
    }
```

Then, next to every `.on_key_down(cx.listener(Self::X_key_down))` (or `Shell::X_key_down`) that belongs to a surface with a text field, add `.relative().child(self.typing(&self.<that focus>, Self::X_key_down, cx))` to the same element. The scout list (line numbers approximate):

| Surface | Handler | Line |
| --- | --- | --- |
| composer | `composer_key_down` | ~12386 |
| git commit box | `git_commit_key_down` | ~8428 |
| doc/diff comment | `comment_key_down` | ~9135 |
| board follow-up | `follow_up_key_down` | ~14705 |
| task dialog | `task_dialog_key_down` | ~9362 |
| peek reply | `peek_key_down` | ~6864 |
| tab rename | `rename_key_down` | ~12134 |
| broadcast | `broadcast_key_down` | ~10881 |
| quick open | `quick_open_key_down` | ~11268 |
| command palette | `palette_key_down` | ~11422 |
| git base menu | `git_base_key_down` | ~10550 |
| dev server dialog | `dev_dialog_key_down` | ~10694 |
| prompt picker (if it has a filter field) | `prompt_picker_key_down` | grep |
| history dialog (if it has a filter field) | `history_key_down` | grep |

Do not add handlers to list-only surfaces (Files tree, Git list, undo, compare, turns, discard, doc dialog). Without a handler GPUI keeps the IME detached there, which is right for single-key shortcuts. If `text_field()` returns an element that already has `.relative()` or children, add the canvas as an extra child. It draws nothing.

- [ ] **Step 7: No duplicate chars.** With a handler registered, a printable key that a `*_key_down` handler does **not** stop goes on to TranslateMessage → WM_CHAR → handler → typed a second time (events.rs:433-459, platform.rs:517-519). For each handler in the table above (and `TerminalPane::find_key_down`), confirm it calls `cx.stop_propagation()` on every path that handles an `Insert` or `Backspace` key. `composer_key_down` (main.rs:5049) and `find_key_down` (view.rs:367) call it first thing. If one does not, add `cx.stop_propagation();` as its first statement. `TerminalPane::key_down` already stops whenever `key_to_bytes` returns bytes.

- [ ] **Step 8: Build, lint, test**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings` then `cargo test -p mkrew-ui --lib`
Expected: no warnings; PASS.

- [ ] **Step 9: Docs.** In `docs/HUMAN_CHECKS.md`, mark the IME/dead-key gaps (~809-811, ~1139) as implemented, and add the manual check from Task 10. In `docs/decisions/0001-ui-framework.md` (~217), add: "Done 2026-10-07: `mkrew_ui::ime::Typing` is the InputHandler of every text field and terminal."

- [ ] **Step 10: Record (no commit)**

Suggested message: `fix(ui): accept IME and Unicode input (Vietnamese Telex, Unikey) in fields and terminals`

---

### Task 7: `oneshot` profile command and the `ai.generate` request

**Design (verified constraints):** SPEC §1 forbids LLM API calls, so generation runs the user's own agent CLI in non-interactive mode. Each profile gets an optional `oneshot` command line, split like `resume` (`split_words`, profile.rs:129-146). The prompt always goes on **stdin**: no 32 K Windows command-line limit for diffs, and no cmd.exe argument escaping. The daemon serves one connection's requests one at a time (server.rs:186), and the UI uses one connection. So `ai.generate` returns at once, a spawned task runs the CLI with a 120 s timeout, and the answer arrives as the `ai.generated` event. The **client picks `generation_id`**, so the event can never arrive before the client knows the id. The prompt runs with cwd = project (or worktree) folder, so the agent can read the code itself when it rewrites a prompt. Seeded profiles are never overwritten (`seed_profiles`, daemon lib.rs:90). Existing users therefore get an error that says exactly what to add.

**Files:**
- Modify: `crates/mkrew-core/src/profile.rs` (struct `AgentProfile` 16-31, `impl AgentProfile` ~110, `parse_profile` 278-293, `RawProfile` 297-313, tests)
- Modify: `crates/mkrew-core/src/snapshots/mkrew_core__profile__tests__snapshot_unknown_field.snap`
- Modify: `crates/mkrew-daemon/src/launch.rs:172` (test literal: add `oneshot: None`)
- Modify: `agents/claude.toml`, `agents/codex.toml`, `agents/omp.toml`
- Modify: `crates/mkrew-protocol/src/lib.rs:16` (→ 24), `crates/mkrew-protocol/src/message.rs` (Request ~171, Event ~320, new types, round-trip test lists ~1624)
- Create: `crates/mkrew-daemon/src/server/ai.rs`
- Modify: `crates/mkrew-daemon/src/server.rs` (`mod ai;` at 35-45, dispatch next to line 379)
- Modify: `crates/mkrew-daemon/Cargo.toml:31` (tokio feature `process`)
- Modify: `crates/mkrew-daemon/tests/daemon.rs` (helper + tests)
- Modify: `docs/SPEC.md` §6 (profile example + text), §7 (request + event tables)
- Create: `docs/decisions/0016-one-shot-agent-helpers.md`

**Interfaces:**
- Produces (core): `AgentProfile.oneshot: Option<String>`; `AgentProfile::oneshot_command(&self) -> Option<AgentProfile>`.
- Produces (protocol, exact):

```rust
pub enum AiKind { CommitMessage, EnhancePrompt }            // serde snake_case
pub struct AiGenerateParams {
    pub generation_id: Ulid,
    pub project_id: Ulid,
    pub worktree_task_id: Option<Ulid>,   // #[serde(default)]
    pub agent_profile: Option<String>,    // #[serde(default)]; None = project default agent
    pub kind: AiKind,
    pub input: String,                    // #[serde(default)]; the prompt for EnhancePrompt
}
pub struct AiGeneratedEvent {
    pub generation_id: Ulid,
    pub project_id: Ulid,
    pub text: Option<String>,
    pub error: Option<String>,
}
Request::AiGenerate(AiGenerateParams)   // "ai.generate", returns null
Event::AiGenerated(AiGeneratedEvent)    // "ai.generated"
PROTOCOL_VERSION = 24
```
- Produces (daemon): `Daemon::ai_generate(&self, params: AiGenerateParams) -> Outcome`.

- [ ] **Step 1: Write the failing profile test** (in profile.rs `mod tests`, next to `resumed_runs_the_resume_command_with_the_native_id`)

```rust
    #[test]
    fn oneshot_splits_like_resume_and_blank_means_none() {
        let p = parse("command = \"x\"\noneshot = \"'C:\\\\Program Files\\\\x.exe' -p --model fast\"").unwrap();
        let one = p.oneshot_command().unwrap();
        assert_eq!(one.command, r"C:\Program Files\x.exe");
        assert_eq!(one.args, ["-p", "--model", "fast"]);
        assert_eq!((one.id, one.env), (p.id.clone(), p.env.clone()), "same agent, same env");
        assert_eq!(parse("command = \"x\"\noneshot = \"  \"").unwrap().oneshot_command(), None);
        assert_eq!(parse("command = \"x\"").unwrap().oneshot_command(), None);
    }
```

(In a TOML basic string `\\\\` in Rust source becomes `\\` in TOML, which is one `\`. If that is hard to read, use a TOML literal string: `oneshot = '\"C:\\Program Files\\x.exe\" -p --model fast'` built with `r#"…"#`.)

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p mkrew-core profile`
Expected: FAIL. The new test does not compile, and `snapshot_unknown_field` lists no `oneshot` key.

- [ ] **Step 3: Implement the profile key**

In `struct AgentProfile`, after `resume`:

```rust
    /// Non-interactive command for one-shot text (commit messages, prompt rewrites; SPEC §6):
    /// split like `resume`, the prompt goes on stdin; `None` = this agent cannot generate.
    pub oneshot: Option<String>,
```

In `impl AgentProfile`, after `resumed`:

```rust
    /// This profile running its `oneshot` command (prompt on stdin, SPEC §6); `None` without
    /// one. Split like [`Self::resumed`]; env and id stay the profile's.
    pub fn oneshot_command(&self) -> Option<AgentProfile> {
        let mut words = split_words(self.oneshot.as_deref()?).into_iter();
        let command = words.next()?;
        Some(AgentProfile {
            command,
            args: words.collect(),
            ..self.clone()
        })
    }
```

In `RawProfile`, after `resume: Option<String>,` add `oneshot: Option<String>,`. In `parse_profile`'s `Ok(AgentProfile { … })`, after `resume: …`, add `oneshot: raw.oneshot.filter(|o| !o.trim().is_empty()),`. Add `oneshot: None,` to the struct literals the compiler flags (profile.rs test literals use `..p`, so they are fine; `crates/mkrew-daemon/src/launch.rs:172` needs it).

Edit the snapshot's last line by hand so the expected list reads: `` expected one of `name`, `kind`, `command`, `args`, `prompt_mode`, `env`, `resume`, `oneshot`, `state`, `usage` `` (the field order of `RawProfile`).

- [ ] **Step 4: Sample profiles**

Add one line after `resume`/`prompt_mode` in each (comment included):

`agents/claude.toml`:
```toml
oneshot = "claude -p"           # one-shot text (✦ commit message / prompt rewrite); prompt on stdin
```
`agents/codex.toml`:
```toml
oneshot = "codex exec --sandbox read-only --skip-git-repo-check -"   # prompt on stdin ("-")
```
`agents/omp.toml`:
```toml
oneshot = "omp -p --no-session"  # one-shot text; prompt on stdin
```

Leave `fake.toml` and `shell.toml` without it. The `[INFERENCE]` that `omp -p` reads its prompt from stdin is unverified (its `--help` documents only `omp -p "<prompt>"`). It goes into the Task 10 manual checks. If it does not work, the user can edit the line, and no code changes.

Run: `cargo test -p mkrew-core profile`
Expected: PASS (including `repo_sample_profiles_parse`).

- [ ] **Step 5: Protocol types** (`crates/mkrew-protocol/src/message.rs`)

In `enum Request`, after `GitShow`:

```rust
    /// One-shot text from an agent CLI (SPEC §6 `oneshot`, §12B, §20B.1). Returns nothing
    /// (`null`) once the run started; the answer follows as `ai.generated`.
    #[serde(rename = "ai.generate")]
    AiGenerate(AiGenerateParams),
```

In `enum Event`, after `GitChanged`:

```rust
    /// The answer to the `ai.generate` with this `generation_id`.
    #[serde(rename = "ai.generated")]
    AiGenerated(AiGeneratedEvent),
```

Types (next to the other params structs; copy the derive list the neighbours use):

```rust
/// What `ai.generate` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiKind {
    /// A Conventional Commits message for the staged changes (else the unstaged ones).
    CommitMessage,
    /// A clearer version of `input`, a prompt for a coding agent.
    EnhancePrompt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiGenerateParams {
    /// Chosen by the client, so it knows the id before the answer can arrive.
    pub generation_id: Ulid,
    pub project_id: Ulid,
    /// Run in this task's worktree instead of the project folder.
    #[serde(default)]
    pub worktree_task_id: Option<Ulid>,
    /// Agent profile id; `None` = the project's default agent.
    #[serde(default)]
    pub agent_profile: Option<String>,
    pub kind: AiKind,
    /// The prompt to improve (`enhance_prompt`); unused for `commit_message`.
    #[serde(default)]
    pub input: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiGeneratedEvent {
    pub generation_id: Ulid,
    pub project_id: Ulid,
    /// The agent's answer, trimmed; `None` on failure.
    pub text: Option<String>,
    /// Why there is no text.
    pub error: Option<String>,
}
```

Add `Request::AiGenerate(AiGenerateParams { generation_id: Ulid::nil(), project_id: Ulid::nil(), worktree_task_id: None, agent_profile: Some("claude".into()), kind: AiKind::EnhancePrompt, input: "fix it".into() })` to the request round-trip list (~1624). Add an `Event::AiGenerated(…)` with `text: Some("feat: x".into()), error: None` to the event round trip (follow the pattern at files.rs:669). Use whatever ULID constructor the neighbouring entries use. Set `PROTOCOL_VERSION` to 24 in `crates/mkrew-protocol/src/lib.rs`.

Run: `cargo test -p mkrew-protocol`
Expected: PASS. Any exhaustive `match` over `Request`/`Event` in `mkrew-cli`, `mkrew-daemon` or `mkrew-ui` that fails to compile gets an arm consistent with its neighbours: CLI event printers ignore it like other UI-only events; the daemon dispatch is added in Step 8.

- [ ] **Step 6: Write the failing daemon tests** (`crates/mkrew-daemon/tests/daemon.rs`)

Helper in `impl Env` (next to `write_fake_profile_as`):

```rust
    /// `<id>.toml`: an agent whose `oneshot` runs the built fake agent with `scenario`.
    fn write_oneshot_profile(&self, id: &str, scenario: &Path) {
        let fake = built_bin("mkrew-fakeagent");
        let toml = format!(
            "command = '{fake}'\noneshot = '\"{fake}\" --scenario \"{scenario}\"'\n",
            fake = fake.display(),
            scenario = scenario.display(),
        );
        self.write_profile(id, &toml);
    }
```

Tests (end of file, near the git tests; reuse `git()` and `error_code()`):

```rust
fn generate(project_id: Ulid, profile: Option<&str>, kind: AiKind, input: &str) -> (Ulid, Request) {
    let generation_id = Ulid::new();
    let request = Request::AiGenerate(AiGenerateParams {
        generation_id,
        project_id,
        worktree_task_id: None,
        agent_profile: profile.map(str::to_owned),
        kind,
        input: input.into(),
    });
    (generation_id, request)
}

fn generated(client: &mut Client, id: Ulid) -> AiGeneratedEvent {
    let event = client.wait_event_where(|e| matches!(e, Event::AiGenerated(g) if g.generation_id == id));
    let Event::AiGenerated(done) = event else { unreachable!("matched above") };
    done
}

fn init_repo(repo: &Path) {
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["config", "core.autocrlf", "false"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "one"]);
}

/// SPEC §6 `oneshot`: the agent's stdout is the answer, without blank edges or a code fence.
#[test]
fn ai_generate_answers_with_the_oneshot_output() {
    let env = Env::new();
    let repo = env.repo();
    init_repo(&repo);
    std::fs::write(repo.join("a.txt"), "a\nb\n").unwrap();
    git(&repo, &["add", "."]);
    let scenario = env.dir.path().join("answer.toml");
    let steps = ["```", "feat: add b", "```"]
        .map(|text| format!("[[step]]\naction = \"print\"\ntext = \"{text}\"\n"))
        .join("\n");
    std::fs::write(&scenario, steps).unwrap();
    env.write_oneshot_profile("gen", &scenario);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    let (id, request) = generate(project.id, Some("gen"), AiKind::CommitMessage, "");
    client.call(request).into_result::<()>().unwrap();
    let done = generated(&mut client, id);
    assert_eq!((done.text.as_deref(), done.error), (Some("feat: add b"), None));
    assert_eq!(done.project_id, project.id);

    let (id, request) = generate(project.id, Some("gen"), AiKind::EnhancePrompt, "add b");
    client.call(request).into_result::<()>().unwrap();
    assert_eq!(generated(&mut client, id).text.as_deref(), Some("feat: add b"));
}

/// What a user meets with old profiles, no default agent, a failing agent or no changes.
#[test]
fn ai_generate_reports_what_is_missing() {
    let env = Env::new();
    let repo = env.repo();
    init_repo(&repo);
    env.write_fake_profile(); // `fake`: no `oneshot`
    let failing = env.dir.path().join("fail.toml");
    std::fs::write(&failing, "[[step]]\naction = \"exit\"\ncode = 3\n").unwrap();
    env.write_oneshot_profile("broken", &failing);
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);

    // A profile seeded before `oneshot` existed: the error says what to add, and where.
    let (_, request) = generate(project.id, Some("fake"), AiKind::EnhancePrompt, "x");
    let outcome = client.call(request);
    let message = format!("{outcome:?}");
    assert_eq!(error_code(outcome), ErrorCode::InvalidState);
    assert!(message.contains("oneshot") && message.contains("fake.toml"), "{message}");

    // No profile given and no default agent.
    let (_, request) = generate(project.id, None, AiKind::EnhancePrompt, "x");
    assert_eq!(error_code(client.call(request)), ErrorCode::InvalidState);

    // Nothing to improve.
    let (_, request) = generate(project.id, Some("broken"), AiKind::EnhancePrompt, "  ");
    assert_eq!(error_code(client.call(request)), ErrorCode::InvalidRequest);

    // The agent fails: the event carries the reason.
    let (id, request) = generate(project.id, Some("broken"), AiKind::EnhancePrompt, "x");
    client.call(request).into_result::<()>().unwrap();
    let done = generated(&mut client, id);
    assert_eq!(done.text, None);
    assert!(done.error.as_deref().is_some_and(|e| e.contains('3')), "{done:?}");

    // A clean repo has nothing to describe; the agent is not even started.
    let (id, request) = generate(project.id, Some("broken"), AiKind::CommitMessage, "");
    client.call(request).into_result::<()>().unwrap();
    assert_eq!(generated(&mut client, id).error.as_deref(), Some("No changes to describe."));
}
```

(Match the existing test conventions: `Env::dir` is the temp dir used by `write_profile`, and `into_result::<T>()` is how other tests read replies. If `Outcome`'s `Debug` does not include the message, read it the way `error_code` reads the code, daemon.rs:363. Run the fake agent once by hand (`target\debug\mkrew-fakeagent --scenario answer.toml < NUL`) to confirm it prints only the step texts. If it prints anything else, change the expected answer to match, without weakening the fence check.)

Run: `cargo test -p mkrew-daemon --test daemon ai_generate`
Expected: FAIL to compile or with `unknown method ai.generate`.

- [ ] **Step 7: Write the failing unit tests for the pure parts** (they live in the new `crates/mkrew-daemon/src/server/ai.rs`, Step 8, in `#[cfg(test)] mod tests`)

```rust
#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn long_diffs_are_cut_on_a_char_boundary_with_a_note() {
        let diff = "é".repeat(10); // 20 bytes
        let cut = truncated(&diff, 5);
        assert!(cut.starts_with("éé") && !cut.starts_with("ééé"), "{cut}");
        assert!(cut.ends_with("[diff truncated]\n"));
        assert_eq!(truncated("short", 5), "short");
    }

    #[test]
    fn answers_lose_blank_edges_and_a_code_fence() {
        assert_eq!(clean_output("\n```text\nfeat: x\n\nbody\n```\n").as_deref(), Some("feat: x\n\nbody"));
        assert_eq!(clean_output("  fix: y  \n").as_deref(), Some("fix: y"));
        assert_eq!(clean_output("```\n```"), None);
        assert_eq!(clean_output(" \n "), None);
    }

    #[test]
    fn commit_prompt_falls_back_to_unstaged_and_new_files() {
        let prompt = commit_prompt_from("", "-old\n+new\n", &["n.txt".into()]).unwrap();
        assert!(prompt.contains("+new") && prompt.contains("new file: n.txt"), "{prompt}");
        let staged = commit_prompt_from("+staged\n", "+unstaged\n", &[]).unwrap();
        assert!(staged.contains("+staged") && !staged.contains("+unstaged"));
        assert_eq!(commit_prompt_from(" ", "", &[]), Err("No changes to describe.".to_owned()));
    }

    /// An agent that never answers is stopped; the user gets a reason, not a spinner forever.
    #[tokio::test]
    async fn run_stops_an_agent_that_does_not_answer() {
        let mut sleeper = if cfg!(windows) {
            let mut c = std::process::Command::new("powershell");
            c.args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"]);
            c
        } else {
            let mut c = std::process::Command::new("sleep");
            c.arg("30");
            c
        };
        sleeper
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let started = std::time::Instant::now();
        let error = run(sleeper, "hi", Duration::from_millis(300)).await.unwrap_err();
        assert!(error.contains("did not answer"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
```

- [ ] **Step 8: Implement `server/ai.rs`**

Enable the tokio feature in `crates/mkrew-daemon/Cargo.toml:31`: add `"process"` to the `features` list. If `Cargo.lock` changes, tell the user. Never commit it.

Create `crates/mkrew-daemon/src/server/ai.rs`:

```rust
//! `ai.generate` (SPEC §6 `oneshot`, §12B, §20B.1; ADR 0016): one-shot text from the user's
//! own agent CLI. Mkrew never calls an LLM API: it runs the profile's non-interactive command
//! in the project folder with the prompt on stdin and broadcasts its stdout as `ai.generated`.

use std::borrow::Cow;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use mkrew_protocol::{AiGenerateParams, AiGeneratedEvent, AiKind, ErrorCode, Event};
use tokio::io::AsyncWriteExt;

use super::{Daemon, Outcome, ok};
use crate::git::{GitBackend, Side};
use crate::launch;

/// Longest an agent may take to answer.
const TIMEOUT: Duration = Duration::from_secs(120);
/// Diff bytes sent to the agent; lockfiles and generated code can be huge.
// ponytail: plain byte cap; summarize per file if long diffs give poor messages.
const MAX_DIFF_BYTES: usize = 64 * 1024;

impl Daemon {
    /// `ai.generate`: checks what it can right away, then runs the agent in the background (this
    /// connection keeps serving requests) and broadcasts `ai.generated`.
    pub(super) async fn ai_generate(&self, params: AiGenerateParams) -> Outcome {
        if params.kind == AiKind::EnhancePrompt && params.input.trim().is_empty() {
            let message = "ai.generate needs a prompt to improve";
            return Outcome::error(ErrorCode::InvalidRequest, message);
        }
        let target = match self.target(params.project_id, params.worktree_task_id).await {
            Ok(target) => target,
            Err(outcome) => return outcome,
        };
        let Some(profile_id) = params
            .agent_profile
            .clone()
            .or_else(|| target.project.default_agent.clone())
        else {
            let message = format!("set a default agent for {} first", target.project.name);
            return Outcome::error(ErrorCode::InvalidState, message);
        };
        let profile = match self.load_profile(&profile_id) {
            Ok(profile) => profile,
            Err(outcome) => return outcome,
        };
        let Some(oneshot) = profile.oneshot_command() else {
            let file = self.paths.agents_dir().join(format!("{profile_id}.toml"));
            let message = format!(
                "agent profile {profile_id} has no `oneshot` command: add a line like \
                 `oneshot = \"claude -p\"` to {}",
                file.display()
            );
            return Outcome::error(ErrorCode::InvalidState, message);
        };
        let builder = match launch::command(&oneshot) {
            Ok(builder) => builder,
            Err(error) => return Outcome::error(ErrorCode::InvalidState, error.to_string()),
        };
        let Some(command) = child_command(&builder, &target.dir) else {
            let message = format!("agent profile {profile_id} has an empty `oneshot` command");
            return Outcome::error(ErrorCode::InvalidState, message);
        };
        let (hub, git, dir) = (Arc::clone(&self.hub), Arc::clone(&self.git), target.dir);
        let (generation_id, project_id) = (params.generation_id, params.project_id);
        tokio::spawn(async move {
            let (kind, input) = (params.kind, params.input);
            let prompt = tokio::task::spawn_blocking(move || prompt(&*git, &dir, kind, &input))
                .await
                .map_err(|error| format!("cannot build the prompt: {error}"))
                .and_then(|prompt| prompt);
            let result = match prompt {
                Ok(prompt) => run(command, &prompt, TIMEOUT).await,
                Err(error) => Err(error),
            };
            if let Err(error) = &result {
                tracing::info!(%generation_id, "ai.generate failed: {error}");
            }
            let (text, error) = match result {
                Ok(text) => (Some(text), None),
                Err(error) => (None, Some(error)),
            };
            let event = AiGeneratedEvent { generation_id, project_id, text, error };
            let _ = hub.events.send(Event::AiGenerated(event));
        });
        ok(&())
    }
}

/// The agent's program as a plain child process (no PTY) in `dir`, pipes for all three
/// streams, no console window. `None` if the command line is empty.
fn child_command(builder: &portable_pty::CommandBuilder, dir: &Path) -> Option<std::process::Command> {
    let (program, args) = builder.get_argv().split_first()?;
    let mut command = std::process::Command::new(program);
    command
        .args(args)
        .envs(builder.iter_extra_env_as_str())
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The daemon has no console; without this the agent would open one.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    Some(command)
}

/// What the agent is asked. Runs git, so call it off the async threads.
fn prompt(git: &dyn GitBackend, dir: &Path, kind: AiKind, input: &str) -> Result<String, String> {
    let failed = |error: std::io::Error| format!("git failed: {error}");
    match kind {
        AiKind::EnhancePrompt => Ok(enhance_prompt(input)),
        AiKind::CommitMessage => {
            let staged = git.file_diff(dir, ".", Side::Staged).map_err(failed)?;
            if !staged.trim().is_empty() {
                return commit_prompt_from(&staged, "", &[]);
            }
            let unstaged = git.file_diff(dir, ".", Side::Unstaged).map_err(failed)?;
            let untracked = git.untracked_files(dir).map_err(failed)?;
            commit_prompt_from("", &unstaged, &untracked)
        }
    }
}

/// The commit-message prompt for the staged diff, else the unstaged diff plus new files.
fn commit_prompt_from(staged: &str, unstaged: &str, untracked: &[String]) -> Result<String, String> {
    let diff = if staged.trim().is_empty() {
        let mut diff = unstaged.to_owned();
        for path in untracked {
            diff.push_str(&format!("new file: {path}\n"));
        }
        diff
    } else {
        staged.to_owned()
    };
    if diff.trim().is_empty() {
        return Err("No changes to describe.".to_owned());
    }
    Ok(format!(
        "Write a git commit message for the changes below, following Conventional Commits: a \
         first line `type(scope): summary` (type is one of feat, fix, docs, style, refactor, \
         perf, test, build, ci, chore; scope optional; summary in the imperative mood, at most \
         72 characters, no final period), then, only if it helps explain why, a blank line and \
         a short body wrapped at 72 characters. Reply with the message only: no quotes, no code \
         fence, no comments. Do not run tools or change files.\n\n{}",
        truncated(&diff, MAX_DIFF_BYTES)
    ))
}

fn enhance_prompt(input: &str) -> String {
    format!(
        "Rewrite the prompt below so a coding agent can act on it well. Keep the author's \
         intent and language. Make the goal, constraints and expected result explicit, and name \
         the relevant files, functions or commands. You may read files in the current directory \
         to get these right, but do not change any file and do not run commands that change \
         anything. Reply with the improved prompt only: no preamble, no code fence.\n\n{input}"
    )
}

/// `text` cut to at most `max` bytes on a char boundary, with a note when cut.
fn truncated(text: &str, max: usize) -> Cow<'_, str> {
    if text.len() <= max {
        return Cow::Borrowed(text);
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}\n[diff truncated]\n", &text[..end]))
}

/// The answer without blank edges and a surrounding ``` fence; `None` if nothing is left.
fn clean_output(stdout: &str) -> Option<String> {
    let text = stdout.trim();
    let text = text
        .strip_prefix("```")
        .and_then(|rest| rest.split_once('\n'))
        .and_then(|(_, body)| body.trim_end().strip_suffix("```"))
        .map_or(text, str::trim);
    (!text.is_empty()).then(|| text.to_owned())
}

/// Runs `command` with `prompt` on stdin; its stdout is the answer. Killed after `timeout`.
async fn run(command: std::process::Command, prompt: &str, timeout: Duration) -> Result<String, String> {
    let mut command = tokio::process::Command::from(command);
    command.kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start the agent: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let prompt = prompt.to_owned();
        // Dropping `stdin` after the write sends EOF. An agent that exits without reading
        // everything breaks the pipe; that is not an error.
        tokio::spawn(async move {
            let _ = stdin.write_all(prompt.as_bytes()).await;
        });
    }
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("the agent did not answer within {} s", timeout.as_secs()))?
        .map_err(|error| format!("the agent failed: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let last: Vec<&str> = stderr.trim().lines().rev().take(3).collect();
        let last: Vec<&str> = last.into_iter().rev().collect();
        return Err(format!("the agent exited with {}: {}", output.status, last.join(" / ")));
    }
    clean_output(&String::from_utf8_lossy(&output.stdout))
        .ok_or_else(|| "the agent printed nothing".to_owned())
}
```

(Notes for the implementer: check `Side`'s variant names at git.rs:190 (`Staged`/`Unstaged`), and that `GitBackend::file_diff(dir, ".", side)` with `--literal-pathspecs` covers the whole folder. The happy-path daemon test proves it. If `"."` is rejected, use `git.diff_command`-style args through a new `GitBackend` method instead and say so. `iter_extra_env_as_str()` yields `(&str, &str)` pairs (portable-pty cmdbuilder.rs:360). `Duration::as_secs()` of 300 ms prints `0 s`. The unit test checks only "did not answer".)

Register it in `crates/mkrew-daemon/src/server.rs`: add `mod ai;` to the `mod` list (35-45), and next to `Request::GitCommit(params) => …` (line 379):

```rust
            Request::AiGenerate(params) => (self.ai_generate(params).await, After::Continue),
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cargo test -p mkrew-daemon --lib server::ai` then `cargo test -p mkrew-daemon --test daemon ai_generate`
Expected: PASS (4 unit tests, 2 end-to-end tests).

- [ ] **Step 10: SPEC + ADR**

`docs/SPEC.md` §6: in the `agents/claude.toml` example, after the `resume = …` line, add:

```toml
# One-shot text (✦ commit message §12B, ✦ improve prompt §20B.1): non-interactive command,
# split like `resume`; the prompt goes on stdin, stdout is the answer. Empty = not offered.
oneshot = "claude -p"
```

Add the codex/omp lines from Step 4 to their examples. After the "Unknown keys…" paragraph, add: "Profiles written before `oneshot` existed are not updated (seeding never overwrites); ✦ then answers `invalid_state` naming the key and the file."

§7 request table, after `git.show`:

```markdown
| `ai.generate` | `generation_id` (client-chosen), `project_id`, `worktree_task_id?`, `agent_profile?` (none = project `default_agent`), `kind` (`commit_message`/`enhance_prompt`), `input` | `null` at once; runs the profile's `oneshot` (§6) with a 120 s limit in the project/worktree folder, prompt on stdin; answer in `ai.generated` |
```

§7 event table, after `git.changed`:

```markdown
| `ai.generated` | answer to an `ai.generate` (`generation_id`, `project_id`, `text?` trimmed and unfenced, `error?`) |
```

Create `docs/decisions/0016-one-shot-agent-helpers.md` (template format, Date 2026-10-07, accepted, Milestone post-M13 fixes):
- Context: the user wants ✦ commit messages and prompt rewrites. SPEC §1 forbids LLM API calls. The daemon serves one connection's requests one at a time.
- Options: 1) profile `oneshot` command + async request/event (no API keys, works with any agent CLI, follows §2.1); 2) type the request into a running agent PTY and scrape the reply (fragile, pollutes the user's session); 3) call a provider API directly (forbidden by §1).
- Decision: option 1. The prompt goes on stdin (32 K command-line limit, cmd.exe escaping). The client chooses the id (no race between reply and event). The cwd is the project folder (the agent reads the code itself for prompt rewrites).
- Consequences: protocol v24. Old seeded profiles need a manual `oneshot` line. The agent could in principle write files, so the shipped commands use read-only modes where the CLI has one (`codex exec --sandbox read-only`; `claude -p` denies edits without a permission prompt) and the prompts forbid edits. The omp stdin behaviour is unverified (HUMAN_CHECKS). The output is only ever put in a text box; nothing is committed or sent automatically.

- [ ] **Step 11: Record (no commit)**

Suggested message: `feat(daemon): one-shot agent text via profile oneshot and ai.generate`

---

### Task 8: ✦ commit message button in the Git view

**Files:**
- Modify: `crates/mkrew-ui/src/git_actions.rs:22-96` (`CommitBox`), tests module
- Modify: `crates/mkrew-ui/src/main.rs` (new `generate_commit_message` next to `commit_git` ~2000; event handling in the event loop after the `Event::GitChanged` block ~6362; `git_commit_box` ~8369-8450)
- Modify: `docs/SPEC.md` §12B "Commit" bullet (~726)

**Interfaces:**
- Consumes: Task 7 `Request::AiGenerate(AiGenerateParams)`, `Event::AiGenerated(AiGeneratedEvent)`, `AiKind::CommitMessage`.
- Produces: `CommitBox.generating: Option<Ulid>`; `CommitBox::begin_generate(&mut self, generation_id: Ulid) -> bool`; `CommitBox::generated(&mut self, generation_id: Ulid, result: Result<String, String>) -> bool`; `fn ai_result(done: &AiGeneratedEvent) -> Result<String, String>` (free fn in main.rs; Task 9 uses it too); `Shell::generate_commit_message(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>)`.

- [ ] **Step 1: Write the failing tests** (git_actions.rs `mod tests`)

```rust
    #[test]
    fn a_generated_message_replaces_the_draft_and_typing_waits_for_it() {
        let mut commit = CommitBox { text: "wip".into(), ..CommitBox::default() };
        let (first, second) = (Ulid::new(), Ulid::new());
        assert!(commit.begin_generate(first));
        assert!(!commit.begin_generate(second), "one generation at a time");
        commit.insert("x");
        commit.key(DialogKey::Backspace);
        assert_eq!(commit.text, "wip", "typing is ignored while the agent writes");
        assert!(commit.begin(Ulid::new(), 1).is_none(), "no commit while the agent writes");
        assert!(!commit.generated(second, Ok("other".into())), "not this box's generation");
        assert!(commit.generated(first, Ok("feat: add x\r\n\r\nWhy.".into())));
        assert_eq!(commit.text, "feat: add x\n\nWhy.");
        assert_eq!(commit.generating, None);
        assert_eq!(commit.error, None);
    }

    #[test]
    fn a_failed_generation_keeps_the_draft_and_says_why() {
        let mut commit = CommitBox { text: "wip".into(), ..CommitBox::default() };
        let id = Ulid::new();
        assert!(commit.begin_generate(id));
        assert!(commit.generated(id, Err("the agent exited with 3".into())));
        assert_eq!(commit.text, "wip");
        assert_eq!(
            commit.error.as_deref(),
            Some("Cannot write a message: the agent exited with 3")
        );
        assert!(commit.begin_generate(Ulid::new()), "the user can retry");
        let mut committing = CommitBox { busy: true, ..CommitBox::default() };
        assert!(!committing.begin_generate(Ulid::new()), "not while committing");
    }
```

(Import `crate::task_dialog::DialogKey` and `ulid::Ulid` in the tests module if `use super::*;` does not bring them in.)

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mkrew-ui --lib git_actions`
Expected: FAIL to compile: no method `begin_generate`.

- [ ] **Step 3: Implement in `CommitBox`**

Add the field after `error`:

```rust
    /// The `ai.generate` writing this draft (SPEC §12B ✦), by id; typing waits for it.
    pub generating: Option<Ulid>,
```

Change the guards: in `key`'s `DialogKey::Backspace` arm, `if !self.busy` becomes `if !self.busy && self.generating.is_none()`. In `insert`, `if self.busy` becomes `if self.busy || self.generating.is_some()`. In `begin`, `if self.busy` becomes `if self.busy || self.generating.is_some()`. Add:

```rust
    /// Starts generation `generation_id` of the message; `false` while committing or while
    /// another generation runs.
    pub fn begin_generate(&mut self, generation_id: Ulid) -> bool {
        if self.busy || self.generating.is_some() {
            return false;
        }
        self.generating = Some(generation_id);
        self.error = None;
        true
    }

    /// The answer of generation `generation_id`: the text replaces the draft, a failure keeps
    /// it and says why. `false` if this box did not ask for it.
    pub fn generated(&mut self, generation_id: Ulid, result: Result<String, String>) -> bool {
        if self.generating != Some(generation_id) {
            return false;
        }
        self.generating = None;
        match result {
            Ok(text) => self.text = text.replace("\r\n", "\n"),
            Err(error) => self.error = Some(format!("Cannot write a message: {error}")),
        }
        true
    }
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p mkrew-ui --lib git_actions`
Expected: PASS.

- [ ] **Step 5: Request + event in main.rs**

Next to `commit_git`:

```rust
    /// ✦ in the commit box (SPEC §12B): the workspace's default agent writes a Conventional
    /// Commits message from the changes; the answer arrives as `ai.generated`.
    fn generate_commit_message(
        &mut self,
        project_id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let view = self.git_view(project_id);
        let generation_id = Ulid::new();
        if !view.commit.begin_generate(generation_id) {
            return;
        }
        let params = AiGenerateParams {
            generation_id,
            project_id,
            worktree_task_id: view.worktree_task_id,
            agent_profile: None,
            kind: AiKind::CommitMessage,
            input: String::new(),
        };
        cx.notify();
        let started = connection.call::<()>(Request::AiGenerate(params));
        cx.spawn_in(window, async move |this, cx| {
            let Err(error) = started.await else {
                return;
            };
            let _ = this.update_in(cx, |shell, _, cx| {
                for view in shell.git.values_mut() {
                    view.commit.generated(generation_id, Err(format!("{error:#}")));
                }
                cx.notify();
            });
        })
        .detach();
    }
```

Free function near the other free helpers at the bottom of main.rs:

```rust
/// The text of an `ai.generated`, or why there is none.
fn ai_result(done: &AiGeneratedEvent) -> Result<String, String> {
    match (&done.text, &done.error) {
        (Some(text), _) => Ok(text.clone()),
        (None, error) => Err(error.clone().unwrap_or_else(|| "no answer".to_owned())),
    }
}
```

In the event loop, right after the `if matches!(event, Event::GitChanged(_)) { … }` block:

```rust
                if let Event::AiGenerated(done) = event {
                    let result = ai_result(done);
                    for view in self.git.values_mut() {
                        view.commit.generated(done.generation_id, result.clone());
                    }
                    cx.notify();
                }
```

Add `AiGenerateParams`, `AiGeneratedEvent`, `AiKind` to main.rs's `mkrew_protocol` import.

- [ ] **Step 6: The ✦ button** in `git_commit_box`

After `let draft = &view.commit;`, add `let generating = draft.generating.is_some();`. Change the text field's `.when(draft.busy, …)` to `.when(draft.busy || generating, |el| el.opacity(opacity::DISABLED))`. Before building the bottom row:

```rust
        let write = if draft.busy || generating {
            let label = if generating { "✦ Writing…" } else { "✦" };
            disabled_button("git-commit-generate", label, theme, false)
        } else {
            quiet_button("git-commit-generate", "✦", theme, theme.surface_raised).on_click(
                cx.listener(move |shell, _, window, cx| {
                    shell.generate_commit_message(project_id, window, cx)
                }),
            )
        };
```

In the bottom row, insert it between the summary and the commit button:

```rust
                    .child(meta_line(theme, summary.into()).flex_1().min_w_0())
                    .child(write.flex_none().tooltip(tooltip(
                        theme,
                        "Write the message with the workspace's default agent (Conventional Commits)",
                        "",
                    )))
                    .child(commit.tooltip(tooltip(theme, tip, "Ctrl+Enter"))),
```

(`✦` U+2726 matches the app's monochrome glyph icons (✓ ✕ ⧗). Segoe UI Symbol renders it through the system fallback. Do not add `.hover()`: `quiet_button` already sets it.)

- [ ] **Step 7: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 8: SPEC.** §12B Commit bullet: `- **Commit:** multi-line message box, Commit button (`Ctrl+Enter`), and ✦: the workspace's default agent writes a Conventional Commits message from the staged changes (else the unstaged ones) via `ai.generate` (§7); the answer replaces the draft, nothing is committed until the user commits. No amend in v1.`

- [ ] **Step 9: Record (no commit)**

Suggested message: `feat(ui): ✦ writes the commit message with the default agent`

---

### Task 9: ✦ improve-prompt button in the composer

**Files:**
- Modify: `crates/mkrew-ui/src/composer.rs` (`Draft` 16-25, `Composers::key`/`insert`/`begin_send`, new methods, tests)
- Modify: `crates/mkrew-ui/src/main.rs` (new `enhance_composer` next to `send_composer` ~5082; the `Event::AiGenerated` block from Task 8; `composer_panel` status ~12390 and buttons ~12500)
- Modify: `docs/SPEC.md` §20B.1

**Interfaces:**
- Consumes: Task 7 protocol types; Task 8 `ai_result(&AiGeneratedEvent)`; `Session.agent_profile: String` and `Session.project_id` from `self.sidebar.sessions()`.
- Produces: `Draft.enhancing: Option<Ulid>`; `Composers::begin_enhance(&mut self, session: Ulid, generation_id: Ulid) -> Option<String>`; `Composers::enhanced(&mut self, generation_id: Ulid, result: Result<String, String>) -> bool`; `Shell::enhance_composer(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>)`.

- [ ] **Step 1: Write the failing tests** (composer.rs `mod tests`)

```rust
    #[test]
    fn an_improved_prompt_replaces_only_its_draft() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut composers = Composers::default();
        for session in [a, b] {
            composers.open(session);
            composers.insert(session, "fix bug");
        }
        let id = Ulid::new();
        assert_eq!(composers.begin_enhance(a, id).as_deref(), Some("fix bug"));
        assert_eq!(composers.begin_enhance(a, Ulid::new()), None, "one rewrite at a time");
        composers.insert(a, "!");
        composers.key(a, DialogKey::Backspace);
        assert!(composers.begin_send(a).is_none(), "no send while the agent rewrites");
        assert!(composers.enhanced(id, Ok("Fix the crash in parse()".into())));
        assert_eq!(composers.shown(a).unwrap().text.text(), "Fix the crash in parse()");
        assert_eq!(composers.shown(b).unwrap().text.text(), "fix bug");
        assert!(!composers.enhanced(id, Ok("again".into())), "answered once");
    }

    #[test]
    fn a_failed_rewrite_keeps_the_draft_and_blank_drafts_are_not_rewritten() {
        let session = Ulid::new();
        let mut composers = Composers::default();
        composers.open(session);
        assert_eq!(composers.begin_enhance(session, Ulid::new()), None, "nothing typed");
        composers.insert(session, "add tests");
        let id = Ulid::new();
        assert!(composers.begin_enhance(session, id).is_some());
        assert!(composers.enhanced(id, Err("the agent did not answer within 120 s".into())));
        let draft = composers.shown(session).unwrap();
        assert_eq!(draft.text.text(), "add tests");
        assert_eq!(
            draft.error.as_deref(),
            Some("Cannot improve the prompt: the agent did not answer within 120 s")
        );
        assert_eq!(draft.enhancing, None);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mkrew-ui --lib composer`
Expected: FAIL to compile: no method `begin_enhance`.

- [ ] **Step 3: Implement in composer.rs**

`Draft` gets, after `busy`:

```rust
    /// The `ai.generate` rewriting this draft (SPEC §20B.1 ✦), by id; typing waits for it.
    pub enhancing: Option<Ulid>,
```

Guards: in `key`, `_ if draft.busy =>` becomes `_ if draft.busy || draft.enhancing.is_some() =>` (Esc still hides). In `insert`, `.filter(|d| !d.busy)` becomes `.filter(|d| !d.busy && d.enhancing.is_none())`. In `begin_send`, add `&& draft.enhancing.is_none()` to the filter. New methods in `impl Composers`:

```rust
    /// Starts rewriting `session`'s draft as generation `generation_id`; returns the text to
    /// improve, or `None` while sending or rewriting or when only blanks were typed.
    pub fn begin_enhance(&mut self, session: Ulid, generation_id: Ulid) -> Option<String> {
        let draft = self.drafts.get_mut(&session).filter(|draft| {
            !draft.busy && draft.enhancing.is_none() && !draft.text.is_blank()
        })?;
        draft.enhancing = Some(generation_id);
        draft.error = None;
        Some(without_blank_edges(draft.text.text()).to_owned())
    }

    /// The answer of generation `generation_id`: the rewrite replaces its draft, a failure
    /// keeps the draft and says why. `false` if no draft asked for it.
    pub fn enhanced(&mut self, generation_id: Ulid, result: Result<String, String>) -> bool {
        let Some(draft) = self
            .drafts
            .values_mut()
            .find(|draft| draft.enhancing == Some(generation_id))
        else {
            return false;
        };
        draft.enhancing = None;
        match result {
            Ok(text) => {
                draft.text.clear();
                draft.text.insert(&text);
            }
            Err(error) => draft.error = Some(format!("Cannot improve the prompt: {error}")),
        }
        true
    }
```

(`without_blank_edges` already exists in composer.rs. `begin_send` uses it.)

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p mkrew-ui --lib composer`
Expected: PASS.

- [ ] **Step 5: Request + event in main.rs**

Next to `send_composer`:

```rust
    /// ✦ in the composer (SPEC §20B.1): the session's own agent rewrites the draft, reading
    /// the code in the project folder if it needs to; the answer arrives as `ai.generated`.
    fn enhance_composer(&mut self, session: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let Some(info) = self.sidebar.sessions().iter().find(|s| s.id == session) else {
            return;
        };
        // ponytail: the project folder even for a worktree session; same code, other branch.
        let (project_id, agent_profile) = (info.project_id, info.agent_profile.clone());
        let generation_id = Ulid::new();
        let Some(input) = self.composers.begin_enhance(session, generation_id) else {
            return;
        };
        cx.notify();
        let started = connection.call::<()>(Request::AiGenerate(AiGenerateParams {
            generation_id,
            project_id,
            worktree_task_id: None,
            agent_profile: Some(agent_profile),
            kind: AiKind::EnhancePrompt,
            input,
        }));
        cx.spawn_in(window, async move |this, cx| {
            let Err(error) = started.await else {
                return;
            };
            let _ = this.update_in(cx, |shell, _, cx| {
                shell.composers.enhanced(generation_id, Err(format!("{error:#}")));
                cx.notify();
            });
        })
        .detach();
    }
```

In the `Event::AiGenerated` block from Task 8, after the `for view in self.git.values_mut()` loop, add:

```rust
                    self.composers.enhanced(done.generation_id, result);
```

(Remove the `.clone()` from the git loop's `result` only if clippy flags a redundant clone. Otherwise keep both.)

- [ ] **Step 6: UI in `composer_panel`**

Status row (~12390, where `("↑", theme.text_muted, "Sending…".to_owned())` is chosen while `draft.busy`): add a branch right after the busy one:

```rust
        } else if draft.enhancing.is_some() {
            Some(("✦", theme.text_muted, "Improving the prompt…".to_owned()))
```

Button, before the `// The prompt library (§20B.3)` `.when(is_focused && !draft.busy, …)` block:

```rust
                    // ✦ (§20B.1): the agent rewrites the draft.
                    .when(is_focused && !draft.busy && !draft.text.is_blank(), |el| {
                        let improve = if draft.enhancing.is_some() {
                            disabled_button("composer-enhance", "✦", theme, false)
                        } else {
                            quiet_button("composer-enhance", "✦", theme, theme.surface_raised)
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.enhance_composer(session, window, cx)
                                }))
                        };
                        el.child(improve.flex_none().tooltip(tooltip(
                            theme,
                            "Improve this prompt with the agent (it may read the code)",
                            "",
                        )))
                    })
```

(GPUI gotcha from PROGRESS notes: an element with `track_focus` grabs focus on mouse down. The button sits inside the focused composer, as "Prompts…" does, so focus stays.)

- [ ] **Step 7: Build and lint**

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings` then `cargo test -p mkrew-ui --lib composer git_actions`
Expected: no warnings; PASS.

- [ ] **Step 8: SPEC.** In §20B.1, after "Drafts are kept per session…", add: `✦ (shown while the composer has the keyboard and the draft is not blank) sends the draft to the session's own agent profile via `ai.generate` (`enhance_prompt`, §7). The agent may read the project folder for context. The rewrite replaces the draft, and the user still sends it. Typing waits until the answer arrives; a failure keeps the draft and shows why.`

- [ ] **Step 9: Record (no commit)**

Suggested message: `feat(ui): ✦ improves the composer prompt with the session's agent`

---

### Task 10: Full verification, manual checks, notes

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (new "To verify" entry)
- Modify: `docs/PROGRESS.md` ("Notes for the next session")

- [ ] **Step 1: Full verify**

Run (PowerShell): `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`
If rustc crashes on mkrew-ui test builds: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace -j 4`.
Expected: fmt clean, no clippy warnings, all tests pass.

- [ ] **Step 2: HUMAN_CHECKS entry.** Add a "To verify — 2026-10-07 daily fixes" section listing the checks the user runs with `cargo run -p mkrew-ui` (stop an old daemon first: `target\debug\mk daemon stop`):
  1. Vietnamese: with Unikey (Telex, default settings) and with Windows "Vietnamese Telex", type `Tiếng Việt có dấu` into a terminal (pwsh, then claude/omp), the composer, the commit box, the palette and quick open. Expected: the text is exact, with no lost or doubled chars.
  2. Ctrl+Shift+1…5 switch views, and Alt+Shift+= / Alt+Shift+- split, with a terminal focused and without. Switch the Windows layout US ↔ Vietnamese while the app runs and repeat.
  3. Spawn omp, leave it at the prompt. After about 3 s the status shows `idle · Ns`. Send a prompt: `working · Ns` counts only during the turn.
  4. In a terminal: `printf '\ue0b0 \uf015 \U000f0001 ✔ 😀 abc\n'` (Git Bash), and launch omp. Expected: icons instead of boxes, monospaced Latin text.
  5. Git view: the file list and commit box sit on the right of the diff, in dark and light themes (retake the §14 screenshots).
  6. Files view: "Search files…" at the top opens quick open. Ctrl+Shift+O does the same. With the Files view switched to a task worktree, picks open the worktree's copy.
  7. ✦ in the commit box with claude, codex and omp as the default agent. Expected: a Conventional Commits message; Commit still needs a click. With an old profile without `oneshot`, the error names the line to add.
  8. ✦ in the composer rewrites the draft (Vietnamese input stays Vietnamese); Send is still manual.
  9. Confirm `omp -p --no-session` reads the prompt from stdin (item 7 with omp). If not, record the working `oneshot` line in `agents/omp.toml`.

- [ ] **Step 3: PROGRESS note.** Under "Notes for the next session", add one bullet: `2026-10-07 daily fixes (plan docs/superpowers/plans/2026-10-07-daily-fixes-and-ai-helpers.md): protocol v24 (ai.generate/ai.generated), profile key oneshot, ADR 0015/0016, ime.rs InputHandler; manual checks in HUMAN_CHECKS "2026-10-07 daily fixes".` Keep `status: DONE`. This work is outside the milestone loop.

- [ ] **Step 4: Record (no commit)**

Suggested message: `docs: manual checks and notes for the 2026-10-07 fixes`
