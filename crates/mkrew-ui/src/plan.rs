//! Plan checklist (SPEC §10, ADR 0039): the newest `session.plan` of each linked
//! session, loaded per workspace on connect and kept live by events; the right panel's Plan
//! card and the sidebar's progress bar. Pure; `main.rs` draws it.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use mkrew_protocol::{Event, SessionPlanEvent};
use ulid::Ulid;

/// Checklist rows the card shows; the rest is "N more".
pub const VISIBLE_ROWS: usize = 8;
/// Rows kept above the first unchecked item, so it is seen in context.
const ROWS_BEFORE_CURRENT: usize = 2;

#[derive(Debug, Default)]
pub struct Plans {
    by_session: HashMap<Ulid, SessionPlanEvent>,
    /// Workspaces whose `session.plan` was requested (once each: events keep them live).
    requested: HashSet<Ulid>,
}

impl Plans {
    /// The workspaces among `project_ids` whose plans were not requested yet, marked requested.
    pub fn to_request(&mut self, project_ids: impl IntoIterator<Item = Ulid>) -> Vec<Ulid> {
        project_ids
            .into_iter()
            .filter(|id| self.requested.insert(*id))
            .collect()
    }

    /// `project_id`'s `session.plan` failed: the next [`Plans::to_request`] asks again.
    pub fn request_failed(&mut self, project_id: Ulid) {
        self.requested.remove(&project_id);
    }

    /// Keeps a `session.plan` event and forgets a plan whose session was unlinked
    /// (`session.updated` without `plan_path`) or exited; returns whether anything shown changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::SessionPlan(plan) => self.keep(plan.clone()),
            Event::SessionUpdated(session) if session.plan_path.is_none() => {
                self.by_session.remove(&session.id).is_some()
            }
            Event::SessionExited(exited) => self.by_session.remove(&exited.session_id).is_some(),
            _ => false,
        }
    }

    /// Keeps one workspace's `session.plan` answer; returns whether anything shown changed.
    pub fn load(&mut self, listed: Vec<SessionPlanEvent>) -> bool {
        let mut changed = false;
        for plan in listed {
            changed |= self.keep(plan);
        }
        changed
    }

    fn keep(&mut self, plan: SessionPlanEvent) -> bool {
        if self.by_session.get(&plan.session_id) == Some(&plan) {
            return false;
        }
        self.by_session.insert(plan.session_id, plan);
        true
    }

    pub fn get(&self, session_id: Ulid) -> Option<&SessionPlanEvent> {
        self.by_session.get(&session_id)
    }

    /// The sidebar bar's fill; `None` without a plan or without items.
    pub fn fraction(&self, session_id: Ulid) -> Option<f32> {
        let plan = self.get(session_id)?;
        fraction(plan.done, plan.total)
    }
}

/// `done` of `total` as 0–1; `None` when there is nothing to count.
pub fn fraction(done: u32, total: u32) -> Option<f32> {
    (total > 0).then(|| done.min(total) as f32 / total as f32)
}

/// The plan's name from its stored path: the file name without `.md`.
pub fn name_of(plan_path: &str) -> String {
    Path::new(plan_path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// What the Plan card shows for one `session.plan`.
#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub name: String,
    /// `6/20`.
    pub count: String,
    pub fraction: f32,
    pub rows: Vec<Row>,
    /// Items not in `rows` (also those the daemon did not send).
    pub more: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub done: bool,
    /// The first unchecked item of the plan.
    pub current: bool,
}

/// At most [`VISIBLE_ROWS`] rows: the first unchecked item with [`ROWS_BEFORE_CURRENT`] rows
/// above it (the top of the list when everything is checked), moved up at the end of the list
/// so the card stays full.
pub fn card(plan: &SessionPlanEvent) -> Card {
    let items = &plan.items;
    let current = items.iter().position(|item| !item.done);
    let start = current
        .unwrap_or(0)
        .saturating_sub(ROWS_BEFORE_CURRENT)
        .min(items.len().saturating_sub(VISIBLE_ROWS));
    let end = (start + VISIBLE_ROWS).min(items.len());
    let rows: Vec<Row> = items[start..end]
        .iter()
        .enumerate()
        .map(|(i, item)| Row {
            text: item.text.clone(),
            done: item.done,
            current: current == Some(start + i),
        })
        .collect();
    Card {
        name: plan.name.clone(),
        count: format!("{}/{}", plan.done, plan.total),
        fraction: fraction(plan.done, plan.total).unwrap_or(0.0),
        more: (plan.total as usize).saturating_sub(rows.len()),
        rows,
        error: plan.error.clone(),
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{
        PlanItem, Session, SessionExitedEvent, SessionKind, SessionState, StateSource,
    };

    use super::*;

    fn plan(session_id: Ulid, done: &[bool]) -> SessionPlanEvent {
        SessionPlanEvent {
            session_id,
            name: "PLAN".into(),
            done: done.iter().filter(|d| **d).count() as u32,
            total: done.len() as u32,
            items: done
                .iter()
                .enumerate()
                .map(|(i, done)| PlanItem {
                    done: *done,
                    text: format!("item {i}"),
                })
                .collect(),
            error: None,
        }
    }

    /// The first `n` boxes checked, `len` boxes in all.
    fn first_done(n: usize, len: usize) -> Vec<bool> {
        (0..len).map(|i| i < n).collect()
    }

    fn session(id: Ulid, plan_path: Option<&str>) -> Session {
        Session {
            id,
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            pinned: false,
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Daemon,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            plan_path: plan_path.map(Into::into),
        }
    }

    #[test]
    fn the_first_unchecked_item_is_highlighted_with_context_and_the_rest_collapsed() {
        let card = card(&plan(Ulid::new(), &first_done(10, 20)));
        assert_eq!((card.count.as_str(), card.fraction), ("10/20", 0.5));
        // Two rows of context above the first unchecked one, then the next ones.
        let shown: Vec<&str> = card.rows.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(
            shown,
            [
                "item 8", "item 9", "item 10", "item 11", "item 12", "item 13", "item 14",
                "item 15"
            ]
        );
        let current: Vec<&str> = card
            .rows
            .iter()
            .filter(|r| r.current)
            .map(|r| r.text.as_str())
            .collect();
        assert_eq!(current, ["item 10"]);
        assert_eq!(card.more, 12);
        assert!(card.rows[0].done && !card.rows[2].done);
    }

    #[test]
    fn the_window_stops_at_the_end_of_the_list() {
        let card = card(&plan(Ulid::new(), &first_done(19, 20)));
        assert_eq!(card.rows.len(), VISIBLE_ROWS);
        assert_eq!(card.rows[0].text, "item 12");
        assert_eq!(card.rows.last().unwrap().text, "item 19");
        assert!(card.rows.last().unwrap().current);
        assert_eq!(card.more, 12);
    }

    #[test]
    fn a_short_or_finished_plan_shows_from_the_top_without_a_highlight() {
        let short = card(&plan(Ulid::new(), &[true, false, false]));
        assert_eq!((short.rows.len(), short.more), (3, 0));
        assert!(short.rows[1].current);

        let finished = card(&plan(Ulid::new(), &first_done(20, 20)));
        assert_eq!(finished.rows[0].text, "item 0");
        assert_eq!((finished.rows.len(), finished.more), (VISIBLE_ROWS, 12));
        assert!(finished.rows.iter().all(|r| r.done && !r.current));
        assert_eq!(finished.fraction, 1.0);
    }

    /// The daemon sends at most 200 items but the exact total: "N more" counts the ones not sent.
    #[test]
    fn more_counts_items_beyond_the_wire_cap() {
        let mut capped = plan(Ulid::new(), &first_done(0, 5));
        capped.total = 300;
        assert_eq!(card(&capped).more, 295);
    }

    #[test]
    fn an_unreadable_plan_shows_its_error_and_no_progress() {
        let mut broken = plan(Ulid::new(), &[]);
        broken.error = Some("no such file".into());
        let card = card(&broken);
        assert_eq!(card.error.as_deref(), Some("no such file"));
        assert_eq!((card.rows.len(), card.more, card.fraction), (0, 0, 0.0));
    }

    #[test]
    fn fraction_needs_a_total_and_never_exceeds_one() {
        assert_eq!(fraction(0, 0), None);
        assert_eq!(fraction(3, 12), Some(0.25));
        assert_eq!(fraction(5, 3), Some(1.0));
    }

    #[test]
    fn name_is_the_file_stem() {
        assert_eq!(name_of("docs/PLAN.md"), "PLAN");
        assert_eq!(name_of("2026-10-10-inbox.md"), "2026-10-10-inbox");
    }

    #[test]
    fn events_replace_and_unlinking_or_exit_clears() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut plans = Plans::default();
        let one = plan(a, &[true, false]);
        assert!(plans.apply(&Event::SessionPlan(one.clone())));
        assert!(
            !plans.apply(&Event::SessionPlan(one)),
            "same content: no redraw"
        );
        assert!(plans.apply(&Event::SessionPlan(plan(a, &[true, true]))));
        assert_eq!(plans.fraction(a), Some(1.0));
        assert!(plans.apply(&Event::SessionPlan(plan(b, &[false]))));

        // Still linked: other updates (a rename) keep it.
        let renamed = Event::SessionUpdated(Box::new(session(a, Some("PLAN.md"))));
        assert!(!plans.apply(&renamed));
        assert_eq!(plans.fraction(a), Some(1.0));
        // Unlinked: gone. Exited: gone.
        let unlinked = Event::SessionUpdated(Box::new(session(a, None)));
        assert!(plans.apply(&unlinked));
        assert_eq!(plans.get(a), None);
        assert!(plans.apply(&Event::SessionExited(SessionExitedEvent {
            session_id: b,
            exit_code: Some(0),
        })));
        assert_eq!(plans.get(b), None);
    }

    #[test]
    fn each_workspace_is_requested_once_until_it_fails() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut plans = Plans::default();
        assert_eq!(plans.to_request([a, b]), [a, b]);
        assert!(plans.to_request([a, b]).is_empty());
        plans.request_failed(a);
        assert_eq!(plans.to_request([a, b]), [a]);
    }
}
