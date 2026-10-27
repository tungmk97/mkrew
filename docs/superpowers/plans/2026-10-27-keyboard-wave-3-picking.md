# Keyboard wave 3: picking by keyboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Lines of a doc or diff and text of a terminal can be picked, copied and sent with the keyboard alone: Shift+↓ / Shift+↑ pick lines in Docs and the Git diff, copy mode (Alt+Shift+C) selects terminal text with vi keys, link hints (Alt+Shift+O) open a terminal link by typing its label.

**Architecture:** Line picking keeps the one existing `comments::Selection`; a pure `pick::step` over "units" (source lines, Markdown blocks, new-side diff lines) decides the new range, `Comments` remembers which end moves, and the shell scrolls that end into view. Copy mode turns on `alacritty_terminal`'s vi mode inside `Emulator`; while it is on the emulator holds incoming output back so the view and the cursor stay put, and a pure `copy_mode::copy_key` maps keys to motions. Link hints scan the visible rows with the existing `links::links_in` / `link_at`, give each link a prefix-free label from a pure `hints` module, draw the labels in the terminal element, and open the picked link through the same `OpenLink` event as Ctrl+Click.

**Tech Stack:** Rust, GPUI (`gpui-pre` 0.3.7), `alacritty_terminal` 0.26, existing `mkrew-ui` crate only. No protocol, storage or daemon change.

**Spec:** `docs/superpowers/specs/2026-10-27-keyboard-only-design.md` (section "Wave 3 - Picking by keyboard", plus its Principles and Testing sections).

## Global Constraints

- Every color, spacing, radius, font size and duration comes from `crates/mkrew-ui/src/theme.rs` tokens; no literals in components (`.claude/skills/mkrew-ui-design/SKILL.md`).
- No `unwrap` / `expect` outside tests unless provably infallible (comment why). `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Tests: pure logic gets unit tests (pattern: `crates/mkrew-ui/src/confirm.rs` tests, keymap tests); no test calls a real agent or network. Do not write tests that pin wording, wiring or defaults.
- Fast loop: `cargo test -p mkrew-ui <filter>`; full verify at the end of the wave: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL); on OOM or linker errors add `-j 2`.
- New default keys are unique across all commands (`Keymap::parse` ignores context): `shift-down`, `shift-up`, `alt-shift-c`, `alt-shift-o`. They avoid plain `Ctrl+<letter>` and `Ctrl+Alt+<letter>`.
- Keys that only mean something inside a mode (copy-mode keys, hint letters) are not in `keymap.toml`.
- A view-context key that a text field also uses (Shift+↑/↓) acts only while the view's list / diff has the keyboard; while a text field, a comment draft or the Files editor has it, the key stays with that field.
- Commits: Conventional Commits (`feat(ui): …`), `git -c user.email=tungnv975@gmail.com commit`, stage only the task's files, never push.
- Docs per wave: SPEC.md §12 / §12B / §10 / §13; `docs/decisions/0085-keyboard-only-operation.md` (wave 1 created it; this wave appends a section); `docs/HUMAN_CHECKS.md` "To verify" entries for visual and keyboard checks in dark + light. Never touch `CHANGELOG.md`.
- Markdown: one sentence per physical line; never the em dash character, use "-".
- Waves run in order 1 → 2 → 3 → 4 on master. Waves 1 and 2 edit `main.rs`, `shell/focus.rs`, `shell/widgets.rs` and `keymap.rs` before this wave runs: re-locate every edit point below by its quoted text, never by line number (line numbers are from before wave 1).
- Cross-wave names (keep exact): `Command::PickDown` / `PickUp` / `CopyMode` / `LinkHints` in `Command::all()` right after `SendComments` (the two pick commands) and right after `SendSelection` (the two terminal ones); `Shell::reveal_pick(project_id, context, cx)` is called by wave 4's NextComment / PreviousComment after `pick_lines`.

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/mkrew-ui/src/pick.rs` (new) | pure: `Unit`, `Head`, `step`, `first_row` |
| `crates/mkrew-ui/src/comments.rs` | `Comments.head`, `step_pick`, `head_line`; a click resets the head |
| `crates/mkrew-ui/src/docs.rs` | `Shown::line_units` / `block_units` |
| `crates/mkrew-ui/src/git_view.rs` | `Shown::pick_units` / `hunk_unit` / `line_row`, `GitView::step_pick` |
| `crates/mkrew-ui/src/preview.rs` | `Page::top` / `reveal`; `source` takes a scroll handle |
| `crates/mkrew-ui/src/shell/picking.rs` (new) | `Shell::pick_step`, `reveal_pick`, `docs_key_down` (Esc in Docs) |
| `crates/mkrew-ui/src/terminal/copy_mode.rs` (new) | pure: `CopyKey`, `Select`, `copy_key` |
| `crates/mkrew-ui/src/terminal/hints.rs` (new) | pure: `labels`, `typed` |
| `crates/mkrew-ui/src/terminal/links.rs` | `links_in` |
| `crates/mkrew-ui/src/terminal/emulator.rs` | vi mode API, output hold |
| `crates/mkrew-ui/src/terminal/view.rs` | copy-mode and hint key routing, hint overlay, events |
| `crates/mkrew-ui/src/keymap.rs`, `palette.rs`, `shell/sessions.rs` | four commands |
| `crates/mkrew-ui/src/shell/pane_render.rs`, `focus.rs`, `toasts.rs`, `main.rs`, `shell/connection.rs`, `theme.rs` | "Copy mode" chip, tab bar notice, fields, token |
| `docs/SPEC.md`, `docs/decisions/0085-keyboard-only-operation.md`, `docs/HUMAN_CHECKS.md` | docs |

## Review Focus

1. Shift+↑/↓ typed in a text field (Docs filter, a comment draft, the Git commit box, the sidebar filter) moves the caret or selects text and never picks lines: the shell's `run` already hands every command to a focused field (`cx.propagate()`), and the Git context is only on the file list. Pinned by the smoke step in Task 9 (and the HUMAN_CHECKS line).
2. A pick that outlives its text (the file was edited, the diff reloaded shorter, the file changed) starts over at the start point instead of panicking or picking nothing: `a_stale_pick_starts_over` (Task 1), `a_pick_past_the_end_after_a_reload_starts_over` (Task 2).
3. Nothing to pick (an empty file, a diff still loading, a viewed file folded away, the History): Shift+↓ does nothing and says nothing wrong: `the_first_press_picks_the_start_unit...` (Task 1, empty units), `no_pick_in_the_history_or_a_folded_diff` (Task 2).
4. An agent streaming output while copy mode or link labels are on: the screen must not move under the cursor or the labels, and the held bytes must stay bounded: `copy_output_in_copy_mode_waits...`, `held_output_is_applied_when_the_hold_ends`, `a_flood_is_applied_not_held_without_limit` (Task 4).
5. More links on screen than labels (676) and links next to wide characters: the extra links get no label instead of a duplicate; columns stay right: `labels_never_start_with_each_other` (Task 6), `a_row_with_wide_chars_keeps_columns` (Task 6).

---

### Task 1: Pure pick model (`pick.rs`) and `Comments.head`

**Files:**
- Create: `crates/mkrew-ui/src/pick.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod pick;` after `pub mod phone_dialog;`)
- Modify: `crates/mkrew-ui/src/comments.rs:15-37` (imports, `Selection` neighbours), `:193-227` (`Comments`, `new`), `:285-288` (`pick`), tests module

**Interfaces:**
- Produces (used by Tasks 2 and 3):
  - `pub type Unit = (u32, u32)`: one thing a key press picks, 1-based inclusive source lines.
  - `pub enum Head { Start, End }` (`Default` = `End`): the end of a multi-unit pick that Shift+↑/↓ moves.
  - `pub fn step(units: &[Unit], current: Option<Selection>, head: Head, down: bool, start: usize) -> Option<(Selection, Head)>`: `None` only when `units` is empty.
  - `pub fn first_row(offset_y: f32, row: f32) -> usize`: the first row a list shows when scrolled by `offset_y`.
  - `Comments.head: Head`; `Comments::step_pick(&mut self, units: &[Unit], down: bool, start: usize) -> bool`; `Comments::head_line(&self) -> Option<u32>`.

- [ ] **Step 1: Write the failing tests** - create `crates/mkrew-ui/src/pick.rs` with the module doc, the signatures below stubbed by `todo!()`, and this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn lines(n: u32) -> Vec<Unit> {
        (1..=n).map(|n| (n, n)).collect()
    }

    /// One key press on `pick`; returns the picked `(start, end)`.
    fn press(units: &[Unit], pick: &mut Option<Selection>, head: &mut Head, down: bool) -> (u32, u32) {
        let (selection, moved) = step(units, *pick, *head, down, 0).expect("units");
        *pick = Some(selection);
        *head = moved;
        (selection.start, selection.end)
    }

    #[test]
    fn the_first_press_picks_the_start_unit_whatever_the_direction() {
        let units = lines(5);
        for down in [true, false] {
            assert_eq!(
                step(&units, None, Head::End, down, 2),
                Some((Selection { start: 3, end: 3 }, Head::End))
            );
        }
        // A start point past the end is the last unit.
        assert_eq!(
            step(&units, None, Head::End, true, 99).map(|(s, _)| (s.start, s.end)),
            Some((5, 5))
        );
        // Nothing to pick.
        assert_eq!(step(&[], None, Head::End, true, 0), None);
    }

    #[test]
    fn down_grows_the_end_up_shrinks_it_and_then_grows_the_start() {
        let units = lines(5);
        let (mut pick, mut head) = (Some(Selection { start: 3, end: 3 }), Head::End);
        let mut run = |down| press(&units, &mut pick, &mut head, down);
        assert_eq!(run(true), (3, 4));
        assert_eq!(run(true), (3, 5));
        assert_eq!(run(true), (3, 5), "stops at the last unit");
        assert_eq!(run(false), (3, 4));
        assert_eq!(run(false), (3, 3));
        assert_eq!(run(false), (2, 3), "one unit: up grows the start");
        assert_eq!(run(false), (1, 3));
        assert_eq!(run(false), (1, 3), "stops at the first unit");
        assert_eq!(run(true), (2, 3), "down now shrinks the start");
    }

    #[test]
    fn blocks_are_picked_whole() {
        let units = [(1, 1), (3, 6), (8, 8)];
        let (mut pick, mut head) = (None, Head::End);
        let mut run = |down| press(&units, &mut pick, &mut head, down);
        assert_eq!(run(true), (1, 1));
        assert_eq!(run(true), (1, 6));
        assert_eq!(run(true), (1, 8));
        assert_eq!(run(false), (1, 6));
        // A click inside a block picked part of its lines: the whole block counts.
        let clicked = Some(Selection { start: 4, end: 4 });
        assert_eq!(
            step(&units, clicked, Head::End, true, 0).map(|(s, _)| (s.start, s.end)),
            Some((3, 8))
        );
    }

    #[test]
    fn a_pick_grows_across_the_gap_between_hunks() {
        let units = [(10, 10), (11, 11), (40, 40)];
        let pick = Some(Selection { start: 11, end: 11 });
        assert_eq!(
            step(&units, pick, Head::End, true, 0).map(|(s, _)| (s.start, s.end)),
            Some((11, 40))
        );
    }

    #[test]
    fn a_stale_pick_starts_over() {
        // Lines 7-9 are gone (the file was edited): the start point is picked instead.
        let stale = Some(Selection { start: 7, end: 9 });
        assert_eq!(
            step(&lines(3), stale, Head::Start, true, 1),
            Some((Selection { start: 2, end: 2 }, Head::End))
        );
    }

    #[test]
    fn a_list_scrolled_by_pixels_shows_whole_rows() {
        assert_eq!(first_row(0.0, 20.0), 0);
        assert_eq!(first_row(-45.0, 20.0), 2);
        assert_eq!(first_row(6.0, 20.0), 0, "overscroll");
        assert_eq!(first_row(-10.0, 0.0), 0, "no row height");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-ui pick::`
Expected: FAIL (panics with `not yet implemented`, or compile error until `pub mod pick;` is added to `lib.rs`; add it first).

- [ ] **Step 3: Implement** - the body of `crates/mkrew-ui/src/pick.rs` above the tests:

```rust
//! Picking lines with the keyboard (SPEC §12, §12B): what Shift+↓ / Shift+↑ pick in the shown
//! doc or diff. Pure and tested headlessly; the shell builds the units of what is shown, keeps
//! the result in [`Comments`](crate::comments::Comments) and scrolls it into view.

use crate::comments::Selection;

/// One thing a key press picks: a source line, a Markdown block or a new-side diff line.
/// 1-based, inclusive source lines; the units of one view are sorted and do not overlap.
pub type Unit = (u32, u32);

/// The end of a pick of several units that Shift+↑ / Shift+↓ move (the other one stays).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Head {
    Start,
    #[default]
    End,
}

/// What Shift+↓ (`down`) or Shift+↑ does to the `current` pick over `units`.
/// Nothing picked (or a pick no unit touches any more, after an edit): the unit `start` (clamped
/// to the last one) is picked, whatever the direction.
/// One unit picked: the direction decides which end grows.
/// Several: the `head` end moves one unit, so the pick grows or shrinks; it never goes past the
/// first or last unit and never flips over its other end.
/// `None` only when there are no units.
pub fn step(
    units: &[Unit],
    current: Option<Selection>,
    head: Head,
    down: bool,
    start: usize,
) -> Option<(Selection, Head)> {
    let last = units.len().checked_sub(1)?;
    let covered = current.and_then(|pick| {
        let lo = units.iter().position(|unit| unit.1 >= pick.start)?;
        let hi = units.iter().rposition(|unit| unit.0 <= pick.end)?;
        (lo <= hi).then_some((lo, hi))
    });
    let (lo, hi, head) = match covered {
        None => {
            let at = start.min(last);
            (at, at, Head::End)
        }
        Some((lo, hi)) => {
            let head = match (lo == hi, down) {
                (true, true) => Head::End,
                (true, false) => Head::Start,
                (false, _) => head,
            };
            match (head, down) {
                (Head::End, true) => (lo, (hi + 1).min(last), head),
                (Head::End, false) => (lo, hi.saturating_sub(1).max(lo), head),
                (Head::Start, true) => ((lo + 1).min(hi), hi, head),
                (Head::Start, false) => (lo.saturating_sub(1), hi, head),
            }
        }
    };
    Some((
        Selection {
            start: units[lo].0,
            end: units[hi].1,
        },
        head,
    ))
}

/// The first row a list of `row`-high rows shows when it is scrolled by `offset_y` (gpui: zero
/// at the top, negative once scrolled down).
pub fn first_row(offset_y: f32, row: f32) -> usize {
    if row <= 0.0 || offset_y >= 0.0 {
        return 0;
    }
    (-offset_y / row).floor() as usize
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-ui pick::`
Expected: PASS (6 tests).

- [ ] **Step 5: `Comments` keeps the head** - in `crates/mkrew-ui/src/comments.rs`:

Add the import after `use crate::task_dialog::DialogKey;`:

```rust
use crate::pick::{self, Head, Unit};
```

In `pub struct Comments`, directly after `pub selection: Option<Selection>,` add:

```rust
    /// Which end of `selection` Shift+↑ / Shift+↓ move (a click resets it to the end).
    pub head: Head,
```

In `Comments::new`, after `selection: None,` add `head: Head::End,`.

Replace the body of `pick` (`/// A click on `start..=end`.`) with:

```rust
    /// A click on `start..=end`.
    pub fn pick(&mut self, start: u32, end: u32, extend: bool) {
        self.selection = Some(Selection::pick(self.selection, start, end, extend));
        self.head = Head::End;
    }

    /// Shift+↓ / Shift+↑ over `units` (see [`pick::step`]); `start` is the unit picked when
    /// nothing is. `false` = there are no units.
    pub fn step_pick(&mut self, units: &[Unit], down: bool, start: usize) -> bool {
        let Some((selection, head)) = pick::step(units, self.selection, self.head, down, start)
        else {
            return false;
        };
        self.selection = Some(selection);
        self.head = head;
        true
    }

    /// The source line at the end Shift+↑ / Shift+↓ move: what scrolls into view.
    pub fn head_line(&self) -> Option<u32> {
        let selection = self.selection?;
        Some(match self.head {
            Head::End => selection.end,
            Head::Start => selection.start,
        })
    }
```

- [ ] **Step 6: Test it** - append to the `tests` module of `comments.rs` (after the test that calls `comments.start_quoted(Some("let a = 1;\nlet b = 2;".into()))`):

```rust
    #[test]
    fn keyboard_picks_keep_their_head_and_a_click_resets_it() {
        let mut comments = Comments::default();
        let lines: Vec<Unit> = (1..=4).map(|n| (n, n)).collect();
        assert!(comments.step_pick(&lines, false, 1));
        assert_eq!(comments.selection, Some(Selection { start: 2, end: 2 }));
        // One line picked: Up grows the start, which is now the head.
        assert!(comments.step_pick(&lines, false, 1));
        assert_eq!(comments.selection, Some(Selection { start: 1, end: 2 }));
        assert_eq!(comments.head_line(), Some(1));
        comments.pick(3, 3, false);
        assert_eq!(comments.head_line(), Some(3));
        assert!(!comments.step_pick(&[], true, 0), "no units");
    }
```

Run: `cargo test -p mkrew-ui comments`
Expected: PASS (the new test and every existing comments test).

- [ ] **Step 7: Format, lint, commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/pick.rs crates/mkrew-ui/src/lib.rs crates/mkrew-ui/src/comments.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pure keyboard pick model for doc and diff lines"
```

---

### Task 2: Units of a doc and of a diff, `GitView::step_pick`

**Files:**
- Modify: `crates/mkrew-ui/src/docs.rs:63-96` (`impl Shown`), tests module
- Modify: `crates/mkrew-ui/src/git_view.rs:290-386` (`impl Shown`), `:1713-1725` (`start_comment` neighbour)
- Test: `crates/mkrew-ui/src/git_view/tests.rs`

**Interfaces:**
- Consumes: `pick::Unit`, `Comments::step_pick`, `Comments::head_line` (Task 1).
- Produces (used by Task 3):
  - `docs::Shown::line_units(&self) -> Vec<Unit>`, `docs::Shown::block_units(&self) -> Vec<Unit>`.
  - `git_view::Shown::pick_units(&self) -> Vec<Unit>`, `git_view::Shown::hunk_unit(&self, hunk: usize) -> usize`, `git_view::Shown::line_row(&self, number: u32, mode: DiffMode) -> Option<usize>`.
  - `GitView::step_pick(&mut self, down: bool) -> bool`.

- [ ] **Step 1: Write the failing tests** - in `crates/mkrew-ui/src/git_view/tests.rs` append:

```rust
/// Two hunks: new lines 1-3 (the second added over a removed one) and 10-11.
fn two_hunks() -> Shown {
    laid_out(vec![
        hunk_of(vec![ctx(1, 1), del(2), add(2), ctx(3, 3)], hunk(1, 3, 1, 3)),
        hunk_of(vec![ctx(9, 10), add(11)], hunk(9, 1, 10, 2)),
    ])
}

/// A Working view showing [`two_hunks`] of its first file.
fn picking() -> GitView {
    let mut view = loaded();
    assert!(view.begin_diff());
    view.diff_loaded(&target(Group::Staged, "a.rs"), Ok(two_hunks()));
    view
}

fn picked(view: &GitView) -> Option<(u32, u32)> {
    view.comments.selection.map(|s| (s.start, s.end))
}

#[test]
fn shift_down_picks_the_first_line_of_the_current_hunk_and_grows_across_hunks() {
    let mut view = picking();
    assert!(view.step_pick(true));
    assert_eq!(picked(&view), Some((1, 1)));
    for expected in [(1, 2), (1, 3), (1, 10), (1, 11), (1, 11)] {
        assert!(view.step_pick(true));
        assert_eq!(picked(&view), Some(expected));
    }
    assert!(view.step_pick(false));
    assert_eq!(picked(&view), Some((1, 10)));

    // `n` chooses the hunk a new pick starts in.
    let mut view = picking();
    view.key(Key::NextHunk);
    view.key(Key::NextHunk);
    assert!(view.step_pick(false));
    assert_eq!(picked(&view), Some((10, 10)));
}

#[test]
fn a_pick_past_the_end_after_a_reload_starts_over() {
    let mut view = picking();
    view.comments.pick(10, 11, false);
    view.begin_diff();
    view.diff_loaded(&target(Group::Staged, "a.rs"), Ok(laid_out(vec![hunk(1, 1, 1, 1)])));
    assert!(view.step_pick(true));
    assert_eq!(picked(&view), Some((1, 1)));
}

#[test]
fn no_pick_in_the_history_or_a_folded_diff() {
    let mut view = loaded();
    assert!(!view.step_pick(true), "diff still loading");
    let mut view = picking();
    view.toggle_history();
    assert!(!view.step_pick(true));
    assert_eq!(picked(&view), None);
}

#[test]
fn a_new_side_line_has_a_row_in_both_layouts() {
    let shown = two_hunks();
    assert_eq!(shown.pick_units(), [(1, 1), (2, 2), (3, 3), (10, 10), (11, 11)]);
    assert_eq!(shown.hunk_unit(0), 0);
    assert_eq!(shown.hunk_unit(1), 3);
    // Unified: hunk 0, ctx 1, del, add 2, ctx 3, gap, hunk 1, ctx 10, add 11.
    assert_eq!(shown.line_row(2, DiffMode::Unified), Some(3));
    assert_eq!(shown.line_row(11, DiffMode::Unified), Some(8));
    // Split pairs the removed and the added line in one row.
    assert_eq!(shown.line_row(2, DiffMode::Split), Some(2));
    assert_eq!(shown.line_row(3, DiffMode::Split), Some(3));
    assert_eq!(shown.line_row(11, DiffMode::Split), Some(7));
    assert_eq!(shown.line_row(5, DiffMode::Unified), None);
}
```

Append to the `tests` module of `crates/mkrew-ui/src/docs.rs`:

```rust
    #[test]
    fn picks_follow_blocks_in_the_preview_and_lines_in_the_source() {
        let shown = Shown::from_text("# Title\n\n```\ncode\n```\n\nafter\n");
        assert_eq!(shown.block_units(), [(1, 1), (3, 5), (7, 7)]);
        assert_eq!(shown.line_units().len(), 7);
        assert_eq!(shown.line_units()[3], (4, 4));
        assert!(Shown::from_text("").block_units().is_empty());
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui -- shift_down_picks a_pick_past a_new_side no_pick_in picks_follow`
Expected: FAIL (compile error: `step_pick`, `pick_units`, `line_units` not defined).

- [ ] **Step 3: Docs units** - in `crates/mkrew-ui/src/docs.rs` add `use crate::pick::Unit;` with the other `use crate::…` lines, and inside `impl Shown` after `from_text`:

```rust
    /// What Shift+↓ / Shift+↑ pick in the source: every line (line `n` is `lines[n - 1]`).
    pub fn line_units(&self) -> Vec<Unit> {
        let count = u32::try_from(self.lines.len()).unwrap_or(u32::MAX);
        (1..=count).map(|n| (n, n)).collect()
    }

    /// What Shift+↓ / Shift+↑ pick in the preview: the source lines of every top-level block.
    pub fn block_units(&self) -> Vec<Unit> {
        (self.blocks.iter())
            .map(|block| (block.lines.start, block.lines.end))
            .collect()
    }
```

- [ ] **Step 4: Diff units** - in `crates/mkrew-ui/src/git_view.rs` add `use crate::pick::Unit;` with the other `use crate::…` lines and, inside `impl Shown` right after `hunk_row`:

```rust
    /// What Shift+↓ / Shift+↑ pick in the diff: the new-side lines it shows (added and
    /// unchanged ones), one unit each, in order; comments anchor on these.
    pub fn pick_units(&self) -> Vec<Unit> {
        (self.diff.hunks.iter())
            .flat_map(|hunk| &hunk.lines)
            .filter_map(|line| line.new_line)
            .map(|number| (number, number))
            .collect()
    }

    /// The index in [`Self::pick_units`] of the first new-side line of hunk `hunk` (of the next
    /// hunk that has one, when `hunk` only removes lines).
    pub fn hunk_unit(&self, hunk: usize) -> usize {
        (self.diff.hunks.iter().take(hunk))
            .flat_map(|hunk| &hunk.lines)
            .filter(|line| line.new_line.is_some())
            .count()
    }

    /// The list row that draws new-side line `number` in `mode` (what a pick scrolls to).
    pub fn line_row(&self, number: u32, mode: DiffMode) -> Option<usize> {
        let at = self
            .rows
            .iter()
            .position(|row| matches!(row, DiffRow::Line(line) if line.new_line == Some(number)))?;
        match mode {
            DiffMode::Unified => Some(at),
            DiffMode::Split => self.split.iter().position(|row| match *row {
                SplitRow::Same(index) => index == at,
                SplitRow::Change { new, .. } => new == Some(at),
                SplitRow::Span(_) => false,
            }),
        }
    }
```

In `impl GitView`, directly after `start_comment`:

```rust
    /// Shift+↓ / Shift+↑ (SPEC §12B): picks the first new-side line of the current hunk (`n` /
    /// `p` choose it; the first hunk without one) when nothing is picked, else grows or shrinks
    /// the pick one line (see [`crate::pick::step`]). `false` = nothing to pick: no diff shown,
    /// the diff folded away, or the History.
    pub fn step_pick(&mut self, down: bool) -> bool {
        if !self.can_comment() || self.diff_collapsed() {
            return false;
        }
        let Diff::Shown(shown) = &self.diff else {
            return false;
        };
        let units = shown.pick_units();
        let start = shown.hunk_unit(self.hunk.unwrap_or(0));
        self.comments.step_pick(&units, down, start)
    }
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p mkrew-ui -- shift_down_picks a_pick_past a_new_side no_pick_in picks_follow`
Expected: PASS (5 tests). Then `cargo test -p mkrew-ui git_view docs` PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/docs.rs crates/mkrew-ui/src/git_view.rs crates/mkrew-ui/src/git_view/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pick units of a doc and of a diff"
```

---

### Task 3: `PickDown` / `PickUp` commands, scrolling and Esc in Docs

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum near `SendComments` ~87, `all()` ~252, `name()` ~342, `default_keys()` ~444, `context()` ~506)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title` ~243, `available` ~353) and `crates/mkrew-ui/src/palette/tests.rs` (~200, ~229, ~263)
- Modify: `crates/mkrew-ui/src/preview.rs:90-110` (`impl Page`), `:696-749` (`source`)
- Create: `crates/mkrew-ui/src/shell/picking.rs`; Modify: `crates/mkrew-ui/src/shell.rs` (`mod picking;` after `mod comments;`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (`run`, arm after `Command::SendComments`)
- Modify: `crates/mkrew-ui/src/shell/docs.rs:469` (`preview::source(` call)
- Modify: `crates/mkrew-ui/src/shell/git_diff.rs:606-611` (`git_diff_lines`)
- Modify: `crates/mkrew-ui/src/shell/comments.rs:478` (margin hint)
- Modify: `crates/mkrew-ui/src/main.rs` (struct `Shell` fields near `doc_page` and `git_diff_scroll_to`; the root `.on_key_down(cx.listener(|shell, event: &KeyDownEvent, window, cx| {` closure), `crates/mkrew-ui/src/shell/connection.rs` (field init near `doc_page: preview::Page::default(),` and `git_diff_scroll_to: Cell::new(None),`)

**Interfaces:**
- Consumes: `Comments::step_pick`, `Comments::head_line`, `GitView::step_pick`, `docs::Shown::{line_units, block_units}`, `git_view::Shown::line_row`, `pick::first_row`.
- Produces:
  - `Command::PickDown` (`pick_down`, `shift-down`) and `Command::PickUp` (`pick_up`, `shift-up`), key context `COMMENTS_CONTEXT` (`"Docs || Git"`).
  - `Shell::pick_step(&mut self, down: bool, cx: &mut Context<'_, Self>)`.
  - `Shell::reveal_pick(&mut self, project_id: Ulid, context: CommentContext, cx: &mut Context<'_, Self>)`: scrolls the moved end of the shown pick into view; safe right after `pick_lines`.
  - `Shell::docs_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<'_, Self>)`.
  - `preview::Page::top(&self) -> usize`, `preview::Page::reveal(&self, ix: usize)`; `preview::source(id, shown, selected, on_line, scroll: &UniformListScrollHandle, theme)`.

- [ ] **Step 1: Commands** - in `crates/mkrew-ui/src/keymap.rs`:

Enum, after `SendComments,`:

```rust
    /// Docs / Git: pick the first line (block) in view, or grow the pick one line (block) down.
    PickDown,
    /// Docs / Git: the same upwards; shrinks a pick that was grown down.
    PickUp,
```

`Command::all()`, after `Command::SendComments,` add `Command::PickDown,` and `Command::PickUp,`.

`name()`, after `Command::SendComments => "send_comments".into(),`:

```rust
            Command::PickDown => "pick_down".into(),
            Command::PickUp => "pick_up".into(),
```

`default_keys()`, after `Command::SendComments => "ctrl-shift-s".into(),`:

```rust
            Command::PickDown => "shift-down".into(),
            Command::PickUp => "shift-up".into(),
```

`context()`: replace `Command::AddComment | Command::SendComments => Some(COMMENTS_CONTEXT),` with

```rust
            Command::AddComment
            | Command::SendComments
            | Command::PickDown
            | Command::PickUp => Some(COMMENTS_CONTEXT),
```

No keymap test is added: `every_default_parses_and_is_unique`, `names_round_trip` and the template tests already cover new commands.

- [ ] **Step 2: Palette** - in `crates/mkrew-ui/src/palette.rs` `title`, after the `Command::SendComments` arm:

```rust
        Command::PickDown => "Comments: Pick the next line or block (grow the pick down)".into(),
        Command::PickUp => "Comments: Pick the previous line or block (grow the pick up)".into(),
```

In `available`, replace `Command::AddComment | Command::SendComments => in_view(View::Docs) || in_view(View::Git),` with

```rust
        Command::AddComment | Command::SendComments | Command::PickDown | Command::PickUp => {
            in_view(View::Docs) || in_view(View::Git)
        }
```

In `crates/mkrew-ui/src/palette/tests.rs` `availability_follows_the_view`: add `Command::PickDown,` after `Command::AddComment,` in the list asserted *not* available in Terminals; after `assert!(git.contains(&Command::AddComment));` add `assert!(git.contains(&Command::PickUp));`; after `assert!(docs.contains(&Command::SendComments));` add `assert!(docs.contains(&Command::PickDown));`.

- [ ] **Step 3: Preview helpers** - in `crates/mkrew-ui/src/preview.rs` add `UniformListScrollHandle` to the `use gpui::{ … }` list. In `impl Page` after `show`:

```rust
    /// The index of the block at the top of the view.
    pub fn top(&self) -> usize {
        self.state.logical_scroll_top().item_ix
    }

    /// Scrolls block `ix` into view (nothing moves when it is shown already).
    pub fn reveal(&self, ix: usize) {
        self.state.scroll_to_reveal_item(ix);
    }
```

Change `source` to track a scroll handle: signature

```rust
pub fn source(
    id: &'static str,
    shown: Rc<Shown>,
    selected: Option<Selection>,
    on_line: OnLine,
    scroll: &UniformListScrollHandle,
    theme: &Theme,
) -> UniformList {
```

and in its body turn `uniform_list(id, total, move |range, _, _| { … })` into `uniform_list(id, total, move |range, _, _| { … }).track_scroll(scroll)` (keep the existing `.font_family(...)` and `.text_size(...)` after it). Update the doc comment: "…a click picks a line; `scroll` follows the list so the shell can read and set its position."

- [ ] **Step 4: Fields** - in `crates/mkrew-ui/src/main.rs` `struct Shell`, after `doc_page: preview::Page,`:

```rust
    /// Scroll of the Docs source view: where Shift+↓ starts, and what keeps a pick in view.
    doc_source_scroll: UniformListScrollHandle,
```

and after `git_diff_scroll_to: Cell<Option<usize>>,`:

```rust
    /// A diff row to scroll into view once the list is laid out (a keyboard pick moved).
    git_diff_reveal: Cell<Option<usize>>,
```

In `crates/mkrew-ui/src/shell/connection.rs` initialisers: after `doc_page: preview::Page::default(),` add `doc_source_scroll: UniformListScrollHandle::new(),`; after `git_diff_scroll_to: Cell::new(None),` add `git_diff_reveal: Cell::new(None),`.

- [ ] **Step 5: Shell methods** - create `crates/mkrew-ui/src/shell/picking.rs`:

```rust
//! Picking lines with the keyboard (SPEC §12, §12B): Shift+↓ / Shift+↑ in Docs and the Git
//! diff, and Esc to drop the pick in Docs.

use crate::*;

impl Shell {
    /// Shift+↓ / Shift+↑ (`PickDown` / `PickUp`): picks the first line or block in view (Git:
    /// the first line of the current hunk), or grows / shrinks the pick one line or block, then
    /// scrolls the moved end into view. A text field never gets here: `run` hands the key to it.
    pub(crate) fn pick_step(&mut self, down: bool, cx: &mut Context<'_, Self>) {
        let (Some(project_id), Some(context)) = (self.workspace, self.comment_context()) else {
            return;
        };
        let moved = match context {
            CommentContext::Doc => {
                let theme = cx.theme();
                let row = theme.terminal_font_size * theme.code_line_height;
                let offset = f32::from(self.doc_source_scroll.0.borrow().base_handle.offset().y);
                let Some(docs) = self.docs.get_mut(&project_id) else {
                    return;
                };
                let Doc::Shown(shown) = &docs.doc else {
                    return;
                };
                let (units, start) = match docs.mode {
                    Mode::Source => (shown.line_units(), mkrew_ui::pick::first_row(offset, row)),
                    Mode::Preview => (shown.block_units(), self.doc_page.top()),
                };
                docs.comments.step_pick(&units, down, start)
            }
            CommentContext::Diff => self
                .git
                .get_mut(&project_id)
                .is_some_and(|view| view.step_pick(down)),
        };
        if moved {
            self.reveal_pick(project_id, context, cx);
        }
    }

    /// Scrolls the moved end of the shown pick (`Comments::head_line`) into view: a Docs source
    /// line, a Docs preview block, or a Git diff row (scrolled once the list is laid out, after
    /// the pick bar changed its heights). Safe to call right after `pick_lines`.
    pub(crate) fn reveal_pick(
        &mut self,
        project_id: Ulid,
        context: CommentContext,
        cx: &mut Context<'_, Self>,
    ) {
        match context {
            CommentContext::Doc => {
                if let Some(docs) = self.docs.get(&project_id)
                    && let Some(line) = docs.comments.head_line()
                    && let Doc::Shown(shown) = &docs.doc
                {
                    match docs.mode {
                        Mode::Source => self.doc_source_scroll.scroll_to_item(
                            (line as usize).saturating_sub(1),
                            ScrollStrategy::Nearest,
                        ),
                        Mode::Preview => {
                            let block = shown
                                .blocks
                                .iter()
                                .position(|b| b.lines.start <= line && line <= b.lines.end);
                            if let Some(block) = block {
                                self.doc_page.reveal(block);
                            }
                        }
                    }
                }
            }
            CommentContext::Diff => {
                let row = self.git.get(&project_id).and_then(|view| match &view.diff {
                    git_view::Diff::Shown(shown) => {
                        shown.line_row(view.comments.head_line()?, self.diff_mode)
                    }
                    _ => None,
                });
                if row.is_some() {
                    self.git_diff_reveal.set(row);
                }
            }
        }
        cx.notify();
    }

    /// Esc in Docs drops the picked lines, while nothing else has the keyboard (the filter and
    /// the comment draft handle their own Esc and stop it first). Git does the same in
    /// `git_key_down`.
    pub(crate) fn docs_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let plain_escape =
            event.keystroke.key == "escape" && !event.keystroke.modifiers.modified();
        if !plain_escape
            || self.view() != View::Docs
            || self.modal_open()
            || !self.focus_handle.is_focused(window)
        {
            return;
        }
        let picked = (self.workspace)
            .and_then(|project_id| self.docs.get_mut(&project_id))
            .map(|docs| &mut docs.comments)
            .filter(|comments| comments.selection.is_some());
        if let Some(comments) = picked {
            comments.selection = None;
            cx.stop_propagation();
            cx.notify();
        }
    }
}
```

In `crates/mkrew-ui/src/shell.rs` add `mod picking;` after `mod comments;`.

- [ ] **Step 6: Wire the key, the list and the Esc** -

`shell/sessions.rs` `run`, after the `Command::SendComments => { … }` arm:

```rust
            Command::PickDown | Command::PickUp => {
                // Only bound while Docs are shown or the Git list has focus; a focused text
                // field, comment draft or dialog never gets here (the guards above).
                self.pick_step(*command == Command::PickDown, cx);
            }
```

`shell/docs.rs`, in the `Mode::Source` arm, change the call to

```rust
                    preview::source(
                        "doc-source",
                        Rc::clone(shown),
                        docs.comments.selection,
                        on_line,
                        &self.doc_source_scroll,
                        theme,
                    )
```

`shell/git_diff.rs` `git_diff_lines`, directly after the `if let Some(row) = self.git_diff_scroll_to.take() { … }` block:

```rust
        if let Some(row) = self.git_diff_reveal.take() {
            self.git_diff_list.scroll_to_reveal_item(row);
        }
```

`main.rs`, first line inside the root `.on_key_down(cx.listener(|shell, event: &KeyDownEvent, window, cx| {` closure (before `let filter_shown`):

```rust
                shell.docs_key_down(event, window, cx);
```

`shell/comments.rs`, in the empty-margin hint, replace
`None => format!("Select lines, then comment ({keys})."),` with

```rust
                None => format!(
                    "Select lines (click or {}), then comment ({keys}).",
                    self.keymap.label(Command::PickDown)
                ),
```

- [ ] **Step 7: Build and run the tests**

Run: `cargo build -p mkrew-ui` then `cargo test -p mkrew-ui palette keymap comments git_view pick`
Expected: build OK (the match in `run` is exhaustive again); tests PASS, including `every_default_parses_and_is_unique`, `every_command_has_a_distinct_title` and the palette availability rows.

- [ ] **Step 8: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pick doc and diff lines with Shift+Down / Shift+Up"
```

---

### Task 4: Emulator copy mode (vi mode) and output hold

**Files:**
- Create: `crates/mkrew-ui/src/terminal/copy_mode.rs`
- Modify: `crates/mkrew-ui/src/terminal.rs` (`pub mod copy_mode;` before `pub mod emulator;`)
- Modify: `crates/mkrew-ui/src/terminal/emulator.rs:6-25` (imports), `:116-128` (`Emulator`), `:130-147` (`new`), `:170-174` (`advance`), `:209-237` (`search_from_view`), tests module

**Interfaces:**
- Produces (used by Tasks 5 and 7):
  - `copy_mode::Select { Chars, Lines }`; `copy_mode::CopyKey { Motion(ViMotion), Page { up: bool }, Edge { top: bool }, Select(Select), Copy, Cancel }`; `copy_mode::copy_key(keystroke: &gpui::Keystroke) -> Option<CopyKey>`.
  - `emulator::CopyEnd { Stay, Leave(Option<String>) }`.
  - `Emulator::copy_mode(&self) -> bool`, `enter_copy_mode(&mut self)`, `leave_copy_mode(&mut self)`, `copy_key(&mut self, key: CopyKey) -> CopyEnd`, `set_hold(&mut self, hold: bool)`.
  - `Emulator::advance` holds bytes while copy mode or `set_hold(true)` is on (up to `HOLD_LIMIT`), and `search_next` moves the vi cursor while copy mode is on.

- [ ] **Step 1: Write the failing tests for the key map** - create `crates/mkrew-ui/src/terminal/copy_mode.rs` with the module doc, the types and `copy_key` stubbed by `todo!()`, and:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    /// A parsed key press; a printable key carries its char, like a real one.
    fn key(spec: &str) -> Option<CopyKey> {
        let mut keystroke = Keystroke::parse(spec).unwrap();
        let m = keystroke.modifiers;
        if keystroke.key.chars().count() == 1 && !m.control && !m.alt {
            keystroke.key_char = Some(if m.shift {
                keystroke.key.to_uppercase()
            } else {
                keystroke.key.clone()
            });
        }
        copy_key(&keystroke)
    }

    fn motion(motion: ViMotion) -> Option<CopyKey> {
        Some(CopyKey::Motion(motion))
    }

    #[test]
    fn arrows_and_hjkl_move_the_cursor() {
        for (spec, expected) in [
            ("left", ViMotion::Left),
            ("h", ViMotion::Left),
            ("down", ViMotion::Down),
            ("j", ViMotion::Down),
            ("up", ViMotion::Up),
            ("k", ViMotion::Up),
            ("right", ViMotion::Right),
            ("l", ViMotion::Right),
        ] {
            assert_eq!(key(spec), motion(expected), "{spec}");
        }
    }

    #[test]
    fn words_lines_and_edges() {
        assert_eq!(key("w"), motion(ViMotion::SemanticRight));
        assert_eq!(key("b"), motion(ViMotion::SemanticLeft));
        assert_eq!(key("e"), motion(ViMotion::SemanticRightEnd));
        assert_eq!(key("0"), motion(ViMotion::First));
        // `$` is Shift+4 on a US layout: the char decides, not the key.
        let dollar = Keystroke {
            modifiers: Modifiers {
                shift: true,
                ..Modifiers::default()
            },
            key: "4".into(),
            key_char: Some("$".into()),
        };
        assert_eq!(copy_key(&dollar), motion(ViMotion::Last));
        assert_eq!(key("g"), Some(CopyKey::Edge { top: true }));
        assert_eq!(key("shift-g"), Some(CopyKey::Edge { top: false }));
        assert_eq!(key("pageup"), Some(CopyKey::Page { up: true }));
        assert_eq!(key("pagedown"), Some(CopyKey::Page { up: false }));
    }

    #[test]
    fn select_copy_and_leave() {
        assert_eq!(key("v"), Some(CopyKey::Select(Select::Chars)));
        assert_eq!(key("shift-v"), Some(CopyKey::Select(Select::Lines)));
        assert_eq!(key("y"), Some(CopyKey::Copy));
        assert_eq!(key("enter"), Some(CopyKey::Copy));
        assert_eq!(key("escape"), Some(CopyKey::Cancel));
    }

    #[test]
    fn every_other_key_does_nothing_and_never_reaches_the_program() {
        for spec in ["x", "space", "tab", "f1", "ctrl-h", "alt-j", "ctrl-shift-v", "delete"] {
            assert_eq!(key(spec), None, "{spec}");
        }
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui copy_mode::` (after adding `pub mod copy_mode;` to `terminal.rs`)
Expected: FAIL (`not yet implemented`).

- [ ] **Step 3: Implement the key map** - body of `crates/mkrew-ui/src/terminal/copy_mode.rs` above the tests:

```rust
//! Copy mode of a terminal pane (SPEC §10): which keys move the vi cursor and select text.
//! Pure and tested headlessly; [`Emulator::copy_key`](crate::terminal::emulator::Emulator::copy_key)
//! applies them to alacritty's vi mode. These keys are not in `keymap.toml`, like the peek's.

use alacritty_terminal::vi_mode::ViMotion;
use gpui::Keystroke;

/// What `v` / Shift+V start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Select {
    Chars,
    Lines,
}

/// What a key press does while copy mode is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyKey {
    Motion(ViMotion),
    /// PgUp (`up`) / PgDn: one screen, the cursor keeps its screen row.
    Page { up: bool },
    /// `g` (the top of the history) / `G` (the bottom).
    Edge { top: bool },
    /// `v` / Shift+V: start a selection, switch its kind, or drop it (same kind again).
    Select(Select),
    /// `y` / Enter: copy the selection and leave copy mode.
    Copy,
    /// Esc: drop the selection, or leave copy mode when there is none.
    Cancel,
}

/// The copy-mode meaning of `keystroke`; `None` = ignored (it never reaches the program).
pub fn copy_key(keystroke: &Keystroke) -> Option<CopyKey> {
    let m = keystroke.modifiers;
    if m.control || m.alt || m.platform {
        return None;
    }
    match keystroke.key.as_str() {
        "left" => return Some(CopyKey::Motion(ViMotion::Left)),
        "right" => return Some(CopyKey::Motion(ViMotion::Right)),
        "up" => return Some(CopyKey::Motion(ViMotion::Up)),
        "down" => return Some(CopyKey::Motion(ViMotion::Down)),
        "pageup" => return Some(CopyKey::Page { up: true }),
        "pagedown" => return Some(CopyKey::Page { up: false }),
        "enter" => return Some(CopyKey::Copy),
        "escape" => return Some(CopyKey::Cancel),
        _ => {}
    }
    let typed = match keystroke.key_char.as_deref() {
        Some(text) => text.to_owned(),
        None if m.shift => keystroke.key.to_uppercase(),
        None => keystroke.key.clone(),
    };
    Some(match typed.as_str() {
        "h" => CopyKey::Motion(ViMotion::Left),
        "l" => CopyKey::Motion(ViMotion::Right),
        "k" => CopyKey::Motion(ViMotion::Up),
        "j" => CopyKey::Motion(ViMotion::Down),
        "w" => CopyKey::Motion(ViMotion::SemanticRight),
        "b" => CopyKey::Motion(ViMotion::SemanticLeft),
        "e" => CopyKey::Motion(ViMotion::SemanticRightEnd),
        "0" => CopyKey::Motion(ViMotion::First),
        "$" => CopyKey::Motion(ViMotion::Last),
        "g" => CopyKey::Edge { top: true },
        "G" => CopyKey::Edge { top: false },
        "v" => CopyKey::Select(Select::Chars),
        "V" => CopyKey::Select(Select::Lines),
        "y" => CopyKey::Copy,
        _ => return None,
    })
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p mkrew-ui copy_mode::`
Expected: PASS (4 tests).

- [ ] **Step 5: Write the failing emulator tests** - append to the `tests` module of `crates/mkrew-ui/src/terminal/emulator.rs` (it already has `numbered_lines`):

```rust
    fn copy_keys(emu: &mut Emulator, keys: impl IntoIterator<Item = CopyKey>) -> CopyEnd {
        let mut end = CopyEnd::Stay;
        for key in keys {
            end = emu.copy_key(key);
        }
        end
    }

    #[test]
    fn copy_vi_motions_select_and_yank() {
        let mut emu = Emulator::new(12, 3);
        emu.advance(b"hello abc\r\nsecond");
        emu.enter_copy_mode();
        assert!(emu.copy_mode());
        copy_keys(
            &mut emu,
            [
                CopyKey::Motion(ViMotion::First),
                CopyKey::Motion(ViMotion::Up),
                CopyKey::Select(Select::Chars),
                CopyKey::Motion(ViMotion::Right),
                CopyKey::Motion(ViMotion::Right),
                CopyKey::Motion(ViMotion::Right),
                CopyKey::Motion(ViMotion::Right),
            ],
        );
        let grid = emu.grid();
        assert!(grid.rows[0][0].selected && grid.rows[0][4].selected && !grid.rows[0][5].selected);
        assert_eq!((grid.cursor.row, grid.cursor.col), (0, 4));
        let end = copy_keys(&mut emu, [CopyKey::Copy]);
        assert_eq!(end, CopyEnd::Leave(Some("hello".into())));
        assert!(!emu.copy_mode());
    }

    #[test]
    fn copy_line_selection_takes_whole_lines() {
        let mut emu = Emulator::new(12, 3);
        emu.advance(b"hello abc\r\nsecond");
        emu.enter_copy_mode();
        let end = copy_keys(
            &mut emu,
            [
                CopyKey::Motion(ViMotion::First),
                CopyKey::Motion(ViMotion::Up),
                CopyKey::Select(Select::Lines),
                CopyKey::Motion(ViMotion::Down),
                CopyKey::Copy,
            ],
        );
        let CopyEnd::Leave(Some(text)) = end else {
            panic!("nothing copied: {end:?}");
        };
        assert!(text.starts_with("hello abc\nsecond"), "{text:?}");
    }

    #[test]
    fn copy_v_starts_switches_and_drops_the_selection() {
        let mut emu = Emulator::new(12, 3);
        emu.advance(b"hello abc\r\nsecond");
        emu.enter_copy_mode();
        copy_keys(&mut emu, [CopyKey::Motion(ViMotion::First)]);
        copy_keys(&mut emu, [CopyKey::Select(Select::Chars)]);
        assert_eq!(emu.selection_text().as_deref(), Some("s"));
        copy_keys(&mut emu, [CopyKey::Select(Select::Lines)]);
        assert_eq!(emu.selection_text().as_deref(), Some("second"));
        copy_keys(&mut emu, [CopyKey::Select(Select::Lines)]);
        assert_eq!(emu.selection_text(), None, "the same kind again drops it");
    }

    #[test]
    fn copy_esc_drops_the_selection_then_leaves() {
        let mut emu = Emulator::new(12, 3);
        emu.advance(b"hello abc\r\nsecond");
        emu.enter_copy_mode();
        copy_keys(
            &mut emu,
            [
                CopyKey::Motion(ViMotion::First),
                CopyKey::Select(Select::Chars),
                CopyKey::Motion(ViMotion::Right),
            ],
        );
        assert!(emu.selection_text().is_some());
        assert_eq!(copy_keys(&mut emu, [CopyKey::Cancel]), CopyEnd::Stay);
        assert_eq!(emu.selection_text(), None);
        assert!(emu.copy_mode());
        assert_eq!(copy_keys(&mut emu, [CopyKey::Cancel]), CopyEnd::Leave(None));
        assert!(!emu.copy_mode());
    }

    #[test]
    fn copy_output_in_copy_mode_waits_for_the_view_to_come_back() {
        let mut emu = Emulator::new(10, 3);
        numbered_lines(&mut emu, 10);
        emu.enter_copy_mode();
        copy_keys(&mut emu, [CopyKey::Edge { top: true }]);
        let (offset, text) = (emu.display_offset(), emu.grid().to_text());
        assert!(offset > 0);
        emu.advance(b"fresh\r\nlast\r\n");
        assert_eq!(emu.display_offset(), offset, "the view did not move");
        assert_eq!(emu.grid().to_text(), text);
        assert_eq!(copy_keys(&mut emu, [CopyKey::Cancel]), CopyEnd::Leave(None));
        assert_eq!(emu.display_offset(), 0, "back on the live screen");
        assert!(emu.grid().to_text().contains("last"), "held output applied");
    }

    #[test]
    fn copy_output_at_the_live_screen_does_not_scroll_it_either() {
        let mut emu = Emulator::new(10, 3);
        emu.advance(b"a\r\nb");
        emu.enter_copy_mode();
        let before = emu.grid().to_text();
        emu.advance(b"\r\nc\r\nd\r\ne");
        assert_eq!(emu.grid().to_text(), before);
    }

    #[test]
    fn copy_page_keeps_the_cursor_row_of_the_screen() {
        let mut emu = Emulator::new(10, 3);
        numbered_lines(&mut emu, 12);
        emu.enter_copy_mode();
        copy_keys(&mut emu, [CopyKey::Edge { top: false }]);
        let row = emu.grid().cursor.row;
        copy_keys(&mut emu, [CopyKey::Page { up: true }]);
        assert!(emu.display_offset() > 0);
        assert_eq!(emu.grid().cursor.row, row);
    }

    #[test]
    fn copy_find_match_moves_the_cursor() {
        let mut emu = Emulator::new(10, 3);
        numbered_lines(&mut emu, 10);
        emu.enter_copy_mode();
        assert!(emu.set_search("line5"));
        assert!(emu.search_next(true));
        let grid = emu.grid();
        let row = grid.to_text().lines().position(|l| l == "line5").unwrap();
        assert_eq!((grid.cursor.row, grid.cursor.col), (row, 0));
    }

    #[test]
    fn held_output_is_applied_when_the_hold_ends() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"a");
        emu.set_hold(true);
        emu.advance(b"b");
        assert_eq!(emu.grid().to_text(), "a\n\n");
        emu.set_hold(false);
        assert_eq!(emu.grid().to_text(), "ab\n\n");
    }

    #[test]
    fn a_flood_is_applied_not_held_without_limit() {
        let mut emu = Emulator::new(10, 2);
        emu.set_hold(true);
        emu.advance(&vec![b'x'; HOLD_LIMIT + 1]);
        assert!(emu.grid().to_text().starts_with('x'), "applied at once");
    }
```

- [ ] **Step 6: Run to verify they fail**

Run: `cargo test -p mkrew-ui copy_ held_output a_flood`
Expected: FAIL (compile error: `CopyKey`, `copy_mode`, `set_hold` not defined).

- [ ] **Step 7: Implement in `emulator.rs`**

Imports (merge into the existing blocks): `use alacritty_terminal::vi_mode::ViMotion;` and `use crate::terminal::copy_mode::{CopyKey, Select};`. The tests module reaches both through `use super::*`.

Constant after `INCREMENTAL_SEARCH_LINES`:

```rust
/// Output held back while the screen must stay as it is (copy mode, link labels) is applied at
/// once above this size.
// ponytail: a flood while holding moves the view instead of growing memory; page the held
// output if a user ever copies from a pane that prints more than this.
const HOLD_LIMIT: usize = 1 << 20;
```

Type after `Cursor`/`Grid` definitions (before `pub struct Emulator`):

```rust
/// What a copy-mode key asks of the pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyEnd {
    /// Still in copy mode.
    Stay,
    /// Left copy mode; the selected text (`y` / Enter), if any, goes to the clipboard.
    Leave(Option<String>),
}
```

`Emulator` fields, after `last_grid`:

```rust
    /// Output waits in `held` while this is set or copy mode is on (see [`Emulator::set_hold`]).
    hold: bool,
    held: Vec<u8>,
```

In `Emulator::new`: add to the `Config { … }` literal `vi_mode_cursor_style: Some(CursorStyle { shape: CursorShape::Block, blinking: false }),` and to `Self { … }`: `hold: false, held: Vec::new(),`.

Replace `advance` (keep `resize`) with:

```rust
    pub fn advance(&mut self, bytes: &[u8]) {
        if self.holding() {
            if self.held.len() + bytes.len() <= HOLD_LIMIT {
                self.held.extend_from_slice(bytes);
                return;
            }
            self.apply_held();
        }
        self.apply(bytes);
    }

    fn apply(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
        self.changed();
        self.recheck_current_match();
    }

    /// Copy mode or an explicit hold: the screen must not change under the user.
    fn holding(&self) -> bool {
        self.hold || self.copy_mode()
    }

    fn apply_held(&mut self) {
        if !self.held.is_empty() {
            let held = std::mem::take(&mut self.held);
            self.apply(&held);
        }
    }

    /// Holds incoming output back (link labels stay under their links) or lets it in again;
    /// copy mode holds on its own.
    pub fn set_hold(&mut self, hold: bool) {
        self.hold = hold;
        if !self.holding() {
            self.apply_held();
        }
    }
```

Copy-mode API, after `selection_text`:

```rust
    /// Copy mode (SPEC §10) is `alacritty_terminal`'s vi mode: a cursor that moves over the
    /// screen and the history, and a selection that follows it.
    pub fn copy_mode(&self) -> bool {
        self.term.mode().contains(TermMode::VI)
    }

    /// Turns copy mode on: the vi cursor starts at the program's cursor (the top-left cell of
    /// the view when that is scrolled out).
    pub fn enter_copy_mode(&mut self) {
        if !self.copy_mode() {
            self.term.toggle_vi_mode();
            self.changed();
        }
    }

    /// Turns copy mode off: the selection goes, held output comes in, the view returns to the
    /// live screen.
    pub fn leave_copy_mode(&mut self) {
        if self.copy_mode() {
            self.term.toggle_vi_mode();
        }
        self.term.selection = None;
        if !self.holding() {
            self.apply_held();
        }
        self.term.scroll_display(Scroll::Bottom);
        self.changed();
    }

    /// Applies one copy-mode key (see [`crate::terminal::copy_mode::copy_key`]).
    pub fn copy_key(&mut self, key: CopyKey) -> CopyEnd {
        match key {
            CopyKey::Motion(motion) => self.term.vi_motion(motion),
            CopyKey::Page { up } => self.copy_page(up),
            CopyKey::Edge { top: true } => {
                self.term.scroll_display(Scroll::Top);
                self.term.vi_motion(ViMotion::High);
            }
            CopyKey::Edge { top: false } => {
                self.term.scroll_display(Scroll::Bottom);
                self.term.vi_motion(ViMotion::Low);
            }
            CopyKey::Select(select) => self.copy_select(select),
            CopyKey::Copy => {
                let text = self.selection_text();
                self.leave_copy_mode();
                return CopyEnd::Leave(text);
            }
            CopyKey::Cancel => {
                if self.term.selection.take().is_none() {
                    self.leave_copy_mode();
                    return CopyEnd::Leave(None);
                }
            }
        }
        self.changed();
        CopyEnd::Stay
    }

    /// One screen back / forward; the cursor stays on its screen row.
    fn copy_page(&mut self, up: bool) {
        let cursor = self.term.vi_mode_cursor.point;
        let row = cursor.line.0 + self.display_offset() as i32;
        self.term
            .scroll_display(if up { Scroll::PageUp } else { Scroll::PageDown });
        let line = Line(row - self.display_offset() as i32);
        self.term.vi_goto_point(Point::new(line, cursor.column));
    }

    /// `v` / Shift+V: starts a selection of the cursor's cell, switches the kind of a
    /// selection, or drops one of the same kind.
    fn copy_select(&mut self, select: Select) {
        let ty = match select {
            Select::Chars => SelectionType::Simple,
            Select::Lines => SelectionType::Lines,
        };
        let point = self.term.vi_mode_cursor.point;
        let current = (self.term.selection.as_ref())
            .filter(|selection| !selection.is_empty())
            .map(|selection| selection.ty);
        match current {
            Some(current) if current == ty => self.term.selection = None,
            Some(_) => {
                if let Some(selection) = self.term.selection.as_mut() {
                    selection.ty = ty;
                }
            }
            None => self.term.selection = Some(Selection::new(ty, point, Side::Left)),
        }
        // A new selection covers the cell under the cursor (its two sides differ).
        if let Some(selection) = self.term.selection.as_mut() {
            selection.include_all();
        }
    }
```

In `search_from_view`, directly after `self.current_match = found;` (before the final `self.current_match.is_some()`):

```rust
        // Copy mode: the match is where the cursor goes.
        if self.copy_mode()
            && let Some(found) = &self.current_match
        {
            self.term.vi_goto_point(*found.start());
        }
```

- [ ] **Step 8: Run to verify they pass**

Run: `cargo test -p mkrew-ui terminal::`
Expected: PASS (the new tests and every existing emulator, find, links and layout test).

- [ ] **Step 9: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/terminal.rs crates/mkrew-ui/src/terminal/copy_mode.rs crates/mkrew-ui/src/terminal/emulator.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): copy mode in the terminal emulator (vi mode, held output)"
```

---

### Task 5: `CopyMode` command, key routing, pane indicator

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum after `SendSelection` ~75, `all()` ~246, `name()` ~336, `default_keys()` ~438, `context()` ~500)
- Modify: `crates/mkrew-ui/src/palette.rs` (`title` ~235, `available` ~339), `crates/mkrew-ui/src/palette/tests.rs` (~196, ~213)
- Modify: `crates/mkrew-ui/src/terminal/view.rs:12-40` (imports), `:79-97` (events), `:187-229` (`set_links`, `load_snapshot`), `:262-286` (`key_down`), `:468-499` (`scroll_wheel`), `:501-528` (`run`), `:719-726` (`type_text`)
- Modify: `crates/mkrew-ui/src/shell/sessions.rs` (arm `Command::Copy | Command::Paste | Command::PageUp | Command::PageDown => {}`), `crates/mkrew-ui/src/shell/focus.rs` (subscriptions after `AttachFailed`), `crates/mkrew-ui/src/main.rs` (`use mkrew_ui::terminal::view::{…}`), `crates/mkrew-ui/src/shell/pane_render.rs` (`pane_status`)

**Interfaces:**
- Consumes: `CopyKey`, `copy_key`, `CopyEnd`, `Emulator::{copy_mode, enter_copy_mode, leave_copy_mode, copy_key}` (Task 4).
- Produces:
  - `Command::CopyMode` (`copy_mode`, `alt-shift-c`, key context `TERMINAL_CONTEXT`).
  - `terminal::view::CopyModeChanged` (unit struct event, emitted when copy mode turns on or off); `TerminalPane::in_copy_mode(&self) -> bool`.
  - The pane header / tab bar shows a "Copy mode" chip.

- [ ] **Step 1: Command** - `keymap.rs`: enum after `SendSelection,`:

```rust
    /// Terminal only: keyboard selection with vi keys (copy mode, SPEC §10).
    CopyMode,
```

`all()` after `Command::SendSelection,` add `Command::CopyMode,`; `name()` after the `SendSelection` arm: `Command::CopyMode => "copy_mode".into(),`; `default_keys()` after the `SendSelection` arm: `Command::CopyMode => "alt-shift-c".into(),`; `context()`: change the terminal arm to

```rust
            Command::Copy
            | Command::Paste
            | Command::PageUp
            | Command::PageDown
            | Command::CopyMode => Some(TERMINAL_CONTEXT),
```

Palette `title`, after the `SendSelection` arm: `Command::CopyMode => "Terminal: Copy mode (select and copy with the keyboard)".into(),`. `available`: change the terminal arm to

```rust
        Command::Copy
        | Command::Paste
        | Command::PageUp
        | Command::PageDown
        | Command::CopyMode => on_panes && context.terminal,
```

Palette tests: add `Command::CopyMode,` after `Command::Copy,` in the not-available-in-Terminals list, and `assert!(focused.contains(&Command::CopyMode));` after `assert!(focused.contains(&Command::Find));`.

`shell/sessions.rs`: change the arm `Command::Copy | Command::Paste | Command::PageUp | Command::PageDown => {}` to

```rust
            Command::Copy
            | Command::Paste
            | Command::PageUp
            | Command::PageDown
            | Command::CopyMode => {}
```

- [ ] **Step 2: Pane events and state** - `terminal/view.rs`:

Imports: add `use crate::terminal::copy_mode::{CopyKey, copy_key};` and change the emulator import to `use crate::terminal::emulator::{CopyEnd, Emulator, Grid};`.

After the `AttachFailed` event impl:

```rust
/// Copy mode turned on or off: the shell redraws the pane's "Copy mode" chip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyModeChanged;

impl EventEmitter<CopyModeChanged> for TerminalPane {}
```

After `selection_text`:

```rust
    /// Copy mode is on: the keys are the UI's, not the program's.
    pub fn in_copy_mode(&self) -> bool {
        self.emulator.copy_mode()
    }
```

`load_snapshot` keeps copy mode on (the shell has no `cx` there to announce an end):

```rust
    pub fn load_snapshot(&mut self, snapshot: &ScreenSnapshot) {
        let copying = self.emulator.copy_mode();
        self.emulator = Emulator::from_snapshot(snapshot);
        if copying {
            self.emulator.enter_copy_mode();
        }
        if let Some(find) = &self.find {
            find.reapply(&mut self.emulator);
        }
    }
```

- [ ] **Step 3: Key routing** - `key_down` gets a copy-mode branch first (keep its doc comment, add "While copy mode is on every key is the UI's."):

```rust
    fn key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.emulator.copy_mode() {
            cx.stop_propagation();
            self.copy_key_down(event, cx);
            return;
        }
        // …the existing body unchanged…
```

New methods in `impl TerminalPane`, after `copy_selection`:

```rust
    /// A key in copy mode: moves the vi cursor or selects (see [`copy_key`]); `y` / Enter put
    /// the selection on the clipboard and leave. Unknown keys are swallowed: nothing reaches
    /// the session.
    fn copy_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<'_, Self>) {
        let Some(key) = copy_key(&event.keystroke) else {
            return;
        };
        if let CopyEnd::Leave(text) = self.emulator.copy_key(key) {
            if let Some(text) = text {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
            cx.emit(CopyModeChanged);
        }
        cx.notify();
    }

    fn toggle_copy_mode(&mut self, cx: &mut Context<'_, Self>) {
        if self.emulator.copy_mode() {
            self.emulator.leave_copy_mode();
        } else {
            self.emulator.enter_copy_mode();
        }
        cx.emit(CopyModeChanged);
        cx.notify();
    }
```

Replace `run` entirely:

```rust
    /// The terminal's own commands; all others go on to the window shell.
    fn run(&mut self, Run(command): &Run, window: &mut Window, cx: &mut Context<'_, Self>) {
        let find_focused = self.find_focus.is_focused(window);
        match command {
            Command::Find => {
                self.find.get_or_insert_default();
                window.focus(&self.find_focus, cx);
                cx.notify();
            }
            Command::Paste if self.find.is_some() && find_focused => {
                let text = cx.read_from_clipboard().and_then(|item| item.text());
                if let (Some(find), Some(text)) = (self.find.as_mut(), text) {
                    find.insert(&text, &mut self.emulator);
                    cx.notify();
                }
            }
            Command::CopyMode if !find_focused => self.toggle_copy_mode(cx),
            Command::Copy => {
                self.copy_selection(cx);
            }
            Command::PageUp | Command::PageDown => {
                let up = *command == Command::PageUp;
                if self.emulator.copy_mode() {
                    self.emulator.copy_key(CopyKey::Page { up });
                } else {
                    self.emulator.scroll_page(up);
                }
                cx.notify();
            }
            // Keys are the UI's while copy mode is on: nothing is typed into the program.
            Command::Paste if self.emulator.copy_mode() => {}
            Command::Paste => {
                self.paste(cx);
            }
            _ => cx.propagate(),
        }
    }
```

`scroll_wheel`: change `if self.emulator.wheel_sends_arrows() {` to `if self.emulator.wheel_sends_arrows() && !self.emulator.copy_mode() {` (in copy mode the wheel scrolls the UI history, never types arrows).

`type_text`: change `if self.exited.is_some() || text.is_empty() {` to `if self.exited.is_some() || text.is_empty() || self.emulator.copy_mode() {` (dropped paths and voice text are not typed while keys are the UI's).

- [ ] **Step 4: Chip, subscription, imports** -

`main.rs`: `use mkrew_ui::terminal::view::{AttachFailed, CopyModeChanged, OpenLink, TerminalPane};`

`shell/focus.rs`, after the `AttachFailed` subscription block:

```rust
                cx.subscribe(&pane, |_, _, _: &CopyModeChanged, cx| cx.notify())
                    .detach();
```

`shell/pane_render.rs` `pane_status`, right before the final `parts` (after the context-share block):

```rust
        if self
            .panes
            .get(&session)
            .is_some_and(|pane| pane.read(cx).in_copy_mode())
        {
            parts.push(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(space::S))
                    .px(px(space::S))
                    .rounded_full()
                    .whitespace_nowrap()
                    .bg(rgb(theme.accent_bg))
                    .child(
                        div()
                            .font_weight(FontWeight(weight::MEDIUM))
                            .text_color(rgb(theme.accent_text))
                            .child("Copy mode"),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme.text_muted))
                            .child("v select · y copy · Esc exit"),
                    )
                    .into_any_element(),
            );
        }
```

- [ ] **Step 5: Build and test**

Run: `cargo build -p mkrew-ui` then `cargo test -p mkrew-ui palette keymap terminal::`
Expected: build OK; tests PASS (the key tests cover the default `alt-shift-c` being unique and the palette title distinct).

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): terminal copy mode on Alt+Shift+C"
```

---

### Task 6: Pure link labels and row scan

**Files:**
- Create: `crates/mkrew-ui/src/terminal/hints.rs`
- Modify: `crates/mkrew-ui/src/terminal.rs` (`pub mod hints;` after `pub mod find;`)
- Modify: `crates/mkrew-ui/src/terminal/links.rs:38-52` (after `link_at`), tests module

**Interfaces:**
- Produces (used by Task 7):
  - `links::links_in(cells: &[char]) -> Vec<Link>`: every link of a row of chars (one char per cell), left to right.
  - `hints::labels(count: usize) -> Vec<String>`: prefix-free labels, single letters first then two letters; at most 676 (fewer than `count` when more are asked: callers `zip`).
  - `hints::Typed { Pick(usize), Narrow, Nothing }`; `hints::typed(labels: &[String], typed: &str) -> Typed`.

- [ ] **Step 1: Write the failing tests** - in `links.rs` tests module append:

```rust
    #[test]
    fn every_link_of_a_row_in_order() {
        let row = chars("see src/a.rs:3 and https://x.dev/a, then Cargo.toml.");
        let found: Vec<Target> = links_in(&row).into_iter().map(|link| link.target).collect();
        assert_eq!(
            found,
            [
                path("src/a.rs", Some(3)),
                url("https://x.dev/a"),
                path("Cargo.toml", None)
            ]
        );
        assert!(links_in(&chars("no links here")).is_empty());
        assert!(links_in(&[]).is_empty());
    }

    #[test]
    fn a_row_with_wide_chars_keeps_columns() {
        let row = ['日', SPACER, ' ', 's', 'r', 'c', '/', 'a', '.', 'r', 's'];
        let found = links_in(&row);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].start, found[0].end), (3, 11));
    }
```

Create `crates/mkrew-ui/src/terminal/hints.rs` with the module doc, `labels` and `typed` stubbed by `todo!()`, and:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_few_links_get_the_home_row() {
        assert_eq!(labels(3), ["a", "s", "d"]);
        assert!(labels(0).is_empty());
        assert_eq!(labels(26).len(), 26);
    }

    #[test]
    fn more_links_than_letters_get_two_letters_after_the_single_ones() {
        let all = labels(27);
        assert_eq!(all.len(), 27);
        assert_eq!(all[0], "a");
        assert_eq!(all[24], "n", "the last single letter");
        assert_eq!(&all[25..], ["ma", "ms"], "two letters start with the unused letter");
        assert!(!all.contains(&"m".to_owned()), "m is a prefix now");
    }

    #[test]
    fn labels_never_start_with_each_other() {
        for count in [1, 5, 26, 27, 60, 200, 676, 700] {
            let all = labels(count);
            assert_eq!(all.len(), count.min(676), "{count}");
            for (i, a) in all.iter().enumerate() {
                assert!(a.chars().all(|c| c.is_ascii_lowercase()));
                for (j, b) in all.iter().enumerate() {
                    assert!(i == j || !b.starts_with(a.as_str()), "{count}: {a} / {b}");
                }
            }
        }
    }

    #[test]
    fn typing_picks_narrows_or_cancels() {
        let all = labels(30);
        assert_eq!(typed(&all, "a"), Typed::Pick(0));
        assert_eq!(typed(&all, "m"), Typed::Narrow);
        assert_eq!(typed(&all, "ms"), Typed::Pick(26));
        assert_eq!(typed(&all, "mq"), Typed::Nothing);
        assert_eq!(typed(&all, "1"), Typed::Nothing);
        assert_eq!(typed(&[], "a"), Typed::Nothing);
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mkrew-ui -- hints:: links_in a_row_with_wide`
Expected: FAIL (`not yet implemented`, or compile error before `links_in` exists).

- [ ] **Step 3: Implement** - `links.rs`, after `link_at`:

```rust
/// Every link of `cells` (one char per cell, as for [`link_at`]), left to right.
pub fn links_in(cells: &[char]) -> Vec<Link> {
    let mut found = Vec::new();
    let mut col = 0;
    while col < cells.len() {
        match link_at(cells, col) {
            Some(link) => {
                col = link.end.max(col + 1);
                found.push(link);
            }
            None => col += 1,
        }
    }
    found
}
```

`hints.rs` body above the tests:

```rust
//! Link hints (SPEC §10): short labels over the links on screen; typing a label opens its
//! link. Pure and tested headlessly; the pane scans the rows, draws the labels and opens the
//! link (like Ctrl+Click). The letters are not in `keymap.toml`.

/// The home row first.
const ALPHABET: &[u8] = b"asdfghjklqwertyuiopzxcvbnm";

/// `count` labels in link order: single letters first, then two letters. No label starts
/// another, so a label is complete the moment it is typed: the last letters of the alphabet
/// stop being labels and become the first letter of the two-letter ones.
// ponytail: at most 26 * 26 labels; links past that get none. A screen holds far fewer.
pub fn labels(count: usize) -> Vec<String> {
    let n = ALPHABET.len();
    // `prefixes` letters give up being labels: capacity (n - prefixes) + prefixes * n.
    let prefixes = if count <= n {
        0
    } else {
        (count - n).div_ceil(n - 1).min(n)
    };
    let letter = |i: usize| char::from(ALPHABET[i]);
    let mut out: Vec<String> = (0..n - prefixes).map(|i| letter(i).to_string()).collect();
    for first in n - prefixes..n {
        for second in 0..n {
            out.push(format!("{}{}", letter(first), letter(second)));
        }
    }
    out.truncate(count);
    out
}

/// What the letters typed so far mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    /// A whole label: open the link at this index.
    Pick(usize),
    /// Some labels still start with it: wait for the next letter.
    Narrow,
    /// None does: cancel.
    Nothing,
}

/// `typed` against `labels` (prefix-free, see [`labels`]).
pub fn typed(labels: &[String], typed: &str) -> Typed {
    if let Some(at) = labels.iter().position(|label| label == typed) {
        return Typed::Pick(at);
    }
    if !typed.is_empty() && labels.iter().any(|label| label.starts_with(typed)) {
        Typed::Narrow
    } else {
        Typed::Nothing
    }
}
```

Add `pub mod hints;` after `pub mod find;` in `terminal.rs`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p mkrew-ui -- hints:: links_in a_row_with_wide`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src/terminal.rs crates/mkrew-ui/src/terminal/hints.rs crates/mkrew-ui/src/terminal/links.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): pure link labels and row scan for link hints"
```

---

### Task 7: `LinkHints` command, overlay, opening, notice

**Files:**
- Modify: `crates/mkrew-ui/src/keymap.rs` (enum, `all()`, `name()`, `default_keys()`, `context()` as in Task 5), `crates/mkrew-ui/src/palette.rs`, `crates/mkrew-ui/src/palette/tests.rs`, `crates/mkrew-ui/src/shell/sessions.rs`
- Modify: `crates/mkrew-ui/src/terminal/view.rs` (events, fields, `link_in`, `key_down`, `run`, `paint`)
- Modify: `crates/mkrew-ui/src/theme.rs:485` (token), `crates/mkrew-ui/src/shell/toasts.rs`, `crates/mkrew-ui/src/shell/focus.rs`, `crates/mkrew-ui/src/shell/pane_render.rs:210-214`, `crates/mkrew-ui/src/main.rs` (field `tab_notice`, import), `crates/mkrew-ui/src/shell/connection.rs` (init)

**Interfaces:**
- Consumes: `hints::{labels, typed, Typed}`, `links::links_in` (Task 6); `Emulator::set_hold` (Task 4); the existing `OpenLink` event and `Shell::open_terminal_link`.
- Produces:
  - `Command::LinkHints` (`link_hints`, `alt-shift-o`, key context `TERMINAL_CONTEXT`).
  - `terminal::view::NoLinks` (unit struct event).
  - `Shell::show_tab_notice(&mut self, text: &str, cx)`, field `Shell.tab_notice: Option<String>`, `theme::TAB_NOTICE_TIMEOUT`.

- [ ] **Step 1: Command** - `keymap.rs`: enum, after `CopyMode,`:

```rust
    /// Terminal only: label the links on screen; typing a label opens that link (SPEC §10).
    LinkHints,
```

`all()` after `Command::CopyMode,` add `Command::LinkHints,`; `name()`: `Command::LinkHints => "link_hints".into(),`; `default_keys()`: `Command::LinkHints => "alt-shift-o".into(),`; `context()` terminal arm gains `| Command::LinkHints`.

`palette.rs` `title`: `Command::LinkHints => "Terminal: Open a link by its label".into(),`; the `available` terminal arm gains `| Command::LinkHints`. `palette/tests.rs`: add `Command::LinkHints,` to the not-available-in-Terminals list and `assert!(focused.contains(&Command::LinkHints));`.

`shell/sessions.rs`: the "handled by the focused terminal pane" arm gains `| Command::LinkHints`.

- [ ] **Step 2: Notice plumbing** -

`theme.rs`, after `PEEK_LEAVE_GRACE`:

```rust
/// How long a one-line notice stays in the tab bar before it goes away by itself.
pub const TAB_NOTICE_TIMEOUT: Duration = Duration::from_secs(3);
```

`main.rs` `struct Shell`, after `tab_error: Option<String>,`:

```rust
    /// A one-line notice in the tab bar that goes away by itself (link hints found none).
    tab_notice: Option<String>,
```

`shell/connection.rs`, after `tab_error: None,` in the initialiser: `tab_notice: None,`.

`shell/pane_render.rs` `render_tab_bar`, right after the `.children(self.tab_error.clone().map(...))` block:

```rust
            .children(
                self.tab_notice
                    .clone()
                    .map(|text| notice(theme.text_muted, text)),
            )
```

`shell/toasts.rs`, in the `impl Shell` block:

```rust
    /// A one-line notice in the tab bar (nothing went wrong, so no toast card); it goes away
    /// by itself.
    pub(crate) fn show_tab_notice(&mut self, text: &str, cx: &mut Context<'_, Self>) {
        let text = text.to_owned();
        self.tab_notice = Some(text.clone());
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(theme::TAB_NOTICE_TIMEOUT)
                .await;
            let _ = this.update(cx, |shell, cx| {
                if shell.tab_notice.as_deref() == Some(text.as_str()) {
                    shell.tab_notice = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }
```

`main.rs` import: `use mkrew_ui::terminal::view::{AttachFailed, CopyModeChanged, NoLinks, OpenLink, TerminalPane};`. `shell/focus.rs`, after the `CopyModeChanged` subscription:

```rust
                cx.subscribe(&pane, |shell, _, _: &NoLinks, cx| {
                    shell.show_tab_notice("No links on screen.", cx);
                })
                .detach();
```

- [ ] **Step 3: Pane state and scan** - `terminal/view.rs`:

Imports: `use crate::terminal::hints::{self, Typed};` and make the links import `use crate::terminal::links::{self, Target};` (already so).

After the `CopyModeChanged` event:

```rust
/// Link hints found no link on screen: the shell shows a notice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoLinks;

impl EventEmitter<NoLinks> for TerminalPane {}
```

New private struct after `HoverLink`:

```rust
/// Link hints on screen (Alt+Shift+O): each link with its label, and the letters typed so far.
struct Hints {
    links: Vec<HoverLink>,
    labels: Vec<String>,
    typed: String,
}
```

Field on `TerminalPane` after `probed`: `hints: Option<Hints>,` and `hints: None,` in `with_emulator`. In `load_snapshot` add `self.hints = None;` as the first line (the new screen has other text).

Split `link_in` so the scan shares its conversion:

```rust
    /// The link at a cell, read from that row of the cached grid only.
    fn link_in(&self, row: usize, col: usize) -> Option<HoverLink> {
        let cells = self.emulator.shared_grid().row_chars(row)?;
        self.hover_link(row, links::link_at(&cells, col)?)
    }

    /// `link` of screen row `row` as something the shell can open: a URL, or a file of the
    /// pane's folder (checked on disk).
    fn hover_link(&self, row: usize, link: links::Link) -> Option<HoverLink> {
        let base = self.links.as_ref()?;
        let open = match link.target {
            Target::Url(url) => OpenLink::Url(url),
            Target::Path { path, line } => OpenLink::File {
                path: links::resolve(base.as_deref()?, &path)?,
                line,
            },
        };
        Some(HoverLink {
            row,
            start: link.start,
            end: link.end,
            open,
        })
    }

    /// Alt+Shift+O: labels every openable link of the visible rows (the grid is read once,
    /// the disk once per path link) and holds the program's output back so the labels stay
    /// under their links. No link: [`NoLinks`].
    fn start_hints(&mut self, cx: &mut Context<'_, Self>) {
        let grid = self.emulator.shared_grid();
        let mut found = Vec::new();
        for row in 0..grid.rows.len() {
            let Some(cells) = grid.row_chars(row) else {
                continue;
            };
            for link in links::links_in(&cells) {
                found.extend(self.hover_link(row, link));
            }
        }
        if found.is_empty() {
            cx.emit(NoLinks);
            return;
        }
        let labels = hints::labels(found.len());
        found.truncate(labels.len());
        self.clear_link(cx);
        self.emulator.set_hold(true);
        self.hints = Some(Hints {
            links: found,
            labels,
            typed: String::new(),
        });
        cx.notify();
    }

    /// Drops the labels and lets held output in. The caller notifies.
    fn stop_hints(&mut self) {
        self.hints = None;
        self.emulator.set_hold(false);
    }

    /// A key while labels show: a letter narrows or picks (the link opens exactly like
    /// Ctrl+Click); Esc or any other key cancels. Nothing reaches the session.
    fn hint_key(&mut self, event: &KeyDownEvent, cx: &mut Context<'_, Self>) {
        let m = event.keystroke.modifiers;
        let letter = (!m.control && !m.alt && !m.platform)
            .then_some(event.keystroke.key_char.as_deref())
            .flatten()
            .map(str::to_lowercase);
        let outcome = match (self.hints.as_mut(), letter) {
            (Some(hints), Some(letter)) => {
                hints.typed.push_str(&letter);
                hints::typed(&hints.labels, &hints.typed)
            }
            _ => Typed::Nothing,
        };
        match outcome {
            Typed::Narrow => {}
            Typed::Pick(at) => {
                let link = (self.hints.take()).and_then(|hints| hints.links.into_iter().nth(at));
                self.stop_hints();
                if let Some(link) = link {
                    cx.emit(link.open);
                }
            }
            Typed::Nothing => self.stop_hints(),
        }
        cx.notify();
    }
```

`key_down` gets the hint branch before the copy-mode one:

```rust
        if self.hints.is_some() {
            cx.stop_propagation();
            self.hint_key(event, cx);
            return;
        }
```

`run` gets, at its top (before `let find_focused`):

```rust
        // Any command but its own ends the labels: the keys that were typing them moved on.
        if !matches!(command, Command::LinkHints) && self.hints.is_some() {
            self.stop_hints();
            cx.notify();
        }
```

and one arm after `Command::CopyMode if !find_focused => …`:

```rust
            Command::LinkHints if !find_focused => self.start_hints(cx),
```

- [ ] **Step 4: Overlay** - in `TerminalElement::paint`, right after the hovered-link underline block (`if let Some(link) = &self.pane.read(cx).hover_link … { … }`) and before `let pane = self.pane.read(cx);`:

```rust
        // Link hints: a label chip over the first cells of each link that still matches what
        // was typed, outside the row cache.
        if let Some(hints) = &self.pane.read(cx).hints {
            for (link, label) in hints.links.iter().zip(&hints.labels) {
                if link.row >= grid.rows.len() || !label.starts_with(&hints.typed) {
                    continue;
                }
                let at = point(
                    origin.x + cell_width * link.start as f32,
                    origin.y + line_height * link.row as f32,
                );
                let chip = Bounds::new(
                    at,
                    size(cell_width * label.chars().count() as f32, line_height),
                );
                window.paint_quad(fill(chip, hsla(theme.accent)));
                let style = CellStyle {
                    fg: theme.on_accent,
                    bg: theme.accent,
                    bold: true,
                    italic: false,
                    underline: false,
                };
                let run = text_run(label.len(), style, &base_font, theme.terminal_underline_thickness);
                let shaped = text_system.shape_line(
                    SharedString::from(label.clone()),
                    font_size,
                    &[run],
                    Some(cell_width),
                );
                // A failed glyph paint only loses that glyph.
                let _ = shaped.paint(at, line_height, TextAlign::Left, None, window, cx);
            }
        }
```

(`text_system`, `base_font`, `font_size`, `origin`, `cell_width`, `line_height` and `grid` are the locals already defined earlier in `paint`.)

- [ ] **Step 5: Build and test**

Run: `cargo build -p mkrew-ui` then `cargo test -p mkrew-ui palette keymap terminal::`
Expected: build OK; tests PASS.

- [ ] **Step 6: Commit**

```bash
cargo fmt --all
cargo clippy -p mkrew-ui --all-targets -- -D warnings
git add crates/mkrew-ui/src
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): open terminal links by label on Alt+Shift+O"
```

---

### Task 8: ADR 0085, SPEC and HUMAN_CHECKS

**Files:**
- Modify: `docs/decisions/0085-keyboard-only-operation.md` (append a section; wave 1 created the file, check it exists with `ls docs/decisions/0085*`)
- Modify: `docs/SPEC.md` (§10 after the "Terminal links" bullet, §12 "Comments" bullet, §12B "Diff comments → agent" bullet, §13 table and the note after it)
- Modify: `docs/HUMAN_CHECKS.md` (new heading under "To verify (non-blocking)")

- [ ] **Step 1: ADR 0085** - append at the end of the file:

```markdown

## Wave 3 - Picking by keyboard

- Lines of a doc or diff are picked with `PickDown` / `PickUp` (`shift-down` / `shift-up`, key context `Docs || Git`) over the existing `comments::Selection`; no new selection type.
- `pick::step` is pure over "units": source lines (Docs source), top-level blocks (Docs preview) and new-side diff lines (Git).
  Nothing picked: the first line or block in view, in Git the first new-side line of the current hunk (`n` / `p` choose the hunk).
  Picked: Shift+↓ moves the end that was last moved, Shift+↑ shrinks it, a single line grows towards the key's direction.
  `Comments.head` remembers which end moves; a click resets it.
- Shift+↑/↓ stays with a focused text field, a comment draft or the Files editor: the shell's `run` already hands every command to them, and the Git key context sits on the file list only.
- Esc clears the pick in Docs (Git already did).
- Terminal copy mode is `alacritty_terminal`'s vi mode (`Term::toggle_vi_mode`, `vi_motion`) on the focused pane's emulator, `copy_mode` = `alt-shift-c`.
  While it is on, keys go to the UI and never to the PTY; the keys are not in `keymap.toml`.
  The emulator holds incoming output back while copy mode (or link labels) is on, so the view and the cursor stay on their text; leaving applies it and returns to the live screen.
  Held output is bounded (1 MiB); a flood above that is applied at once.
- Link hints (`link_hints` = `alt-shift-o`) scan the visible rows with the same detection as Ctrl+Click and label each openable link: single letters first, then two letters, never a label that starts another.
  The picked link opens through the `OpenLink` event of Ctrl+Click.
  No link: a tab bar notice "No links on screen." that goes away after 3 seconds.
- Consequence: `Alt+Shift+C` and `Alt+Shift+O` no longer reach a focused terminal (`ESC C`, `ESC O`); rebind `copy_mode` / `link_hints` in Settings → Keyboard if a program needs them.
- Out of scope: Ctrl+Shift+L in Docs (it sends from Files, the Git diff and terminals only, as before).
```

- [ ] **Step 2: SPEC §12 and §12B** - in `docs/SPEC.md`:

Replace `- **Comments:** pick lines (click a preview block or a source line, Shift+click extends) →` with

```markdown
- **Comments:** pick lines (click a preview block or a source line, Shift+click extends; Shift+↓ / Shift+↑ pick the first line or block in view, then grow or shrink the pick by one line or block, scrolled into view; Esc clears it; a text field keeps its own Shift+↑/↓, ADR 0085) →
```

Replace `- **Diff comments → agent:** pick diff lines (click a new-side line, Shift+click extends; as in` with

```markdown
- **Diff comments → agent:** pick diff lines (click a new-side line, Shift+click extends; Shift+↓ / Shift+↑ pick the first new-side line of the current hunk (`n` / `p` choose it), then grow or shrink the pick one line, across hunks, scrolled into view; as in
```

- [ ] **Step 3: SPEC §10** - after the "Terminal links" bullet (it ends with "or painted."), insert:

```markdown
- **Copy mode** (`Alt+Shift+C`, ADR 0085): keyboard selection in the focused pane, on `alacritty_terminal`'s vi mode.
  While it is on the keys go to the UI, not the program, and the pane header (the tab bar for a single pane) shows `Copy mode` with the key hints.
  `←↓↑→` / `h j k l` move the cursor; `w` / `b` / `e` by word, `0` / `$` line start / end, `g` / `G` top / bottom of the history, `PgUp` / `PgDn` a page.
  `v` / `Shift+V` start, switch or drop a character / line selection.
  `y` or `Enter` copy it and leave; `Ctrl+Shift+C` copies it too; `Ctrl+Shift+L` sends it to an agent's composer; `Ctrl+Shift+F` opens the find bar, whose match moves the cursor.
  `Esc` drops the selection, or leaves copy mode when there is none.
  Output that arrives meanwhile is held back (up to 1 MiB) so the view and the cursor stay on their text; leaving copy mode applies it and returns to the live screen.
- **Link hints** (`Alt+Shift+O`, ADR 0085): labels every link of the visible rows (the same detection as Ctrl+Click, so a path counts only when it names a file of the pane's folder) with `a`, `s`, `d`, `f`, … then two letters.
  Typing a label opens that link exactly like Ctrl+Click; `Esc` or any other key cancels.
  Output is held back while the labels show.
  No link on screen: the tab bar says "No links on screen." for 3 seconds.
```

- [ ] **Step 4: SPEC §13** - in the table, after the row `| Docs | Send comments to agent | Ctrl+Shift+S |` add:

```markdown
| Docs / Git | Pick lines: the first line or block in view (Git: the current hunk's first line), then grow / shrink the pick one line or block (Esc clears; a text field keeps Shift+↑/↓) | Shift+↓ / Shift+↑ |
```

After the row `| Terminal | Page up / down | Shift+PgUp / Shift+PgDn |` add:

```markdown
| Terminal | Copy mode: select and copy with vi keys (§10) / label the links on screen and open one by typing its label (§10) | Alt+Shift+C / Alt+Shift+O |
```

In the note after the table, extend the sentence that starts `Alt+Shift+A / Alt+Shift+S and F2 no longer reach a focused terminal` by adding a new paragraph directly after it:

```markdown
Alt+Shift+C and Alt+Shift+O no longer reach a focused terminal either (`ESC C`, `ESC O`); rebind `copy_mode` / `link_hints` in Settings → Keyboard if a program needs them.
The keys of copy mode (`h j k l w b e 0 $ g G v V y`, arrows, PgUp / PgDn, Enter, Esc) and the letters of link hints are not in `keymap.toml`.
```

- [ ] **Step 5: HUMAN_CHECKS** - in `docs/HUMAN_CHECKS.md`, directly under the italic intro paragraph of "To verify (non-blocking)" and above the newest `### To verify - …` heading, insert:

```markdown
### To verify - 2026-10-27 Keyboard wave 3: picking by keyboard (ADR 0085)

- [ ] **Pick lines in Docs (dark + light):** Docs → a `.md`, nothing picked: Shift+↓ picks the first block in view (Preview) or line (Source), highlighted like a click; more Shift+↓ grows, Shift+↑ shrinks, the moved end scrolls into view; Esc clears; Ctrl+Alt+M opens the comment draft on it.
- [ ] **Pick lines in the Git diff (dark + light, Unified + Split):** Git → a changed file with 2+ hunks: `n` then Shift+↓ picks the first new-side line of that hunk, Shift+↓ grows across the gap into the next hunk and scrolls; the pick bar sits under the last line; Esc clears; Ctrl+Alt+M comments, Ctrl+Shift+L sends.
- [ ] **Text fields keep Shift+↑/↓:** in the Docs filter, a comment draft and the Git commit box (two lines typed), Shift+↑ / Shift+↓ select text and pick no lines.
- [ ] **Copy mode (dark + light):** a shell pane with a few screens of output: Alt+Shift+C → block cursor, "Copy mode" chip with the key hints in the tab bar (single pane) or pane header (split). `h j k l`, `w b e`, `0 $`, `g G`, PgUp / PgDn move it; `v` / `V` select (selection color readable); `y` copies (paste elsewhere to check) and leaves; typing `x` in copy mode types nothing in the shell; while an agent keeps printing the view does not move, and leaving shows the live screen. Ctrl+Shift+F finds and moves the cursor; Ctrl+Shift+L sends the selection.
- [ ] **Link hints (dark + light):** a pane showing a URL, a file path of the workspace and a path that does not exist: Alt+Shift+O → accent labels over the first two only; typing a label opens the URL in the browser / the file in Files at its line; Esc or another key cancels; with 30+ links two-letter labels appear and narrow as you type; with none the tab bar says "No links on screen." and clears after 3 s.
- [ ] **AltGr layouts:** on a layout where Alt+Shift+C / O type a character, rebind `copy_mode` / `link_hints` in Settings → Keyboard and check the new keys.
```

- [ ] **Step 6: Commit**

```bash
git add docs/decisions/0085-keyboard-only-operation.md docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs(spec): keyboard wave 3, picking by keyboard (ADR 0085)"
```

---

### Task 9: Full verify and keyboard-only smoke run

**Files:**
- Modify: `docs/HUMAN_CHECKS.md` (mark what the smoke run verified, leave the rest)

- [ ] **Step 1: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and every test pass. On OOM or linker errors re-run with `-j 2`. Fix anything red in the task that owns it, with a new commit.
Literal check on the wave's diff (the first commit of Task 1 to `HEAD`): `git diff <first-commit>~1 -- crates/mkrew-ui/src | grep -nE '^\+.*(#[0-9a-fA-F]{6}|px\([0-9]|Duration::from_)'` must hit only `theme.rs`.

- [ ] **Step 2: Build the dev exes**

Run: `cargo build -p mkrew-ui -p mkrew-cli -p mkrew-daemon`
Expected: `target/debug/mkrew.exe`, `mk.exe`, `mkrew-daemon.exe` built.

- [ ] **Step 3: Prepare the sandbox** (never touch the user's own daemon or data). In PowerShell, from the repo root:

```powershell
$root = Join-Path $env:TEMP "mkrew-wave3-smoke"
Remove-Item -Recurse -Force $root -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$root\data", "$root\config", "$root\repo\docs", "$root\repo\src", "$root\shots" | Out-Null
$env:MKREW_DATA_DIR = "$root\data"; $env:MKREW_CONFIG_DIR = "$root\config"
$env:MKREW_SOCKET = '\\.\pipe\mkrew-wave3-smoke'; $env:MKREW_NOTIFY = 'off'
Set-Content "$root\config\settings.toml" "[features]`ndocs = true"
Set-Content "$root\repo\docs\plan.md" "# Plan`n`nFirst paragraph.`nSecond line.`n`n## Steps`n`n- one`n- two`n`nLast paragraph.`n"
Set-Content "$root\repo\src\a.txt" "1`n2`n3`n4`n5`n6`n7`n8`n9`n10`n11`n12`n13`n14`n15`n16`n17`n18`n19`n20`n"
git -C "$root\repo" init -q; git -C "$root\repo" add -A
git -C "$root\repo" -c user.email=t@t -c user.name=t commit -q -m init
(Get-Content "$root\repo\src\a.txt") -replace '^3$', '3 changed' -replace '^18$', '18 changed' | Set-Content "$root\repo\src\a.txt"
```

Helper for keys and screenshots (keep it in the shell session, not in the repo):

```powershell
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
function K($keys) { [System.Windows.Forms.SendKeys]::SendWait($keys); Start-Sleep -Milliseconds 500 }
function Shot($name) {
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save("$root\shots\$name.png")
}
$p = Start-Process "$PWD\target\debug\mkrew.exe" -PassThru
Start-Sleep -Seconds 6
& "$PWD\target\debug\mk.exe" project add "$root\repo"
Start-Sleep -Seconds 3
(New-Object -ComObject WScript.Shell).AppActivate($p.Id) | Out-Null
```

(SendKeys notation: `+{DOWN}` Shift+Down, `%+c` Alt+Shift+C, `%+o` Alt+Shift+O, `^+5` Ctrl+Shift+5, `^+4` Ctrl+Shift+4, `%m`-style Alt; `^%m` Ctrl+Alt+M.)

- [ ] **Step 4: Perform the wave's actions keyboard-only, in dark and light** (repeat the Docs, Git and terminal checks once per theme; switch with Ctrl+Shift+J; look at each `Shot` with the `read` tool):
  1. Docs: `K '^+5'`; `K '+{DOWN}'` (picks the first block, `Shot docs-1`), `K '+{DOWN}'` twice more, `K '+{UP}'`, then `K '{ESC}'` (pick gone). Toggle Source with `K '^+e'` and repeat. `K '^%m'` opens a draft on a pick.
  2. Git: `K '^+4'`; `K 'n'`; `K '+{DOWN}'` six times (crosses into the second hunk, `Shot git-1`); `K '{ESC}'`.
  3. Fields: in Docs press `/`-free route: focus the Docs filter (`K '^+f'`), type `ab`, press `+{UP}` and `+{DOWN}`: no block picked.
  4. Terminal: `K '^+1'`; `K '^+t'` for a shell; type `echo https://example.com/a src/a.txt:3 nope/missing.rs` and Enter; `K '%+o'` (labels on the first two only, `Shot hints-1`), type the first label (the URL opens in the browser; close it). Run `1..80 | % { "line $_" }`; `K '%+c'`, then `k`, `k`, `v`, `l`, `l`, `y`; paste into the Docs filter to see the copied text. In copy mode type `x`: nothing appears in the shell. `K '%+o'` with an empty screen: the tab bar notice shows and clears after 3 s.
  5. Record in `Shot`s and notes what each did. Anything that did not work is a bug: fix it in the task that owns it, with a new commit, and repeat.

- [ ] **Step 5: Clean up** - end only the processes this step started: `Stop-Process -Id $p.Id`; stop the sandbox daemon with the `mk` subcommand that `& "$PWD\target\debug\mk.exe" --help` lists for stopping a daemon, under the same `MKREW_*` environment; then `Remove-Item -Recurse -Force $root` and clear the `MKREW_*` variables of the shell.

- [ ] **Step 6: HUMAN_CHECKS** - in the entry added in Task 8, tick (`- [x]`) and append `(smoke: dark + light, <date>)` to every line the run covered; leave unticked, and add one sentence of why, what the run could not do (the browser opening, AltGr layouts, the system clipboard of another app, the light look if a screenshot was dark-only).

- [ ] **Step 7: Commit**

```bash
git add docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs(spec): keyboard wave 3 smoke results"
```

---

## Self-review

**Spec coverage** (Wave 3 section of the spec):

| Spec requirement | Task |
| --- | --- |
| `PickDown` / `PickUp`, `shift-down` / `shift-up`, context `Docs \|\| Git` | 3 |
| Start point: Git first new-side line of the current hunk (`n` / `p`), Docs first line / block in view | 2 (Git), 3 (`pick_step`, `first_row`, `Page::top`) |
| Picked range scrolls into view | 3 (`reveal_pick`, `git_diff_reveal`) |
| Esc clears the pick in Docs (Git already) | 3 (`docs_key_down`) |
| `Ctrl+Alt+M` / `Ctrl+Shift+L` work on the pick; the pick is `comments::Selection` | 1 (no new type), smoke in 9 (Docs: Ctrl+Shift+L is Files / Git / terminal only, ADR notes it) |
| Text fields keep Shift+↑/↓ (spec principle) | Review Focus 1, smoke in 9 |
| `CopyMode` `alt-shift-c`, Terminal context; vi mode on the emulator | 4, 5 |
| Keys go to the UI, not the PTY; header shows "Copy mode" with hints | 5 |
| Motions, `v` / `V`, `y` / Enter, `Ctrl+Shift+L`, `Ctrl+Shift+F`, Esc | 4 (keys, find), 5 (routing; Ctrl+Shift+L and Ctrl+Shift+C stay global / pane commands) |
| Output in copy mode does not move the view; leaving returns live | 4 (hold) |
| `LinkHints` `alt-shift-o`: scan, labels, open like Ctrl+Click, Esc / other key cancels, notice | 6, 7 |
| Tests: pick boundaries, copy-mode emulator test, label assignment and matching | 1, 2, 4, 6 |
| SPEC §12 / §12B / §10 / §13, ADR 0085, HUMAN_CHECKS, smoke run | 8, 9 |

**Placeholder scan:** no TBD / TODO / "similar to" left; every code step has code. The only deferred lookups are the `mk` stop-daemon subcommand (Task 9 step 5: asks `--help`, it is not guessed) and line numbers after waves 1-2 (quoted anchors given).

**Type consistency:** `Unit`, `Head`, `step`, `first_row`, `Comments::{step_pick, head_line}` (Task 1) are the ones Tasks 2 and 3 call; `Shown::{line_units, block_units}` and `git_view::Shown::{pick_units, hunk_unit, line_row}`, `GitView::step_pick` (Task 2) are the ones `pick_step` / `reveal_pick` call; `CopyKey`, `Select`, `CopyEnd`, `Emulator::{copy_mode, enter_copy_mode, leave_copy_mode, copy_key, set_hold}` (Task 4) are the ones `view.rs` calls in Tasks 5 and 7; `hints::{labels, typed, Typed}` and `links::links_in` (Task 6) are the ones `start_hints` / `hint_key` call.

**Spec gaps and conflicts noticed while planning** (also reported to the parent):
- The spec says `Ctrl+Shift+L` sends the picked lines "as with a mouse pick", but `open_selection_dialog` sends only from Files, Git and terminals (`_ => return` for Docs, palette `available` excludes Docs); a Docs pick can be commented but not sent as a selection. This plan leaves it as is and records it in ADR 0085.
- The spec's copy mode says output "does not move the view" but alacritty keeps the view still only when scrolled back; at the live screen it scrolls. The plan holds output in the emulator (bounded, 1 MiB) so both cases stay put; link hints reuse the same hold so labels stay under their links.
- The spec's Wave 4 `NextComment` needs a reveal after `pick_lines`; this wave provides `Shell::reveal_pick(project_id, context, cx)` for it.
