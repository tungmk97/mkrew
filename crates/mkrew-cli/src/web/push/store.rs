//! What `mk web` push remembers (ADR 0066): the VAPID key and the phones that opted in, in one
//! private file `<config>/web_push.json`. A subscription URL is a capability to push to that
//! phone, so it is kept like `web_token` and never printed.

use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use serde::{Deserialize, Serialize};

use super::crypto::{self, Vapid};
use crate::web::token::write_private;

/// Phones one PC keeps. More than that is a mistake, or someone with the token filling the file.
pub const MAX_SUBSCRIPTIONS: usize = 8;

const HOSTS: [&str; 2] = ["fcm.googleapis.com", "updates.push.services.mozilla.com"];
const HOST_SUFFIXES: [&str; 2] = [".push.apple.com", ".notify.windows.com"];

/// Only the push services of Apple, Google, Mozilla and Microsoft get a request. Whoever holds
/// the page's token can send a subscription; without this check `mk web` would POST to any URL
/// they name from the user's PC. `https`, no user, no port, plain lowercase host.
pub fn endpoint_allowed(endpoint: &str) -> bool {
    let Some(rest) = endpoint.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    host.bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-')
        && (HOSTS.contains(&host)
            || HOST_SUFFIXES
                .iter()
                .any(|suffix| host.len() > suffix.len() && host.ends_with(suffix)))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    pub endpoint: String,
    /// The phone's P-256 public key, base64url (65 bytes decoded).
    pub p256dh: String,
    /// The phone's auth secret, base64url (16 bytes decoded).
    pub auth: String,
}

impl Subscription {
    /// From the browser's `PushSubscription.toJSON()`:
    /// `{"endpoint", "expirationTime", "keys": {"p256dh", "auth"}}`.
    pub fn parse(body: &[u8]) -> anyhow::Result<Self> {
        #[derive(Deserialize)]
        struct Keys {
            p256dh: String,
            auth: String,
        }
        #[derive(Deserialize)]
        struct Wire {
            endpoint: String,
            keys: Keys,
        }
        let wire: Wire = serde_json::from_slice(body)
            .context("the subscription is not the JSON a browser makes")?;
        Ok(Self {
            endpoint: wire.endpoint,
            p256dh: wire.keys.p256dh,
            auth: wire.keys.auth,
        })
    }

    /// The decoded phone key and auth secret.
    pub fn keys(&self) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
        Ok((
            B64.decode(&self.p256dh)
                .context("p256dh is not base64url")?,
            B64.decode(&self.auth).context("auth is not base64url")?,
        ))
    }

    fn check(&self) -> anyhow::Result<()> {
        if !endpoint_allowed(&self.endpoint) {
            bail!("that push service is not one Mkrew sends to");
        }
        let (p256dh, auth) = self.keys()?;
        if p256dh.len() != 65 || auth.len() != 16 {
            bail!("the subscription keys have the wrong size");
        }
        // A key that is not a P-256 point fails here, not on the first notification.
        crypto::encrypt(&p256dh, &auth, b"{}").map(drop)
    }
}

/// The `endpoint` of an unsubscribe request, `{"endpoint": "…"}`.
pub fn endpoint_in(body: &[u8]) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct Wire {
        endpoint: String,
    }
    Ok(serde_json::from_slice::<Wire>(body)
        .context("the request is not JSON with an endpoint")?
        .endpoint)
}

#[derive(Serialize, Deserialize)]
struct Saved {
    /// The VAPID key, PKCS#8 as base64url.
    vapid: String,
    #[serde(default)]
    subscriptions: Vec<Subscription>,
}

pub struct Store {
    file: PathBuf,
    vapid: Vapid,
    subs: Vec<Subscription>,
}

impl Store {
    /// Loads `file`. A missing file, or one that is not a valid store, makes a new VAPID key
    /// (phones must then tap the bell again; a bad file gets one stderr line). A file that
    /// cannot be read (sharing violation, not UTF-8) is an error and is left alone, so the
    /// phones in it are not lost. `drop_subscriptions` (`mk web --new-token`)
    /// forgets the phones and keeps the key. Entries whose endpoint is not allowed (a
    /// hand-edited or old file) are dropped on load.
    pub fn open(file: &Path, drop_subscriptions: bool) -> anyhow::Result<Self> {
        let (mut store, mut changed) = match std::fs::read_to_string(file) {
            Ok(text) => match Self::parse(file, &text) {
                Ok(store) => store,
                Err(e) => {
                    eprintln!(
                        "mk web: {}: {e:#}; making a new push key (phones must tap the bell again)",
                        file.display()
                    );
                    (Self::fresh(file)?, true)
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::fresh(file)?, true),
            Err(e) => return Err(e).with_context(|| format!("read {}", file.display())),
        };
        if drop_subscriptions && !store.subs.is_empty() {
            store.subs.clear();
            changed = true;
        }
        if changed {
            store.save()?;
        }
        Ok(store)
    }

    fn fresh(file: &Path) -> anyhow::Result<Self> {
        Ok(Self {
            file: file.to_owned(),
            vapid: Vapid::generate()?,
            subs: Vec::new(),
        })
    }

    /// The store and whether the file held entries that were dropped (so it must be rewritten).
    fn parse(file: &Path, text: &str) -> anyhow::Result<(Self, bool)> {
        let saved: Saved = serde_json::from_str(text)?;
        let pkcs8 = B64.decode(&saved.vapid)?;
        let total = saved.subscriptions.len();
        let mut subs = saved.subscriptions;
        subs.retain(|s| endpoint_allowed(&s.endpoint));
        subs.truncate(MAX_SUBSCRIPTIONS);
        let dropped = subs.len() != total;
        Ok((
            Self {
                file: file.to_owned(),
                vapid: Vapid::from_pkcs8(&pkcs8)?,
                subs,
            },
            dropped,
        ))
    }

    pub fn vapid(&self) -> &Vapid {
        &self.vapid
    }

    pub fn subscriptions(&self) -> &[Subscription] {
        &self.subs
    }

    /// Adds a phone, or refreshes the keys of a known one (same endpoint).
    pub fn add(&mut self, sub: Subscription) -> anyhow::Result<()> {
        sub.check()?;
        if let Some(known) = self.subs.iter_mut().find(|s| s.endpoint == sub.endpoint) {
            *known = sub;
        } else if self.subs.len() >= MAX_SUBSCRIPTIONS {
            bail!("this PC already notifies {MAX_SUBSCRIPTIONS} phones");
        } else {
            self.subs.push(sub);
        }
        self.save()
    }

    pub fn remove(&mut self, endpoint: &str) -> anyhow::Result<()> {
        let before = self.subs.len();
        self.subs.retain(|s| s.endpoint != endpoint);
        if self.subs.len() == before {
            return Ok(());
        }
        self.save()
    }

    fn save(&self) -> anyhow::Result<()> {
        let saved = Saved {
            vapid: B64.encode(self.vapid.pkcs8()),
            subscriptions: self.subs.clone(),
        };
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        }
        write_private(&self.file, &serde_json::to_string_pretty(&saved)?)
            .with_context(|| format!("write {}", self.file.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The RFC 8291 receiver: a valid P-256 point and a 16-byte auth secret.
    const P256DH: &str =
        "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
    const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";

    fn sub(endpoint: &str) -> Subscription {
        Subscription {
            endpoint: endpoint.into(),
            p256dh: P256DH.into(),
            auth: AUTH.into(),
        }
    }

    fn file(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("config").join("web_push.json")
    }

    #[test]
    fn only_the_four_push_services_are_allowed() {
        for ok in [
            "https://fcm.googleapis.com/fcm/send/abc",
            "https://updates.push.services.mozilla.com/wpush/v2/abc",
            "https://web.push.apple.com/QZ12",
            "https://api.push.apple.com/3/device/x",
            "https://wns2-par02p.notify.windows.com/w/?token=abc",
        ] {
            assert!(endpoint_allowed(ok), "{ok}");
        }
        for bad in [
            "http://fcm.googleapis.com/fcm/send/abc",
            "https://fcm.googleapis.com.evil.test/x",
            "https://evil.test/fcm.googleapis.com",
            "https://evil.test/?h=web.push.apple.com",
            "https://web.push.apple.com.evil.test/x",
            "https://user@web.push.apple.com/x",
            "https://web.push.apple.com:8443/x",
            "https://.push.apple.com/x",
            "https://WEB.PUSH.APPLE.COM/x",
            "https://localhost/x",
            "https://127.0.0.1/x",
            "https://192.168.1.5/x",
            "https:///x",
            "ftp://web.push.apple.com/x",
            "",
        ] {
            assert!(!endpoint_allowed(bad), "{bad}");
        }
    }

    #[test]
    fn a_browser_subscription_parses() {
        let body = format!(
            r#"{{"endpoint":"https://fcm.googleapis.com/fcm/send/abc","expirationTime":null,"keys":{{"p256dh":"{P256DH}","auth":"{AUTH}"}}}}"#
        );
        let parsed = Subscription::parse(body.as_bytes()).unwrap();
        assert_eq!(parsed, sub("https://fcm.googleapis.com/fcm/send/abc"));
        let (p256dh, auth) = parsed.keys().unwrap();
        assert_eq!((p256dh.len(), auth.len()), (65, 16));
        assert!(Subscription::parse(b"nope").is_err());
        assert!(
            Subscription::parse(br#"{"endpoint":"x"}"#).is_err(),
            "keys missing"
        );
        assert_eq!(
            endpoint_in(br#"{"endpoint":"https://x"}"#).unwrap(),
            "https://x"
        );
        assert!(endpoint_in(b"{}").is_err());
    }

    #[test]
    fn add_refuses_other_hosts_bad_keys_and_too_many_phones() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&file(&dir), false).unwrap();
        assert!(store.add(sub("https://evil.test/x")).is_err(), "host");
        let mut short = sub("https://fcm.googleapis.com/fcm/send/a");
        short.auth = "AAAA".into();
        assert!(store.add(short).is_err(), "auth secret size");
        let mut off_curve = sub("https://fcm.googleapis.com/fcm/send/a");
        off_curve.p256dh = B64.encode([4u8; 65]);
        assert!(store.add(off_curve).is_err(), "not a P-256 point");
        assert!(store.subscriptions().is_empty());
        for i in 0..MAX_SUBSCRIPTIONS {
            store
                .add(sub(&format!("https://fcm.googleapis.com/fcm/send/{i}")))
                .unwrap();
        }
        assert!(
            store
                .add(sub("https://fcm.googleapis.com/fcm/send/extra"))
                .is_err()
        );
        // A phone that is already known may refresh its keys when the list is full.
        store
            .add(sub("https://fcm.googleapis.com/fcm/send/0"))
            .unwrap();
        assert_eq!(store.subscriptions().len(), MAX_SUBSCRIPTIONS);
    }

    #[test]
    fn the_same_phone_twice_is_one_entry_and_remove_forgets_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&file(&dir), false).unwrap();
        let endpoint = "https://web.push.apple.com/abc";
        store.add(sub(endpoint)).unwrap();
        store.add(sub(endpoint)).unwrap();
        assert_eq!(store.subscriptions().len(), 1);
        store.remove("https://web.push.apple.com/other").unwrap();
        assert_eq!(store.subscriptions().len(), 1);
        store.remove(endpoint).unwrap();
        assert!(store.subscriptions().is_empty());
    }

    #[test]
    fn phones_and_key_survive_a_restart_and_new_token_drops_only_the_phones() {
        let dir = tempfile::tempdir().unwrap();
        let path = file(&dir);
        let mut store = Store::open(&path, false).unwrap();
        store.add(sub("https://web.push.apple.com/abc")).unwrap();
        let key = store.vapid().public_key().to_vec();

        let again = Store::open(&path, false).unwrap();
        assert_eq!(again.vapid().public_key(), key);
        assert_eq!(
            again.subscriptions(),
            [sub("https://web.push.apple.com/abc")]
        );

        let renewed = Store::open(&path, true).unwrap();
        assert!(renewed.subscriptions().is_empty());
        assert_eq!(
            renewed.vapid().public_key(),
            key,
            "phones keep working with the same key"
        );
        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains("web.push.apple.com"), "{on_disk}");
    }

    #[test]
    fn a_file_that_cannot_be_read_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = file(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, [0xff, 0xfe, 0x00]).unwrap(); // not UTF-8: read_to_string fails
        assert!(Store::open(&path, false).is_err());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            [0xff, 0xfe, 0x00],
            "left untouched"
        );
    }

    #[test]
    fn a_corrupt_file_makes_a_fresh_key_instead_of_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let path = file(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        for garbage in ["", "not json", r#"{"vapid":"AAAA","subscriptions":[]}"#] {
            std::fs::write(&path, garbage).unwrap();
            let store = Store::open(&path, false).unwrap();
            assert!(store.subscriptions().is_empty(), "{garbage:?}");
            assert_eq!(store.vapid().public_key().len(), 65);
            // And the repaired file loads.
            assert_eq!(
                Store::open(&path, false).unwrap().vapid().public_key(),
                store.vapid().public_key()
            );
        }
    }

    #[test]
    fn entries_with_a_disallowed_endpoint_are_dropped_on_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = file(&dir);
        let mut store = Store::open(&path, false).unwrap();
        store.add(sub("https://web.push.apple.com/abc")).unwrap();
        let text = std::fs::read_to_string(&path)
            .unwrap()
            .replace("web.push.apple.com/abc", "evil.test/abc");
        std::fs::write(&path, text).unwrap();
        let again = Store::open(&path, false).unwrap();
        assert!(again.subscriptions().is_empty());
        assert!(
            !std::fs::read_to_string(&path)
                .unwrap()
                .contains("evil.test")
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_owner_only_even_when_it_existed_and_leaves_no_temp() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = file(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let mut store = Store::open(&path, false).unwrap();
        store.add(sub("https://web.push.apple.com/abc")).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }
}
