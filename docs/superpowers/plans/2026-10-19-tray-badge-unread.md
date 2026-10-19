# Tray icon, taskbar badge, close to tray and Mark unread — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** With the Mkrew window minimized, hidden or behind others, the user still sees how many sessions "need you" (taskbar badge, tray icon, tray menu, taskbar flash), the window's ✕ hides it to the tray instead of quitting, and a finished session can be put back into "Needs you" with **Mark unread**.

**Architecture:** A pure model in `mkrew-ui` (`attention.rs`: counts, tray menu data, change detection; `glyph.rs`: badge and dot bitmaps; `inbox.rs`: the unread mark) feeds a thin Windows layer (`desktop/win32.rs`: `ITaskbarList3` overlay, `FlashWindowEx`, `ShowWindow`, and a tray icon that owns a hidden window + message loop on its own thread). `Shell` calls the layer only when the count changes (`shell/attention.rs`). "Viewed" is UI memory today (`inbox::Viewed`, nothing in the daemon or DB), so Mark unread lives there too: **no protocol change, no migration**. macOS and Linux: every call is a no-op and the window still quits on ✕.

**Tech Stack:** Rust 2024, gpui-pre 0.3.7, `windows` 0.62 (already in the tree through gpui's Windows backend), `raw-window-handle` 0.6 (same), `async-channel` (already a dependency).

**Spec:** `docs/SPEC.md` §2.2/§2.6 (principles), §8 (daemon notifications, 30 s per session), §10 (window, title bar, sidebar), §11 (pane menu, status), §13 (palette-only actions), §14 (tokens), §15 (perf), §19 (backlog line `tray icon / taskbar badge; "mark unread"`), §20.0 (settings), §20A (attention); design decided in chat 2026-10-19, recorded in the "Design" section of this plan.

## Global Constraints

- **Numbers are reserved for planning; before creating, re-check the newest ADR/migration/PROTOCOL_VERSION and the newest SPEC §20 section letter, and take the next free one if another plan landed first (the jira-board plan already took §20I, so this plan uses §20J; if §20J is taken too, take the next free §20 letter and rename it everywhere in this plan).** This plan uses **ADR 0063 only**. Reserved migration 0020 and protocol 44 are **not used** (checked 2026-10-19: ADR max 0061, migration max 0019, `PROTOCOL_VERSION` 43 at `crates/mkrew-protocol/src/lib.rs:16`).
- Deviating from or adding to the spec needs an ADR in `docs/decisions/` AND a `docs/SPEC.md` update in the same commit (Task 1).
- SPEC §2.6 "Few dependencies. Every new crate needs a reason": this plan adds **no new crate to the tree**. `windows 0.62.2` and `raw-window-handle 0.6.2` are already in `Cargo.lock` (gpui-pre-windows 0.3.7 depends on both); mkrew-ui gets direct, Windows-only edges to them. `tray-icon` / `muda` are not in the lock and are not added.
- `unsafe_code = "deny"` (`Cargo.toml:11`). Unsafe Win32 calls live only in `crates/mkrew-ui/src/desktop/win32.rs` behind a file-level `#![allow(unsafe_code)]`, each `unsafe` block with a `// SAFETY:` line (pattern: `crates/mkrew-daemon/src/awake.rs:209`).
- No `unwrap`/`expect` outside tests (workspace clippy lints). Errors of OS calls are logged with `tracing::warn!` and never crash the UI.
- UI (skill `mkrew-ui-design`): every color comes from `crates/mkrew-ui/src/theme.rs` tokens (`Theme::status_needs_input`, `status_working`), spacing 4/8/12/16/24/32, radius 4/8/12, both themes; new menu rows reuse `menu_item` (`shell/widgets.rs:518`). Add a `docs/HUMAN_CHECKS.md` "To verify" entry (Task 9).
- Tests never call a real agent or the network. `cargo insta` is not installed (this plan adds no snapshot).
- Commits: Conventional Commits, `git -c user.email=tungnv975@gmail.com commit -m "…"`, stage **explicit paths only** (never `git add -A`; other agents have WIP in `crates/mkrew-ui/src/{run.rs,theme.rs,shell/run.rs,shell/sidebar.rs}`, `docs/HUMAN_CHECKS.md`). Where a file also holds someone else's hunks, `git add -p`. Never commit `Cargo.lock` (project rule) — but see Task 6: packaging uses `--locked`, so tell the human it needs committing.
- Verify: per-crate loop `cargo test -p <crate> <filter> -j 4`; at the end `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`. No protocol/CLI change, so the `mkrew-cli`/`mkrew-fakeagent` prebuild is not needed.
- Windows is first-class (SPEC §2.4); macOS/Linux must still compile (`desktop/none.rs`) and behave as before.
- Perf (SPEC §15): counts come from the sessions the UI already holds; the OS is touched only when a count (or the listed set) changes, never per frame.

## Review Focus

1. **Tray icon absent (Explorer not running, `Shell_NotifyIconW(NIM_ADD)` failed, non-Windows) while `close_to_tray = true`:** ✕ must still quit, never hide the window into nothing. Pinned by `hide_on_close` truth table (Task 4); the call site in Task 8.
2. **Mark unread on the pane that has the keyboard** (a right-click on a sidebar row focuses that pane first, `shell/sidebar.rs:193`): the next render must not view it again at once. Pinned by the `Viewed::view` memo test (Task 2).
3. **12 sessions need you:** the badge reads `9+`, the tray menu lists 8 lines but its tooltip and icon use the real count 12. Pinned in Tasks 4 and 5.
4. **Same count, different set** (A leaves while B enters, or `needs_input → done` of one session): the tray menu must refresh although the count did not change. Pinned by the `stamp` test (Task 4).
5. **Hidden window + something that must be visible:** a toast click, "Quit UI" with unsaved Files changes, a tray menu pick, a second launch. All go through `Shell::restore_window` (Task 8); not testable headless (`Shell` needs a real window), so it is in `docs/HUMAN_CHECKS.md` (Task 9).

## Design

Decisions (each default and why, one line):

- **"Needs you" = the Inbox definition** `inbox::needs_you` over the sessions of known workspaces (ADR 0036; tests at `sidebar/tests.rs:610` already pin chip = Inbox). Not `Sidebar::view().counts`: that one follows the sidebar's text filter and builds all rows.
- **Where `viewed` lives:** `Shell.viewed: inbox::Viewed` (`main.rs:414`), a `HashMap` in UI memory; nothing is persisted (a UI restart makes every done session unviewed again — today's behaviour). Mark unread is the same: UI memory, no daemon, no DB, no protocol. `Viewed.unread: HashMap<Ulid, i64>` keyed by `state_changed_at`, so a state change lapses the mark exactly like `seen`.
- **Mark unread is offered only for finished agents** (`kind = Agent`, `state` Done or Idle, not already in Needs you); `Exited` never counts (`SessionExited` does not touch `state_changed_at`, `sidebar.rs:486`).
- **Mark unread vs the focused pane:** `Viewed::view` remembers the last `(session, state_changed_at)` it saw and ignores a repeat; `Viewed::activated()` forgets it when the window is activated. So a mark survives while the pane keeps the keyboard, and goes when the user looks at it again (another pane and back, or returning to the window).
- **Menu / palette:** the sidebar row right-click and the pane header right-click are the same `PaneMenu` (`shell/tabs.rs:395`, `shell/menus.rs:565`) → one new item. Palette: `Extra::MarkUnread` "Agent: Mark unread" (palette-only like Broadcast, no key; keys are scarce, §13).
- **Tray implementation: `windows` crate `Shell_NotifyIconW`, own thread.** `tray-icon` (+ `muda`) are not in `Cargo.lock`; the crate adds two crates and still needs a message loop. gpui's Win32 loop on the main thread would serve a hidden window too, but its window class/proc is private and a tray tied to the main window's life is fragile; a dedicated thread (hidden top-level window, `GetMessageW` loop) is independent of gpui and survives the main window being hidden. It also receives `TaskbarCreated` (broadcast to top-level windows only), so the icon comes back after an Explorer restart.
- **Tray icon states:** idle = Mkrew's own icon (resource id 1, `crates/mkrew-ui/resources/windows/mkrew.rc:2`); working / needs you = a plain dot generated at runtime in `theme.status_working` / `status_needs_input` (no extra .ico files, both themes follow the tokens). Tooltip `Mkrew · 2 need you · 1 working`.
- **Tray menu** (built on right-click from a snapshot the UI keeps in a `Mutex`): up to `TRAY_SESSIONS = 8` lines `agent · workspace — state, age` (a `const`, not a setting: it never changes), separator, greyed `● N working`, separator, `Open Mkrew`, `Quit UI (agents keep running)`. Left click = Open.
- **Badge:** `ITaskbarList3::SetOverlayIcon` with a 3×5-pixel-font bitmap (no GDI text): amber (`status_needs_input`) rounded square with a dark digit, `9+` from 10. Description text `inbox::label(n)` for screen readers. macOS dock badge: **no-op**: gpui-pre 0.3.7 has no badge/dock-tile call (grep of `gpui-pre`, `-macos`, `-platform`, `-windows` sources: only the unrelated "taskbar/dock areas" in `window.rs:1465`), and adding `objc2-app-kit` as a direct dependency cannot be compiled or checked on this Windows machine. Blocker stated, not hidden.
- **Flash:** own `FlashWindowEx(FLASHW_TRAY | FLASHW_TIMERNOFG)` until the window comes to the foreground, `FLASHW_STOP` on activation / when nothing needs you / when the setting is switched off. gpui's `Window::request_attention` (`gpui-pre-windows window.rs:866`) is not used: it flashes once (`FLASHW_ALL`, `uCount: 1`) and has no stop. Triggered by the `session.state` event that *enters* `needs_input` while `!window.is_window_active()` (a hidden window counts as not active; the flash then does nothing visible and the tray icon carries the signal).
- **Close to tray** uses the hook that already exists: `window.on_window_should_close` (`shell/connection.rs:44`, gpui `window.rs:6658`; Windows `handle_close_msg` returns 0 = veto when the callback says false, `gpui-pre-windows events.rs:336`). gpui has **no public hide**, so the window is hidden with `ShowWindow(hwnd, SW_HIDE)` on the HWND from `Window::window_handle()` (gpui implements `HasWindowHandle` for `Window`, `window.rs:7360`). gpui reports the change through `WM_SHOWWINDOW` → `WindowVisibility::Hidden` (`events.rs:1268`, `report_visibility` is written for exactly this "ShowWindow from outside" case), so `observe_window_visibility` → `sync_panes` already detaches every pane while hidden (a perf win). Shown again with `ShowWindow(SW_SHOW | SW_RESTORE)` + `Window::activate_window()` (gpui's `activate` restores only minimized windows, not hidden ones, `window.rs:822`). Fallbacks considered: minimize (keeps the taskbar button, not "to tray"); close and reopen a new window (loses in-memory UI state — viewed set, unsaved Files text, layouts not yet saved — and `main.rs:217` quits the App when the last window closes). Not needed.
- **Quit UI** = unsaved-Files question first (window shown so it can be seen), then `cx.quit()`; the daemon and its agents are separate processes and keep running (§2.2). No palette "Quit" entry: with `close_to_tray = false` ✕ quits, with it on the tray menu quits (speculative otherwise).
- **Second launch:** the tray's hidden window class is `MkrewTray.<fnv1a of the lower-cased exe path>`; a new `mkrew.exe` first looks for that class, posts "open" to it and exits. Same exe only, so a dev build (`target\debug`) next to the installed app still runs on its own. Without this a hidden window plus a Start-menu click would create a second window and a second tray icon.
- **Settings** (hot-reloaded like the other keys: `settings_modified` mtime poll every 500 ms, `shell/connection.rs:57-87`): `[notify] flash_taskbar = true` (key lives in `[notify]` as asked; `NotifySettings` is `deny_unknown_fields` and read by the daemon too, so an older daemon reports it as an unknown key until restarted) and `[ui] close_to_tray = true`.
- **Tray exists whenever the UI runs on Windows**, independent of `close_to_tray` (the setting only governs ✕).

File structure:

| File | Responsibility |
| --- | --- |
| `crates/mkrew-core/src/settings.rs` | `NotifySettings.flash_taskbar`, `UiSettings.close_to_tray`, template, tests |
| `crates/mkrew-ui/src/inbox.rs` | `Viewed.unread`, `mark_unread`, `activated`, `view` memo, `can_mark_unread` |
| `crates/mkrew-ui/src/attention.rs` (new) | pure: `Counts`, `Tracker`, `Item`, `TrayMenu`, `entered_needs_input`, `hide_on_close` |
| `crates/mkrew-ui/src/glyph.rs` (new) | pure: badge and dot bitmaps |
| `crates/mkrew-ui/src/desktop.rs` (new) + `desktop/win32.rs` + `desktop/none.rs` | the OS layer: `Desktop`, `TrayEvent`, `instance_key`, `escape_menu` |
| `crates/mkrew-ui/src/shell/attention.rs` (new) | `impl Shell`: `sync_attention`, `restore_window`, `tray_event`, `quit_ui` |
| `crates/mkrew-ui/src/palette.rs`, `shell/{menus,tabs,navigation,files,connection,workspace}.rs`, `main.rs`, `shell.rs`, `lib.rs`, `Cargo.toml` | wiring |
| `docs/decisions/0063-…md`, `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md` | docs |

Plan size: 9 tasks, one plan file (the split threshold in the brief was ~12).

---

### Task 1: Settings keys, ADR 0063, SPEC

**Files:**
- Modify: `crates/mkrew-core/src/settings.rs:51-70` (`NotifySettings`), `:234-268` (`UiSettings`), `:367-399` (`template`), tests at `:544-613`
- Modify: `crates/mkrew-daemon/src/notify.rs:412-418` (test helper)
- Create: `docs/decisions/0063-tray-badge-close-to-tray-mark-unread.md`
- Modify: `docs/SPEC.md` (five edits below)

**Interfaces:**
- Produces: `NotifySettings.flash_taskbar: bool` (default `true`), `UiSettings.close_to_tray: bool` (default `true`). Tasks 8 reads both.

- [ ] **Step 1: Write the failing tests** — in `crates/mkrew-core/src/settings.rs` `mod tests`, add (next to `ui_diff_wrap_parses_and_defaults_to_on`, `:746`):

```rust
    #[test]
    fn flash_taskbar_and_close_to_tray_default_on_and_parse() {
        let settings = |src: &str| parse(Path::new("s.toml"), src).unwrap();
        assert!(settings("").notify.flash_taskbar);
        assert!(settings("").ui.close_to_tray);
        let off = settings("[notify]\nflash_taskbar = false\n");
        assert!(!off.notify.flash_taskbar);
        assert!(off.ui.close_to_tray, "another section's key does not change it");
        assert!(!settings("[ui]\nclose_to_tray = false\n").ui.close_to_tray);
        assert!(parse_err("[notify]\nflash_taskbar = 1\n").contains("invalid type"));
        assert!(parse_err("[ui]\nclose_to_tra = false\n").contains("unknown field `close_to_tra`"));
    }
```

and extend `full_file_parses` (`:545`): in `src` add `flash_taskbar = false` under `[notify]` (after `sound = false`) and `close_to_tray = false` under `[ui]` (after `font_size = 14`); in the expected value add `flash_taskbar: false,` to `NotifySettings { … }` and `close_to_tray: false,` to `UiSettings { … }` (after `reading_width: true,`).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-core settings -j 4`
Expected: FAIL to compile (`no field flash_taskbar on type NotifySettings`).

- [ ] **Step 3: Implement** — `settings.rs`:

`NotifySettings` (after `pub sound: bool,`):

```rust
    /// Windows: the taskbar button flashes while a session needs input and the window is not
    /// focused (SPEC §20J). The daemon ignores it; the UI reads it.
    pub flash_taskbar: bool,
```

and in its `Default` add `flash_taskbar: true,`.

`UiSettings` (after `pub reading_width: bool,`):

```rust
    /// `[ui] close_to_tray` (SPEC §20J): the window's ✕ hides it to the tray icon instead of
    /// quitting the UI (Windows, while the tray icon exists). The agents run in the daemon either way.
    pub close_to_tray: bool,
```

and in its `Default` add `close_to_tray: true,`.

`template()`: under `[notify]` add the line `     # flash_taskbar = true\n\` after `# sound = true`; before `[ui]` add a comment line and the key:

```rust
     # Close to tray (Windows): the window's ✕ hides it to the tray icon; false quits the UI.\n\
```
placed with the other `[ui]` comment lines above `[ui]\n\` (each comment line must not contain ` = `, the template test uncomments those that do), and `     # close_to_tray = true\n\` after `# reading_width = true`.

`crates/mkrew-daemon/src/notify.rs:412` — keep the helper compiling and leave the daemon's behaviour alone:

```rust
    fn switches(needs_input: bool, done: bool, sound: bool) -> NotifySettings {
        NotifySettings {
            needs_input,
            done,
            sound,
            ..NotifySettings::default()
        }
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-core settings -j 4` then `cargo test -p mkrew-daemon notify -j 4`
Expected: PASS (`the_template_parses_to_the_defaults_commented_or_not` included).

- [ ] **Step 5: Write ADR 0063** — create `docs/decisions/0063-tray-badge-close-to-tray-mark-unread.md`:

```markdown
# 0063 — Tray icon, taskbar badge, close to tray and Mark unread

- Date: 2026-10-19
- Status: accepted (extends ADR 0036; SPEC §10, §11, §19, §20.0, new §20J)
- Milestone: none (user request 2026-10-19; SPEC §19 backlog line "tray icon / taskbar badge; mark unread" becomes a feature)

## Context

The user often minimizes or closes the window and misses agents that wait for input: the OS toast
fires once per session per 30 s (SPEC §8) and vanishes. "Needs you" (ADR 0036: needs input, plus
finished and not yet viewed) is only visible inside the window. "Viewed" is UI memory
(`inbox::Viewed`); the daemon and the database know nothing about it.

## Decision

- **Taskbar badge (Windows):** the main window's overlay icon (`ITaskbarList3::SetOverlayIcon`)
  shows the Needs-you count, `9+` from 10, cleared at 0. **Flash (Windows):** `FlashWindowEx` while a
  session entered `needs_input` and the window is not focused; `[notify] flash_taskbar` (default
  true) turns it off.
- **Tray icon (Windows)**, owned by the UI process on its own thread: states idle / working /
  needs you, a menu with up to 8 sessions that need you (click opens the pane), `● N working`,
  Open Mkrew, Quit UI (agents keep running).
- **Close to tray:** `[ui] close_to_tray` (default true): ✕ / Alt+F4 hide the window
  (`ShowWindow(SW_HIDE)` on the window's HWND, vetoing gpui's close) while the tray icon exists;
  the tray, a toast click or a second launch of the same exe shows it. Quit UI asks about unsaved
  Files changes first. The daemon keeps the agents running anyway (SPEC §2.2).
- **Mark unread:** pane menu (also the sidebar row's) and palette "Agent: Mark unread" put a
  finished agent session back into Needs you until its pane is focused again or it changes state.
  UI memory only, like "viewed". **No protocol change, no migration.**
- macOS and Linux: no badge, no tray, no flash; ✕ quits as before.

## Alternatives

- `tray-icon` + `muda` crates: not in `Cargo.lock`, two more crates (SPEC §2.6) and still a message
  loop to own. `Shell_NotifyIconW` through the `windows` crate gpui already builds with is ~150 lines.
- Tray on gpui's own Win32 loop (subclassing its window): its window procedure is private and a
  tray tied to the main window is fragile; a dedicated thread is independent.
- Hide by minimizing: the taskbar button stays, not "to tray". Close and reopen a new window: loses
  in-memory UI state and `main.rs` quits the App when the last window closes. gpui has no public
  hide, so the HWND is hidden directly; gpui already reports `WM_SHOWWINDOW` as window visibility.
- gpui `Window::request_attention`: flashes once (`FLASHW_ALL`, count 1), no stop.
- Persist viewed / unread in the daemon (migration + protocol 44): "viewed" is not persisted today;
  persisting only the unread half would be inconsistent. Rejected.
- macOS dock badge: gpui-pre 0.3.7 has no call; `objc2-app-kit` would be a new direct dependency
  that cannot be built or checked here. No-op until someone can test it on a Mac.

## Consequences

- ✕ no longer quits on Windows: tray → "Quit UI", or `close_to_tray = false`. If the tray icon
  could not be created, ✕ still quits (never hide into nothing).
- mkrew-ui gets Windows-only direct dependencies on `windows 0.62` and `raw-window-handle 0.6`
  (both already in the tree); `Cargo.lock` gains edges, no new package. `unsafe` is confined to
  `crates/mkrew-ui/src/desktop/win32.rs`.
- A hidden window detaches every pane (SPEC §10), so a tray-resident UI costs almost nothing.
- Starting `mkrew.exe` again shows the running window; a dev build at another path stays independent.
- `NotifySettings` is `deny_unknown_fields` and shared with the daemon: a daemon older than this
  change reports `flash_taskbar` as an unknown key until it is restarted.
- Explorer restarts: the tray icon is re-added on `TaskbarCreated`; the overlay is set again on the
  next count change or when the window is shown.
```

- [ ] **Step 6: SPEC edits** — `docs/SPEC.md` (five exact edits):

1. §10 title bar, line 810: replace `all edges resize; the controls are hidden in full` with `all edges resize; ✕ and Alt+F4 hide the window to the tray while \`[ui] close_to_tray\` is on (§20J); the controls are hidden in full`.
2. §11 pane menu, line 981: replace `greyed for a run pane) and **Change color**` with `greyed for a run pane), **Mark unread** (a finished agent goes back to Needs you until its pane is focused again, §20J; greyed otherwise) and **Change color**`.
3. §20.0 settings block: after the line `sound = true                # notification plays the OS default sound` add
   `flash_taskbar = true         # §20J; Windows: the taskbar button flashes while a session needs input and the window is not focused`
   and after the line `sidebar_width = 280 …` add
   `close_to_tray = true         # §20J; Windows: ✕ hides the window to the tray icon (false: ✕ quits the UI)`.
4. §19 backlog (line 1641): replace `tray icon / taskbar badge; "mark unread" in the Inbox; remote workspaces` with `remote workspaces`.
5. Append at the end of SPEC.md (after the last section, §20I if the jira-board plan has landed, else §20H):

```markdown

### §20J Tray, taskbar badge and mark unread (outside the milestones, ADR 0063)

Notice who needs you with the window minimized, hidden or behind others. "Needs you" is the
sidebar chip's count (§10, ADR 0036): needs input, plus finished and not yet viewed.

1. **Taskbar badge (Windows).** The main window's taskbar button shows an overlay icon with that
   count (`ITaskbarList3::SetOverlayIcon`; amber, `9+` from 10; description "3 need you");
   cleared at 0. macOS and Linux: nothing (gpui has no dock-badge call).
2. **Flash (Windows).** When a session enters `needs_input` while the window is not focused, the
   taskbar button flashes until the window is focused (`FlashWindowEx`); it stops on focus and when
   nothing needs you any more. `[notify] flash_taskbar = false` turns it off (applies live).
3. **Tray icon (Windows).** Owned by the UI process, shown while it runs. Icon: Mkrew's own while
   nothing works or waits, a dot in the working color while an agent works, a dot in the needs-input
   color while something needs you; tooltip `Mkrew · 2 need you · 1 working`. Right-click menu: up to
   8 sessions that need you, newest first, `agent · workspace — state, age` (a click shows the window
   and opens that pane); a greyed `● N working`; `Open Mkrew`; `Quit UI (agents keep running)`. A
   left click opens the window.
4. **Close to tray (Windows).** `[ui] close_to_tray = true` (default): ✕ and Alt+F4 hide the window
   instead of closing it (no pane is attached while hidden, §10). The tray menu, a toast click and
   starting Mkrew again (same exe) show it. `false`, or no tray icon: ✕ quits the UI as before
   (unsaved Files changes ask first). `Quit UI` shows the window if unsaved Files changes need an
   answer, then ends the UI; the daemon and every agent keep running (§2.2).
5. **Mark unread.** Pane menu (right-click a split pane's header or a sidebar session row) **Mark
   unread** and palette "Agent: Mark unread" (no key) put a finished agent session (done, or idle
   after work) back into Needs you — chip, row tint, Inbox, badge, tray — until its pane is focused
   again in the active window or its state changes. Greyed for a session that already needs you,
   works, or is a terminal. Not saved: like "viewed" it lives in UI memory.
6. **Cost.** Counts come from the sessions the UI already holds; the badge, the icon and the menu
   are updated only when the count (or the listed set) changes, never per frame.
```

- [ ] **Step 7: Commit**

```bash
git add docs/decisions/0063-tray-badge-close-to-tray-mark-unread.md crates/mkrew-core/src/settings.rs crates/mkrew-daemon/src/notify.rs
git add -p docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(core): flash_taskbar and close_to_tray settings; ADR 0063, SPEC §20J"
```

---

### Task 2: Mark unread model (`inbox.rs`)

**Files:**
- Modify: `crates/mkrew-ui/src/inbox.rs:27-78` (`Viewed`, `needs_you`), tests module (`:162-398`)

**Interfaces:**
- Consumes: existing `Viewed`, `needs_you`, `entries` (`inbox.rs:30,68,82`).
- Produces: `Viewed::view(&mut self, &Session) -> bool` (was `()`; `true` = something that decides Needs you changed), `Viewed::activated(&mut self)`, `Viewed::mark_unread(&mut self, &Session)`, `pub fn can_mark_unread(session: &Session, viewed: &Viewed) -> bool`. Callers of `view` ignoring the result (`main.rs:636`, `sidebar/tests.rs:599`, inbox tests) keep compiling.

- [ ] **Step 1: Write the failing tests** — append to `mod tests` in `inbox.rs` (it already has `project`, `session`, `ids` helpers):

```rust
    #[test]
    fn mark_unread_puts_a_viewed_done_back_until_it_is_viewed_again() {
        let api = project("api");
        let done = session(&api, SessionState::Done, 5);
        let other = session(&api, SessionState::Working, 1);
        let projects = [api];
        let mut viewed = Viewed::default();
        assert!(viewed.view(&done));
        assert!(entries(&projects, &[done.clone()], &viewed).is_empty());
        assert!(can_mark_unread(&done, &viewed));

        viewed.mark_unread(&done);
        assert_eq!(entries(&projects, &[done.clone()], &viewed).len(), 1);
        assert!(!can_mark_unread(&done, &viewed), "it already needs you");

        // The pane keeps the keyboard: looking again changes nothing, so the mark stays.
        assert!(!viewed.view(&done));
        assert_eq!(entries(&projects, &[done.clone()], &viewed).len(), 1);

        // Another pane, then back: viewed again.
        viewed.view(&other);
        assert!(viewed.view(&done));
        assert!(entries(&projects, &[done], &viewed).is_empty());
    }

    #[test]
    fn activating_the_window_looks_at_the_focused_pane_again() {
        let api = project("api");
        let done = session(&api, SessionState::Done, 5);
        let mut viewed = Viewed::default();
        viewed.view(&done);
        viewed.mark_unread(&done);
        assert!(needs_you(&done, &viewed));
        viewed.activated();
        assert!(viewed.view(&done), "the mark was dropped");
        assert!(!needs_you(&done, &viewed));
    }

    #[test]
    fn a_mark_lapses_with_the_state_and_never_lists_an_exited_session() {
        let api = project("api");
        let mut idle = session(&api, SessionState::Idle, 5);
        let mut viewed = Viewed::default();
        assert!(!needs_you(&idle, &viewed), "idle without work is not Needs you");
        assert!(can_mark_unread(&idle, &viewed));
        viewed.mark_unread(&idle);
        assert!(needs_you(&idle, &viewed));

        // `session.exited` changes the state but not `state_changed_at`.
        idle.state = SessionState::Exited;
        assert!(!needs_you(&idle, &viewed));

        // Working again (a new state): the mark lapses.
        idle.state = SessionState::Working;
        idle.state_changed_at = 6;
        assert!(!needs_you(&idle, &viewed));
    }

    #[test]
    fn only_finished_agents_can_be_marked_unread() {
        let api = project("api");
        let viewed = Viewed::default();
        let can = |state| can_mark_unread(&session(&api, state, 1), &viewed);
        assert!(can(SessionState::Idle));
        assert!(!can(SessionState::Working));
        assert!(!can(SessionState::NeedsInput), "already needs you");
        assert!(!can(SessionState::Starting));
        assert!(!can(SessionState::Exited));
        // Done and not viewed already needs you; a terminal never does.
        assert!(!can(SessionState::Done));
        let shell = Session {
            kind: SessionKind::Shell,
            ..session(&api, SessionState::Idle, 1)
        };
        assert!(!can_mark_unread(&shell, &viewed));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-ui inbox -j 4`
Expected: FAIL to compile (`no method named mark_unread`, `can_mark_unread` not found, `view` returns `()`).

- [ ] **Step 3: Implement** — in `inbox.rs` replace the `Viewed` struct and its `view` (`:27-42`) and `needs_you` (`:65-78`):

```rust
/// Which finishes the user has seen: a finished session leaves the Inbox once its pane was
/// focused in the active window; a later finish (new `state_changed_at`) shows it again.
/// "Mark unread" (ADR 0063) puts one back until it is looked at again. UI memory only.
#[derive(Debug, Default)]
pub struct Viewed {
    /// Session → `state_changed_at` of the state it was last viewed in.
    seen: HashMap<Ulid, i64>,
    /// Agent session → `state_changed_at` of its `working → idle`: an agent without a `done`
    /// hook (or whose `done` decayed through new output) finishes this way.
    quiet: HashMap<Ulid, i64>,
    /// Session → `state_changed_at` of the state it was marked unread in; a state change
    /// lapses it, like `seen`.
    unread: HashMap<Ulid, i64>,
    /// The (session, `state_changed_at`) [`Viewed::view`] saw last. Looking at the same state
    /// again changes nothing, so a pane marked unread while it has the keyboard stays unread.
    last: Option<(Ulid, i64)>,
}

impl Viewed {
    /// `session` is on screen and focused. `true` if that changed what needs the user.
    pub fn view(&mut self, session: &Session) -> bool {
        let state = (session.id, session.state_changed_at);
        if self.last.replace(state) == Some(state) {
            return false;
        }
        let seen = self.seen.insert(session.id, session.state_changed_at);
        let marked = self.unread.remove(&session.id).is_some();
        seen != Some(session.state_changed_at) || marked
    }

    /// The window was activated: the focused pane is looked at again, even one marked unread.
    pub fn activated(&mut self) {
        self.last = None;
    }

    /// "Mark unread": `session` needs the user again until it is viewed (see [`can_mark_unread`]).
    pub fn mark_unread(&mut self, session: &Session) {
        self.unread.insert(session.id, session.state_changed_at);
    }

    /// Notes an agent going quiet after work. Call before the sidebar applies `event`, so
    /// `sessions` still holds the previous state.
    pub fn apply(&mut self, event: &Event, sessions: &[Session]) {
        let Event::SessionState(event) = event else {
            return;
        };
        let was_working = sessions.iter().any(|s| {
            s.id == event.session_id
                && s.kind == SessionKind::Agent
                && s.state == SessionState::Working
        });
        if event.state == SessionState::Idle && was_working {
            self.quiet.insert(event.session_id, event.changed_at);
        }
    }

    fn seen(&self, session: &Session) -> bool {
        self.seen.get(&session.id) == Some(&session.state_changed_at)
    }

    fn marked(&self, session: &Session) -> bool {
        self.unread.get(&session.id) == Some(&session.state_changed_at)
    }
}

/// A session that needs the user: it asks for input, it finished (done, or idle right after
/// working) and its pane was not focused since (a later finish shows again), or it was marked
/// unread and not viewed since. The one definition behind the Inbox entries, the sidebar chip,
/// its highlighted rows and the sidebar footer, the taskbar badge and the tray.
pub fn needs_you(session: &Session, viewed: &Viewed) -> bool {
    // An exit changes the state but not `state_changed_at`: an exited session never counts.
    if session.state != SessionState::Exited && viewed.marked(session) {
        return true;
    }
    match session.state {
        SessionState::NeedsInput => true,
        SessionState::Done => !viewed.seen(session),
        SessionState::Idle => {
            viewed.quiet.get(&session.id) == Some(&session.state_changed_at)
                && !viewed.seen(session)
        }
        _ => false,
    }
}

/// Whether "Mark unread" applies: a finished agent (done or idle) that is not already in Needs you.
pub fn can_mark_unread(session: &Session, viewed: &Viewed) -> bool {
    session.kind == SessionKind::Agent
        && matches!(session.state, SessionState::Done | SessionState::Idle)
        && !needs_you(session, viewed)
}
```

Also change the module doc's first sentence list to mention the mark: append `, or marked unread` after `and not yet viewed`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-ui inbox -j 4` then `cargo test -p mkrew-ui sidebar -j 4`
Expected: PASS (the old inbox and sidebar tests included).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/inbox.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Viewed remembers sessions marked unread (ADR 0063)"
```

---

### Task 3: Mark unread in the pane menu and the palette

**Files:**
- Modify: `crates/mkrew-ui/src/palette.rs:51-141` (`Context`, `Extra`), `crates/mkrew-ui/src/palette/tests.rs:6-25` and new test
- Modify: `crates/mkrew-ui/src/shell/menus.rs:591-602` (pane menu), `crates/mkrew-ui/src/shell/tabs.rs:344` (after `pin_session`), `crates/mkrew-ui/src/shell/navigation.rs:370-387,479-547`, `crates/mkrew-ui/src/shell/connection.rs:39-40`

**Interfaces:**
- Consumes: `inbox::can_mark_unread`, `Viewed::mark_unread`, `Viewed::activated` (Task 2).
- Produces: `palette::Context.markable: bool`, `Extra::MarkUnread` (`ALL` is now 17 long), `Shell::can_mark_unread(&self, Ulid) -> bool`, `Shell::mark_unread(&mut self, Ulid, &mut Context<Self>)` (Task 8 adds the OS refresh to it).

- [ ] **Step 1: Write the failing test** — `palette/tests.rs`: add `markable: false,` to `context()` (after `agent_pane: false,`, `:17`) and add:

```rust
#[test]
fn mark_unread_is_listed_only_for_a_session_that_can_be_marked() {
    assert!(!Extra::MarkUnread.available(&context(View::Terminals)));
    let marked = Context {
        markable: true,
        ..context(View::Terminals)
    };
    assert!(Extra::MarkUnread.available(&marked));
    let entry = entries(&Keymap::defaults(), &marked, &[], &[], &[])
        .into_iter()
        .find(|entry| entry.action == Action::Extra(Extra::MarkUnread))
        .expect("listed");
    assert_eq!(entry.title, "Agent: Mark unread");
    assert_eq!(entry.shortcut, "", "palette-only: no key is bound");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-ui palette -j 4`
Expected: FAIL to compile (`no field markable`, `no variant MarkUnread`).

- [ ] **Step 3: Implement the palette part** — `palette.rs`:

In `Context` after `pub agent_pane: bool,`:

```rust
    /// The focused pane's session can be marked unread (a finished agent, `inbox::can_mark_unread`).
    pub markable: bool,
```

In `enum Extra` add `MarkUnread,` after `PhoneAccess,`; `pub const ALL: [Self; 17]` with `Self::MarkUnread,` appended; `title`: `Self::MarkUnread => "Agent: Mark unread",`; `available`: `Self::MarkUnread => context.markable,` (put before the `Self::OpenSettings | Self::PhoneAccess => true` arm).

- [ ] **Step 4: Implement the shell part**

`shell/tabs.rs` — after `pin_session` (`:344`):

```rust
    /// Whether "Mark unread" applies to `session_id` (a finished agent not already in Needs you).
    pub(crate) fn can_mark_unread(&self, session_id: Ulid) -> bool {
        (self.sidebar.sessions().iter())
            .find(|s| s.id == session_id)
            .is_some_and(|session| inbox::can_mark_unread(session, &self.viewed))
    }

    /// "Mark unread" (pane menu, palette): the finished agent session is back in Needs you until
    /// its pane is focused again (ADR 0063). UI memory only, like "viewed".
    pub(crate) fn mark_unread(&mut self, session_id: Ulid, cx: &mut Context<'_, Self>) {
        self.pane_menu = None;
        if let Some(session) = (self.sidebar.sessions().iter())
            .find(|s| s.id == session_id)
            .filter(|session| inbox::can_mark_unread(session, &self.viewed))
        {
            self.viewed.mark_unread(session);
        }
        cx.notify();
    }
```

`shell/menus.rs` — in `render_pane_menu`, after `let pinned = session.pinned;` (`:579`) add `let markable = inbox::can_mark_unread(session, &self.viewed);` and insert between the Pin item (ends `:601`) and `.child(separator())` (`:602`):

```rust
            .child(
                menu_item("pane-menu-unread".into(), theme)
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(theme.text))
                            .child("Mark unread"),
                    )
                    .when(markable, |el| {
                        el.on_click(cx.listener(move |shell, _, _, cx| {
                            shell.mark_unread(session_id, cx);
                        }))
                    })
                    .when(!markable, |el| el.opacity(opacity::DISABLED)),
            )
```

`shell/navigation.rs` — in `open_palette`'s `palette::Context { … }` (`:370`), after `agent_pane: self.focused_agent().is_some(),` add:

```rust
            markable: self
                .focused_session()
                .is_some_and(|id| self.can_mark_unread(id)),
```

and in `run_palette_pick`'s `match extra` add, before the `// Handled above: it needs no workspace.` comment (`:544`):

```rust
                    Extra::MarkUnread => {
                        if let Some(session) = self.focused_session() {
                            self.mark_unread(session, cx);
                        }
                    }
```

`shell/connection.rs:39-40` — replace the activation observer:

```rust
        // Coming back to the window views the focused pane (a done session leaves the Inbox),
        // even one marked unread while it kept the keyboard.
        cx.observe_window_activation(window, |shell, window, cx| {
            if window.is_window_active() {
                shell.viewed.activated();
            }
            cx.notify();
        })
        .detach();
```

- [ ] **Step 5: Run to verify it passes and builds**

Run: `cargo test -p mkrew-ui palette -j 4` then `cargo build -p mkrew-ui -j 4`
Expected: PASS and a clean build (the exhaustive `match extra` and the `Context` literal compile).

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-ui/src/palette.rs crates/mkrew-ui/src/palette/tests.rs crates/mkrew-ui/src/shell/menus.rs crates/mkrew-ui/src/shell/tabs.rs crates/mkrew-ui/src/shell/navigation.rs
git add -p crates/mkrew-ui/src/shell/connection.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Mark unread in the pane menu and the palette (ADR 0063)"
```

---

### Task 4: The pure attention model (`attention.rs`)

**Files:**
- Create: `crates/mkrew-ui/src/attention.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod attention;` after `pub mod ask;`, update the doc comment's list with "the taskbar / tray attention model")

**Interfaces:**
- Consumes: `inbox::{needs_you, entries, label}`, `sidebar::status_display(state).label`, `status::format_duration`.
- Produces (Tasks 5, 6, 7, 8 rely on these exact names):
  - `pub const TRAY_SESSIONS: usize = 8;`
  - `pub enum Icon { Idle, Working, NeedsYou }`
  - `pub struct Counts { pub needs_you: usize, pub working: usize, .. }` (+ private `stamp`), `Counts::icon(self) -> Icon`, `Default`, `Copy`, `PartialEq`
  - `pub fn counts(&[Project], &[Session], &Viewed) -> Counts`
  - `pub struct Item { pub session_id: Ulid, pub agent: String, pub workspace: String, pub state: SessionState, pub since: i64 }`, `Item::text(&self, now_ms: i64) -> String`
  - `pub struct TrayMenu { pub needs_you: usize, pub working: usize, pub items: Vec<Item> }` (`Default`, `Clone`), `TrayMenu::tip(&self) -> String`
  - `pub fn menu(&[Project], &[Session], &Viewed, Counts) -> TrayMenu`
  - `pub struct Tracker` (`Default`), `Tracker::update(&mut self, Counts) -> Change`, `pub struct Change { pub badge: bool, pub tray: bool }`
  - `pub fn entered_needs_input(&Event, &[Session]) -> bool`
  - `pub fn hide_on_close(close_to_tray: bool, has_tray: bool) -> bool`

- [ ] **Step 1: Write the failing tests** — create `crates/mkrew-ui/src/attention.rs` with only the test module first (the items above are added in Step 3; this file is then completed):

```rust
#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionStateEvent, StateSource};

    use super::*;

    fn project(name: &str) -> Project {
        Project {
            id: Ulid::new(),
            name: name.to_owned(),
            path: format!("C:\\src\\{name}"),
            run_commands: Vec::new(),
            run_autostart: false,
            default_agent: None,
            default_shell: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
            color: None,
            pinned: false,
        }
    }

    fn session(project: &Project, state: SessionState, changed_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: project.id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Hook,
            state_changed_at: changed_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
        }
    }

    fn state_event(session: &Session, state: SessionState) -> Event {
        Event::SessionState(SessionStateEvent {
            session_id: session.id,
            state,
            source: StateSource::Hook,
            changed_at: 9,
        })
    }

    #[test]
    fn counts_are_the_inbox_and_the_working_agents() {
        let api = project("api");
        let asking = session(&api, SessionState::NeedsInput, 1);
        let done = session(&api, SessionState::Done, 2);
        let seen = session(&api, SessionState::Done, 3);
        let working = session(&api, SessionState::Working, 4);
        let terminal = Session {
            kind: SessionKind::Shell,
            ..session(&api, SessionState::Working, 5)
        };
        let orphan = session(&project("gone"), SessionState::NeedsInput, 6);
        let mut viewed = Viewed::default();
        viewed.view(&seen);
        let all = [asking, done, seen, working, terminal, orphan];

        let counts = counts(std::slice::from_ref(&api), &all, &viewed);
        assert_eq!((counts.needs_you, counts.working), (2, 1));
        assert_eq!(
            counts.needs_you,
            inbox::entries(std::slice::from_ref(&api), &all, &viewed).len(),
            "the badge counts exactly what the Inbox lists"
        );
        assert_eq!(counts.icon(), Icon::NeedsYou);
    }

    #[test]
    fn the_icon_follows_the_most_urgent_state() {
        let api = project("api");
        let viewed = Viewed::default();
        let icon = |sessions: &[Session]| counts(std::slice::from_ref(&api), sessions, &viewed).icon();
        assert_eq!(icon(&[]), Icon::Idle);
        assert_eq!(icon(&[session(&api, SessionState::Working, 1)]), Icon::Working);
        let both = [
            session(&api, SessionState::Working, 1),
            session(&api, SessionState::NeedsInput, 2),
        ];
        assert_eq!(icon(&both), Icon::NeedsYou);
    }

    #[test]
    fn the_tracker_reports_only_what_changed() {
        let api = project("api");
        let viewed = Viewed::default();
        let projects = [api.clone()];
        let mut tracker = Tracker::default();
        let none = counts(&projects, &[], &viewed);
        assert_eq!(tracker.update(none), Change::default(), "nothing yet, nothing to show");

        let asking = [session(&api, SessionState::NeedsInput, 1)];
        let one = counts(&projects, &asking, &viewed);
        assert_eq!(tracker.update(one), Change { badge: true, tray: true });
        assert_eq!(tracker.update(one), Change::default(), "same counts: no OS call");

        let working = [asking[0].clone(), session(&api, SessionState::Working, 2)];
        let more = counts(&projects, &working, &viewed);
        assert_eq!(
            tracker.update(more),
            Change { badge: false, tray: true },
            "a working agent changes the tray, not the badge"
        );
    }

    #[test]
    fn the_same_count_with_another_set_still_refreshes_the_tray() {
        let api = project("api");
        let viewed = Viewed::default();
        let projects = [api.clone()];
        let mut a = session(&api, SessionState::NeedsInput, 1);
        let mut b = session(&api, SessionState::Working, 2);
        let mut tracker = Tracker::default();
        tracker.update(counts(&projects, &[a.clone(), b.clone()], &viewed));

        // A leaves, B enters: still one session needs you, and one works.
        a.state = SessionState::Working;
        a.state_changed_at = 3;
        b.state = SessionState::NeedsInput;
        b.state_changed_at = 3;
        let change = tracker.update(counts(&projects, &[a.clone(), b.clone()], &viewed));
        assert_eq!(change, Change { badge: false, tray: true });

        // The same session asks, then finishes: one needs you either way.
        b.state = SessionState::Done;
        b.state_changed_at = 4;
        let change = tracker.update(counts(&projects, &[a, b], &viewed));
        assert!(change.tray && !change.badge);
    }

    #[test]
    fn twelve_waiting_sessions_list_eight_and_count_twelve() {
        let api = project("api");
        let sessions: Vec<Session> = (1..=12)
            .map(|at| session(&api, SessionState::NeedsInput, at))
            .collect();
        let viewed = Viewed::default();
        let projects = [api];
        let counts = counts(&projects, &sessions, &viewed);
        let menu = menu(&projects, &sessions, &viewed, counts);
        assert_eq!(menu.needs_you, 12);
        assert_eq!(menu.items.len(), TRAY_SESSIONS);
        assert_eq!(menu.items[0].since, 12, "newest first");
        assert_eq!(menu.items[7].since, 5);
    }

    #[test]
    fn a_menu_line_names_agent_workspace_state_and_age() {
        let item = Item {
            session_id: Ulid::new(),
            agent: "claude".to_owned(),
            workspace: "api".to_owned(),
            state: SessionState::NeedsInput,
            since: 1_000,
        };
        assert_eq!(item.text(181_000), "claude · api — needs input, 3m");
        let done = Item {
            state: SessionState::Done,
            ..item
        };
        assert_eq!(done.text(13_000), "claude · api — done, 12s");
    }

    #[test]
    fn the_tooltip_names_both_counts() {
        let menu = |needs_you, working| TrayMenu {
            needs_you,
            working,
            items: Vec::new(),
        };
        assert_eq!(menu(2, 1).tip(), "Mkrew · 2 need you · 1 working");
        assert_eq!(menu(0, 0).tip(), "Mkrew · Inbox clear · 0 working");
        assert_eq!(menu(1, 3).tip(), "Mkrew · 1 needs you · 3 working");
    }

    #[test]
    fn only_entering_needs_input_counts_for_the_flash() {
        let api = project("api");
        let working = session(&api, SessionState::Working, 1);
        let asking = session(&api, SessionState::NeedsInput, 1);
        let sessions = [working.clone(), asking.clone()];
        assert!(entered_needs_input(&state_event(&working, SessionState::NeedsInput), &sessions));
        assert!(
            !entered_needs_input(&state_event(&asking, SessionState::NeedsInput), &sessions),
            "already asking"
        );
        assert!(!entered_needs_input(&state_event(&working, SessionState::Done), &sessions));
        let unknown = session(&api, SessionState::Working, 1);
        assert!(
            entered_needs_input(&state_event(&unknown, SessionState::NeedsInput), &sessions),
            "a session the list does not know yet"
        );
    }

    #[test]
    fn closing_hides_only_while_the_tray_icon_is_there() {
        assert!(hide_on_close(true, true));
        assert!(!hide_on_close(true, false), "no tray icon: ✕ quits, never hide into nothing");
        assert!(!hide_on_close(false, true));
        assert!(!hide_on_close(false, false));
    }
}
```

- [ ] **Step 2: Register the module and run to verify it fails**

Add `pub mod attention;` to `crates/mkrew-ui/src/lib.rs` (alphabetical, after `pub mod ask;`).

Run: `cargo test -p mkrew-ui attention -j 4`
Expected: FAIL to compile (`cannot find function counts`, …).

- [ ] **Step 3: Implement** — put this above the test module in `attention.rs`:

```rust
//! What the OS shell shows about the Inbox (SPEC §20J, ADR 0063): the Needs-you and working
//! counts, the tray menu's lines, and which of them changed since the last look, so the taskbar
//! badge, the tray icon and its menu are touched only on a change. Pure and tested headlessly.

use mkrew_protocol::{Event, Project, Session, SessionKind, SessionState};
use ulid::Ulid;

use crate::inbox::{self, Viewed};
use crate::sidebar::status_display;
use crate::status::format_duration;

/// Most sessions the tray menu lists (the count and the badge still show all of them).
pub const TRAY_SESSIONS: usize = 8;

/// The tray icon's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Nothing works or waits.
    Idle,
    /// An agent works.
    Working,
    /// Something needs the user.
    NeedsYou,
}

/// The numbers behind the badge, the icon and the tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// [`inbox::needs_you`] sessions of known workspaces: exactly what the Inbox lists.
    pub needs_you: usize,
    /// Agent sessions working (the sidebar's `Working` chip).
    pub working: usize,
    /// Changes whenever the *set* of Needs-you sessions or any of their states changes, even
    /// when `needs_you` does not (one leaves as another enters).
    stamp: u64,
}

impl Counts {
    pub fn icon(self) -> Icon {
        if self.needs_you > 0 {
            Icon::NeedsYou
        } else if self.working > 0 {
            Icon::Working
        } else {
            Icon::Idle
        }
    }
}

/// One pass over the sessions, no allocation.
pub fn counts(projects: &[Project], sessions: &[Session], viewed: &Viewed) -> Counts {
    let mut counts = Counts::default();
    let known = |session: &&Session| projects.iter().any(|p| p.id == session.project_id);
    for session in sessions.iter().filter(known) {
        if session.kind == SessionKind::Agent && session.state == SessionState::Working {
            counts.working += 1;
        }
        if inbox::needs_you(session, viewed) {
            counts.needs_you += 1;
            let id = (session.id.0 as u64) ^ ((session.id.0 >> 64) as u64);
            counts.stamp = counts
                .stamp
                .wrapping_add(id ^ (session.state_changed_at as u64));
        }
    }
    counts
}

/// One tray menu line: a session that needs the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub session_id: Ulid,
    /// Agent profile id.
    pub agent: String,
    /// Workspace name.
    pub workspace: String,
    pub state: SessionState,
    /// Unix ms of the state change that put the session in Needs you.
    pub since: i64,
}

impl Item {
    /// `claude · api — needs input, 3m` (the age is read when the menu opens).
    pub fn text(&self, now_ms: i64) -> String {
        format!(
            "{} · {} — {}, {}",
            self.agent,
            self.workspace,
            status_display(self.state).label,
            format_duration(now_ms - self.since)
        )
    }
}

/// What the tray's tooltip and menu show; the UI keeps the newest copy for the tray thread.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayMenu {
    pub needs_you: usize,
    pub working: usize,
    /// Newest first, at most [`TRAY_SESSIONS`].
    pub items: Vec<Item>,
}

impl TrayMenu {
    /// `Mkrew · 2 need you · 1 working`
    pub fn tip(&self) -> String {
        format!(
            "Mkrew · {} · {} working",
            inbox::label(self.needs_you),
            self.working
        )
    }
}

/// The tray menu for `counts` (built only when [`Tracker::update`] says the tray changed).
pub fn menu(
    projects: &[Project],
    sessions: &[Session],
    viewed: &Viewed,
    counts: Counts,
) -> TrayMenu {
    let items = inbox::entries(projects, sessions, viewed)
        .into_iter()
        .take(TRAY_SESSIONS)
        .map(|entry| Item {
            session_id: entry.session_id,
            agent: entry.agent,
            workspace: entry.workspace,
            state: entry.state,
            since: entry.state_changed_at,
        })
        .collect();
    TrayMenu {
        needs_you: counts.needs_you,
        working: counts.working,
        items,
    }
}

/// What to update in the OS after a recount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Change {
    /// The Needs-you number moved: the taskbar badge.
    pub badge: bool,
    /// Anything the tray shows moved: icon, tooltip, menu.
    pub tray: bool,
}

/// Remembers the last [`Counts`] so only a change reaches the OS.
#[derive(Debug, Default)]
pub struct Tracker {
    last: Counts,
}

impl Tracker {
    pub fn update(&mut self, now: Counts) -> Change {
        let last = std::mem::replace(&mut self.last, now);
        Change {
            badge: now.needs_you != last.needs_you,
            tray: now != last,
        }
    }
}

/// Whether `event` moves a session *into* `needs_input` (the taskbar flash trigger). Call before
/// the sidebar applies the event, so `sessions` still holds the previous state.
pub fn entered_needs_input(event: &Event, sessions: &[Session]) -> bool {
    let Event::SessionState(event) = event else {
        return false;
    };
    event.state == SessionState::NeedsInput
        && sessions
            .iter()
            .find(|s| s.id == event.session_id)
            .is_none_or(|s| s.state != SessionState::NeedsInput)
}

/// Whether the window's ✕ hides it: only with the setting on and a tray icon to bring it back.
pub fn hide_on_close(close_to_tray: bool, has_tray: bool) -> bool {
    close_to_tray && has_tray
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-ui attention -j 4`
Expected: PASS. (`status_display(NeedsInput).label` must read `needs input` and `Done` → `done`: `sidebar.rs:38`; if a label differs, change the expected strings in the two text tests to the labels `status_display` returns, not the code.)

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/attention.rs
git add -p crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): attention model for the taskbar and tray (ADR 0063)"
```

---

### Task 5: Badge and tray-dot bitmaps (`glyph.rs`)

**Files:**
- Create: `crates/mkrew-ui/src/glyph.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod glyph;` after `pub mod git_view;`... alphabetical: after `pub mod gutter;`? `glyph` < `gutter`: place before `pub mod gutter;`)

**Interfaces:**
- Produces: `pub struct Bitmap { pub size: u32, pub bgra: Vec<u8> }` with `Bitmap::pixel(&self, x, y) -> [u8; 4]` (BGRA); `pub fn badge_text(count: usize) -> String`; `pub fn badge(count: usize, size: u32, bg: u32, ink: u32) -> Bitmap`; `pub fn dot(size: u32, rgb: u32) -> Bitmap`. `bg`/`ink`/`rgb` are `0xRRGGBB` like the theme tokens. Tasks 6 and 7 consume them.

- [ ] **Step 1: Write the failing tests** — create `glyph.rs` starting with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const AMBER: u32 = 0xF5A623;
    const INK: u32 = 0x141519;

    fn bgra(rgb: u32) -> [u8; 4] {
        [rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8, 255]
    }

    fn count(bitmap: &Bitmap, rgb: u32) -> usize {
        let want = bgra(rgb);
        (0..bitmap.size)
            .flat_map(|y| (0..bitmap.size).map(move |x| (x, y)))
            .filter(|&(x, y)| bitmap.pixel(x, y) == want)
            .count()
    }

    #[test]
    fn the_label_caps_at_nine_plus() {
        assert_eq!(badge_text(0), "");
        assert_eq!(badge_text(1), "1");
        assert_eq!(badge_text(9), "9");
        assert_eq!(badge_text(10), "9+");
        assert_eq!(badge_text(250), "9+");
    }

    #[test]
    fn a_badge_is_a_rounded_amber_square_with_the_digit_in_ink() {
        let bitmap = badge(3, 16, AMBER, INK);
        assert_eq!((bitmap.size, bitmap.bgra.len()), (16, 16 * 16 * 4));
        assert_eq!(bitmap.pixel(0, 0)[3], 0, "the corner is cut");
        assert_eq!(bitmap.pixel(8, 1), bgra(AMBER));
        // A `3` has 11 lit cells of the 3×5 font, each drawn 2×2 at 16 px.
        assert_eq!(count(&bitmap, INK), 11 * 4);
    }

    #[test]
    fn nine_plus_fits_inside_the_square() {
        let bitmap = badge(12, 16, AMBER, INK);
        // `9` has 12 lit cells and `+` 5.
        assert_eq!(count(&bitmap, INK), (12 + 5) * 4);
        let columns: Vec<u32> = (0..16)
            .filter(|&x| (0..16).any(|y| bitmap.pixel(x, y) == bgra(INK)))
            .collect();
        assert_eq!((columns.first(), columns.last()), (Some(&1), Some(&14)));
    }

    #[test]
    fn a_zero_badge_is_a_plain_square() {
        assert_eq!(count(&badge(0, 16, AMBER, INK), INK), 0);
    }

    #[test]
    fn a_dot_is_round() {
        let bitmap = dot(16, AMBER);
        assert_eq!(bitmap.pixel(0, 0)[3], 0);
        assert_eq!(bitmap.pixel(8, 8), bgra(AMBER));
        assert_eq!(bitmap.pixel(15, 15)[3], 0);
        assert_eq!(dot(32, AMBER).bgra.len(), 32 * 32 * 4);
    }
}
```

- [ ] **Step 2: Register the module and run to verify it fails**

Add `pub mod glyph;` to `lib.rs`. Run: `cargo test -p mkrew-ui glyph -j 4`
Expected: FAIL to compile (`cannot find function badge`).

- [ ] **Step 3: Implement** — above the test module:

```rust
//! Raster icons for the OS shell (ADR 0063): the taskbar badge and the tray's state dot. Pure,
//! BGRA top-down, hard edges (alpha 0 or 255, so premultiplying changes nothing). Digits come
//! from a 3×5 pixel font scaled by whole pixels: no font or GDI text is needed.

/// Digits 0–9 and `+`, one byte per row, the low three bits from left to right.
const FONT: [[u8; 5]; 11] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
    [0b000, 0b010, 0b111, 0b010, 0b000], // +
];

/// A square picture, 4 bytes per pixel in BGRA order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub size: u32,
    pub bgra: Vec<u8>,
}

impl Bitmap {
    fn new(size: u32) -> Self {
        Self {
            size,
            bgra: vec![0; (size * size * 4) as usize],
        }
    }

    fn fill(&mut self, x: u32, y: u32, rgb: u32) {
        let at = ((y * self.size + x) * 4) as usize;
        self.bgra[at..at + 4].copy_from_slice(&[rgb as u8, (rgb >> 8) as u8, (rgb >> 16) as u8, 255]);
    }

    /// The BGRA pixel at (`x`, `y`).
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * self.size + x) * 4) as usize;
        [self.bgra[at], self.bgra[at + 1], self.bgra[at + 2], self.bgra[at + 3]]
    }
}

fn glyph(c: char) -> Option<&'static [u8; 5]> {
    match c {
        '0'..='9' => FONT.get(c as usize - '0' as usize),
        '+' => FONT.get(10),
        _ => None,
    }
}

/// The badge text: nothing for 0, `1`…`9`, then `9+`.
pub fn badge_text(count: usize) -> String {
    match count {
        0 => String::new(),
        1..=9 => count.to_string(),
        _ => "9+".to_owned(),
    }
}

/// The taskbar overlay: a rounded square (radius `size / 4`) in `bg` with the count in `ink`.
pub fn badge(count: usize, size: u32, bg: u32, ink: u32) -> Bitmap {
    let mut bitmap = Bitmap::new(size);
    let radius = size / 4;
    for y in 0..size {
        for x in 0..size {
            let cx = x.clamp(radius, size - 1 - radius);
            let cy = y.clamp(radius, size - 1 - radius);
            let (dx, dy) = (x.abs_diff(cx), y.abs_diff(cy));
            if dx * dx + dy * dy <= radius * radius {
                bitmap.fill(x, y, bg);
            }
        }
    }
    let text: Vec<&[u8; 5]> = badge_text(count).chars().filter_map(glyph).collect();
    if text.is_empty() {
        return bitmap;
    }
    let scale = (size / 8).max(1);
    let width = (text.len() as u32 * 4 - 1) * scale;
    let left = size.saturating_sub(width) / 2;
    let top = size.saturating_sub(5 * scale) / 2;
    for (index, rows) in text.iter().enumerate() {
        let origin = left + index as u32 * 4 * scale;
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..3u32 {
                if (*bits >> (2 - col)) & 1 == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        bitmap.fill(
                            origin + col * scale + dx,
                            top + row as u32 * scale + dy,
                            ink,
                        );
                    }
                }
            }
        }
    }
    bitmap
}

/// The tray's working / needs-you icon: a filled circle in `rgb`.
pub fn dot(size: u32, rgb: u32) -> Bitmap {
    let mut bitmap = Bitmap::new(size);
    let side = size as i32;
    for y in 0..size {
        for x in 0..size {
            // Pixel centres in half-pixel units from the middle.
            let (dx, dy) = (2 * x as i32 + 1 - side, 2 * y as i32 + 1 - side);
            if dx * dx + dy * dy <= side * side {
                bitmap.fill(x, y, rgb);
            }
        }
    }
    bitmap
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-ui glyph -j 4`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/glyph.rs
git add -p crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): badge and tray dot bitmaps (ADR 0063)"
```

---

### Task 6: The OS layer, part 1: `Desktop` with taskbar badge, flash, hide / show

**Files:**
- Modify: `crates/mkrew-ui/Cargo.toml` (new `[target.'cfg(windows)'.dependencies]` section after `[dependencies]`)
- Create: `crates/mkrew-ui/src/desktop.rs`, `crates/mkrew-ui/src/desktop/win32.rs`, `crates/mkrew-ui/src/desktop/none.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod desktop;` after `pub mod context_menu;`)

**Interfaces:**
- Consumes: `glyph::{badge, Bitmap}` (Task 5), `attention::{Icon, TrayMenu}` (Task 4), `inbox::label`.
- Produces: `pub enum TrayEvent { Open, Session(Ulid), Quit }`; `pub fn instance_key(exe: &str) -> String`; `pub fn escape_menu(text: &str) -> String`; `pub fn raise_running_instance() -> bool`; `pub use … as Desktop` with:
  `Desktop::new(&gpui::Window) -> (Desktop, async_channel::Receiver<TrayEvent>)`, `has_tray(&self) -> bool`, `set_badge(&mut self, count: usize, bg: u32, ink: u32)`, `flash(&mut self, on: bool)`, `set_tray(&self, icon: attention::Icon, rgb: u32, menu: TrayMenu)`, `remove_tray(&self)`, `hide_window(&mut self)`, `show_window(&mut self)`.
  This task makes the tray calls real no-ops on Windows (`has_tray` false); Task 7 replaces them with the real tray.

Verified against source: `ITaskbarList3::SetOverlayIcon(hwnd, hicon, pszdescription: Param<PCWSTR>)` and `HrInit` in `windows-0.62.2/src/Windows/Win32/UI/Shell/mod.rs:47044,47200`; `TaskbarList` CLSID `:56399`; `FlashWindowEx`, `FLASHWINFO`, `FLASHW_TRAY|FLASHW_TIMERNOFG|FLASHW_STOP` in `…/UI/WindowsAndMessaging/mod.rs:702,3509,3552-3557`; `ShowWindow`, `IsIconic`, `SW_HIDE/SW_SHOW/SW_RESTORE` `:2342,1320,6409-6421`; `CreateIconIndirect` + `ICONINFO` (has `Default`) `:373,4067`; `CreateBitmap`/`DeleteObject` in `…/Graphics/Gdi/mod.rs:187,463`; `impl From<HBITMAP> for HGDIOBJ` `:5300`; OLE/STA is initialised on the main thread by gpui (`gpui-pre-windows platform.rs:166`), so `CoCreateInstance` works there; `Window: HasWindowHandle` (`gpui-pre window.rs:7360`) with `Win32WindowHandle.hwnd: NonZeroIsize` (`gpui-pre-windows window.rs:594`).

- [ ] **Step 1: Write the failing tests** — create `crates/mkrew-ui/src/desktop.rs` with the pure tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instance_key_is_per_exe_path_and_ignores_case() {
        let installed = instance_key(r"C:\Program Files\Mkrew\mkrew.exe");
        assert_eq!(installed, instance_key(r"c:\program files\mkrew\MKREW.EXE"));
        assert_ne!(installed, instance_key(r"C:\src\mkrew\target\debug\mkrew.exe"));
        assert_eq!(installed.len(), 16);
    }

    #[test]
    fn an_ampersand_in_a_menu_line_is_not_an_accelerator() {
        assert_eq!(escape_menu("claude · R&D — done, 3m"), "claude · R&&D — done, 3m");
        assert_eq!(escape_menu("plain"), "plain");
    }
}
```

and in `desktop/win32.rs` (created in Step 3) a `#[cfg(test)]` module with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_badge_bitmap_becomes_an_icon() {
        let bitmap = glyph::badge(3, 16, 0xF5A623, 0x141519);
        assert!(icon_from(&bitmap).is_some());
        assert!(icon_from(&glyph::dot(16, 0xF5A623)).is_some());
    }
}
```

- [ ] **Step 2: Add the dependencies, register the module, run to verify it fails**

`crates/mkrew-ui/Cargo.toml`, after the `[dependencies]` table (before `[build-dependencies]`):

```toml
[target.'cfg(windows)'.dependencies]
# Taskbar badge, flashing, tray icon and hiding the window (SPEC §20J, ADR 0063). Same 0.62 and
# 0.6 that gpui's Windows backend already builds with, so the tree gains no package; the
# features are ones it already enables.
raw-window-handle = "0.6"
windows = { version = "0.62", features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_System_Com",
    "Win32_System_LibraryLoader",
    "Win32_UI_Controls",
    "Win32_UI_Shell",
    "Win32_UI_Shell_Common",
    "Win32_UI_WindowsAndMessaging",
] }
```

Add `pub mod desktop;` to `lib.rs` (after `pub mod context_menu;`). Run: `cargo test -p mkrew-ui desktop -j 4`
Expected: FAIL to compile (`instance_key` not found).

- [ ] **Step 3: Implement the façade** — `desktop.rs`, above the tests:

```rust
//! The OS around the main window (SPEC §20J, ADR 0063): taskbar badge and flashing, the tray
//! icon and its menu, hiding and showing the window. Windows only; elsewhere every call does
//! nothing (gpui-pre 0.3.7 has no dock-badge, tray or hide-window call) and `has_tray()` is false,
//! so a closed window still quits.

use ulid::Ulid;

#[cfg(not(windows))]
mod none;
#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
use none as os;
#[cfg(windows)]
use win32 as os;

pub use os::Os as Desktop;

/// What a click in the tray asks of the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// Show the window.
    Open,
    /// Show the window on this session's pane.
    Session(Ulid),
    /// Quit the UI (the daemon and the agents keep running).
    Quit,
}

/// FNV-1a of the lower-cased exe path. Two launches of the same exe share it, a dev build at
/// another path does not (the second launch only forwards to its own kind).
pub fn instance_key(exe: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in exe.to_lowercase().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// [`instance_key`] of the running exe.
fn current_key() -> String {
    std::env::current_exe()
        .map(|exe| instance_key(&exe.to_string_lossy()))
        .unwrap_or_default()
}

/// Asks another running Mkrew of the same exe to show its window. `true`: it was asked, this
/// launch should end.
pub fn raise_running_instance() -> bool {
    os::raise(&current_key())
}

/// A menu line with `&` doubled: Win32 menus read a lone `&` as an accelerator mark.
pub fn escape_menu(text: &str) -> String {
    text.replace('&', "&&")
}
```

`desktop/none.rs`:

```rust
//! Everything but Windows: nothing to show or hide, and no tray, so `has_tray` is false and the
//! window's ✕ quits as before (macOS dock badge: gpui-pre 0.3.7 has no call, ADR 0063).

use super::TrayEvent;
use crate::attention::{Icon, TrayMenu};

pub struct Os;

impl Os {
    /// The receiver is closed at once: there is no tray to listen to.
    pub fn new(_window: &gpui::Window) -> (Self, async_channel::Receiver<TrayEvent>) {
        (Self, async_channel::unbounded::<TrayEvent>().1)
    }

    pub fn has_tray(&self) -> bool {
        false
    }

    pub fn set_badge(&mut self, _count: usize, _bg: u32, _ink: u32) {}

    pub fn flash(&mut self, _on: bool) {}

    pub fn set_tray(&self, _icon: Icon, _rgb: u32, _menu: TrayMenu) {}

    pub fn remove_tray(&self) {}

    pub fn hide_window(&mut self) {}

    pub fn show_window(&mut self) {}
}

pub fn raise(_key: &str) -> bool {
    false
}
```

- [ ] **Step 4: Implement the Windows side (taskbar, flash, hide / show)** — `desktop/win32.rs`:

```rust
//! Windows side of [`super::Desktop`] (ADR 0063): the taskbar overlay and flash of the main
//! window and hiding / showing it, through the `windows` crate gpui's backend already builds
//! with. The tray is in the same file (added by the next task).
#![allow(unsafe_code)]

use std::ffi::c_void;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{CreateBitmap, DeleteObject};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::UI::Shell::{ITaskbarList3, TaskbarList};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, FLASHW_STOP, FLASHW_TIMERNOFG, FLASHW_TRAY, FLASHWINFO,
    FlashWindowEx, GetSystemMetrics, HICON, ICONINFO, IsIconic, SM_CXSMICON, SW_HIDE, SW_RESTORE,
    SW_SHOW, ShowWindow,
};
use windows::core::HSTRING;

use super::TrayEvent;
use crate::attention::{Icon, TrayMenu};
use crate::glyph::{self, Bitmap};
use crate::inbox;

/// An icon this process made; destroyed on drop.
pub struct OwnedIcon(pub HICON);

impl Drop for OwnedIcon {
    fn drop(&mut self) {
        // SAFETY: the handle came from `CreateIconIndirect` and is destroyed once.
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}

/// The small-icon size of the current DPI (16 px at 100 %).
pub fn small_icon_size() -> u32 {
    // SAFETY: plain query, no pointers.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) };
    u32::try_from(size).unwrap_or(16).max(16)
}

/// A 32-bit alpha icon from `bitmap`.
pub fn icon_from(bitmap: &Bitmap) -> Option<OwnedIcon> {
    let size = i32::try_from(bitmap.size).ok()?;
    // A 1-bit mask, rows padded to 16 bits; all zero = the colour bitmap decides (its alpha).
    let mask_bits = vec![0u8; (bitmap.size.div_ceil(16) * 2 * bitmap.size) as usize];
    // SAFETY: `bgra` holds `size * size * 4` bytes and `mask_bits` one padded row per line;
    // `CreateBitmap` copies both. The bitmaps are freed below, the icon keeps its own copy.
    unsafe {
        let color = CreateBitmap(size, size, 1, 32, Some(bitmap.bgra.as_ptr().cast::<c_void>()));
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast::<c_void>()));
        let info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color,
        };
        let icon = CreateIconIndirect(&info);
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
        icon.map_err(|error| tracing::warn!("cannot make an icon: {error}"))
            .ok()
            .map(OwnedIcon)
    }
}

#[derive(Clone, Copy)]
struct Badge {
    count: usize,
    bg: u32,
    ink: u32,
}

/// The main window's Windows-only extras.
pub struct Os {
    hwnd: Option<HWND>,
    list: Option<ITaskbarList3>,
    flashing: bool,
    badge: Option<Badge>,
}

impl Os {
    pub fn new(window: &gpui::Window) -> (Self, async_channel::Receiver<TrayEvent>) {
        let hwnd = match window.window_handle().map(|handle| handle.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => Some(HWND(handle.hwnd.get() as *mut c_void)),
            _ => None,
        };
        // gpui initialised OLE (an STA) on this thread, so the shell's taskbar list can be made.
        let list = hwnd.and_then(|_| {
            // SAFETY: plain COM activation; no pointers are passed in.
            let made: windows::core::Result<ITaskbarList3> =
                unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER) };
            // SAFETY: `HrInit` takes no arguments.
            made.and_then(|list| unsafe { list.HrInit() }.map(|()| list))
                .inspect_err(|error| tracing::warn!("taskbar badge unavailable: {error}"))
                .ok()
        });
        let this = Self {
            hwnd,
            list,
            flashing: false,
            badge: None,
        };
        (this, async_channel::unbounded::<TrayEvent>().1)
    }

    pub fn has_tray(&self) -> bool {
        false
    }

    /// The overlay icon with `count` (cleared at 0).
    pub fn set_badge(&mut self, count: usize, bg: u32, ink: u32) {
        self.badge = (count > 0).then_some(Badge { count, bg, ink });
        self.apply_badge();
    }

    fn apply_badge(&self) {
        let (Some(hwnd), Some(list)) = (self.hwnd, &self.list) else {
            return;
        };
        let icon = self
            .badge
            .and_then(|b| icon_from(&glyph::badge(b.count, small_icon_size(), b.bg, b.ink)));
        let description = self
            .badge
            .map_or_else(HSTRING::new, |b| HSTRING::from(inbox::label(b.count)));
        let handle = icon
            .as_ref()
            .map_or(HICON(std::ptr::null_mut()), |icon| icon.0);
        // SAFETY: `hwnd` is the live main window; the shell copies the icon during the call; a
        // null icon clears the overlay.
        if let Err(error) = unsafe { list.SetOverlayIcon(hwnd, handle, &description) } {
            tracing::warn!("cannot set the taskbar badge: {error}");
        }
    }

    /// Starts or stops flashing the taskbar button (until the window is in front).
    pub fn flash(&mut self, on: bool) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        if std::mem::replace(&mut self.flashing, on) == on {
            return;
        }
        let info = FLASHWINFO {
            cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
            hwnd,
            dwFlags: if on {
                FLASHW_TRAY | FLASHW_TIMERNOFG
            } else {
                FLASHW_STOP
            },
            uCount: 0,
            dwTimeout: 0,
        };
        // SAFETY: `info` is fully initialised and `cbSize` is set.
        unsafe {
            let _ = FlashWindowEx(&info);
        }
    }

    pub fn set_tray(&self, _icon: Icon, _rgb: u32, _menu: TrayMenu) {}

    pub fn remove_tray(&self) {}

    /// Hides the window (and its taskbar button); gpui reports it as `WindowVisibility::Hidden`.
    pub fn hide_window(&mut self) {
        if let Some(hwnd) = self.hwnd {
            // SAFETY: `hwnd` is the live main window.
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    }

    /// Shows a hidden or minimized window; the caller raises it with `Window::activate_window`.
    pub fn show_window(&mut self) {
        let Some(hwnd) = self.hwnd else {
            return;
        };
        // SAFETY: `hwnd` is the live main window.
        unsafe {
            let command = if IsIconic(hwnd).as_bool() { SW_RESTORE } else { SW_SHOW };
            let _ = ShowWindow(hwnd, command);
        }
        // A hidden window's taskbar button starts without its overlay.
        self.apply_badge();
    }
}

pub fn raise(_key: &str) -> bool {
    false
}
```

(and the `#[cfg(test)] mod tests` from Step 1 at the end of the file.)

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p mkrew-ui desktop -j 4`
Expected: PASS (the key and escape tests, plus `a_badge_bitmap_becomes_an_icon` on Windows). If rustc names a missing item gated by a `windows` feature, add that feature to the `Cargo.toml` list — every feature gpui's Windows backend turns on is already compiled (`gpui-pre-windows-0.3.7/Cargo.toml:144-207`). If a signature differs from the one used here, read the item in `~/.cargo/registry/src/*/windows-0.62.2/src/Windows/Win32/…` and adapt; do not guess.

Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit** — and tell the human: `Cargo.lock` now has new edges (mkrew-ui → `windows`, `raw-window-handle`; no new package), and `scripts/package*.{ps1,sh}` build with `--locked`, so the lock file must be committed by the human before packaging (project rule: the executor does not commit it).

```bash
git add crates/mkrew-ui/Cargo.toml crates/mkrew-ui/src/desktop.rs crates/mkrew-ui/src/desktop
git add -p crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): Desktop: taskbar badge, flash, hide and show the window (ADR 0063)"
```

---

### Task 7: The OS layer, part 2: tray icon and second-launch forwarding

**Files:**
- Modify: `crates/mkrew-ui/src/desktop/win32.rs` (replace `Os`, `raise`; add the tray)

**Interfaces:**
- Consumes: Task 6's `win32.rs` items (`icon_from`, `small_icon_size`, `OwnedIcon`), `attention::{Icon, TrayMenu}`, `desktop::{escape_menu, TrayEvent}`, `status::now_ms`.
- Produces: the real behaviour of `Desktop::has_tray`, `set_tray`, `remove_tray`, `new` (which now also starts the tray) and `raise_running_instance`. The tray runs on its own thread with a hidden top-level window of class `MkrewTray.<instance key>`; messages: `WM_APP+1` icon callback, `WM_APP+2` state update (wparam = icon index, lparam = `0xRRGGBB`), `WM_APP+3` "open" from a second launch.

Verified against source (`windows-0.62.2`): `NOTIFYICONDATAW` (Default = zeroed, `NIF_*`, `NIM_ADD/MODIFY/DELETE`) `UI/Shell/mod.rs:51984-52003,52111-52160`; `Shell_NotifyIconW` `:4450`; `WNDCLASSW` (Default) / `RegisterClassW` / `CreateWindowExW` / `DefWindowProcW` / `GetMessageW` / `DispatchMessageW` / `TranslateMessage` / `PostMessageW` / `FindWindowW` / `RegisterWindowMessageW` / `CreatePopupMenu` / `AppendMenuW(hmenu, flags, id: usize, text: Param<PCWSTR>)` / `TrackPopupMenu(…, nreserved: Option<i32>, hwnd, prcrect: Option<*const RECT>) -> BOOL` / `DestroyMenu` / `SetForegroundWindow` / `GetCursorPos` / `LoadImageW` / `MF_*`, `TPM_*`, `WM_*`, `IMAGE_ICON` in `UI/WindowsAndMessaging/mod.rs`; `GetModuleHandleW` in `System/LibraryLoader/mod.rs:224`. The app icon is resource id 1 (`crates/mkrew-ui/resources/windows/mkrew.rc:2`). The legacy callback (no `NIM_SETVERSION`) sends the mouse message in `lParam`.

- [ ] **Step 1: Write the failing test** — in the `tests` module of `win32.rs` add (the thread, class registration, `FindWindowW` and `PostMessageW` are real; `NIM_ADD` may fail on a machine without a shell and that does not matter, the window exists either way):

```rust
    #[test]
    fn a_second_launch_reaches_the_tray_window() {
        let key = format!("test-{}", std::process::id());
        assert!(!raise(&key), "nobody listens yet");
        let (tray, events) = Tray::start(&key).expect("the tray window");
        assert!(raise(&key));
        let started = std::time::Instant::now();
        let event = loop {
            if let Ok(event) = events.try_recv() {
                break Some(event);
            }
            if started.elapsed() > std::time::Duration::from_secs(2) {
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(event, Some(TrayEvent::Open));
        tray.remove();
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p mkrew-ui desktop -j 4`
Expected: FAIL to compile (`Tray` not found).

- [ ] **Step 3: Implement the tray** — in `win32.rs`, extend the imports:

```rust
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFY_ICON_MESSAGE,
    NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    FindWindowW, GetCursorPos, GetMessageW, IMAGE_ICON, LR_DEFAULTCOLOR, LoadImageW, MF_GRAYED,
    MF_SEPARATOR, MF_STRING, MSG, PostMessageW, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP,
    WNDCLASSW,
};
use windows::core::{PCWSTR, w};

use super::escape_menu;
use crate::status;
```

(merging with the existing `use` lines; keep `rustfmt` ordering) and add above `Os`:

```rust
/// Tray icon callback (`uCallbackMessage`); `lParam` is the mouse message.
const WM_TRAY: u32 = WM_APP + 1;
/// UI → tray thread: `wParam` icon index (0 idle, 1 working, 2 needs you), `lParam` `0xRRGGBB`.
const WM_TRAY_STATE: u32 = WM_APP + 2;
/// A second launch asks the first to show its window.
const WM_OPEN: u32 = WM_APP + 3;
const ICON_ID: u32 = 1;
const CMD_OPEN: i32 = 1;
const CMD_QUIT: i32 = 2;
const CMD_SESSION: i32 = 100;

/// What the UI thread shares with the tray thread.
struct Shared {
    menu: Mutex<TrayMenu>,
    /// The icon is in the notification area.
    shown: AtomicBool,
    /// The tray window, 0 until it exists.
    hwnd: AtomicIsize,
    events: async_channel::Sender<TrayEvent>,
}

/// The tray icon: its hidden window and message loop live on the `mkrew-tray` thread.
pub struct Tray {
    shared: Arc<Shared>,
}

fn class_name(key: &str) -> Vec<u16> {
    format!("MkrewTray.{key}")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

fn icon_index(icon: Icon) -> usize {
    match icon {
        Icon::Idle => 0,
        Icon::Working => 1,
        Icon::NeedsYou => 2,
    }
}

fn icon_of(index: usize) -> Icon {
    match index {
        1 => Icon::Working,
        2 => Icon::NeedsYou,
        _ => Icon::Idle,
    }
}

impl Tray {
    /// Starts the tray thread; `None` if its window could not be made.
    pub fn start(key: &str) -> Option<(Tray, async_channel::Receiver<TrayEvent>)> {
        let (events, receiver) = async_channel::unbounded();
        let shared = Arc::new(Shared {
            menu: Mutex::new(TrayMenu::default()),
            shown: AtomicBool::new(false),
            hwnd: AtomicIsize::new(0),
            events,
        });
        let (ready, started) = std::sync::mpsc::sync_channel(1);
        let (class, thread) = (class_name(key), Arc::clone(&shared));
        std::thread::Builder::new()
            .name("mkrew-tray".into())
            .spawn(move || run(&class, &thread, &ready))
            .ok()?;
        started.recv_timeout(Duration::from_secs(2)).ok()?;
        (shared.hwnd.load(Ordering::Acquire) != 0).then_some((Tray { shared }, receiver))
    }

    /// The icon is in the notification area (a failed `NIM_ADD` leaves it false).
    pub fn shown(&self) -> bool {
        self.shared.shown.load(Ordering::Relaxed)
    }

    /// New menu data and icon; the tray thread applies them.
    pub fn set(&self, icon: Icon, rgb: u32, menu: TrayMenu) {
        *self.shared.menu.lock().unwrap_or_else(PoisonError::into_inner) = menu;
        let hwnd = HWND(self.shared.hwnd.load(Ordering::Acquire) as *mut c_void);
        // SAFETY: posting to a window of this process; a dead window just fails.
        let posted = unsafe {
            PostMessageW(
                Some(hwnd),
                WM_TRAY_STATE,
                WPARAM(icon_index(icon)),
                LPARAM(rgb as isize),
            )
        };
        if let Err(error) = posted {
            tracing::warn!("cannot update the tray icon: {error}");
        }
    }

    /// Takes the icon out of the notification area (the process is about to exit).
    pub fn remove(&self) {
        let hwnd = HWND(self.shared.hwnd.load(Ordering::Acquire) as *mut c_void);
        let data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            ..Default::default()
        };
        // SAFETY: `data` is initialised and names the icon `run` added.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
        self.shared.shown.store(false, Ordering::Relaxed);
    }
}

/// Tray-thread state, reached from the window procedure.
struct State {
    shared: Arc<Shared>,
    /// `RegisterWindowMessageW("TaskbarCreated")`: Explorer restarted, add the icon again.
    taskbar_created: u32,
    /// The icon currently in the notification area.
    current: RefCell<Option<OwnedIcon>>,
    /// The last (icon, color), for the re-add.
    kind: Cell<(Icon, u32)>,
}

thread_local! {
    static STATE: RefCell<Option<Rc<State>>> = const { RefCell::new(None) };
}

fn app_icon(size: u32) -> Option<OwnedIcon> {
    let side = i32::try_from(size).ok()?;
    // SAFETY: resource id 1 of this exe is the app icon (`mkrew.rc`); the handle is ours to destroy.
    unsafe {
        let module = GetModuleHandleW(PCWSTR::null()).ok()?;
        let handle = LoadImageW(
            Some(HINSTANCE(module.0)),
            PCWSTR(1 as *const u16),
            IMAGE_ICON,
            side,
            side,
            LR_DEFAULTCOLOR,
        )
        .ok()?;
        Some(OwnedIcon(HICON(handle.0)))
    }
}

impl State {
    fn send(&self, event: TrayEvent) {
        let _ = self.shared.events.try_send(event);
    }

    /// `NIM_ADD` or `NIM_MODIFY` with the icon for `icon` / `rgb` and the current tooltip.
    fn apply(&self, hwnd: HWND, message: NOTIFY_ICON_MESSAGE, icon: Icon, rgb: u32) {
        self.kind.set((icon, rgb));
        let size = small_icon_size();
        let made = match icon {
            Icon::Idle => app_icon(size),
            Icon::Working | Icon::NeedsYou => icon_from(&glyph::dot(size, rgb)),
        };
        let tip = (self.shared.menu.lock())
            .unwrap_or_else(PoisonError::into_inner)
            .tip();
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ICON_ID,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: WM_TRAY,
            hIcon: made
                .as_ref()
                .map_or(HICON(std::ptr::null_mut()), |icon| icon.0),
            ..Default::default()
        };
        for (slot, unit) in data.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
            *slot = unit;
        }
        // SAFETY: `data` is fully initialised and `cbSize` is set.
        let ok = unsafe { Shell_NotifyIconW(message, &data) }.as_bool();
        if message == NIM_ADD {
            self.shared.shown.store(ok, Ordering::Relaxed);
            if !ok {
                tracing::warn!("the tray icon could not be added; ✕ will quit");
            }
        }
        if ok {
            // The shell has the new icon: the old one can go.
            *self.current.borrow_mut() = made;
        }
    }

    /// The right-click menu: sessions that need you, `● N working`, Open, Quit UI.
    fn show_menu(&self, hwnd: HWND) {
        let menu = (self.shared.menu.lock())
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let now = status::now_ms();
        // SAFETY: a popup menu made, shown and destroyed on this thread; `hwnd` is our window.
        let picked = unsafe {
            let Ok(popup) = CreatePopupMenu() else {
                return;
            };
            for (index, item) in menu.items.iter().enumerate() {
                let text = HSTRING::from(escape_menu(&item.text(now)));
                let _ = AppendMenuW(popup, MF_STRING, CMD_SESSION as usize + index, &text);
            }
            if !menu.items.is_empty() {
                let _ = AppendMenuW(popup, MF_SEPARATOR, 0, PCWSTR::null());
            }
            let working = HSTRING::from(format!("● {} working", menu.working));
            let _ = AppendMenuW(popup, MF_STRING | MF_GRAYED, 0, &working);
            let _ = AppendMenuW(popup, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(popup, MF_STRING, CMD_OPEN as usize, w!("Open Mkrew"));
            let _ = AppendMenuW(
                popup,
                MF_STRING,
                CMD_QUIT as usize,
                w!("Quit UI (agents keep running)"),
            );
            let mut at = POINT::default();
            let _ = GetCursorPos(&mut at);
            // A tray menu only closes when its owner is the foreground window; the WM_NULL
            // after it lets the menu go away (the documented tray-menu recipe).
            let _ = SetForegroundWindow(hwnd);
            let picked = TrackPopupMenu(
                popup,
                TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN,
                at.x,
                at.y,
                None,
                hwnd,
                None,
            );
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(popup);
            picked.0
        };
        match picked {
            0 => {}
            CMD_OPEN => self.send(TrayEvent::Open),
            CMD_QUIT => self.send(TrayEvent::Quit),
            id if id >= CMD_SESSION => {
                let index = usize::try_from(id - CMD_SESSION).unwrap_or(usize::MAX);
                if let Some(item) = menu.items.get(index) {
                    self.send(TrayEvent::Session(item.session_id));
                }
            }
            _ => {}
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Cloned out so no borrow is held while a menu pumps messages back into this procedure.
    let state = STATE.with(|state| state.borrow().clone());
    let Some(state) = state else {
        // SAFETY: forwarding the unchanged message.
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    };
    match message {
        WM_TRAY => match lparam.0 as u32 {
            WM_LBUTTONUP => state.send(TrayEvent::Open),
            WM_RBUTTONUP => state.show_menu(hwnd),
            _ => {}
        },
        WM_TRAY_STATE => state.apply(hwnd, NIM_MODIFY, icon_of(wparam.0), lparam.0 as u32),
        WM_OPEN => state.send(TrayEvent::Open),
        other if other == state.taskbar_created => {
            let (icon, rgb) = state.kind.get();
            state.apply(hwnd, NIM_ADD, icon, rgb);
        }
        // SAFETY: forwarding the unchanged message.
        _ => return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
    LRESULT(0)
}

/// The tray thread: register the class, make the hidden window, add the icon, pump messages.
fn run(class: &[u16], shared: &Arc<Shared>, ready: &SyncSender<()>) {
    // SAFETY: Win32 window creation on this thread; `class` outlives every call that reads it.
    unsafe {
        let Ok(module) = GetModuleHandleW(PCWSTR::null()) else {
            let _ = ready.send(());
            return;
        };
        let hinstance = HINSTANCE(module.0);
        let class_name = PCWSTR(class.as_ptr());
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: hinstance,
            lpszClassName: class_name,
            ..Default::default()
        };
        if RegisterClassW(&window_class) == 0 {
            tracing::warn!("cannot register the tray window class");
            let _ = ready.send(());
            return;
        }
        // A plain hidden top-level window (never shown): only top-level windows get the
        // `TaskbarCreated` broadcast, and `FindWindowW` of a second launch finds only those.
        let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Mkrew tray"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(hinstance),
            None,
        ) else {
            tracing::warn!("cannot make the tray window");
            let _ = ready.send(());
            return;
        };
        let state = Rc::new(State {
            shared: Arc::clone(shared),
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            current: RefCell::new(None),
            kind: Cell::new((Icon::Idle, 0)),
        });
        STATE.with(|slot| *slot.borrow_mut() = Some(Rc::clone(&state)));
        state.apply(hwnd, NIM_ADD, Icon::Idle, 0);
        shared.hwnd.store(hwnd.0 as isize, Ordering::Release);
        let _ = ready.send(());
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// Another running Mkrew of the same exe was asked to show its window.
pub fn raise(key: &str) -> bool {
    let class = class_name(key);
    // SAFETY: `class` is NUL-terminated and outlives both calls.
    unsafe {
        let Ok(hwnd) = FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) else {
            return false;
        };
        PostMessageW(Some(hwnd), WM_OPEN, WPARAM(0), LPARAM(0)).is_ok()
    }
}
```

and replace `Os` (struct, `new`, `has_tray`, `set_tray`, `remove_tray`) and delete the old `raise`:

```rust
/// The main window's Windows-only extras.
pub struct Os {
    hwnd: Option<HWND>,
    list: Option<ITaskbarList3>,
    flashing: bool,
    badge: Option<Badge>,
    tray: Option<Tray>,
}
```

`new` — replace the tail (`let this = …; (this, …)`) with:

```rust
        let (tray, events) = match Tray::start(&super::current_key()) {
            Some((tray, events)) => (Some(tray), events),
            None => (None, async_channel::unbounded::<TrayEvent>().1),
        };
        let this = Self {
            hwnd,
            list,
            flashing: false,
            badge: None,
            tray,
        };
        (this, events)
```

```rust
    pub fn has_tray(&self) -> bool {
        self.tray.as_ref().is_some_and(Tray::shown)
    }

    pub fn set_tray(&self, icon: Icon, rgb: u32, menu: TrayMenu) {
        if let Some(tray) = &self.tray {
            tray.set(icon, rgb, menu);
        }
    }

    pub fn remove_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.remove();
        }
    }
```

(`super::current_key` is private in `desktop.rs`: make it `pub(super)`-visible by changing `fn current_key()` to `pub(crate) fn current_key()`.)

Add a test for the menu limit of the tooltip? Not needed: `TrayMenu::tip` is pinned in Task 4, `escape_menu` in Task 6.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p mkrew-ui desktop -j 4` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS and clean. The icon image, the menu and the click paths are checked by hand (`docs/HUMAN_CHECKS.md`, Task 9): they need Explorer and a mouse.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/desktop/win32.rs crates/mkrew-ui/src/desktop.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): tray icon with menu and second-launch forwarding (ADR 0063)"
```

---

### Task 8: Wire it into `Shell`

**Files:**
- Create: `crates/mkrew-ui/src/shell/attention.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (add `mod attention;` after `mod ask_dialog;`), `crates/mkrew-ui/src/main.rs` (imports, `struct Shell`, `main`, `render`), `crates/mkrew-ui/src/shell/connection.rs` (`new`, hot reload, `refresh`, `dispatch`), `crates/mkrew-ui/src/shell/files.rs:565-595`, `crates/mkrew-ui/src/shell/workspace.rs:245-261`, `crates/mkrew-ui/src/shell/tabs.rs` (`mark_unread`)

**Interfaces:**
- Consumes: Tasks 2–7: `attention::{counts, menu, Tracker, entered_needs_input, hide_on_close}`, `Desktop` and `TrayEvent`, `Viewed::view -> bool`, `Shell::mark_unread`.
- Produces: `Shell::sync_attention(&mut self, &mut Context<Self>)`, `Shell::restore_window(&mut self, &mut Window)`, `Shell::tray_event`, `Shell::quit_ui`, `Shell::ask_unsaved`, fields `desktop`, `attention`, `close_to_tray`, `flash_taskbar`.

`Shell` needs a real window, so nothing here can be unit-tested headless; every decision it makes goes through the pure, tested functions of Tasks 2–4. Verification is a clean build, clippy, the existing suite, and the HUMAN_CHECKS of Task 9.

- [ ] **Step 1: Imports and fields** — `main.rs`:

after `use mkrew_ui::ask::{self, Ask};` add `use mkrew_ui::attention::{self, Tracker};`; after `use mkrew_ui::context_menu::{LayoutMenu, PaneMenu, TabMenu};` add `use mkrew_ui::desktop::{Desktop, TrayEvent};`.

In `struct Shell`, after `viewed: Viewed,` (`:414`):

```rust
    /// The OS around the window (ADR 0063): taskbar badge and flash, tray icon, hide / show.
    desktop: Desktop,
    /// What `desktop` last showed, so only a change reaches the OS.
    attention: Tracker,
    /// `[ui] close_to_tray`: the window's ✕ hides it while the tray icon is there.
    close_to_tray: bool,
    /// `[notify] flash_taskbar`.
    flash_taskbar: bool,
```

- [ ] **Step 2: `shell/attention.rs`** — create and register (`mod attention;` in `shell.rs`):

```rust
//! The OS shell's view of the Inbox (SPEC §20J, ADR 0063): taskbar badge, flash, tray icon and
//! menu, hiding the window to the tray, quitting the UI.

use crate::*;

impl Shell {
    /// Recounts "Needs you" and updates only what changed: the badge when its number moved, the
    /// tray (icon, tooltip, menu) when anything it shows moved. O(sessions), no allocation when
    /// nothing changed; called on session events, after a list load and when a view changed
    /// what needs the user, never per frame.
    pub(crate) fn sync_attention(&mut self, cx: &mut Context<'_, Self>) {
        let (projects, sessions) = (self.sidebar.projects(), self.sidebar.sessions());
        let counts = attention::counts(projects, sessions, &self.viewed);
        let change = self.attention.update(counts);
        if !change.tray {
            return;
        }
        let theme = cx.theme();
        if change.badge {
            // Dark ink on amber in both themes: the dark theme's background token.
            self.desktop
                .set_badge(counts.needs_you, theme.status_needs_input, Theme::dark().bg);
        }
        let color = match counts.icon() {
            attention::Icon::NeedsYou => theme.status_needs_input,
            _ => theme.status_working,
        };
        let menu = attention::menu(projects, sessions, &self.viewed, counts);
        self.desktop.set_tray(counts.icon(), color, menu);
        if counts.needs_you == 0 {
            self.desktop.flash(false);
        }
    }

    /// Shows the window again (tray, toast click, second launch) and raises it. gpui's
    /// `activate_window` only restores a *minimized* window, not a hidden one.
    pub(crate) fn restore_window(&mut self, window: &mut Window) {
        self.desktop.show_window();
        window.activate_window();
    }

    /// A click in the tray.
    pub(crate) fn tray_event(
        &mut self,
        event: TrayEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        match event {
            TrayEvent::Open => self.restore_window(window),
            TrayEvent::Session(session) => {
                self.restore_window(window);
                self.open_session(session, window, cx);
            }
            TrayEvent::Quit => self.quit_ui(window, cx),
        }
        cx.notify();
    }

    /// "Quit UI": unsaved Files changes are asked first (with the window shown, so the question
    /// is seen), then the UI ends; the daemon and the agents keep running (SPEC §2.2).
    pub(crate) fn quit_ui(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let again: AfterSave = Box::new(|shell, window, cx| shell.quit_ui(window, cx));
        if self.ask_unsaved(again, window, cx) {
            self.restore_window(window);
            return;
        }
        self.desktop.remove_tray();
        cx.quit();
    }
}
```

- [ ] **Step 3: Close to tray and the unsaved question** — `shell/files.rs`: replace the doc comment and body of `window_should_close` (`:565-595`) with:

```rust
    /// The window's close button (§2.5, §20J): with `[ui] close_to_tray` on and a tray icon to
    /// bring it back, the window only hides (Files changes stay in memory); otherwise unsaved
    /// Files changes ask first, one file at a time, and the window closes once none are left.
    /// The workspace that holds them is shown first, so a refused save is visible.
    pub(crate) fn window_should_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        if attention::hide_on_close(self.close_to_tray, self.desktop.has_tray()) {
            self.desktop.hide_window();
            return false;
        }
        let again: AfterSave = Box::new(|shell, window, cx| shell.close_when_saved(window, cx));
        !self.ask_unsaved(again, window, cx)
    }

    /// Closes the window once no Files change is unsaved, asking about the rest first.
    fn close_when_saved(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let again: AfterSave = Box::new(|shell, window, cx| shell.close_when_saved(window, cx));
        if !self.ask_unsaved(again, window, cx) {
            window.remove_window();
        }
    }

    /// Asks about the first unsaved Files change (`then` runs after it was saved or dropped).
    /// `true` while a question is open or was just opened, `false` when nothing is unsaved.
    pub(crate) fn ask_unsaved(
        &mut self,
        then: AfterSave,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) -> bool {
        let unsaved = (self.files.values())
            .find(|view| view.unsaved().is_some())
            .map(|view| view.project_id);
        let Some(project_id) = unsaved else {
            return false;
        };
        // A confirmation is open (Alt+F4 reaches here): answer it first; asking now would
        // replace it.
        if window.has_active_prompt() {
            return true;
        }
        if self.workspace != Some(project_id) {
            self.activate_workspace(project_id, window, cx);
        }
        self.show_view(View::Files, window, cx);
        self.leave_file(project_id, then, window, cx);
        true
    }
```

(`View`, `AfterSave` come from the crate root through `use crate::*`, as they do in the current body.)

- [ ] **Step 4: `Shell::new`** — `shell/connection.rs`:

(a) after `let settings = mkrew_core::settings::load(&settings_file).unwrap_or_default();` (`:17`) add `let flash_taskbar = settings.notify.flash_taskbar;` and after `let diff_wrap = ui.diff_wrap;` (`:21`) add `let close_to_tray = ui.close_to_tray;`.

(b) replace the activation observer from Task 3 so it also stops the flash, and add the tray pump and the quit hook right after it (before the `// Unsaved Files changes` comment):

```rust
        // Coming back to the window views the focused pane (a done session leaves the Inbox),
        // even one marked unread while it kept the keyboard, and ends the taskbar flash.
        cx.observe_window_activation(window, |shell, window, cx| {
            if window.is_window_active() {
                shell.viewed.activated();
                shell.desktop.flash(false);
            }
            cx.notify();
        })
        .detach();
        // The tray icon (Windows): its clicks arrive here from the tray thread.
        let (desktop, tray_events) = Desktop::new(window);
        cx.spawn_in(window, async move |this, cx| {
            while let Ok(event) = tray_events.recv().await {
                let handled =
                    this.update_in(cx, |shell, window, cx| shell.tray_event(event, window, cx));
                if handled.is_err() {
                    return;
                }
            }
        })
        .detach();
        // An icon left in the notification area would linger until the mouse passes over it.
        cx.on_app_quit(|shell, _| {
            shell.desktop.remove_tray();
            async {}
        })
        .detach();
```

(c) in the `Self { … }` literal after `viewed: Viewed::default(),` (`:320`):

```rust
            desktop,
            attention: Tracker::default(),
            close_to_tray,
            flash_taskbar,
```

(d) hot reload — in the `if let Ok(settings) = mkrew_core::settings::load(&shell.settings_file) {` block (`:76`), before `shell.apply_diff_mode(…)`:

```rust
                                    shell.close_to_tray = settings.ui.close_to_tray;
                                    shell.flash_taskbar = settings.notify.flash_taskbar;
                                    if !shell.flash_taskbar {
                                        shell.desktop.flash(false);
                                    }
```

(e) `refresh` — after `shell.sidebar.load(projects, sessions);` (`:471`) add `shell.sync_attention(cx);`.

(f) `dispatch` — replace `:650-657`:

```rust
                // Before the sidebar applies it: `viewed` and the flash read the previous state.
                if self.flash_taskbar
                    && !window.is_window_active()
                    && attention::entered_needs_input(event, self.sidebar.sessions())
                {
                    self.desktop.flash(true);
                }
                self.viewed.apply(event, self.sidebar.sessions());
                match self.sidebar.apply(event) {
                    Change::None => {}
                    Change::Redraw => {
                        cx.notify();
                        self.sync_attention(cx);
                    }
                    // A new session (e.g. spawned by `mk`): re-list.
                    Change::Refresh => self.refresh(window, cx),
                }
```

- [ ] **Step 5: Render, mark unread, toast click, second launch**

`main.rs` `render` — replace `:630-637` with:

```rust
        // Rendering is what marks the focused pane of the active window viewed (its done leaves
        // the Inbox): every state change and window activation re-renders. A change in what
        // needs the user also reaches the badge and the tray.
        let mut viewed_changed = false;
        if window.is_window_active()
            && let Some(id) = self.focused_session()
            && let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == id)
        {
            viewed_changed = self.viewed.view(session);
        }
        if viewed_changed {
            self.sync_attention(cx);
        }
```

`shell/tabs.rs` `mark_unread` — add `self.sync_attention(cx);` after `self.viewed.mark_unread(session);` is not possible inside the `if let` borrow; restructure:

```rust
    pub(crate) fn mark_unread(&mut self, session_id: Ulid, cx: &mut Context<'_, Self>) {
        self.pane_menu = None;
        if self.can_mark_unread(session_id)
            && let Some(session) = self.sidebar.sessions().iter().find(|s| s.id == session_id)
        {
            self.viewed.mark_unread(session);
            self.sync_attention(cx);
        }
        cx.notify();
    }
```

(If borrowck objects to `sync_attention(&mut self)` while `session` is borrowed, copy what `mark_unread` needs first: `let session = session.clone(); self.viewed.mark_unread(&session); self.sync_attention(cx);`.)

`shell/workspace.rs` `notification_clicked` — replace `window.activate_window();` (`:254`) with `self.restore_window(window);` and extend its doc comment: `raises the window (showing it again if it was hidden to the tray, restoring it if minimized)`.

`main.rs` `main()` — right after the `Keymap::load` block (`:161`) and before `gpui_platform::application()`:

```rust
    // A second launch of the same exe shows the running window (it may be hidden in the tray)
    // instead of opening another one; a dev build at another path starts on its own.
    if mkrew_ui::desktop::raise_running_instance() {
        return;
    }
```

- [ ] **Step 6: Build, lint, test**

Run: `cargo build -p mkrew-ui -j 4` → clean. If `let` chains or borrows in Steps 3-5 are rejected, fix with the smallest local change (clone the small value) — do not add `unsafe` or `RefCell`.
Run: `cargo clippy -p mkrew-ui --all-targets -- -D warnings` → clean.
Run: `cargo test -p mkrew-ui -j 4` → PASS (inbox, sidebar, palette, attention, glyph, desktop, and the rest unchanged).

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/shell/attention.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/files.rs crates/mkrew-ui/src/shell/workspace.rs crates/mkrew-ui/src/shell/tabs.rs
git add -p crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): badge, flash, tray and close to tray in the shell (ADR 0063)"
```

---

### Task 9: Human checks, progress note, full verify

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` ("To verify" section, newest first), `docs/PROGRESS.md` ("Notes for the next session", newest first)

- [ ] **Step 1: HUMAN_CHECKS** — add under `## To verify (non-blocking)`, above the 2026-10-15 entries:

```markdown
### To verify — 2026-10-19 Tray, taskbar badge, close to tray, Mark unread (ADR 0063)

- [ ] **Restart the daemon after installing** (settings only, no protocol change): an older daemon
  reports `[notify] flash_taskbar` as an unknown key. Cargo.lock gained edges (`windows`,
  `raw-window-handle`); commit it before `scripts/package*` (they build `--locked`).
- [ ] **Taskbar badge (dark + light, 100 % / 150 % / 200 % DPI):** with two agents waiting for
  input the Mkrew taskbar button shows an amber `2`; answering one → `1`; none → no overlay; ten or
  more → `9+`. Hover text / screen reader says "2 need you".
- [ ] **Flash:** minimize Mkrew, make an agent need input → the taskbar button flashes until you
  click it. It does not flash while the window is focused. `[notify] flash_taskbar = false` in
  settings.toml (saved, within a second) → no flash.
- [ ] **Tray icon:** Mkrew's logo while idle; a dot in the working color while an agent works; an
  amber dot while something needs you; the tooltip reads `Mkrew · 2 need you · 1 working`.
  Right-click: up to 8 lines `agent · workspace — state, age` (an `&` in a workspace name shows
  as `&`), a greyed `● N working`, `Open Mkrew`, `Quit UI (agents keep running)`. Left click opens
  the window. A line opens that session's pane, also from a hidden or minimized window.
- [ ] **Close to tray:** ✕ and Alt+F4 hide the window (no taskbar button); `mk session list` still
  shows the agents. Tray → Open: the same layout, terminals show their content again. With
  `[ui] close_to_tray = false` ✕ quits the UI (agents keep running).
- [ ] **Quit UI with an unsaved Files edit:** the window appears and asks Save / Don't save /
  Cancel; Cancel leaves the UI running; afterwards Quit UI ends it and `mk session list` still
  shows the agents.
- [ ] **Toast click while hidden:** the window comes back on that session's pane.
- [ ] **Explorer restart** (`taskkill /f /im explorer.exe`, then `explorer`): the tray icon comes
  back; the badge is set again after the next change.
- [ ] **Second launch:** starting `mkrew.exe` again (Start menu) while it is hidden shows the
  running window and opens no second tray icon. A dev build (`target\debug\mkrew.exe`) next to
  the installed one runs on its own.
- [ ] **Mark unread (dark + light):** right-click a finished session's row (and its pane header) →
  **Mark unread**: the row is tinted, the Needs you chip, the badge and the tray count go up by
  one; it stays while that pane keeps the keyboard; focusing another pane and coming back clears
  it. Palette "Agent: Mark unread" does the same on the focused pane. The item is greyed for a
  session that works, already needs you, or is a terminal. Restarting the UI forgets marks (as it
  forgets "viewed").
- [ ] **macOS / Linux (CI only):** nothing changes — no badge, no tray, ✕ quits.
```

- [ ] **Step 2: PROGRESS note** — add as the first line under `## Notes for the next session`:

```markdown
- 2026-10-19 tray, taskbar badge, close to tray, Mark unread (plan docs/superpowers/plans/2026-10-19-tray-badge-unread.md, ADR 0063, no protocol/migration change): `attention.rs` (counts, `Tracker`, tray menu data), `glyph.rs` (badge / dot bitmaps), `inbox::Viewed` gains `unread` + `view` memo, `desktop/{win32,none}.rs` (`ITaskbarList3` overlay, `FlashWindowEx`, `ShowWindow`, `Shell_NotifyIconW` on its own thread, second-launch forwarding by class `MkrewTray.<exe hash>`), `shell/attention.rs`; settings `[notify] flash_taskbar`, `[ui] close_to_tray`; `Extra::MarkUnread` + pane menu item. macOS dock badge is a no-op (gpui-pre has no call). Cargo.lock gained `windows`/`raw-window-handle` edges to mkrew-ui (needs committing for `--locked` packaging). Manual checks in HUMAN_CHECKS "2026-10-19 Tray, taskbar badge…". Outside the milestone loop.
```

- [ ] **Step 3: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: `verify: OK` (fmt, clippy `-D warnings`, `cargo test --workspace`). Fix only what this plan caused.

- [ ] **Step 4: Commit**

```bash
git add -p docs/HUMAN_CHECKS.md docs/PROGRESS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: tray, badge, close to tray and Mark unread checks and notes (ADR 0063)"
```

---

## Self-Review

**Spec coverage (the brief's A2 items):**
1. Taskbar badge: Task 5 (bitmap), 6 (`SetOverlayIcon`), 8 (`sync_attention`, count = Inbox definition, only on change); macOS/Linux no-op with the exact blocker (no gpui call; no verifiable objc dep) in Design + ADR.
2. Flash: Task 4 (`entered_needs_input`), 6 (`FlashWindowEx`), 8 (trigger, stop on activation / zero / setting off); setting Task 1, hot reload Task 8 step 4(d).
3. Tray: Task 7 (icon states, menu: ≤8 `agent · workspace — state, age`, `● N working`, Open, Quit UI), close-to-tray Task 8 step 3 using the existing `on_window_should_close` (verified `connection.rs:44`, gpui `window.rs:6658`, Windows `events.rs:336`), hide/show verified (gpui has no public hide → HWND `ShowWindow`; fallback analysis in Design), crate evaluation (`tray-icon`/`muda` absent from `Cargo.lock`; `windows 0.62.2` present), event loop question answered (own thread).
4. Mark unread: "viewed" found to be UI memory (`inbox::Viewed`, `main.rs:414`), so no protocol/migration (Tasks 2, 3); pane/row menu + palette.
5. ADR 0063 + SPEC (§10, §11, §19, §20.0, new §20J): Task 1. Counts only from data the UI has, updates on change only: Tasks 4 and 8.

**Placeholder scan:** none ("TBD"/"similar to"/"handle edge cases" absent); every code step has code. Task 6 deliberately returns a closed receiver and `has_tray() == false` and Task 7 replaces exactly those items with the real tray (shown in full there).

**Type consistency:** `Counts`, `Tracker::update -> Change { badge, tray }`, `TrayMenu`, `Item::text(now_ms)`, `Icon`, `Desktop::{new, has_tray, set_badge, flash, set_tray, remove_tray, hide_window, show_window}`, `TrayEvent::{Open, Session, Quit}`, `Viewed::{view -> bool, activated, mark_unread}`, `inbox::can_mark_unread`, `Shell::{sync_attention(cx), restore_window(window), mark_unread(id, cx), can_mark_unread(id), ask_unsaved(then, window, cx)}` are used with the same signatures across tasks. `Icon` in `shell/attention.rs` is written `attention::Icon` because `main.rs` already imports `icons::Icon`.

**Review Focus → tests:** 1 `closing_hides_only_while_the_tray_icon_is_there` (Task 4); 2 `mark_unread_puts_a_viewed_done_back_until_it_is_viewed_again` (Task 2); 3 `twelve_waiting_sessions_list_eight_and_count_twelve` + `nine_plus_fits_inside_the_square` (Tasks 4, 5); 4 `the_same_count_with_another_set_still_refreshes_the_tray` (Task 4); 5 HUMAN_CHECKS (Task 9) — `Shell` is not constructible without a window.
