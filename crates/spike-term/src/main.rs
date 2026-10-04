//! M0 spike: terminal-in-GPUI (SPEC §18).

use std::time::Duration;

use anyhow::Result;
use portable_pty::CommandBuilder;
use spike_term::emulator::Emulator;
use spike_term::launch::{self, Invocation, Mode};
use spike_term::pty::PtySession;
use spike_term::view::Measure;

const COLS: u16 = 80;
const ROWS: u16 = 24;

const USAGE: &str = "usage: spike-term [--dump | --stats [--probe-keys]] [-- <command> [args]]
       spike-term --flood <MB>
  (no args)            run the default shell in a window
  --dump               run headless and print the screen instead
  --stats              print RAM/CPU/frames/key latency to stderr every second
  --probe-keys         with --stats: type a synthetic key every 500 ms to measure latency
  --flood <MB>         print <MB> MiB of colored output to stdout and exit
  -- <command> [args]  run <command> (e.g. claude, codex, omp) instead of the shell";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(Invocation { mode, command }) = launch::parse_args(&args) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let window_measure = match mode {
        Mode::Flood { megabytes } => {
            // stdout is line-buffered; without this every flood line is its own write.
            let mut out = std::io::BufWriter::with_capacity(64 * 1024, std::io::stdout().lock());
            spike_term::flood::write_flood(&mut out, megabytes.saturating_mul(1024 * 1024))?;
            return Ok(());
        }
        Mode::Dump => None,
        Mode::Window { stats, probe_keys } => Some(Measure { stats, probe_keys }),
    };
    let command = match command {
        Some(argv) => launch::command(&argv)?,
        None => CommandBuilder::new_default_prog(),
    };
    match window_measure {
        Some(measure) => window(command, measure),
        None => run_dump(command),
    }
}

/// Spawns `command` and renders it in a GPUI window.
fn window(command: CommandBuilder, measure: Measure) -> Result<()> {
    let session = PtySession::spawn(command, COLS, ROWS)?;
    spike_term::view::run(session, Emulator::new(COLS.into(), ROWS.into()), measure);
    Ok(())
}

/// Spawns `command`, waits until its output settles, prints the screen as text.
fn run_dump(command: CommandBuilder) -> Result<()> {
    let mut session = PtySession::spawn(command, COLS, ROWS)?;
    let mut emulator = Emulator::new(COLS.into(), ROWS.into());
    session.pump(
        &mut emulator,
        Duration::from_millis(1500),
        Duration::from_secs(10),
    )?;
    print!("{}", emulator.grid().to_text());
    Ok(())
}
