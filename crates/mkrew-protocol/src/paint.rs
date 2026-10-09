//! Turns a daemon [`ScreenSnapshot`] (SPEC §7) into ANSI escape sequences, so a client terminal
//! (`mk attach`, or the UI's own emulator) shows the current screen before raw output frames
//! continue from it.

use std::fmt::Write;

use crate::{CellRun, ScreenSnapshot, TermColor};

/// Escape sequences that redraw `snapshot` from scratch and apply its cursor and modes.
pub fn paint(snapshot: &ScreenSnapshot) -> String {
    let modes = &snapshot.modes;
    let mut out = String::from("\x1b[0m");
    out.push_str(if modes.alt_screen {
        "\x1b[?1049h"
    } else {
        "\x1b[?1049l"
    });
    out.push_str("\x1b[H\x1b[2J");
    // History: write each line from the top and let line feeds scroll it off the screen, so the
    // receiving terminal keeps it as scrollback. `rows - 1` extra feeds push the last written
    // line out too; the screen is blank again for the absolute rows below. The alternate
    // screen has no scrollback (the daemon sends none there; skip it anyway).
    if !modes.alt_screen && !snapshot.history.is_empty() {
        for runs in &snapshot.history {
            push_runs(&mut out, runs);
            out.push_str("\r\n");
        }
        for _ in 1..snapshot.rows {
            out.push('\n');
        }
    }
    for (row, runs) in snapshot.lines.iter().enumerate() {
        if runs.is_empty() {
            continue;
        }
        let _ = write!(out, "\x1b[{};1H", row + 1);
        push_runs(&mut out, runs);
    }
    // Mouse: the snapshot only says "some mode is on"; button-event tracking is the common one.
    for (on, set) in [
        (modes.app_cursor, "?1"),
        (modes.bracketed_paste, "?2004"),
        (modes.mouse, "?1002"),
        (modes.sgr_mouse, "?1006"),
        (modes.focus_events, "?1004"),
    ] {
        let _ = write!(out, "\x1b[{set}{}", if on { 'h' } else { 'l' });
    }
    out.push_str(if modes.app_keypad { "\x1b=" } else { "\x1b>" });
    let cursor = &snapshot.cursor;
    let _ = write!(out, "\x1b[{};{}H", cursor.row + 1, cursor.col + 1);
    out.push_str(if cursor.visible {
        "\x1b[?25h"
    } else {
        "\x1b[?25l"
    });
    out
}

/// Puts the user's terminal back into a sane state after detaching.
pub const RESET: &str =
    "\x1b[0m\x1b[?1049l\x1b[?1l\x1b[?2004l\x1b[?1002l\x1b[?1006l\x1b[?1004l\x1b>\x1b[?25h";

fn sgr(out: &mut String, run: &CellRun) {
    out.push_str("\x1b[0");
    for (flag, code) in [
        (CellRun::BOLD, 1),
        (CellRun::DIM, 2),
        (CellRun::ITALIC, 3),
        (CellRun::UNDERLINE, 4),
        (CellRun::INVERSE, 7),
        (CellRun::HIDDEN, 8),
        (CellRun::STRIKEOUT, 9),
    ] {
        if run.flags & flag != 0 {
            let _ = write!(out, ";{code}");
        }
    }
    color(out, run.fg, 30, 90, 38);
    color(out, run.bg, 40, 100, 48);
    out.push('m');
}

/// One row's styled runs, then the style reset.
fn push_runs(out: &mut String, runs: &[CellRun]) {
    for run in runs {
        sgr(out, run);
        out.push_str(&run.text);
    }
    out.push_str("\x1b[0m");
}

/// `base` + n for ANSI 0–7, `bright` + n − 8 for 8–15, `extended;5;n` / `extended;2;r;g;b` else.
fn color(out: &mut String, color: TermColor, base: u8, bright: u8, extended: u8) {
    let _ = match color {
        TermColor::Default => Ok(()),
        TermColor::Indexed(n @ 0..=7) => write!(out, ";{}", base + n),
        TermColor::Indexed(n @ 8..=15) => write!(out, ";{}", bright + n - 8),
        TermColor::Indexed(n) => write!(out, ";{extended};5;{n}"),
        TermColor::Rgb([r, g, b]) => write!(out, ";{extended};2;{r};{g};{b}"),
    };
}

#[cfg(test)]
mod tests {
    use crate::{CursorSnapshot, TermModes};

    use super::*;

    fn run(text: &str, fg: TermColor, bg: TermColor, flags: u16) -> CellRun {
        CellRun {
            text: text.into(),
            fg,
            bg,
            flags,
        }
    }

    #[test]
    fn paints_runs_cursor_and_modes() {
        let snapshot = ScreenSnapshot {
            cols: 20,
            rows: 3,
            lines: vec![
                vec![
                    run(
                        "ok",
                        TermColor::Indexed(2),
                        TermColor::Default,
                        CellRun::BOLD,
                    ),
                    run(" plain", TermColor::Default, TermColor::Default, 0),
                ],
                vec![],
                vec![run(
                    "x",
                    TermColor::Rgb([1, 2, 3]),
                    TermColor::Indexed(9),
                    CellRun::UNDERLINE | CellRun::INVERSE,
                )],
            ],
            history: vec![],
            cursor: CursorSnapshot {
                row: 2,
                col: 1,
                visible: false,
            },
            modes: TermModes {
                alt_screen: true,
                bracketed_paste: true,
                ..TermModes::default()
            },
        };
        let expected = concat!(
            "\x1b[0m\x1b[?1049h\x1b[H\x1b[2J",
            "\x1b[1;1H\x1b[0;1;32mok\x1b[0m plain\x1b[0m",
            "\x1b[3;1H\x1b[0;4;7;38;2;1;2;3;101mx\x1b[0m",
            "\x1b[?1l\x1b[?2004h\x1b[?1002l\x1b[?1006l\x1b[?1004l\x1b>",
            "\x1b[3;2H\x1b[?25l",
        );
        assert_eq!(paint(&snapshot), expected);
    }

    /// History goes before the screen and only on the main screen; it never changes how the
    /// screen itself is painted.
    #[test]
    fn history_is_painted_only_on_the_main_screen() {
        let mut snapshot = ScreenSnapshot {
            cols: 20,
            rows: 3,
            lines: vec![vec![run("$ ", TermColor::Default, TermColor::Default, 0)]],
            history: vec![],
            cursor: CursorSnapshot {
                row: 0,
                col: 2,
                visible: true,
            },
            modes: TermModes::default(),
        };
        let without = paint(&snapshot);
        snapshot.history = vec![
            vec![run("old", TermColor::Indexed(1), TermColor::Default, 0)],
            vec![],
        ];
        // Two history lines, then rows - 1 = 2 feeds, then the same screen as before.
        assert_eq!(
            paint(&snapshot),
            without.replacen(
                "\x1b[2J",
                "\x1b[2J\x1b[0;31mold\x1b[0m\r\n\x1b[0m\r\n\n\n",
                1
            )
        );
        snapshot.modes.alt_screen = true;
        let alt = paint(&snapshot);
        snapshot.history.clear();
        assert_eq!(alt, paint(&snapshot));
    }

    #[test]
    fn extended_indexed_colors() {
        let mut out = String::new();
        color(&mut out, TermColor::Indexed(7), 30, 90, 38);
        color(&mut out, TermColor::Indexed(8), 30, 90, 38);
        color(&mut out, TermColor::Indexed(200), 40, 100, 48);
        assert_eq!(out, ";37;90;48;5;200");
    }
}
