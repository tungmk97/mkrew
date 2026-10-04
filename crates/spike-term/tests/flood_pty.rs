//! `spike-term --flood` through a real PTY, without the UI: the output reaches the grid and the
//! headless throughput is printed (`cargo test -p spike-term --test flood_pty -- --nocapture`),
//! so the UI's flood numbers (scripts/measure-spike.sh) can be compared to the PTY's own limit.

use std::time::{Duration, Instant};

use portable_pty::CommandBuilder;
use spike_term::emulator::Emulator;
use spike_term::pty::PtySession;

#[test]
fn flood_output_reaches_the_grid_through_the_pty() {
    const MIB: u64 = 1;
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_spike-term"));
    cmd.args(["--flood", &MIB.to_string()]);
    cmd.cwd(std::env::temp_dir());
    let mut session = PtySession::spawn(cmd, 80, 24).unwrap();
    let mut emu = Emulator::new(80, 24);

    let start = Instant::now();
    let end = start + Duration::from_secs(60);
    let mut bytes = 0usize;
    let mut exited = None;
    while exited.is_none() && Instant::now() < end {
        let drained = session.drain(&mut emu).unwrap();
        bytes += drained.bytes;
        exited = drained.exited;
        if !drained.output {
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    let elapsed = start.elapsed();
    // Output can arrive after the exit event.
    session
        .pump(&mut emu, Duration::from_millis(500), Duration::from_secs(5))
        .unwrap();

    assert_eq!(exited, Some(0), "flood did not finish within 60 s");
    let text = emu.grid().to_text();
    assert!(
        text.contains(" flood  the quick brown fox"),
        "grid was:\n{text}"
    );
    println!(
        "pty flood: {bytes} bytes in {:.2}s = {:.2} MB/s (headless, no UI)",
        elapsed.as_secs_f64(),
        bytes as f64 / elapsed.as_secs_f64() / (1024.0 * 1024.0)
    );
}
