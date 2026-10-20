# In-app Toast Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the in-app toast layer that SPEC §14 already promises, and send it the results of user actions that fail silently today (only a `tracing` line, or nothing).

**Architecture:** A pure queue `Toasts` (`crates/mkrew-ui/src/toast.rs`: push / dismiss / expire with injected `Instant`, max 3, dedupe) is owned by `Shell`. `shell/toasts.rs` draws it bottom-right over the content area, and the existing 500 ms status tick expires it. Failures that already have a place of their own (in-dialog error row, commit box, tab bar notice, card notice) stay there; only results with no place left become toasts.

**Tech Stack:** Rust 2024, GPUI (`gpui-pre =0.3.7`), existing Lucide icon pipeline (ADR 0051), `mkrew_ui::theme` tokens.

**Spec:** docs/SPEC.md §14 (design system: the bullet "In-app toasts: bottom-right, auto-hide after 4 s, never blocking.", the Icons bullet, tokens), §20.0 (settings errors are listed in the palette); design decided in chat 2026-10-19, recorded in the "Design" section of this plan.

## Global Constraints

- SPEC §14, verbatim: "In-app toasts: bottom-right, auto-hide after 4 s, never blocking." (Task 2 tightens the wording; the behavior does not change, so **no ADR**.)
- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens. Spacing only 4 · 8 · 12 · 16 · 24 · 32 (`theme::space::{XS,S,M,L,XL,XXL}`); radius 4 / 8 / 12 (`theme::radius`). Both themes (dark + light). Text contrast ≥ 4.5:1. Status is never conveyed by color alone (icon + text).
- No animation is added (motion rule "120–180 ms ease-out, respect reduce-motion" is trivially met).
- No `unwrap` / `expect` outside tests. `thiserror` in libraries, `tracing` for logs. `cargo fmt`; `cargo clippy --workspace --all-targets -- -D warnings` clean. Edition 2024.
- Tests never call a real agent or the network. The `mkrew` binary has no headless GPUI harness (`TestAppContext` / `test-support` are not used anywhere in `crates/mkrew-ui`), so tests cover the pure code (`toast.rs`, `composer.rs`, `files_view.rs`, `theme.rs`) and the render / async paths go in `docs/HUMAN_CHECKS.md`.
- Commits: Conventional Commits, `git -c user.email=tungnv975@gmail.com commit`, **stage explicit paths only** (never `add -A`). Never commit `Cargo.lock`, `docs/REDEPLOY_VI.md`, `docs/BUILD_VI.md`, `.github`. At plan time these files already carry other people's uncommitted changes: `crates/mkrew-ui/src/run.rs`, `crates/mkrew-ui/src/shell/run.rs`, `crates/mkrew-ui/src/shell/sidebar.rs`, `docs/HUMAN_CHECKS.md`. For those, stage only your own hunks (`git add -p <path>`).
- Line numbers below are from `fb1ac83` plus that dirty tree. **Locate every edit by the quoted symbol/code, re-read the range first**, because the other 2026-10-19 plans (jira board, sidebar workspace cards, tray/badge, viewed files, phone push) also touch `main.rs` (`struct Shell`), `connection.rs`, `sidebar.rs`, `theme.rs`.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`. Per-crate loop: `cargo test -p mkrew-ui <filter> -j 4`. There is no protocol change, so no `mkrew-cli` / `mkrew-fakeagent` rebuild is needed. `cargo insta` is not installed (this plan adds no snapshot).
- Numbers are reserved for planning; before creating, re-check the newest ADR/migration/PROTOCOL_VERSION and take the next free one if another plan landed first. This plan uses **no** ADR (0062 stays free), **no** migration, **no** protocol change.

## Review Focus

Input classes the spec implies but a happy-path test would miss, most likely first:

1. **The same failure repeating** (a batch of failing card moves, "Follow in Zed" failing on every agent edit, a panel dragged ten times with a broken `settings.toml`): must not stack copies or push out other toasts; the same text restarts its timer. Pinned by Task 1 tests `the_same_message_restarts_its_toast_instead_of_stacking` and `a_full_stack_drops_the_oldest_and_a_repeat_drops_nothing`.
2. **A click on ✕ just as the toast expires, or a double click**: `dismiss` of an id that is gone must be a harmless `false`, and ids are never reused so a late click never closes a newer toast. Pinned by Task 1 test `dismiss_removes_only_its_toast_and_a_late_click_is_harmless`.
3. **A very long or multi-line error** (git hook stderr, an `anyhow` chain): the card keeps its width and shows at most `TOAST_MAX_LINES` lines; it never grows past the window. Render-only: HUMAN_CHECKS (Task 6).
4. **A toast while a modal dialog is open or a terminal has the keyboard**: drawn above the dialog shade, takes no focus, Esc still closes the dialog and typing still reaches the terminal / field, only the card's own area absorbs clicks. Render-only: HUMAN_CHECKS (Task 6).
5. **A result that arrives after the place that would show it is gone** (Esc closed the Undo / task / Run setup dialog, the user left the file being saved, the Git view moved to another repo): a toast instead of silence. Pinned by Task 3 test `a_save_that_comes_back_after_the_file_was_left_is_reported`, the rest by HUMAN_CHECKS. The opposite case, an *expected* race (`session.unqueue` → `not_found` because the text was just written), must stay silent: Task 4 test `a_text_written_meanwhile_is_not_an_unqueue_failure`.

---

## Design

Decisions (each default and why, one line):

- **One stack owned by `Shell`** (`toasts: Toasts`), drawn as the last child of the content area `div` in `Render for Shell` (`main.rs`, the `.relative()` flex row below the title bar) so it sits bottom-right over the content and above every dialog shade. Why: no new entity, same pattern as every menu / dialog.
- **Max 3 visible, oldest dropped; same kind + same text restarts instead of stacking.** Why: spam control with 6 lines of code; covers repeated failures.
- **Expiry rides the existing 500 ms `STATUS_TICK` loop** (`shell/connection.rs`), not a timer per toast, so a toast hides after 4.0–4.5 s (SPEC wording becomes "about 4 s"). Why: no timers to cancel or race, no extra tasks, and a timer that fires a hair early could strand a toast forever.
- **Hover pauses nothing; no animation; no auto-focus.** Why: the brief says keep it simple; the card is a plain `occlude()` element (like `menu_panel`), never focusable.
- **Kinds: `Info` and `Error`.** Error = Lucide `circle-alert` in `status_exited` (the token every existing error mark already uses, e.g. `shell/composer.rs` "✕", `status_exited`) + text in `text`; Info = Lucide `info` in `accent_text`. Two new SVGs vendored with the documented ADR 0051 procedure ("Adding an icon"); Task 1 pins their contrast on the card (`surface_raised`) in both themes. ✕ dismiss reuses `Icon::X`.
- **Info toasts only where a click has no other feedback today**: "Copy path" and "Copy link" (Shell-level copies). Not added to every action (YAGNI).
- **Keep in place** whatever already has a visible place (table below) and **every in-dialog error where the dialog stays open**, including Phone access Start / Stop (`shell/phone_access.rs`): its comment "(there is no toast layer)" is corrected, the behavior stays.
- **Settings writes** (`write_ui_value` failing) keep `settings_error` (palette entry, §20.0) **and** also toast, because the palette entry is only seen if the user opens the palette.
- **No ADR.** Behavior stays "bottom-right, auto-hide ~4 s, never blocking"; Task 2 only tightens SPEC §14 wording (kinds, max 3, icons, when used).

### Silent-failure inventory (verified by reading each site)

**Routed to a toast** (today: `tracing` only, or dropped):

| Site (path:line @ fb1ac83) | Action | Today | Task |
| --- | --- | --- | --- |
| `shell/composer.rs:108` | paste image into composer, `attachment.save` fails | log | 4 |
| `terminal/view.rs:687-690`, `:704` | paste image into a terminal: over 10 MiB / `attachment.save` fails | log | 4 |
| `shell/composer.rs:389-395` | ✕ on a queued text, `session.unqueue` fails (except `not_found`) | log | 4 |
| `shell/history.rs:44-54` | Follow in Zed, `zed::open` fails (background) | log | 4 |
| `shell/inspector.rs:37`, `shell/workspace_actions.rs:403`, `:434`, `:450` | save panel width / theme / diff layout / diff wrap | `settings_error` (palette only) | 5 |
| `shell/dialogs.rs:197-203` | `task.undo` / `turn.restore` fails after Esc closed the dialog | log (HUMAN_CHECKS 2278 admits it) | 3 |
| `shell/task_dialog.rs:131-138`, `:191-198` | task save / delete fails after the dialog closed | log | 3 |
| `shell/git_dialogs.rs:75-82`, `:185-191` | `git.discard` / `git.set_base` fails after the dialog closed | log | 3 |
| `shell/run.rs:252-254` | `project.update` (Run setup) fails after the dialog closed | dropped | 3 |
| `shell/send_dialog.rs:150-157` | `comment.send_to_agent` fails after the dialog closed | log | 3 |
| `shell/worktrees.rs:288-290` | merge / remove worktree fails after the dialog closed | dropped | 3 |
| `shell/git.rs:625-627`, `:709-711`, `:791-793` | stage / commit / fetch / push result when the Git view moved to another repo or worktree | dropped (`git_at` → `None`) | 3 |
| `shell/files.rs:506-508` + `files_view.rs:702-707` | file save result when the Files view or the opened file is gone | dropped | 3 |
| `shell/workspace_actions.rs:326-327` | Copy path | no feedback | 2 (info) |
| `shell/phone_access.rs:126-127` | Copy link | no feedback | 2 (info) |

**Kept where they are** (already visible; not silent): `git.rs:623/706/788` rows under the diff header / commit box / header; `files.rs:503` `save_error` banner; `run.rs:37,76` and `workspace_actions.rs` `sidebar_error`; `sessions.rs:50`, `tabs.rs:139,438,599,725`, `workspace.rs:156,319` `tab_error`; `board.rs:110,150,256,341` `card_error`; `board.rs:471`, `composer.rs:181`, `peek.rs:350`, `dialogs.rs:70`, `comments.rs:282` drafts / dialogs with their own error row; `ask_dialog.rs:97-105`, `git.rs:746-757`, `composer.rs:144-153` generation errors in their dialog / box; `phone_access.rs:93` (dialog open); `history.rs:426`, `worktrees.rs:306`, `send_dialog.rs:155` (dialog open).

**Passive, not user-triggered, deliberately not toasted**: `connection.rs:455,464,492,537,565,593` reloads (board / sidebar show their own error), `git.rs:123,189,377` background loads, `terminal/view.rs:248` resize / typed-text transport (a dead daemon already shows the connection state), `pane_render.rs:897` detach, `voice.rs:176`, `tabs.rs:40`.

---

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/mkrew-ui/src/toast.rs` (create) | pure queue: `Kind`, `Toast`, `Toasts`, `MAX_VISIBLE` |
| `crates/mkrew-ui/src/shell/toasts.rs` (create) | `Shell::toast_error / toast_info / settings_write_failed / render_toasts` |
| `icon/lucide/circle-alert.svg`, `icon/lucide/info.svg` (create) | the two new icons |
| `crates/mkrew-ui/src/theme.rs` | `TOAST_WIDTH`, `TOAST_MAX_LINES`, contrast test |
| `crates/mkrew-ui/src/icons.rs`, `lib.rs`, `main.rs`, `shell.rs`, `shell/connection.rs` | wiring |
| the sites in the inventory | one small edit each |

---

### Task 1: The toast queue and its tokens

**Files:**
- Create: `crates/mkrew-ui/src/toast.rs`
- Modify: `crates/mkrew-ui/src/lib.rs:14-15` (doc sentence) and `:75` (`pub mod thread_view;` → add `pub mod toast;` after it)
- Modify: `crates/mkrew-ui/src/theme.rs:62-63` (`TOAST_DURATION`), tests module (after `fn code_stays_readable`)
- Test: both files above

**Interfaces:**
- Consumes: `crate::theme::TOAST_DURATION: Duration` (exists, `theme.rs:63`, currently unused).
- Produces (Task 2 uses these exact names):
  - `pub const MAX_VISIBLE: usize = 3;`
  - `pub enum Kind { Info, Error }` (`Clone, Copy, Debug, PartialEq, Eq`)
  - `pub struct Toast { pub id: u64, pub kind: Kind, pub text: String, pub expires: Instant }`
  - `pub struct Toasts` (`Debug, Default`) with `push(&mut self, kind: Kind, text: impl Into<String>, now: Instant)`, `dismiss(&mut self, id: u64) -> bool`, `expire(&mut self, now: Instant) -> bool`, `items(&self) -> &[Toast]`
  - `theme::TOAST_WIDTH: f32 = 320.0`, `theme::TOAST_MAX_LINES: usize = 4`

- [ ] **Step 1: Write the failing tests**

Create `crates/mkrew-ui/src/toast.rs` with only the module doc and the tests:

```rust
//! In-app toasts (SPEC §14): a short queue of messages drawn bottom-right over the content.
//! Pure: the time is passed in, so the tests need no clock. `shell/toasts.rs` draws it and
//! the status tick expires it.

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::theme::TOAST_DURATION;

    fn later(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn texts(toasts: &Toasts) -> Vec<&str> {
        toasts
            .items()
            .iter()
            .map(|toast| toast.text.as_str())
            .collect()
    }

    #[test]
    fn a_toast_hides_exactly_when_toast_duration_has_passed() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push(Kind::Error, "Cannot save the theme", base);
        assert!(!toasts.expire(base + TOAST_DURATION - Duration::from_millis(1)));
        assert_eq!(texts(&toasts), ["Cannot save the theme"]);
        assert!(toasts.expire(base + TOAST_DURATION));
        assert!(toasts.items().is_empty());
        assert!(!toasts.expire(base + TOAST_DURATION * 2), "nothing left to drop");
    }

    #[test]
    fn the_oldest_toast_expires_first_and_the_newer_one_keeps_its_own_time() {
        let base = Instant::now();
        let half = TOAST_DURATION / 2;
        let mut toasts = Toasts::default();
        toasts.push(Kind::Info, "a", base);
        toasts.push(Kind::Info, "b", base + half);
        assert!(toasts.expire(base + TOAST_DURATION));
        assert_eq!(texts(&toasts), ["b"]);
        assert!(toasts.expire(base + half + TOAST_DURATION));
        assert!(toasts.items().is_empty());
    }

    #[test]
    fn a_full_stack_drops_the_oldest_and_a_repeat_drops_nothing() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        for text in ["a", "b", "c", "d"] {
            toasts.push(Kind::Error, text, base);
        }
        assert_eq!(toasts.items().len(), MAX_VISIBLE);
        assert_eq!(texts(&toasts), ["b", "c", "d"], "the oldest went");
        let ids: Vec<u64> = toasts.items().iter().map(|toast| toast.id).collect();
        toasts.push(Kind::Error, "b", later(base, 1));
        let again: Vec<u64> = toasts.items().iter().map(|toast| toast.id).collect();
        assert_eq!(again, ids, "a repeat of a shown message drops nobody");
    }

    #[test]
    fn the_same_message_restarts_its_toast_instead_of_stacking() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push(Kind::Error, "Cannot follow in Zed: not found", base);
        let first = toasts.items()[0].id;
        let again = later(base, 3_000);
        toasts.push(Kind::Error, "Cannot follow in Zed: not found", again);
        assert_eq!(toasts.items().len(), 1);
        assert_eq!(toasts.items()[0].id, first);
        assert!(
            !toasts.expire(base + TOAST_DURATION),
            "restarted: the first start no longer counts"
        );
        assert!(toasts.expire(again + TOAST_DURATION));
        // The same words with another kind are another message.
        toasts.push(Kind::Info, "x", base);
        toasts.push(Kind::Error, "x", base);
        assert_eq!(toasts.items().len(), 2);
    }

    #[test]
    fn dismiss_removes_only_its_toast_and_a_late_click_is_harmless() {
        let base = Instant::now();
        let mut toasts = Toasts::default();
        toasts.push(Kind::Info, "a", base);
        toasts.push(Kind::Info, "b", base);
        let a = toasts.items()[0].id;
        assert!(toasts.dismiss(a));
        assert_eq!(texts(&toasts), ["b"]);
        assert!(!toasts.dismiss(a), "already gone: expired or clicked twice");
        assert_eq!(texts(&toasts), ["b"]);
        toasts.push(Kind::Info, "c", base);
        assert_ne!(toasts.items()[1].id, a, "ids are never reused");
    }
}
```

Add `pub mod toast;` to `lib.rs` right after `pub mod thread_view;` so the module is compiled.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui toast -j 4`
Expected: FAIL to compile (`cannot find type Toasts` / `Kind` / `MAX_VISIBLE`).

- [ ] **Step 3: Write the implementation**

In `toast.rs`, put this between the module doc and `#[cfg(test)]`:

```rust
use std::time::Instant;

use crate::theme::TOAST_DURATION;

/// Most toasts shown at once; a new one past this drops the oldest.
pub const MAX_VISIBLE: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Info,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    /// Never reused, so a late click on ✕ cannot close a newer toast.
    pub id: u64,
    pub kind: Kind,
    pub text: String,
    /// It is dropped by the first [`Toasts::expire`] at or after this time.
    pub expires: Instant,
}

/// The shown toasts, oldest first.
#[derive(Debug, Default)]
pub struct Toasts {
    next_id: u64,
    items: Vec<Toast>,
}

impl Toasts {
    /// Shows `text`. The same kind and text as a toast already shown restarts that one instead
    /// of stacking a copy (a failure that repeats); past [`MAX_VISIBLE`] the oldest goes.
    pub fn push(&mut self, kind: Kind, text: impl Into<String>, now: Instant) {
        let text = text.into();
        // Overflow of an `Instant` is not reachable in practice; if it ever were, the toast
        // would just hide at the next tick.
        let expires = now.checked_add(TOAST_DURATION).unwrap_or(now);
        if let Some(shown) = self
            .items
            .iter_mut()
            .find(|toast| toast.kind == kind && toast.text == text)
        {
            shown.expires = expires;
            return;
        }
        self.next_id += 1;
        self.items.push(Toast {
            id: self.next_id,
            kind,
            text,
            expires,
        });
        if self.items.len() > MAX_VISIBLE {
            self.items.remove(0);
        }
    }

    /// The ✕ of toast `id`. False when it is already gone.
    pub fn dismiss(&mut self, id: u64) -> bool {
        let before = self.items.len();
        self.items.retain(|toast| toast.id != id);
        self.items.len() != before
    }

    /// Drops what has expired by `now`. True when something went (the window needs a redraw).
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.items.len();
        self.items.retain(|toast| toast.expires > now);
        self.items.len() != before
    }

    pub fn items(&self) -> &[Toast] {
        &self.items
    }
}
```

In `lib.rs`, change the end of the module doc (lines 14-15) from
`//! input (microphone and speech to text), the phone-access link, token and QR code and its` / `//! dialog model, and the scroll bar of the Files editor and the Markdown preview.` to
`//! dialog model, the toast queue, and the scroll bar of the Files editor and the Markdown preview.` (keep the first line, replace the second).

In `theme.rs` replace lines 62-63

```rust
/// In-app toasts auto-hide after this long.
pub const TOAST_DURATION: Duration = Duration::from_secs(4);
```

with

```rust
/// In-app toasts auto-hide after this long (expired on the [`STATUS_TICK`], so up to one tick
/// later).
pub const TOAST_DURATION: Duration = Duration::from_secs(4);

/// Width of a toast card (px at the default 13 px body, drawn with [`rems_px`]) and the lines
/// its text shows before it is cut.
pub const TOAST_WIDTH: f32 = 320.0;
pub const TOAST_MAX_LINES: usize = 4;
```

In `theme.rs`'s `mod tests`, insert this test right after `fn code_stays_readable` (before `fn highlighted_diff_lines_stay_readable`):

```rust
    /// A toast card is `surface_raised`: its text, its two marks (info accent, error red) and
    /// the ✕ must read on it in both appearances.
    #[test]
    fn toast_text_and_marks_read_on_the_card() {
        for theme in [Theme::dark(), Theme::light()] {
            let ratio = contrast(theme.text, theme.surface_raised);
            assert!(ratio >= 4.5, "text: {ratio:.2}");
            for (name, mark) in [
                ("info", theme.accent_text),
                ("error", theme.status_exited),
                ("close", theme.text_muted),
            ] {
                let ratio = contrast(mark, theme.surface_raised);
                assert!(ratio >= 3.0, "{name}: {ratio:.2}");
            }
        }
    }
```

(If `close` fails on a theme, use `theme.text_dim` for the ✕ in Task 2 and here; do not change a palette token.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui toast -j 4`
Expected: PASS (5 queue tests + `toast_text_and_marks_read_on_the_card`; the theme test guards existing tokens, so it passes on first run).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/toast.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/theme.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): toast queue with expiry, dedupe and tokens"
```

(`theme.rs` and `lib.rs` may carry other plans' hunks: use `git add -p` for them if `git diff` shows more than yours.)

---

### Task 2: The toast layer: icons, Shell state, render, expiry, info toasts, SPEC wording

**Files:**
- Create: `icon/lucide/circle-alert.svg`, `icon/lucide/info.svg`, `crates/mkrew-ui/src/shell/toasts.rs`
- Modify: `crates/mkrew-ui/src/icons.rs:52,64` (two `icons!` lines)
- Modify: `crates/mkrew-ui/src/shell.rs` (add `mod toasts;`)
- Modify: `crates/mkrew-ui/src/main.rs` (import near `:103-104`, field after `sidebar_view` `:608`, render child after the `file_menu` children `:796-804`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`Shell { … }` literal `:415`; status tick before `shell.sync_thread(cx);` `:94`)
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs:325-330` (`copy_file_path`), `crates/mkrew-ui/src/shell/phone_access.rs:82-83,125-130`
- Modify: `docs/SPEC.md:1374`
- Test: `crates/mkrew-ui/src/icons.rs` existing test `every_icon_is_served_and_is_a_lucide_svg` covers the new files

**Interfaces:**
- Consumes (Task 1): `toast::{Kind, Toasts}` with `push`, `dismiss`, `expire`, `items`; `theme::{TOAST_WIDTH, TOAST_MAX_LINES}`.
- Consumes (existing): `menu_panel(id, theme) -> Stateful<Div>` (`shell/widgets.rs:540`, `occlude()`d, `surface_raised`, border, `shadow_md`), `icon_svg(Icon, f32, u32) -> Svg` (`icons.rs:152`), `tooltip(theme, &str, keys) -> impl Fn(..)` (`widgets.rs:835`), `theme::rems_px`.
- Produces (Tasks 3-5 use these exact signatures), all on `impl Shell` in `shell/toasts.rs`:
  - `pub(crate) fn toast_error(&mut self, text: impl Into<String>, cx: &mut Context<'_, Self>)`
  - `pub(crate) fn toast_info(&mut self, text: impl Into<String>, cx: &mut Context<'_, Self>)`
  - `pub(crate) fn settings_write_failed(&mut self, what: &str, error: impl std::fmt::Display, cx: &mut Context<'_, Self>)`
  - `pub(crate) fn render_toasts(&self, theme: &Theme, cx: &mut Context<'_, Self>) -> Option<gpui::AnyElement>`
  - `Shell.toasts: Toasts`

- [ ] **Step 1: Vendor the two icons (the ADR 0051 "Adding an icon" procedure)**

Create `icon/lucide/circle-alert.svg` exactly:

```
<!-- @license lucide-static v1.55.0 - ISC -->
<svg
  class="lucide lucide-circle-alert"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <circle cx="12" cy="12" r="10" />
  <line x1="12" x2="12" y1="8" y2="12" />
  <line x1="12" x2="12.01" y1="16" y2="16" />
</svg>
```

Create `icon/lucide/info.svg` exactly:

```
<!-- @license lucide-static v1.55.0 - ISC -->
<svg
  class="lucide lucide-info"
  xmlns="http://www.w3.org/2000/svg"
  width="24"
  height="24"
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <circle cx="12" cy="12" r="10" />
  <path d="M12 16v-4" />
  <path d="M12 8h.01" />
</svg>
```

(Both are byte-for-byte what `https://unpkg.com/lucide-static@1.55.0/icons/<name>.svg` serves; the existing `x.svg` has the same header.)

In `icons.rs`, in the `Icon in "lucide"` list add `CircleAlert => "circle-alert",` on its own line before `CircleStop => "circle-stop",` (`:52`) and `Info => "info",` after `Inbox => "inbox",` (`:64`).

- [ ] **Step 2: Run the icon test**

Run: `cargo test -p mkrew-ui icons -j 4`
Expected: PASS (`every_icon_is_served_and_is_a_lucide_svg` now also checks both files exist, contain `<svg`, and `viewBox="0 0 24 24"`). A missing file fails with `failed to read … include_bytes`.

- [ ] **Step 3: Shell state and wiring**

`main.rs`: after the line `use mkrew_ui::theme::{self, ActiveTheme, Theme, font_size, opacity, radius, space, weight};` add

```rust
use mkrew_ui::toast::{self, Toasts};
```

(keeps rustfmt's alphabetical order: `theme` < `toast` < `turn`).

`main.rs`, `struct Shell`, after the `sidebar_view` field (last field, before `}`):

```rust
    /// The in-app toasts (SPEC §14): pushed by `toast_error` / `toast_info`, expired on the
    /// status tick.
    toasts: Toasts,
```

`shell/connection.rs`, in the `Self { … }` literal, after `sidebar_view: None,`:

```rust
            toasts: Toasts::default(),
```

`shell/connection.rs`, in the status tick closure (`this.update_in(cx, |shell, window, cx| { … })`), right before the line `shell.sync_thread(cx);`:

```rust
                    // Toasts hide here, within one tick of their time (SPEC §14).
                    if shell.toasts.expire(Instant::now()) {
                        cx.notify();
                    }
```

`shell.rs`: add `mod toasts;` next to `mod tabs;` (the second `mod` group is not strictly sorted).

`main.rs`, `Render for Shell`: the content area's last child call is today

```rust
                    .children(
                        (self.file_menu.as_ref())
                            .filter(|menu| { /* … */ })
                            .map(|menu| self.render_file_menu(menu, &theme, cx)),
                    ),
```

The final `,` of that block is the trailing comma of the enclosing `.child(` argument. Drop it from the file_menu `.children(..)` and put it after the new call, so the sequence ends `… render_file_menu(…)), ),` → `… render_file_menu(…))` then:

```rust
                    .children(self.render_toasts(&theme, cx)),
```

- [ ] **Step 4: Write `shell/toasts.rs`**

```rust
//! The in-app toast stack (SPEC §14): `toast_error` / `toast_info` push, the status tick
//! (`connection.rs`) expires, `render_toasts` draws it bottom-right over the content.

use crate::*;

impl Shell {
    /// A failure that has no place of its own to show in (its dialog closed, its view moved).
    pub(crate) fn toast_error(&mut self, text: impl Into<String>, cx: &mut Context<'_, Self>) {
        self.toasts.push(toast::Kind::Error, text, Instant::now());
        cx.notify();
    }

    /// Feedback for an action that otherwise shows nothing (a copy).
    pub(crate) fn toast_info(&mut self, text: impl Into<String>, cx: &mut Context<'_, Self>) {
        self.toasts.push(toast::Kind::Info, text, Instant::now());
        cx.notify();
    }

    /// A failed write of `settings.toml`: the palette lists the file's problem (§20.0) and the
    /// toast says the choice was not saved.
    pub(crate) fn settings_write_failed(
        &mut self,
        what: &str,
        error: impl std::fmt::Display,
        cx: &mut Context<'_, Self>,
    ) {
        let error = error.to_string();
        self.toast_error(format!("Cannot save {what}: {error}"), cx);
        self.settings_error = Some(error);
    }

    /// The stack, newest at the bottom, in the bottom-right corner of the content area. The
    /// wrapper has no hitbox: only a card (like a menu panel) absorbs clicks, and nothing here
    /// takes the keyboard.
    pub(crate) fn render_toasts(
        &self,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> Option<gpui::AnyElement> {
        if self.toasts.items().is_empty() {
            return None;
        }
        let cards = self.toasts.items().iter().map(|item| {
            // Icon + text, never color alone.
            let (glyph, color) = match item.kind {
                toast::Kind::Info => (Icon::Info, theme.accent_text),
                toast::Kind::Error => (Icon::CircleAlert, theme.status_exited),
            };
            let id = item.id;
            let hover = theme.hover;
            menu_panel(("toast", id), theme)
                .w(theme::rems_px(theme::TOAST_WIDTH))
                .flex_row()
                .items_center()
                .gap(px(space::S))
                .px(px(space::M))
                .py(px(space::S))
                .child(icon_svg(glyph, theme::ICON_SIZE_SMALL, color))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .line_clamp(theme::TOAST_MAX_LINES)
                        .text_size(font_size::BODY)
                        .text_color(rgb(theme.text))
                        .child(item.text.clone()),
                )
                .child(
                    div()
                        .id(("toast-close", id))
                        .flex_none()
                        .p(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .active(|style| style.opacity(opacity::ACTIVE))
                        .tooltip(tooltip(theme, "Dismiss", ""))
                        .child(icon_svg(Icon::X, theme::ICON_SIZE_SMALL, theme.text_muted))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.toasts.dismiss(id);
                            cx.notify();
                        })),
                )
        });
        Some(
            div()
                .absolute()
                .bottom(px(space::L))
                .right(px(space::L))
                .flex()
                .flex_col()
                .gap(px(space::S))
                .children(cards)
                .into_any_element(),
        )
    }
}
```

- [ ] **Step 5: Info toasts for the two copies**

`shell/workspace_actions.rs`, `copy_file_path`: replace

```rust
        if let Some(full) = self.target_file(project_id, target, path) {
            cx.write_to_clipboard(ClipboardItem::new_string(full.display().to_string()));
        }
        cx.notify();
```

with

```rust
        if let Some(full) = self.target_file(project_id, target, path) {
            cx.write_to_clipboard(ClipboardItem::new_string(full.display().to_string()));
            self.toast_info("Path copied", cx);
        }
        cx.notify();
```

`shell/phone_access.rs`, `copy_phone_link`: replace

```rust
        if let Some(text) = self.phone.copy_link() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        cx.notify();
```

with

```rust
        if let Some(text) = self.phone.copy_link() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            // The text says "Link copied", never the link: it carries the access token.
            self.toast_info("Link copied", cx);
        }
        cx.notify();
```

`shell/phone_access.rs:82-83`: replace the doc comment

```rust
    /// Start / Stop: `[web] enabled` in `settings.toml`; the daemon's settings watcher does the
    /// rest. A failed write shows in the dialog (there is no toast layer).
```

with

```rust
    /// Start / Stop: `[web] enabled` in `settings.toml`; the daemon's settings watcher does the
    /// rest. A failed write shows in the dialog, which stays open (a failure with a place of its
    /// own is not a toast, SPEC §14).
```

- [ ] **Step 6: Tighten SPEC §14**

In `docs/SPEC.md` replace the single line (verified at `:1374`)

```markdown
- In-app toasts: bottom-right, auto-hide after 4 s, never blocking.
```

with

```markdown
- In-app toasts (`crates/mkrew-ui/src/toast.rs` queue, `shell/toasts.rs` card): bottom-right over
  the content, at most 3 (a fourth drops the oldest; the same message again restarts its toast),
  each hides after about 4 s (`TOAST_DURATION`, expired on the 500 ms status tick, so 4.0–4.5 s) or
  on its ✕. Never blocking: no focus, no animation, only the card itself takes clicks. Two kinds,
  always icon + text: info (Lucide `info`, `accent-text`) and error (Lucide `circle-alert`,
  `status-exited`). Used for results of an action that has no place of its own (a failure after its
  dialog closed or its view moved, a failed settings write, a copy); a failure that has one (a
  dialog's error row, the commit box, the tab bar notice, a card notice) stays there.
```

- [ ] **Step 7: Compile**

Run: `cargo check -p mkrew-ui -j 4`
Expected: no errors. If `let Some(view) = … else` / `if let … else` borrow errors show up in later tasks they are fixed there; this task has none. If `.bottom(px(..))` is rejected, use `.bottom_0()` and a `.pb(px(space::L))` wrapper (existing code uses `.bottom_0()`, `.left(px(..))`, `.right(px(..))`, `.top(px(..))`; `bottom` comes from the same `position_style_methods!` macro).

- [ ] **Step 8: Commit**

```bash
git add icon/lucide/circle-alert.svg icon/lucide/info.svg crates/mkrew-ui/src/shell/toasts.rs crates/mkrew-ui/src/icons.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/shell/workspace_actions.rs crates/mkrew-ui/src/shell/phone_access.rs docs/SPEC.md
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): in-app toast layer (SPEC 14), copy path/link feedback"
```

(`main.rs`, `connection.rs`, `workspace_actions.rs`: `git add -p` if another plan's hunks are present.)

---

### Task 3: Results that arrive with nowhere to show

Everything here is the same shape: a result comes back after the dialog / view / file that would have shown it is gone, and today it is dropped or logged. Each site gets a toast in its "gone" branch; the in-place branch is unchanged.

**Files:**
- Modify: `crates/mkrew-ui/src/files_view.rs:699-719` (`saved` returns `Option<String>`, new `unshown_save`)
- Test: `crates/mkrew-ui/src/files_view/tests.rs` (after `a_save_marks_what_it_sent_and_one_runs_at_a_time`)
- Modify: `crates/mkrew-ui/src/shell/files.rs:499-514`, `shell/git.rs` (`stage_git` `:621-630`, `commit_git` `:705-714`, `run_git_remote` `:787-796`), `shell/dialogs.rs:197-203`, `shell/task_dialog.rs:131-138,191-198`, `shell/git_dialogs.rs:75-82,185-191`, `shell/run.rs:251-254`, `shell/send_dialog.rs:150-157`, `shell/worktrees.rs:287-290`

**Interfaces:**
- Consumes (Task 2): `Shell::toast_error(&mut self, impl Into<String>, &mut Context<'_, Shell>)`.
- Produces: `FilesView::saved(&mut self, path: &str, point: Option<u64>, written: Result<FsWritten, String>) -> Option<String>` and `pub fn unshown_save(written: Result<FsWritten, String>) -> Option<String>` in `files_view.rs`. Task 6 documents them; nothing else uses them.

- [ ] **Step 1: Write the failing test**

In `files_view/tests.rs`, after `a_save_marks_what_it_sent_and_one_runs_at_a_time`:

```rust
#[test]
fn a_save_that_comes_back_after_the_file_was_left_is_reported() {
    let mut view = opened("a\n");
    view.editor_mut().unwrap().insert("b");
    let save = view.begin_save().unwrap();
    // The user opened another file while the write was in flight.
    view.open("src/main.rs");
    let failed = view.saved("README.md", save.point, Err("disk full".into()));
    assert_eq!(failed.as_deref(), Some("Not saved: disk full"));
    let newer = FsWritten {
        saved: false,
        hash: "#other".into(),
    };
    assert_eq!(
        view.saved("README.md", None, Ok(newer)).as_deref(),
        Some("Not saved: the file changed on disk meanwhile.")
    );
    let fine = FsWritten {
        saved: true,
        hash: "#x".into(),
    };
    assert_eq!(view.saved("README.md", None, Ok(fine)), None, "a good save is no news");
    // Shown: the banner has it, nothing more to say.
    let mut view = opened("a\n");
    view.editor_mut().unwrap().insert("b");
    let save = view.begin_save().unwrap();
    assert_eq!(view.saved("README.md", save.point, Err("disk full".into())), None);
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p mkrew-ui a_save_that_comes_back -j 4`
Expected: FAIL to compile (`saved` returns `()`; comparing `()` with `Option<String>`).

- [ ] **Step 3: Implement**

`files_view.rs`, replace `saved` (`:699-719`) with:

```rust
    /// The `fs.write` answer for `path`, which saved the state `point`. `Some(why)` when the
    /// answer has nowhere to show because the user opened another file meanwhile: the shell
    /// toasts it.
    pub fn saved(
        &mut self,
        path: &str,
        point: Option<u64>,
        written: Result<FsWritten, String>,
    ) -> Option<String> {
        self.saving = false;
        if self.file.as_deref() != Some(path) {
            return unshown_save(written);
        }
        let Content::Shown(shown) = &mut self.content else {
            return unshown_save(written);
        };
        match written {
            Ok(written) if written.saved => {
                shown.editor.mark_saved(point);
                shown.hash = Some(written.hash);
                shown.save_error = None;
                shown.disk_changed = None;
            }
            // Another version on disk: the banner offers Reload / Keep mine.
            Ok(written) => shown.disk_changed = Some(written.hash),
            Err(error) => shown.save_error = Some(format!("Not saved: {error}")),
        }
        None
    }
```

and, as a free function right after the `impl FilesView` block that contains `saved` (or directly above `impl FilesView`), add:

```rust
/// A save's answer that has no file view to land in: only a failure or a refused save is news.
pub fn unshown_save(written: Result<FsWritten, String>) -> Option<String> {
    match written {
        Ok(written) if written.saved => None,
        Ok(_) => Some("Not saved: the file changed on disk meanwhile.".to_owned()),
        Err(error) => Some(format!("Not saved: {error}")),
    }
}
```

`shell/files.rs`, in `save_file`, replace

```rust
                let done = matches!(&written, Ok(written) if written.saved);
                if let Some(view) = shell.files_at(project_id, target) {
                    view.saved(&save.path, save.point, written);
                }
```

with

```rust
                let done = matches!(&written, Ok(written) if written.saved);
                let unshown = match shell.files_at(project_id, target) {
                    Some(view) => view.saved(&save.path, save.point, written),
                    // The workspace's Files view is gone.
                    None => files_view::unshown_save(written),
                };
                if let Some(message) = unshown {
                    shell.toast_error(message, cx);
                }
```

`shell/git.rs`, `stage_git`: replace

```rust
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                let requests = view.staged(result);
```

with

```rust
                let Some(view) = shell.git_at(project_id, &at) else {
                    // The view moved to another repo or worktree: no row left for the failure.
                    if let Err(error) = result {
                        let what = if stage { "Stage" } else { "Unstage" };
                        shell.toast_error(format!("{what} failed: {error}"), cx);
                    }
                    return;
                };
                let requests = view.staged(result);
```

`commit_git`: replace

```rust
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                view.commit.committed(result);
```

with

```rust
                let Some(view) = shell.git_at(project_id, &at) else {
                    if let Err(error) = result {
                        shell.toast_error(format!("Commit failed: {error}"), cx);
                    }
                    return;
                };
                view.commit.committed(result);
```

`run_git_remote`: replace

```rust
                let Some(view) = shell.git_at(project_id, &at) else {
                    return;
                };
                view.remotes.finished(remote, result);
```

with

```rust
                let Some(view) = shell.git_at(project_id, &at) else {
                    if let Err(error) = result {
                        shell.toast_error(format!("{} failed: {error}", remote.label()), cx);
                    }
                    return;
                };
                view.remotes.finished(remote, result);
```

(If a borrow error appears on `shell.toast_error` inside the `let … else`, rewrite that one as `let Some(view) = shell.git_at(..) else { … }` → `match shell.git_at(..) { Some(view) => …, None => … }`.)

`shell/dialogs.rs`, `confirm_undo`: replace

```rust
                Some(error) => {
                    tracing::warn!(?target, "undo failed: {error}");
                    if let Some(dialog) = shell.open_undo(serial) {
                        dialog.undo_failed(&error);
                        shell.load_undo_preview(window, cx);
                    }
                    cx.notify();
                }
```

with

```rust
                Some(error) => {
                    tracing::warn!(?target, "undo failed: {error}");
                    if let Some(dialog) = shell.open_undo(serial) {
                        dialog.undo_failed(&error);
                        shell.load_undo_preview(window, cx);
                    } else {
                        // Esc closed the dialog while it ran: no row left to say why.
                        shell.toast_error(format!("Undo failed: {error}"), cx);
                    }
                    cx.notify();
                }
```

`shell/task_dialog.rs`, `save_task`: replace

```rust
                    if let Some(dialog) = shell.open_dialog(serial) {
                        dialog.busy = false;
                        dialog.error = Some(format!("Cannot save the task: {error:#}"));
                    }
```

with

```rust
                    if let Some(dialog) = shell.open_dialog(serial) {
                        dialog.busy = false;
                        dialog.error = Some(format!("Cannot save the task: {error:#}"));
                    } else {
                        shell.toast_error(format!("Cannot save the task: {error:#}"), cx);
                    }
```

`delete_task`: replace

```rust
                    if let Some(dialog) = shell.open_dialog(serial) {
                        dialog.busy = false;
                        dialog.error = Some(format!("Cannot delete the task: {error}"));
                    }
```

with

```rust
                    if let Some(dialog) = shell.open_dialog(serial) {
                        dialog.busy = false;
                        dialog.error = Some(format!("Cannot delete the task: {error}"));
                    } else {
                        shell.toast_error(format!("Cannot delete the task: {error}"), cx);
                    }
```

`shell/git_dialogs.rs`, `confirm_git_discard`: replace

```rust
                        if let Some(dialog) = open {
                            dialog.busy = false;
                            dialog.error = Some(error);
                        }
```

with

```rust
                        if let Some(dialog) = open {
                            dialog.busy = false;
                            dialog.error = Some(error);
                        } else {
                            shell.toast_error(format!("Discard failed: {error}"), cx);
                        }
```

`set_git_base`: replace

```rust
                        if let Some(dialog) = open {
                            dialog.failed(error);
                        }
```

with

```rust
                        if let Some(dialog) = open {
                            dialog.failed(error);
                        } else {
                            shell.toast_error(format!("Cannot set the base: {error}"), cx);
                        }
```

`shell/run.rs`, `save_run_setup`: replace

```rust
                if shell.dialog_serial != serial || shell.run_setup.is_none() {
                    return;
                }
```

with

```rust
                if shell.dialog_serial != serial || shell.run_setup.is_none() {
                    // Closed while saving: a failure has no dialog left to show in.
                    if let Err(error) = &updated {
                        shell.toast_error(format!("Cannot save the run commands: {error:#}"), cx);
                    }
                    return;
                }
```

`shell/send_dialog.rs`, `send_comments`: replace

```rust
                        if let (true, Some(DocDialog::Send(dialog))) =
                            (open, shell.doc_dialog.as_mut())
                        {
                            dialog.failed(format!("Cannot send: {error:#}"));
                        }
```

with

```rust
                        if let (true, Some(DocDialog::Send(dialog))) =
                            (open, shell.doc_dialog.as_mut())
                        {
                            dialog.failed(format!("Cannot send: {error:#}"));
                        } else {
                            shell.toast_error(format!("Cannot send the comments: {error:#}"), cx);
                        }
```

`shell/worktrees.rs`, `confirm_worktree`: replace

```rust
                let Some(dialog) = shell.open_worktree(serial) else {
                    return;
                };
                let failure = answer.and_then(|value| {
```

with

```rust
                let Some(dialog) = shell.open_worktree(serial) else {
                    // Closed meanwhile: a merge or removal that failed has no dialog to say so.
                    if let Err(error) = &answer {
                        shell.toast_error(format!("The worktree action failed: {error:#}"), cx);
                    }
                    return;
                };
                let failure = answer.and_then(|value| {
```

- [ ] **Step 4: Run tests and compile**

Run: `cargo test -p mkrew-ui files_view -j 4` then `cargo check -p mkrew-ui -j 4`
Expected: PASS (the new test and the existing `a_save_marks_what_it_sent…` / `a_failed_save…` ones; `saved`'s old callers ignore the `Option`). No compile errors. If an `if let … else` shows a borrow error on `shell` inside the `else` (an `Option<&mut _>` scrutinee), bind first: `let open_dialog = shell.open_undo(serial).is_some();` style, or `match`. The `else` branch never uses the `None` value, so NLL accepts the straightforward form; edition 2024 also drops scrutinee temporaries before `else`.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view/tests.rs crates/mkrew-ui/src/shell/files.rs crates/mkrew-ui/src/shell/git.rs crates/mkrew-ui/src/shell/dialogs.rs crates/mkrew-ui/src/shell/task_dialog.rs crates/mkrew-ui/src/shell/git_dialogs.rs crates/mkrew-ui/src/shell/run.rs crates/mkrew-ui/src/shell/send_dialog.rs crates/mkrew-ui/src/shell/worktrees.rs
git -c user.email=tungnv975@gmail.com commit -m "fix(ui): toast results that arrive after their dialog or view is gone"
```

(`shell/run.rs` already has someone else's uncommitted change: `git add -p crates/mkrew-ui/src/shell/run.rs` and take only the `save_run_setup` hunk.)

---

### Task 4: Failures that only logged: image paste, unqueue, Follow in Zed

**Files:**
- Modify: `crates/mkrew-ui/src/composer.rs:9` (import), after `pub fn attachment` (`:247`) (new items), tests module end
- Modify: `crates/mkrew-ui/src/terminal/view.rs:29,42-43,91,685-707`
- Modify: `crates/mkrew-ui/src/main.rs:100`
- Modify: `crates/mkrew-ui/src/shell/focus.rs:172-179`
- Modify: `crates/mkrew-ui/src/shell/composer.rs:74-111,379-397`
- Modify: `crates/mkrew-ui/src/shell/history.rs:42-54`
- Test: `crates/mkrew-ui/src/composer.rs` (`mod tests`)

**Interfaces:**
- Consumes (Task 2): `Shell::toast_error`.
- Produces:
  - `composer::MAX_ATTACHMENT_BYTES: usize` (moved from `terminal/view.rs`, now `pub`)
  - `composer::attachment_refusal(len: usize) -> Option<String>`
  - `composer::unqueue_failure(body: &mkrew_protocol::ErrorBody) -> Option<String>`
  - `terminal::view::AttachFailed(pub String)` (`Clone, Debug, PartialEq, Eq`), `EventEmitter<AttachFailed> for TerminalPane`

- [ ] **Step 1: Write the failing tests**

At the end of `composer.rs`'s `mod tests` (after `queues_follow_session_queue_events`), add:

```rust
    #[test]
    fn an_image_over_the_daemon_limit_is_refused_with_a_reason() {
        assert_eq!(attachment_refusal(0), None);
        assert_eq!(attachment_refusal(MAX_ATTACHMENT_BYTES), None, "the limit itself fits");
        assert_eq!(
            attachment_refusal(MAX_ATTACHMENT_BYTES + 1).as_deref(),
            Some("The image is over 10 MiB, so it was not attached.")
        );
    }

    #[test]
    fn a_text_written_meanwhile_is_not_an_unqueue_failure() {
        use mkrew_protocol::{ErrorBody, ErrorCode};
        let body = |code, message: &str| ErrorBody {
            code,
            message: message.into(),
        };
        // The `session.queue` event already dropped it: nothing to tell the user.
        assert_eq!(
            unqueue_failure(&body(ErrorCode::NotFound, "send 01 is not queued (already written?)")),
            None
        );
        assert_eq!(
            unqueue_failure(&body(ErrorCode::InvalidState, "session exited")).as_deref(),
            Some("Cannot remove the queued text: session exited")
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p mkrew-ui composer -j 4`
Expected: FAIL to compile (`attachment_refusal`, `unqueue_failure`, `MAX_ATTACHMENT_BYTES` not found in `composer`).

- [ ] **Step 3: Implement the helpers**

`composer.rs:9`: change the import to

```rust
use mkrew_protocol::{ErrorBody, ErrorCode, Event, QueuedSend, SessionSendParams};
```

After `pub fn attachment(...) { ... }` (ends `:247`), add:

```rust
/// `attachment.save` refuses larger images (§20B.5); not worth sending.
pub const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// Why a clipboard image of `len` bytes is not sent (a toast); `None` when it fits.
pub fn attachment_refusal(len: usize) -> Option<String> {
    (len > MAX_ATTACHMENT_BYTES).then(|| {
        format!(
            "The image is over {} MiB, so it was not attached.",
            MAX_ATTACHMENT_BYTES / (1024 * 1024)
        )
    })
}

/// The toast for a failed `session.unqueue`. `None` for `not_found`: the text was written
/// meanwhile and the `session.queue` event already dropped it from the list.
pub fn unqueue_failure(body: &ErrorBody) -> Option<String> {
    (body.code != ErrorCode::NotFound)
        .then(|| format!("Cannot remove the queued text: {}", body.message))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui composer -j 4`
Expected: PASS.

- [ ] **Step 5: The terminal pane tells the shell**

`terminal/view.rs:29`: change the composer import to

```rust
use crate::composer::{attachment, attachment_refusal, typed_paths};
```

and delete the local const and its doc (lines 42-43):

```rust
/// `attachment.save` refuses larger images (§20B.5); not worth sending.
const MAX_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;
```

After `impl EventEmitter<OpenLink> for TerminalPane {}` (`:91`) add:

```rust
/// A pasted image could not be attached (too big, or `attachment.save` failed): the shell shows
/// the text as a toast (SPEC §14).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttachFailed(pub String);

impl EventEmitter<AttachFailed> for TerminalPane {}
```

Replace `paste_image` (`:685-707`) with:

```rust
    /// Stores a clipboard image through `attachment.save`, then types its path (§20B.5). A
    /// failure reaches the user as [`AttachFailed`].
    fn paste_image(&mut self, ext: &str, data: Vec<u8>, cx: &mut Context<'_, Self>) {
        let session_id = self.session_id;
        if let Some(message) = attachment_refusal(data.len()) {
            tracing::warn!(%session_id, size = data.len(), "pasted image over the limit, not saved");
            cx.emit(AttachFailed(message));
            return;
        }
        let saved = self
            .connection
            .call::<AttachmentSaveResult>(Request::AttachmentSave(AttachmentSaveParams {
                session_id,
                ext: ext.to_owned(),
                data,
            }));
        cx.spawn(async move |this, cx| match saved.await {
            Ok(saved) => {
                let _ = this.update(cx, |pane, cx| {
                    pane.type_text(&typed_paths([saved.path.as_str()]), cx)
                });
            }
            Err(error) => {
                tracing::warn!(%session_id, "attachment.save failed: {error:#}");
                let _ = this.update(cx, |_, cx| {
                    cx.emit(AttachFailed(format!("Cannot attach the image: {error:#}")));
                });
            }
        })
        .detach();
    }
```

`main.rs:100`: `use mkrew_ui::terminal::view::{AttachFailed, OpenLink, TerminalPane};`

`shell/focus.rs`, in `attach`, right after the existing `cx.subscribe_in(&pane, window, … open_terminal_link …).detach();`:

```rust
                cx.subscribe(&pane, |shell, _, failed: &AttachFailed, cx| {
                    shell.toast_error(failed.0.clone(), cx);
                })
                .detach();
```

- [ ] **Step 6: Composer paste, unqueue, Follow in Zed**

`shell/composer.rs`, `composer_paste_image`: update the doc comment line `/// PNG), then its path goes into the draft. A failure only logs, like a pane's paste.` to `/// PNG), then its path goes into the draft. A failure is a toast, like a pane's paste.`; after the `let Some((ext, data)) = … else { return; };` block insert:

```rust
        if let Some(message) = composer::attachment_refusal(data.len()) {
            self.toast_error(message, cx);
            return;
        }
```

and replace the `Err` arm

```rust
            Err(error) => tracing::warn!(%session, "attachment.save failed: {error:#}"),
```

with

```rust
            Err(error) => {
                tracing::warn!(%session, "attachment.save failed: {error:#}");
                let _ = this.update(cx, |shell, cx| {
                    shell.toast_error(format!("Cannot attach the image: {error:#}"), cx);
                });
            }
```

`shell/composer.rs`, `unqueue`: replace

```rust
        cx.spawn(async move |_, _| match removed.await {
            Ok(Outcome::Result(_)) => {}
            Ok(Outcome::Error(body)) => {
                tracing::warn!(%session, "session.unqueue failed: {}", body.message);
            }
            Err(error) => tracing::warn!(%session, "session.unqueue failed: {error:#}"),
        })
        .detach();
```

with

```rust
        cx.spawn(async move |this, cx| {
            let failure = match removed.await {
                Ok(Outcome::Result(_)) => return,
                Ok(Outcome::Error(body)) => {
                    tracing::warn!(%session, "session.unqueue failed: {}", body.message);
                    composer::unqueue_failure(&body)
                }
                Err(error) => {
                    tracing::warn!(%session, "session.unqueue failed: {error:#}");
                    Some(format!("Cannot remove the queued text: {error:#}"))
                }
            };
            if let Some(message) = failure {
                let _ = this.update(cx, |shell, cx| shell.toast_error(message, cx));
            }
        })
        .detach();
```

`shell/history.rs`, `follow_wait`: replace

```rust
                // The PATH lookup and process spawn stay off the UI thread: this fires on
                // every followed edit, not on a click.
                cx.background_executor()
                    .spawn(async move {
                        if let Err(error) = zed::open(&full, edit.line) {
                            tracing::warn!(
                                %session_id,
                                path = edit.path,
                                "follow in Zed failed: {error}"
                            );
                        }
                    })
                    .detach();
```

with

```rust
                // The PATH lookup and process spawn stay off the UI thread: this fires on
                // every followed edit, not on a click. A failure is a toast; the same one
                // restarts instead of stacking, so a missing Zed is one card, not one per edit.
                let (path, line) = (edit.path, edit.line);
                cx.spawn(async move |this, cx| {
                    let opened = cx
                        .background_executor()
                        .spawn(async move { zed::open(&full, line).map_err(|e| e.to_string()) })
                        .await;
                    if let Err(error) = opened {
                        tracing::warn!(%session_id, path, "follow in Zed failed: {error}");
                        let _ = this.update(cx, |shell, cx| {
                            shell.toast_error(format!("Cannot follow in Zed: {error}"), cx);
                        });
                    }
                })
                .detach();
```

Also update the doc comment of `follow_wait` (`history.rs:~19`, "failure is logged (following stays on)") to "failure is a toast (following stays on)" — read the 3 lines above the fn first and edit only that clause.

- [ ] **Step 7: Compile and test**

Run: `cargo check -p mkrew-ui -j 4` then `cargo test -p mkrew-ui composer -j 4`
Expected: no errors; tests PASS. (`this.update` in a `cx.spawn(async move |this, cx|` returns `Result`, hence `let _ =`; same shape as `paste_image`'s existing `Ok` arm.)

- [ ] **Step 8: Commit**

```bash
git add crates/mkrew-ui/src/composer.rs crates/mkrew-ui/src/terminal/view.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/focus.rs crates/mkrew-ui/src/shell/composer.rs crates/mkrew-ui/src/shell/history.rs
git -c user.email=tungnv975@gmail.com commit -m "fix(ui): toast failed image paste, unqueue and Follow in Zed"
```

---

### Task 5: Failed settings writes toast too

**Files:**
- Modify: `crates/mkrew-ui/src/shell/inspector.rs:19,26-40,124,128`
- Modify: `crates/mkrew-ui/src/shell/sidebar.rs:28-31,780,784`
- Modify: `crates/mkrew-ui/src/shell/workspace.rs:32,359`
- Modify: `crates/mkrew-ui/src/shell/workspace_actions.rs:398-458`

**Interfaces:**
- Consumes (Task 2): `Shell::settings_write_failed(&mut self, what: &str, error: impl Display, cx: &mut Context<'_, Self>)` (sets `settings_error` and toasts `Cannot save {what}: {error}`).
- Produces: `end_inspector_drag(&mut self, cx: &mut Context<'_, Self>)`, `end_sidebar_drag(&mut self, cx: &mut Context<'_, Self>)`, `save_width(&mut self, key: &str, width: f32, cx: &mut Context<'_, Self>)` (new `cx` parameter; every caller below is updated in this task).

There is no pure code here (the write itself is `mkrew_core::settings::write_ui_value`, tested in `mkrew-core`); the check is the compile plus HUMAN_CHECKS.

- [ ] **Step 1: Thread `cx` through the width saves**

`shell/inspector.rs`: replace

```rust
    /// The edge was released: saves the dragged width as `[ui] inspector_width`.
    pub(crate) fn end_inspector_drag(&mut self) {
        if std::mem::take(&mut self.inspector_dragging) {
            self.save_width("inspector_width", self.inspector_width);
        }
    }

    /// Saves a dragged panel width as `[ui] <key>` (one write per drag; the file is hand-edited
    /// config). A file that does not parse is not written; the palette lists why (§20.0).
    pub(crate) fn save_width(&mut self, key: &str, width: f32) {
        // Clamped to its theme limits while dragged, so it fits a u16.
        let width = (width.round() as u16).to_string();
        if let Err(error) = mkrew_core::settings::write_ui_value(&self.settings_file, key, &width) {
            self.settings_error = Some(error.to_string());
        }
    }
```

with

```rust
    /// The edge was released: saves the dragged width as `[ui] inspector_width`.
    pub(crate) fn end_inspector_drag(&mut self, cx: &mut Context<'_, Self>) {
        if std::mem::take(&mut self.inspector_dragging) {
            self.save_width("inspector_width", self.inspector_width, cx);
        }
    }

    /// Saves a dragged panel width as `[ui] <key>` (one write per drag; the file is hand-edited
    /// config). A file that does not parse is not written; the palette lists why (§20.0) and a
    /// toast says the width was not saved.
    pub(crate) fn save_width(&mut self, key: &str, width: f32, cx: &mut Context<'_, Self>) {
        // Clamped to its theme limits while dragged, so it fits a u16.
        let width = (width.round() as u16).to_string();
        if let Err(error) = mkrew_core::settings::write_ui_value(&self.settings_file, key, &width) {
            self.settings_write_failed("the panel width", error, cx);
        }
    }
```

In the same file: `toggle_inspector` (`:19`) `self.end_inspector_drag();` → `self.end_inspector_drag(cx);`, and the two grip listeners (`:124`, `:128`) `cx.listener(|shell, _, _, _| shell.end_inspector_drag())` → `cx.listener(|shell, _, _, cx| shell.end_inspector_drag(cx))`.

`shell/sidebar.rs`: replace

```rust
    pub(crate) fn end_sidebar_drag(&mut self) {
        if std::mem::take(&mut self.sidebar_dragging) {
            self.save_width("sidebar_width", self.sidebar_width);
        }
    }
```

with

```rust
    pub(crate) fn end_sidebar_drag(&mut self, cx: &mut Context<'_, Self>) {
        if std::mem::take(&mut self.sidebar_dragging) {
            self.save_width("sidebar_width", self.sidebar_width, cx);
        }
    }
```

and the two listeners (`:780`, `:784`) `cx.listener(|shell, _, _, _| shell.end_sidebar_drag())` → `cx.listener(|shell, _, _, cx| shell.end_sidebar_drag(cx))`. (`sidebar.rs` has others' uncommitted changes and another plan rewrites the sidebar: re-read, edit by symbol.)

`shell/workspace.rs`: `activate_workspace` (`:32`) and `show_view` (`:359`): `self.end_inspector_drag();` → `self.end_inspector_drag(cx);` (both functions already have `cx`).

- [ ] **Step 2: The three explicit settings writes**

`shell/workspace_actions.rs`, `toggle_theme`: replace

```rust
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "theme", &value)
        {
            self.settings_error = Some(error.to_string());
        }
```

with

```rust
        if let Err(error) =
            mkrew_core::settings::write_ui_value(&self.settings_file, "theme", &value)
        {
            self.settings_write_failed("the theme", error, cx);
        }
```

`set_diff_mode`: in its `if let Err(error) = mkrew_core::settings::write_ui_value(&self.settings_file, "diff_mode", …) { self.settings_error = Some(error.to_string()); }` replace the body with `self.settings_write_failed("the diff layout", error, cx);`.

`set_diff_wrap`: same, body → `self.settings_write_failed("the diff wrapping", error, cx);`.

Update the three doc comments' last clause: "the palette lists why (§20.0)" → "the palette lists why (§20.0) and a toast says it was not saved" (one clause each; read the lines first).

- [ ] **Step 3: Compile**

Run: `cargo check -p mkrew-ui -j 4`
Expected: no errors. If any caller of `end_inspector_drag` / `end_sidebar_drag` / `save_width` was missed the compiler names it (`grep -rn "end_inspector_drag\|end_sidebar_drag\|save_width" crates/mkrew-ui/src` should show only calls with `cx`).

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/shell/sidebar.rs crates/mkrew-ui/src/shell/workspace.rs crates/mkrew-ui/src/shell/workspace_actions.rs
git -c user.email=tungnv975@gmail.com commit -m "fix(ui): toast a failed settings.toml write"
```

(`sidebar.rs` and others: `git add -p`, only your hunks.)

---

### Task 6: Docs, manual checks, full verify

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (new block at the top of "To verify (non-blocking)", after line 16, before `### To verify — 2026-10-15 Ask AI dialog (ADR 0061)`)
- Modify: `docs/PROGRESS.md` (one bullet at the end of "Notes for the next session", `:31+`)
- Modify: `docs/decisions/0053-phone-access-ui.md:55`

- [ ] **Step 1: HUMAN_CHECKS entry**

Insert (new block, blank line before and after):

```markdown
### To verify — 2026-10-19 In-app toasts (SPEC §14)

- [ ] **Info toast (dark + light):** Files tree → right-click a file → Copy path → a card appears
  bottom-right over the content ("Path copied", `info` icon in accent, ✕ at the right, 8 px between
  cards, 16 px from the window edge). It hides by itself after about 4 s (4.0–4.5 s); ✕ closes it at
  once; the ✕ tooltip says "Dismiss". Phone access → Copy link → "Link copied" (never the link).
- [ ] **Error toast:** break `%APPDATA%\mkrew\settings.toml` (delete a `]`), click the title bar theme
  toggle → "Cannot save the theme: …" with the red `circle-alert` icon and the text in the normal
  text color; the theme still changes for this session and the palette lists the settings problem.
  Drag the sidebar edge / the session panel edge → "Cannot save the panel width: …". Fix the file.
- [ ] **Stack:** toggle the theme five times quickly → still one card (the same text restarts its
  time). Then trigger three different failures (theme toggle, diff layout, panel drag) and a fourth
  → at most 3 cards, the oldest is gone.
- [ ] **Never blocks:** with a card showing, type in a terminal and in the composer (keys still go
  there; the card never takes focus), click the pane and the sidebar next to the card (works); only
  the card itself swallows a click. Open Ask AI (Ctrl+Shift+G) with a card showing → the card is
  drawn above the shade, Esc still closes the dialog.
- [ ] **Long text and scale:** a long error (Git → Push to a remote that rejects with a long hook
  message) → the card keeps its width and shows at most 4 lines. Set `[ui] font_size = 18` → the
  card widens with the text; no clipping. Light theme: icon, text and ✕ are readable on the card.
- [ ] **Silent failures now visible:** paste a clipboard image over 10 MiB into a terminal pane and
  into the composer → "The image is over 10 MiB, so it was not attached."; Share → Follow in Zed
  with `zed` removed from PATH, let an agent edit a file → one card "Cannot follow in Zed: …" (not
  one per edit); open Undo task on a card, press Esc at once while it runs and make it fail (e.g. a
  file locked by another program) → "Undo failed: …"; Git view → Push, switch workspace before it
  answers and let it fail → "Push failed: …". A ✕ on a queued text of a session whose text was just
  written shows nothing (not an error).
- [ ] **Kept in place:** Phone access → Start with a read-only `settings.toml` → the error row stays
  inside the dialog (and no toast); a failed commit still shows under the commit box; `Cannot
  start agent` still shows in the tab bar.
```

- [ ] **Step 2: PROGRESS note and ADR 0053 line**

`docs/PROGRESS.md`: read "Notes for the next session" to find its last bullet and append one line after it:

```markdown
- 2026-10-19 toast layer (plan docs/superpowers/plans/2026-10-19-toast-layer.md, no ADR: SPEC §14 wording tightened, no protocol change): pure queue `crates/mkrew-ui/src/toast.rs` (max 3, dedupe, injected `Instant`), card + `Shell::toast_error / toast_info / settings_write_failed` in `shell/toasts.rs`, expired on the 500 ms status tick (so 4.0–4.5 s); Lucide `circle-alert` + `info` added (ADR 0051 procedure). Toasts carry only results with no place of their own (dialog closed, view moved, background paste / unqueue / Follow in Zed, settings writes, copies); in-dialog errors stay (Phone access Start/Stop too). Manual checks in HUMAN_CHECKS "2026-10-19 In-app toasts".
```

`docs/decisions/0053-phone-access-ui.md:55`: replace
`- The UI cannot show a toast (none exists); a failed Start / Stop is an error row in the dialog.` with
`- The dialog stays open, so a failed Start / Stop is an error row in the dialog, not a toast (SPEC §14: a failure that has a place of its own stays there; the toast layer was added later).`

- [ ] **Step 3: Self-check for leftovers**

Run (each must show only the expected hits):

- `grep -rn "there is no toast layer\|no toasts yet" crates/mkrew-ui/src docs/decisions` → no output.
- `grep -rn "TOAST_DURATION" crates/mkrew-ui/src` → `theme.rs` (definition + doc), `toast.rs` (use + tests); no longer unused.
- `grep -rn "0x[0-9A-Fa-f]\{6\}" crates/mkrew-ui/src/shell/toasts.rs crates/mkrew-ui/src/toast.rs` → no output (no hex colors outside `theme.rs`); `grep -n "px([0-9]" crates/mkrew-ui/src/shell/toasts.rs` → no output (only `space::` / `theme::` values).

- [ ] **Step 4: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, `clippy --workspace --all-targets -- -D warnings` and all tests pass (new: 5 queue tests, theme contrast test, `files_view` save test, 2 composer tests; icons test now covers 2 more files). Fix any `cargo fmt` diff with `cargo fmt --all` and re-stage only your files.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/plans/2026-10-19-toast-layer.md docs/PROGRESS.md docs/decisions/0053-phone-access-ui.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: toast layer checks and notes"
```

(`docs/HUMAN_CHECKS.md` has someone else's uncommitted changes at plan time: `git add -p docs/HUMAN_CHECKS.md` and take only the new block. Do not commit `docs/SPEC.md` here: Task 2 already did.)
