//! Phone access: while `[web] enabled`, the daemon runs `mk web` as a child and reports it in
//! `daemon.status`. The `mk` here is the fake agent (`MKREW_MK_EXE`): it takes `web` as its
//! prompt and plays the scenario named by `MKREW_FAKE_SCENARIO`.

use mkrew_protocol::WebStatus;

use super::*;

/// The fake agent's `ask` with the daemon's null stdin ends at once, printing this to stderr.
const STDIN_CLOSED: &str = "mkrew-fakeagent: stdin closed while waiting for an answer";

impl Env {
    /// `settings.toml` with `[web] enabled = <on>`.
    fn set_web(&self, on: bool) {
        let dir = self.dir.path().join("config");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("settings.toml"),
            format!("[web]\nenabled = {on}\n"),
        )
        .unwrap();
    }

    /// A daemon whose `mk` is the fake agent playing `scenario` (TOML text).
    fn start_with_mk(&self, scenario: &str) -> (Daemon, Client) {
        let file = self.dir.path().join("mk-scenario.toml");
        std::fs::write(&file, scenario).unwrap();
        let fake = built_bin("mkrew-fakeagent");
        self.start_with(&[
            (mkrew_daemon::ENV_MK_EXE, fake.as_path()),
            ("MKREW_FAKE_SCENARIO", file.as_path()),
        ])
    }
}

/// A `mk web` that lives 15 s and rewrites `beat` with a counter every 100 ms.
fn heartbeat(beat: &Path) -> String {
    (0..150)
        .map(|n| {
            format!(
                "[[step]]\naction = \"write_file\"\npath = '{}'\ncontent = \"{n}\"\n\n\
                 [[step]]\naction = \"sleep\"\nms = 100\n\n",
                beat.display()
            )
        })
        .collect()
}

/// True when `beat` did not change for 600 ms: nothing writes it any more.
fn is_frozen(beat: &Path) -> bool {
    let before = std::fs::read_to_string(beat).unwrap_or_default();
    std::thread::sleep(Duration::from_millis(600));
    std::fs::read_to_string(beat).unwrap_or_default() == before
}

/// Polls `daemon.status` until `web` satisfies `wanted`; returns it.
fn wait_web(client: &mut Client, wanted: impl Fn(&WebStatus) -> bool) -> WebStatus {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
        if wanted(&status.web) {
            return status.web;
        }
        assert!(
            Instant::now() < deadline,
            "web status stayed {:?}",
            status.web
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn mk_web_runs_while_enabled_and_dies_when_disabled() {
    let env = Env::new();
    let beat = env.dir.path().join("beat.txt");
    env.set_web(true);
    let (mut daemon, mut client) = env.start_with_mk(&heartbeat(&beat));

    let on = wait_web(&mut client, |w| w.running);
    assert_eq!(
        on,
        WebStatus {
            enabled: true,
            running: true,
            error: None
        }
    );
    wait_text(&beat); // the child really runs, not only reported

    env.set_web(false);
    // `enabled` only turns false after the child was killed and reaped.
    let off = wait_web(&mut client, |w| !w.enabled);
    assert_eq!(off, WebStatus::default());
    assert!(
        is_frozen(&beat),
        "mk web still runs after it was turned off"
    );

    // The first child's last beat must not pass for the new child's.
    std::fs::remove_file(&beat).unwrap();
    env.set_web(true);
    wait_web(&mut client, |w| w.running);
    wait_text(&beat);
    assert!(!is_frozen(&beat), "turning it on again starts a new child");
    client.stop();
    assert!(daemon.wait_exit().success());
}

#[test]
fn mk_web_dies_with_the_daemon() {
    let env = Env::new();
    let beat = env.dir.path().join("beat.txt");
    env.set_web(true);
    let (mut daemon, mut client) = env.start_with_mk(&heartbeat(&beat));
    wait_web(&mut client, |w| w.running);
    wait_text(&beat);

    client.stop();
    assert!(daemon.wait_exit().success());
    assert!(is_frozen(&beat), "mk web outlived the daemon");
}

/// A killed daemon (Task Manager, crash) runs no cleanup: the job object takes `mk web` along.
#[cfg(windows)]
#[test]
fn mk_web_dies_when_the_daemon_is_killed() {
    let env = Env::new();
    let beat = env.dir.path().join("beat.txt");
    env.set_web(true);
    let (mut daemon, mut client) = env.start_with_mk(&heartbeat(&beat));
    wait_web(&mut client, |w| w.running);
    wait_text(&beat);

    daemon.0.kill().unwrap();
    daemon.wait_exit();
    assert!(is_frozen(&beat), "mk web outlived the killed daemon");
}

#[test]
fn a_mk_web_that_exits_at_once_reports_its_last_stderr_line() {
    let env = Env::new();
    env.set_web(true);
    let (mut daemon, mut client) = env.start_with_mk("[[step]]\naction = \"ask\"\n");

    let down = wait_web(&mut client, |w| !w.running && w.error.is_some());
    assert_eq!(
        down,
        WebStatus {
            enabled: true,
            running: false,
            error: Some(STDIN_CLOSED.into())
        }
    );

    env.set_web(false);
    let off = wait_web(&mut client, |w| !w.enabled);
    assert_eq!(off, WebStatus::default(), "turning it off clears the error");
    client.stop();
    assert!(daemon.wait_exit().success());
}

#[test]
fn a_missing_mk_is_reported() {
    let env = Env::new();
    env.set_web(true);
    let missing = env.dir.path().join("no-such-mk");
    let (mut daemon, mut client) = env.start_with(&[(mkrew_daemon::ENV_MK_EXE, missing.as_path())]);

    let web = wait_web(&mut client, |w| w.error.is_some());
    assert_eq!(
        web,
        WebStatus {
            enabled: true,
            running: false,
            error: Some(format!(
                "mk{} not found next to mkrew-daemon",
                std::env::consts::EXE_SUFFIX
            ))
        }
    );
    client.stop();
    assert!(daemon.wait_exit().success());
}
