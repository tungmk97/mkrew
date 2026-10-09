//! Grid → runs snapshot test (SPEC §10, §16): a daemon screen snapshot plus following output
//! go through the UI emulator and `layout_row`; the dump shows every shaped segment, its style
//! runs and the background rects, so batching regressions show up as snapshot diffs.

use std::fmt::Write as _;

use mkrew_protocol::{CellRun, CursorSnapshot, ScreenSnapshot, TermColor, TermModes};
use mkrew_ui::terminal::emulator::Emulator;
use mkrew_ui::terminal::layout::layout_row;
use mkrew_ui::theme::Theme;

fn run(text: &str, fg: TermColor, bg: TermColor, flags: u16) -> CellRun {
    CellRun {
        text: text.into(),
        fg,
        bg,
        flags,
    }
}

fn dump(emulator: &Emulator, theme: &Theme) -> String {
    let grid = emulator.grid();
    let mut out = String::new();
    for (index, cells) in grid.rows.iter().enumerate() {
        let cursor_col =
            (grid.cursor.visible && grid.cursor.row == index).then_some(grid.cursor.col);
        let layout = layout_row(cells, cursor_col, theme);
        if layout.segments.is_empty() && layout.backgrounds.is_empty() {
            continue;
        }
        let _ = writeln!(out, "row {index}");
        for segment in &layout.segments {
            let _ = writeln!(
                out,
                "  @{} {:?}{}",
                segment.start_col,
                segment.text,
                if segment.grid_aligned { "" } else { " (free)" }
            );
            for run in &segment.runs {
                let style = run.style;
                let _ = writeln!(
                    out,
                    "    {} bytes fg={:06X} bg={:06X}{}{}{}",
                    run.len,
                    style.fg,
                    style.bg,
                    if style.bold { " bold" } else { "" },
                    if style.italic { " italic" } else { "" },
                    if style.underline { " underline" } else { "" },
                );
            }
        }
        for bg in &layout.backgrounds {
            let _ = writeln!(out, "  bg @{}+{} {:06X}", bg.start_col, bg.cols, bg.color);
        }
    }
    out
}

#[test]
fn snapshot_and_output_batch_into_runs() {
    let snapshot = ScreenSnapshot {
        cols: 24,
        rows: 4,
        lines: vec![
            vec![
                run(
                    "✔ ",
                    TermColor::Indexed(2),
                    TermColor::Default,
                    CellRun::BOLD,
                ),
                run("build ok", TermColor::Default, TermColor::Default, 0),
            ],
            vec![
                run(
                    "日本",
                    TermColor::Default,
                    TermColor::Default,
                    CellRun::WIDE,
                ),
                run(" ", TermColor::Default, TermColor::Default, 0),
                run(
                    "warn",
                    TermColor::Rgb([1, 2, 3]),
                    TermColor::Indexed(3),
                    CellRun::UNDERLINE,
                ),
            ],
            vec![run(
                "> ",
                TermColor::Default,
                TermColor::Default,
                CellRun::DIM,
            )],
        ],
        history: vec![],
        cursor: CursorSnapshot {
            row: 2,
            col: 2,
            visible: true,
        },
        modes: TermModes::default(),
    };
    let mut emulator = Emulator::from_snapshot(&snapshot);
    emulator.advance(b"\x1b[7mls\x1b[0m");
    insta::assert_snapshot!(dump(&emulator, &Theme::dark()));
}
