//! UI-side terminal emulator: daemon snapshot + output bytes in, `Grid` out (SPEC §7, §10).
//!
//! The daemon owns the session and already answers terminal queries (DSR etc.), so this
//! emulator never writes back to the PTY.

use std::cell::RefCell;
use std::sync::Arc;

use alacritty_terminal::Term;
use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, TermMode};
use alacritty_terminal::vte::ansi::{Color, CursorShape, NamedColor, Processor, StdSyncHandler};
use mkrew_protocol::ScreenSnapshot;
use mkrew_protocol::paint::paint;

use crate::terminal::input::InputMode;

/// UI-side scrollback for attached panes (SPEC §10).
const SCROLLBACK_LINES: usize = 10_000;

/// How far back a find-bar query edit looks (Enter searches the whole history).
const INCREMENTAL_SEARCH_LINES: usize = 1_000;

/// One cell of a [`Grid`] snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct GridCell {
    pub c: char,
    /// Zero-width chars attached to `c` (combining marks, VS16, ZWJ sequences).
    pub zerowidth: Vec<char>,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
    /// Inside the mouse selection.
    pub selected: bool,
    /// Part of a find-bar match.
    pub found: Found,
}

/// Find-bar highlight of a cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Found {
    #[default]
    No,
    /// Any match on screen.
    Match,
    /// The match Enter / Shift+Enter moved to.
    Current,
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
    /// Active find-bar pattern. A `RefCell` because the regex's lazy DFA caches mutate while
    /// [`Emulator::grid`] (`&self`) highlights matches.
    search: Option<RefCell<RegexSearch>>,
    /// Match the last [`Emulator::search_next`] moved to.
    current_match: Option<Match>,
    /// The last [`Emulator::shared_grid`]; every `&mut self` method drops it, so it always shows
    /// the current state. Saves rebuilding the grid on frames where this pane did not change.
    last_grid: RefCell<Option<Arc<Grid>>>,
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
            search: None,
            current_match: None,
            last_grid: RefCell::default(),
        }
    }

    /// The terminal state changed: the next [`Emulator::shared_grid`] rebuilds.
    fn changed(&mut self) {
        *self.last_grid.get_mut() = None;
    }

    /// A fresh emulator showing `snapshot` (history as scrollback, screen, cursor, modes);
    /// output frames that follow the snapshot continue from it.
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
        self.changed();
        self.recheck_current_match();
    }

    pub fn resize(&mut self, cols: usize, rows: usize) {
        self.term.resize(TermSize::new(cols, rows));
        self.changed();
        self.recheck_current_match();
    }

    /// Sets the find-bar pattern: a regex, case-insensitive unless it has an uppercase letter.
    /// Empty clears the search. Returns `false` (and highlights nothing) if the pattern is not
    /// a valid regex.
    pub fn set_search(&mut self, pattern: &str) -> bool {
        self.current_match = None;
        self.changed();
        if pattern.is_empty() {
            self.search = None;
            return true;
        }
        self.search = RegexSearch::new(pattern).ok().map(RefCell::new);
        self.search.is_some()
    }

    /// Moves to the next match towards older output (`older`) or newer output, wrapping around
    /// the history, and scrolls the view to it. The first move starts from the edge of the
    /// view. Returns `false` if nothing matches.
    pub fn search_next(&mut self, older: bool) -> bool {
        self.search_from_view(older, None)
    }

    /// Like `search_next(true)`, but only looks [`INCREMENTAL_SEARCH_LINES`] back and never
    /// wraps: runs on every query edit, so a miss must not scan the whole history.
    pub fn search_nearby(&mut self) -> bool {
        self.search_from_view(true, Some(INCREMENTAL_SEARCH_LINES))
    }

    fn search_from_view(&mut self, older: bool, max_lines: Option<usize>) -> bool {
        let (top, bottom) = self.view_lines();
        self.changed();
        let Some(search) = self.search.as_mut() else {
            return false;
        };
        // Left: the next match starts before `origin`; right: it starts at or after it.
        let (direction, origin) = match (older, &self.current_match) {
            (true, Some(current)) => (
                Direction::Left,
                current.start().sub(&self.term, Boundary::None, 1),
            ),
            (false, Some(current)) => (
                Direction::Right,
                current.start().add(&self.term, Boundary::None, 1),
            ),
            (true, None) => (Direction::Left, Point::new(bottom, self.term.last_column())),
            (false, None) => (Direction::Right, Point::new(top, Column(0))),
        };
        let found =
            self.term
                .search_next(search.get_mut(), origin, direction, Side::Left, max_lines);
        if let Some(found) = &found {
            self.term.scroll_to_point(*found.end());
            self.term.scroll_to_point(*found.start());
        }
        self.current_match = found;
        self.current_match.is_some()
    }

    /// The current match no longer holds the matched text (output scrolled it away, a resize
    /// reflowed it): forget it, so the next move starts from the view again.
    fn recheck_current_match(&mut self) {
        let (Some(search), Some(current)) = (self.search.as_mut(), self.current_match.clone())
        else {
            return;
        };
        let topmost = self.term.topmost_line();
        let bottommost = self.term.bottommost_line();
        let in_grid = |point: &Point| point.line >= topmost && point.line <= bottommost;
        let still = in_grid(current.start())
            && in_grid(current.end())
            && self
                .term
                .regex_search_right(search.get_mut(), *current.start(), *current.end())
                == Some(current);
        if !still {
            self.current_match = None;
        }
    }

    /// Grid lines of the top and bottom visible rows.
    fn view_lines(&self) -> (Line, Line) {
        let top = self.view_point(0, 0).line;
        (top, top + (self.term.screen_lines() as i32 - 1))
    }

    /// Matches touching the visible rows, in order.
    fn visible_matches(&self) -> Vec<Match> {
        let Some(search) = &self.search else {
            return Vec::new();
        };
        let (top, bottom) = self.view_lines();
        // Start one line above the view so a match wrapped onto the top row still shows.
        let start = Point::new(top - 1, Column(0)).grid_clamp(&self.term, Boundary::Grid);
        let end = Point::new(bottom, self.term.last_column());
        let mut regex = search.borrow_mut();
        RegexIter::new(start, end, Direction::Right, &self.term, &mut regex).collect()
    }

    /// Modes set by the program that change how keys and pastes are encoded.
    pub fn input_mode(&self) -> InputMode {
        let mode = self.term.mode();
        InputMode {
            app_cursor: mode.contains(TermMode::APP_CURSOR),
            bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        }
    }

    /// A full-screen program (alternate screen, no history) asked for the wheel as arrow keys
    /// and does not read the mouse itself.
    pub fn wheel_sends_arrows(&self) -> bool {
        let mode = self.term.mode();
        mode.contains(TermMode::ALT_SCREEN | TermMode::ALTERNATE_SCROLL)
            && !mode.intersects(TermMode::MOUSE_MODE)
    }

    /// Lines scrolled back into the history; 0 = following the live screen.
    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// Scrolls the view one screen into the history (`up`) or back towards the live screen.
    pub fn scroll_page(&mut self, up: bool) {
        self.term
            .scroll_display(if up { Scroll::PageUp } else { Scroll::PageDown });
        self.changed();
    }

    /// Scrolls the view by `lines` (positive = into the history).
    pub fn scroll_lines(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
        self.changed();
    }

    /// Back to the live screen (typing does this).
    pub fn scroll_to_bottom(&mut self) {
        self.term.scroll_display(Scroll::Bottom);
        self.changed();
    }

    /// Starts a selection at a visible cell (row 0 = top of the view). `side` = which half of
    /// the cell was hit, so a click between cells selects nothing.
    pub fn start_selection(&mut self, row: usize, col: usize, side: Side) {
        let point = self.view_point(row, col);
        self.term.selection = Some(Selection::new(SelectionType::Simple, point, side));
        self.changed();
    }

    /// Moves the end of the current selection; no-op without one.
    pub fn update_selection(&mut self, row: usize, col: usize, side: Side) {
        let point = self.view_point(row, col);
        if let Some(selection) = self.term.selection.as_mut() {
            selection.update(point, side);
        }
        self.changed();
    }

    pub fn clear_selection(&mut self) {
        if self.term.selection.take().is_some() {
            self.changed();
        }
    }

    /// Selected text (lines joined with `\n`, trailing blanks trimmed); `None` if nothing is
    /// selected.
    pub fn selection_text(&self) -> Option<String> {
        self.term
            .selection_to_string()
            .filter(|text| !text.is_empty())
    }

    /// Grid point of a visible cell, accounting for the scrolled-back view.
    fn view_point(&self, row: usize, col: usize) -> Point {
        // Screen sizes are u16 and the history 10k lines, so these fit in i32.
        let line = row as i32 - self.display_offset() as i32;
        Point::new(Line(line), Column(col))
    }

    /// [`Emulator::grid`], built once per change and shared (the view paints every frame).
    pub fn shared_grid(&self) -> Arc<Grid> {
        let mut last = self.last_grid.borrow_mut();
        Arc::clone(last.get_or_insert_with(|| Arc::new(self.grid())))
    }

    pub fn grid(&self) -> Grid {
        let cols = self.term.columns();
        let blank = GridCell {
            c: ' ',
            zerowidth: Vec::new(),
            fg: Color::Named(NamedColor::Foreground),
            bg: Color::Named(NamedColor::Background),
            flags: Flags::empty(),
            selected: false,
            found: Found::No,
        };
        let mut rows = vec![vec![blank; cols]; self.term.screen_lines()];
        let content = self.term.renderable_content();
        let offset = content.display_offset as i32;
        let selection = content.selection;
        for indexed in content.display_iter {
            let row = (indexed.point.line.0 + offset) as usize;
            let col = indexed.point.column.0;
            if let Some(slot) = rows.get_mut(row).and_then(|r| r.get_mut(col)) {
                *slot = GridCell {
                    selected: selection.is_some_and(|range| range.contains(indexed.point)),
                    found: Found::No,
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
        // One pass over the matched cells (the current match last, so it wins), not a lookup
        // per cell: a one-letter query can match thousands of times on screen.
        for found in self.visible_matches() {
            mark(&mut rows, &found, offset, Found::Match);
        }
        if let Some(current) = &self.current_match {
            mark(&mut rows, current, offset, Found::Current);
        }
        Grid { cols, rows, cursor }
    }
}

/// Sets `found` on the visible cells of `range` (grid lines; `offset` = display offset).
fn mark(rows: &mut [Vec<GridCell>], range: &Match, offset: i32, found: Found) {
    let (start, end) = (*range.start(), *range.end());
    for line in start.line.0..=end.line.0 {
        let Some(cells) = usize::try_from(line + offset)
            .ok()
            .and_then(|row| rows.get_mut(row))
        else {
            continue;
        };
        let first = if line == start.line.0 {
            start.column.0
        } else {
            0
        };
        let last = if line == end.line.0 {
            end.column.0
        } else {
            usize::MAX
        };
        for cell in cells
            .iter_mut()
            .skip(first)
            .take(last.saturating_sub(first).saturating_add(1))
        {
            cell.found = found;
        }
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

    fn numbered_lines(emu: &mut Emulator, count: usize) {
        for n in 0..count {
            emu.advance(format!("line{n}\r\n").as_bytes());
        }
    }

    #[test]
    fn selection_marks_cells_and_copies_text() {
        let mut emu = Emulator::new(10, 3);
        emu.advance(b"hello\r\nworld");
        assert_eq!(emu.selection_text(), None);
        emu.start_selection(0, 1, Side::Left);
        emu.update_selection(1, 2, Side::Right);
        assert_eq!(emu.selection_text().as_deref(), Some("ello\nwor"));
        let grid = emu.grid();
        assert!(!grid.rows[0][0].selected);
        assert!(grid.rows[0][1].selected && grid.rows[0][9].selected);
        assert!(grid.rows[1][2].selected && !grid.rows[1][3].selected);
        emu.clear_selection();
        assert_eq!(emu.selection_text(), None);
        assert!(!emu.grid().rows[0][1].selected);
    }

    #[test]
    fn shared_grid_is_rebuilt_after_every_change() {
        let mut emu = Emulator::new(10, 3);
        emu.advance(b"hello");
        let first = emu.shared_grid();
        assert!(Arc::ptr_eq(&first, &emu.shared_grid()), "unchanged: reused");
        emu.start_selection(0, 0, Side::Left);
        emu.update_selection(0, 1, Side::Right);
        assert!(emu.shared_grid().rows[0][1].selected);
        emu.clear_selection();
        assert!(!emu.shared_grid().rows[0][1].selected);
        emu.advance(b"!");
        assert_eq!(*emu.shared_grid(), emu.grid());
        assert_eq!(emu.shared_grid().to_text().lines().next(), Some("hello!"));
    }

    #[test]
    fn click_without_drag_selects_nothing() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"hello");
        emu.start_selection(0, 2, Side::Left);
        emu.update_selection(0, 2, Side::Left);
        assert_eq!(emu.selection_text(), None);
    }

    #[test]
    fn scrollback_pages_through_history() {
        let mut emu = Emulator::new(10, 3);
        numbered_lines(&mut emu, 10);
        assert_eq!(emu.grid().to_text(), "line8\nline9\n\n");
        emu.scroll_page(true);
        assert_eq!(emu.display_offset(), 3);
        assert_eq!(emu.grid().to_text(), "line5\nline6\nline7\n");
        emu.scroll_lines(-1);
        assert_eq!(emu.grid().to_text(), "line6\nline7\nline8\n");
        emu.scroll_page(false);
        assert_eq!(emu.display_offset(), 0);
        emu.scroll_page(true);
        emu.scroll_to_bottom();
        assert_eq!(emu.grid().to_text(), "line8\nline9\n\n");
    }

    #[test]
    fn selection_in_scrolled_view_uses_history_lines() {
        let mut emu = Emulator::new(10, 3);
        numbered_lines(&mut emu, 10);
        emu.scroll_page(true);
        emu.start_selection(0, 0, Side::Left);
        emu.update_selection(0, 4, Side::Right);
        assert_eq!(emu.selection_text().as_deref(), Some("line5"));
        assert!(emu.grid().rows[0][0].selected);
        // The selection stays on its text when the view moves.
        emu.scroll_to_bottom();
        assert_eq!(emu.selection_text().as_deref(), Some("line5"));
        assert!(!emu.grid().rows[0][0].selected);
    }

    #[test]
    fn wheel_becomes_arrows_only_in_alt_screen_without_mouse_mode() {
        let mut emu = Emulator::new(10, 3);
        // ALTERNATE_SCROLL is on by default; the main screen scrolls the history.
        assert!(!emu.wheel_sends_arrows());
        emu.advance(b"\x1b[?1049h");
        assert!(emu.wheel_sends_arrows());
        emu.advance(b"\x1b[?1000h");
        assert!(!emu.wheel_sends_arrows(), "program reads the mouse");
        emu.advance(b"\x1b[?1000l\x1b[?1007l");
        assert!(!emu.wheel_sends_arrows(), "alternate scroll off");
    }

    fn found_cols(emu: &Emulator, row: usize) -> Vec<(usize, Found)> {
        emu.grid().rows[row]
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.found != Found::No)
            .map(|(col, cell)| (col, cell.found))
            .collect()
    }

    #[test]
    fn search_walks_the_history_and_scrolls_to_each_match() {
        let mut emu = Emulator::new(10, 3);
        numbered_lines(&mut emu, 10);
        assert!(emu.set_search("line[27]"));
        // Older from the bottom of the view: line7, then line2, then wrap to line7.
        assert!(emu.search_next(true));
        assert_eq!(emu.grid().to_text(), "line7\nline8\nline9\n");
        assert_eq!(
            found_cols(&emu, 0),
            (0..5).map(|col| (col, Found::Current)).collect::<Vec<_>>()
        );
        assert!(emu.search_next(true));
        assert_eq!(emu.grid().to_text().lines().next(), Some("line2"));
        assert!(emu.search_next(true));
        assert_eq!(emu.grid().to_text(), "line5\nline6\nline7\n");
        // Newer from line7 wraps to line2; both are on screen then.
        emu.scroll_page(true);
        assert_eq!(emu.grid().to_text(), "line2\nline3\nline4\n");
        assert!(emu.search_next(false));
        assert_eq!(emu.grid().to_text().lines().next(), Some("line2"));
        assert_eq!(found_cols(&emu, 0)[0], (0, Found::Current));
    }

    #[test]
    fn search_highlights_every_visible_match() {
        let mut emu = Emulator::new(12, 2);
        emu.advance(b"foo bar foo\r\nFOO");
        assert!(emu.set_search("foo"));
        let match_at =
            |cols: std::ops::Range<usize>| cols.map(|col| (col, Found::Match)).collect::<Vec<_>>();
        let mut both = match_at(0..3);
        both.extend(match_at(8..11));
        assert_eq!(found_cols(&emu, 0), both);
        // Lowercase pattern = case-insensitive.
        assert_eq!(found_cols(&emu, 1), match_at(0..3));
        // An uppercase letter makes it case-sensitive.
        assert!(emu.set_search("FOO"));
        assert!(found_cols(&emu, 0).is_empty());
        assert_eq!(found_cols(&emu, 1), match_at(0..3));
    }

    #[test]
    fn nearby_search_stops_short_of_old_history() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"needle\r\n");
        numbered_lines(&mut emu, INCREMENTAL_SEARCH_LINES + 10);
        assert!(emu.set_search("needle"));
        assert!(!emu.search_nearby());
        assert_eq!(emu.display_offset(), 0);
        assert!(emu.search_next(true), "Enter searches the whole history");
        assert_eq!(emu.grid().to_text().lines().next(), Some("needle"));
    }

    #[test]
    fn a_screen_full_of_matches_is_marked_cell_by_cell() {
        let (cols, rows) = (200, 60);
        let mut emu = Emulator::new(cols, rows);
        numbered_lines(&mut emu, SCROLLBACK_LINES);
        let line = "e".repeat(cols);
        for _ in 0..rows {
            emu.advance(line.as_bytes());
        }
        assert!(emu.set_search("e"));
        assert!(emu.search_next(true));
        let grid = emu.grid();
        let count = |kind| {
            grid.rows
                .iter()
                .flatten()
                .filter(|cell| cell.found == kind)
                .count()
        };
        assert_eq!(count(Found::Current), 1);
        assert_eq!(count(Found::Match), cols * rows - 1);
    }

    #[test]
    fn invalid_or_empty_pattern_clears_the_search() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"a(b");
        assert!(emu.set_search("a"));
        assert!(emu.search_next(true));
        assert!(!emu.set_search("a("));
        assert!(!emu.search_next(true));
        assert!(found_cols(&emu, 0).is_empty());
        assert!(emu.set_search(""));
        assert!(!emu.search_next(false));
        assert!(emu.set_search("x"));
        assert!(!emu.search_next(true), "no match");
    }

    #[test]
    fn current_match_is_dropped_when_its_text_changes() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"abc");
        assert!(emu.set_search("abc"));
        assert!(emu.search_next(true));
        assert_eq!(found_cols(&emu, 0)[0], (0, Found::Current));
        // Overwrite the matched text in place.
        emu.advance(b"\rxyz");
        assert!(found_cols(&emu, 0).is_empty());
        emu.advance(b"\r\nabc");
        assert_eq!(found_cols(&emu, 1)[0], (0, Found::Match));
    }

    #[test]
    fn history_is_capped_at_the_scrollback_limit() {
        let mut emu = Emulator::new(10, 2);
        numbered_lines(&mut emu, SCROLLBACK_LINES + 50);
        emu.scroll_lines(1_000_000);
        assert_eq!(emu.display_offset(), SCROLLBACK_LINES);
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
            history: vec![],
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

    /// A pane re-attached after a tab switch (or an overflow resnapshot) scrolls back into the
    /// daemon's history; the live screen and the output that follows are unchanged (ADR 0030).
    #[test]
    fn snapshot_history_is_scrollable() {
        let line = |text: &str| {
            vec![CellRun {
                text: text.into(),
                fg: TermColor::Default,
                bg: TermColor::Default,
                flags: 0,
            }]
        };
        let mut snapshot = ScreenSnapshot {
            cols: 10,
            rows: 3,
            lines: vec![line("$ ls"), line("a.txt"), vec![]],
            history: vec![],
            cursor: CursorSnapshot {
                row: 2,
                col: 0,
                visible: true,
            },
            modes: TermModes::default(),
        };
        let screen = Emulator::from_snapshot(&snapshot).grid();
        snapshot.history = (0..5).map(|n| line(&format!("old{n}"))).collect();

        let mut emu = Emulator::from_snapshot(&snapshot);
        assert_eq!(
            emu.grid(),
            screen,
            "the live screen is the same with history"
        );
        // The history sits right above the screen, in order.
        emu.scroll_lines(3);
        assert_eq!(emu.grid().to_text(), "old2\nold3\nold4\n");
        emu.scroll_lines(1_000_000);
        let top = emu.grid().to_text();
        assert!(top.contains("old0\nold1\n"), "{top}");
        // Output continues on the live screen.
        emu.scroll_to_bottom();
        emu.advance(b"b.txt");
        assert_eq!(emu.grid().to_text(), "$ ls\na.txt\nb.txt\n");
    }
}
