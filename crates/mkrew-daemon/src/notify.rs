//! OS notifications (SPEC §8): one when a session enters needs_input or done, at most one per
//! session per [`THROTTLE`]; a click becomes a `notification.clicked` event.
//!
//! The policy lives in [`start`] and is tested with a fake [`Notifier`]. `MKREW_NOTIFY`
//! picks the backend: unset = the OS, `off` = none, anything else = a JSON-lines file path
//! (tests read it instead of showing real toasts).

use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mkrew_protocol::{Event, NotificationClickedEvent, SessionState};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use ulid::Ulid;

use crate::session::Hub;

/// Selects the notifier: unset = OS, `off` = none, else a JSON-lines file.
pub const ENV_NOTIFY: &str = "MKREW_NOTIFY";
pub const THROTTLE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub session_id: Ulid,
    pub project_id: Ulid,
    pub title: String,
    pub body: String,
}

/// Where a click on a shown notice is reported.
pub type Clicked = mpsc::UnboundedSender<Notice>;

pub trait Notifier: Send + Sync + 'static {
    /// Shows `notice` without blocking the caller; a click sends it to `clicked`.
    fn show(&self, notice: Notice, clicked: Clicked);
}

/// The notifier `MKREW_NOTIFY` asks for; `None` = notifications off.
pub fn from_env() -> Option<Arc<dyn Notifier>> {
    match std::env::var(ENV_NOTIFY).ok().filter(|v| !v.is_empty()) {
        None => Some(Arc::new(OsNotifier)),
        Some(v) if v == "off" => None,
        Some(path) => Some(Arc::new(FileNotifier(PathBuf::from(path)))),
    }
}

/// Watches `hub` events and notifies; runs until aborted.
pub fn start(hub: Arc<Hub>, notifier: Arc<dyn Notifier>) -> JoinHandle<()> {
    let mut events = hub.events.subscribe();
    tokio::spawn(async move {
        let (clicked, mut clicks) = mpsc::unbounded_channel();
        let mut throttle = Throttle::default();
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(Event::SessionState(e)) => {
                        let Some(body) = body(e.state) else { continue };
                        let Some(notice) = notice(&hub, e.session_id, body) else { continue };
                        if throttle.allow(e.session_id, Instant::now()) {
                            notifier.show(notice, clicked.clone());
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(n, "notifications missed events");
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                },
                Some(notice) = clicks.recv() => {
                    let _ = hub.events.send(Event::NotificationClicked(NotificationClickedEvent {
                        session_id: notice.session_id,
                        project_id: notice.project_id,
                    }));
                }
            }
        }
    })
}

fn body(state: SessionState) -> Option<&'static str> {
    match state {
        SessionState::NeedsInput => Some("Needs input"),
        SessionState::Done => Some("Done"),
        _ => None,
    }
}

fn notice(hub: &Hub, session_id: Ulid, body: &str) -> Option<Notice> {
    let store = hub.store();
    let session = match store.list_sessions(None) {
        Ok(sessions) => sessions.into_iter().find(|s| s.id == session_id)?,
        Err(e) => {
            tracing::error!(%session_id, "notification lookup failed: {e}");
            return None;
        }
    };
    let project = store.get_project(session.project_id).ok()?;
    let name = session.title.unwrap_or(session.agent_profile);
    Some(Notice {
        session_id,
        project_id: project.id,
        title: format!("{} · {name}", project.name),
        body: body.to_owned(),
    })
}

/// At most one notice per session per [`THROTTLE`].
#[derive(Debug, Default)]
struct Throttle {
    last: HashMap<Ulid, Instant>,
}

impl Throttle {
    fn allow(&mut self, session_id: Ulid, now: Instant) -> bool {
        self.last
            .retain(|_, shown| now.saturating_duration_since(*shown) < THROTTLE);
        if self.last.contains_key(&session_id) {
            return false;
        }
        self.last.insert(session_id, now);
        true
    }
}

/// `notify-rust`. `show` and waiting for the click both block, so each notice gets its own
/// short-lived thread (it ends on click or dismissal; the throttle bounds how many exist).
struct OsNotifier;

impl Notifier for OsNotifier {
    fn show(&self, notice: Notice, clicked: Clicked) {
        let spawned = std::thread::Builder::new()
            .name("notification".into())
            .spawn(move || {
                let mut notification = notify_rust::Notification::new();
                notification
                    .appname("Mkrew")
                    .summary(&notice.title)
                    .body(&notice.body);
                // Linux servers only report a body click for a "default" action.
                #[cfg(all(unix, not(target_os = "macos")))]
                notification.action("default", "Open");
                let handle = match notification.show() {
                    Ok(handle) => handle,
                    Err(e) => return tracing::warn!("showing a notification failed: {e}"),
                };
                // macOS only sends from `wait_*` with a running main run loop, which the
                // daemon lacks: dropping the handle sends it, clicks are not reported.
                #[cfg(not(target_os = "macos"))]
                {
                    use notify_rust::NotificationResponse;
                    let _ = handle.wait_for_response(|response: &NotificationResponse| {
                        if matches!(
                            response,
                            NotificationResponse::Default | NotificationResponse::Action(_)
                        ) {
                            let _ = clicked.send(notice.clone());
                        }
                    });
                }
                #[cfg(target_os = "macos")]
                drop((handle, clicked));
            });
        if let Err(e) = spawned {
            tracing::warn!("starting a notification thread failed: {e}");
        }
    }
}

/// Appends each notice as one JSON line; never reports clicks.
struct FileNotifier(PathBuf);

impl Notifier for FileNotifier {
    fn show(&self, notice: Notice, _clicked: Clicked) {
        let written = serde_json::to_string(&notice)
            .map_err(std::io::Error::other)
            .and_then(|line| {
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.0)?;
                writeln!(file, "{line}")
            });
        if let Err(e) = written {
            tracing::warn!(path = %self.0.display(), "writing a notification failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use mkrew_protocol::{Session, SessionKind, SessionStateEvent, StateSource};
    use mkrew_store::Store;

    use super::*;

    #[derive(Default)]
    struct Fake(Mutex<Vec<(Notice, Clicked)>>);

    impl Notifier for Fake {
        fn show(&self, notice: Notice, clicked: Clicked) {
            self.0.lock().unwrap().push((notice, clicked));
        }
    }

    impl Fake {
        fn notices(&self) -> Vec<Notice> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|(n, _)| n.clone())
                .collect()
        }

        async fn wait_for(&self, count: usize) -> Vec<Notice> {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let notices = self.notices();
                if notices.len() >= count || Instant::now() > deadline {
                    return notices;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }

    fn hub(dir: &tempfile::TempDir) -> (Arc<Hub>, Ulid) {
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let project = store.add_project(dir.path(), Some("repo")).unwrap();
        (Arc::new(Hub::new(store)), project.id)
    }

    fn add_session(hub: &Hub, project_id: Ulid, title: Option<&str>) -> Ulid {
        let id = Ulid::new();
        hub.store()
            .insert_session(&Session {
                id,
                task_id: None,
                project_id,
                agent_profile: "claude".into(),
                kind: SessionKind::Agent,
                title: title.map(Into::into),
                native_session_id: None,
                state: SessionState::Working,
                state_source: StateSource::Daemon,
                state_changed_at: 0,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 0,
                ended_at: None,
            })
            .unwrap();
        id
    }

    fn state(hub: &Hub, session_id: Ulid, state: SessionState) {
        hub.events
            .send(Event::SessionState(SessionStateEvent {
                session_id,
                state,
                source: StateSource::Hook,
                changed_at: 0,
            }))
            .unwrap();
    }

    #[test]
    fn throttle_allows_one_notice_per_session_per_window() {
        let mut throttle = Throttle::default();
        let (a, b) = (Ulid::new(), Ulid::new());
        let t0 = Instant::now();
        assert!(throttle.allow(a, t0));
        assert!(throttle.allow(b, t0));
        assert!(!throttle.allow(a, t0 + THROTTLE - Duration::from_millis(1)));
        assert!(throttle.allow(a, t0 + THROTTLE));
        assert!(!throttle.allow(a, t0 + THROTTLE));
    }

    #[tokio::test]
    async fn needs_input_and_done_notify_once_per_session_per_window() {
        let dir = tempfile::tempdir().unwrap();
        let (hub, project_id) = hub(&dir);
        let first = add_session(&hub, project_id, None);
        let second = add_session(&hub, project_id, Some("fix tests"));
        let fake = Arc::new(Fake::default());
        let task = start(Arc::clone(&hub), fake.clone());

        state(&hub, first, SessionState::Working);
        state(&hub, first, SessionState::Idle);
        state(&hub, first, SessionState::NeedsInput);
        state(&hub, first, SessionState::Working);
        state(&hub, first, SessionState::Done);
        // Events are handled in order: once `second` shows up, `first` got all of its own.
        state(&hub, second, SessionState::Done);
        let notices = fake.wait_for(2).await;
        assert_eq!(
            notices,
            [
                Notice {
                    session_id: first,
                    project_id,
                    title: "repo · claude".into(),
                    body: "Needs input".into(),
                },
                Notice {
                    session_id: second,
                    project_id,
                    title: "repo · fix tests".into(),
                    body: "Done".into(),
                },
            ]
        );
        task.abort();
    }

    #[tokio::test]
    async fn a_click_emits_notification_clicked() {
        let dir = tempfile::tempdir().unwrap();
        let (hub, project_id) = hub(&dir);
        let session_id = add_session(&hub, project_id, None);
        let fake = Arc::new(Fake::default());
        let task = start(Arc::clone(&hub), fake.clone());
        let mut events = hub.events.subscribe();

        state(&hub, session_id, SessionState::NeedsInput);
        assert_eq!(fake.wait_for(1).await.len(), 1);
        let (notice, clicked) = fake.0.lock().unwrap().remove(0);
        clicked.send(notice).unwrap();
        let clicked = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Event::NotificationClicked(e) = events.recv().await.unwrap() {
                    return e;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(
            clicked,
            NotificationClickedEvent {
                session_id,
                project_id
            }
        );
        task.abort();
    }

    #[test]
    fn the_file_notifier_appends_json_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notices.jsonl");
        let notifier = FileNotifier(path.clone());
        let notice = Notice {
            session_id: Ulid::new(),
            project_id: Ulid::new(),
            title: "repo · claude".into(),
            body: "Done".into(),
        };
        let (clicked, _) = mpsc::unbounded_channel();
        notifier.show(notice.clone(), clicked.clone());
        notifier.show(notice.clone(), clicked);
        let text = std::fs::read_to_string(path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["session_id"], notice.session_id.to_string());
        assert_eq!(lines[0]["body"], "Done");
    }
}
