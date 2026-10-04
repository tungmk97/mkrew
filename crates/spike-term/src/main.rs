//! M0 spike: terminal-in-GPUI (SPEC §18).

use std::time::Duration;

use anyhow::Result;
use portable_pty::CommandBuilder;
use spike_term::emulator::Emulator;
use spike_term::launch::{self, Invocation};
use spike_term::pty::PtySession;

const COLS: u16 = 80;
const ROWS: u16 = 24;

const USAGE: &str = "usage: spike-term [--dump] [-- <command> [args]]
  (no args)            run the default shell in a window
  --dump               run headless and print the screen instead
  -- <command> [args]  run <command> (e.g. claude, codex, omp) instead of the shell";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(Invocation { dump, command }) = launch::parse_args(&args) else {
        eprintln!("{USAGE}");
        std::process::exit(2);
    };
    let command = match command {
        Some(argv) => launch::command(&argv)?,
        None => CommandBuilder::new_default_prog(),
    };
    if dump {
        run_dump(command)
    } else {
        window(command)
    }
}

/// Spawns `command` and renders it in a GPUI window.
fn window(command: CommandBuilder) -> Result<()> {
    let session = PtySession::spawn(command, COLS, ROWS)?;
    spike_term::view::run(session, Emulator::new(COLS.into(), ROWS.into()));
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
