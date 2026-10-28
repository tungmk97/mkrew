//! Rate limit and change gate of the per-session screen scan (ADR 0088). Pure: the caller
//! passes `now`.

use std::time::{Duration, Instant};

/// Two scans of one session are at least this far apart: at most 4 a second (ADR 0088).
pub const SCAN_GAP: Duration = Duration::from_millis(250);

/// When a session's screen is scanned: right away after a quiet gap, else once the gap since
/// the last scan has passed (the trailing scan, so the final screen is always seen).
#[derive(Debug)]
pub struct ScanRate {
    gap: Duration,
    last: Option<Instant>,
    /// Output arrived inside the gap: a scan is owed at `last + gap`.
    pending: bool,
}

impl ScanRate {
    pub const fn new(gap: Duration) -> Self {
        Self {
            gap,
            last: None,
            pending: false,
        }
    }

    /// Output arrived at `now`: `true` = scan the screen now. `urgent` (a `session.send` waits
    /// for the state) always scans.
    pub fn on_output(&mut self, now: Instant, urgent: bool) -> bool {
        let due = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= self.gap);
        if due || urgent {
            self.scanned(now);
            true
        } else {
            self.pending = true;
            false
        }
    }

    /// When the owed trailing scan is due; `None` when nothing is held back.
    pub fn deadline(&self) -> Option<Instant> {
        if self.pending {
            self.last.map(|last| last + self.gap)
        } else {
            None
        }
    }

    /// A scan ran at `now` (the trailing one included).
    pub fn scanned(&mut self, now: Instant) {
        self.last = Some(now);
        self.pending = false;
    }

    /// Takes the owed scan: `true` if there was one (the session ended before it fired).
    pub fn take_pending(&mut self) -> bool {
        std::mem::take(&mut self.pending)
    }
}

/// ADR 0015 at scan time: `true` when the visible text changed since the previous scan
/// (`print` holds that scan's fingerprint; it is updated). Only visible changes are work: idle
/// TUIs keep sending cursor/title/progress bytes, which would hold a session in `working`.
/// `None` is "no baseline": it counts for a `Starting` session (hook-only profiles still leave
/// `starting`), but after a stretch of skipped scans (the caller reset `print`) it only sets
/// the baseline.
pub fn changed(print: &mut Option<u64>, now_print: u64, starting: bool) -> bool {
    let before = print.replace(now_print);
    before != Some(now_print) && (before.is_some() || starting)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use mkrew_core::state::PATTERN_TAIL_LINES;
    use mkrew_core::{StateConfig, StateMachine, StateMode};
    use mkrew_protocol::SessionState;

    use super::*;
    use crate::emulator::Emulator;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn the_first_output_scans_at_once_and_a_close_one_waits_for_the_trailing_scan() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(ms(250));
        assert!(rate.on_output(t0, false));
        assert_eq!(rate.deadline(), None);
        assert!(!rate.on_output(t0 + ms(10), false));
        assert_eq!(rate.deadline(), Some(t0 + ms(250)));
        // The gap runs from the last scan, not from the last output.
        assert!(!rate.on_output(t0 + ms(249), false));
        assert_eq!(rate.deadline(), Some(t0 + ms(250)));
        rate.scanned(t0 + ms(250));
        assert_eq!(rate.deadline(), None);
        assert!(!rate.take_pending());
        // Quiet for a whole gap: the next output scans at once again.
        assert!(rate.on_output(t0 + ms(500), false));
    }

    /// 300 batches at 16 ms (about 60/s): never two scans closer than the gap, and the
    /// last batch is always followed by a scan within one gap.
    #[test]
    fn a_flood_scans_at_most_four_times_a_second_and_the_last_batch_is_scanned() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        let mut scans = Vec::new();
        let mut last_batch = t0;
        for i in 0..300 {
            let now = t0 + ms(i * 16);
            // The trailing scan fires between two batches when it is due.
            while let Some(due) = rate.deadline().filter(|due| *due <= now) {
                rate.scanned(due);
                scans.push(due);
            }
            if rate.on_output(now, false) {
                scans.push(now);
            }
            last_batch = now;
        }
        let due = rate.deadline().expect("the last batch is owed a scan");
        assert!(due <= last_batch + SCAN_GAP);
        rate.scanned(due);
        scans.push(due);
        for pair in scans.windows(2) {
            assert!(pair[1] - pair[0] >= SCAN_GAP, "{pair:?}");
        }
        // 4.8 s of output.
        assert!(scans.len() <= 21, "{} scans", scans.len());
    }

    /// A queued `session.send` is delivered when the state allows it; the state must be
    /// current then, so a waiting send scans every batch.
    #[test]
    fn a_waiting_send_is_never_held_back() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        assert!(rate.on_output(t0, false));
        assert!(rate.on_output(t0 + ms(1), true));
        assert_eq!(rate.deadline(), None);
    }

    #[test]
    fn an_exit_can_claim_the_owed_scan_once() {
        let t0 = Instant::now();
        let mut rate = ScanRate::new(SCAN_GAP);
        rate.on_output(t0, false);
        rate.on_output(t0 + ms(5), false);
        assert!(rate.take_pending());
        assert!(!rate.take_pending());
        assert_eq!(rate.deadline(), None);
    }

    fn machine() -> StateMachine {
        StateMachine::new(&StateConfig {
            mode: StateMode::Screen,
            idle_after_ms: 3000,
            needs_input_patterns: vec![r"(?i)do you want to".into()],
        })
        .unwrap()
    }

    /// What `Actor::scan_screen` feeds the machine: the tail of the real emulator.
    fn scan(emu: &Emulator, machine: &mut StateMachine, now: Instant) -> Option<SessionState> {
        let tail = emu.tail_to_cursor(PATTERN_TAIL_LINES);
        let tail: Vec<&str> = tail.iter().map(String::as_str).collect();
        machine.on_output(now, &tail)
    }

    /// The agent asks 100 ms after the last scan and then stays silent: the prompt must still
    /// become `needs_input` (at the trailing scan), never be missed.
    #[test]
    fn a_prompt_that_lands_inside_the_gap_is_classified_by_the_trailing_scan() {
        let t0 = Instant::now();
        let mut emu = Emulator::new(40, 6);
        let mut machine = machine();
        let mut rate = ScanRate::new(SCAN_GAP);
        emu.advance(b"Compiling...\r\n");
        assert!(rate.on_output(t0, false));
        assert_eq!(scan(&emu, &mut machine, t0), Some(SessionState::Working));

        emu.advance(b"Do you want to proceed?\r\n");
        assert!(!rate.on_output(t0 + ms(100), false));
        assert_eq!(machine.state(), SessionState::Working);

        let due = rate.deadline().expect("a trailing scan is owed");
        assert_eq!(due, t0 + SCAN_GAP);
        rate.scanned(due);
        assert_eq!(
            scan(&emu, &mut machine, due),
            Some(SessionState::NeedsInput)
        );
    }

    /// ADR 0015 across a gap: bytes that change nothing visible (cursor, title, identical
    /// repaint) are not activity at the next scan; a changed row is.
    #[test]
    fn the_gate_counts_visible_changes_between_two_scans_only() {
        let mut emu = Emulator::new(20, 4);
        let mut print = None;
        emu.advance(b"hello");
        // First scan of a `Starting` session counts.
        assert!(changed(&mut print, emu.text_fingerprint(), true));
        emu.advance(b"\x1b[?25l\x1b]0;t\x07\x1b[H\x1b[2Jhello\x1b[?25h");
        assert!(!changed(&mut print, emu.text_fingerprint(), false));
        // `x` then back to `hello` inside one gap: not seen (documented in ADR 0088).
        emu.advance(b"\x1b[Hx");
        emu.advance(b"\x1b[Hh");
        assert!(!changed(&mut print, emu.text_fingerprint(), false));
        emu.advance(b"!");
        assert!(changed(&mut print, emu.text_fingerprint(), false));
    }

    /// After a stretch where the scan was skipped (a hook fired, `print` reset to `None`)
    /// the first scan of a session that left `Starting` only sets the baseline: a `done`
    /// session must not turn `working` just because its screen moved while nobody looked.
    #[test]
    fn the_first_scan_after_a_skipped_stretch_is_a_baseline() {
        let mut print = None;
        assert!(!changed(&mut print, 7, false));
        assert_eq!(print, Some(7));
        assert!(!changed(&mut print, 7, false));
        assert!(changed(&mut print, 8, false));
    }
}
