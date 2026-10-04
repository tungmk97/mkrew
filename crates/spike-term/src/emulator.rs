//! Headless terminal emulator: bytes in, `Grid` snapshot out (SPEC §8, §10).

use std::sync::{Arc, Mutex};

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::Config;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, StdSyncHandler};

/// Small scrollback, as the daemon will use (SPEC §8).
const SCROLLBACK_LINES: usize = 200;

/// Collects bytes the terminal wants to send back to the program (e.g. DSR replies).
/// ConPTY asks for the cursor position on startup and stalls until it gets an answer.
#[derive(Clone, Default)]
struct ReplyListener {
    replies: Arc<Mutex<Vec<u8>>>,
}

impl EventListener for ReplyListener {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event
            && let Ok(mut replies) = self.replies.lock()
        {
            replies.extend_from_slice(text.as_bytes());
        }
    }
}

/// One cell of a [`Grid`] snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct GridCell {
    pub c: char,
    /// Zero-width chars attached to `c` (combining marks, VS16, ZWJ sequences).
    pub zerowidth: Vec<char>,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}

/// Cursor position in screen coordinates (row 0 = top visible line).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub visible: bool,
}

/// Snapshot of the visible screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Grid {
    pub cols: usize,
    pub rows: Vec<Vec<GridCell>>,
    pub cursor: Cursor,
}

impl Grid {
    /// Plain-text dump: one line per row, trailing blanks trimmed, wide-char spacers skipped,
    /// zero-width chars kept after their base char.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            let line: String = row
                .iter()
                .filter(|cell| {
                    !cell
                        .flags
                        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                })
                .flat_map(|cell| std::iter::once(cell.c).chain(cell.zerowidth.iter().copied()))
                .collect();
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }
}

/// Terminal state machine fed with PTY output.
pub struct Emulator {
    term: Term<ReplyListener>,
    parser: Processor<StdSyncHandler>,
    listener: ReplyListener,
}

impl Emulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        let listener = ReplyListener::default();
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Config::default()
        };
        let term = Term::new(config, &TermSize::new(cols, rows), listener.clone());
        Self {
            term,
            parser: Processor::new(),
            listener,
        }
    }

    /// Feeds output bytes; returns bytes that must be written back to the PTY.
    pub fn advance(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.advance(&mut self.term, bytes);
        match self.listener.replies.lock() {
            Ok(mut replies) => std::mem::take(&mut *replies),
            Err(_) => Vec::new(),
        }
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.term.resize(TermSize::new(cols, rows));
    }

    pub fn grid(&self) -> Grid {
        let cols = self.term.columns();
        let blank = GridCell {
            c: ' ',
            zerowidth: Vec::new(),
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
        };
        let mut rows = vec![vec![blank; cols]; self.term.screen_lines()];
        let content = self.term.renderable_content();
        let offset = content.display_offset as i32;
        for indexed in content.display_iter {
            let row = (indexed.point.line.0 + offset) as usize;
            let col = indexed.point.column.0;
            if let Some(slot) = rows.get_mut(row).and_then(|r| r.get_mut(col)) {
                *slot = GridCell {
                    c: indexed.cell.c,
                    zerowidth: indexed
                        .cell
                        .zerowidth()
                        .map(<[char]>::to_vec)
                        .unwrap_or_default(),
                    fg: indexed.cell.fg,
                    bg: indexed.cell.bg,
                    flags: indexed.cell.flags,
                };
            }
        }
        let cursor = Cursor {
            row: (content.cursor.point.line.0 + offset).max(0) as usize,
            col: content.cursor.point.column.0,
            visible: content.cursor.shape != CursorShape::Hidden,
        };
        Grid { cols, rows, cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::vte::ansi::Rgb;

    #[test]
    fn plain_text_and_cursor() {
        let mut emu = Emulator::new(10, 3);
        emu.advance(b"hi\r\nthere");
        let grid = emu.grid();
        assert_eq!(grid.to_text(), "hi\nthere\n\n");
        assert_eq!(
            grid.cursor,
            Cursor {
                row: 1,
                col: 5,
                visible: true
            }
        );
    }

    #[test]
    fn colors_are_captured() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"\x1b[31;44mA\x1b[38;2;1;2;3mB\x1b[0mC");
        let row = &emu.grid().rows[0];
        assert_eq!(row[0].fg, Color::Named(NamedColor::Red));
        assert_eq!(row[0].bg, Color::Named(NamedColor::Blue));
        assert_eq!(row[1].fg, Color::Spec(Rgb { r: 1, g: 2, b: 3 }));
        assert_eq!(row[2].fg, Color::Named(NamedColor::Foreground));
    }

    #[test]
    fn cursor_moves_and_hides() {
        let mut emu = Emulator::new(10, 5);
        emu.advance(b"\x1b[3;4H\x1b[?25l");
        let cursor = emu.grid().cursor;
        assert_eq!((cursor.row, cursor.col, cursor.visible), (2, 3, false));
    }

    #[test]
    fn wide_chars_dump_once() {
        let mut emu = Emulator::new(10, 1);
        emu.advance("a日b".as_bytes());
        let grid = emu.grid();
        assert!(grid.rows[0][1].flags.contains(Flags::WIDE_CHAR));
        assert_eq!(grid.to_text(), "a日b\n");
    }

    #[test]
    fn cursor_position_request_is_answered() {
        let mut emu = Emulator::new(10, 5);
        let reply = emu.advance(b"\x1b[2;3H\x1b[6n");
        assert_eq!(reply, b"\x1b[2;3R");
        assert!(emu.advance(b"x").is_empty());
    }

    #[test]
    fn resize_changes_dimensions() {
        let mut emu = Emulator::new(10, 5);
        emu.resize(20, 3);
        let grid = emu.grid();
        assert_eq!((grid.cols, grid.rows.len()), (20, 3));
    }
}
