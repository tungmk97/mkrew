//! The newest rate limits of each agent (spec 2026-11-22 section 5.1). Memory only: limits
//! belong to the account, not to a session, and the next turn or status line run fills them
//! again after a restart.

use mkrew_protocol::{AgentLimits, Cli};

/// At most one entry per agent.
#[derive(Debug, Default)]
pub struct Limits {
    list: Vec<AgentLimits>,
}

impl Limits {
    /// Keeps `new` when it is news (see [`AgentLimits::supersedes`]); returns whether it did.
    pub fn offer(&mut self, new: AgentLimits) -> bool {
        match self.list.iter_mut().find(|old| old.agent == new.agent) {
            Some(old) if new.supersedes(old) => {
                *old = new;
                true
            }
            Some(_) => false,
            None => {
                self.list.push(new);
                true
            }
        }
    }

    pub fn all(&self) -> Vec<AgentLimits> {
        self.list.clone()
    }
}

/// `limits` as the daemon keeps them: only Claude and Codex report, a percentage must be a
/// number and is clamped to 0-100.
pub fn checked(mut limits: AgentLimits) -> Result<AgentLimits, String> {
    if !matches!(limits.agent, Cli::Claude | Cli::Codex) {
        return Err(format!("{:?} does not report limits", limits.agent));
    }
    for window in [&mut limits.five_hour, &mut limits.weekly]
        .into_iter()
        .flatten()
    {
        if !window.used_percent.is_finite() {
            return Err("used_percent is not a number".into());
        }
        window.used_percent = window.used_percent.clamp(0.0, 100.0);
    }
    Ok(limits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::Hub;
    use mkrew_protocol::{Event, LimitWindow};
    use mkrew_store::Store;

    fn limits(agent: Cli, five: f32, at: i64) -> AgentLimits {
        AgentLimits {
            agent,
            five_hour: Some(LimitWindow {
                used_percent: five,
                resets_at: Some(9_999),
            }),
            weekly: None,
            observed_at: at,
        }
    }

    #[test]
    fn the_newest_report_of_each_agent_wins() {
        let mut store = Limits::default();
        assert!(store.offer(limits(Cli::Claude, 10.0, 100)));
        assert!(store.offer(limits(Cli::Codex, 5.0, 50)), "another agent");
        assert!(!store.offer(limits(Cli::Claude, 90.0, 99)), "older");
        assert!(!store.offer(limits(Cli::Claude, 10.0, 100)), "same content");
        assert!(
            store.offer(limits(Cli::Claude, 20.0, 100)),
            "a tie: the later one"
        );
        assert!(store.offer(limits(Cli::Claude, 30.0, 200)));
        let all = store.all();
        assert_eq!(all.len(), 2);
        let claude = all.iter().find(|l| l.agent == Cli::Claude).unwrap();
        assert_eq!(claude.five_hour.map(|w| w.used_percent), Some(30.0));
    }

    #[test]
    fn other_agents_and_non_numbers_are_refused_and_percentages_clamped() {
        assert!(checked(limits(Cli::Omp, 1.0, 1)).is_err());
        assert!(checked(limits(Cli::Copilot, 1.0, 1)).is_err());
        assert!(checked(limits(Cli::Claude, f32::NAN, 1)).is_err());
        assert!(checked(limits(Cli::Codex, f32::INFINITY, 1)).is_err());
        let clamped = checked(limits(Cli::Claude, -5.0, 1)).unwrap();
        assert_eq!(clamped.five_hour.map(|w| w.used_percent), Some(0.0));
        let clamped = checked(limits(Cli::Claude, 120.0, 1)).unwrap();
        assert_eq!(clamped.five_hour.map(|w| w.used_percent), Some(100.0));
    }

    #[test]
    fn the_hub_announces_a_change_once() {
        let dir = tempfile::tempdir().unwrap();
        let hub = Hub::new(Store::open(&dir.path().join("mkrew.db")).unwrap());
        let mut events = hub.events.subscribe();
        assert_eq!(hub.offer_limits(limits(Cli::Claude, 10.0, 100)), Ok(true));
        assert!(matches!(
            events.try_recv(),
            Ok(Event::AgentLimits(l)) if l.observed_at == 100
        ));
        assert_eq!(hub.offer_limits(limits(Cli::Claude, 10.0, 100)), Ok(false));
        assert!(events.try_recv().is_err(), "a repeat is not announced");
        assert!(hub.offer_limits(limits(Cli::Omp, 1.0, 1)).is_err());
        assert_eq!(hub.limits().len(), 1);
    }
}
