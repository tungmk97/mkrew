//! `--stats`: once a second, prints RAM, CPU, frames, output rate and key latency to stderr,
//! for the M0 measurements (SPEC §15). See `scripts/measure-spike.sh`.
//!
//! Key latency = key bytes written to the PTY → end of the first frame painted after new output
//! arrived (the echo). It excludes OS input dispatch and the GPU present, and includes the
//! frame tick that drains PTY output.

use std::time::{Duration, Instant};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

/// Counters for one report interval plus run totals.
#[derive(Debug)]
pub struct Stats {
    started: Instant,
    interval_start: Instant,
    frames: u32,
    bytes: u64,
    latencies: Vec<Duration>,
    last_paint: Option<Instant>,
    max_frame_gap: Duration,
    pending_key: Option<Instant>,
    output_since_key: bool,
    total_bytes: u64,
    total_latencies: Vec<Duration>,
    peak_rss: u64,
    cpu_samples: Vec<f32>,
}

impl Stats {
    pub fn new(now: Instant) -> Self {
        Self {
            started: now,
            interval_start: now,
            frames: 0,
            bytes: 0,
            latencies: Vec::new(),
            last_paint: None,
            max_frame_gap: Duration::ZERO,
            pending_key: None,
            output_since_key: false,
            total_bytes: 0,
            total_latencies: Vec::new(),
            peak_rss: 0,
            cpu_samples: Vec::new(),
        }
    }

    /// Key bytes were written to the PTY. While one key is in flight, later keys are ignored
    /// (a key that never echoes is closed by the next unrelated output: one outlier sample).
    pub fn key_sent(&mut self, now: Instant) {
        if self.pending_key.is_none() {
            self.pending_key = Some(now);
            self.output_since_key = false;
        }
    }

    /// `bytes` of PTY output were fed to the emulator.
    pub fn output(&mut self, bytes: usize) {
        self.bytes += bytes as u64;
        self.total_bytes += bytes as u64;
        if self.pending_key.is_some() && bytes > 0 {
            self.output_since_key = true;
        }
    }

    /// A frame finished painting.
    pub fn painted(&mut self, now: Instant) {
        self.frames += 1;
        if let Some(last) = self.last_paint {
            self.max_frame_gap = self.max_frame_gap.max(now - last);
        }
        self.last_paint = Some(now);
        if self.output_since_key
            && let Some(sent) = self.pending_key.take()
        {
            self.latencies.push(now - sent);
            self.output_since_key = false;
        }
    }

    /// The line for the interval ending `now` (`rss` bytes, `cpu` % of one core); starts the
    /// next interval.
    pub fn report(&mut self, now: Instant, rss: u64, cpu: f32) -> String {
        let secs = (now - self.interval_start).as_secs_f64().max(f64::EPSILON);
        let line = format!(
            "stats t={:.0}s rss={} cpu={cpu:.1}% frames={} out={:.1}MB/s frame_gap_max={} {}",
            (now - self.started).as_secs_f64(),
            mb(rss),
            self.frames,
            self.bytes as f64 / secs / MIB,
            ms(self.max_frame_gap),
            latency_summary(&mut self.latencies),
        );
        self.peak_rss = self.peak_rss.max(rss);
        self.cpu_samples.push(cpu);
        self.total_latencies.append(&mut self.latencies);
        self.interval_start = now;
        self.frames = 0;
        self.bytes = 0;
        self.max_frame_gap = Duration::ZERO;
        line
    }

    /// The line for the whole run up to the last `report`.
    pub fn summary(&mut self, now: Instant) -> String {
        let secs = (now - self.started).as_secs_f64();
        let cpu_mean = if self.cpu_samples.is_empty() {
            0.0
        } else {
            self.cpu_samples.iter().sum::<f32>() / self.cpu_samples.len() as f32
        };
        format!(
            "stats total t={secs:.1}s rss_peak={} cpu_mean={cpu_mean:.1}% out={:.1}MB {}",
            mb(self.peak_rss),
            self.total_bytes as f64 / MIB,
            latency_summary(&mut self.total_latencies),
        )
    }
}

const MIB: f64 = 1024.0 * 1024.0;

fn mb(bytes: u64) -> String {
    format!("{:.1}MB", bytes as f64 / MIB)
}

fn ms(duration: Duration) -> String {
    format!("{:.1}ms", duration.as_secs_f64() * 1000.0)
}

/// `key_p50=… key_p95=… key_max=… keys=n`, or `keys=0`. Sorts `latencies`.
fn latency_summary(latencies: &mut [Duration]) -> String {
    if latencies.is_empty() {
        return "keys=0".to_owned();
    }
    latencies.sort();
    let at = |q: f64| latencies[((latencies.len() - 1) as f64 * q).round() as usize];
    format!(
        "key_p50={} key_p95={} key_max={} keys={}",
        ms(at(0.5)),
        ms(at(0.95)),
        ms(at(1.0)),
        latencies.len()
    )
}

/// Samples this process's resident memory and CPU usage.
pub struct ProcessProbe {
    system: System,
    pid: Option<Pid>,
}

impl ProcessProbe {
    pub fn new() -> Self {
        let mut probe = Self {
            system: System::new(),
            pid: sysinfo::get_current_pid().ok(),
        };
        // The first refresh is the CPU baseline.
        probe.sample();
        probe
    }

    /// Resident memory in bytes (Windows: working set) and CPU % of one core since the last
    /// sample; zeros if the platform is unsupported.
    pub fn sample(&mut self) -> (u64, f32) {
        let Some(pid) = self.pid else {
            return (0, 0.0);
        };
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            ProcessRefreshKind::new().with_cpu().with_memory(),
        );
        self.system
            .process(pid)
            .map_or((0, 0.0), |process| (process.memory(), process.cpu_usage()))
    }
}

impl Default for ProcessProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn key_latency_ends_at_first_paint_after_output() {
        let t0 = Instant::now();
        let mut stats = Stats::new(t0);
        stats.key_sent(t0);
        stats.key_sent(t0 + MS); // ignored: a key is already in flight
        stats.painted(t0 + 2 * MS); // no echo yet
        stats.output(1);
        stats.painted(t0 + 5 * MS);
        stats.painted(t0 + 9 * MS); // nothing pending any more
        let line = stats.report(t0 + 1000 * MS, 50 * 1024 * 1024, 0.5);
        assert!(line.contains("key_p50=5.0ms"), "{line}");
        assert!(line.contains("keys=1"), "{line}");
        assert!(line.contains("frames=3"), "{line}");
        assert!(line.contains("frame_gap_max=4.0ms"), "{line}");
        assert!(line.contains("rss=50.0MB cpu=0.5%"), "{line}");
    }

    #[test]
    fn output_before_the_key_does_not_count_as_echo() {
        let t0 = Instant::now();
        let mut stats = Stats::new(t0);
        stats.output(10);
        stats.key_sent(t0 + MS);
        stats.painted(t0 + 2 * MS);
        let line = stats.report(t0 + 1000 * MS, 0, 0.0);
        assert!(line.contains("keys=0"), "{line}");
    }

    #[test]
    fn report_resets_the_interval_and_summary_keeps_totals() {
        let t0 = Instant::now();
        let mut stats = Stats::new(t0);
        stats.output(2 * 1024 * 1024);
        let first = stats.report(t0 + 1000 * MS, 10 * 1024 * 1024, 4.0);
        assert!(first.contains("out=2.0MB/s"), "{first}");
        for i in 0..4u32 {
            let at = t0 + (1100 + 100 * i) * MS;
            stats.key_sent(at);
            stats.output(1);
            stats.painted(at + (i + 1) * MS);
        }
        let second = stats.report(t0 + 2000 * MS, 20 * 1024 * 1024, 2.0);
        assert!(second.contains("out=0.0MB/s"), "{second}");
        assert!(second.contains("key_p50=3.0ms key_p95=4.0ms key_max=4.0ms keys=4"));
        let total = stats.summary(t0 + 2000 * MS);
        assert!(total.contains("rss_peak=20.0MB cpu_mean=3.0%"), "{total}");
        assert!(total.contains("out=2.0MB key_p50=3.0ms"), "{total}");
        assert!(total.ends_with("keys=4"), "{total}");
    }

    #[test]
    fn probe_reports_own_memory() {
        let (rss, _cpu) = ProcessProbe::new().sample();
        assert!(rss > 0);
    }
}
