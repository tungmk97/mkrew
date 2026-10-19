//! The "Phone access" dialog's model (ADR 0053): what the daemon and Tailscale last said, the
//! rows and the link they make, and the poll bookkeeping. Pure and tested headlessly;
//! `shell/phone_access.rs` renders it and runs the blocking calls off the UI thread.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mkrew_core::tailscale::{self, ServeCheck};
use mkrew_protocol::WebStatus;

use crate::{phone, theme};

/// A click on Start / Stop waits at most this long for the daemon to re-read `settings.toml`
/// (it debounces changes); then the row shows what the daemon really reports.
const PENDING_TIMEOUT: Duration = Duration::from_secs(5);

/// What `daemon.status` last said about `mk web`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Daemon {
    Loading,
    Known(WebStatus),
    Failed(String),
}

/// What Tailscale last said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Serve {
    Checking,
    /// `tailscale serve` already proxies Mkrew's port.
    Ready,
    Missing,
    /// Not installed, not running or logged out; the message, if any, is the CLI's.
    Unavailable(Option<String>),
}

/// The Set up button's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setup {
    Idle,
    Running,
    Failed(String),
}

/// `serve_status()` plus the MagicDNS name: a logged-out Tailscale answers "not served", but
/// without a name there is nothing to set up or to link to.
pub fn serve_state(check: Result<ServeCheck, String>, host: Option<&str>) -> Serve {
    match check {
        Err(message) => Serve::Unavailable(Some(message)),
        Ok(_) if host.is_none() => Serve::Unavailable(None),
        Ok(ServeCheck::Ready) => Serve::Ready,
        Ok(ServeCheck::Missing) => Serve::Missing,
    }
}

/// The glyph before a row's text (status is never color alone, SPEC §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    On,
    Off,
    Done,
    Down,
    Wait,
}

impl Mark {
    pub fn glyph(self) -> &'static str {
        match self {
            Mark::On => "●",
            Mark::Off | Mark::Wait => "○",
            Mark::Done => "✓",
            Mark::Down => "✕",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Start,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRow {
    pub mark: Mark,
    pub text: String,
    pub switch: Option<Switch>,
}

/// The Status row. `pending` is a Start (`true`) / Stop (`false`) the daemon has not reported
/// yet.
pub fn status_row(daemon: &Daemon, pending: Option<bool>) -> StatusRow {
    let row = |mark, text: &str, switch| StatusRow {
        mark,
        text: text.to_owned(),
        switch,
    };
    match daemon {
        Daemon::Loading => row(Mark::Wait, "Checking…", None),
        Daemon::Failed(error) => StatusRow {
            mark: Mark::Down,
            text: format!("Cannot read the daemon status: {error}"),
            switch: None,
        },
        Daemon::Known(web) => match pending {
            Some(true) => row(Mark::Wait, "Starting…", None),
            Some(false) => row(Mark::Wait, "Stopping…", None),
            None if !web.enabled => row(Mark::Off, "Off", Some(Switch::Start)),
            None if web.running => row(Mark::On, "On", Some(Switch::Stop)),
            None => match &web.error {
                Some(error) => StatusRow {
                    mark: Mark::Down,
                    text: format!("Not running: {}. Retrying…", error.trim_end_matches('.')),
                    switch: Some(Switch::Stop),
                },
                None => row(Mark::Wait, "Starting…", Some(Switch::Stop)),
            },
        },
    }
}

/// The Set up button: pressable, or showing that `tailscale serve` is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetUp {
    Ready,
    Busy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServeRow {
    pub mark: Mark,
    pub text: &'static str,
    pub button: Option<SetUp>,
    /// Shown under the row: the Set up failure, or why Tailscale is unavailable.
    pub error: Option<String>,
}

pub fn serve_row(serve: &Serve, setup: &Setup) -> ServeRow {
    let row = |mark, text, button, error| ServeRow {
        mark,
        text,
        button,
        error,
    };
    match serve {
        Serve::Checking => row(Mark::Wait, "Checking Tailscale…", None, None),
        Serve::Ready => row(Mark::Done, "Tailscale serve is set up", None, None),
        Serve::Missing => row(
            Mark::Off,
            "Tailscale doesn't publish Mkrew yet",
            Some(if *setup == Setup::Running {
                SetUp::Busy
            } else {
                SetUp::Ready
            }),
            match setup {
                Setup::Failed(error) => Some(error.clone()),
                _ => None,
            },
        ),
        Serve::Unavailable(message) => row(
            Mark::Down,
            "Install and sign in to Tailscale first",
            None,
            message.clone(),
        ),
    }
}

/// The notifications switch of the dialog (ADR 0066).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushSwitch {
    TurnOn,
    TurnOff,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushRow {
    pub mark: Mark,
    pub text: &'static str,
    pub switch: PushSwitch,
}

/// The Phone notifications row. It shows while phone access is on: the phone page is what asks
/// for notifications, so the switch means nothing before.
pub fn push_row(daemon: &Daemon, push: bool) -> Option<PushRow> {
    let Daemon::Known(web) = daemon else {
        return None;
    };
    if !web.enabled {
        return None;
    }
    Some(if push {
        PushRow {
            mark: Mark::On,
            text: "Phone notifications: on",
            switch: PushSwitch::TurnOff,
        }
    } else {
        PushRow {
            mark: Mark::Off,
            text: "Phone notifications: off",
            switch: PushSwitch::TurnOn,
        }
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkBlock {
    Hidden,
    /// On, name known, `mk web` has not written `web_token` yet (first start).
    Starting,
    Ready,
}

/// The phone link and its QR code.
#[derive(Clone)]
pub struct Link {
    pub text: String,
    /// `None` when the link is too long for any QR code.
    pub image: Option<Arc<gpui::Image>>,
}

/// What the dialog reads from the machine: blocking (two `tailscale` runs and a file), so call
/// it off the UI thread.
pub struct Local {
    pub serve: Serve,
    pub host: Option<String>,
    pub token: Option<String>,
    /// `[web] push` in settings.toml (ADR 0066).
    pub push: bool,
}

impl Local {
    pub fn read(config_dir: &Path, settings_file: &Path) -> Local {
        let check = tailscale::serve_status();
        // No second process when the first one already failed.
        let host = if check.is_ok() {
            tailscale::dns_name()
        } else {
            None
        };
        Local {
            serve: serve_state(check, host.as_deref()),
            host,
            token: phone::read_token(config_dir),
            push: mkrew_core::settings::load(settings_file).is_ok_and(|settings| settings.web.push),
        }
    }
}

/// Phone access as the UI knows it: the title bar icon and the dialog read this.
pub struct Phone {
    pub daemon: Daemon,
    pub serve: Serve,
    pub setup: Setup,
    /// A failed Start / Stop (`settings.toml` could not be written), shown in the dialog.
    pub notice: Option<String>,
    /// "Copy link" was just pressed (the label says so until the next poll).
    pub copied: bool,
    pending: Option<(bool, Instant)>,
    polling: bool,
    last_poll: Option<Instant>,
    host: Option<String>,
    token: Option<String>,
    /// `[web] push` as last read; the notifications row shows it.
    pub push: bool,
    link: Option<Link>,
}

impl Default for Phone {
    fn default() -> Self {
        Phone {
            daemon: Daemon::Loading,
            serve: Serve::Checking,
            setup: Setup::Idle,
            notice: None,
            copied: false,
            pending: None,
            polling: false,
            last_poll: None,
            host: None,
            token: None,
            push: false,
            link: None,
        }
    }
}

impl Phone {
    /// `mk web` runs: the title bar icon turns accent.
    pub fn running(&self) -> bool {
        matches!(&self.daemon, Daemon::Known(web) if web.running)
    }

    pub fn pending(&self) -> Option<bool> {
        self.pending.map(|(wanted, _)| wanted)
    }

    pub fn link(&self) -> Option<&Link> {
        self.link.as_ref()
    }

    pub fn link_block(&self) -> LinkBlock {
        let on = matches!(&self.daemon, Daemon::Known(web) if web.enabled && web.running);
        match (on, &self.host, &self.token) {
            (true, Some(_), Some(_)) => LinkBlock::Ready,
            (true, Some(_), None) => LinkBlock::Starting,
            _ => LinkBlock::Hidden,
        }
    }

    /// The dialog opens: Tailscale is asked again, an earlier Set up failure is forgotten.
    pub fn opened(&mut self) {
        self.serve = Serve::Checking;
        self.setup = Setup::Idle;
        self.notice = None;
        self.copied = false;
    }

    /// The loop's tick: the dialog polls on every tick, a closed one every
    /// [`theme::PHONE_POLL_IDLE`].
    pub fn due(&self, now: Instant, open: bool) -> bool {
        open || self
            .last_poll
            .is_none_or(|last| now.duration_since(last) >= theme::PHONE_POLL_IDLE)
    }

    /// `false` while the previous poll has not answered (answers must not arrive out of order).
    pub fn begin_poll(&mut self, now: Instant) -> bool {
        if self.polling {
            return false;
        }
        self.polling = true;
        self.last_poll = Some(now);
        true
    }

    /// A poll answered: `web` from `daemon.status`, `local` only when the dialog is open.
    /// `colors` = `#rrggbb` of `qr_dark` / `qr_light`.
    pub fn polled(
        &mut self,
        web: Result<WebStatus, String>,
        local: Option<Local>,
        now: Instant,
        colors: (&str, &str),
    ) {
        self.polling = false;
        self.copied = false;
        match web {
            Ok(web) => {
                if let Some((wanted, since)) = self.pending
                    && (web.enabled == wanted || now.duration_since(since) >= PENDING_TIMEOUT)
                {
                    self.pending = None;
                }
                self.daemon = Daemon::Known(web);
            }
            Err(error) => self.daemon = Daemon::Failed(error),
        }
        let Some(local) = local else {
            return;
        };
        self.serve = local.serve;
        self.host = local.host;
        self.token = local.token;
        self.push = local.push;
        self.link = match (&self.host, &self.token) {
            (Some(host), Some(token)) => {
                let text = phone::link(host, token);
                // Every poll lands here: render the QR again only for another link.
                if self.link.as_ref().is_some_and(|link| link.text == text) {
                    self.link.take()
                } else {
                    let image = phone::qr_svg(&text, colors.0, colors.1).map(|svg| {
                        Arc::new(gpui::Image::from_bytes(
                            gpui::ImageFormat::Svg,
                            svg.into_bytes(),
                        ))
                    });
                    Some(Link { text, image })
                }
            }
            _ => None,
        };
    }

    /// Start / Stop was written to `settings.toml`; the row waits for the daemon to agree.
    pub fn written(&mut self, enabled: bool, now: Instant) {
        self.pending = Some((enabled, now));
        self.notice = None;
    }

    pub fn write_failed(&mut self, error: String) {
        self.notice = Some(format!("Cannot write settings.toml: {error}"));
    }

    /// `true` when a Set up should start now (Tailscale lacks the serve entry, none runs).
    pub fn begin_setup(&mut self) -> bool {
        if self.serve != Serve::Missing || self.setup == Setup::Running {
            return false;
        }
        self.setup = Setup::Running;
        true
    }

    pub fn setup_done(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.setup = Setup::Idle;
                // The command succeeded; the next poll confirms.
                self.serve = Serve::Ready;
            }
            Err(error) => self.setup = Setup::Failed(error),
        }
    }

    /// The text for the clipboard; `None` without a link.
    pub fn copy_link(&mut self) -> Option<String> {
        let text = self.link.as_ref()?.text.clone();
        self.copied = true;
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COLORS: (&str, &str) = ("#101114", "#FFFFFF");

    fn web(enabled: bool, running: bool, error: Option<&str>) -> WebStatus {
        WebStatus {
            enabled,
            running,
            error: error.map(str::to_owned),
        }
    }

    fn known(enabled: bool, running: bool, error: Option<&str>) -> Daemon {
        Daemon::Known(web(enabled, running, error))
    }

    fn token(pair: &str) -> String {
        pair.repeat(32)
    }

    fn local(host: Option<&str>, token: Option<String>) -> Local {
        Local {
            serve: serve_state(Ok(ServeCheck::Ready), host),
            host: host.map(str::to_owned),
            token,
            push: false,
        }
    }

    /// A poll while the dialog is open: `mk web` runs, Tailscale answered.
    fn polled_on(phone: &mut Phone, now: Instant, host: Option<&str>, token: Option<String>) {
        phone.polled(
            Ok(web(true, true, None)),
            Some(local(host, token)),
            now,
            COLORS,
        );
    }

    #[test]
    fn the_status_row_follows_the_daemon() {
        let row = status_row(&known(false, false, None), None);
        assert_eq!(
            (row.mark, row.text.as_str(), row.switch),
            (Mark::Off, "Off", Some(Switch::Start))
        );
        let row = status_row(&known(true, true, None), None);
        assert_eq!(
            (row.mark, row.text.as_str(), row.switch),
            (Mark::On, "On", Some(Switch::Stop))
        );
        // The daemon restarts a failed child with backoff: Stop stays, the reason shows, and an
        // error that already ends with a period does not get a second one.
        let row = status_row(&known(true, false, Some("address already in use.")), None);
        assert_eq!(row.mark, Mark::Down);
        assert_eq!(row.text, "Not running: address already in use. Retrying…");
        assert_eq!(row.switch, Some(Switch::Stop));
        // Enabled but no error yet: the daemon is still starting the child.
        let row = status_row(&known(true, false, None), None);
        assert_eq!(
            (row.text.as_str(), row.switch),
            ("Starting…", Some(Switch::Stop))
        );
        let row = status_row(&Daemon::Loading, None);
        assert_eq!((row.text.as_str(), row.switch), ("Checking…", None));
        let row = status_row(&Daemon::Failed("closed".into()), None);
        assert_eq!(row.mark, Mark::Down);
        assert_eq!(row.text, "Cannot read the daemon status: closed");
        assert_eq!(row.switch, None);
    }

    #[test]
    fn a_click_shows_progress_without_a_button_until_the_daemon_agrees() {
        let row = status_row(&known(false, false, None), Some(true));
        assert_eq!(
            (row.mark, row.text.as_str(), row.switch),
            (Mark::Wait, "Starting…", None)
        );
        let row = status_row(&known(true, true, None), Some(false));
        assert_eq!(
            (row.mark, row.text.as_str(), row.switch),
            (Mark::Wait, "Stopping…", None)
        );
    }

    #[test]
    fn the_wait_ends_when_the_daemon_agrees_or_after_five_seconds() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        phone.written(true, t0);
        assert_eq!(phone.pending(), Some(true));
        // The daemon has not re-read settings.toml yet.
        phone.polled(
            Ok(web(false, false, None)),
            None,
            t0 + Duration::from_secs(1),
            COLORS,
        );
        assert_eq!(phone.pending(), Some(true));
        phone.polled(
            Ok(web(true, false, None)),
            None,
            t0 + Duration::from_secs(2),
            COLORS,
        );
        assert_eq!(phone.pending(), None);
        // A daemon that never agrees (settings.toml it cannot read) does not leave the row stuck.
        phone.written(false, t0);
        phone.polled(
            Ok(web(true, true, None)),
            None,
            t0 + Duration::from_secs(4),
            COLORS,
        );
        assert_eq!(phone.pending(), Some(false));
        phone.polled(
            Ok(web(true, true, None)),
            None,
            t0 + Duration::from_secs(5),
            COLORS,
        );
        assert_eq!(phone.pending(), None);
    }

    #[test]
    fn a_failed_status_call_says_why_and_the_next_one_recovers() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(!phone.running());
        phone.polled(Ok(web(true, true, None)), None, t0, COLORS);
        assert!(phone.running());
        phone.polled(Err("pipe closed".into()), None, t0, COLORS);
        assert!(
            !phone.running(),
            "no icon accent for a status that could not be read"
        );
        assert_eq!(phone.daemon, Daemon::Failed("pipe closed".into()));
        phone.polled(Ok(web(true, true, None)), None, t0, COLORS);
        assert!(phone.running());
    }

    #[test]
    fn the_link_shows_only_while_on_with_a_name() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert_eq!(phone.link_block(), LinkBlock::Hidden);
        // On, a name, but `mk web` has not written web_token yet (first start).
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), None);
        assert_eq!(phone.link_block(), LinkBlock::Starting);
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        assert_eq!(phone.link_block(), LinkBlock::Ready);
        assert_eq!(
            phone.link().map(|link| link.text.clone()),
            Some(format!("https://pc.tail1.ts.net/#t={}", token("a1")))
        );
        // No MagicDNS name: nothing to link to.
        polled_on(&mut phone, t0, None, Some(token("a1")));
        assert_eq!(phone.link_block(), LinkBlock::Hidden);
        assert!(phone.link().is_none());
        // Off: the link is hidden even with a name and a token.
        phone.polled(
            Ok(web(false, false, None)),
            Some(local(Some("pc.tail1.ts.net"), Some(token("a1")))),
            t0,
            COLORS,
        );
        assert_eq!(phone.link_block(), LinkBlock::Hidden);
    }

    #[test]
    fn the_qr_is_rebuilt_only_when_the_link_changes() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        let first = phone
            .link()
            .and_then(|link| link.image.clone())
            .expect("a QR for a normal link");
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        let same = phone
            .link()
            .and_then(|link| link.image.clone())
            .expect("still there");
        assert!(
            Arc::ptr_eq(&first, &same),
            "a poll every second must not re-render the QR"
        );
        // `mk web --new-token`: the old QR must not stay on screen.
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("b2")));
        let next = phone.link().expect("link");
        assert_eq!(
            next.text,
            format!("https://pc.tail1.ts.net/#t={}", token("b2"))
        );
        assert!(!Arc::ptr_eq(&first, next.image.as_ref().expect("QR")));
        // The token file goes away: no link, no stale QR.
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), None);
        assert!(phone.link().is_none());
    }

    #[test]
    fn a_poll_without_tailscale_data_keeps_what_the_dialog_knew() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        // Dialog closed: only `daemon.status` is read.
        phone.polled(Ok(web(true, true, None)), None, t0, COLORS);
        assert_eq!(phone.serve, Serve::Ready);
        assert!(phone.link().is_some());
    }

    #[test]
    fn copying_returns_the_link_and_the_label_resets_on_the_next_poll() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert_eq!(phone.copy_link(), None);
        assert!(!phone.copied);
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        assert_eq!(
            phone.copy_link(),
            Some(format!("https://pc.tail1.ts.net/#t={}", token("a1")))
        );
        assert!(phone.copied);
        polled_on(&mut phone, t0, Some("pc.tail1.ts.net"), Some(token("a1")));
        assert!(!phone.copied);
    }

    #[test]
    fn serve_state_tells_missing_from_unavailable() {
        assert_eq!(
            serve_state(Ok(ServeCheck::Ready), Some("pc.ts.net")),
            Serve::Ready
        );
        assert_eq!(
            serve_state(Ok(ServeCheck::Missing), Some("pc.ts.net")),
            Serve::Missing
        );
        // Logged out: `tailscale serve status` answers, but there is no MagicDNS name.
        assert_eq!(
            serve_state(Ok(ServeCheck::Missing), None),
            Serve::Unavailable(None)
        );
        assert_eq!(
            serve_state(Ok(ServeCheck::Ready), None),
            Serve::Unavailable(None)
        );
        assert_eq!(
            serve_state(Err("program not found".into()), None),
            Serve::Unavailable(Some("program not found".into()))
        );
    }

    #[test]
    fn the_tailscale_row_offers_set_up_only_when_it_can_work() {
        let row = serve_row(&Serve::Checking, &Setup::Idle);
        assert_eq!((row.text, row.button), ("Checking Tailscale…", None));
        let row = serve_row(&Serve::Ready, &Setup::Idle);
        assert_eq!(
            (row.mark, row.text, row.button),
            (Mark::Done, "Tailscale serve is set up", None)
        );
        let row = serve_row(&Serve::Missing, &Setup::Idle);
        assert_eq!(row.text, "Tailscale doesn't publish Mkrew yet");
        assert_eq!((row.button, row.error), (Some(SetUp::Ready), None));
        let row = serve_row(&Serve::Missing, &Setup::Running);
        assert_eq!(row.button, Some(SetUp::Busy));
        let row = serve_row(&Serve::Missing, &Setup::Failed("access denied".into()));
        assert_eq!(
            row.button,
            Some(SetUp::Ready),
            "Set up can be pressed again"
        );
        assert_eq!(row.error.as_deref(), Some("access denied"));
        let row = serve_row(&Serve::Unavailable(Some("not found".into())), &Setup::Idle);
        assert_eq!(
            (row.mark, row.text, row.button),
            (Mark::Down, "Install and sign in to Tailscale first", None)
        );
        assert_eq!(row.error.as_deref(), Some("not found"));
        let row = serve_row(&Serve::Unavailable(None), &Setup::Idle);
        assert_eq!((row.button, row.error), (None, None));
    }

    #[test]
    fn set_up_runs_once_and_can_be_pressed_again_after_a_failure() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(
            !phone.begin_setup(),
            "nothing to set up before Tailscale was checked"
        );
        phone.polled(
            Ok(web(true, true, None)),
            Some(Local {
                serve: Serve::Missing,
                host: Some("pc.ts.net".into()),
                token: None,
                push: false,
            }),
            t0,
            COLORS,
        );
        assert!(phone.begin_setup());
        assert!(
            !phone.begin_setup(),
            "a second click while it runs does nothing"
        );
        phone.setup_done(Err("access denied".into()));
        assert_eq!(phone.setup, Setup::Failed("access denied".into()));
        assert!(phone.begin_setup());
        phone.setup_done(Ok(()));
        assert_eq!(
            (phone.setup.clone(), phone.serve.clone()),
            (Setup::Idle, Serve::Ready)
        );
        assert!(!phone.begin_setup(), "already set up");
    }

    #[test]
    fn the_notifications_row_exists_only_while_phone_access_is_on() {
        assert_eq!(push_row(&Daemon::Loading, true), None);
        assert_eq!(push_row(&Daemon::Failed("closed".into()), true), None);
        assert_eq!(
            push_row(&known(false, false, None), true),
            None,
            "phone access off"
        );
        let off = push_row(&known(true, true, None), false).unwrap();
        assert_eq!(
            (off.mark, off.text, off.switch),
            (Mark::Off, "Phone notifications: off", PushSwitch::TurnOn)
        );
        // Enabled but `mk web` not up yet (restarting): the row stays so it does not flicker.
        let on = push_row(&known(true, false, Some("address in use")), true).unwrap();
        assert_eq!(
            (on.mark, on.text, on.switch),
            (Mark::On, "Phone notifications: on", PushSwitch::TurnOff)
        );
    }

    #[test]
    fn a_poll_brings_the_push_switch_from_settings_toml() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(!phone.push);
        phone.polled(
            Ok(web(true, true, None)),
            Some(Local {
                push: true,
                ..local(Some("pc.ts.net"), None)
            }),
            t0,
            COLORS,
        );
        assert!(phone.push);
        // The icon's own poll (dialog closed) has no `Local`: the last value stays.
        phone.polled(Ok(web(true, true, None)), None, t0, COLORS);
        assert!(phone.push);
    }

    #[test]
    fn opening_forgets_the_last_check_but_not_the_daemon_state() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        polled_on(&mut phone, t0, Some("pc.ts.net"), Some(token("a1")));
        phone.write_failed("disk full".into());
        phone.opened();
        assert_eq!(phone.serve, Serve::Checking);
        assert_eq!(phone.setup, Setup::Idle);
        assert_eq!(phone.notice, None);
        assert!(phone.running());
    }

    #[test]
    fn a_failed_write_is_reported_and_clears_with_the_next_attempt() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        phone.write_failed("access denied".into());
        assert_eq!(
            phone.notice.as_deref(),
            Some("Cannot write settings.toml: access denied")
        );
        phone.written(true, t0);
        assert_eq!(phone.notice, None);
    }

    #[test]
    fn polls_do_not_overlap_and_a_closed_dialog_polls_slowly() {
        let t0 = Instant::now();
        let mut phone = Phone::default();
        assert!(phone.due(t0, false), "never polled");
        assert!(phone.begin_poll(t0));
        assert!(!phone.begin_poll(t0), "one request in flight at a time");
        phone.polled(Ok(web(false, false, None)), None, t0, COLORS);
        assert!(phone.begin_poll(t0));
        phone.polled(Ok(web(false, false, None)), None, t0, COLORS);
        assert!(!phone.due(t0 + Duration::from_secs(4), false));
        assert!(phone.due(t0 + theme::PHONE_POLL_IDLE, false));
        assert!(phone.due(t0, true), "open: every tick");
    }
}
