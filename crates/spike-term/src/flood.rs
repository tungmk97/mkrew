//! `--flood`: prints high-volume colored output as fast as possible, for the throughput
//! measurement (SPEC §15 "Throughput"). Run it inside the window:
//! `spike-term --stats -- spike-term --flood 50`.

use std::fmt::Write as _;
use std::io::{self, Write};

/// Writes colored lines to `out` until at least `total_bytes` are written; returns the count.
/// Each line mixes 256-color fg/bg, bold and plain text and stays under 80 visible columns.
pub fn write_flood(out: &mut impl Write, total_bytes: u64) -> io::Result<u64> {
    let mut written = 0u64;
    let mut line = String::new();
    let mut line_no = 0u64;
    while written < total_bytes {
        line.clear();
        flood_line(line_no, &mut line);
        out.write_all(line.as_bytes())?;
        written += line.len() as u64;
        line_no += 1;
    }
    out.flush()?;
    Ok(written)
}

/// One flood line, numbered `line_no` (deterministic, so the output is reproducible).
fn flood_line(line_no: u64, line: &mut String) {
    let color = line_no % 256;
    let bg = (line_no * 7 + 16) % 256;
    // Writing into a String cannot fail.
    let _ = write!(
        line,
        "\x1b[38;5;{color}m{line_no:>8}\x1b[0m \x1b[1;48;5;{bg}m flood \x1b[0m \
         \x1b[32mthe quick brown fox\x1b[0m \x1b[3;35mjumps over\x1b[0m the lazy dog\r\n"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emulator::Emulator;

    #[test]
    fn writes_at_least_the_requested_bytes() {
        let mut out = Vec::new();
        let written = write_flood(&mut out, 10_000).unwrap();
        assert_eq!(written, out.len() as u64);
        assert!(written >= 10_000);
        let mut one_line = String::new();
        flood_line(0, &mut one_line);
        assert!(written < 10_000 + one_line.len() as u64 * 2);
    }

    #[test]
    fn output_renders_as_colored_lines_within_80_columns() {
        let mut out = Vec::new();
        write_flood(&mut out, 1).unwrap();
        let mut emu = Emulator::new(80, 3);
        emu.advance(&out);
        let grid = emu.grid();
        let text = grid.to_text();
        let first = text.lines().next().unwrap();
        assert_eq!(
            first.trim_end(),
            "       0  flood  the quick brown fox jumps over the lazy dog"
        );
        assert!(first.trim_end().chars().count() < 80);
        assert_ne!(grid.rows[0][20].fg, grid.rows[0][45].fg);
    }
}
