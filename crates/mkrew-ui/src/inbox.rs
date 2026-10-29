//! The "needs you" set (SPEC §10, §11): every session that needs input, or finished (done, or
//! an agent that went quiet after working, or one marked unread) and not yet viewed; sessions
//! asking for input first, then newest first. It feeds the sidebar's `Needs you` chip, its bold
//! rows, its footer and "next Inbox entry". Pure and tested headlessly.

use std::collections::HashMap;

use mkrew_protocol::{Event, Project, Session, SessionKind, SessionState, StateSource};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// The file of this computer's viewed state, in the data dir (UI-only: the daemon never reads it).
pub const VIEWED_FILE: &str = "ui_viewed.json";

/// The viewed file of a window: [`VIEWED_FILE`] for this computer's window (`None`), its own
/// `ui_viewed-<key>.json` for a machine window (`instance` = its instance id, e.g.
/// `machine:<name>`; the key is the hash of [`crate::desktop::instance_key`]). Each window has
/// its own because [`Viewed::save`] drops the sessions its own daemon does not list.
pub fn viewed_file(instance: Option<&str>) -> String {
    instance.map_or_else(
        || VIEWED_FILE.to_owned(),
        |id| format!("ui_viewed-{}.json", crate::desktop::instance_key("", id)),
    )
}

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

/// Which finishes the user has seen: a finished session leaves the Inbox once its pane was
/// focused in the active window; a later finish (new `state_changed_at`) shows it again.
/// "Mark unread" (ADR 0064) puts one back until it is looked at again. Kept across UI restarts
/// in [`VIEWED_FILE`] ([`Viewed::load`], [`Viewed::save`]).
#[derive(Debug, Default)]
pub struct Viewed {
    /// Where [`Viewed::save`] writes; `None` = nowhere (a state made by hand).
    path: Option<std::path::PathBuf>,
    /// Session → `state_changed_at` of the state it was last viewed in.
    seen: HashMap<Ulid, i64>,
    /// Agent session → `state_changed_at` of its `working → idle`: an agent without a `done`
    /// hook (or whose `done` decayed through new output) finishes this way.
    quiet: HashMap<Ulid, i64>,
    /// Session → `state_changed_at` of the state it was marked unread in; a state change
    /// lapses it, like `seen`.
    unread: HashMap<Ulid, i64>,
    /// The (session, `state_changed_at`) [`Viewed::view`] saw last. Looking at the same state
    /// again changes nothing, so a pane marked unread while it has the keyboard stays unread.
    last: Option<(Ulid, i64)>,
    /// A map changed since [`Viewed::take_changed`], so the file is stale.
    changed: bool,
}

/// What [`VIEWED_FILE`] holds: the three maps of [`Viewed`].
#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    #[serde(default)]
    seen: HashMap<Ulid, i64>,
    #[serde(default)]
    quiet: HashMap<Ulid, i64>,
    #[serde(default)]
    unread: HashMap<Ulid, i64>,
}

impl Viewed {
    /// The state `json` holds; empty for anything unreadable (it never fails a start).
    pub fn from_json(json: &str) -> Viewed {
        let saved: Saved = serde_json::from_str(json).unwrap_or_default();
        Viewed {
            seen: saved.seen,
            quiet: saved.quiet,
            unread: saved.unread,
            ..Viewed::default()
        }
    }

    /// The JSON [`Viewed::from_json`] reads back.
    pub fn to_json(&self) -> String {
        let saved = Saved {
            seen: self.seen.clone(),
            quiet: self.quiet.clone(),
            unread: self.unread.clone(),
        };
        serde_json::to_string(&saved).unwrap_or_default()
    }

    /// Reads `path`; empty when it is missing or corrupt. [`Viewed::save`] writes it back.
    pub fn load(path: std::path::PathBuf) -> Viewed {
        let mut viewed = std::fs::read_to_string(&path)
            .map_or_else(|_| Viewed::default(), |json| Viewed::from_json(&json));
        viewed.path = Some(path);
        viewed
    }

    /// Writes the state to its file once something changed since the last call (else nothing
    /// happens, not even an allocation), after dropping the sessions that are not in
    /// `sessions` any more. A failed write is only logged: the file is a cache of what the
    /// user saw.
    pub fn save(&mut self, sessions: &[Session]) {
        if !self.take_changed() {
            return;
        }
        self.prune(sessions);
        let Some(file) = &self.path else {
            return;
        };
        if let Err(error) = std::fs::write(file, self.to_json()) {
            tracing::warn!("cannot save {}: {error}", file.display());
        }
    }

    /// Whether a map changed since the last call.
    fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// Forgets the sessions that are not in `sessions`.
    pub fn prune(&mut self, sessions: &[Session]) {
        let known = |id: &Ulid| sessions.iter().any(|session| session.id == *id);
        self.seen.retain(|id, _| known(id));
        self.quiet.retain(|id, _| known(id));
        self.unread.retain(|id, _| known(id));
        self.last.take_if(|(id, _)| !known(id));
    }

    /// `session` is on screen and focused. `true` if that changed what needs the user.
    pub fn view(&mut self, session: &Session) -> bool {
        let state = (session.id, session.state_changed_at);
        if self.last.replace(state) == Some(state) {
            return false;
        }
        let seen = self.seen.insert(session.id, session.state_changed_at);
        let marked = self.unread.remove(&session.id).is_some();
        let changed = seen != Some(session.state_changed_at) || marked;
        self.changed |= changed;
        changed
    }

    /// The window was activated: the focused pane is looked at again, even one marked unread.
    pub fn activated(&mut self) {
        self.last = None;
    }

    /// "Mark unread": `session` needs the user again until it is viewed (see [`can_mark_unread`]).
    pub fn mark_unread(&mut self, session: &Session) {
        self.unread.insert(session.id, session.state_changed_at);
        self.changed = true;
    }

    /// Notes an agent going quiet after work. Call before the sidebar applies `event`, so
    /// `sessions` still holds the previous state.
    pub fn apply(&mut self, event: &Event, sessions: &[Session]) {
        let Event::SessionState(event) = event else {
            return;
        };
        let was_working = sessions.iter().any(|s| {
            s.id == event.session_id
                && s.kind == SessionKind::Agent
                && s.state == SessionState::Working
        });
        if event.state == SessionState::Idle && was_working {
            let before = self.quiet.insert(event.session_id, event.changed_at);
            self.changed |= before != Some(event.changed_at);
        }
    }

    fn seen(&self, session: &Session) -> bool {
        self.seen.get(&session.id) == Some(&session.state_changed_at)
    }

    fn marked(&self, session: &Session) -> bool {
        self.unread.get(&session.id) == Some(&session.state_changed_at)
    }
}

/// A session that needs the user: it asks for input, it finished (done, or idle right after
/// working) and its pane was not focused since (a later finish shows again), or it was marked
/// unread and not viewed since. The one definition behind the Inbox entries, the sidebar chip,
/// its bold rows and the sidebar footer, the taskbar badge and the tray.
pub fn needs_you(session: &Session, viewed: &Viewed) -> bool {
    // An exit changes the state but not `state_changed_at`: an exited session never counts.
    if session.state != SessionState::Exited && viewed.marked(session) {
        return true;
    }
    match session.state {
        SessionState::NeedsInput => true,
        SessionState::Done => !viewed.seen(session),
        SessionState::Idle => {
            viewed.quiet.get(&session.id) == Some(&session.state_changed_at)
                && !viewed.seen(session)
        }
        _ => false,
    }
}

/// Whether "Mark unread" applies: a finished agent (done or idle) that is not already in Needs you.
pub fn can_mark_unread(session: &Session, viewed: &Viewed) -> bool {
    session.kind == SessionKind::Agent
        && matches!(session.state, SessionState::Done | SessionState::Idle)
        && !needs_you(session, viewed)
}

/// The Inbox: sessions that need input first (a blocked agent outranks a finished one), then
/// newest state change first. Sessions of unknown (e.g. archived) workspaces are left out.
pub fn entries(projects: &[Project], sessions: &[Session], viewed: &Viewed) -> Vec<Entry> {
    let mut entries: Vec<Entry> = sessions
        .iter()
        .filter(|session| needs_you(session, viewed))
        .filter_map(|session| {
            let project = projects.iter().find(|p| p.id == session.project_id)?;
            Some(Entry {
                session_id: session.id,
                project_id: session.project_id,
                workspace: project.name.clone(),
                agent: session.agent_profile.clone(),
                title: crate::history::title(session).map(str::to_owned),
                state: session.state,
                state_source: session.state_source,
                state_changed_at: session.state_changed_at,
            })
        })
        .collect();
    entries.sort_by_key(|entry| order(entry.state, entry.state_changed_at, entry.session_id));
    entries
}

/// The Inbox sort key of a session in `state`, changed at `at`.
fn order(state: SessionState, at: i64, id: Ulid) -> std::cmp::Reverse<(bool, i64, Ulid)> {
    std::cmp::Reverse((state == SessionState::NeedsInput, at, id))
}

/// Where `focused` sits in the Inbox: `Ok(index)` when listed; a focused `done` session already
/// left the Inbox once viewed, so `Err(index)` is where it was listed; `None` for any other.
fn place(entries: &[Entry], focused: &Session) -> Option<Result<usize, usize>> {
    if let Some(index) = entries.iter().position(|e| e.session_id == focused.id) {
        return Some(Ok(index));
    }
    (focused.state == SessionState::Done).then(|| {
        let was = order(focused.state, focused.state_changed_at, focused.id);
        Err(entries.partition_point(|e| order(e.state, e.state_changed_at, e.session_id) < was))
    })
}

/// The session "next Inbox entry" (§13) jumps to: the entry after `focused`, wrapping to the
/// first; an unlisted focus (not a viewed `done`) starts at the first entry. `None` if the Inbox
/// is empty.
pub fn next(entries: &[Entry], focused: Option<&Session>) -> Option<Ulid> {
    let after = match focused.and_then(|focused| place(entries, focused)) {
        Some(Ok(index)) => index + 1,
        Some(Err(index)) => index,
        None => 0,
    };
    entries
        .get(after)
        .or_else(|| entries.first())
        .map(|entry| entry.session_id)
}

/// The session "previous Inbox entry" jumps to: the entry before `focused`, wrapping to the
/// last; an unlisted focus (not a viewed `done`) starts at the last entry. `None` if the Inbox
/// is empty.
pub fn previous(entries: &[Entry], focused: Option<&Session>) -> Option<Ulid> {
    let before = match focused.and_then(|focused| place(entries, focused)) {
        Some(Ok(index) | Err(index)) => index.checked_sub(1),
        None => None,
    };
    before
        .and_then(|index| entries.get(index))
        .or_else(|| entries.last())
        .map(|entry| entry.session_id)
}

/// The sidebar footer's Inbox state: `Inbox clear`, `1 needs you`, `3 need you`.
pub fn label(waiting: usize) -> String {
    match waiting {
        0 => "Inbox clear".to_owned(),
        1 => "1 needs you".to_owned(),
        n => format!("{n} need you"),
    }
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
    use super::*;

    fn project(name: &str) -> Project {
        Project {
            id: Ulid::new(),
            jira_keys: Vec::new(),
            position: 0,
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
            title: Some("fix login".to_owned()),
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

    fn ids(entries: &[Entry]) -> Vec<Ulid> {
        entries.iter().map(|entry| entry.session_id).collect()
    }

    #[test]
    fn lists_needs_input_first_then_finished_newest_first() {
        let (api, web) = (project("api"), project("web"));
        let asking = session(&api, SessionState::NeedsInput, 10);
        let finished = session(&web, SessionState::Done, 20);
        let older = session(&web, SessionState::Done, 15);
        let sessions = vec![
            older.clone(),
            asking.clone(),
            session(&api, SessionState::Working, 30),
            session(&api, SessionState::Idle, 40),
            session(&web, SessionState::Exited, 50),
            finished.clone(),
        ];
        let entries = entries(&[api, web], &sessions, &Viewed::default());
        assert_eq!(ids(&entries), vec![asking.id, finished.id, older.id]);
        let second = &entries[1];
        assert_eq!(
            (
                second.workspace.as_str(),
                second.agent.as_str(),
                second.state
            ),
            ("web", "claude", SessionState::Done)
        );
        assert_eq!(second.title.as_deref(), Some("fix login"));
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
    fn an_agent_quiet_after_work_stays_until_viewed_but_a_shell_or_fresh_idle_does_not() {
        let api = project("api");
        let mut agent = session(&api, SessionState::Working, 1);
        let mut shell = Session {
            kind: SessionKind::Shell,
            ..session(&api, SessionState::Working, 1)
        };
        let starting = session(&api, SessionState::Starting, 1);
        let mut viewed = Viewed::default();
        let before = [agent.clone(), shell.clone(), starting.clone()];
        for s in &before {
            viewed.apply(
                &Event::SessionState(mkrew_protocol::SessionStateEvent {
                    session_id: s.id,
                    state: SessionState::Idle,
                    source: StateSource::Screen,
                    changed_at: 7,
                }),
                &before,
            );
        }
        let mut fresh = starting;
        for s in [&mut agent, &mut shell, &mut fresh] {
            s.state = SessionState::Idle;
            s.state_changed_at = 7;
        }
        assert!(needs_you(&agent, &viewed));
        assert!(!needs_you(&shell, &viewed));
        assert!(!needs_you(&fresh, &viewed));

        viewed.view(&agent);
        assert!(!needs_you(&agent, &viewed));
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

        assert_eq!(previous(&entries, None), Some(oldest.id));
        assert_eq!(previous(&entries, Some(&working)), Some(oldest.id));
        assert_eq!(previous(&entries, Some(oldest)), Some(middle.id));
        assert_eq!(previous(&entries, Some(newest)), Some(oldest.id));
        assert_eq!(previous(&[], Some(newest)), None);
    }

    #[test]
    fn stepping_from_a_viewed_done_keeps_its_place_below_the_asking_ones() {
        let api = project("api");
        let asking = session(&api, SessionState::NeedsInput, 0);
        let newest = session(&api, SessionState::Done, 3);
        let done = session(&api, SessionState::Done, 2);
        let oldest = session(&api, SessionState::Done, 1);
        let mut viewed = Viewed::default();
        viewed.view(&done);
        let sessions = [newest.clone(), done.clone(), oldest.clone(), asking.clone()];
        let entries = entries(&[api], &sessions, &viewed);
        assert_eq!(ids(&entries), vec![asking.id, newest.id, oldest.id]);

        assert_eq!(next(&entries, Some(&done)), Some(oldest.id));
        assert_eq!(previous(&entries, Some(&done)), Some(newest.id));
        // The oldest done wraps around.
        let older = Session {
            state_changed_at: 0,
            ..done.clone()
        };
        assert_eq!(next(&entries, Some(&older)), Some(asking.id));
        assert_eq!(previous(&entries, Some(&older)), Some(oldest.id));
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
    fn needs_you_is_input_wanted_or_done_and_unseen() {
        let api = project("api");
        let asking = session(&api, SessionState::NeedsInput, 1);
        let done = session(&api, SessionState::Done, 2);
        let seen = session(&api, SessionState::Done, 3);
        let working = session(&api, SessionState::Working, 4);
        let idle = session(&api, SessionState::Idle, 5);
        let mut viewed = Viewed::default();
        viewed.view(&seen);
        let all = [asking.clone(), done.clone(), seen.clone(), working, idle];
        let flagged: Vec<Ulid> = all
            .iter()
            .filter(|s| needs_you(s, &viewed))
            .map(|s| s.id)
            .collect();
        assert_eq!(flagged, vec![asking.id, done.id]);
        // The chips and the sidebar footer count exactly what the Inbox lists.
        let listed: Vec<Ulid> = entries(&[api], &all, &viewed)
            .iter()
            .map(|e| e.session_id)
            .collect();
        assert_eq!(listed.len(), flagged.len());
        assert!(flagged.iter().all(|id| listed.contains(id)));
    }

    #[test]
    fn sessions_of_unknown_workspaces_are_left_out() {
        let api = project("api");
        let orphan = session(&project("gone"), SessionState::NeedsInput, 1);
        assert!(entries(&[api], &[orphan], &Viewed::default()).is_empty());
    }

    #[test]
    fn the_status_bar_names_the_inbox_state() {
        assert_eq!(label(0), "Inbox clear");
        assert_eq!(label(1), "1 needs you");
        assert_eq!(label(3), "3 need you");
    }

    #[test]
    fn mark_unread_puts_a_viewed_done_back_until_it_is_viewed_again() {
        let api = project("api");
        let done = session(&api, SessionState::Done, 5);
        let other = session(&api, SessionState::Working, 1);
        let projects = [api];
        let mut viewed = Viewed::default();
        assert!(viewed.view(&done));
        assert!(entries(&projects, std::slice::from_ref(&done), &viewed).is_empty());
        assert!(can_mark_unread(&done, &viewed));

        viewed.mark_unread(&done);
        assert_eq!(
            entries(&projects, std::slice::from_ref(&done), &viewed).len(),
            1
        );
        assert!(!can_mark_unread(&done, &viewed), "it already needs you");

        // The pane keeps the keyboard: looking again changes nothing, so the mark stays.
        assert!(!viewed.view(&done));
        assert_eq!(
            entries(&projects, std::slice::from_ref(&done), &viewed).len(),
            1
        );

        // Another pane, then back: viewed again.
        viewed.view(&other);
        assert!(viewed.view(&done));
        assert!(entries(&projects, &[done], &viewed).is_empty());
    }

    #[test]
    fn activating_the_window_looks_at_the_focused_pane_again() {
        let api = project("api");
        let done = session(&api, SessionState::Done, 5);
        let mut viewed = Viewed::default();
        viewed.view(&done);
        viewed.mark_unread(&done);
        assert!(needs_you(&done, &viewed));
        viewed.activated();
        assert!(viewed.view(&done), "the mark was dropped");
        assert!(!needs_you(&done, &viewed));
    }

    #[test]
    fn a_mark_lapses_with_the_state_and_never_lists_an_exited_session() {
        let api = project("api");
        let mut idle = session(&api, SessionState::Idle, 5);
        let mut viewed = Viewed::default();
        assert!(
            !needs_you(&idle, &viewed),
            "idle without work is not Needs you"
        );
        assert!(can_mark_unread(&idle, &viewed));
        viewed.mark_unread(&idle);
        assert!(needs_you(&idle, &viewed));

        // `session.exited` changes the state but not `state_changed_at`.
        idle.state = SessionState::Exited;
        assert!(!needs_you(&idle, &viewed));

        // Working again (a new state): the mark lapses.
        idle.state = SessionState::Working;
        idle.state_changed_at = 6;
        assert!(!needs_you(&idle, &viewed));
    }

    #[test]
    fn only_finished_agents_can_be_marked_unread() {
        let api = project("api");
        let viewed = Viewed::default();
        let can = |state| can_mark_unread(&session(&api, state, 1), &viewed);
        assert!(can(SessionState::Idle));
        assert!(!can(SessionState::Working));
        assert!(!can(SessionState::NeedsInput), "already needs you");
        assert!(!can(SessionState::Starting));
        assert!(!can(SessionState::Exited));
        // Done and not viewed already needs you; a terminal never does.
        assert!(!can(SessionState::Done));
        let shell = Session {
            kind: SessionKind::Shell,
            ..session(&api, SessionState::Idle, 1)
        };
        assert!(!can_mark_unread(&shell, &viewed));
    }

    /// A viewed state with every kind of memory: a seen done, a quiet idle agent, an unread
    /// mark and a pending done, over `sessions` (which `needs_you` is asked about).
    fn remembered() -> (Viewed, Vec<Session>) {
        let api = project("api");
        let seen = session(&api, SessionState::Done, 3);
        let pending = session(&api, SessionState::Done, 4);
        let marked = session(&api, SessionState::Idle, 5);
        let mut quiet = session(&api, SessionState::Working, 6);
        let mut viewed = Viewed::default();
        viewed.view(&seen);
        viewed.mark_unread(&marked);
        viewed.apply(
            &Event::SessionState(mkrew_protocol::SessionStateEvent {
                session_id: quiet.id,
                state: SessionState::Idle,
                source: StateSource::Screen,
                changed_at: 7,
            }),
            std::slice::from_ref(&quiet),
        );
        quiet.state = SessionState::Idle;
        quiet.state_changed_at = 7;
        (viewed, vec![seen, pending, marked, quiet])
    }

    #[test]
    fn a_saved_state_answers_needs_you_like_the_live_one() {
        let (viewed, sessions) = remembered();
        let live: Vec<bool> = sessions.iter().map(|s| needs_you(s, &viewed)).collect();
        assert_eq!(live, [false, true, true, true]);

        let reloaded = Viewed::from_json(&viewed.to_json());
        let after: Vec<bool> = sessions.iter().map(|s| needs_you(s, &reloaded)).collect();
        assert_eq!(after, live);
        // A later finish of the seen session still shows again.
        let mut again = sessions[0].clone();
        again.state_changed_at = 9;
        assert!(needs_you(&again, &reloaded));
    }

    #[test]
    fn saving_drops_the_sessions_that_are_gone() {
        let (mut viewed, sessions) = remembered();
        let dir = tempfile::tempdir().unwrap();
        // Only the seen session and the marked one are still listed.
        let live = [sessions[0].clone(), sessions[2].clone()];
        viewed.path = Some(dir.path().join(VIEWED_FILE));
        viewed.save(&live);

        let reloaded = Viewed::load(dir.path().join(VIEWED_FILE));
        assert!(!needs_you(&sessions[0], &reloaded), "still seen");
        assert!(needs_you(&sessions[2], &reloaded), "still marked");
        assert_eq!(reloaded.seen.len() + reloaded.unread.len(), 2);
        assert!(
            reloaded.quiet.is_empty(),
            "the quiet agent is not listed any more"
        );
        assert!(viewed.quiet.is_empty(), "pruned in memory too");
    }

    #[test]
    fn only_a_change_is_written() {
        let api = project("api");
        let done = session(&api, SessionState::Done, 3);
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(VIEWED_FILE);
        let mut viewed = Viewed::load(file.clone());
        viewed.save(std::slice::from_ref(&done));
        assert!(!file.exists(), "nothing changed yet");

        assert!(viewed.view(&done));
        viewed.save(std::slice::from_ref(&done));
        assert!(file.exists());
        std::fs::remove_file(&file).unwrap();
        // Looking at the same state again changes nothing: no write.
        viewed.activated();
        assert!(!viewed.view(&done));
        viewed.save(std::slice::from_ref(&done));
        assert!(!file.exists());
    }

    #[test]
    fn a_missing_or_corrupt_file_is_an_empty_state() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(VIEWED_FILE);
        let api = project("api");
        let done = session(&api, SessionState::Done, 3);
        assert!(needs_you(&done, &Viewed::load(file.clone())));
        for junk in ["", "not json", "[1,2]", "{\"seen\": {\"x\": 1}}"] {
            std::fs::write(&file, junk).unwrap();
            assert!(needs_you(&done, &Viewed::load(file.clone())), "{junk:?}");
        }
        // Unknown keys and absent maps are fine.
        let partial = Viewed::from_json(&format!("{{\"seen\": {{\"{}\": 3}}, \"x\": 1}}", done.id));
        assert!(!needs_you(&done, &partial));
    }

    #[test]
    fn windows_do_not_overwrite_each_others_viewed_state() {
        let dir = tempfile::tempdir().unwrap();
        let machine_file = viewed_file(Some("machine:home pc"));
        assert_ne!(machine_file, VIEWED_FILE);
        assert_ne!(machine_file, viewed_file(Some("machine:work")));
        assert_eq!(machine_file, viewed_file(Some("machine:HOME PC")));
        assert_eq!(viewed_file(None), VIEWED_FILE);

        let api = project("api");
        let (mine, theirs) = (
            session(&api, SessionState::Done, 3),
            session(&api, SessionState::Done, 4),
        );
        let mut local = Viewed::load(dir.path().join(viewed_file(None)));
        assert!(local.view(&mine));
        local.save(std::slice::from_ref(&mine));
        // The machine window knows none of this computer's sessions: it must not prune them.
        let mut machine = Viewed::load(dir.path().join(&machine_file));
        assert!(machine.view(&theirs));
        machine.save(std::slice::from_ref(&theirs));

        let local = Viewed::load(dir.path().join(viewed_file(None)));
        assert!(!needs_you(&mine, &local), "still seen here");
        let machine = Viewed::load(dir.path().join(&machine_file));
        assert!(!needs_you(&theirs, &machine));
        assert!(needs_you(&mine, &machine), "not seen there");
    }
}
