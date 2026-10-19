//! Web Push from `mk web` (ADR 0066): when a session enters needs_input or done, every phone
//! that tapped the bell gets a system notification through its browser's push service. The
//! policy is the desktop toast's (`mkrew_daemon::decide`: `[notify]` switches, one per session
//! per 30 s) plus `[web] push`. Nothing here talks to the network except [`UreqSender`]; tests
//! use a fake [`Sender`], and `MKREW_PUSH=<file>` swaps in a file sink.

mod crypto;
mod store;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use mkrew_core::settings::{self, Settings};
use mkrew_daemon::{Throttle, decide};
use mkrew_protocol::SessionState;
use serde::Serialize;
use ulid::Ulid;

use store::{Store, Subscription, endpoint_allowed, endpoint_in};

/// `MKREW_PUSH=<file>`: append each message to the file as a JSON line instead of sending it
/// (tests read it, like `MKREW_NOTIFY`).
pub const ENV_PUSH: &str = "MKREW_PUSH";
/// How long a push service holds a message for a phone that is off: a notice older than this is noise.
const TTL_SECONDS: &str = "3600";
const SEND_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest session or workspace name in a notification.
const NAME_CHARS: usize = 60;

/// One HTTPS POST to a push service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    Delivered,
    /// The push service says the subscription is gone (404 / 410): forget the phone.
    Gone,
    Failed(String),
}

pub trait Sender: Send + Sync + 'static {
    /// Blocking.
    fn send(&self, post: &Post) -> Sent;
}

/// The real thing: `ureq` over rustls.
pub struct UreqSender;

impl Sender for UreqSender {
    fn send(&self, post: &Post) -> Sent {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(SEND_TIMEOUT))
            // 4xx / 5xx are answers to read, not transport errors.
            .http_status_as_error(false)
            // A push service must not bounce `mk web` to another host (ADR 0066): 3xx = failed.
            .max_redirects(0)
            .build()
            .into();
        let mut request = agent.post(post.url.as_str());
        for (name, value) in &post.headers {
            request = request.header(*name, value.as_str());
        }
        match request.send(&post.body[..]) {
            Ok(response) => match response.status().as_u16() {
                200..=299 => Sent::Delivered,
                404 | 410 => Sent::Gone,
                other => Sent::Failed(format!("HTTP {other}")),
            },
            Err(e) => Sent::Failed(e.to_string()),
        }
    }
}

/// `MKREW_PUSH` sink: one JSON line per message (url, headers, body length).
struct FileSender(PathBuf);

impl Sender for FileSender {
    fn send(&self, post: &Post) -> Sent {
        let line = serde_json::json!({
            "url": post.url,
            "headers": post.headers,
            "body_len": post.body.len(),
        });
        let written = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.0)
            .and_then(|mut file| writeln!(file, "{line}"));
        match written {
            Ok(()) => Sent::Delivered,
            Err(e) => Sent::Failed(e.to_string()),
        }
    }
}

pub fn sender_from_env() -> Arc<dyn Sender> {
    match std::env::var(ENV_PUSH).ok().filter(|v| !v.is_empty()) {
        Some(path) => Arc::new(FileSender(PathBuf::from(path))),
        None => Arc::new(UreqSender),
    }
}

/// What the phone shows; encrypted into the push message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Payload {
    pub title: String,
    pub body: String,
    /// The session id the page opens when the notification is tapped.
    pub session: String,
}

/// `name · project` / `needs you` or `done`; `None` for any other state. (Name first: that is
/// this lock-screen title's own order, not the desktop toast's `project · name`.)
pub fn payload(
    session_id: Ulid,
    name: &str,
    project: &str,
    state: SessionState,
) -> Option<Payload> {
    let body = match state {
        SessionState::NeedsInput => "needs you",
        SessionState::Done => "done",
        _ => return None,
    };
    Some(Payload {
        title: format!("{} · {}", clip(name), clip(project)),
        body: body.to_owned(),
        session: session_id.to_string(),
    })
}

/// At most [`NAME_CHARS`] characters, cut on a character boundary, with `…` when cut.
fn clip(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= NAME_CHARS {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(NAME_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// Phones should be told: push is on, somebody subscribed, and the §8 policy allows it now.
/// Cheap checks first, so a suppressed event never uses up the session's 30 s window.
pub fn wanted(
    settings: &Settings,
    subscribers: usize,
    session_id: Ulid,
    state: SessionState,
    throttle: &mut Throttle,
    now: Instant,
) -> bool {
    settings.web.push
        && subscribers > 0
        && decide(session_id, state, &settings.notify, throttle, now).is_some()
}

/// The answer to a subscribe / unsubscribe request.
#[derive(Debug)]
pub enum Reply {
    Done,
    /// `[web] push` is off on this PC.
    Off,
    Bad(String),
}

pub struct Push {
    file: PathBuf,
    settings_file: PathBuf,
    renew: bool,
    /// Opened (and the VAPID key made) only while `[web] push` is on; `None` until then.
    store: Mutex<Option<Store>>,
    sender: Arc<dyn Sender>,
}

impl Push {
    /// `renew` (`mk web --new-token`) forgets the phones; the VAPID key stays. With `[web] push`
    /// off nothing is read or created; the store opens on first use after push is turned on.
    /// An `Err` is for the caller to log: `mk web` serves the page without push.
    pub fn open(
        file: &Path,
        settings_file: PathBuf,
        renew: bool,
        sender: Arc<dyn Sender>,
    ) -> anyhow::Result<Self> {
        let push = Self {
            file: file.to_owned(),
            settings_file,
            renew,
            store: Mutex::new(None),
            sender,
        };
        if push.enabled() {
            *push.lock() = Some(Store::open(file, renew)?);
        }
        Ok(push)
    }

    fn lock(&self) -> MutexGuard<'_, Option<Store>> {
        self.store.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `f` on the store, opening it first when push is on now. `None`: push is off and
    /// the store is not open, or opening failed (logged).
    fn with_store<T>(&self, f: impl FnOnce(&mut Store) -> T) -> Option<T> {
        let mut guard = self.lock();
        if guard.is_none() {
            if !self.enabled() {
                return None;
            }
            match Store::open(&self.file, self.renew) {
                Ok(store) => *guard = Some(store),
                Err(e) => {
                    eprintln!("mk web: push store: {e:#}");
                    return None;
                }
            }
        }
        guard.as_mut().map(f)
    }

    /// `settings.toml` as it is now. `None` when it cannot be read: push then stays off.
    pub fn settings(&self) -> Option<Settings> {
        settings::load(&self.settings_file).ok()
    }

    /// Read on every call: turning `[web] push` on or off needs no restart.
    pub fn enabled(&self) -> bool {
        self.settings().is_some_and(|s| s.web.push)
    }

    pub fn subscribers(&self) -> usize {
        self.lock().as_ref().map_or(0, |s| s.subscriptions().len())
    }

    /// The VAPID public key (base64url) the page subscribes with; `None` while push is off.
    pub fn public_key(&self) -> Option<String> {
        if !self.enabled() {
            return None;
        }
        self.with_store(|s| B64.encode(s.vapid().public_key()))
    }

    pub fn subscribe(&self, body: &[u8]) -> Reply {
        if !self.enabled() {
            return Reply::Off;
        }
        match Subscription::parse(body) {
            Ok(sub) => match self.with_store(|s| s.add(sub)) {
                Some(Ok(())) => Reply::Done,
                Some(Err(e)) => Reply::Bad(format!("{e:#}")),
                None => Reply::Bad("the push key is not available".to_owned()),
            },
            Err(e) => Reply::Bad(format!("{e:#}")),
        }
    }

    /// Always allowed, even with push off: a phone must be able to leave.
    pub fn unsubscribe(&self, body: &[u8]) -> Reply {
        let endpoint = match endpoint_in(body) {
            Ok(endpoint) => endpoint,
            Err(e) => return Reply::Bad(format!("{e:#}")),
        };
        // No store open = nobody to remove (and with push off, no file to create).
        match self.lock().as_mut().map(|s| s.remove(&endpoint)) {
            Some(Err(e)) => Reply::Bad(format!("{e:#}")),
            _ => Reply::Done,
        }
    }

    /// Sends `payload` to every phone at unix time `now`; a phone the push service reports gone
    /// is forgotten. Blocking (up to [`SEND_TIMEOUT`] per phone): run it off the async threads.
    pub fn deliver(&self, payload: &Payload, now: u64) {
        let Ok(text) = serde_json::to_vec(payload) else {
            return;
        };
        let Some(phones) = self.with_store(|s| s.subscriptions().to_vec()) else {
            return;
        };
        let mut gone = Vec::new();
        for phone in &phones {
            let host = crypto::origin(&phone.endpoint).unwrap_or("?");
            match self.post(phone, &text, now) {
                Ok(Sent::Delivered) => {}
                Ok(Sent::Gone) => gone.push(phone.endpoint.clone()),
                Ok(Sent::Failed(why)) => eprintln!("mk web: push to {host}: {why}"),
                Err(e) => eprintln!("mk web: push to {host}: {e:#}"),
            }
        }
        for endpoint in gone {
            if let Some(Err(e)) = self.with_store(|s| s.remove(&endpoint)) {
                eprintln!("mk web: push: {e:#}");
            }
        }
    }

    fn post(&self, phone: &Subscription, text: &[u8], now: u64) -> anyhow::Result<Sent> {
        // The file may be old or hand-edited: check again right before the request.
        anyhow::ensure!(
            endpoint_allowed(&phone.endpoint),
            "that push service is not one Mkrew sends to"
        );
        let (p256dh, auth) = phone.keys()?;
        let body = crypto::encrypt(&p256dh, &auth, text)?;
        let authorization = self
            .with_store(|s| s.vapid().authorization(&phone.endpoint, now))
            .context("the push key is not available")??;
        Ok(self.sender.send(&Post {
            url: phone.endpoint.clone(),
            headers: vec![
                ("Authorization", authorization),
                ("Content-Encoding", "aes128gcm".to_owned()),
                ("TTL", TTL_SECONDS.to_owned()),
                ("Urgency", "high".to_owned()),
            ],
            body,
        }))
    }

    /// [`Push::deliver`] on a blocking thread.
    pub async fn notify(self: Arc<Self>, payload: Payload) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Err(e) = tokio::task::spawn_blocking(move || self.deliver(&payload, now)).await {
            eprintln!("mk web: push: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use mkrew_core::settings::WebSettings;

    use super::*;

    const SUB_JSON: &str = r#"{"endpoint":"https://fcm.googleapis.com/fcm/send/abc","expirationTime":null,"keys":{"p256dh":"BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4","auth":"BTBZMqHH6r4Tts7J_aSIgg"}}"#;
    const OTHER_JSON: &str = r#"{"endpoint":"https://web.push.apple.com/zzz","keys":{"p256dh":"BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4","auth":"BTBZMqHH6r4Tts7J_aSIgg"}}"#;

    /// Records every post; answers per endpoint (default: delivered).
    struct Fake {
        posts: Mutex<Vec<Post>>,
        answers: HashMap<String, Sent>,
    }

    impl Sender for Fake {
        fn send(&self, post: &Post) -> Sent {
            self.posts.lock().unwrap().push(post.clone());
            self.answers
                .get(&post.url)
                .cloned()
                .unwrap_or(Sent::Delivered)
        }
    }

    fn push_with(on: bool, answers: &[(&str, Sent)]) -> (tempfile::TempDir, Push, Arc<Fake>) {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.toml");
        std::fs::write(&settings, format!("[web]\npush = {on}\n")).unwrap();
        let fake = Arc::new(Fake {
            posts: Mutex::new(Vec::new()),
            answers: answers
                .iter()
                .map(|(u, s)| ((*u).to_owned(), s.clone()))
                .collect(),
        });
        let sender: Arc<dyn Sender> = fake.clone();
        let push = Push::open(&dir.path().join("web_push.json"), settings, false, sender).unwrap();
        (dir, push, fake)
    }

    fn note() -> Payload {
        Payload {
            title: "omp · mkrew".into(),
            body: "needs you".into(),
            session: "01J00000000000000000000000".into(),
        }
    }

    fn settings(push: bool) -> Settings {
        Settings {
            web: WebSettings {
                enabled: true,
                push,
            },
            ..Settings::default()
        }
    }

    #[test]
    fn wanted_follows_the_switches_the_phones_and_the_30_second_limit() {
        let t0 = Instant::now();
        let (a, b) = (Ulid::new(), Ulid::new());
        let mut throttle = Throttle::default();
        // Push off, or nobody subscribed: nothing, and the window is not used up.
        assert!(!wanted(
            &settings(false),
            1,
            a,
            SessionState::NeedsInput,
            &mut throttle,
            t0
        ));
        assert!(!wanted(
            &settings(true),
            0,
            a,
            SessionState::NeedsInput,
            &mut throttle,
            t0
        ));
        assert!(wanted(
            &settings(true),
            1,
            a,
            SessionState::NeedsInput,
            &mut throttle,
            t0
        ));
        // The same session again within 30 s: no (SPEC §8); another session: yes.
        assert!(!wanted(
            &settings(true),
            1,
            a,
            SessionState::Done,
            &mut throttle,
            t0 + Duration::from_secs(5)
        ));
        assert!(wanted(
            &settings(true),
            1,
            b,
            SessionState::Done,
            &mut throttle,
            t0 + Duration::from_secs(5)
        ));
        assert!(wanted(
            &settings(true),
            1,
            a,
            SessionState::Done,
            &mut throttle,
            t0 + Duration::from_secs(31)
        ));
        // Only needs_input and done.
        for state in [
            SessionState::Working,
            SessionState::Idle,
            SessionState::Exited,
            SessionState::Starting,
        ] {
            assert!(
                !wanted(&settings(true), 1, Ulid::new(), state, &mut throttle, t0),
                "{state:?}"
            );
        }
    }

    #[test]
    fn the_notify_switches_apply_to_phones_too() {
        let t0 = Instant::now();
        let mut throttle = Throttle::default();
        let mut quiet = settings(true);
        quiet.notify.needs_input = false;
        assert!(!wanted(
            &quiet,
            1,
            Ulid::new(),
            SessionState::NeedsInput,
            &mut throttle,
            t0
        ));
        assert!(wanted(
            &quiet,
            1,
            Ulid::new(),
            SessionState::Done,
            &mut throttle,
            t0
        ));
        quiet.notify.done = false;
        assert!(!wanted(
            &quiet,
            1,
            Ulid::new(),
            SessionState::Done,
            &mut throttle,
            t0
        ));
    }

    #[test]
    fn the_payload_names_the_session_and_clips_long_text() {
        let id = Ulid::new();
        let needs = payload(id, "omp", "mkrew", SessionState::NeedsInput).unwrap();
        assert_eq!(
            (
                needs.title.as_str(),
                needs.body.as_str(),
                needs.session.as_str()
            ),
            ("omp · mkrew", "needs you", id.to_string().as_str())
        );
        assert_eq!(
            payload(id, "omp", "mkrew", SessionState::Done)
                .unwrap()
                .body,
            "done"
        );
        assert_eq!(payload(id, "omp", "mkrew", SessionState::Working), None);
        let long = "é".repeat(500);
        let clipped = payload(id, &long, &long, SessionState::Done).unwrap();
        assert!(
            clipped.title.chars().count() <= 60 + 3 + 60,
            "{}",
            clipped.title
        );
        assert!(clipped.title.contains('…'));
        assert!(serde_json::to_vec(&clipped).unwrap().len() < crypto::MAX_PLAINTEXT);
    }

    #[test]
    fn the_key_and_subscriptions_exist_only_while_push_is_on() {
        let (_dir, off, _) = push_with(false, &[]);
        assert_eq!(off.public_key(), None);
        assert!(matches!(off.subscribe(SUB_JSON.as_bytes()), Reply::Off));
        assert_eq!(off.subscribers(), 0);

        let (_dir, on, _) = push_with(true, &[]);
        assert_eq!(on.public_key().unwrap().len(), 87, "65 bytes as base64url");
        assert!(matches!(on.subscribe(SUB_JSON.as_bytes()), Reply::Done));
        assert!(matches!(on.subscribe(b"nope"), Reply::Bad(_)));
        let evil = SUB_JSON.replace("fcm.googleapis.com", "evil.test");
        let Reply::Bad(why) = on.subscribe(evil.as_bytes()) else {
            panic!("accepted")
        };
        assert!(why.contains("not one Mkrew sends to"), "{why}");
        assert_eq!(on.subscribers(), 1);
        assert!(matches!(
            on.unsubscribe(br#"{"endpoint":"https://fcm.googleapis.com/fcm/send/abc"}"#),
            Reply::Done
        ));
        assert_eq!(on.subscribers(), 0);
    }

    #[test]
    fn push_off_creates_no_key_or_file_and_turning_it_on_needs_no_restart() {
        let (dir, push, _) = push_with(false, &[]);
        let file = dir.path().join("web_push.json");
        assert!(
            !file.exists(),
            "open with push off must not create the file"
        );
        assert!(matches!(
            push.unsubscribe(br#"{"endpoint":"https://fcm.googleapis.com/x"}"#),
            Reply::Done
        ));
        assert!(!file.exists());
        std::fs::write(dir.path().join("settings.toml"), "[web]\npush = true\n").unwrap();
        assert_eq!(push.public_key().unwrap().len(), 87);
        assert!(file.exists());
        assert!(matches!(push.subscribe(SUB_JSON.as_bytes()), Reply::Done));
    }

    #[test]
    fn a_store_failure_is_an_error_from_open_not_a_panic() {
        let dir = tempfile::tempdir().unwrap();
        let settings = dir.path().join("settings.toml");
        std::fs::write(&settings, "[web]\npush = true\n").unwrap();
        // The file's parent is a regular file, so the key cannot be saved.
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, "x").unwrap();
        let sender: Arc<dyn Sender> = Arc::new(UreqSender);
        assert!(Push::open(&blocker.join("web_push.json"), settings, false, sender).is_err());
    }

    #[test]
    fn a_broken_settings_file_means_push_is_off() {
        let (dir, push, _) = push_with(true, &[]);
        assert!(push.enabled());
        std::fs::write(dir.path().join("settings.toml"), "[web\npush = true").unwrap();
        assert!(!push.enabled());
        assert_eq!(push.public_key(), None);
        std::fs::remove_file(dir.path().join("settings.toml")).unwrap();
        assert!(!push.enabled(), "no file = the defaults = off");
    }

    #[test]
    fn deliver_sends_one_encrypted_vapid_post_per_phone() {
        let (_dir, push, fake) = push_with(true, &[]);
        push.subscribe(SUB_JSON.as_bytes());
        push.subscribe(OTHER_JSON.as_bytes());
        push.deliver(&note(), 1_700_000_000);
        let posts = fake.posts.lock().unwrap();
        assert_eq!(posts.len(), 2);
        let header = |post: &Post, name: &str| {
            post.headers
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| v.clone())
        };
        for post in posts.iter() {
            assert!(
                header(post, "Authorization")
                    .unwrap()
                    .starts_with("vapid t=")
            );
            assert_eq!(
                header(post, "Content-Encoding").as_deref(),
                Some("aes128gcm")
            );
            assert_eq!(header(post, "TTL").as_deref(), Some("3600"));
            assert_eq!(header(post, "Urgency").as_deref(), Some("high"));
            let text = serde_json::to_vec(&note()).unwrap();
            assert_eq!(post.body.len(), 86 + text.len() + 1 + 16);
            assert!(
                !String::from_utf8_lossy(&post.body).contains("needs you"),
                "ciphertext only"
            );
        }
        let urls: Vec<&str> = posts.iter().map(|p| p.url.as_str()).collect();
        assert_eq!(
            urls,
            [
                "https://fcm.googleapis.com/fcm/send/abc",
                "https://web.push.apple.com/zzz"
            ]
        );
    }

    #[test]
    fn a_disallowed_endpoint_is_never_posted_to() {
        let (_dir, push, fake) = push_with(true, &[]);
        let mut phone = Subscription::parse(SUB_JSON.as_bytes()).unwrap();
        phone.endpoint = "https://evil.test/x".into();
        assert!(push.post(&phone, b"{}", 1_700_000_000).is_err());
        assert!(fake.posts.lock().unwrap().is_empty());
    }

    #[test]
    fn a_gone_phone_is_dropped_a_failing_one_stays_and_the_others_still_get_it() {
        let gone = "https://fcm.googleapis.com/fcm/send/abc";
        let flaky = "https://web.push.apple.com/zzz";
        let (dir, push, fake) = push_with(
            true,
            &[(gone, Sent::Gone), (flaky, Sent::Failed("HTTP 503".into()))],
        );
        push.subscribe(SUB_JSON.as_bytes());
        push.subscribe(OTHER_JSON.as_bytes());
        push.deliver(&note(), 1_700_000_000);
        assert_eq!(
            fake.posts.lock().unwrap().len(),
            2,
            "the first failure does not stop the second"
        );
        assert_eq!(push.subscribers(), 1, "404/410 removes the phone");
        let saved = std::fs::read_to_string(dir.path().join("web_push.json")).unwrap();
        assert!(!saved.contains(gone) && saved.contains(flaky), "{saved}");
    }

    #[test]
    fn the_file_sender_appends_one_json_line_per_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sink.jsonl");
        let sender = FileSender(path.clone());
        let post = Post {
            url: "https://web.push.apple.com/x".into(),
            headers: vec![("TTL", "3600".into())],
            body: vec![0; 5],
        };
        assert!(matches!(sender.send(&post), Sent::Delivered));
        assert!(matches!(sender.send(&post), Sent::Delivered));
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<serde_json::Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["url"], "https://web.push.apple.com/x");
        assert_eq!(lines[0]["body_len"], 5);
        assert_eq!(lines[0]["headers"][0][0], "TTL");
    }
}
