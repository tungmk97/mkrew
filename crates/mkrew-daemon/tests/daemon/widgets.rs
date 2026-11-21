//! Agent limits and machine stats (spec 2026-11-22 sections 5 and 6) over the real socket.

use super::*;
use mkrew_protocol::{AgentLimits, Cli, LimitWindow};

fn window(percent: f32) -> Option<LimitWindow> {
    Some(LimitWindow {
        used_percent: percent,
        resets_at: Some(4_000_000_000),
    })
}

fn report(agent: Cli, percent: f32, at: i64) -> Request {
    Request::LimitsReport(AgentLimits {
        agent,
        five_hour: window(percent),
        weekly: None,
        observed_at: at,
    })
}

fn listed(client: &mut Client) -> Vec<AgentLimits> {
    client.call(Request::LimitsGet).into_result().unwrap()
}

#[test]
fn a_report_is_listed_and_announced_and_the_newest_one_wins() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    assert_eq!(listed(&mut client), vec![]);

    client
        .call(report(Cli::Claude, 40.0, 100))
        .into_result::<()>()
        .unwrap();
    client.wait_event_where(
        |e| matches!(e, Event::AgentLimits(l) if l.agent == Cli::Claude && l.observed_at == 100),
    );
    assert_eq!(listed(&mut client).len(), 1);

    // An older report is ignored and not announced; a newer one replaces.
    client
        .call(report(Cli::Claude, 90.0, 50))
        .into_result::<()>()
        .unwrap();
    client
        .call(report(Cli::Claude, 55.0, 200))
        .into_result::<()>()
        .unwrap();
    client.wait_event_where(|e| matches!(e, Event::AgentLimits(l) if l.observed_at == 200));
    let all = listed(&mut client);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].five_hour.map(|w| w.used_percent), Some(55.0));
    assert!(
        !client
            .events
            .iter()
            .any(|e| matches!(e, Event::AgentLimits(l) if l.observed_at == 50)),
        "an older report is not announced"
    );

    // Limits are per agent; a client that connects later reads them at once.
    client
        .call(report(Cli::Codex, 10.0, 60))
        .into_result::<()>()
        .unwrap();
    let mut later = Client::connect(&env.socket).unwrap();
    later.hello().unwrap();
    assert_eq!(listed(&mut later).len(), 2);
    client.stop();
}

#[test]
fn only_claude_and_codex_may_report_limits() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let outcome = client.call(report(Cli::Omp, 10.0, 1));
    assert_eq!(error_code(outcome), ErrorCode::InvalidRequest);
    assert_eq!(listed(&mut client), vec![]);
    client.stop();
}

#[test]
fn a_percentage_beyond_the_scale_is_clamped() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    client
        .call(report(Cli::Claude, 140.0, 1))
        .into_result::<()>()
        .unwrap();
    assert_eq!(
        listed(&mut client)[0].five_hour.map(|w| w.used_percent),
        Some(100.0)
    );
    client.stop();
}

#[test]
fn machine_stats_arrive_while_a_client_is_connected() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    // Real `sysinfo` on this machine, nothing else: the first reading comes after one
    // sampling period (2 s).
    let Event::MachineStats(stats) =
        client.wait_event_where(|e| matches!(e, Event::MachineStats(_)))
    else {
        unreachable!()
    };
    assert!((0.0..=100.0).contains(&stats.cpu_percent), "{stats:?}");
    assert!(
        stats.ram_total == 0 || stats.ram_used <= stats.ram_total,
        "{stats:?}"
    );
    client.stop();
}
