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
    // Drains only when woken, like the UI (more than one drain budget of output, so a capped
    // drain must re-arm the wake). On a helper thread so a lost wake fails instead of hanging.
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut session = PtySession::spawn(cmd, 80, 24).unwrap();
        let wake = session.wake();
        let mut emu = Emulator::new(80, 24);
        let start = Instant::now();
        let mut bytes = 0usize;
        let exited = loop {
            let drained = session.drain(&mut emu).unwrap();
            bytes += drained.bytes;
            if let Some(code) = drained.exited {
                break code;
            }
            wake.recv_blocking().unwrap();
        };
        let elapsed = start.elapsed();
        // Output can arrive after the exit event.
        session
            .pump(&mut emu, Duration::from_millis(500), Duration::from_secs(5))
            .unwrap();
        let _ = done_tx.send((exited, bytes, elapsed, emu.grid().to_text()));
    });
    let (exited, bytes, elapsed, text) = done_rx
        .recv_timeout(Duration::from_secs(60))
        .expect("flood did not finish within 60 s");

    assert_eq!(exited, 0);
    assert!(bytes as u64 >= MIB * 1024 * 1024, "only {bytes} bytes");
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
