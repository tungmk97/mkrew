//! The omp native surface (TSP) end to end (spec 2026-11-23-omp-tsp-surface-design §9): the
//! fake agent in `--tsp` mode replays the committed startup record of a real omp, the daemon
//! answers its hello, acks its frames, serves `tsp.snapshot` and `tsp.update`, and writes
//! `tsp.event` back to the agent's stdin (which the fake agent records to a file).
//!
//! Windows prerequisite: `conpty.dll` and `OpenConsole.exe` (Task 1, `scripts/fetch-conpty.sh`)
//! must sit next to `mkrew-daemon.exe` in `target/debug`. The system ConPTY drops the APC
//! query, so the hello never arrives and the live-path tests below fall back instead.
//! Build the fake agent first: `cargo build -p mkrew-cli -p mkrew-fakeagent --bins`.

use mkrew_protocol::{
    TspChange, TspEventParams, TspNode, TspSnapshot, TspSnapshotParams, TspState,
};

use super::*;

/// omp 18.13.2's own record of its startup: `o`, `t`, ten frames and their acks.
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/superpowers/specs/2026-11-23-omp-tsp/omp-18.13.2-startup.tsp.jsonl"
);
/// Frames in [`FIXTURE`] (`s` 1 to 10).
const FIXTURE_FRAMES: u64 = 10;
const NO_HELLO_HINT: &str = "the session fell back to the terminal: on Windows put conpty.dll \
and OpenConsole.exe next to mkrew-daemon.exe (scripts/fetch-conpty.sh)";

/// The fake profile `id` with `tsp = true` and the fake agent's `args` (a TOML array).
fn write_tsp_profile(env: &Env, id: &str, args: &str) {
    env.write_fake_profile_as(id, args);
    let path = env
        .dir
        .path()
        .join("config")
        .join("agents")
        .join(format!("{id}.toml"));
    let text = std::fs::read_to_string(&path).unwrap();
    let tsp = text.replace(
        "prompt_mode = \"none\"",
        "prompt_mode = \"none\"\ntsp = true",
    );
    assert_ne!(tsp, text);
    env.write_profile(id, &tsp);
}

/// A `--tsp` profile; returns the file the fake agent records its stdin to.
fn write_replaying_profile(env: &Env, id: &str) -> PathBuf {
    let record = env.dir.path().join(format!("{id}-record.bin"));
    let args = format!("['--tsp', '{FIXTURE}', '--record', '{}']", record.display());
    write_tsp_profile(env, id, &args);
    record
}

fn tsp_snapshot(client: &mut Client, id: Ulid) -> TspSnapshot {
    client
        .call(Request::TspSnapshot(TspSnapshotParams { session_id: id }))
        .into_result()
        .unwrap()
}

fn tsp_event(client: &mut Client, id: Ulid, event: Value) -> Outcome {
    client.call(Request::TspEvent(TspEventParams {
        session_id: id,
        surface: "s:1".into(),
        event,
    }))
}

/// Waits for the `session.updated` that reports `want` (or `fallback`, which ends the wait).
fn wait_tsp(client: &mut Client, id: Ulid, want: TspState) {
    let event = client.wait_event_where(|e| {
        matches!(e, Event::SessionUpdated(s)
            if s.id == id && (s.tsp == want || s.tsp == TspState::Fallback))
    });
    let Event::SessionUpdated(session) = event else {
        unreachable!()
    };
    assert_eq!(session.tsp, want, "{NO_HELLO_HINT}");
}

/// Polls `tsp.snapshot` until the first surface has applied frame `seq`.
fn wait_frames(client: &mut Client, id: Ulid, seq: u64) -> TspSnapshot {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let snapshot = tsp_snapshot(client, id);
        if snapshot.surfaces.first().is_some_and(|s| s.seq >= seq) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "surfaces stuck at {:?}, wanted seq {seq}",
            snapshot.surfaces.iter().map(|s| s.seq).collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Waits until what the fake agent recorded satisfies `ready`; returns it.
fn wait_record(path: &Path, ready: impl Fn(&str) -> bool) -> String {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        let text = std::fs::read(path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        if ready(&text) {
            return text;
        }
        assert!(Instant::now() < deadline, "the agent recorded {text:?}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn find<'a>(node: &'a TspNode, wanted: &impl Fn(&TspNode) -> bool) -> Option<&'a TspNode> {
    if wanted(node) {
        return Some(node);
    }
    node.c.iter().find_map(|child| find(child, wanted))
}

fn kill(client: &mut Client, id: Ulid) {
    let request = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(request).into_result::<()>().unwrap();
    client.wait_state(id, SessionState::Exited);
}

#[test]
fn a_tsp_session_goes_live_and_trades_acks_and_events_with_the_agent() {
    let env = Env::new();
    let record = write_replaying_profile(&env, "tsp");
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "tsp").into_result().unwrap();
    let id = spawned.session_id;

    // waiting -> live, announced and listed.
    wait_tsp(&mut client, id, TspState::Live);
    assert_eq!(client.session(id).tsp, TspState::Live);

    // Every frame reaches clients once, in order, after the surface opened.
    let last = FIXTURE_FRAMES;
    client.wait_event_where(|e| {
        matches!(e, Event::TspUpdate(u) if u.session_id == id
            && matches!(u.change, TspChange::Frame { seq, .. } if seq == last))
    });
    let changes: Vec<&TspChange> = client
        .events
        .iter()
        .filter_map(|e| match e {
            Event::TspUpdate(u) if u.session_id == id => {
                assert_eq!(u.surface, "s:1");
                Some(&u.change)
            }
            _ => None,
        })
        .collect();
    assert!(
        matches!(changes.first(), Some(TspChange::Open { .. })),
        "{changes:?}"
    );
    let seqs: Vec<u64> = changes
        .iter()
        .filter_map(|c| match c {
            TspChange::Frame { seq, .. } => Some(*seq),
            _ => None,
        })
        .collect();
    assert_eq!(seqs, (1..=FIXTURE_FRAMES).collect::<Vec<_>>());

    // The snapshot is what a late client starts from.
    let snapshot = wait_frames(&mut client, id, FIXTURE_FRAMES);
    assert_eq!(snapshot.state, TspState::Live);
    assert_eq!(snapshot.surfaces.len(), 1);
    let surface = &snapshot.surfaces[0];
    assert_eq!((surface.id.as_str(), surface.seq), ("s:1", FIXTURE_FRAMES));
    assert!(
        find(&surface.root, &|n| n.k == "editor").is_some(),
        "{:?}",
        surface.root
    );

    // A click on a node with `actions.click` reaches the agent's stdin as `e action`.
    let clickable = find(&surface.root, &|n| {
        n.p.get("actions")
            .and_then(|a| a.get("click"))
            .is_some_and(Value::is_string)
    })
    .expect("the fixture has clickable nodes");
    let act = clickable.p["actions"]["click"].as_str().unwrap().to_owned();
    let event = json!({"ev": "action", "id": clickable.id, "act": act});
    tsp_event(&mut client, id, event)
        .into_result::<()>()
        .unwrap();

    let acks = |text: &str| text.matches("\"ev\":\"ack\"").count();
    let recorded = wait_record(&record, |text| {
        text.contains("\"ev\":\"action\"") && acks(text) >= FIXTURE_FRAMES as usize
    });
    assert!(recorded.contains("\x1b_tsp;e;"), "{recorded:?}");
    assert!(
        recorded.contains(&format!("\"act\":\"{act}\"")),
        "{recorded:?}"
    );
    assert!(recorded.contains("\"sf\":\"s:1\""), "{recorded:?}");
    // The hello reply came first (omp waits for it), and each frame was acked exactly once.
    assert!(recorded.starts_with("\x1b_tsp;r;"), "{recorded:?}");
    assert!(recorded.contains("\"r\":\"hello\""), "{recorded:?}");
    assert_eq!(acks(&recorded), FIXTURE_FRAMES as usize, "{recorded:?}");

    // TSP bytes never reach the log or the screen; the plain text after them does.
    kill(&mut client, id);
    let log = session_log(&env, id);
    assert!(log.contains("fake tsp done"), "{log}");
    assert!(!log.contains("_tsp;"), "{log:?}");

    // Once the session is gone the requests say so.
    let snapshot = Request::TspSnapshot(TspSnapshotParams { session_id: id });
    assert_eq!(error_code(client.call(snapshot)), ErrorCode::InvalidState);
    let unknown = Request::TspSnapshot(TspSnapshotParams {
        session_id: Ulid::new(),
    });
    assert_eq!(error_code(client.call(unknown)), ErrorCode::NotFound);
    client.stop();
}

#[test]
fn a_tsp_profile_whose_agent_says_no_hello_falls_back_to_the_terminal() {
    let env = Env::new();
    // The default `ask` scenario prints text and asks for confirmation; it never says hello.
    write_tsp_profile(&env, "tsp", "[]");
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "tsp").into_result().unwrap();
    let id = spawned.session_id;

    // Within the 2 s hello wait after the first output (TIMEOUT only bounds the test).
    wait_tsp(&mut client, id, TspState::Fallback);
    // The pane is today's terminal: the screen still drives the state, the text is logged.
    client.wait_state(id, SessionState::NeedsInput);
    let snapshot = tsp_snapshot(&mut client, id);
    assert_eq!(snapshot.state, TspState::Fallback);
    assert!(snapshot.surfaces.is_empty(), "{snapshot:?}");
    assert_eq!(client.session(id).tsp, TspState::Fallback);
    kill(&mut client, id);
    let log = session_log(&env, id);
    assert!(log.contains("Fake agent ready."), "{log}");
    client.stop();
}

#[test]
fn tsp_event_rejects_events_the_p1_list_does_not_have_and_never_writes_them() {
    let env = Env::new();
    let record = write_replaying_profile(&env, "tsp");
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "tsp").into_result().unwrap();
    let id = spawned.session_id;
    wait_tsp(&mut client, id, TspState::Live);
    let snapshot = wait_frames(&mut client, id, FIXTURE_FRAMES);
    let editor = find(&snapshot.surfaces[0].root, &|n| n.k == "editor").unwrap();

    // Local text editing is P3: `edit` is refused, with a reason, before anything is written.
    let edit = json!({"ev": "edit", "id": editor.id, "from": 0, "to": 0, "cursor": 1, "len": 0, "text": "a"});
    assert_eq!(
        error_code(tsp_event(&mut client, id, edit)),
        ErrorCode::InvalidRequest
    );
    let no_ev = json!({"id": editor.id});
    assert_eq!(
        error_code(tsp_event(&mut client, id, no_ev)),
        ErrorCode::InvalidRequest
    );
    // A valid event after it proves the order on the wire: `focus` arrives, `edit` never did.
    let focus = json!({"ev": "focus", "id": editor.id});
    tsp_event(&mut client, id, focus)
        .into_result::<()>()
        .unwrap();
    let recorded = wait_record(&record, |text| text.contains("\"ev\":\"focus\""));
    assert!(!recorded.contains("\"ev\":\"edit\""), "{recorded:?}");
    kill(&mut client, id);
    client.stop();
}

#[test]
fn a_session_without_tsp_answers_off_and_takes_no_events() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;

    let snapshot = tsp_snapshot(&mut client, id);
    assert_eq!(
        snapshot,
        TspSnapshot {
            state: TspState::Off,
            surfaces: Vec::new(),
            blobs: Vec::new(),
            reason: None,
        }
    );
    assert_eq!(client.session(id).tsp, TspState::Off);
    let theme = json!({"ev": "theme", "dark": true});
    assert_eq!(
        error_code(tsp_event(&mut client, id, theme)),
        ErrorCode::InvalidRequest
    );
    kill(&mut client, id);
    client.stop();
}

#[test]
fn the_agent_is_started_with_pi_tui_native_matching_its_tsp_setting() {
    let env = Env::new();
    let script = if cfg!(windows) {
        "command = 'cmd.exe'\nargs = ['/c', 'echo', 'native=%PI_TUI_NATIVE%']"
    } else {
        "command = 'sh'\nargs = ['-c', 'echo native=$PI_TUI_NATIVE']"
    };
    env.write_profile("on", &format!("name = \"On\"\n{script}\ntsp = true\n"));
    env.write_profile("off", &format!("name = \"Off\"\n{script}\ntsp = false\n"));
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    for (profile, expected) in [("on", "native=1"), ("off", "native=0")] {
        let spawned: SessionSpawnResult = client.spawn(&project, profile).into_result().unwrap();
        client.wait_state(spawned.session_id, SessionState::Exited);
        let log = session_log(&env, spawned.session_id);
        assert!(log.contains(expected), "{profile}: {log}");
    }
    client.stop();
}
