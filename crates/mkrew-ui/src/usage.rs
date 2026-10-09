//! Token usage (SPEC §20E.3): the newest `session.usage` of each session, loaded per workspace
//! on connect and kept live by events; the pane header's `ctx 62%` and the total tokens of a
//! task card and a sidebar workspace row. Pure; `main.rs` renders the text.

use std::collections::{HashMap, HashSet};

use mkrew_core::usage::Usage;
use mkrew_protocol::{Event, Session, SessionUsageEvent};
use ulid::Ulid;

/// From this context share on the pane header shows a warning (SPEC §20E.3).
pub const WARNING_PERCENT: u64 = 80;

#[derive(Debug, Default)]
pub struct Usages {
    by_session: HashMap<Ulid, SessionUsageEvent>,
    /// Workspaces whose `session.usage` was requested (once each: events keep them live).
    requested: HashSet<Ulid>,
}

impl Usages {
    /// The workspaces among `project_ids` whose usage was not requested yet, marked requested.
    pub fn to_request(&mut self, project_ids: impl IntoIterator<Item = Ulid>) -> Vec<Ulid> {
        project_ids
            .into_iter()
            .filter(|id| self.requested.insert(*id))
            .collect()
    }

    /// `project_id`'s `session.usage` failed: the next [`Usages::to_request`] asks again.
    pub fn request_failed(&mut self, project_id: Ulid) {
        self.requested.remove(&project_id);
    }

    /// Keeps a `session.usage` event; returns whether anything shown changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        let Event::SessionUsage(usage) = event else {
            return false;
        };
        self.keep(usage.clone())
    }

    /// Keeps one workspace's `session.usage` answer; returns whether anything shown changed.
    pub fn load(&mut self, listed: Vec<SessionUsageEvent>) -> bool {
        let mut changed = false;
        for usage in listed {
            changed |= self.keep(usage);
        }
        changed
    }

    /// A session's totals only grow (its agent's conversation only gets longer), so the larger
    /// wins: an answer read before an event that was handled first does not undo it.
    fn keep(&mut self, usage: SessionUsageEvent) -> bool {
        match self.by_session.get(&usage.session_id) {
            Some(known) if *known == usage || total(known) > total(&usage) => false,
            _ => {
                self.by_session.insert(usage.session_id, usage);
                true
            }
        }
    }

    pub fn get(&self, session_id: Ulid) -> Option<&SessionUsageEvent> {
        self.by_session.get(&session_id)
    }

    /// Input + output tokens of `session_id`.
    pub fn total(&self, session_id: Ulid) -> Option<u64> {
        self.get(session_id).map(total)
    }

    /// Tokens of `task_id`: its newest session that reported usage (a resumed run shows the
    /// earlier one until its first turn ends). Each session counts its agent's whole
    /// conversation, so a resumed run already includes the earlier ones: never a sum.
    pub fn task_total(&self, task_id: Ulid, sessions: &[Session]) -> Option<u64> {
        sessions
            .iter()
            .filter(|session| session.task_id == Some(task_id))
            .filter_map(|session| Some((session, self.total(session.id)?)))
            .max_by_key(|(session, _)| (session.started_at, session.id))
            .map(|(_, total)| total)
    }

    /// Total tokens of `project_id`: per task its [`Usages::task_total`], plus every session
    /// not started for a task. `None` while none of them reported usage.
    pub fn workspace_total(&self, project_id: Ulid, sessions: &[Session]) -> Option<u64> {
        // One pass (it runs per sidebar row per frame): per task its newest session with usage.
        let mut newest: HashMap<Ulid, ((i64, Ulid), u64)> = HashMap::new();
        let mut loose: Option<u64> = None;
        for session in sessions.iter().filter(|s| s.project_id == project_id) {
            let Some(total) = self.total(session.id) else {
                continue;
            };
            match session.task_id {
                Some(task_id) => {
                    let key = (session.started_at, session.id);
                    let slot = newest.entry(task_id).or_insert((key, total));
                    if key > slot.0 {
                        *slot = (key, total);
                    }
                }
                None => *loose.get_or_insert(0) += total,
            }
        }
        newest
            .into_values()
            .map(|(_, total)| total)
            .chain(loose)
            .reduce(|a, b| a + b)
    }
}

fn total(usage: &SessionUsageEvent) -> u64 {
    usage.input + usage.output
}

/// The pane header's context share: `ctx 62%`, with `warning` from [`WARNING_PERCENT`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextShare {
    pub label: String,
    pub warning: bool,
    /// Hover text: context size of the window and the tokens used.
    pub hover: String,
}

/// `None` without the profile's `context_window`.
pub fn context_share(usage: &SessionUsageEvent) -> Option<ContextShare> {
    let window = usage.context_window?;
    let percent = Usage {
        input: usage.input,
        output: usage.output,
        context: usage.context,
    }
    .context_percent(Some(window))?;
    Some(ContextShare {
        label: format!("ctx {percent}%"),
        warning: percent >= WARNING_PERCENT,
        hover: format!(
            "Context {} of {} tokens · {} tokens used ({} in, {} out)",
            format_count(usage.context),
            format_count(window),
            format_count(total(usage)),
            format_count(usage.input),
            format_count(usage.output),
        ),
    })
}

/// `850 tok`, `12.3k tok`, `1.2M tok`.
pub fn format_tokens(tokens: u64) -> String {
    format!("{} tok", format_count(tokens))
}

/// `850`, `12.3k`, `1.2M` (one decimal, rounded half up).
pub fn format_count(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let tenths_k = (n + 50) / 100;
    if tenths_k < 10_000 {
        return format!("{}.{}k", tenths_k / 10, tenths_k % 10);
    }
    let tenths_m = (n + 50_000) / 100_000;
    format!("{}.{}M", tenths_m / 10, tenths_m % 10)
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionKind, SessionState, StateSource};

    use super::*;

    fn usage(session_id: Ulid, input: u64, output: u64) -> SessionUsageEvent {
        SessionUsageEvent {
            session_id,
            input,
            output,
            context: 0,
            context_window: None,
        }
    }

    fn session(project_id: Ulid, task_id: Option<Ulid>, started_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id,
            project_id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            native_session_id: None,
            state: SessionState::Idle,
            state_source: StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
        }
    }

    #[test]
    fn events_and_loads_keep_the_larger_totals() {
        let id = Ulid::new();
        let mut usages = Usages::default();
        assert_eq!(usages.total(id), None);
        assert!(usages.apply(&Event::SessionUsage(usage(id, 100, 20))));
        assert_eq!(usages.total(id), Some(120));
        // The same event again changes nothing.
        assert!(!usages.apply(&Event::SessionUsage(usage(id, 100, 20))));
        // A workspace answer read before that event does not undo it.
        assert!(!usages.load(vec![usage(id, 50, 10)]));
        assert_eq!(usages.total(id), Some(120));
        // A newer turn end does.
        let other = Ulid::new();
        assert!(usages.load(vec![usage(id, 300, 40), usage(other, 1, 1)]));
        assert_eq!(
            (usages.total(id), usages.total(other)),
            (Some(340), Some(2))
        );
        assert!(
            !usages.apply(&Event::SessionExited(mkrew_protocol::SessionExitedEvent {
                session_id: id,
                exit_code: None,
            }))
        );
    }

    #[test]
    fn each_workspace_is_requested_once_unless_it_failed() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut usages = Usages::default();
        assert_eq!(usages.to_request([a]), vec![a]);
        assert_eq!(usages.to_request([a, b]), vec![b]);
        assert!(usages.to_request([a, b]).is_empty());
        usages.request_failed(b);
        assert_eq!(usages.to_request([a, b]), vec![b]);
    }

    #[test]
    fn a_workspace_counts_each_tasks_newest_session_and_sessions_without_a_task() {
        let (project, elsewhere, task) = (Ulid::new(), Ulid::new(), Ulid::new());
        let first_run = session(project, Some(task), 1);
        let resumed = session(project, Some(task), 2);
        let loose = session(project, None, 3);
        let shell = session(project, None, 4);
        let other = session(elsewhere, None, 5);
        let sessions = [
            resumed.clone(),
            first_run.clone(),
            loose.clone(),
            shell,
            other.clone(),
        ];
        let mut usages = Usages::default();
        assert_eq!(usages.workspace_total(project, &sessions), None);
        usages.load(vec![
            usage(first_run.id, 1_000, 100),
            usage(loose.id, 10, 5),
            usage(other.id, 7, 7),
        ]);
        // The resumed run has no turn end yet: its task still shows the first run's.
        assert_eq!(usages.task_total(task, &sessions), Some(1_100));
        assert_eq!(usages.workspace_total(project, &sessions), Some(1_115));
        usages.load(vec![usage(resumed.id, 3_000, 300)]);
        // The resumed run already counts the first one: 3_300, not 4_400.
        assert_eq!(usages.task_total(task, &sessions), Some(3_300));
        assert_eq!(usages.workspace_total(project, &sessions), Some(3_315));
        assert_eq!(usages.workspace_total(elsewhere, &sessions), Some(14));
    }

    #[test]
    fn labels_show_tokens_and_the_context_share() {
        let id = Ulid::new();
        let mut lines: Vec<String> = [0, 850, 999, 1_000, 12_345, 999_949, 999_950, 1_250_000]
            .into_iter()
            .map(|n| format!("{n} → {}", format_tokens(n)))
            .collect();
        for (context, window) in [
            (124_000, Some(200_000)),
            (160_000, Some(200_000)),
            (199_999, Some(200_000)),
            (50_000, None),
            (50_000, Some(0)),
        ] {
            let usage = SessionUsageEvent {
                context,
                context_window: window,
                ..usage(id, 1_234_567, 45_678)
            };
            lines.push(match context_share(&usage) {
                Some(share) => format!(
                    "{context}/{window:?} → {}{} · {}",
                    share.label,
                    if share.warning { " (warning)" } else { "" },
                    share.hover
                ),
                None => format!("{context}/{window:?} → none"),
            });
        }
        insta::assert_snapshot!(lines.join("\n"));
    }
}
