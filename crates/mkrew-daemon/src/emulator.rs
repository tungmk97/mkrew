//! Headless terminal emulator per session (SPEC §8): the real screen text for state detection
//! and the screen snapshot sent on attach. Adapted from `spike-term/src/emulator.rs`.

use std::sync::{Arc, Mutex, PoisonError};

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Processor, StdSyncHandler};
use mkrew_protocol::{CellRun, CursorSnapshot, ScreenSnapshot, TermColor, TermModes};

/// Small on purpose: long history lives in the session log file (SPEC §8).
pub const SCROLLBACK_LINES: usize = 200;

/// Collects bytes the terminal sends back to the program (e.g. DSR replies).
/// ConPTY asks for the cursor position on startup and stalls until it gets an answer.
#[derive(Clone, Default)]
struct ReplyListener {
    replies: Arc<Mutex<Vec<u8>>>,
}

impl EventListener for ReplyListener {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(text) = event {
            let mut replies = self.replies.lock().unwrap_or_else(PoisonError::into_inner);
            replies.extend_from_slice(text.as_bytes());
        }
    }
}

pub struct Emulator {
    term: Term<ReplyListener>,
    parser: Processor<StdSyncHandler>,
    listener: ReplyListener,
}

impl Emulator {
    pub fn new(cols: u16, rows: u16) -> Self {
        let listener = ReplyListener::default();
        let config = Config {
            scrolling_history: SCROLLBACK_LINES,
            ..Config::default()
        };
        let size = TermSize::new(usize::from(cols), usize::from(rows));
        Self {
            term: Term::new(config, &size, listener.clone()),
            parser: Processor::new(),
            listener,
        }
    }

    /// Feeds output bytes; returns bytes that must be written back to the PTY.
    pub fn advance(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.advance(&mut self.term, bytes);
        let mut replies = self
            .listener
            .replies
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *replies)
    }

    /// Visible lines from the top of the screen down to the cursor row, trailing blanks
    /// trimmed. What the state machine matches `needs_input_patterns` against (SPEC §6).
    pub fn lines_to_cursor(&self) -> Vec<String> {
        let grid = self.term.grid();
        let cursor_row = self.term.grid().cursor.point.line.0.max(0);
        let last = usize::try_from(cursor_row)
            .unwrap_or(0)
            .min(self.term.screen_lines().saturating_sub(1));
        (0..=last)
            .map(|row| {
                let line = &grid[Line(i32::try_from(row).unwrap_or(i32::MAX))];
                let text: String = (0..self.term.columns())
                    .map(|c| line[Column(c)].c)
                    .collect();
                text.trim_end().to_owned()
            })
            .collect()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.term
            .resize(TermSize::new(usize::from(cols), usize::from(rows)));
    }

    /// The program asked for pastes wrapped in `ESC[200~` … `ESC[201~` (mode 2004).
    pub fn bracketed_paste(&self) -> bool {
        self.term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// The visible screen, cursor and input modes (`session.attach`, SPEC §7).
    pub fn snapshot(&self) -> ScreenSnapshot {
        let grid = self.term.grid();
        let lines = (0..self.term.screen_lines())
            .map(|row| {
                let line = &grid[Line(i32::try_from(row).unwrap_or(i32::MAX))];
                let mut runs: Vec<CellRun> = Vec::new();
                for col in 0..self.term.columns() {
                    let cell = &line[Column(col)];
                    if cell
                        .flags
                        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                    {
                        continue;
                    }
                    let (fg, bg, flags) = (color(cell.fg), color(cell.bg), run_flags(cell.flags));
                    match runs.last_mut() {
                        Some(run) if (run.fg, run.bg, run.flags) == (fg, bg, flags) => {
                            run.text.push(cell.c);
                        }
                        _ => runs.push(CellRun {
                            text: cell.c.into(),
                            fg,
                            bg,
                            flags,
                        }),
                    }
                    if let (Some(run), Some(zerowidth)) = (runs.last_mut(), cell.zerowidth()) {
                        run.text.extend(zerowidth);
                    }
                }
                trim_trailing_blanks(&mut runs);
                runs
            })
            .collect();
        let point = grid.cursor.point;
        let mode = self.term.mode();
        ScreenSnapshot {
            cols: to_u16(self.term.columns()),
            rows: to_u16(self.term.screen_lines()),
            lines,
            cursor: CursorSnapshot {
                row: to_u16(usize::try_from(point.line.0).unwrap_or(0)),
                col: to_u16(point.column.0),
                visible: mode.contains(TermMode::SHOW_CURSOR),
            },
            modes: TermModes {
                alt_screen: mode.contains(TermMode::ALT_SCREEN),
                app_cursor: mode.contains(TermMode::APP_CURSOR),
                app_keypad: mode.contains(TermMode::APP_KEYPAD),
                bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
                mouse: mode.intersects(TermMode::MOUSE_MODE),
                sgr_mouse: mode.contains(TermMode::SGR_MOUSE),
                focus_events: mode.contains(TermMode::FOCUS_IN_OUT),
            },
        }
    }
}

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// ANSI colors 0–15 (and their dim variants) become indexed; theme colors are `Default`.
fn color(color: Color) -> TermColor {
    match color {
        Color::Spec(rgb) => TermColor::Rgb([rgb.r, rgb.g, rgb.b]),
        Color::Indexed(i) => TermColor::Indexed(i),
        Color::Named(named) => {
            let n = named as usize;
            let dim_black = NamedColor::DimBlack as usize;
            let index = match n {
                0..16 => n,
                _ if (dim_black..dim_black + 8).contains(&n) => n - dim_black,
                _ => return TermColor::Default,
            };
            u8::try_from(index).map_or(TermColor::Default, TermColor::Indexed)
        }
    }
}

fn run_flags(flags: Flags) -> u16 {
    [
        (Flags::BOLD, CellRun::BOLD),
        (Flags::DIM, CellRun::DIM),
        (Flags::ITALIC, CellRun::ITALIC),
        (Flags::ALL_UNDERLINES, CellRun::UNDERLINE),
        (Flags::INVERSE, CellRun::INVERSE),
        (Flags::HIDDEN, CellRun::HIDDEN),
        (Flags::STRIKEOUT, CellRun::STRIKEOUT),
        (Flags::WIDE_CHAR, CellRun::WIDE),
    ]
    .into_iter()
    .filter(|(cell, _)| flags.intersects(*cell))
    .fold(0, |acc, (_, bit)| acc | bit)
}

/// Drops trailing spaces that would render as nothing (default background, no decoration).
fn trim_trailing_blanks(runs: &mut Vec<CellRun>) {
    const VISIBLE_ON_BLANK: u16 = CellRun::INVERSE | CellRun::UNDERLINE | CellRun::STRIKEOUT;
    while let Some(run) = runs.last_mut() {
        if run.bg != TermColor::Default || run.flags & VISIBLE_ON_BLANK != 0 {
            return;
        }
        let trimmed = run.text.trim_end_matches(' ').len();
        run.text.truncate(trimmed);
        if !run.text.is_empty() {
            return;
        }
        runs.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_stop_at_the_cursor_row() {
        let mut emu = Emulator::new(20, 5);
        emu.advance(b"one\r\ntwo  \r\n\x1b[31mthree\x1b[0m");
        assert_eq!(emu.lines_to_cursor(), ["one", "two", "three"]);
        emu.advance(b"\x1b[1;1H");
        assert_eq!(emu.lines_to_cursor(), ["one"]);
    }

    #[test]
    fn cursor_position_request_is_answered() {
        let mut emu = Emulator::new(10, 5);
        assert_eq!(emu.advance(b"\x1b[2;3H\x1b[6n"), b"\x1b[2;3R");
        assert!(emu.advance(b"x").is_empty());
    }

    fn run(text: &str, fg: TermColor, bg: TermColor, flags: u16) -> CellRun {
        CellRun {
            text: text.into(),
            fg,
            bg,
            flags,
        }
    }

    #[test]
    fn snapshot_groups_styled_runs_and_trims_blanks() {
        let mut emu = Emulator::new(12, 3);
        emu.advance(b"ab\x1b[1;31mcd\x1b[0m \x1b[44mx \x1b[0m  \r\n\x1b[38;2;1;2;3m\x1b[2mz");
        let snap = emu.snapshot();
        assert_eq!((snap.cols, snap.rows), (12, 3));
        let d = TermColor::Default;
        assert_eq!(
            snap.lines[0],
            [
                run("ab", d, d, 0),
                run("cd", TermColor::Indexed(1), d, CellRun::BOLD),
                run(" ", d, d, 0),
                run("x ", d, TermColor::Indexed(4), 0),
            ]
        );
        assert_eq!(
            snap.lines[1],
            [run("z", TermColor::Rgb([1, 2, 3]), d, CellRun::DIM)]
        );
        assert!(snap.lines[2].is_empty());
        assert_eq!(
            snap.cursor,
            CursorSnapshot {
                row: 1,
                col: 1,
                visible: true
            }
        );
    }

    #[test]
    fn snapshot_wide_chars_get_their_own_run() {
        let mut emu = Emulator::new(10, 2);
        emu.advance("a日本b".as_bytes());
        let d = TermColor::Default;
        assert_eq!(
            emu.snapshot().lines[0],
            [
                run("a", d, d, 0),
                run("日本", d, d, CellRun::WIDE),
                run("b", d, d, 0)
            ]
        );
    }

    #[test]
    fn snapshot_reports_modes_and_hidden_cursor() {
        let mut emu = Emulator::new(10, 2);
        assert_eq!(emu.snapshot().modes, TermModes::default());
        emu.advance(b"\x1b[?1049h\x1b[?1h\x1b[?2004h\x1b[?1000h\x1b[?1006h\x1b[?25l");
        let snap = emu.snapshot();
        assert_eq!(
            snap.modes,
            TermModes {
                alt_screen: true,
                app_cursor: true,
                bracketed_paste: true,
                mouse: true,
                sgr_mouse: true,
                ..TermModes::default()
            }
        );
        assert!(!snap.cursor.visible);
    }

    #[test]
    fn resize_changes_the_snapshot_size() {
        let mut emu = Emulator::new(10, 2);
        emu.resize(30, 7);
        let snap = emu.snapshot();
        assert_eq!((snap.cols, snap.rows, snap.lines.len()), (30, 7, 7));
    }

    #[test]
    fn scrolled_output_keeps_the_visible_screen() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"a\r\nb\r\nc");
        assert_eq!(emu.lines_to_cursor(), ["b", "c"]);
    }
}
