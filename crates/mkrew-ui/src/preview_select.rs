//! Text selection in the Files Markdown preview (SPEC §12A, ADR 0072): each top-level block
//! flattened into the text it shows, with its *pieces* (one per drawn text element, in the
//! order `preview::block` draws them) located in it, and a selection between two points.
//! Copy gives the rendered text. Pure and tested headlessly; `preview.rs` draws it.

use std::ops::Range;

use crate::docs::Shown;
use crate::markdown::{Block, BlockKind, Item, Lines};
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
        Selection {
            anchor: at,
            head: at,
        }
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
        let from = if key == (start.block, start.piece) {
            start.byte
        } else {
            0
        };
        let to = if key == (end.block, end.piece) {
            end.byte
        } else {
            usize::MAX
        };
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
                flat.text
                    .push_str(&prefix(item, start.map(|first| first + index as u64)));
                push_blocks(flat, &item.blocks, shown);
            }
        }
        BlockKind::Quote { blocks, .. } => push_blocks(flat, blocks, shown),
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
        BlockKind::Rule | BlockKind::Html(_) | BlockKind::FrontMatter(_) => {}
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
        flat.pieces.extend(
            child
                .pieces
                .into_iter()
                .map(|r| r.start + base..r.end + base),
        );
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
    // Ctrl+A is the only selection that keeps the list markers.
    if select_all(shown) == Some(*selection) {
        let texts: Vec<String> = (shown.blocks.iter())
            .map(|block| flat(block, shown))
            .filter(|flat| !flat.pieces.is_empty())
            .map(|flat| flat.text)
            .collect();
        return texts.join("\n");
    }
    let parts: Vec<String> = touched(shown, selection)
        .into_iter()
        .map(|(_, text)| text)
        .collect();
    parts.join("\n")
}

/// The blocks that have selected text, in page order, each with its index and that text.
fn touched(shown: &Shown, selection: &Selection) -> Vec<(usize, String)> {
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
        let from = if index == start.block {
            flat.offset(start.piece, start.byte)
        } else {
            0
        };
        let to = if index == end.block {
            flat.offset(end.piece, end.byte)
        } else {
            flat.text.len()
        };
        if let Some(text) = flat.text.get(from..to).filter(|text| !text.is_empty()) {
            parts.push((index, text.to_owned()));
        }
    }
    parts
}

/// The source lines of the blocks `selection` touches (a block with no selected text, such as
/// a diagram, is not touched): from the first block's first line to the last block's last
/// line. "Send selection" sends the Markdown of these lines (ADR 0103). `None` when nothing
/// is selected.
pub fn source_lines(shown: &Shown, selection: &Selection) -> Option<Lines> {
    if selection.is_empty() {
        return None;
    }
    let mut blocks = touched(shown, selection)
        .into_iter()
        .filter_map(|(index, _)| shown.blocks.get(index))
        .map(|block| block.lines);
    let first = blocks.next()?;
    let last = blocks.next_back().unwrap_or(first);
    Some(Lines {
        start: first.start.min(last.start),
        end: first.end.max(last.end),
    })
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
        anchor: Point {
            block: first,
            piece: 0,
            byte: 0,
        },
        head: Point {
            block: last,
            piece,
            byte: flat.pieces[piece].len(),
        },
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
        anchor: Point {
            byte: range.start,
            ..at
        },
        head: Point {
            byte: range.end,
            ..at
        },
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
        let selection = Selection {
            anchor: at(0, 0, 6),
            head: at(2, 1, 3),
        };
        assert_eq!(
            copy_text(&page, &selection),
            "bold\nA\tB\n1\t2\n- one\n[x] two"
        );
    }

    #[test]
    fn a_backwards_drag_copies_the_same_text() {
        let page = shown("first line\n\nsecond line\n");
        let forward = Selection {
            anchor: at(0, 0, 6),
            head: at(1, 0, 6),
        };
        let backward = Selection {
            anchor: forward.head,
            head: forward.anchor,
        };
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
        // "Tiếng " is 8 bytes (ế is 3); a head past the text clamps to its end.
        let selection = Selection {
            anchor: at(0, 0, 8),
            head: at(0, 0, 999),
        };
        assert_eq!(copy_text(&page, &selection), "Việt có dấu");
    }

    /// A regression pin, not a red-first test: ADR 0105's code card (language label, Copy) and
    /// path pills draw no piece of their own, so a paragraph is one piece and a highlighted code
    /// block one piece per line, as before. The real guard is the `debug_assert_eq!` in
    /// `preview::message` and `preview::page`, which runs whenever the pieces are drawn.
    #[test]
    fn pills_and_code_cards_add_no_pieces() {
        let page =
            shown("Open `src/a.rs:3` and `v1.2` now.\n\n```rust\nfn a() {}\nfn b() {}\n```\n");
        assert_eq!(piece_count(&page.blocks[0], &page), 1);
        assert_eq!(piece_count(&page.blocks[1], &page), 2);
        let untagged = shown("```\nplain\n```\n");
        assert_eq!(piece_count(&untagged.blocks[0], &untagged), 1);
    }

    #[test]
    fn an_empty_selection_copies_nothing() {
        let page = shown("abc\n");
        assert_eq!(copy_text(&page, &Selection::caret(at(0, 0, 1))), "");
    }

    #[test]
    fn range_in_covers_the_pieces_between_the_ends() {
        let selection = Selection {
            anchor: at(2, 1, 4),
            head: at(0, 3, 2),
        };
        assert_eq!(selection.range_in(0, 2), None, "before the start");
        assert_eq!(selection.range_in(0, 3), Some(2..usize::MAX));
        assert_eq!(selection.range_in(1, 0), Some(0..usize::MAX));
        assert_eq!(selection.range_in(2, 1), Some(0..4));
        assert_eq!(selection.range_in(2, 2), None, "after the end");
        assert_eq!(Selection::caret(at(1, 0, 1)).range_in(1, 0), None);
    }

    #[test]
    fn list_markers_are_copied_only_by_select_all() {
        let page = shown("- one\n- two\n");
        let word = word_at(&page, at(0, 0, 1));
        assert_eq!(copy_text(&page, &word), "one");
        let drag = Selection {
            anchor: at(0, 0, 0),
            head: at(0, 0, 2),
        };
        assert_eq!(copy_text(&page, &drag), "on");
        assert_eq!(all(&page), "- one\n- two");
    }

    #[test]
    fn double_click_selects_the_word_under_the_pointer() {
        let page = shown("Tiếng Việt có\n");
        // Byte 9 is inside "Việt" (bytes 8..14: ệ is 3 bytes).
        let word = word_at(&page, at(0, 0, 9));
        assert_eq!((word.anchor.byte, word.head.byte), (8, 14));
        assert_eq!(copy_text(&page, &word), "Việt");
    }

    #[test]
    fn select_all_spans_the_first_to_the_last_text() {
        let page = shown("***\n\nonly\n\n***\n");
        let selection = select_all(&page).unwrap();
        assert_eq!(
            (selection.anchor, selection.head),
            (at(1, 0, 0), at(1, 0, 4))
        );
        assert_eq!(select_all(&shown("***\n")), None);
    }

    const PAGE: &str =
        "# Title\n\nfirst para\n\nsecond para\n\n```mermaid\ngraph TD; A-->B\n```\n\nlast\n";

    #[test]
    fn source_lines_are_the_union_of_the_blocks_touched() {
        let page = shown(PAGE);
        let lines = |anchor: Point, head: Point| {
            source_lines(&page, &Selection { anchor, head }).map(|l| (l.start, l.end))
        };
        // Inside one block: its whole range.
        assert_eq!(lines(at(1, 0, 2), at(1, 0, 5)), Some((3, 3)));
        // Across two blocks.
        assert_eq!(lines(at(1, 0, 2), at(2, 0, 3)), Some((3, 5)));
        // A backwards drag is the same.
        assert_eq!(lines(at(2, 0, 3), at(1, 0, 2)), Some((3, 5)));
        // Ending on the first byte of a block selects nothing of it.
        assert_eq!(lines(at(1, 0, 2), at(2, 0, 0)), Some((3, 3)));
        // A diagram shows no text: it neither counts nor splits the selection.
        assert_eq!(lines(at(2, 0, 0), at(4, 0, 4)), Some((5, 11)));
        // Nothing selected.
        assert_eq!(lines(at(1, 0, 2), at(1, 0, 2)), None);
        assert_eq!(
            source_lines(&page, &select_all(&page).unwrap()).map(|l| (l.start, l.end)),
            Some((1, 11))
        );
    }

    #[test]
    fn source_lines_count_after_front_matter_and_with_crlf_endings() {
        let page = shown("---\r\ntitle: x\r\n---\r\n\r\nfirst para\r\n\r\nsecond para\r\n");
        let lines = |anchor: Point, head: Point| {
            source_lines(&page, &Selection { anchor, head }).map(|l| (l.start, l.end))
        };
        // Block 0 is the front matter; the paragraphs follow it on lines 5 and 7.
        assert_eq!(lines(at(1, 0, 2), at(1, 0, 5)), Some((5, 5)));
        assert_eq!(lines(at(1, 0, 2), at(2, 0, 3)), Some((5, 7)));
        assert_eq!(
            crate::docs::source_text(&page.lines, 5, 7).as_deref(),
            Some("first para\n\nsecond para"),
            "the Markdown source, without the carriage returns"
        );
    }
}
