//! Sidebar Inbox (SPEC §10, §11): every session that needs input, or is done and not yet
//! viewed, newest first. Pure and tested headlessly; `main.rs` renders the entries.

use std::collections::HashMap;

use mkrew_protocol::{Project, Session, SessionState, StateSource};
use ulid::Ulid;

/// One Inbox line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub session_id: Ulid,
    pub project_id: Ulid,
    /// Workspace name.
    pub workspace: String,
    /// Agent profile id.
    pub agent: String,
    pub title: Option<String>,
    pub state: SessionState,
    pub state_source: StateSource,
    /// Unix ms of the state change that put the session in the Inbox.
    pub state_changed_at: i64,
}

/// Which `done` states the user has seen: a done session leaves the Inbox once its pane was
/// focused in the active window; a later `done` (new `state_changed_at`) shows it again.
#[derive(Debug, Default)]
pub struct Viewed {
    /// Session → `state_changed_at` of its viewed `done`.
    done: HashMap<Ulid, i64>,
}

impl Viewed {
    /// `session` is on screen and focused.
    pub fn view(&mut self, session: &Session) {
        if session.state == SessionState::Done {
            self.done.insert(session.id, session.state_changed_at);
        }
    }

    fn seen(&self, session: &Session) -> bool {
        self.done.get(&session.id) == Some(&session.state_changed_at)
    }
}

/// The last non-blank screen line of each Inbox entry (§20A.4), read once when the entry
/// appears (`session.read lines=1`); a later state change appears again and reads again.
#[derive(Debug, Default)]
pub struct LastLines {
    /// Session → `state_changed_at` of the read entry and its line (`None` while reading,
    /// or when the screen was blank or the read failed).
    lines: HashMap<Ulid, (i64, Option<String>)>,
}

impl LastLines {
    /// The entries to read now: those without a line for their current state change. They
    /// count as reading from here on; sessions that left the Inbox are forgotten.
    pub fn to_read(&mut self, entries: &[Entry]) -> Vec<(Ulid, i64)> {
        self.lines
            .retain(|id, _| entries.iter().any(|e| e.session_id == *id));
        let mut to_read = Vec::new();
        for e in entries {
            if self
                .lines
                .get(&e.session_id)
                .is_none_or(|(at, _)| *at != e.state_changed_at)
            {
                self.lines.insert(e.session_id, (e.state_changed_at, None));
                to_read.push((e.session_id, e.state_changed_at));
            }
        }
        to_read
    }

    /// The read of `session` at `state_changed_at` returned `lines`; ignored if the entry
    /// changed since.
    pub fn read(&mut self, session: Ulid, state_changed_at: i64, lines: &[String]) {
        if let Some((at, line)) = self.lines.get_mut(&session)
            && *at == state_changed_at
        {
            *line = lines
                .iter()
                .rev()
                .map(|l| l.trim())
                .find(|l| !l.is_empty())
                .map(str::to_owned);
        }
    }

    /// The read of `session` at `state_changed_at` failed: the next [`LastLines::to_read`]
    /// asks again.
    pub fn forget(&mut self, session: Ulid, state_changed_at: i64) {
        if self
            .lines
            .get(&session)
            .is_some_and(|(at, _)| *at == state_changed_at)
        {
            self.lines.remove(&session);
        }
    }

    /// The line shown under `entry`, if read.
    pub fn get(&self, entry: &Entry) -> Option<&str> {
        match self.lines.get(&entry.session_id) {
            Some((at, Some(line))) if *at == entry.state_changed_at => Some(line),
            _ => None,
        }
    }
}

/// The Inbox, newest state change first. Sessions of unknown (e.g. archived) workspaces are
/// left out.
pub fn entries(projects: &[Project], sessions: &[Session], viewed: &Viewed) -> Vec<Entry> {
    let mut entries: Vec<Entry> = sessions
        .iter()
        .filter(|session| match session.state {
            SessionState::NeedsInput => true,
            SessionState::Done => !viewed.seen(session),
            _ => false,
        })
        .filter_map(|session| {
            let project = projects.iter().find(|p| p.id == session.project_id)?;
            Some(Entry {
                session_id: session.id,
                project_id: session.project_id,
                workspace: project.name.clone(),
                agent: session.agent_profile.clone(),
                title: session.title.clone(),
                state: session.state,
                state_source: session.state_source,
                state_changed_at: session.state_changed_at,
            })
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse((entry.state_changed_at, entry.session_id)));
    entries
}

/// The session "next Inbox entry" (§13) jumps to: the entry after `focused`, wrapping to the
/// first. A focused `done` session already left the Inbox once viewed, so it counts from where
/// it was listed; any other unlisted focus starts at the first (newest) entry. `None` if the
/// Inbox is empty.
pub fn next(entries: &[Entry], focused: Option<&Session>) -> Option<Ulid> {
    let key = |at: i64, id: Ulid| std::cmp::Reverse((at, id));
    let after = focused.map_or(0, |focused| {
        let listed = entries.iter().position(|e| e.session_id == focused.id);
        match listed {
            Some(index) => index + 1,
            None if focused.state == SessionState::Done => {
                let was = key(focused.state_changed_at, focused.id);
                entries.partition_point(|e| key(e.state_changed_at, e.session_id) < was)
            }
            None => 0,
        }
    });
    entries
        .get(after)
        .or_else(|| entries.first())
        .map(|entry| entry.session_id)
}

/// What a clicked OS notification shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    /// The session's pane.
    Session(Ulid),
    /// Only its workspace: the session is not listed yet.
    Workspace(Ulid),
}

/// Where a click on the notification of `session_id` in `project_id` goes; `None` if neither
/// is known.
pub fn jump(
    projects: &[Project],
    sessions: &[Session],
    session_id: Ulid,
    project_id: Ulid,
) -> Option<Jump> {
    if sessions.iter().any(|s| s.id == session_id) {
        Some(Jump::Session(session_id))
    } else if projects.iter().any(|p| p.id == project_id) {
        Some(Jump::Workspace(project_id))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::SessionKind;

    use super::*;

    fn project(name: &str) -> Project {
        Project {
            id: Ulid::new(),
            name: name.to_owned(),
            path: format!("C:\\src\\{name}"),
            run_commands: Vec::new(),
            run_autostart: false,
            default_agent: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
            color: None,
        }
    }

    fn session(project: &Project, state: SessionState, changed_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: project.id,
            agent_profile: "claude".to_owned(),
            kind: SessionKind::Agent,
            title: Some("fix login".to_owned()),
            native_session_id: None,
            state,
            state_source: StateSource::Hook,
            state_changed_at: changed_at,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    fn ids(entries: &[Entry]) -> Vec<Ulid> {
        entries.iter().map(|entry| entry.session_id).collect()
    }

    #[test]
    fn lists_needs_input_and_done_newest_first() {
        let (api, web) = (project("api"), project("web"));
        let asking = session(&api, SessionState::NeedsInput, 10);
        let finished = session(&web, SessionState::Done, 20);
        let sessions = vec![
            asking.clone(),
            session(&api, SessionState::Working, 30),
            session(&api, SessionState::Idle, 40),
            session(&web, SessionState::Exited, 50),
            finished.clone(),
        ];
        let entries = entries(&[api, web], &sessions, &Viewed::default());
        assert_eq!(ids(&entries), vec![finished.id, asking.id]);
        let first = &entries[0];
        assert_eq!(
            (first.workspace.as_str(), first.agent.as_str(), first.state),
            ("web", "claude", SessionState::Done)
        );
        assert_eq!(first.title.as_deref(), Some("fix login"));
    }

    #[test]
    fn nothing_to_do_is_empty() {
        let api = project("api");
        let sessions = vec![session(&api, SessionState::Working, 1)];
        assert!(entries(&[api], &sessions, &Viewed::default()).is_empty());
        assert!(entries(&[], &[], &Viewed::default()).is_empty());
    }

    #[test]
    fn done_stays_until_viewed_and_returns_when_done_again() {
        let api = project("api");
        let mut done = session(&api, SessionState::Done, 5);
        let projects = [api];
        let mut viewed = Viewed::default();
        assert_eq!(entries(&projects, &[done.clone()], &viewed).len(), 1);

        viewed.view(&done);
        assert!(entries(&projects, &[done.clone()], &viewed).is_empty());

        // The agent works again and finishes a second time.
        done.state_changed_at = 9;
        assert_eq!(entries(&projects, &[done.clone()], &viewed).len(), 1);
    }

    #[test]
    fn needs_input_stays_while_viewed() {
        let api = project("api");
        let asking = session(&api, SessionState::NeedsInput, 5);
        let mut viewed = Viewed::default();
        viewed.view(&asking);
        assert_eq!(entries(&[api], &[asking], &viewed).len(), 1);
    }

    #[test]
    fn next_entry_follows_the_focused_session_and_wraps() {
        let api = project("api");
        let sessions: Vec<Session> = (1..=3)
            .map(|at| session(&api, SessionState::NeedsInput, at))
            .collect();
        let entries = entries(std::slice::from_ref(&api), &sessions, &Viewed::default());
        let [oldest, middle, newest] = [&sessions[0], &sessions[1], &sessions[2]];
        let working = session(&api, SessionState::Working, 2);

        assert_eq!(next(&entries, None), Some(newest.id));
        assert_eq!(next(&entries, Some(&working)), Some(newest.id));
        assert_eq!(next(&entries, Some(newest)), Some(middle.id));
        assert_eq!(next(&entries, Some(middle)), Some(oldest.id));
        assert_eq!(next(&entries, Some(oldest)), Some(newest.id));
        assert_eq!(next(&entries[..1], Some(newest)), Some(newest.id));
        assert_eq!(next(&[], Some(newest)), None);
    }

    #[test]
    fn next_entry_after_a_viewed_done_keeps_its_place() {
        let api = project("api");
        let newest = session(&api, SessionState::NeedsInput, 3);
        let done = session(&api, SessionState::Done, 2);
        let oldest = session(&api, SessionState::NeedsInput, 1);
        let mut viewed = Viewed::default();
        viewed.view(&done);
        let sessions = [newest.clone(), done.clone(), oldest.clone()];
        let entries = entries(&[api], &sessions, &viewed);
        assert_eq!(ids(&entries), vec![newest.id, oldest.id]);

        assert_eq!(next(&entries, Some(&done)), Some(oldest.id));
        // The oldest done wraps around.
        let older = Session {
            state_changed_at: 0,
            ..done.clone()
        };
        assert_eq!(next(&entries, Some(&older)), Some(newest.id));
    }

    #[test]
    fn notification_click_jumps_to_the_session_else_its_workspace() {
        let api = project("api");
        let asking = session(&api, SessionState::NeedsInput, 1);
        let (projects, sessions) = ([api.clone()], [asking.clone()]);
        assert_eq!(
            jump(&projects, &sessions, asking.id, api.id),
            Some(Jump::Session(asking.id))
        );
        assert_eq!(
            jump(&projects, &sessions, Ulid::new(), api.id),
            Some(Jump::Workspace(api.id))
        );
        assert_eq!(jump(&projects, &sessions, Ulid::new(), Ulid::new()), None);
    }

    #[test]
    fn last_lines_are_read_once_per_appearance() {
        let api = project("api");
        let mut asking = session(&api, SessionState::NeedsInput, 5);
        let projects = [api];
        let mut last = LastLines::default();
        let listed = entries(&projects, &[asking.clone()], &Viewed::default());
        assert_eq!(last.to_read(&listed), vec![(asking.id, 5)]);
        // Reading: not asked again, nothing shown yet.
        assert!(last.to_read(&listed).is_empty());
        assert_eq!(last.get(&listed[0]), None);

        last.read(
            asking.id,
            5,
            &["Continue?".to_owned(), "  Apply edit? (y/n)  ".to_owned()],
        );
        assert_eq!(last.get(&listed[0]), Some("Apply edit? (y/n)"));
        assert!(last.to_read(&listed).is_empty());

        // A new state change re-reads; a late answer of the old read is ignored.
        asking.state_changed_at = 9;
        let listed = entries(&projects, &[asking.clone()], &Viewed::default());
        assert_eq!(last.get(&listed[0]), None);
        assert_eq!(last.to_read(&listed), vec![(asking.id, 9)]);
        last.read(asking.id, 5, &["old".to_owned()]);
        assert_eq!(last.get(&listed[0]), None);
        last.read(asking.id, 9, &["   ".to_owned()]);
        assert_eq!(last.get(&listed[0]), None);

        // A failed read is asked again; a stale failure is ignored.
        last.forget(asking.id, 5);
        assert!(last.to_read(&listed).is_empty());
        last.forget(asking.id, 9);
        assert_eq!(last.to_read(&listed), vec![(asking.id, 9)]);

        // Leaving the Inbox forgets it; coming back reads again.
        assert!(last.to_read(&[]).is_empty());
        assert_eq!(last.to_read(&listed), vec![(asking.id, 9)]);
    }

    #[test]
    fn sessions_of_unknown_workspaces_are_left_out() {
        let api = project("api");
        let orphan = session(&project("gone"), SessionState::NeedsInput, 1);
        assert!(entries(&[api], &[orphan], &Viewed::default()).is_empty());
    }
}
