//! Attach, output streaming, paste back-pressure, send queue, `session.read`.

use super::*;

#[test]
fn daemon_stop_terminates_live_sessions() {
    let env = Env::new();
    env.write_fake_profile();
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    client.wait_state(spawned.session_id, SessionState::NeedsInput);

    let started = Instant::now();
    client.stop();
    assert!(daemon.wait_exit().success());
    // Under the daemon's 5 s kill grace: the graceful close (ConPTY close / SIGHUP) worked.
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );

    let sessions = Store::open(&env.data_dir().join("mkrew.db"))
        .unwrap()
        .list_sessions(None)
        .unwrap();
    assert_eq!(sessions[0].state, SessionState::Exited);
}

#[test]
fn attach_streams_output_and_reattach_restores_the_screen() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut first) = env.start();
    let project = first.add_project(&env.repo());
    let spawned: SessionSpawnResult = first.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    first.wait_state(id, SessionState::NeedsInput);

    let attached: SessionAttachResult = first.attach(id).into_result().unwrap();
    assert_eq!(attached.state, SessionState::NeedsInput);
    assert_eq!((attached.snapshot.cols, attached.snapshot.rows), (80, 24));
    let screen = screen_text(&attached.snapshot);
    assert!(screen.contains("Plan: edit src/main.rs"), "{screen}");
    assert!(screen.contains("Do you want to proceed? (y/n)"), "{screen}");

    let resize = |cols, rows| {
        Request::SessionResize(SessionResizeParams {
            session_id: id,
            cols,
            rows,
        })
    };
    first.call(resize(100, 30)).into_result::<()>().unwrap();
    assert_eq!(
        error_code(first.call(resize(0, 30))),
        ErrorCode::InvalidRequest
    );
    // Applied by the session task right after the reply.
    let deadline = Instant::now() + TIMEOUT;
    while (first.session(id).cols, first.session(id).rows) != (100, 30) {
        assert!(Instant::now() < deadline, "resize not saved");
        std::thread::sleep(Duration::from_millis(20));
    }
    first
        .call(Request::SessionDetach(SessionRef { session_id: id }))
        .into_result::<()>()
        .unwrap();
    // Output queued before the detach reply (e.g. ConPTY repainting after the resize) is fine.
    let before_detach = first.output.get(&id).map_or(0, Vec::len);

    // Another client re-attaches: the screen comes from the daemon's emulator, at the new size.
    let mut second = Client::connect(&env.socket).unwrap();
    second.hello().unwrap();
    let again: SessionAttachResult = second.attach(id).into_result().unwrap();
    assert_eq!((again.snapshot.cols, again.snapshot.rows), (100, 30));
    let screen = screen_text(&again.snapshot);
    assert!(screen.contains("Do you want to proceed? (y/n)"), "{screen}");

    // Input reaches the agent; its output streams back as binary frames.
    second.input(id, b"y\r").into_result::<()>().unwrap();
    second.wait_output(id, "Edited src/main.rs");
    second.wait_state(id, SessionState::Exited);
    first.call(Request::DaemonStatus);
    assert_eq!(
        first.output.get(&id).map_or(0, Vec::len),
        before_detach,
        "detached client got output"
    );

    assert_eq!(error_code(second.attach(id)), ErrorCode::InvalidState);
    assert_eq!(error_code(second.input(id, b"x")), ErrorCode::InvalidState);
    assert_eq!(error_code(second.attach(Ulid::new())), ErrorCode::NotFound);
    second.stop();
}

#[test]
fn slow_attached_client_gets_a_snapshot_instead_of_stalling_the_session() {
    let env = Env::new();
    env.write_fake_profile_as("flood", "['--scenario', 'flood']");
    let (_daemon, mut slow) = env.start();
    let project = slow.add_project(&env.repo());
    let spawned: SessionSpawnResult = slow.spawn(&project, "flood").into_result().unwrap();
    let id = spawned.session_id;
    slow.attach(id)
        .into_result::<SessionAttachResult>()
        .unwrap();

    // `slow` stops reading. The session must keep running and the daemon keep answering.
    let mut other = Client::connect(&env.socket).unwrap();
    other.hello().unwrap();
    let session = other.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));

    // Its queue overflowed: the dropped output was replaced by a fresh snapshot.
    let deadline = Instant::now() + TIMEOUT;
    let snapshot = loop {
        let found = slow.events.iter().find_map(|e| match e {
            Event::SessionSnapshot(s) if s.session_id == id => Some(s.snapshot.clone()),
            _ => None,
        });
        if let Some(snapshot) = found {
            break snapshot;
        }
        assert!(Instant::now() < deadline, "no session.snapshot event");
        slow.call(Request::DaemonStatus);
    };
    assert_eq!((snapshot.cols, snapshot.rows), (80, 24));
    let received = slow.output.get(&id).map_or(0, Vec::len);
    let log = std::fs::metadata(env.data_dir().join("sessions").join(format!("{id}.log")))
        .unwrap()
        .len();
    assert!(
        (received as u64) < log,
        "received {received} of {log} bytes: nothing was dropped"
    );
    slow.stop();
}

#[test]
fn big_paste_to_a_child_that_does_not_read_leaves_the_daemon_responsive() {
    let env = Env::new();
    // The sleeper prints a line and then never reads its input.
    let scenario = env.dir.path().join("sleeper.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"print\"\ntext = \"Sleeper ready.\"\n\n\
         [[step]]\naction = \"sleep\"\nms = 120000\n",
    )
    .unwrap();
    env.write_fake_profile_as(
        "sleeper",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let sleeper: SessionSpawnResult = client.spawn(&project, "sleeper").into_result().unwrap();
    let sleeper = sleeper.session_id;
    let attached: SessionAttachResult = client.attach(sleeper).into_result().unwrap();
    // Printed before the attach it is in the snapshot, after it in the output frames.
    if !screen_text(&attached.snapshot).contains("Sleeper ready.") {
        client.wait_output(sleeper, "Sleeper ready.");
    }

    // A 4 MiB paste on its own connection; it may wait until the session is killed.
    let socket = env.socket.clone();
    let (done_tx, done) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut paster = Client::connect(&socket).unwrap();
        paster.hello().unwrap();
        let outcome = paster.input(sleeper, &vec![b'a'; 4 * 1024 * 1024]);
        let _ = done_tx.send(outcome);
    });
    std::thread::sleep(Duration::from_millis(500));

    // Other sessions and the daemon keep working while the paste is pending.
    let fake: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let fake = fake.session_id;
    client.wait_state(fake, SessionState::NeedsInput);
    client.input(fake, b"y\r").into_result::<()>().unwrap();
    let session = client.wait_state(fake, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    let status: DaemonStatusResult = client.call(Request::DaemonStatus).into_result().unwrap();
    assert_eq!(status.sessions, 1);
    // Otherwise the OS absorbed the paste and this test proves nothing.
    assert!(done.try_recv().is_err(), "the paste is not backed up");

    // The sleeper's own task still handles commands: a kill ends it and the paste.
    let kill = SessionKillParams {
        session_id: sleeper,
        force: true,
    };
    client
        .call(Request::SessionKill(kill))
        .into_result::<()>()
        .unwrap();
    client.wait_state(sleeper, SessionState::Exited);
    let outcome = done
        .recv_timeout(TIMEOUT)
        .expect("the paste never returned");
    // Normally cut short by the exit; `ok` if the last chunks were queued just before.
    if outcome.clone().into_result::<()>().is_err() {
        assert_eq!(error_code(outcome), ErrorCode::InvalidState);
    }
    client.stop();
}

#[test]
fn send_while_working_is_queued_until_the_agent_asks_and_wait_sees_it_finish() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;

    let working: SessionWaitResult = client
        .call(wait(id, &[SessionState::Working], Some(15_000)))
        .into_result()
        .unwrap();
    assert_eq!(working.state, SessionState::Working);
    // Queued while working, delivered once the prompt shows; the agent then finishes.
    client
        .call(send(id, "y"))
        .into_result::<SessionSendResult>()
        .unwrap();
    let done: SessionWaitResult = client
        .call(wait(
            id,
            &[SessionState::Idle, SessionState::Done],
            Some(30_000),
        ))
        .into_result()
        .unwrap();
    // The agent exits right after answering: exit ends every wait.
    assert!(
        matches!(done.state, SessionState::Idle | SessionState::Exited),
        "{done:?}"
    );
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    assert!(session_log(&env, id).contains("Done."));

    // An exited session answers at once; sending to it is refused.
    let exited: SessionWaitResult = client
        .call(wait(id, &[SessionState::NeedsInput], None))
        .into_result()
        .unwrap();
    assert_eq!(
        exited,
        SessionWaitResult {
            state: SessionState::Exited,
            exit_code: Some(0)
        }
    );
    assert_eq!(
        error_code(client.call(send(id, "y"))),
        ErrorCode::InvalidState
    );
    client.stop();
}

/// Texts sent while the agent works wait with a `send_id`; one removed from the queue is never
/// typed, the other is, and `session.queue` follows every change (SPEC §20B.2).
#[test]
fn a_queued_text_removed_while_working_is_never_typed() {
    let env = Env::new();
    let scenario = env.dir.path().join("twice.toml");
    let tail = "[[step]]\naction = \"print\"\ntext = \"step\"\n\n".repeat(4);
    std::fs::write(
        &scenario,
        format!(
            "[[step]]\naction = \"spinner\"\ntext = \"Working\"\nms = 4000\n\n\
             [[step]]\naction = \"ask\"\n\n{tail}\
             [[step]]\naction = \"ask\"\n\n{tail}\
             [[step]]\naction = \"print\"\ntext = \"Done.\"\n"
        ),
    )
    .unwrap();
    env.write_fake_profile_as(
        "twice",
        &format!("['--scenario', '{}']", scenario.display()),
    );
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "twice").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::Working);

    let kept: SessionSendResult = client.call(send(id, "kept-answer")).into_result().unwrap();
    let removed: SessionSendResult = client
        .call(send(id, "removed-answer"))
        .into_result()
        .unwrap();
    assert!(kept.queued && removed.queued, "{kept:?} {removed:?}");
    assert_ne!(kept.send_id, removed.send_id);
    let both = vec![
        QueuedSend {
            send_id: kept.send_id,
            text: "kept-answer".into(),
        },
        QueuedSend {
            send_id: removed.send_id,
            text: "removed-answer".into(),
        },
    ];
    client.wait_event(&Event::SessionQueue(SessionQueueEvent {
        session_id: id,
        items: both,
    }));
    client
        .call(unqueue(id, removed.send_id))
        .into_result::<()>()
        .unwrap();
    assert_eq!(
        error_code(client.call(unqueue(id, removed.send_id))),
        ErrorCode::NotFound
    );
    client.wait_event(&Event::SessionQueue(SessionQueueEvent {
        session_id: id,
        items: vec![QueuedSend {
            send_id: kept.send_id,
            text: "kept-answer".into(),
        }],
    }));

    // The first question takes the kept text; once written it can no longer be removed.
    client.wait_event(&Event::SessionQueue(SessionQueueEvent {
        session_id: id,
        items: Vec::new(),
    }));
    assert_eq!(
        error_code(client.call(unqueue(id, kept.send_id))),
        ErrorCode::NotFound
    );
    client.wait_state(id, SessionState::NeedsInput);
    client
        .call(send(id, "last-answer"))
        .into_result::<SessionSendResult>()
        .unwrap();
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    let log = session_log(&env, id);
    assert!(log.contains("kept-answer"), "{log}");
    assert!(log.contains("last-answer"), "{log}");
    assert!(!log.contains("removed-answer"), "{log}");
    client.stop();
}

/// A session that exits with texts still waiting announces an empty queue.
#[test]
fn exiting_with_queued_texts_announces_an_empty_queue() {
    let env = Env::new();
    let scenario = env.dir.path().join("slow.toml");
    std::fs::write(
        &scenario,
        "[[step]]\naction = \"spinner\"\ntext = \"Working\"\nms = 20000\n",
    )
    .unwrap();
    env.write_fake_profile_as("slow", &format!("['--scenario', '{}']", scenario.display()));
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "slow").into_result().unwrap();
    let id = spawned.session_id;
    client.wait_state(id, SessionState::Working);
    let sent: SessionSendResult = client.call(send(id, "later")).into_result().unwrap();
    assert!(sent.queued, "{sent:?}");
    let kill = Request::SessionKill(SessionKillParams {
        session_id: id,
        force: true,
    });
    client.call(kill).into_result::<()>().unwrap();
    client.wait_event(&Event::SessionQueue(SessionQueueEvent {
        session_id: id,
        items: Vec::new(),
    }));
    client.wait_state(id, SessionState::Exited);
    client.stop();
}

#[test]
fn session_read_shows_the_prompt_line_and_the_answer_goes_through() {
    let env = Env::new();
    env.write_fake_profile();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&env.repo());
    let spawned: SessionSpawnResult = client.spawn(&project, "fake").into_result().unwrap();
    let id = spawned.session_id;
    let session = client.wait_state(id, SessionState::NeedsInput);

    // Read without attaching: the last non-blank line is the agent's question.
    let last: SessionReadResult = client.call(read(id, 1)).into_result().unwrap();
    assert_eq!(last.lines.len(), 1, "{last:?}");
    assert!(last.lines[0].contains("(y/n)"), "{last:?}");
    assert_eq!(last.state, SessionState::NeedsInput);
    assert_eq!(last.state_changed_at, session.state_changed_at);
    // More lines than the screen has: every non-blank row, top to bottom.
    let all: SessionReadResult = client.call(read(id, 200)).into_result().unwrap();
    assert!(all.lines.iter().all(|l| !l.trim().is_empty()), "{all:?}");
    assert_eq!(
        all.lines.first().map(String::as_str),
        Some("Fake agent ready.")
    );
    assert_eq!(all.lines.last(), last.lines.last());

    for lines in [0, 201] {
        assert_eq!(
            error_code(client.call(read(id, lines))),
            ErrorCode::InvalidRequest
        );
    }
    assert_eq!(
        error_code(client.call(read(Ulid::new(), 1))),
        ErrorCode::NotFound
    );

    // Quick reply: written at once (not queued), the agent finishes and exits.
    let sent: SessionSendResult = client.call(send(id, "y")).into_result().unwrap();
    assert!(!sent.queued, "{sent:?}");
    let session = client.wait_state(id, SessionState::Exited);
    assert_eq!(session.exit_code, Some(0));
    assert!(session_log(&env, id).contains("Done."));
    assert_eq!(
        error_code(client.call(read(id, 1))),
        ErrorCode::InvalidState
    );
    client.stop();
}
