//! M0 spike: terminal-in-GPUI (SPEC §18).

use std::time::Duration;

use anyhow::Result;
use portable_pty::CommandBuilder;
use spike_term::emulator::Emulator;
use spike_term::pty::PtySession;

const COLS: u16 = 80;
const ROWS: u16 = 24;

const USAGE: &str = "usage: spike-term          run the default shell in a window
       spike-term --dump   run the default shell headless and print its screen";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => window(),
        [flag] if flag == "--dump" => dump(),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

/// Spawns the default shell and renders it in a GPUI window.
fn window() -> Result<()> {
    let session = PtySession::spawn(CommandBuilder::new_default_prog(), COLS, ROWS)?;
    spike_term::view::run(session, Emulator::new(COLS.into(), ROWS.into()));
    Ok(())
}

/// Spawns the default shell, waits until its output settles, prints the screen as text.
fn dump() -> Result<()> {
    let mut session = PtySession::spawn(CommandBuilder::new_default_prog(), COLS, ROWS)?;
    let mut emulator = Emulator::new(COLS.into(), ROWS.into());
    session.pump(
        &mut emulator,
        Duration::from_millis(1500),
        Duration::from_secs(10),
    )?;
    print!("{}", emulator.grid().to_text());
    Ok(())
}
