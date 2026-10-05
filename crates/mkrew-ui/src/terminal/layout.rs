//! Grid rows → styled text segments and background rects (SPEC §10). Pure (no gpui types) so
//! the batching and color rules are unit-tested; `view` only shapes and paints the result.

use alacritty_terminal::index::Side;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::vte::ansi::{Color, NamedColor};

use crate::terminal::emulator::{Found, GridCell};
use crate::theme::Theme;

/// Resolved style of one cell. Colors are `0xRRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellStyle {
    pub fg: u32,
    pub bg: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
}

/// `len` bytes of a segment's text drawn with `style`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub len: usize,
    pub style: CellStyle,
}

/// Text shaped as one line and painted at `start_col`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start_col: usize,
    pub text: String,
    pub runs: Vec<Run>,
    /// Every char is one single-width cell: glyphs may be forced onto the cell grid.
    /// False for a lone wide char or a char with zero-width marks attached.
    pub grid_aligned: bool,
}

/// A non-default background over `cols` cells from `start_col`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Background {
    pub start_col: usize,
    pub cols: usize,
    pub color: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RowLayout {
    pub segments: Vec<Segment>,
    pub backgrounds: Vec<Background>,
}

const SPACERS: Flags = Flags::WIDE_CHAR_SPACER.union(Flags::LEADING_WIDE_CHAR_SPACER);

/// Batches a row into as few shaped segments/runs as possible: adjacent single-width cells share
/// a segment, adjacent cells with equal style share a run. Trailing blanks are dropped.
/// `cursor_col` draws the cursor block on that cell (both cells of a wide char).
pub fn layout_row(cells: &[GridCell], cursor_col: Option<usize>, theme: &Theme) -> RowLayout {
    let mut layout = RowLayout::default();
    let text_end = cells
        .iter()
        .rposition(|cell| !is_blank(cell))
        .map_or(0, |last| last + 1);
    let cursor_cols = cursor_col.map(|col| {
        let wide = cells
            .get(col)
            .is_some_and(|cell| cell.flags.contains(Flags::WIDE_CHAR));
        col..=if wide { col + 1 } else { col }
    });
    let mut current: Option<Segment> = None;

    for (col, cell) in cells.iter().enumerate() {
        let is_cursor = cursor_cols.as_ref().is_some_and(|cols| cols.contains(&col));
        let style = cell_style(cell, is_cursor, theme);
        push_background(&mut layout.backgrounds, col, style.bg, theme);
        if col >= text_end {
            continue;
        }
        if cell.flags.intersects(SPACERS) {
            layout.segments.extend(current.take());
            continue;
        }
        if cell.flags.contains(Flags::WIDE_CHAR) || !cell.zerowidth.is_empty() {
            layout.segments.extend(current.take());
            let mut segment = Segment {
                start_col: col,
                text: String::new(),
                runs: Vec::new(),
                grid_aligned: false,
            };
            push_text(&mut segment, cell, style);
            layout.segments.push(segment);
            continue;
        }
        let segment = current.get_or_insert_with(|| Segment {
            start_col: col,
            text: String::new(),
            runs: Vec::new(),
            grid_aligned: true,
        });
        push_text(segment, cell, style);
    }
    layout.segments.extend(current);
    layout
}

fn is_blank(cell: &GridCell) -> bool {
    cell.c == ' ' && cell.zerowidth.is_empty() && !cell.flags.intersects(Flags::ALL_UNDERLINES)
}

fn push_text(segment: &mut Segment, cell: &GridCell, style: CellStyle) {
    let start = segment.text.len();
    segment.text.push(cell.c);
    segment.text.extend(cell.zerowidth.iter());
    let len = segment.text.len() - start;
    match segment.runs.last_mut() {
        Some(run) if run.style == style => run.len += len,
        _ => segment.runs.push(Run { len, style }),
    }
}

fn push_background(backgrounds: &mut Vec<Background>, col: usize, color: u32, theme: &Theme) {
    if color == theme.bg {
        return;
    }
    match backgrounds.last_mut() {
        Some(bg) if bg.color == color && bg.start_col + bg.cols == col => bg.cols += 1,
        _ => backgrounds.push(Background {
            start_col: col,
            cols: 1,
            color,
        }),
    }
}

/// Applies SGR flags (inverse, dim, hidden) and the cursor block to a cell's colors.
pub fn cell_style(cell: &GridCell, is_cursor: bool, theme: &Theme) -> CellStyle {
    let mut fg = resolve_color(cell.fg, theme);
    let mut bg = resolve_color(cell.bg, theme);
    if cell.flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut fg, &mut bg);
    }
    if cell.flags.contains(Flags::DIM) {
        fg = blend(fg, bg, theme.dim_blend);
    }
    if cell.flags.contains(Flags::HIDDEN) {
        fg = bg;
    }
    // Program colors (inverse bars, muted text) can vanish on the selection: use body text.
    // Same for find matches; the selection wins over a match.
    match cell.found {
        _ if cell.selected => {
            fg = theme.text;
            bg = theme.terminal_selection_bg;
        }
        Found::Current => {
            fg = theme.text;
            bg = theme.terminal_match_current_bg;
        }
        Found::Match => {
            fg = theme.text;
            bg = theme.terminal_match_bg;
        }
        Found::No => {}
    }
    if is_cursor {
        fg = theme.bg;
        bg = theme.accent;
    }
    CellStyle {
        fg,
        bg,
        bold: cell.flags.contains(Flags::BOLD),
        italic: cell.flags.contains(Flags::ITALIC),
        underline: cell.flags.intersects(Flags::ALL_UNDERLINES),
    }
}

/// Maps a terminal color to the theme: ANSI 0–15 and the default colors come from tokens,
/// 16–255 follow the xterm 6×6×6 cube and gray ramp, truecolor passes through.
pub fn resolve_color(color: Color, theme: &Theme) -> u32 {
    match color {
        Color::Spec(rgb) => pack(rgb.r, rgb.g, rgb.b),
        Color::Indexed(index) => indexed_color(index, theme),
        Color::Named(named) => match named {
            NamedColor::Foreground | NamedColor::BrightForeground => theme.text,
            NamedColor::Background => theme.bg,
            NamedColor::Cursor => theme.accent,
            NamedColor::DimForeground => blend(theme.text, theme.bg, theme.dim_blend),
            NamedColor::DimBlack
            | NamedColor::DimRed
            | NamedColor::DimGreen
            | NamedColor::DimYellow
            | NamedColor::DimBlue
            | NamedColor::DimMagenta
            | NamedColor::DimCyan
            | NamedColor::DimWhite => {
                let index = named as usize - NamedColor::DimBlack as usize;
                blend(theme.ansi[index], theme.bg, theme.dim_blend)
            }
            // Black..=BrightWhite have discriminants 0..=15.
            ansi => theme.ansi[(ansi as usize) % 16],
        },
    }
}

fn indexed_color(index: u8, theme: &Theme) -> u32 {
    const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match index {
        0..=15 => theme.ansi[usize::from(index)],
        16..=231 => {
            let i = index - 16;
            pack(
                CUBE_LEVELS[usize::from(i / 36)],
                CUBE_LEVELS[usize::from(i / 6 % 6)],
                CUBE_LEVELS[usize::from(i % 6)],
            )
        }
        232..=255 => {
            let level = 8 + 10 * (index - 232);
            pack(level, level, level)
        }
    }
}

fn pack(r: u8, g: u8, b: u8) -> u32 {
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// `weight` of `color` mixed with `1 - weight` of `towards`, per channel.
fn blend(color: u32, towards: u32, weight: f32) -> u32 {
    let channel = |shift: u32| {
        let a = ((color >> shift) & 0xFF) as f32;
        let b = ((towards >> shift) & 0xFF) as f32;
        // Rounded mix of two 0..=255 values stays in 0..=255.
        (a * weight + b * (1.0 - weight)).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// Smallest grid the view will ask for, so a tiny window never yields a 0-sized PTY.
pub const MIN_COLS: u16 = 2;
pub const MIN_ROWS: u16 = 1;

/// Grid size (cols, rows) that fits `width` x `height` pixels after `padding` on every side.
pub fn grid_size(
    width: f32,
    height: f32,
    padding: f32,
    cell_width: f32,
    line_height: f32,
) -> (u16, u16) {
    let fit = |space: f32, cell: f32, min: u16| {
        let count = ((space - 2.0 * padding) / cell).floor();
        // `as` saturates for out-of-range and NaN floats; then clamp to the minimum.
        (count as u16).max(min)
    };
    (
        fit(width, cell_width, MIN_COLS),
        fit(height, line_height, MIN_ROWS),
    )
}

/// The visible cell (row, col) under a point `x`, `y` pixels from the grid's top-left corner
/// (inside the padding), and which half of it was hit. Points outside clamp to the edge cells.
pub fn cell_at(
    x: f32,
    y: f32,
    cell_width: f32,
    line_height: f32,
    (cols, rows): (usize, usize),
) -> (usize, usize, Side) {
    let col_f = (x / cell_width).max(0.0);
    // `as` saturates NaN/huge floats; then clamp to the grid.
    let col = (col_f as usize).min(cols.saturating_sub(1));
    let side = if x >= cell_width * cols as f32 {
        Side::Right
    } else if col_f.fract() < 0.5 {
        Side::Left
    } else {
        Side::Right
    };
    let row = ((y / line_height).max(0.0) as usize).min(rows.saturating_sub(1));
    (row, col, side)
}

/// Per-row cache of rendered rows: a row is rebuilt only when its cells or cursor changed
/// ("re-layout only dirty rows", SPEC §10).
pub struct RowCache<T> {
    rows: Vec<Option<CachedRow<T>>>,
}

struct CachedRow<T> {
    cells: Vec<GridCell>,
    cursor_col: Option<usize>,
    value: T,
}

impl<T> Default for RowCache<T> {
    fn default() -> Self {
        Self { rows: Vec::new() }
    }
}

impl<T> RowCache<T> {
    /// Returns the cached value for row `index`, calling `build` only if the row is dirty.
    pub fn row(
        &mut self,
        index: usize,
        cells: &[GridCell],
        cursor_col: Option<usize>,
        build: impl FnOnce() -> T,
    ) -> &T {
        if self.rows.len() <= index {
            self.rows.resize_with(index + 1, || None);
        }
        let slot = &mut self.rows[index];
        let fresh = matches!(slot, Some(row) if row.cursor_col == cursor_col && row.cells == cells);
        if !fresh {
            *slot = None;
        }
        &slot
            .get_or_insert_with(|| CachedRow {
                cells: cells.to_vec(),
                cursor_col,
                value: build(),
            })
            .value
    }

    /// Drops rows past `len` (after the grid shrank).
    pub fn truncate(&mut self, len: usize) {
        self.rows.truncate(len);
    }

    /// Forgets everything (font or theme changed).
    pub fn clear(&mut self) {
        self.rows.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::emulator::Emulator;

    fn row(bytes: &[u8], cols: usize) -> Vec<GridCell> {
        let mut emu = Emulator::new(cols, 1);
        emu.advance(bytes);
        emu.grid().rows.remove(0)
    }

    fn style(fg: u32) -> CellStyle {
        CellStyle {
            fg,
            bg: Theme::dark().bg,
            bold: false,
            italic: false,
            underline: false,
        }
    }

    #[test]
    fn equal_styles_share_one_run_and_trailing_blanks_are_dropped() {
        let theme = Theme::dark();
        let layout = layout_row(&row(b"ab \x1b[31mcd\x1b[0m   ", 20), None, &theme);
        assert_eq!(layout.segments.len(), 1);
        let segment = &layout.segments[0];
        assert_eq!((segment.start_col, segment.text.as_str()), (0, "ab cd"));
        assert!(segment.grid_aligned);
        assert_eq!(
            segment.runs,
            vec![
                Run {
                    len: 3,
                    style: style(theme.text)
                },
                Run {
                    len: 2,
                    style: style(theme.ansi[1])
                },
            ]
        );
        assert!(layout.backgrounds.is_empty());
    }

    #[test]
    fn wide_chars_get_their_own_segment() {
        let layout = layout_row(&row("a日b".as_bytes(), 10), None, &Theme::dark());
        let segments: Vec<_> = layout
            .segments
            .iter()
            .map(|s| (s.start_col, s.text.as_str(), s.grid_aligned))
            .collect();
        assert_eq!(
            segments,
            vec![(0, "a", true), (1, "日", false), (3, "b", true)]
        );
    }

    #[test]
    fn zero_width_marks_stay_with_their_base_char() {
        let layout = layout_row(&row("xe\u{301}y".as_bytes(), 10), None, &Theme::dark());
        let texts: Vec<_> = layout.segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["x", "e\u{301}", "y"]);
        assert!(!layout.segments[1].grid_aligned);
        assert_eq!(layout.segments[2].start_col, 2);
    }

    #[test]
    fn backgrounds_merge_and_skip_the_default() {
        let theme = Theme::dark();
        let layout = layout_row(&row(b"x\x1b[44m  \x1b[45m \x1b[0m", 10), None, &theme);
        assert_eq!(
            layout.backgrounds,
            vec![
                Background {
                    start_col: 1,
                    cols: 2,
                    color: theme.ansi[4]
                },
                Background {
                    start_col: 3,
                    cols: 1,
                    color: theme.ansi[5]
                },
            ]
        );
    }

    #[test]
    fn cursor_on_wide_char_covers_both_cells() {
        let theme = Theme::dark();
        let layout = layout_row(&row("a日b".as_bytes(), 10), Some(1), &theme);
        assert_eq!(
            layout.backgrounds,
            vec![Background {
                start_col: 1,
                cols: 2,
                color: theme.accent
            }]
        );
    }

    #[test]
    fn cursor_cell_is_drawn_as_accent_block() {
        let theme = Theme::dark();
        let layout = layout_row(&row(b"ab", 10), Some(5), &theme);
        assert_eq!(
            layout.backgrounds,
            vec![Background {
                start_col: 5,
                cols: 1,
                color: theme.accent
            }]
        );
        let on_text = cell_style(&row(b"ab", 10)[1], true, &theme);
        assert_eq!((on_text.fg, on_text.bg), (theme.bg, theme.accent));
    }

    #[test]
    fn sgr_flags_change_colors_and_font() {
        let theme = Theme::dark();
        let cells = row(b"\x1b[1;3;4mA\x1b[0;7mB\x1b[0;8mC\x1b[0;2mD", 10);
        let a = cell_style(&cells[0], false, &theme);
        assert!(a.bold && a.italic && a.underline);
        let b = cell_style(&cells[1], false, &theme);
        assert_eq!((b.fg, b.bg), (theme.bg, theme.text));
        let c = cell_style(&cells[2], false, &theme);
        assert_eq!(c.fg, theme.bg);
        let d = cell_style(&cells[3], false, &theme);
        assert_eq!(d.fg, blend(theme.text, theme.bg, theme.dim_blend));
    }

    #[test]
    fn colors_resolve_through_theme_cube_and_ramp() {
        let theme = Theme::light();
        assert_eq!(
            resolve_color(Color::Named(NamedColor::BrightRed), &theme),
            theme.ansi[9]
        );
        assert_eq!(resolve_color(Color::Indexed(3), &theme), theme.ansi[3]);
        assert_eq!(resolve_color(Color::Indexed(16), &theme), 0x000000);
        assert_eq!(resolve_color(Color::Indexed(196), &theme), 0xFF0000);
        assert_eq!(resolve_color(Color::Indexed(231), &theme), 0xFFFFFF);
        assert_eq!(resolve_color(Color::Indexed(232), &theme), 0x080808);
        assert_eq!(resolve_color(Color::Indexed(255), &theme), 0xEEEEEE);
        assert_eq!(
            resolve_color(Color::Named(NamedColor::Background), &theme),
            theme.bg
        );
    }

    #[test]
    fn blend_mixes_channels() {
        assert_eq!(blend(0xFF0000, 0x000000, 0.5), 0x800000);
        assert_eq!(blend(0x123456, 0xABCDEF, 1.0), 0x123456);
    }

    #[test]
    fn row_cache_rebuilds_only_dirty_rows() {
        let mut cache = RowCache::default();
        let mut builds = 0;
        let a = row(b"a", 4);
        let b = row(b"b", 4);
        let mut get = |cache: &mut RowCache<u32>, index, cells: &[GridCell], cursor| {
            *cache.row(index, cells, cursor, || {
                builds += 1;
                builds
            })
        };
        assert_eq!(get(&mut cache, 0, &a, None), 1);
        assert_eq!(get(&mut cache, 1, &b, None), 2);
        assert_eq!(get(&mut cache, 0, &a, None), 1, "clean row reused");
        assert_eq!(get(&mut cache, 1, &a, None), 3, "content changed");
        assert_eq!(get(&mut cache, 1, &a, Some(0)), 4, "cursor moved in");
        cache.clear();
        assert_eq!(get(&mut cache, 0, &a, None), 5);
    }

    #[test]
    fn selected_cells_get_the_selection_background() {
        let theme = Theme::dark();
        let mut emu = Emulator::new(10, 1);
        emu.advance(b"a\x1b[7mb\x1b[0;90mc\x1b[0md");
        emu.start_selection(0, 1, Side::Left);
        emu.update_selection(0, 2, Side::Right);
        let cells = emu.grid().rows.remove(0);
        let layout = layout_row(&cells, None, &theme);
        assert_eq!(
            layout.backgrounds,
            vec![Background {
                start_col: 1,
                cols: 2,
                color: theme.terminal_selection_bg
            }]
        );
        // An inverse cell and a muted cell both read as body text on the selection.
        assert_eq!(layout.segments[0].runs.len(), 3);
        assert_eq!(layout.segments[0].runs[1].len, 2);
        assert_eq!(layout.segments[0].runs[1].style.fg, theme.text);
        // The cursor block wins over the selection.
        assert_eq!(cell_style(&cells[1], true, &theme).bg, theme.accent);
    }

    #[test]
    fn find_matches_get_match_backgrounds() {
        let theme = Theme::dark();
        let mut emu = Emulator::new(12, 1);
        emu.advance(b"ab ab \x1b[7mab\x1b[0m");
        assert!(emu.set_search("ab"));
        assert!(emu.search_next(true));
        let cells = emu.grid().rows.remove(0);
        let layout = layout_row(&cells, None, &theme);
        let bg = |start_col, color| Background {
            start_col,
            cols: 2,
            color,
        };
        assert_eq!(
            layout.backgrounds,
            vec![
                bg(0, theme.terminal_match_bg),
                bg(3, theme.terminal_match_bg),
                bg(6, theme.terminal_match_current_bg),
            ]
        );
        // The inverse match reads as body text too.
        assert_eq!(cell_style(&cells[6], false, &theme).fg, theme.text);
        // The selection wins over a match.
        emu.start_selection(0, 0, Side::Left);
        emu.update_selection(0, 0, Side::Right);
        let cells = emu.grid().rows.remove(0);
        assert_eq!(
            cell_style(&cells[0], false, &theme).bg,
            theme.terminal_selection_bg
        );
    }

    #[test]
    fn cell_at_maps_pixels_to_cells_and_sides() {
        let dims = (80, 24);
        assert_eq!(cell_at(0.0, 0.0, 10.0, 20.0, dims), (0, 0, Side::Left));
        assert_eq!(cell_at(14.0, 39.0, 10.0, 20.0, dims), (1, 1, Side::Left));
        assert_eq!(cell_at(16.0, 41.0, 10.0, 20.0, dims), (2, 1, Side::Right));
        // Outside the grid: clamp to the edge cells.
        assert_eq!(cell_at(-5.0, -5.0, 10.0, 20.0, dims), (0, 0, Side::Left));
        assert_eq!(
            cell_at(5000.0, 5000.0, 10.0, 20.0, dims),
            (23, 79, Side::Right)
        );
    }

    #[test]
    fn grid_size_fits_cells_inside_padding() {
        // 8 px padding each side; 10 x 20 px cells.
        assert_eq!(grid_size(816.0, 496.0, 8.0, 10.0, 20.0), (80, 24));
        assert_eq!(
            grid_size(825.0, 515.0, 8.0, 10.0, 20.0),
            (80, 24),
            "partial cells drop"
        );
        assert_eq!(grid_size(10.0, 10.0, 8.0, 10.0, 20.0), (MIN_COLS, MIN_ROWS));
    }
}
