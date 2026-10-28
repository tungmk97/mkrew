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
use alacritty_terminal::vi_mode::ViMotion;
use alacritty_terminal::vte::ansi::{
    Color, CursorShape, CursorStyle, NamedColor, Processor, StdSyncHandler,
};
use mkrew_protocol::ScreenSnapshot;
use mkrew_protocol::paint::paint;

use crate::terminal::copy_mode::{CopyKey, Select};
use crate::terminal::input::InputMode;
use crate::terminal::links::SPACER;

/// UI-side scrollback for attached panes: what the daemon keeps and a re-attach brings back
/// (ADR 0082, ADR 0088), so a larger number would only last until the next tab switch.
const SCROLLBACK_LINES: usize = 1_000;

/// Scrollback of a History replay pane ([`Emulator::for_replay`]): one pane at a time, dropped when
/// the dialog closes, fed the tail of a log that holds far more than 1,000 lines (ADR 0088).
const REPLAY_SCROLLBACK_LINES: usize = 10_000;

/// How far back a find-bar query edit looks (Enter searches the whole history). An attached pane's
/// history is no longer than this; only a replay pane sees the difference.
const INCREMENTAL_SEARCH_LINES: usize = 1_000;

/// Output held back while the screen must stay as it is (copy mode, link labels) is applied at
/// once above this size.
// ponytail: a flood while holding moves the view instead of growing memory; page the held
// output if a user ever copies from a pane that prints more than this.
const HOLD_LIMIT: usize = 1 << 20;

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
    /// `Hidden` when the program hid it; a bar unless the program asked for another shape.
    pub shape: CursorShape,
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

    /// The chars of screen row `row`, one per cell (a wide char's second cell is
    /// [`SPACER`]), for link detection; `None` past the last row.
    pub fn row_chars(&self, row: usize) -> Option<Vec<char>> {
        let spacers = Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER;
        let cells = self.rows.get(row)?;
        Some(
            (cells.iter())
                .map(|cell| {
                    if cell.flags.intersects(spacers) {
                        SPACER
                    } else {
                        cell.c
                    }
                })
                .collect(),
        )
    }
}

/// What a copy-mode key asks of the pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyEnd {
    /// Still in copy mode.
    Stay,
    /// Left copy mode; the selected text (`y` / Enter), if any, goes to the clipboard.
    Leave(Option<String>),
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
    /// Output waits in `held` while this is set or copy mode is on (see [`Emulator::set_hold`]).
    hold: bool,
    held: Vec<u8>,
}

impl Emulator {
    pub fn new(cols: usize, rows: usize) -> Self {
        Self::with_scrollback(cols, rows, SCROLLBACK_LINES)
    }

    /// An emulator for a History replay pane, which keeps [`REPLAY_SCROLLBACK_LINES`].
    pub fn for_replay(cols: usize, rows: usize) -> Self {
        Self::with_scrollback(cols, rows, REPLAY_SCROLLBACK_LINES)
    }

    fn with_scrollback(cols: usize, rows: usize, lines: usize) -> Self {
        let config = Config {
            scrolling_history: lines,
            default_cursor_style: CursorStyle {
                shape: CursorShape::Beam,
                blinking: false,
            },
            vi_mode_cursor_style: Some(CursorStyle {
                shape: CursorShape::Block,
                blinking: false,
            }),
            ..Config::default()
        };
        Self {
            term: Term::new(config, &TermSize::new(cols, rows), VoidListener),
            parser: Processor::new(),
            search: None,
            current_match: None,
            last_grid: RefCell::default(),
            hold: false,
            held: Vec::new(),
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
        // Copy mode: the match is where the cursor goes.
        if self.copy_mode()
            && let Some(found) = &self.current_match
        {
            self.term.vi_goto_point(*found.start());
        }
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

    /// Copy mode (SPEC §10) is `alacritty_terminal`'s vi mode: a cursor that moves over the
    /// screen and the history, and a selection that follows it.
    pub fn copy_mode(&self) -> bool {
        self.term.mode().contains(TermMode::VI)
    }

    /// Turns copy mode on: the vi cursor starts at the program's cursor (the top-left cell of
    /// the view when that is scrolled out).
    pub fn enter_copy_mode(&mut self) {
        if !self.copy_mode() {
            self.term.selection = None;
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
                // A line selection ends with a line break; pasted into a shell it would run.
                let text = (self.selection_text())
                    .map(|text| text.trim_end_matches('\n').to_owned())
                    .filter(|text| !text.is_empty());
                self.leave_copy_mode();
                return CopyEnd::Leave(text);
            }
            CopyKey::Cancel => {
                if (self.term.selection.take())
                    .filter(|selection| !selection.is_empty())
                    .is_none()
                {
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

    /// Grid point of a visible cell, accounting for the scrolled-back view.
    fn view_point(&self, row: usize, col: usize) -> Point {
        // Screen sizes are u16 and the history at most 10k lines, so these fit in i32.
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
            shape: content.cursor.shape,
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
                shape: CursorShape::Beam
            }
        );
    }

    #[test]
    fn program_cursor_shape_overrides_the_bar() {
        let mut emu = Emulator::new(10, 3);
        assert_eq!(emu.grid().cursor.shape, CursorShape::Beam);
        emu.advance(b"\x1b[2 q");
        assert_eq!(emu.grid().cursor.shape, CursorShape::Block);
        emu.advance(b"\x1b[0 q");
        assert_eq!(
            emu.grid().cursor.shape,
            CursorShape::Beam,
            "reset = default"
        );
        emu.advance(b"\x1b[?25l");
        assert_eq!(emu.grid().cursor.shape, CursorShape::Hidden);
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

    /// Link detection indexes cells by column: one char per cell, a wide char's second cell
    /// is the spacer.
    #[test]
    fn a_row_has_one_char_per_cell() {
        let mut emu = Emulator::new(8, 1);
        emu.advance("a日b".as_bytes());
        let grid = emu.grid();
        assert_eq!(
            grid.row_chars(0),
            Some(vec!['a', '日', SPACER, 'b', ' ', ' ', ' ', ' '])
        );
        assert_eq!(grid.row_chars(1), None);
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
        // Only a replay pane has more history than the incremental window.
        let mut emu = Emulator::for_replay(10, 2);
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

    /// ADR 0088: an attached pane keeps what the daemon keeps (1,000 lines), no more.
    #[test]
    fn an_attached_pane_scrolls_back_1000_lines() {
        let mut emu = Emulator::new(10, 2);
        numbered_lines(&mut emu, 1_500);
        emu.scroll_lines(1_000_000);
        assert_eq!(emu.display_offset(), 1_000);
    }

    /// A pane re-attached after the daemon's history is full (1,000 lines) keeps every line of it.
    #[test]
    fn a_full_daemon_history_fits_the_pane_scrollback() {
        let line = |text: String| {
            vec![CellRun {
                text,
                fg: TermColor::Default,
                bg: TermColor::Default,
                flags: 0,
            }]
        };
        let snapshot = ScreenSnapshot {
            cols: 10,
            rows: 3,
            lines: vec![vec![], vec![], vec![]],
            history: (0..1_000).map(|n| line(format!("old{n}"))).collect(),
            cursor: CursorSnapshot {
                row: 0,
                col: 0,
                visible: true,
            },
            modes: TermModes::default(),
        };
        let mut emu = Emulator::from_snapshot(&snapshot);
        emu.scroll_lines(1_000_000);
        assert_eq!(emu.display_offset(), 1_000);
        assert!(
            emu.grid().to_text().starts_with("old0\nold1\nold2\n"),
            "the oldest daemon line is still there"
        );
    }

    /// The History dialog replays a log tail with far more than 1,000 lines and scrolls through it.
    #[test]
    fn a_replay_pane_keeps_10000_lines() {
        let mut emu = Emulator::for_replay(10, 2);
        numbered_lines(&mut emu, 12_000);
        emu.scroll_lines(1_000_000);
        assert_eq!(emu.display_offset(), 10_000);
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
        assert_eq!(text, "hello abc\nsecond", "no trailing line break");
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
        // alacritty ends a line selection with the line break.
        assert_eq!(emu.selection_text().as_deref(), Some("second\n"));
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

    #[test]
    fn overflow_applies_the_held_bytes_first_then_the_new_ones() {
        let mut emu = Emulator::new(10, 2);
        emu.set_hold(true);
        emu.advance(b"ab\r");
        // Carriage returns do not scroll: "c" overwrites "a" only if it comes after the held "ab".
        let mut flood = vec![b'\r'; HOLD_LIMIT];
        flood[0] = b'c';
        emu.advance(&flood);
        assert_eq!(emu.grid().to_text(), "cb\n\n", "held first, then new");
    }

    #[test]
    fn leaving_copy_mode_keeps_an_explicit_hold() {
        let mut emu = Emulator::new(10, 2);
        emu.advance(b"a");
        emu.set_hold(true);
        emu.enter_copy_mode();
        emu.advance(b"b");
        emu.leave_copy_mode();
        assert_eq!(emu.grid().to_text(), "a\n\n", "still held");
        emu.set_hold(false);
        assert_eq!(emu.grid().to_text(), "ab\n\n");
    }

    #[test]
    fn copy_esc_after_a_click_leaves_at_once() {
        let mut emu = Emulator::new(12, 3);
        emu.advance(b"hello");
        emu.start_selection(0, 1, Side::Left);
        emu.enter_copy_mode();
        assert_eq!(copy_keys(&mut emu, [CopyKey::Cancel]), CopyEnd::Leave(None));
        assert!(!emu.copy_mode());
    }

    #[test]
    fn entering_copy_mode_drops_a_mouse_selection() {
        let mut emu = Emulator::new(12, 3);
        emu.advance(b"hello");
        emu.start_selection(0, 0, Side::Left);
        emu.update_selection(0, 3, Side::Right);
        assert!(emu.selection_text().is_some());
        emu.enter_copy_mode();
        assert_eq!(emu.selection_text(), None);
    }
}
