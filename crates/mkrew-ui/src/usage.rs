//! Token usage (SPEC §20E.3): the newest `session.usage` of each session, loaded per workspace
//! on connect and kept live by events; the pane header's `ctx 62%` and the total tokens of a
//! task card. Pure; `main.rs` renders the text.

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

    /// All tokens of `session_id` (input, cache and output).
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
}

/// Every token of the conversation: fresh input, cache read, cache write and output.
fn total(usage: &SessionUsageEvent) -> u64 {
    usage.input + usage.cache_read + usage.cache_write + usage.output
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
        context: usage.context,
        ..Usage::default()
    }
    .context_percent(Some(window))?;
    let cached = usage.cache_read + usage.cache_write;
    let cached = if cached > 0 {
        format!(", {} cached", format_count(cached))
    } else {
        String::new()
    };
    Some(ContextShare {
        label: format!("ctx {percent}%"),
        warning: percent >= WARNING_PERCENT,
        hover: format!(
            "Context {} of {} tokens · {} tokens used ({} in{cached}, {} out)",
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

/// What the status bar shows of the focused agent's usage. A field with no data is `None`:
/// hidden, never `0` (SPEC §10; no money estimate, no tokens per second).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageFields {
    pub context: Option<ContextShare>,
    /// `in 12.3k · out 800` (`in` excludes cache tokens); `None` until a turn reported any.
    pub tokens: Option<String>,
    /// `cache 1.3M read · 40.0k write`; a side without tokens is left out.
    pub cache: Option<String>,
    pub model: Option<String>,
}

pub fn fields(usage: &SessionUsageEvent) -> UsageFields {
    let tokens = (usage.input.saturating_add(usage.output) > 0).then(|| {
        format!(
            "in {} · out {}",
            format_count(usage.input),
            format_count(usage.output)
        )
    });
    let cache: Vec<String> = [(usage.cache_read, "read"), (usage.cache_write, "write")]
        .into_iter()
        .filter(|(tokens, _)| *tokens > 0)
        .map(|(tokens, side)| format!("{} {side}", format_count(tokens)))
        .collect();
    UsageFields {
        context: context_share(usage),
        tokens,
        cache: (!cache.is_empty()).then(|| format!("cache {}", cache.join(" · "))),
        model: usage
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_owned),
    }
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
            cache_read: 0,
            cache_write: 0,
            context: 0,
            context_window: None,
            model: None,
            transcript: None,
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
            color: None,
            native_session_id: None,
            state: SessionState::Idle,
            state_source: StateSource::Daemon,
            state_changed_at: started_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
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

    /// The total counts the cache (so it is the number it was before the split), and the
    /// split event of a turn replaces the unsplit one an older daemon stored for it: equal
    /// totals are not "smaller".
    #[test]
    fn totals_count_cached_tokens_and_a_split_event_replaces_an_unsplit_one() {
        let id = Ulid::new();
        let mut usages = Usages::default();
        // Stored before the split: `input` still had the cache in it.
        assert!(usages.apply(&Event::SessionUsage(usage(id, 1_000, 20))));
        let split = SessionUsageEvent {
            cache_read: 800,
            cache_write: 100,
            ..usage(id, 100, 20)
        };
        assert!(usages.apply(&Event::SessionUsage(split.clone())));
        assert_eq!(usages.total(id), Some(1_020));
        assert_eq!(usages.get(id), Some(&split));
        // A smaller (older) answer still does not undo it.
        assert!(!usages.load(vec![usage(id, 100, 20)]));
    }

    /// The hover says how many of the input tokens came from the cache, only when there are any.
    #[test]
    fn the_hover_names_cached_tokens() {
        let id = Ulid::new();
        let split = SessionUsageEvent {
            cache_read: 48_000,
            cache_write: 2_000,
            context: 51_000,
            context_window: Some(200_000),
            ..usage(id, 1_000, 500)
        };
        let share = context_share(&split).unwrap();
        assert_eq!(share.label, "ctx 25%");
        assert_eq!(
            share.hover,
            "Context 51.0k of 200.0k tokens · 51.5k tokens used (1.0k in, 50.0k cached, 500 out)"
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
    fn a_task_shows_its_newest_session_with_usage() {
        let (project, task) = (Ulid::new(), Ulid::new());
        let first_run = session(project, Some(task), 1);
        let resumed = session(project, Some(task), 2);
        let sessions = [resumed.clone(), first_run.clone()];
        let mut usages = Usages::default();
        usages.load(vec![usage(first_run.id, 1_000, 100)]);
        // The resumed run has no turn end yet: its task still shows the first run's.
        assert_eq!(usages.task_total(task, &sessions), Some(1_100));
        usages.load(vec![usage(resumed.id, 3_000, 300)]);
        // The resumed run already counts the first one: 3_300, not 4_400.
        assert_eq!(usages.task_total(task, &sessions), Some(3_300));
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

    /// A field with no data is `None`, never `0` or an empty string.
    #[test]
    fn fields_without_data_are_none() {
        let id = Ulid::new();
        assert_eq!(fields(&usage(id, 0, 0)), UsageFields::default());
        // A blank model name is no model.
        let blank = SessionUsageEvent {
            model: Some("  ".to_owned()),
            ..usage(id, 0, 0)
        };
        assert_eq!(fields(&blank).model, None);
        // Context needs the profile's window.
        let no_window = SessionUsageEvent {
            context: 50_000,
            context_window: None,
            ..usage(id, 1, 1)
        };
        assert_eq!(fields(&no_window).context, None);
    }

    #[test]
    fn fields_format_the_real_data() {
        let id = Ulid::new();
        let full = SessionUsageEvent {
            cache_read: 1_250_000,
            cache_write: 0,
            context: 124_000,
            context_window: Some(200_000),
            model: Some(" claude-opus-4 ".to_owned()),
            ..usage(id, 12_345, 800)
        };
        let shown = fields(&full);
        assert_eq!(shown.tokens.as_deref(), Some("in 12.3k · out 800"));
        // Only the side that has tokens is named.
        assert_eq!(shown.cache.as_deref(), Some("cache 1.3M read"));
        assert_eq!(shown.model.as_deref(), Some("claude-opus-4"));
        let context = shown.context.expect("window known");
        assert_eq!(context.label, "ctx 62%");

        let both = SessionUsageEvent {
            cache_read: 800,
            cache_write: 40_000,
            ..usage(id, 1, 1)
        };
        assert_eq!(
            fields(&both).cache.as_deref(),
            Some("cache 800 read · 40.0k write")
        );
        // Output only (every input token came from the cache) is still real data.
        assert_eq!(
            fields(&usage(id, 0, 300)).tokens.as_deref(),
            Some("in 0 · out 300")
        );
    }

    /// The label keeps the true share past the window.
    #[test]
    fn a_share_past_the_window_warns() {
        let over = SessionUsageEvent {
            context: 250_000,
            context_window: Some(200_000),
            ..usage(Ulid::new(), 1, 1)
        };
        let share = context_share(&over).expect("window known");
        assert_eq!(share.label, "ctx 125%");
        assert!(share.warning);
    }
}
