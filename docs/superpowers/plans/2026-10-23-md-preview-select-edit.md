# Files Markdown preview: select, copy, block edit — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the Files view's Markdown preview, drag-select text across blocks and copy it
(Ctrl+C = rendered text), and edit one top-level block's Markdown in place (✎ → field →
Ctrl+Enter).

**Architecture:** A pure model `crates/mkrew-ui/src/preview_select.rs` flattens each top-level
block into the text it shows (its *pieces* located in it) and holds a `Selection` between two
`Point`s; it produces the copied text. `preview.rs` draws the selection's background into each
piece and records each painted piece's `TextLayout` (hit testing). `FilesView` owns the
selection and the block edit (pure, tested). A new shell file `shell/files_preview.rs` wires
mouse, keys, ✎ and the edit field.

**Tech Stack:** Rust, gpui-pre 0.3.7 (`StyledText`, `TextLayout`, `canvas`, `list`), existing
`TextBox`/`Input`, `Editor::replace`.

**Spec:** `docs/superpowers/specs/2026-10-23-md-preview-select-edit-design.md`

## Global Constraints

- Work in a git worktree (skill `superpowers:using-git-worktrees`); the spec and this plan live
  uncommitted in the main checkout — read them from there
  (`C:/Users/NguyenTung/Workspace/PersonalProject/mkrew-build-kit/mkrew/docs/superpowers/...`).
- Scope: Files view preview only. Docs view, Ask AI and the thread panel must render exactly as
  before (they keep calling `preview::blocks` / `preview::page(.., None, None, ..)`).
- Copy = rendered text: `\n` between blocks and lines, `\t` between table cells, list prefixes
  `- ` / `N. ` / `[x] ` / `[ ] `; Mermaid diagrams drawn as images, rules and HTML blocks copy
  nothing.
- Edit per **top-level** block; Ctrl+Enter applies (one undo step, file becomes unsaved), Esc or a
  click outside cancels.
- Numbers: ADR **0072** (0071 is the latest; 0070 is absent — re-check `docs/decisions/` before
  writing). No migration, no `PROTOCOL_VERSION` change, no new crate dependency.
- UI: theme tokens only (`theme.rs`), dark + light; skill `mkrew-ui-design`. Selection color =
  `theme.terminal_selection_bg` (already used by fields and the editor).
- No `unwrap`/`expect` outside tests. No test calls a real agent or the network.
- Git: stage explicit paths only; commit with `git -c user.email=tungnv975@gmail.com commit`.
  Never commit `docs/superpowers/specs/*`, `Cargo.lock`, `.github/workflows/*`, `packaging/*`.
  Never push, reset, stash, clean or rebase.
- Verify: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh` (plain `bash` is WSL). Locked
  exe → rename to `*.locked-old.exe`; never kill the daemon, Mkrew or omp. No `cargo clean`.
- gpui facts checked in `~/.cargo/registry/src/*/gpui-pre-0.3.7/src/elements/text.rs`:
  `TextLayout::{bounds, index_for_position, line_layouts, line_height}` **panic before the text
  is prepainted** → only record a layout from a `canvas` paint callback placed after the text;
  `StyledText::with_highlights` needs sorted, non-overlapping ranges; `TextLayout` is `Clone`,
  not `Debug`.

## Review Focus

1. **A press inside the edit field must not reach the page** (it would steal focus to the hidden
   editor and move the preview selection) → the field's wrapper stops mouse-down / move
   propagation; manual check in Task 6 HUMAN_CHECKS ("a click inside the field keeps typing there").
2. **Applying after the text changed** (file re-read from disk, or a second apply before the page
   was rebuilt) must not write over the wrong lines → `apply_block_edit` stamp check and
   `open_block_edit` refusing a stale page; tests in Task 3.
3. **Vietnamese / multi-byte text**: a selection end never splits a char, copy never panics →
   `copy_text` uses `str::get` and clamps; test `copy_keeps_vietnamese_whole` in Task 1.
4. **Typing keys in Preview never edit the hidden editor** → the preview key handler handles only
   Copy / SelectAll / Esc; test `edit_keys_do_nothing_while_previewing` in Task 3.
5. **Renderer and model drift** (a piece drawn that the model does not count, or the reverse:
   highlights land on the wrong text) → `debug_assert_eq!` on piece counts per block in Task 2
   (fires in every debug run of the app and in the Task 4/5 smoke runs).

---

### Task 1: Pure selection model (`preview_select.rs`)

**Files:**
- Create: `crates/mkrew-ui/src/preview_select.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod preview_select;` after `pub mod preview;`)

**Interfaces:**
- Consumes: `crate::docs::Shown` (`blocks`, `code: HashMap<u32, Code>` with `lines`/`spans`,
  `diagrams: HashMap<u32, Result<Diagram, String>>`), `crate::markdown::{Block, BlockKind, Item}`,
  `crate::preview::inline`, `crate::text_box::class`.
- Produces:
  - `pub struct Point { pub block: usize, pub piece: usize, pub byte: usize }` (`Copy, Ord, Hash, Default`)
  - `pub struct Selection { pub anchor: Point, pub head: Point }` (`Copy, Eq`)
  - `Selection::caret(at: Point) -> Selection`, `Selection::is_empty(&self) -> bool`,
    `Selection::range_in(&self, block: usize, piece: usize) -> Option<Range<usize>>`
    (end `usize::MAX` = to the piece's end)
  - `pub struct Flat { pub text: String, pub pieces: Vec<Range<usize>> }`
  - `pub fn flat(block: &Block, shown: &Shown) -> Flat`
  - `pub fn piece_count(block: &Block, shown: &Shown) -> usize`
  - `pub fn copy_text(shown: &Shown, selection: &Selection) -> String`
  - `pub fn select_all(shown: &Shown) -> Option<Selection>`
  - `pub fn word_at(shown: &Shown, at: Point) -> Selection`

The piece order **must** equal the order `preview::block` draws its texts (Task 2):
Heading → 1 piece (even if empty); Paragraph → 1 piece only if `runs` is non-empty; List → each
item's child blocks; Quote → child blocks; Code → one piece per `code.lines.iter().zip(&code.spans)`
of `shown.code[&block.lines.start]`, none if `shown.diagrams[&start]` is `Ok` or the code is
missing; Table → head cells then each row's cells; Rule / Html → none.

- [ ] **Step 1: Write the failing tests** — append to the new file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn shown(text: &str) -> Shown {
        Shown::from_text(text)
    }

    fn all(shown: &Shown) -> String {
        copy_text(shown, &select_all(shown).expect("something to select"))
    }

    fn at(block: usize, piece: usize, byte: usize) -> Point {
        Point { block, piece, byte }
    }

    #[test]
    fn copies_rendered_text_across_paragraph_table_and_list() {
        let page = shown("Intro **bold**\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n- one\n- [x] two\n");
        let selection = Selection { anchor: at(0, 0, 6), head: at(2, 1, 3) };
        assert_eq!(copy_text(&page, &selection), "bold\nA\tB\n1\t2\n- one\n[x] two");
    }

    #[test]
    fn a_backwards_drag_copies_the_same_text() {
        let page = shown("first line\n\nsecond line\n");
        let forward = Selection { anchor: at(0, 0, 6), head: at(1, 0, 6) };
        let backward = Selection { anchor: forward.head, head: forward.anchor };
        assert_eq!(copy_text(&page, &forward), "line\nsecond");
        assert_eq!(copy_text(&page, &backward), "line\nsecond");
    }

    #[test]
    fn numbered_lists_and_quotes_copy_their_markers_and_lines() {
        let page = shown("3. three\n4. four\n\n> quoted\n");
        assert_eq!(all(&page), "3. three\n4. four\nquoted");
    }

    #[test]
    fn code_blocks_copy_line_by_line() {
        let page = shown("```rust\nfn a() {}\n\nlet b;\n```\n");
        assert_eq!(flat(&page.blocks[0], &page).pieces.len(), 3);
        assert_eq!(all(&page), "fn a() {}\n\nlet b;");
    }

    #[test]
    fn drawn_diagrams_and_rules_copy_nothing() {
        let page = shown("a\n\n```mermaid\ngraph TD; A-->B\n```\n\n---\n\nb\n");
        assert!(
            matches!(page.diagrams.get(&3), Some(Ok(_))),
            "precondition: the diagram is drawn"
        );
        assert_eq!(piece_count(&page.blocks[1], &page), 0);
        assert_eq!(all(&page), "a\nb");
    }

    #[test]
    fn copy_keeps_vietnamese_whole() {
        let page = shown("Tiếng Việt có dấu\n");
        // "Tiếng " is 7 bytes (ế is 3); a head past the text clamps to its end.
        let selection = Selection { anchor: at(0, 0, 7), head: at(0, 0, 999) };
        assert_eq!(copy_text(&page, &selection), "Việt có dấu");
    }

    #[test]
    fn an_empty_selection_copies_nothing() {
        let page = shown("abc\n");
        assert_eq!(copy_text(&page, &Selection::caret(at(0, 0, 1))), "");
    }

    #[test]
    fn range_in_covers_the_pieces_between_the_ends() {
        let selection = Selection { anchor: at(2, 1, 4), head: at(0, 3, 2) };
        assert_eq!(selection.range_in(0, 2), None, "before the start");
        assert_eq!(selection.range_in(0, 3), Some(2..usize::MAX));
        assert_eq!(selection.range_in(1, 0), Some(0..usize::MAX));
        assert_eq!(selection.range_in(2, 1), Some(0..4));
        assert_eq!(selection.range_in(2, 2), None, "after the end");
        assert_eq!(Selection::caret(at(1, 0, 1)).range_in(1, 0), None);
    }

    #[test]
    fn double_click_selects_the_word_under_the_pointer() {
        let page = shown("Tiếng Việt có\n");
        // Byte 9 is inside "Việt" (bytes 7..13: ệ is 3 bytes).
        let word = word_at(&page, at(0, 0, 9));
        assert_eq!((word.anchor.byte, word.head.byte), (7, 13));
        assert_eq!(copy_text(&page, &word), "Việt");
    }

    #[test]
    fn select_all_spans_the_first_to_the_last_text() {
        let page = shown("---\n\nonly\n\n---\n");
        let selection = select_all(&page).unwrap();
        assert_eq!((selection.anchor, selection.head), (at(1, 0, 0), at(1, 0, 4)));
        assert_eq!(select_all(&shown("---\n")), None);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mkrew-ui preview_select`
Expected: FAIL to compile (`flat`, `copy_text`, … not defined).

- [ ] **Step 3: Write the implementation** — top of `preview_select.rs`:

```rust
//! Text selection in the Files Markdown preview (SPEC §12A, ADR 0072): each top-level block
//! flattened into the text it shows, with its *pieces* (one per drawn text element, in the
//! order `preview::block` draws them) located in it, and a selection between two points.
//! Copy gives the rendered text. Pure and tested headlessly; `preview.rs` draws it.

use std::ops::Range;

use crate::docs::Shown;
use crate::markdown::{Block, BlockKind, Item};
use crate::preview::inline;
use crate::text_box;

/// A byte of piece `piece` of top-level block `block`; ordered as on the page.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Point {
    pub block: usize,
    pub piece: usize,
    pub byte: usize,
}

/// From where the press started (`anchor`) to where the pointer is (`head`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Point,
    pub head: Point,
}

impl Selection {
    pub fn caret(at: Point) -> Self {
        Selection { anchor: at, head: at }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    fn ordered(&self) -> (Point, Point) {
        (self.anchor.min(self.head), self.anchor.max(self.head))
    }

    /// The selected bytes of piece `piece` of block `block`; `usize::MAX` = to its end.
    pub fn range_in(&self, block: usize, piece: usize) -> Option<Range<usize>> {
        let (start, end) = self.ordered();
        let key = (block, piece);
        if self.is_empty() || key < (start.block, start.piece) || key > (end.block, end.piece) {
            return None;
        }
        let from = if key == (start.block, start.piece) { start.byte } else { 0 };
        let to = if key == (end.block, end.piece) { end.byte } else { usize::MAX };
        (from < to).then_some(from..to)
    }
}

/// A top-level block as copied text, and where each piece sits in it.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Flat {
    pub text: String,
    pub pieces: Vec<Range<usize>>,
}

impl Flat {
    fn piece(&mut self, text: &str) {
        let start = self.text.len();
        self.text.push_str(text);
        self.pieces.push(start..self.text.len());
    }

    /// Where `byte` of piece `piece` sits in `text` (clamped to the piece).
    fn offset(&self, piece: usize, byte: usize) -> usize {
        match self.pieces.get(piece) {
            Some(range) => range.start + byte.min(range.len()),
            None => self.text.len(),
        }
    }
}

pub fn flat(block: &Block, shown: &Shown) -> Flat {
    let mut flat = Flat::default();
    push_block(&mut flat, block, shown);
    flat
}

/// How many texts `preview::block` draws for `block` (the renderer checks it agrees).
pub fn piece_count(block: &Block, shown: &Shown) -> usize {
    flat(block, shown).pieces.len()
}

fn push_block(flat: &mut Flat, block: &Block, shown: &Shown) {
    match &block.kind {
        BlockKind::Heading { runs, .. } => flat.piece(&inline(runs).0),
        BlockKind::Paragraph(runs) if runs.is_empty() => {}
        BlockKind::Paragraph(runs) => flat.piece(&inline(runs).0),
        BlockKind::List { start, items } => {
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    flat.text.push('\n');
                }
                flat.text.push_str(&prefix(item, start.map(|first| first + index as u64)));
                push_blocks(flat, &item.blocks, shown);
            }
        }
        BlockKind::Quote(inner) => push_blocks(flat, inner, shown),
        BlockKind::Code { .. } => {
            let first = block.lines.start;
            if matches!(shown.diagrams.get(&first), Some(Ok(_))) {
                return;
            }
            if let Some(code) = shown.code.get(&first) {
                // The same lines `preview::code_block` draws.
                for (index, (line, _)) in code.lines.iter().zip(&code.spans).enumerate() {
                    if index > 0 {
                        flat.text.push('\n');
                    }
                    flat.piece(line);
                }
            }
        }
        BlockKind::Table { head, rows, .. } => {
            for (index, row) in std::iter::once(head).chain(rows).enumerate() {
                if index > 0 {
                    flat.text.push('\n');
                }
                for (column, cell) in row.cells.iter().enumerate() {
                    if column > 0 {
                        flat.text.push('\t');
                    }
                    flat.piece(&inline(cell).0);
                }
            }
        }
        BlockKind::Rule | BlockKind::Html(_) => {}
    }
}

/// Child blocks one per line; a block that shows no text adds no line.
// ponytail: nested lists copy without indentation; add it when someone pastes nested lists.
fn push_blocks(flat: &mut Flat, blocks: &[Block], shown: &Shown) {
    let mut first = true;
    for block in blocks {
        let child = self::flat(block, shown);
        if child.pieces.is_empty() {
            continue;
        }
        if !first {
            flat.text.push('\n');
        }
        first = false;
        let base = flat.text.len();
        flat.text.push_str(&child.text);
        flat.pieces
            .extend(child.pieces.into_iter().map(|r| r.start + base..r.end + base));
    }
}

/// The marker a copied list item starts with.
fn prefix(item: &Item, number: Option<u64>) -> String {
    match (item.checked, number) {
        (Some(true), _) => "[x] ".to_owned(),
        (Some(false), _) => "[ ] ".to_owned(),
        (None, Some(number)) => format!("{number}. "),
        (None, None) => "- ".to_owned(),
    }
}

/// The selected text as shown: blocks one per line.
pub fn copy_text(shown: &Shown, selection: &Selection) -> String {
    if selection.is_empty() {
        return String::new();
    }
    let (start, end) = selection.ordered();
    let mut parts = Vec::new();
    for index in start.block..=end.block {
        let Some(block) = shown.blocks.get(index) else {
            break;
        };
        let flat = flat(block, shown);
        if flat.pieces.is_empty() {
            continue;
        }
        let from = if index == start.block { flat.offset(start.piece, start.byte) } else { 0 };
        let to = if index == end.block {
            flat.offset(end.piece, end.byte)
        } else {
            flat.text.len()
        };
        if let Some(text) = flat.text.get(from..to).filter(|text| !text.is_empty()) {
            parts.push(text.to_owned());
        }
    }
    parts.join("\n")
}

/// Ctrl+A: from the first to the last text of the page; `None` when it shows no text.
pub fn select_all(shown: &Shown) -> Option<Selection> {
    let first = (0..shown.blocks.len()).find(|&i| piece_count(&shown.blocks[i], shown) > 0)?;
    let (last, flat) = (0..shown.blocks.len()).rev().find_map(|i| {
        let flat = flat(&shown.blocks[i], shown);
        (!flat.pieces.is_empty()).then_some((i, flat))
    })?;
    let piece = flat.pieces.len() - 1;
    Some(Selection {
        anchor: Point { block: first, piece: 0, byte: 0 },
        head: Point { block: last, piece, byte: flat.pieces[piece].len() },
    })
}

/// A double click: the run of word chars (or blanks, or punctuation) under `at`.
pub fn word_at(shown: &Shown, at: Point) -> Selection {
    let text = shown.blocks.get(at.block).and_then(|block| {
        let flat = flat(block, shown);
        let range = flat.pieces.get(at.piece)?.clone();
        Some(flat.text[range].to_owned())
    });
    let Some(text) = text else {
        return Selection::caret(at);
    };
    let range = word_range(&text, at.byte);
    Selection {
        anchor: Point { byte: range.start, ..at },
        head: Point { byte: range.end, ..at },
    }
}

fn word_range(text: &str, byte: usize) -> Range<usize> {
    let mut at = byte.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let class = text[at..]
        .chars()
        .next()
        .or_else(|| text[..at].chars().next_back())
        .map(text_box::class);
    let Some(class) = class else {
        return at..at;
    };
    let start = text[..at]
        .char_indices()
        .rev()
        .take_while(|(_, c)| text_box::class(*c) == class)
        .last()
        .map_or(at, |(i, _)| i);
    let end = text[at..]
        .char_indices()
        .find(|(_, c)| text_box::class(*c) != class)
        .map_or(text.len(), |(i, _)| at + i);
    start..end
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mkrew-ui preview_select`
Expected: PASS (10 tests). If `copies_rendered_text_across_paragraph_table_and_list` fails
because `markdown::parse` shapes a tight list or task item differently, read
`markdown.rs::Builder` and fix `push_block` (never the expected text: it is the agreed copy
format).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/preview_select.rs crates/mkrew-ui/src/lib.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): selection model for the Markdown preview"
```

---

### Task 2: Draw the selection and record hit layouts (`preview.rs`)

**Files:**
- Modify: `crates/mkrew-ui/src/preview.rs` (`Page`, `page`, `block`, `blocks`, `list_item`,
  `table_row`, `code_block`, `text`, `code_line`; new `with_selection`, `nearest`,
  `closest_byte`, `Selecting`, `Hits`)
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (delete private `closest_byte`, use
  `mkrew_ui::preview::closest_byte`)
- Modify: `crates/mkrew-ui/src/shell/docs.rs` (both `preview::page` calls get the new `None`
  argument)

**Interfaces:**
- Consumes: Task 1 `preview_select::{Point, Selection, piece_count}`.
- Produces:
  - `pub struct Selecting(pub Option<Selection>);` (`Clone, Copy`) — `page`'s new argument
    `select: Option<Selecting>` placed **after** `wrap`; `None` = not selectable (Docs).
  - `Page::hit(&self, at: gpui::Point<Pixels>) -> Option<preview_select::Point>`
  - `Page::remeasure(&self)`
  - `pub fn with_selection(highlights: Vec<(Range<usize>, HighlightStyle)>, sel: Option<Range<usize>>, len: usize, bg: Hsla) -> Vec<(Range<usize>, HighlightStyle)>`
  - `pub fn nearest<K: Copy + Ord>(rects: impl IntoIterator<Item = (K, Bounds<Pixels>)>, at: gpui::Point<Pixels>) -> Option<K>`
  - `pub fn closest_byte(layout: &TextLayout, position: gpui::Point<Pixels>) -> usize` (moved
    verbatim from `widgets.rs`)
  - `preview::block` / `preview::blocks` keep their signatures (Ask AI, thread panel).

- [ ] **Step 1: Write the failing tests** — add to `preview.rs`'s `mod tests`:

```rust
    fn bold() -> HighlightStyle {
        HighlightStyle {
            font_weight: Some(FontWeight(weight::SEMIBOLD)),
            ..HighlightStyle::default()
        }
    }

    fn lit() -> Hsla {
        Hsla::from(rgb(0x334455))
    }

    #[test]
    fn selection_cuts_highlights_and_fills_plain_text() {
        let merged = with_selection(vec![(0..5, bold())], Some(3..8), 10, lit());
        let on = |style: HighlightStyle| HighlightStyle { background_color: Some(lit()), ..style };
        assert_eq!(
            merged,
            vec![(0..3, bold()), (3..5, on(bold())), (5..8, on(HighlightStyle::default()))]
        );
    }

    #[test]
    fn selection_covers_a_code_span_background_and_clamps_to_the_text() {
        let code = HighlightStyle {
            background_color: Some(Hsla::from(rgb(0x111111))),
            ..HighlightStyle::default()
        };
        let merged = with_selection(vec![(2..4, code)], Some(0..usize::MAX), 6, lit());
        let backgrounds: Vec<_> = merged.iter().map(|(r, s)| (r.clone(), s.background_color)).collect();
        assert_eq!(
            backgrounds,
            vec![(0..2, Some(lit())), (2..4, Some(lit())), (4..6, Some(lit()))]
        );
    }

    #[test]
    fn no_selection_keeps_the_highlights() {
        assert_eq!(with_selection(vec![(0..5, bold())], None, 5, lit()), vec![(0..5, bold())]);
        assert_eq!(with_selection(vec![(0..5, bold())], Some(9..12), 5, lit()), vec![(0..5, bold())]);
    }

    #[test]
    fn nearest_prefers_the_text_under_then_beside_the_pointer() {
        let rect = |y: f32| gpui::bounds(gpui::point(px(10.), px(y)), gpui::size(px(100.), px(20.)));
        let rects = [((0, 0), rect(0.)), ((1, 0), rect(40.)), ((2, 0), rect(80.))];
        assert_eq!(nearest(rects, gpui::point(px(50.), px(45.))), Some((1, 0)), "inside");
        assert_eq!(nearest(rects, gpui::point(px(500.), px(85.))), Some((2, 0)), "right of it");
        assert_eq!(nearest(rects, gpui::point(px(50.), px(28.))), Some((0, 0)), "gap: closer above");
        assert_eq!(nearest(rects, gpui::point(px(50.), px(-30.))), Some((0, 0)), "above the page");
        assert_eq!(nearest(Vec::<((usize, usize), _)>::new(), gpui::point(px(0.), px(0.))), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mkrew-ui preview::tests`
Expected: FAIL to compile (`with_selection`, `nearest` not defined).

- [ ] **Step 3: Pure helpers** — add above `pub fn page` in `preview.rs` (imports: add
`std::collections::HashMap`, `gpui::{Bounds, TextLayout, point}`;
`crate::preview_select::{self, Selection}`):

```rust
/// `highlights` (sorted, not overlapping) with the selection `sel` drawn on `bg`, in a text of
/// `len` bytes: highlights are cut at the selection's ends, their selected parts get `bg` (over
/// a code span's own background), selected plain text gets `bg` alone. `sel` may end past the
/// text. Hand-made because `gpui::combine_highlights` folds overlaps in `HashSet` order.
pub fn with_selection(
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    sel: Option<Range<usize>>,
    len: usize,
    bg: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let Some(sel) = sel
        .map(|sel| sel.start.min(len)..sel.end.min(len))
        .filter(|sel| !sel.is_empty())
    else {
        return highlights;
    };
    let lit = |style: HighlightStyle| HighlightStyle { background_color: Some(bg), ..style };
    let mut out = Vec::with_capacity(highlights.len() + 3);
    let mut at = 0;
    for (range, style) in highlights {
        let gap = at.max(sel.start)..range.start.min(sel.end);
        if gap.start < gap.end {
            out.push((gap, lit(HighlightStyle::default())));
        }
        let cuts = [
            range.start,
            sel.start.clamp(range.start, range.end),
            sel.end.clamp(range.start, range.end),
            range.end,
        ];
        for pair in cuts.windows(2).filter(|pair| pair[0] < pair[1]) {
            let inside = pair[0] >= sel.start && pair[1] <= sel.end;
            out.push((pair[0]..pair[1], if inside { lit(style) } else { style }));
        }
        at = range.end;
    }
    let tail = at.max(sel.start)..sel.end;
    if tail.start < tail.end {
        out.push((tail, lit(HighlightStyle::default())));
    }
    out
}

/// The key of the rectangle nearest to `at`: by vertical distance (0 inside its rows), then
/// horizontal, then key order.
pub fn nearest<K: Copy + Ord>(
    rects: impl IntoIterator<Item = (K, Bounds<Pixels>)>,
    at: gpui::Point<Pixels>,
) -> Option<K> {
    let gap = |from: Pixels, to: Pixels, x: Pixels| {
        let (from, to, x) = (f32::from(from), f32::from(to), f32::from(x));
        (from - x).max(x - to).max(0.0)
    };
    rects
        .into_iter()
        .map(|(key, b)| ((gap(b.top(), b.bottom(), at.y), gap(b.left(), b.right(), at.x)), key))
        .min_by(|(a, ka), (b, kb)| {
            a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)).then(ka.cmp(kb))
        })
        .map(|(_, key)| key)
}
```

Move `closest_byte` from `shell/widgets.rs:456-478` into `preview.rs` unchanged except
`pub fn` and `layout: &TextLayout`; in `widgets.rs` delete it and import
`mkrew_ui::preview::closest_byte` (`caret_field` keeps calling `closest_byte(layout, position)`).

- [ ] **Step 4: Run the pure tests**

Run: `cargo test -p mkrew-ui preview::tests`
Expected: PASS.

- [ ] **Step 5: Selection-aware rendering** — in `preview.rs`:

Hits and the page argument (`Page` derives `Debug`, `TextLayout` is not `Debug`):

```rust
/// The layouts of the pieces painted last frame, by (top-level block, piece); filled from
/// `canvas` paint callbacks, so every layout in it is prepainted (its methods panic before).
#[derive(Clone, Default)]
pub struct Hits(Rc<RefCell<HashMap<(usize, usize), TextLayout>>>);

impl std::fmt::Debug for Hits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Hits({})", self.0.borrow().len())
    }
}

/// Makes `page`'s text selectable, with this selection drawn (Files preview).
#[derive(Debug, Clone, Copy)]
pub struct Selecting(pub Option<Selection>);
```

Add `hits: Hits` to `Page` (and `hits: Hits::default()` in `Default`), plus in `impl Page`:

```rust
    /// The text point under window position `at`, or the nearest painted one; `None` when no
    /// text is painted.
    pub fn hit(&self, at: gpui::Point<Pixels>) -> Option<preview_select::Point> {
        let hits = self.hits.0.borrow();
        let key = nearest(hits.iter().map(|(key, layout)| (*key, layout.bounds())), at)?;
        let byte = closest_byte(hits.get(&key)?, at);
        Some(preview_select::Point { block: key.0, piece: key.1, byte })
    }

    /// Measures every block again: one block changed height (the block edit field).
    pub fn remeasure(&self) {
        self.state.remeasure();
    }
```

The per-block drawing context and the wrapper of every selectable text:

```rust
/// Drawing one top-level block of a selectable page: the next piece's number, the selection,
/// where painted pieces are recorded, the selection color.
struct Pick {
    block: usize,
    next: Cell<usize>,
    selection: Option<Selection>,
    hits: Hits,
    bg: Hsla,
}

/// A piece of text: `build` gets its selected bytes (end may be `usize::MAX`) and the selection
/// color. On a selectable page it is numbered, shows an I-beam, and its layout is recorded once
/// painted (a size-0 canvas after it, so it takes no room).
fn selectable(
    pick: Option<&Pick>,
    build: impl FnOnce(Option<Range<usize>>, Hsla) -> StyledText,
) -> AnyElement {
    let Some(pick) = pick else {
        return build(None, Hsla::default()).into_any_element();
    };
    let piece = pick.next.get();
    pick.next.set(piece + 1);
    let selected = pick.selection.and_then(|s| s.range_in(pick.block, piece));
    let styled = build(selected, pick.bg);
    let layout = styled.layout().clone();
    let (hits, key) = (pick.hits.clone(), (pick.block, piece));
    div()
        .cursor(CursorStyle::IBeam)
        .child(styled)
        .child(
            canvas(
                |_, _, _| (),
                move |_, (), _, _| {
                    hits.0.borrow_mut().insert(key, layout.clone());
                },
            )
            .absolute()
            .size_0(),
        )
        .into_any_element()
}
```

Thread `pick: Option<&Pick>` through the private renderers; keep the public wrappers:

```rust
pub fn blocks(blocks: &[Block], shown: &Shown, theme: &Theme) -> Vec<AnyElement> {
    blocks_in(blocks, shown, None, theme)
}

fn blocks_in(blocks: &[Block], shown: &Shown, pick: Option<&Pick>, theme: &Theme) -> Vec<AnyElement> {
    blocks
        .iter()
        .filter_map(|block| block_in(block, shown, pick, theme))
        .collect()
}

pub fn block(block: &Block, shown: &Shown, theme: &Theme) -> Option<AnyElement> {
    block_in(block, shown, None, theme)
}
```

Rename the old `pub fn block` body to `fn block_in(block, shown, pick: Option<&Pick>, theme)`
and change its text call sites:
- Heading: `.child(selectable(pick, |sel, bg| text(runs, theme, sel, bg)))`
- Paragraph: same.
- List: `list_item(item, number, shown, pick, theme)`; inside `list_item`,
  `.children(blocks_in(&item.blocks, shown, pick, theme))`.
- Quote: `.children(blocks_in(inner, shown, pick, theme))`.
- Code: `code_block(lang.as_deref(), highlighted, pick, theme)` (the `code(text, …)` fallback and
  `Html` stay unselectable, matching Task 1).
- Table: `table_row(head, aligns, pick, theme)` / `table_row(row, aligns, pick, theme)`; inside,
  `.child(selectable(pick, |sel, bg| text(cell, theme, sel, bg)))`.

`text` gains the selection:

```rust
fn text(runs: &[Run], theme: &Theme, selected: Option<Range<usize>>, bg: Hsla) -> StyledText {
    // … unchanged up to `let highlights = styled.into_iter().map(...)` …
    let len = text.len();
    let highlights = with_selection(highlights.collect(), selected, len, bg);
    StyledText::new(text)
        .with_highlights(highlights)
        .with_font_family_overrides(mono)
}
```

`code_line` keeps its signature; split its highlight building out so `code_block` can add the
selection:

```rust
fn code_highlights(
    spans: &[(Range<usize>, Token)],
    found: &[(Range<usize>, bool)],
    theme: &Theme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    // today's body of `code_line` up to and including `.collect()`
}

pub fn code_line(
    text: String,
    spans: &[(Range<usize>, Token)],
    found: &[(Range<usize>, bool)],
    theme: &Theme,
) -> StyledText {
    StyledText::new(text).with_highlights(code_highlights(spans, found, theme))
}
```

In `code_block`, each line becomes:

```rust
div().child(selectable(pick, |sel, bg| {
    let len = line.len();
    StyledText::new(line)
        .with_highlights(with_selection(code_highlights(spans, &[], theme), sel, len, bg))
}))
```

`page` gains `select: Option<Selecting>` after `wrap`. At its top:
`if select.is_some() { page.hits.0.borrow_mut().clear(); }` and
`let (hits, bg) = (page.hits.clone(), Hsla::from(rgb(theme.terminal_selection_bg)));`.
In the list closure replace the `self::block(...)` call with:

```rust
        let pick = select.map(|Selecting(selection)| Pick {
            block: index,
            next: Cell::new(0),
            selection,
            hits: hits.clone(),
            bg,
        });
        let element = block_in(block, &shown, pick.as_ref(), item_theme);
        if let Some(pick) = &pick {
            debug_assert_eq!(
                pick.next.get(),
                preview_select::piece_count(block, &shown),
                "preview_select::flat and preview::block disagree on block {index}'s pieces"
            );
        }
        let Some(element) = element else {
            return div().into_any_element();
        };
```

Update the two existing callers in `shell/docs.rs` (Docs `preview::page(.., Some(wrap), fonts, theme)`
and Files `render_md_preview`) to pass `None` for `select` (Files is wired in Task 4).

- [ ] **Step 6: Build and run the crate tests**

Run: `cargo test -p mkrew-ui` then `cargo clippy -p mkrew-ui --all-targets -- -D warnings`
Expected: PASS / clean.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/preview.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/shell/docs.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): draw a text selection in the Markdown preview"
```

---

### Task 3: Selection and block edit state in `FilesView`

**Files:**
- Modify: `crates/mkrew-ui/src/files_view.rs`
- Test: `crates/mkrew-ui/src/files_view/tests.rs`

**Interfaces:**
- Consumes: Task 1 (`preview_select::{self, Point, Selection}`), `crate::markdown::Lines`,
  `crate::text_box::TextBox`, `crate::task_dialog::DialogKey`, `crate::editor::Pos`.
- Produces (all on `FilesView`):
  - `pub preview_selection: Option<Selection>`, `pub block_edit: Option<BlockEdit>`
  - `pub struct BlockEdit { pub block: usize, pub lines: Lines, stamp: Stamp, pub text: TextBox }`
  - `pub enum BlockEditOutcome { Edited, Cancel, Apply }`
  - `pub struct StaleEdit;`
  - `pub fn preview_click(&mut self, at: Point, extend: bool, clicks: usize)`
  - `pub fn preview_drag(&mut self, at: Point)`
  - `pub fn preview_select_all(&mut self)`
  - `pub fn preview_copy(&self) -> Option<String>`
  - `pub fn clear_preview_selection(&mut self) -> bool`
  - `pub fn open_block_edit(&mut self, block: usize) -> bool`
  - `pub fn block_edit_key(&mut self, key: DialogKey) -> BlockEditOutcome`
  - `pub fn apply_block_edit(&mut self) -> Result<(), StaleEdit>`

- [ ] **Step 1: Write the failing tests** — append to `files_view/tests.rs`:

```rust
use crate::preview_select::Point;
use crate::task_dialog::DialogKey;

/// README.md opened with `text` and its preview built.
fn previewed(text: &str) -> FilesView {
    let mut view = opened(text);
    let (stamp, source) = view.preview_request().expect("a preview to build");
    view.preview_ready(stamp, page(&source));
    view
}

fn point(block: usize, byte: usize) -> Point {
    Point { block, piece: 0, byte }
}

#[test]
fn a_drag_in_the_preview_copies_the_rendered_text() {
    let mut view = previewed("# Title\n\nold para\n\nend\n");
    view.preview_click(point(1, 0), false, 1);
    view.preview_drag(point(1, 3));
    assert_eq!(view.preview_copy().as_deref(), Some("old"));
    view.preview_click(point(2, 3), true, 1);
    assert_eq!(view.preview_copy().as_deref(), Some("old para\nend"), "Shift extends");
    view.preview_click(point(1, 5), false, 2);
    assert_eq!(view.preview_copy().as_deref(), Some("para"), "double click: a word");
    view.preview_select_all();
    assert_eq!(view.preview_copy().as_deref(), Some("Title\nold para\nend"));
    assert!(view.clear_preview_selection());
    assert_eq!(view.preview_copy(), None);
}

#[test]
fn edit_keys_do_nothing_while_previewing() {
    let mut view = previewed("a\n");
    view.preview_click(point(0, 0), false, 1);
    view.preview_select_all();
    let _ = view.preview_copy();
    assert_eq!(text(&view), "a\n");
    assert_eq!(view.unsaved(), None);
}

#[test]
fn applying_a_block_edit_replaces_its_lines_in_one_undo_step() {
    let mut view = previewed("# T\n\nold para\n\nend\n");
    assert!(view.open_block_edit(1));
    assert_eq!(view.block_edit.as_ref().map(|edit| edit.text.text()), Some("old para"));
    view.block_edit_key(DialogKey::SelectAll);
    view.block_edit_key(DialogKey::Insert("new".into()));
    view.block_edit_key(DialogKey::Enter);
    view.block_edit_key(DialogKey::Insert("line".into()));
    assert_eq!(view.block_edit_key(DialogKey::Save), BlockEditOutcome::Apply);
    assert_eq!(view.apply_block_edit(), Ok(()));
    assert_eq!(text(&view), "# T\n\nnew\nline\n\nend\n");
    assert!(view.block_edit.is_none());
    assert_eq!(view.unsaved(), Some("README.md"));
    view.editor_mut().unwrap().key(EditKey::Undo, 10);
    assert_eq!(text(&view), "# T\n\nold para\n\nend\n");
}

#[test]
fn an_emptied_block_leaves_a_blank_line() {
    let mut view = previewed("a\n\nb\n\nc\n");
    assert!(view.open_block_edit(1));
    view.block_edit_key(DialogKey::SelectAll);
    view.block_edit_key(DialogKey::Backspace);
    assert_eq!(view.apply_block_edit(), Ok(()));
    assert_eq!(text(&view), "a\n\n\n\nc\n");
}

#[test]
fn a_block_edit_is_refused_once_the_text_changed() {
    let mut view = previewed("a\n\nb\n");
    assert!(view.open_block_edit(1));
    view.editor_mut().unwrap().insert("x");
    assert_eq!(view.apply_block_edit(), Err(StaleEdit));
    assert_eq!(text(&view), "xa\n\nb\n", "nothing written");
    assert!(view.block_edit.is_some(), "the typed text stays to copy");
    view.block_edit = None;
    assert!(!view.open_block_edit(0), "the page is of the old text");
}

#[test]
fn esc_cancels_and_leaving_the_preview_drops_edit_and_selection() {
    let mut view = previewed("a\n\nb\n");
    assert!(view.open_block_edit(0));
    assert_eq!(view.block_edit_key(DialogKey::Cancel), BlockEditOutcome::Cancel);
    view.preview_click(point(0, 0), false, 1);
    view.preview_drag(point(0, 1));
    view.toggle_mode();
    assert!(view.block_edit.is_none() && view.preview_selection.is_none());
    view.toggle_mode();
    assert!(view.open_block_edit(0));
    view.preview_select_all();
    view.open("src/main.rs");
    assert!(view.block_edit.is_none() && view.preview_selection.is_none());
}

#[test]
fn a_new_page_clears_the_selection() {
    let mut view = previewed("a\n");
    assert!(view.open_block_edit(0));
    assert_eq!(view.apply_block_edit(), Ok(()));
    view.preview_select_all();
    assert!(view.preview_selection.is_some(), "the old page is still shown");
    let (stamp, source) = view.preview_request().expect("the edited text is parsed");
    view.preview_ready(stamp, page(&source));
    assert_eq!(view.preview_selection, None);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p mkrew-ui files_view`
Expected: FAIL to compile.

- [ ] **Step 3: Implement** in `files_view.rs`:

Types (near `Stamp` / `MdPreview`):

```rust
/// The top-level block of the preview whose Markdown is edited in place (SPEC §12A, ADR 0072):
/// its source lines as of `stamp`, and the field's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockEdit {
    pub block: usize,
    pub lines: Lines,
    stamp: Stamp,
    pub text: TextBox,
}

/// What a key in the block edit field does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockEditOutcome {
    Edited,
    /// Esc: close it, the typed text is dropped.
    Cancel,
    /// Ctrl+Enter.
    Apply,
}

/// The text changed since the field opened: applying could hit the wrong lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaleEdit;
```

Fields on `FilesView` (after `preview_asked`), initialised to `None` in `new`:

```rust
    /// The preview's text selection; cleared with every new page.
    pub preview_selection: Option<Selection>,
    /// The block being edited in the preview, `None` = none.
    pub block_edit: Option<BlockEdit>,
```

Clearing: in `open` (next to `self.preview = None;`), in `toggle_mode` (after switching), and in
`preview_ready` (after `self.preview = Some(..)`, i.e. only when the page is accepted) add:

```rust
        self.preview_selection = None;
        self.block_edit = None;
```

Methods:

```rust
    /// A press on the preview at `at`: a caret, Shift extends, a double click takes the word.
    pub fn preview_click(&mut self, at: Point, extend: bool, clicks: usize) {
        let Some(shown) = self.preview().cloned() else {
            return;
        };
        self.preview_selection = Some(match self.preview_selection {
            _ if clicks == 2 => preview_select::word_at(&shown, at),
            Some(selection) if extend => Selection { head: at, ..selection },
            _ => Selection::caret(at),
        });
    }

    pub fn preview_drag(&mut self, at: Point) {
        if let Some(selection) = &mut self.preview_selection {
            selection.head = at;
        }
    }

    pub fn preview_select_all(&mut self) {
        self.preview_selection = self.preview().and_then(|shown| preview_select::select_all(shown));
    }

    /// Ctrl+C in the preview: the selected text as shown; `None` when nothing is selected.
    pub fn preview_copy(&self) -> Option<String> {
        let text = preview_select::copy_text(self.preview()?, self.preview_selection.as_ref()?);
        (!text.is_empty()).then_some(text)
    }

    /// Esc: `true` when there was a selection.
    pub fn clear_preview_selection(&mut self) -> bool {
        self.preview_selection.take().is_some()
    }

    /// ✎ on block `block`: its source lines go into the field. Refused while the page is of an
    /// older text (its lines may have moved) or the file cannot be edited.
    pub fn open_block_edit(&mut self, block: usize) -> bool {
        let Some(stamp) = self.stamp() else {
            return false;
        };
        let current = self.preview.as_ref().is_some_and(|preview| preview.stamp == stamp);
        let lines = self
            .preview()
            .and_then(|shown| shown.blocks.get(block))
            .map(|block| block.lines);
        let (Some(lines), true) = (lines, current) else {
            return false;
        };
        let Some(shown) = self.shown() else {
            return false;
        };
        let source = shown.editor.lines();
        let from = (lines.start as usize).saturating_sub(1);
        let to = (lines.end as usize).min(source.len());
        if from >= to {
            return false;
        }
        let mut text = TextBox::default();
        text.insert(&source[from..to].join("\n"));
        self.block_edit = Some(BlockEdit { block, lines, stamp, text });
        true
    }

    /// A key in the field. Clipboard keys are the shell's: it reads the selection before Copy /
    /// Cut and inserts the clipboard on Paste.
    pub fn block_edit_key(&mut self, key: DialogKey) -> BlockEditOutcome {
        let Some(edit) = &mut self.block_edit else {
            return BlockEditOutcome::Cancel;
        };
        match key {
            DialogKey::Cancel => return BlockEditOutcome::Cancel,
            DialogKey::Save => return BlockEditOutcome::Apply,
            DialogKey::Enter => edit.text.insert("\n"),
            DialogKey::Insert(text) => edit.text.insert(&text),
            DialogKey::Cut => {
                if edit.text.input().selection().is_some() {
                    edit.text.edit(&DialogKey::Backspace);
                }
            }
            DialogKey::Paste | DialogKey::Copy | DialogKey::SwitchField => {}
            key => {
                edit.text.edit(&key);
            }
        }
        BlockEditOutcome::Edited
    }

    /// Ctrl+Enter: the field's text replaces the block's lines as one undo step and the field
    /// closes (the caller rebuilds the page). The field stays when the text changed since it
    /// opened.
    pub fn apply_block_edit(&mut self) -> Result<(), StaleEdit> {
        let Some(edit) = self.block_edit.take() else {
            return Ok(());
        };
        if self.stamp() != Some(edit.stamp) {
            self.block_edit = Some(edit);
            return Err(StaleEdit);
        }
        let Some(editor) = self.editor_mut() else {
            return Ok(());
        };
        let from = (edit.lines.start as usize).saturating_sub(1);
        let last = (edit.lines.end as usize).min(editor.lines().len()).saturating_sub(1);
        let Some(end_col) = editor.lines().get(last).map(String::len) else {
            return Ok(());
        };
        editor.replace(
            Pos { line: from, col: 0 }..Pos { line: last, col: end_col },
            edit.text.text(),
        );
        self.preview_selection = None;
        Ok(())
    }
```

Imports at the top of `files_view.rs`: `crate::editor::Pos` (if missing),
`crate::markdown::Lines`, `crate::preview_select::{self, Point, Selection}`,
`crate::task_dialog::DialogKey`, `crate::text_box::TextBox`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p mkrew-ui files_view`
Expected: PASS (old and new tests).

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/files_view.rs crates/mkrew-ui/src/files_view/tests.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): preview selection and block edit state in the Files view"
```

---

### Task 4: Shell — select and copy in the Files preview

**Files:**
- Create: `crates/mkrew-ui/src/shell/files_preview.rs`
- Modify: `crates/mkrew-ui/src/shell.rs` (add `mod files_preview;` after `pub(crate) mod files_render;`)
- Modify: `crates/mkrew-ui/src/shell/docs.rs` (delete `render_md_preview`; it moves)
- Modify: `crates/mkrew-ui/src/shell/files_render.rs:612-614` (new call)

**Interfaces:**
- Consumes: Task 2 `preview::{page, Selecting}`, `Page::hit`; Task 3
  `FilesView::{preview_click, preview_drag, preview_select_all, preview_copy, clear_preview_selection, preview_selection}`.
- Produces: `Shell::render_md_preview(&self, project_id: Ulid, view: &FilesView, window: &mut Window, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement`, `Shell::file_preview_key_down`.

- [ ] **Step 1: Create `shell/files_preview.rs`**

```rust
//! The Files view's Markdown preview (SPEC §12A, ADR 0072): the page, text selection with copy,
//! and editing one block's Markdown in place.

use crate::*;

impl Shell {
    /// A `.md` file of the Files view as the Docs preview: the page of the editor's text
    /// (unsaved edits included), selectable.
    pub(crate) fn render_md_preview(
        &self,
        project_id: Ulid,
        view: &FilesView,
        _window: &mut Window,
        theme: &Theme,
        cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        let shown = match view.preview() {
            None => {
                return message(theme, None, "Rendering the preview…".into(), None)
                    .flex_1()
                    .into_any_element();
            }
            Some(shown) if shown.blocks.is_empty() => {
                return message(theme, None, "This file is empty.".into(), None)
                    .flex_1()
                    .into_any_element();
            }
            Some(shown) => shown,
        };
        let fonts = cx.global::<theme::Fonts>().clone();
        let select = Some(preview::Selecting(view.preview_selection));
        preview::page(&self.file_page, view.opening, shown, None, select, &fonts, theme)
            .track_focus(&self.file_editor_focus)
            .on_key_down(cx.listener(Self::file_preview_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |shell, event: &MouseDownEvent, window, cx| {
                    shell.preview_press(project_id, event, window, cx);
                }),
            )
            .on_mouse_move(cx.listener(move |shell, event: &MouseMoveEvent, _, cx| {
                if event.dragging() {
                    shell.preview_drag_to(project_id, event.position, cx);
                }
            }))
            .into_any_element()
    }

    /// A press on the page: the keyboard comes here (Ctrl+C, Ctrl+A, Esc), the caret goes to
    /// the text under the pointer (Shift extends, a double click takes the word).
    fn preview_press(
        &mut self,
        project_id: Ulid,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        window.focus(&self.file_editor_focus, cx);
        let at = self.file_page.hit(event.position);
        let view = self.files_view(project_id);
        match at {
            Some(at) => view.preview_click(at, event.modifiers.shift, event.click_count),
            None => {
                view.clear_preview_selection();
            }
        }
        cx.notify();
    }

    fn preview_drag_to(
        &mut self,
        project_id: Ulid,
        position: Point<Pixels>,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(at) = self.file_page.hit(position) {
            self.files_view(project_id).preview_drag(at);
            cx.notify();
        }
    }

    /// Keys of the preview: Ctrl+C copies the selection as shown, Ctrl+A selects the page, Esc
    /// clears it. Nothing edits here (the editor is hidden); other keys go on to the keymap.
    pub(crate) fn file_preview_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let Some(project_id) = self.workspace else {
            return;
        };
        let plain = !event.keystroke.modifiers.modified();
        if plain && event.keystroke.key == "escape" {
            if self.files_view(project_id).clear_preview_selection() {
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        match editor::edit_key(&event.keystroke) {
            Some(EditKey::Copy) => {
                cx.stop_propagation();
                if let Some(text) = self.files_view(project_id).preview_copy() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }
            Some(EditKey::SelectAll) => {
                cx.stop_propagation();
                self.files_view(project_id).preview_select_all();
                cx.notify();
            }
            _ => {}
        }
    }
}
```

Check while writing: `Point`, `Pixels`, `MouseMoveEvent`, `KeyDownEvent`, `ClipboardItem`,
`EditKey`, `editor`, `message`, `preview`, `theme` reach this file through `use crate::*` (see
how `files.rs` / `docs.rs` use them; add a `use` line for any that is missing).
`Modifiers::modified()` — confirm it exists with
`grep -n "fn modified" ~/.cargo/registry/src/*/gpui-pre-0.3.7/src/platform/keystroke.rs`;
if not, use `!m.control && !m.alt && !m.shift && !m.platform`.

- [ ] **Step 2: Wire it** — delete `render_md_preview` from `shell/docs.rs`; in
`files_render.rs` replace the `Content::Shown(_) if view.previewing()` arm body with
`self.render_md_preview(project_id, view, window, theme, cx)`. Add `mod files_preview;` to
`shell.rs`.

- [ ] **Step 3: Build**

Run: `cargo build -p mkrew-ui && cargo test -p mkrew-ui`
Expected: build OK, tests PASS.

- [ ] **Step 4: Smoke** — `cargo run -p mkrew-ui` (debug build: the piece-count `debug_assert`
is live). Open a `.md` with a heading, paragraph, table, list, code block and a ```mermaid block
in Files → drag across them: the selection color follows the pointer; Ctrl+C → paste into
Notepad shows the agreed text (table cells tab-separated); double-click selects a word; Ctrl+A,
Esc. No panic. A press on the Mermaid image puts the caret in the nearest text. The scroll bar
still drags without changing the selection.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/shell/files_preview.rs crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/docs.rs crates/mkrew-ui/src/shell/files_render.rs
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): select and copy text in the Files Markdown preview"
```

---

### Task 5: Shell — ✎ and the block edit field

**Files:**
- Modify: `crates/mkrew-ui/src/shell/files_preview.rs`
- Modify: `crates/mkrew-ui/src/shell/widgets.rs` (`caret_field_for`, `typing_input_for`)
- Modify: `crates/mkrew-ui/src/main.rs` (field `file_block_focus: FocusHandle` after `file_editor_focus`)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (`file_block_focus: cx.focus_handle(),`)
- Modify: `crates/mkrew-ui/src/icons.rs` (`Pencil => "pencil",` between `PanelRight` and `Play`)
- Create: `icon/lucide/pencil.svg`

**Interfaces:**
- Consumes: Task 3 `FilesView::{open_block_edit, block_edit_key, apply_block_edit, block_edit}`,
  `BlockEditOutcome`, `StaleEdit`; Task 2 `Page::remeasure`; existing `Shell::{file_edited,
  build_preview, toast_error}`, `task_dialog::dialog_key`.
- Produces: `widgets::caret_field_for(view: &Entity<Shell>, …)`, `widgets::typing_input_for(view: Entity<Shell>, …)`.

- [ ] **Step 1: Entity-based widget variants** (the `Wrap` closure has no `Context`). In
`widgets.rs`, make `typing_input` delegate:

```rust
pub(crate) fn typing_input(
    focus: &FocusHandle,
    on_key: ime::OnKey<Shell>,
    marked: &Rc<ime::Composing<FocusHandle>>,
    typing_target: &ime::FocusedField<Shell>,
    cx: &Context<'_, Shell>,
) -> impl IntoElement + use<> {
    typing_input_for(cx.entity(), focus, on_key, marked, typing_target)
}

/// [`typing_input`] from outside a `Context` (a closure holding the view).
pub(crate) fn typing_input_for(
    view: Entity<Shell>,
    focus: &FocusHandle,
    on_key: ime::OnKey<Shell>,
    marked: &Rc<ime::Composing<FocusHandle>>,
    typing_target: &ime::FocusedField<Shell>,
) -> impl IntoElement + use<> {
    // today's body of `typing_input`, with `view` from the argument instead of `cx.entity()`
}
```

Same for `caret_field`: move its body into
`caret_field_for(view: &Entity<Shell>, id, input, placeholder, focused, theme, on_caret)` where
each `cx.listener(move |shell, event, _, cx| { … })` becomes
`move |event, _, cx: &mut App| { view.update(cx, |shell, cx| { …same body… }) }` (clone `view`
into each closure); `caret_field` becomes `caret_field_for(&cx.entity(), …)`.

- [ ] **Step 2: Pencil icon** — `icon/lucide/pencil.svg` (lucide-static, ISC; same header as the
other files; compare the paths with https://lucide.dev/icons/pencil once):

```xml
<!-- @license lucide-static v1.55.0 - ISC -->
<svg
  class="lucide lucide-pencil"
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
  <path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" />
  <path d="m15 5 4 4" />
</svg>
```

and `Pencil => "pencil",` in the `Icon` list of `icons.rs`.

- [ ] **Step 3: The Files wrap, open / key / apply / close** — in `render_md_preview` pass
`Some(self.file_block_wrap(project_id, view, window, theme, cx))` as `wrap` (rename `_window` to
`window`), and add to `impl Shell` in `files_preview.rs`:

```rust
    /// Every top-level block gets ✎ on hover; the edited one is replaced by its field.
    fn file_block_wrap(
        &self,
        project_id: Ulid,
        view: &FilesView,
        window: &Window,
        theme: &Theme,
        cx: &Context<'_, Self>,
    ) -> preview::Wrap {
        let this = cx.entity();
        let theme = theme.clone();
        let edit = view.block_edit.as_ref().map(|edit| (edit.block, edit.text.input().clone()));
        let focused = self.file_block_focus.is_focused(window);
        let focus = self.file_block_focus.clone();
        let (marked, target) = (self.ime_marked.clone(), self.typing_target.clone());
        Rc::new(move |index, _block, element| {
            if let Some((_, input)) = edit.as_ref().filter(|(block, _)| *block == index) {
                return block_edit_field(
                    project_id, &this, &focus, input, focused, &marked, &target, &theme,
                )
                .into_any_element();
            }
            let group = SharedString::from(format!("file-block-{index}"));
            let this = this.clone();
            div()
                .id(("file-block", index))
                .group(group.clone())
                .relative()
                .child(element)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .right(px(-space::XL))
                        .opacity(opacity::HIDDEN)
                        .group_hover(group, |style| style.opacity(opacity::FULL))
                        .child(
                            icon_button(("file-block-edit", index), Icon::Pencil, &theme)
                                .tooltip(tooltip(&theme, "Edit this block's Markdown", ""))
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    this.update(cx, |shell, cx| {
                                        shell.open_block_edit(project_id, index, window, cx);
                                    });
                                }),
                        ),
                )
                .into_any_element()
        })
    }

    fn open_block_edit(
        &mut self,
        project_id: Ulid,
        block: usize,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.files_view(project_id).open_block_edit(block) {
            self.file_page.remeasure();
            window.focus(&self.file_block_focus, cx);
        }
        cx.notify();
    }

    fn close_block_edit(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        if self.files_view(project_id).block_edit.take().is_some() {
            self.file_page.remeasure();
            window.focus(&self.file_editor_focus, cx);
        }
        cx.notify();
    }

    fn apply_block_edit(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        match self.files_view(project_id).apply_block_edit() {
            Ok(()) => {
                self.file_page.remeasure();
                window.focus(&self.file_editor_focus, cx);
                self.file_edited(project_id, window, cx);
                self.build_preview(project_id, window, cx);
            }
            Err(StaleEdit) => self.toast_error("The file changed; edit again", cx),
        }
        cx.notify();
    }

    fn click_block_edit(&mut self, project_id: Ulid, caret: Caret) {
        if let Some(edit) = &mut self.files_view(project_id).block_edit {
            edit.text.click(caret.at, caret.extend, caret.clicks);
        }
    }

    /// Keys of the block edit field: Enter is a new line, Ctrl+Enter applies, Esc cancels;
    /// Ctrl+C / X / V use the clipboard.
    pub(crate) fn file_block_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        cx.stop_propagation();
        let (Some(key), Some(project_id)) =
            (task_dialog::dialog_key(&event.keystroke), self.workspace)
        else {
            return;
        };
        let pasted = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let view = self.files_view(project_id);
        if matches!(key, DialogKey::Copy | DialogKey::Cut)
            && let Some(text) = view
                .block_edit
                .as_ref()
                .and_then(|edit| edit.text.input().selected_text().map(str::to_owned))
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if let (Some(text), Some(edit)) = (pasted, &mut view.block_edit) {
            edit.text.insert(&text);
        }
        match view.block_edit_key(key) {
            BlockEditOutcome::Edited => cx.notify(),
            BlockEditOutcome::Cancel => self.close_block_edit(project_id, window, cx),
            BlockEditOutcome::Apply => self.apply_block_edit(project_id, window, cx),
        }
    }
```

And the field (free fn in the same file):

```rust
/// The edited block: its Markdown in a caret field, with how to apply / cancel under it.
/// Presses and drags inside stay here (the page would take the keyboard and move its own
/// selection); a press outside cancels.
#[allow(clippy::too_many_arguments)]
fn block_edit_field(
    project_id: Ulid,
    this: &Entity<Shell>,
    focus: &FocusHandle,
    input: &Input,
    focused: bool,
    marked: &Rc<ime::Composing<FocusHandle>>,
    target: &ime::FocusedField<Shell>,
    theme: &Theme,
) -> gpui::Div {
    let (keys, out, grab) = (this.clone(), this.clone(), focus.clone());
    let field = caret_field_for(
        this,
        "file-block-field",
        input,
        "Markdown…",
        focused,
        theme,
        move |shell, caret| shell.click_block_edit(project_id, caret),
    )
    .max_h(px(theme::PROMPT_FIELD_HEIGHT))
    .overflow_y_scroll()
    .track_focus(focus)
    .on_key_down(move |event, window, cx| {
        keys.update(cx, |shell, cx| shell.file_block_key_down(event, window, cx));
    })
    .child(typing_input_for(this.clone(), focus, Shell::file_block_key_down, marked, target));
    div()
        .flex()
        .flex_col()
        .gap(px(space::XS))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&grab, cx);
            cx.stop_propagation();
        })
        .on_mouse_move(|_, _, cx| cx.stop_propagation())
        .on_mouse_down_out(move |_, window, cx| {
            out.update(cx, |shell, cx| shell.close_block_edit(project_id, window, cx));
        })
        .child(field)
        .child(
            div()
                .text_size(font_size::META)
                .text_color(rgb(theme.text_muted))
                .child("Ctrl+Enter apply · Esc cancel"),
        )
}
```

Notes for the implementer: `Input` is `mkrew_ui::text_box::Input`; `Caret` is the widgets
struct; `BlockEditOutcome`, `StaleEdit` come from `mkrew_ui::files_view`; `ime`, `Icon`,
`icon_button`, `tooltip`, `opacity`, `space`, `font_size`, `DialogKey`, `task_dialog` reach the
shell through `use crate::*` like in `cards.rs` / `board.rs` — mirror their imports for any
missing name. `right(px(-space::XL))` puts ✎ in the page's side padding (`page` pads each block
by `space::XL`); if it clips, use `right_0()` and note it in HUMAN_CHECKS.

- [ ] **Step 4: Focus handle** — `main.rs`: `file_block_focus: FocusHandle,` with doc comment
`/// The block edit field of the Files Markdown preview (ADR 0072).`; `connection.rs`:
`file_block_focus: cx.focus_handle(),` next to `file_editor_focus`.

- [ ] **Step 5: Build + tests**

Run: `cargo build -p mkrew-ui && cargo test -p mkrew-ui`
Expected: OK / PASS.

- [ ] **Step 6: Smoke** (`cargo run -p mkrew-ui`, dark and light): hover a paragraph → ✎ shows;
click → the block turns into a field with its Markdown, caret visible, Vietnamese typing via
Unikey works; a click mid-text moves the caret and keeps the keyboard in the field; Enter adds a
line; Ctrl+Enter → preview shows the change, file ● unsaved, Ctrl+S saves; Source → Ctrl+Z
undoes it in one step. Esc and a click outside cancel. Edit a table and a list (whole block).
✎ on another block switches. Blocks below never overlap the field while opening / closing it or
while it grows.

- [ ] **Step 7: Commit**

```bash
git add crates/mkrew-ui/src/shell/files_preview.rs crates/mkrew-ui/src/shell/widgets.rs crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs crates/mkrew-ui/src/icons.rs icon/lucide/pencil.svg
git -c user.email=tungnv975@gmail.com commit -m "feat(ui): edit a Markdown block in place from the Files preview"
```

---

### Task 6: ADR, SPEC, HUMAN_CHECKS, full verify

**Files:**
- Create: `docs/decisions/0072-files-preview-select-copy-block-edit.md`
- Modify: `docs/SPEC.md` (§12A "Markdown preview" bullet, §12 "Comments" bullet)
- Modify: `docs/HUMAN_CHECKS.md` (new section at the top of "To verify (non-blocking)")

- [ ] **Step 1: ADR 0072** (copy the headings of `docs/decisions/0071-in-app-update.md`):
Context (§12A said editing only in Source; the user wants to copy rendered text and fix a passage
without leaving the preview), Decision (selection model over flattened pieces; layouts recorded
from paint callbacks; hand-made highlight merge; per-top-level-block edit with a stamp check;
Files only), Consequences (Docs unchanged; nested lists copy without indentation; no edge
autoscroll while dragging; an emptied block leaves a blank line; one more focus handle; a
Lucide pencil icon).

- [ ] **Step 2: SPEC §12A** — replace in the "Markdown preview" bullet
`toggle; Source is the editor above, and editing only happens there.` with:

```
toggle; Source is the editor above. In Preview (ADR 0072) the text can be selected with the
mouse across blocks (Shift+click extends, double-click takes a word, Ctrl+A selects the page,
Esc clears) and Ctrl+C copies it as shown (blocks and lines on their own lines, table cells
tab-separated, list markers `- ` / `N. ` / `[x] `; drawn diagrams copy nothing). Hovering a
top-level block shows ✎: its Markdown opens in a field in its place; Ctrl+Enter puts it back
over the block's source lines (one undo step, unsaved like any edit), Esc or a click outside
cancels; refused with a toast when the text changed since the field opened.
```

§12 Comments bullet: change `(ADR 0006; character selection and anchor-aligned cards are backlog)`
to `(ADR 0006; character selection in Docs and anchor-aligned cards are backlog; the Files
preview selects characters, ADR 0072)`.

- [ ] **Step 3: HUMAN_CHECKS** — add as the first section under "## To verify (non-blocking)":

```markdown
### To verify — 2026-10-23 Select, copy and edit in the Files Markdown preview (ADR 0072)

- [ ] **Select (dark + light):** Files → a `.md` with heading, paragraph, table, list, code and
  a ```mermaid block. Drag across them → the selection color follows the pointer, readable in
  both themes, over code spans too. Shift+click extends; double-click selects a word (also in
  Vietnamese text); Ctrl+A selects all; Esc clears. Typing letters in Preview changes nothing.
- [ ] **Copy:** Ctrl+C, paste into Notepad → plain text, table cells separated by tabs, list
  items with `- ` / `1. ` / `[x] `, no Mermaid source.
- [ ] **Edit (dark + light):** hover a block → ✎ at its right; click → its Markdown in a field
  (caret, Unikey typing, click mid-text, drag to select, Ctrl+C / X / V). A click inside the
  field keeps typing there. Ctrl+Enter → the preview updates, the file shows unsaved, Ctrl+S
  saves; Source → Ctrl+Z undoes the whole edit. Esc / click outside cancels. Blocks below never
  overlap the field while it grows.
- [ ] **Docs view unchanged:** Docs preview still picks blocks for comments; Ask AI and the
  thread panel render Markdown as before.
```

- [ ] **Step 4: Full verify**

Run: `"C:/Program Files/Git/bin/bash.exe" scripts/verify.sh`
Expected: fmt check, clippy `-D warnings` and all tests pass. Fix and re-run until green.

- [ ] **Step 5: Commit**

```bash
git add docs/decisions/0072-files-preview-select-copy-block-edit.md docs/SPEC.md docs/HUMAN_CHECKS.md
git -c user.email=tungnv975@gmail.com commit -m "docs: ADR 0072, SPEC §12A preview select/copy/edit, human checks"
```

- [ ] **Step 6: Report** — list for the user the paths left uncommitted: the spec
(`docs/superpowers/specs/2026-10-23-md-preview-select-edit-design.md`), this plan if not
committed, and `Cargo.lock` if it changed.
