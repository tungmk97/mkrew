//! Grid-dump snapshot tests: ANSI fixtures in, `insta` text snapshots out (SPEC §16).

// Test-only helpers may panic on bad fixtures; clippy's in-tests allowance skips non-#[test] fns.
#![allow(clippy::expect_used)]

use std::fmt::Write as _;

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, NamedColor};
use spike_term::emulator::{Emulator, Grid, GridCell};

/// Decodes the escaped fixture format described in `tests/fixtures/README.md`.
fn decode(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\r' | '\n' => {}
            '\\' => match chars.next() {
                Some('e') => out.push(0x1b),
                Some('r') => out.push(b'\r'),
                Some('n') => out.push(b'\n'),
                Some('\\') => out.push(b'\\'),
                Some('x') => {
                    let hex: String = chars.by_ref().take(2).collect();
                    out.push(u8::from_str_radix(&hex, 16).expect("bad \\x escape"));
                }
                other => panic!("unknown escape \\{other:?}"),
            },
            c => out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes()),
        }
    }
    out
}

/// Flags that only describe wide-char layout; they are visible in the text dump already.
const LAYOUT_FLAGS: Flags = Flags::WIDE_CHAR
    .union(Flags::WIDE_CHAR_SPACER)
    .union(Flags::LEADING_WIDE_CHAR_SPACER);

fn is_default_style(cell: &GridCell) -> bool {
    cell.fg == Color::Named(NamedColor::Foreground)
        && cell.bg == Color::Named(NamedColor::Background)
        && cell.flags.difference(LAYOUT_FLAGS).is_empty()
}

/// Text dump, cursor, every run of non-default style as `row:start..end fg bg flags`, then the
/// columns of double-width chars per row.
fn dump(grid: &Grid) -> String {
    let mut out = grid.to_text();
    let cursor = grid.cursor;
    let _ = writeln!(
        out,
        "--- cursor: row {} col {} {}",
        cursor.row,
        cursor.col,
        if cursor.visible { "visible" } else { "hidden" }
    );
    out.push_str("--- styles\n");
    for (r, row) in grid.rows.iter().enumerate() {
        let mut start = 0;
        while start < row.len() {
            let style = &row[start];
            let len = row[start..]
                .iter()
                .take_while(|cell| {
                    cell.fg == style.fg
                        && cell.bg == style.bg
                        && cell.flags.difference(LAYOUT_FLAGS)
                            == style.flags.difference(LAYOUT_FLAGS)
                })
                .count();
            if !is_default_style(style) {
                let _ = writeln!(
                    out,
                    "{r}:{start}..{} fg={:?} bg={:?} flags={:?}",
                    start + len,
                    style.fg,
                    style.bg,
                    style.flags.difference(LAYOUT_FLAGS)
                );
            }
            start += len;
        }
    }
    out.push_str("--- wide chars (row: cols)\n");
    for (r, row) in grid.rows.iter().enumerate() {
        let cols: Vec<usize> = (0..row.len())
            .filter(|&c| row[c].flags.contains(Flags::WIDE_CHAR))
            .collect();
        if !cols.is_empty() {
            let _ = writeln!(out, "{r}: {cols:?}");
        }
    }
    out
}

fn snapshot(fixture: &str, cols: usize, rows: usize) -> String {
    let path = format!(
        "{}/tests/fixtures/{fixture}.ans",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).expect("read fixture");
    let mut emu = Emulator::new(cols, rows);
    emu.advance(&decode(&text));
    dump(&emu.grid())
}

#[test]
fn decode_handles_escapes_and_ignores_line_breaks() {
    assert_eq!(
        decode("a\\e[1m\r\nb\\r\\n\\\\\\x41é"),
        b"a\x1b[1mb\r\n\\A\xc3\xa9"
    );
}

#[test]
fn colors() {
    insta::assert_snapshot!(snapshot("colors", 40, 5));
}

#[test]
fn cursor_moves() {
    insta::assert_snapshot!(snapshot("cursor_moves", 20, 6));
}

#[test]
fn alt_screen_enter() {
    insta::assert_snapshot!(snapshot("alt_screen_enter", 30, 10));
}

#[test]
fn alt_screen_exit() {
    insta::assert_snapshot!(snapshot("alt_screen_exit", 30, 10));
}

#[test]
fn wide_emoji() {
    insta::assert_snapshot!(snapshot("wide_emoji", 24, 6));
}

#[test]
fn tui_frame() {
    insta::assert_snapshot!(snapshot("tui_frame", 60, 16));
}
