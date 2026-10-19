//! What the OS shell shows about the Inbox (SPEC §20J, ADR 0064): the Needs-you and working
//! counts, the tray menu's lines, and which of them changed since the last look, so the taskbar
//! badge, the tray icon and its menu are touched only on a change. Pure and tested headlessly.

use mkrew_protocol::{Event, Project, Session, SessionKind, SessionState};
use ulid::Ulid;

use crate::inbox::{self, Viewed};
use crate::sidebar::status_display;
use crate::status::format_duration;

/// Most sessions the tray menu lists (the count and the badge still show all of them).
pub const TRAY_SESSIONS: usize = 8;

/// The tray icon's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Nothing works or waits.
    Idle,
    /// An agent works.
    Working,
    /// Something needs the user.
    NeedsYou,
}

/// The numbers behind the badge, the icon and the tooltip.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    /// [`inbox::needs_you`] sessions of known workspaces: exactly what the Inbox lists.
    pub needs_you: usize,
    /// Agent sessions working (the sidebar's `Working` chip).
    pub working: usize,
    /// Changes whenever the *set* of Needs-you sessions or any of their states changes, even
    /// when `needs_you` does not (one leaves as another enters).
    stamp: u64,
}

impl Counts {
    pub fn icon(self) -> Icon {
        if self.needs_you > 0 {
            Icon::NeedsYou
        } else if self.working > 0 {
            Icon::Working
        } else {
            Icon::Idle
        }
    }
}

/// One pass over the sessions, no allocation.
pub fn counts(projects: &[Project], sessions: &[Session], viewed: &Viewed) -> Counts {
    let mut counts = Counts::default();
    let known = |session: &&Session| projects.iter().any(|p| p.id == session.project_id);
    for session in sessions.iter().filter(known) {
        if session.kind == SessionKind::Agent && session.state == SessionState::Working {
            counts.working += 1;
        }
        if inbox::needs_you(session, viewed) {
            counts.needs_you += 1;
            let id = (session.id.0 as u64) ^ ((session.id.0 >> 64) as u64);
            counts.stamp = counts
                .stamp
                .wrapping_add(id ^ (session.state_changed_at as u64));
        }
    }
    counts
}

/// One tray menu line: a session that needs the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub session_id: Ulid,
    /// Agent profile id.
    pub agent: String,
    /// Workspace name.
    pub workspace: String,
    pub state: SessionState,
    /// Unix ms of the state change that put the session in Needs you.
    pub since: i64,
}

impl Item {
    /// `claude · api — needs input, 3m` (the age is read when the menu opens).
    pub fn text(&self, now_ms: i64) -> String {
        format!(
            "{} · {} — {}, {}",
            self.agent,
            self.workspace,
            status_display(self.state).label,
            format_duration(now_ms - self.since)
        )
    }
}

/// What the tray's tooltip and menu show; the UI keeps the newest copy for the tray thread.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrayMenu {
    pub needs_you: usize,
    pub working: usize,
    /// Newest first, at most [`TRAY_SESSIONS`].
    pub items: Vec<Item>,
}

impl TrayMenu {
    /// `Mkrew · 2 need you · 1 working`
    pub fn tip(&self) -> String {
        format!(
            "Mkrew · {} · {} working",
            inbox::label(self.needs_you),
            self.working
        )
    }
}

/// The tray menu for `counts` (built only when [`Tracker::update`] says the tray changed).
pub fn menu(
    projects: &[Project],
    sessions: &[Session],
    viewed: &Viewed,
    counts: Counts,
) -> TrayMenu {
    let items = inbox::entries(projects, sessions, viewed)
        .into_iter()
        .take(TRAY_SESSIONS)
        .map(|entry| Item {
            session_id: entry.session_id,
            agent: entry.agent,
            workspace: entry.workspace,
            state: entry.state,
            since: entry.state_changed_at,
        })
        .collect();
    TrayMenu {
        needs_you: counts.needs_you,
        working: counts.working,
        items,
    }
}

/// What to update in the OS after a recount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Change {
    /// The Needs-you number moved: the taskbar badge.
    pub badge: bool,
    /// Anything the tray shows moved: icon, tooltip, menu.
    pub tray: bool,
}

/// Remembers the last [`Counts`] so only a change reaches the OS.
#[derive(Debug, Default)]
pub struct Tracker {
    last: Counts,
}

impl Tracker {
    pub fn update(&mut self, now: Counts) -> Change {
        let last = std::mem::replace(&mut self.last, now);
        Change {
            badge: now.needs_you != last.needs_you,
            tray: now != last,
        }
    }
}

/// Whether `event` moves a session *into* `needs_input` (the taskbar flash trigger). Call before
/// the sidebar applies the event, so `sessions` still holds the previous state.
pub fn entered_needs_input(event: &Event, sessions: &[Session]) -> bool {
    let Event::SessionState(event) = event else {
        return false;
    };
    event.state == SessionState::NeedsInput
        && sessions
            .iter()
            .find(|s| s.id == event.session_id)
            .is_none_or(|s| s.state != SessionState::NeedsInput)
}

/// Whether the window's ✕ hides it: only with the setting on and a tray icon to bring it back.
pub fn hide_on_close(close_to_tray: bool, has_tray: bool) -> bool {
    close_to_tray && has_tray
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{SessionStateEvent, StateSource};

    use super::*;

    fn project(name: &str) -> Project {
        Project {
            id: Ulid::new(),
            name: name.to_owned(),
            path: format!("C:\\src\\{name}"),
            run_commands: Vec::new(),
            run_autostart: false,
            default_agent: None,
            default_shell: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
            color: None,
            pinned: false,
        }
    }

    fn session(project: &Project, state: SessionState, changed_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: project.id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            native_session_id: None,
            state,
            state_source: StateSource::Hook,
            state_changed_at: changed_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
            pinned: false,
            position: 0.0,
            plan_path: None,
        }
    }

    fn state_event(session: &Session, state: SessionState) -> Event {
        Event::SessionState(SessionStateEvent {
            session_id: session.id,
            state,
            source: StateSource::Hook,
            changed_at: 9,
        })
    }

    #[test]
    fn counts_are_the_inbox_and_the_working_agents() {
        let api = project("api");
        let asking = session(&api, SessionState::NeedsInput, 1);
        let done = session(&api, SessionState::Done, 2);
        let seen = session(&api, SessionState::Done, 3);
        let working = session(&api, SessionState::Working, 4);
        let terminal = Session {
            kind: SessionKind::Shell,
            ..session(&api, SessionState::Working, 5)
        };
        let orphan = session(&project("gone"), SessionState::NeedsInput, 6);
        let mut viewed = Viewed::default();
        viewed.view(&seen);
        let all = [asking, done, seen, working, terminal, orphan];

        let counts = counts(std::slice::from_ref(&api), &all, &viewed);
        assert_eq!((counts.needs_you, counts.working), (2, 1));
        assert_eq!(
            counts.needs_you,
            inbox::entries(std::slice::from_ref(&api), &all, &viewed).len(),
            "the badge counts exactly what the Inbox lists"
        );
        assert_eq!(counts.icon(), Icon::NeedsYou);
    }

    #[test]
    fn the_icon_follows_the_most_urgent_state() {
        let api = project("api");
        let viewed = Viewed::default();
        let icon =
            |sessions: &[Session]| counts(std::slice::from_ref(&api), sessions, &viewed).icon();
        assert_eq!(icon(&[]), Icon::Idle);
        assert_eq!(
            icon(&[session(&api, SessionState::Working, 1)]),
            Icon::Working
        );
        let both = [
            session(&api, SessionState::Working, 1),
            session(&api, SessionState::NeedsInput, 2),
        ];
        assert_eq!(icon(&both), Icon::NeedsYou);
    }

    #[test]
    fn the_tracker_reports_only_what_changed() {
        let api = project("api");
        let viewed = Viewed::default();
        let projects = [api.clone()];
        let mut tracker = Tracker::default();
        let none = counts(&projects, &[], &viewed);
        assert_eq!(
            tracker.update(none),
            Change::default(),
            "nothing yet, nothing to show"
        );

        let asking = [session(&api, SessionState::NeedsInput, 1)];
        let one = counts(&projects, &asking, &viewed);
        assert_eq!(
            tracker.update(one),
            Change {
                badge: true,
                tray: true
            }
        );
        assert_eq!(
            tracker.update(one),
            Change::default(),
            "same counts: no OS call"
        );

        let working = [asking[0].clone(), session(&api, SessionState::Working, 2)];
        let more = counts(&projects, &working, &viewed);
        assert_eq!(
            tracker.update(more),
            Change {
                badge: false,
                tray: true
            },
            "a working agent changes the tray, not the badge"
        );
    }

    #[test]
    fn the_same_count_with_another_set_still_refreshes_the_tray() {
        let api = project("api");
        let viewed = Viewed::default();
        let projects = [api.clone()];
        let mut a = session(&api, SessionState::NeedsInput, 1);
        let mut b = session(&api, SessionState::Working, 2);
        let mut tracker = Tracker::default();
        tracker.update(counts(&projects, &[a.clone(), b.clone()], &viewed));

        // A leaves, B enters: still one session needs you, and one works.
        a.state = SessionState::Working;
        a.state_changed_at = 3;
        b.state = SessionState::NeedsInput;
        b.state_changed_at = 3;
        let change = tracker.update(counts(&projects, &[a.clone(), b.clone()], &viewed));
        assert_eq!(
            change,
            Change {
                badge: false,
                tray: true
            }
        );

        // The same session asks, then finishes: one needs you either way.
        b.state = SessionState::Done;
        b.state_changed_at = 4;
        let change = tracker.update(counts(&projects, &[a, b], &viewed));
        assert!(change.tray && !change.badge);
    }

    #[test]
    fn twelve_waiting_sessions_list_eight_and_count_twelve() {
        let api = project("api");
        let sessions: Vec<Session> = (1..=12)
            .map(|at| session(&api, SessionState::NeedsInput, at))
            .collect();
        let viewed = Viewed::default();
        let projects = [api];
        let counts = counts(&projects, &sessions, &viewed);
        let menu = menu(&projects, &sessions, &viewed, counts);
        assert_eq!(menu.needs_you, 12);
        assert_eq!(menu.items.len(), TRAY_SESSIONS);
        assert_eq!(menu.items[0].since, 12, "newest first");
        assert_eq!(menu.items[7].since, 5);
    }

    #[test]
    fn a_menu_line_names_agent_workspace_state_and_age() {
        let item = Item {
            session_id: Ulid::new(),
            agent: "claude".to_owned(),
            workspace: "api".to_owned(),
            state: SessionState::NeedsInput,
            since: 1_000,
        };
        assert_eq!(item.text(181_000), "claude · api — needs input, 3m");
        let done = Item {
            state: SessionState::Done,
            ..item
        };
        assert_eq!(done.text(13_000), "claude · api — done, 12s");
    }

    #[test]
    fn the_tooltip_names_both_counts() {
        let menu = |needs_you, working| TrayMenu {
            needs_you,
            working,
            items: Vec::new(),
        };
        assert_eq!(menu(2, 1).tip(), "Mkrew · 2 need you · 1 working");
        assert_eq!(menu(0, 0).tip(), "Mkrew · Inbox clear · 0 working");
        assert_eq!(menu(1, 3).tip(), "Mkrew · 1 needs you · 3 working");
    }

    #[test]
    fn only_entering_needs_input_counts_for_the_flash() {
        let api = project("api");
        let working = session(&api, SessionState::Working, 1);
        let asking = session(&api, SessionState::NeedsInput, 1);
        let sessions = [working.clone(), asking.clone()];
        assert!(entered_needs_input(
            &state_event(&working, SessionState::NeedsInput),
            &sessions
        ));
        assert!(
            !entered_needs_input(&state_event(&asking, SessionState::NeedsInput), &sessions),
            "already asking"
        );
        assert!(!entered_needs_input(
            &state_event(&working, SessionState::Done),
            &sessions
        ));
        let unknown = session(&api, SessionState::Working, 1);
        assert!(
            entered_needs_input(&state_event(&unknown, SessionState::NeedsInput), &sessions),
            "a session the list does not know yet"
        );
    }

    #[test]
    fn closing_hides_only_while_the_tray_icon_is_there() {
        assert!(hide_on_close(true, true));
        assert!(
            !hide_on_close(true, false),
            "no tray icon: ✕ quits, never hide into nothing"
        );
        assert!(!hide_on_close(false, true));
        assert!(!hide_on_close(false, false));
    }
}
