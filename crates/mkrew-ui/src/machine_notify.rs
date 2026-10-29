//! OS notifications of a machine window (spec §6.5): the other computer's daemon shows its
//! own, on the other computer; the window shows the same ones here. It feeds `session.state`
//! events to the daemon's own policy (`mkrew_daemon::decide`: this computer's `[notify]`
//! switches, one per session per 30 s) and shows them with its notifier (`MKREW_NOTIFY`
//! honored). The toast's `mkrew://` link names the machine; where a click comes back on a
//! channel instead (not Windows) [`MachineNotices::take_clicks`] reads it.

use std::sync::Arc;
use std::time::Instant;

use mkrew_core::settings::NotifySettings;
use mkrew_daemon::{Clicked, Notice, Notifier, Throttle, decide};
use mkrew_protocol::{Event, Project, Session};
use tokio::sync::mpsc;
use ulid::Ulid;

pub struct MachineNotices {
    machine: String,
    /// `None` = notifications off (`MKREW_NOTIFY=off`).
    notifier: Option<Arc<dyn Notifier>>,
    throttle: Throttle,
    clicked: Clicked,
    clicks: mpsc::UnboundedReceiver<Notice>,
}

impl MachineNotices {
    pub fn new(machine: String, notifier: Option<Arc<dyn Notifier>>) -> Self {
        let (clicked, clicks) = mpsc::unbounded_channel();
        Self {
            machine,
            notifier,
            throttle: Throttle::default(),
            clicked,
            clicks,
        }
    }

    /// Shows the notice `event` asks for, if any. The title is the daemon's (`<workspace> ·
    /// <session name>`: the user's title, else the terminal title, else the profile).
    pub fn on_event(
        &mut self,
        event: &Event,
        notify: &NotifySettings,
        sessions: &[Session],
        projects: &[Project],
        now: Instant,
    ) {
        let (Event::SessionState(change), Some(notifier)) = (event, &self.notifier) else {
            return;
        };
        let Some(alert) = decide(
            change.session_id,
            change.state,
            notify,
            &mut self.throttle,
            now,
        ) else {
            return;
        };
        let Some(session) = sessions.iter().find(|s| s.id == change.session_id) else {
            return;
        };
        let Some(project) = projects.iter().find(|p| p.id == session.project_id) else {
            return;
        };
        notifier.show(
            Notice {
                session_id: session.id,
                project_id: project.id,
                title: format!("{} · {}", project.name, crate::history::label(session)),
                body: alert.body.to_owned(),
                sound: alert.sound,
                machine: Some(self.machine.clone()),
            },
            self.clicked.clone(),
        );
    }

    /// The (session, workspace) of every notice clicked since the last call.
    pub fn take_clicks(&mut self) -> Vec<(Ulid, Ulid)> {
        std::iter::from_fn(|| self.clicks.try_recv().ok())
            .map(|notice| (notice.session_id, notice.project_id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use mkrew_protocol::{SessionKind, SessionState, SessionStateEvent, StateSource};

    use super::*;

    #[derive(Default)]
    struct Fake(Mutex<Vec<(Notice, Clicked)>>);

    impl Notifier for Fake {
        fn show(&self, notice: Notice, clicked: Clicked) {
            self.0.lock().unwrap().push((notice, clicked));
        }
    }

    fn project(name: &str) -> Project {
        Project {
            id: Ulid::new(),
            name: name.to_owned(),
            path: format!("/home/me/{name}"),
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
            jira_keys: Vec::new(),
            position: 0,
        }
    }

    fn session(project: &Project) -> Session {
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
            state: SessionState::Working,
            state_source: StateSource::Hook,
            state_changed_at: 0,
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

    fn state(session: &Session, state: SessionState) -> Event {
        Event::SessionState(SessionStateEvent {
            session_id: session.id,
            state,
            source: StateSource::Hook,
            changed_at: 1,
        })
    }

    fn notices(fake: &Arc<Fake>) -> Vec<Notice> {
        fake.0
            .lock()
            .unwrap()
            .iter()
            .map(|(n, _)| n.clone())
            .collect()
    }

    #[test]
    fn a_needs_input_session_notifies_here_with_the_machine_in_the_link() {
        let repo = project("repo");
        let mut agent = session(&repo);
        agent.terminal_title = Some("Fix login".into());
        let fake = Arc::new(Fake::default());
        let mut notices_of = MachineNotices::new("Home PC".into(), Some(fake.clone()));
        let (sessions, projects) = ([agent.clone()], [repo.clone()]);
        let settings = NotifySettings::default();
        let t0 = Instant::now();

        // Working is no notice; needs_input is, with the daemon's title rule.
        notices_of.on_event(
            &state(&agent, SessionState::Working),
            &settings,
            &sessions,
            &projects,
            t0,
        );
        assert!(notices(&fake).is_empty());
        notices_of.on_event(
            &state(&agent, SessionState::NeedsInput),
            &settings,
            &sessions,
            &projects,
            t0,
        );
        assert_eq!(
            notices(&fake),
            [Notice {
                session_id: agent.id,
                project_id: repo.id,
                title: "repo · Fix login".into(),
                body: "Needs input".into(),
                sound: settings.sound,
                machine: Some("Home PC".into()),
            }]
        );

        // Once per 30 s per session, then again.
        let soon = t0 + Duration::from_secs(5);
        notices_of.on_event(
            &state(&agent, SessionState::Done),
            &settings,
            &sessions,
            &projects,
            soon,
        );
        assert_eq!(notices(&fake).len(), 1);
        let later = t0 + Duration::from_secs(31);
        notices_of.on_event(
            &state(&agent, SessionState::Done),
            &settings,
            &sessions,
            &projects,
            later,
        );
        assert_eq!(notices(&fake).len(), 2);
    }

    #[test]
    fn this_computers_notify_switches_decide() {
        let repo = project("repo");
        let agent = session(&repo);
        let fake = Arc::new(Fake::default());
        let mut notices_of = MachineNotices::new("Home PC".into(), Some(fake.clone()));
        let (sessions, projects) = ([agent.clone()], [repo]);
        let quiet = NotifySettings {
            needs_input: false,
            ..NotifySettings::default()
        };
        notices_of.on_event(
            &state(&agent, SessionState::NeedsInput),
            &quiet,
            &sessions,
            &projects,
            Instant::now(),
        );
        assert!(notices(&fake).is_empty());
    }

    #[test]
    fn unknown_sessions_and_a_missing_notifier_show_nothing() {
        let repo = project("repo");
        let agent = session(&repo);
        let fake = Arc::new(Fake::default());
        let mut unlisted = MachineNotices::new("Home PC".into(), Some(fake.clone()));
        unlisted.on_event(
            &state(&agent, SessionState::NeedsInput),
            &NotifySettings::default(),
            &[],
            std::slice::from_ref(&repo),
            Instant::now(),
        );
        assert!(notices(&fake).is_empty());

        let mut off = MachineNotices::new("Home PC".into(), None);
        off.on_event(
            &state(&agent, SessionState::NeedsInput),
            &NotifySettings::default(),
            std::slice::from_ref(&agent),
            &[repo],
            Instant::now(),
        );
        assert!(off.take_clicks().is_empty());
    }

    #[test]
    fn a_click_comes_back_as_the_session_and_workspace() {
        let repo = project("repo");
        let agent = session(&repo);
        let fake = Arc::new(Fake::default());
        let mut notices_of = MachineNotices::new("Home PC".into(), Some(fake.clone()));
        notices_of.on_event(
            &state(&agent, SessionState::NeedsInput),
            &NotifySettings::default(),
            std::slice::from_ref(&agent),
            std::slice::from_ref(&repo),
            Instant::now(),
        );
        assert!(notices_of.take_clicks().is_empty());
        let (notice, clicked) = fake.0.lock().unwrap().remove(0);
        clicked.send(notice).unwrap();
        assert_eq!(notices_of.take_clicks(), [(agent.id, repo.id)]);
        assert!(notices_of.take_clicks().is_empty());
    }
}
