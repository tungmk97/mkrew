//! UI-side terminal emulator: daemon snapshot + output bytes in, `Grid` out (SPEC §7, §10).
//!
//! The daemon owns the session and already answers terminal queries (DSR etc.), so this
//! emulator never writes back to the PTY.

use alacritty_terminal::Term;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, StdSyncHandler};
use mkrew_protocol::ScreenSnapshot;
use mkrew_protocol::paint::paint;

use crate::terminal::input::InputMode;

/// UI-side scrollback for attached panes (SPEC §10).
const SCROLLBACK_LINES: usize = 10_000;

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

/// Terminal state machine fed with a session's output.
pub struct Emulator {
    term: Term<VoidListener>,
    parser: Processor<StdSyncHandler>,
}

impl Emulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &TermSize::new(cols, rows), VoidListener),
            parser: Processor::new(),
        }
    }

    /// A fresh emulator showing `snapshot` (screen, cursor, modes); output frames that follow
    /// the snapshot continue from it.
    pub fn from_snapshot(snapshot: &ScreenSnapshot) -> Self {
        let mut emulator = Self::new(
            usize::from(snapshot.cols.max(1)),
            usize::from(snapshot.rows.max(1)),
        );
        emulator.advance(paint(snapshot).as_bytes());
        emulator
    }

    /// (cols, rows) without building a grid snapshot.
    pub fn size(&self) -> (usize, usize) {
        (self.term.columns(), self.term.screen_lines())
    }

    pub fn advance(&mut self, bytes: &[u8]) {
        self.parser.advance(&mut self.term, bytes);
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.term.resize(TermSize::new(cols, rows));
    }

    /// Modes set by the program that change how keys and pastes are encoded.
    pub fn input_mode(&self) -> InputMode {
        let mode = self.term.mode();
        InputMode {
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        }
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
    use mkrew_protocol::{CellRun, CursorSnapshot, TermColor, TermModes};

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
    fn wide_chars_dump_once() {
        let mut emu = Emulator::new(10, 1);
        emu.advance("a日b".as_bytes());
        let grid = emu.grid();
        assert!(grid.rows[0][1].flags.contains(Flags::WIDE_CHAR));
        assert_eq!(grid.to_text(), "a日b\n");
    }

    #[test]
    fn queries_are_left_to_the_daemon() {
        // A DSR must not panic or block: the daemon's emulator answers it.
        let mut emu = Emulator::new(10, 5);
        emu.advance(b"\x1b[6nok");
        assert_eq!(emu.grid().to_text().lines().next(), Some("ok"));
    }

    #[test]
    fn resize_changes_dimensions() {
        let mut emu = Emulator::new(10, 5);
        emu.resize(20, 3);
        let grid = emu.grid();
        assert_eq!((grid.cols, grid.rows.len()), (20, 3));
        assert_eq!(emu.size(), (20, 3));
    }

    #[test]
    fn snapshot_restores_screen_cursor_and_modes_then_output_continues() {
        let snapshot = ScreenSnapshot {
            cols: 12,
            rows: 3,
            lines: vec![
                vec![CellRun {
                    text: "$ ls".into(),
                    fg: TermColor::Indexed(2),
                    bg: TermColor::Default,
                    flags: CellRun::BOLD,
                }],
                vec![],
                vec![],
            ],
            cursor: CursorSnapshot {
                row: 1,
                col: 0,
                visible: true,
            },
            modes: TermModes {
                app_cursor: true,
                bracketed_paste: true,
                ..TermModes::default()
            },
        };
        let mut emu = Emulator::from_snapshot(&snapshot);
        assert_eq!(emu.size(), (12, 3));
        assert_eq!(
            emu.input_mode(),
            InputMode {
                app_cursor: true,
                bracketed_paste: true
            }
        );
        let grid = emu.grid();
        assert_eq!(grid.rows[0][0].fg, Color::Named(NamedColor::Green));
        assert!(grid.rows[0][0].flags.contains(Flags::BOLD));
        emu.advance(b"a.txt");
        assert_eq!(emu.grid().to_text(), "$ ls\na.txt\n\n");
    }
}
