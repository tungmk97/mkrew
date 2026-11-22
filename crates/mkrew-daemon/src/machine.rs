//! CPU and RAM of this machine for the widget bar (spec 2026-11-22 section 6). A sampler task
//! reads `sysinfo` every [`SAMPLE_EVERY`] while at least one client is connected and announces
//! `machine.stats` when a value moved enough ([`should_send`]) or [`HEARTBEAT`] has passed.
//! With no client connected the task waits and does no work.
//!
//! Every connection that said `hello` counts, `mk web`'s relay included; a short-lived `mk`
//! call connects and leaves before the first sample is due.

use std::sync::Arc;
use std::time::Duration;

use mkrew_protocol::{Event, MachineStats};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};

use crate::session::Hub;

pub const SAMPLE_EVERY: Duration = Duration::from_secs(2);
pub const HEARTBEAT: Duration = Duration::from_secs(10);
/// CPU moves of at least this many percentage points are news.
const CPU_STEP: f32 = 1.0;
/// RAM moves of at least this many bytes (100 MiB) are news.
const RAM_STEP: u64 = 100 * 1024 * 1024;

/// Whether `now` is sent after `last` (what was last sent, `since_last` ago): the first reading,
/// a CPU move of 1 point, a RAM move of 100 MB, another RAM total, or [`HEARTBEAT`] without
/// news. Compared with what was last SENT, so slow drift still gets through.
pub fn should_send(last: Option<&MachineStats>, since_last: Duration, now: &MachineStats) -> bool {
    let Some(last) = last else {
        return true;
    };
    since_last >= HEARTBEAT
        || (now.cpu_percent - last.cpu_percent).abs() >= CPU_STEP
        || now.ram_used.abs_diff(last.ram_used) >= RAM_STEP
        || now.ram_total != last.ram_total
}

/// One connected client; the sampler runs while any exists.
pub struct ClientGuard(Arc<Hub>);

impl ClientGuard {
    pub fn join(hub: &Arc<Hub>) -> Self {
        hub.clients.send_modify(|count| *count += 1);
        Self(Arc::clone(hub))
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0
            .clients
            .send_modify(|count| *count = count.saturating_sub(1));
    }
}

fn read(system: &mut sysinfo::System) -> MachineStats {
    system.refresh_cpu_usage();
    system.refresh_memory();
    MachineStats {
        cpu_percent: system.global_cpu_usage(),
        ram_used: system.used_memory(),
        ram_total: system.total_memory(),
    }
}

/// The sampler task; runs until aborted.
pub fn start(hub: Arc<Hub>) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut clients = hub.clients.subscribe();
        let mut system = sysinfo::System::new();
        loop {
            // Idle: nothing runs until a client is connected.
            while *clients.borrow_and_update() == 0 {
                if clients.changed().await.is_err() {
                    return;
                }
            }
            // A CPU reading needs an earlier one to compare with.
            system.refresh_cpu_usage();
            let mut ticks = tokio::time::interval_at(Instant::now() + SAMPLE_EVERY, SAMPLE_EVERY);
            ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
            let mut last: Option<(MachineStats, Instant)> = None;
            loop {
                let at = tokio::select! {
                    at = ticks.tick() => at,
                    changed = clients.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        if *clients.borrow_and_update() == 0 {
                            break;
                        }
                        // A client that joined needs the figures at the next sample, not at
                        // the next heartbeat.
                        last = None;
                        continue;
                    }
                };
                let now = read(&mut system);
                let since = last.as_ref().map_or(Duration::MAX, |(_, sent)| {
                    at.saturating_duration_since(*sent)
                });
                if should_send(last.as_ref().map(|(stats, _)| stats), since, &now) {
                    last = Some((now, at));
                    let _ = hub.events.send(Event::MachineStats(now));
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_store::Store;

    const MB: u64 = 1024 * 1024;

    fn stats(cpu: f32, used_mb: u64) -> MachineStats {
        MachineStats {
            cpu_percent: cpu,
            ram_used: used_mb * MB,
            ram_total: 32 * 1024 * MB,
        }
    }

    #[test]
    fn the_first_reading_is_always_sent() {
        assert!(should_send(None, Duration::ZERO, &stats(0.0, 0)));
    }

    #[test]
    fn a_small_change_waits_for_the_heartbeat() {
        let last = stats(23.0, 1000);
        let two = Duration::from_secs(2);
        assert!(!should_send(Some(&last), two, &stats(23.5, 1050)));
        assert!(
            !should_send(Some(&last), two, &stats(23.99, 1000)),
            "just under a point"
        );
        assert!(should_send(Some(&last), two, &stats(24.0, 1000)), "a point");
        assert!(
            should_send(Some(&last), two, &stats(22.0, 1000)),
            "down counts too"
        );
        assert!(
            !should_send(Some(&last), two, &stats(23.0, 1099)),
            "just under 100 MB"
        );
        assert!(should_send(Some(&last), two, &stats(23.0, 1100)), "100 MB");
        assert!(
            should_send(Some(&last), two, &stats(23.0, 900)),
            "freed RAM counts too"
        );
        assert!(!should_send(
            Some(&last),
            Duration::from_millis(9_900),
            &last
        ));
        assert!(should_send(Some(&last), HEARTBEAT, &last), "the heartbeat");
        let other_total = MachineStats {
            ram_total: 16 * 1024 * MB,
            ..last
        };
        assert!(should_send(Some(&last), two, &other_total), "another total");
    }

    #[test]
    fn the_client_count_follows_the_guards() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Arc::new(Hub::new(Store::open(&dir.path().join("mkrew.db")).unwrap()));
        assert_eq!(*hub.clients.borrow(), 0);
        let (a, b) = (ClientGuard::join(&hub), ClientGuard::join(&hub));
        assert_eq!(*hub.clients.borrow(), 2);
        drop(a);
        assert_eq!(*hub.clients.borrow(), 1);
        drop(b);
        assert_eq!(*hub.clients.borrow(), 0);
    }

    /// `start`: nothing is sent with no client, the first sample after a join is sent, and a
    /// client that leaves silences it until the next one joins (which sends again at once).
    #[tokio::test(start_paused = true)]
    async fn the_sampler_sends_after_a_join_and_stops_after_the_last_leaves() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Arc::new(Hub::new(Store::open(&dir.path().join("mkrew.db")).unwrap()));
        let mut events = hub.events.subscribe();
        let sampler = start(Arc::clone(&hub));
        let sent = |events: &mut tokio::sync::broadcast::Receiver<Event>| {
            std::iter::from_fn(|| events.try_recv().ok())
                .filter(|e| matches!(e, Event::MachineStats(_)))
                .count()
        };

        tokio::time::sleep(HEARTBEAT * 2).await;
        assert_eq!(sent(&mut events), 0, "no client, no sample");

        let client = ClientGuard::join(&hub);
        tokio::time::sleep(SAMPLE_EVERY + Duration::from_millis(500)).await;
        assert_eq!(sent(&mut events), 1, "the first sample after the join");

        drop(client);
        tokio::time::sleep(HEARTBEAT * 2).await;
        assert_eq!(sent(&mut events), 0, "the last client left");

        let _client = ClientGuard::join(&hub);
        tokio::time::sleep(SAMPLE_EVERY + Duration::from_millis(500)).await;
        assert_eq!(sent(&mut events), 1, "a rejoin sends again");
        sampler.abort();
    }
}
