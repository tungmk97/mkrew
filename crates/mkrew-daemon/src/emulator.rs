//! Headless terminal emulator per session (SPEC §8): the real screen text for state detection.
//! Adapted from `spike-term/src/emulator.rs`; attach snapshots (T1.9) build on it.

use std::sync::{Arc, Mutex, PoisonError};

use alacritty_terminal::Term;
use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Config;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};

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

    #[test]
    fn scrolled_output_keeps_the_visible_screen() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"a\r\nb\r\nc");
        assert_eq!(emu.lines_to_cursor(), ["b", "c"]);
    }
}
