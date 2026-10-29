//! OS notifications (SPEC §8, §20A.2): one when a session enters needs_input or done, unless
//! `notify.needs_input` / `notify.done` turns that kind off, at most one per session per
//! [`THROTTLE`], with the OS default sound when `notify.sound`. A click shows the session: on
//! Windows the toast opens its `mkrew://` link (ADR 0079), elsewhere it becomes a
//! `notification.clicked` event.
//!
//! The policy is `decide`; [`start`] applies it and is tested with a fake [`Notifier`].
//! `MKREW_NOTIFY` picks the backend: unset = the OS, `off` = none, anything else = a JSON-lines
//! file path (tests read it instead of showing real toasts).

use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mkrew_core::settings::NotifySettings;
use mkrew_protocol::{Event, NotificationClickedEvent, SessionState};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use ulid::Ulid;

use crate::session::Hub;
use crate::settings::SettingsHolder;

/// Selects the notifier: unset = OS, `off` = none, else a JSON-lines file.
pub const ENV_NOTIFY: &str = "MKREW_NOTIFY";
pub const THROTTLE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub session_id: Ulid,
    pub project_id: Ulid,
    pub title: String,
    pub body: String,
    /// Play the OS default sound.
    pub sound: bool,
    /// The machine the session runs on, when a machine window shows this notice on this
    /// computer (the link then names it); `None` = this computer's own daemon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
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
        None => {
            #[cfg(windows)]
            {
                // Before any WinRT call, see `keep_mta_alive`.
                keep_mta_alive();
                crate::wintoast::register_link_handler();
            }
            Some(Arc::new(OsNotifier))
        }
        Some(v) if v == "off" => None,
        Some(path) => Some(Arc::new(FileNotifier(PathBuf::from(path)))),
    }
}

/// Watches `hub` events and notifies with the current `settings`; runs until aborted.
pub fn start(
    hub: Arc<Hub>,
    settings: Arc<SettingsHolder>,
    notifier: Arc<dyn Notifier>,
) -> JoinHandle<()> {
    let mut events = hub.events.subscribe();
    tokio::spawn(async move {
        let (clicked, mut clicks) = mpsc::unbounded_channel();
        let mut throttle = Throttle::default();
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(Event::SessionState(e)) => {
                        let notify = settings.current().notify;
                        let alert =
                            decide(e.session_id, e.state, &notify, &mut throttle, Instant::now());
                        let Some(alert) = alert else { continue };
                        if let Some(notice) = notice(&hub, e.session_id, alert) {
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

/// What to show for one state change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Alert {
    pub body: &'static str,
    pub sound: bool,
}

/// The notification for `session_id` entering `state`, if any: only needs_input and done,
/// each behind its switch, then the throttle (a switched-off kind does not use up the window;
/// an allowed one does, even if the session lookup for the notice then fails).
/// Also used by `mk web` for phone push (ADR 0067): one policy for toast and push.
pub fn decide(
    session_id: Ulid,
    state: SessionState,
    settings: &NotifySettings,
    throttle: &mut Throttle,
    now: Instant,
) -> Option<Alert> {
    let body = match state {
        SessionState::NeedsInput if settings.needs_input => "Needs input",
        SessionState::Done if settings.done => "Done",
        _ => return None,
    };
    throttle.allow(session_id, now).then_some(Alert {
        body,
        sound: settings.sound,
    })
}

fn notice(hub: &Hub, session_id: Ulid, alert: Alert) -> Option<Notice> {
    let store = hub.store();
    let session = match store.list_sessions(None) {
        Ok(sessions) => sessions.into_iter().find(|s| s.id == session_id)?,
        Err(e) => {
            tracing::error!(%session_id, "notification lookup failed: {e}");
            return None;
        }
    };
    let project = store.get_project(session.project_id).ok()?;
    // The user's name, else the agent's own title, else the profile (ADR 0031).
    let name = session
        .title
        .or_else(|| hub.terminal_title(session_id))
        .unwrap_or(session.agent_profile);
    Some(Notice {
        session_id,
        project_id: project.id,
        title: format!("{} · {name}", project.name),
        body: alert.body.to_owned(),
        sound: alert.sound,
        machine: None,
    })
}

/// At most one notice per session per [`THROTTLE`].
#[derive(Debug, Default)]
pub struct Throttle {
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

/// The OS notifier. Windows: a WinRT toast whose click opens the session's `mkrew://` link
/// (ADR 0079), so `clicked` is not used there. Elsewhere `notify-rust`: `show` and waiting for
/// the click both block, so each notice gets its own short-lived thread (it ends on click or
/// dismissal; the throttle bounds how many exist).
struct OsNotifier;

#[cfg(windows)]
impl Notifier for OsNotifier {
    fn show(&self, notice: Notice, _clicked: Clicked) {
        // WinRT calls block and the notify task is async; the thread ends right after `Show`.
        let spawned = std::thread::Builder::new()
            .name("notification".into())
            .spawn(move || {
                if let Err(e) = crate::wintoast::show(&notice) {
                    tracing::warn!("showing a notification failed: {e}");
                }
            });
        if let Err(e) = spawned {
            tracing::warn!("starting a notification thread failed: {e}");
        }
    }
}

#[cfg(not(windows))]
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
                set_sound(&mut notification, notice.sound);
                // Linux servers only report a body click for a "default" action.
                #[cfg(not(target_os = "macos"))]
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

/// windows-rs caches WinRT factories process-wide and pins the MTA only when no MTA exists yet.
/// A toast's factory cached while another component's MTA exists is left pointing into an
/// unloaded DLL once that component's `CoUninitialize` ends the MTA: the next toast crashes
/// (0xc0000005 activating `XmlDocument`). Holding an MTA usage for the process lifetime is the
/// documented fix (windows-rs #1169; Old New Thing, 2021-11-05 "cached factories").
#[cfg(windows)]
#[allow(unsafe_code)]
fn keep_mta_alive() {
    use windows_sys::Win32::System::Com::CoIncrementMTAUsage;
    let mut cookie = std::ptr::null_mut();
    // SAFETY: `cookie` is a valid out pointer; it is never passed to `CoDecrementMTAUsage`, so
    // the MTA lives until the process exits.
    let hr = unsafe { CoIncrementMTAUsage(&mut cookie) };
    if hr < 0 {
        tracing::warn!(hr, "keeping the COM MTA alive failed");
    }
}

/// The OS default sound, or silence (no sound name).
#[cfg(target_os = "macos")]
fn set_sound(notification: &mut notify_rust::Notification, sound: bool) {
    if sound {
        // mac-notification-sys maps exactly this string to the system default sound.
        notification.sound_name("NSUserNotificationDefaultSoundName");
    }
}

/// A freedesktop sound hint; whether it plays is up to the notification server. Linux has no
/// single default sound: "message-new-instant" is the sound-naming spec's generic alert.
#[cfg(all(unix, not(target_os = "macos")))]
fn set_sound(notification: &mut notify_rust::Notification, sound: bool) {
    if sound {
        notification.sound_name("message-new-instant");
    } else {
        notification.hint(notify_rust::Hint::SuppressSound(true));
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

    /// The crash of 2026-10-14: a toast's WinRT factory is cached while another component's
    /// MTA exists; that component's `CoUninitialize` must not end the MTA (COM would unload
    /// the factory's DLL under the cache).
    #[cfg(windows)]
    #[allow(unsafe_code)]
    #[test]
    fn mta_outlives_another_components_couninitialize() {
        use windows_sys::Win32::System::Com::{
            APTTYPE_MTA, COINIT_MULTITHREADED, CoGetApartmentType, CoInitializeEx, CoUninitialize,
        };
        keep_mta_alive();
        std::thread::spawn(|| unsafe {
            assert!(CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) >= 0);
            CoUninitialize();
        })
        .join()
        .unwrap();
        let apartment = std::thread::spawn(|| {
            let (mut kind, mut qualifier) = (0, 0);
            let hr = unsafe { CoGetApartmentType(&mut kind, &mut qualifier) };
            (hr, kind)
        })
        .join()
        .unwrap();
        assert_eq!(apartment, (0, APTTYPE_MTA));
    }

    /// Settings read from `<dir>/settings.toml` (missing = defaults).
    fn settings(dir: &tempfile::TempDir) -> Arc<SettingsHolder> {
        Arc::new(SettingsHolder::load(dir.path().join("settings.toml")))
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
                terminal_title: None,
                color: None,
                native_session_id: None,
                state: SessionState::Working,
                state_source: StateSource::Daemon,
                state_changed_at: 0,
                exit_code: None,
                cols: 80,
                rows: 24,
                started_at: 0,
                ended_at: None,
                pinned: false,
                position: 0.0,
                plan_path: None,
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

    fn switches(needs_input: bool, done: bool, sound: bool) -> NotifySettings {
        NotifySettings {
            needs_input,
            done,
            sound,
            ..NotifySettings::default()
        }
    }

    #[test]
    fn decide_covers_every_kind_and_switch() {
        let t0 = Instant::now();
        let needs = Some(Alert {
            body: "Needs input",
            sound: true,
        });
        let done = Some(Alert {
            body: "Done",
            sound: true,
        });
        let cases = [
            (SessionState::NeedsInput, switches(true, true, true), needs),
            (SessionState::NeedsInput, switches(false, true, true), None),
            (SessionState::Done, switches(true, true, true), done),
            (SessionState::Done, switches(true, false, true), None),
            (SessionState::Working, switches(true, true, true), None),
            (SessionState::Idle, switches(true, true, true), None),
            (SessionState::Exited, switches(true, true, true), None),
        ];
        for (state, settings, expected) in cases {
            let mut throttle = Throttle::default();
            let got = decide(Ulid::new(), state, &settings, &mut throttle, t0);
            assert_eq!(got, expected, "{state:?} with {settings:?}");
        }
    }

    #[test]
    fn decide_reports_the_sound_switch() {
        let mut throttle = Throttle::default();
        let alert = decide(
            Ulid::new(),
            SessionState::Done,
            &switches(true, true, false),
            &mut throttle,
            Instant::now(),
        );
        assert_eq!(
            alert,
            Some(Alert {
                body: "Done",
                sound: false,
            })
        );
    }

    #[test]
    fn decide_throttles_per_session_and_a_switched_off_kind_keeps_the_window_free() {
        let mut throttle = Throttle::default();
        let session = Ulid::new();
        let t0 = Instant::now();
        let quiet_done = switches(true, false, true);
        let all = switches(true, true, true);
        // Done is off: nothing shown, and the window is not used up.
        assert_eq!(
            decide(session, SessionState::Done, &quiet_done, &mut throttle, t0),
            None
        );
        assert!(decide(session, SessionState::NeedsInput, &all, &mut throttle, t0).is_some());
        let later = t0 + THROTTLE - Duration::from_millis(1);
        assert_eq!(
            decide(session, SessionState::Done, &all, &mut throttle, later),
            None
        );
        let other = Ulid::new();
        assert!(decide(other, SessionState::Done, &all, &mut throttle, later).is_some());
        assert!(
            decide(
                session,
                SessionState::Done,
                &all,
                &mut throttle,
                t0 + THROTTLE
            )
            .is_some()
        );
    }

    #[tokio::test]
    async fn switches_and_sound_follow_the_settings_file() {
        let dir = tempfile::tempdir().unwrap();
        let (hub, project_id) = hub(&dir);
        let settings = settings(&dir);
        std::fs::write(
            dir.path().join("settings.toml"),
            "[notify]\ndone = false\nsound = false\n",
        )
        .unwrap();
        settings.reload();
        let first = add_session(&hub, project_id, None);
        let second = add_session(&hub, project_id, None);
        let fake = Arc::new(Fake::default());
        let task = start(Arc::clone(&hub), Arc::clone(&settings), fake.clone());

        state(&hub, first, SessionState::Done);
        state(&hub, second, SessionState::NeedsInput);
        let notices = fake.wait_for(1).await;
        assert_eq!(
            notices,
            [Notice {
                session_id: second,
                project_id,
                title: "repo · claude".into(),
                body: "Needs input".into(),
                sound: false,
                machine: None,
            }]
        );

        // A change while running applies to the next event.
        std::fs::write(
            dir.path().join("settings.toml"),
            "[notify]\nneeds_input = false\n",
        )
        .unwrap();
        settings.reload();
        let third = add_session(&hub, project_id, None);
        let fourth = add_session(&hub, project_id, None);
        state(&hub, third, SessionState::NeedsInput);
        state(&hub, fourth, SessionState::Done);
        let notices = fake.wait_for(2).await;
        assert_eq!(
            notices[1..],
            [Notice {
                session_id: fourth,
                project_id,
                title: "repo · claude".into(),
                body: "Done".into(),
                sound: true,
                machine: None,
            }]
        );
        task.abort();
    }

    #[tokio::test]
    async fn needs_input_and_done_notify_once_per_session_per_window() {
        let dir = tempfile::tempdir().unwrap();
        let (hub, project_id) = hub(&dir);
        let first = add_session(&hub, project_id, None);
        let second = add_session(&hub, project_id, Some("fix tests"));
        let fake = Arc::new(Fake::default());
        let task = start(Arc::clone(&hub), settings(&dir), fake.clone());

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
                    sound: true,
                    machine: None,
                },
                Notice {
                    session_id: second,
                    project_id,
                    title: "repo · fix tests".into(),
                    body: "Done".into(),
                    sound: true,
                    machine: None,
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
        let task = start(Arc::clone(&hub), settings(&dir), fake.clone());
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
            sound: true,
            machine: None,
        };
        let (clicked, _) = mpsc::unbounded_channel();
        notifier.show(notice.clone(), clicked.clone());
        notifier.show(notice.clone(), clicked);
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["session_id"], notice.session_id.to_string());
        assert_eq!(lines[0]["body"], "Done");
        assert!(
            lines[0].get("machine").is_none(),
            "a daemon's own notice has no machine key"
        );
        let (clicked, _) = mpsc::unbounded_channel();
        let remote = Notice {
            machine: Some("Home PC".into()),
            ..notice
        };
        notifier.show(remote, clicked);
        let text = std::fs::read_to_string(&path).unwrap();
        let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        assert_eq!(last["machine"], "Home PC");
    }
}
