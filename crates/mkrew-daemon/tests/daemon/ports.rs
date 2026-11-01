//! `port.list` / `port.stop` (spec 2026-11-01) against real processes on `127.0.0.1:0`.

use super::*;

/// The command line that starts the fake dev server.
fn listener_line() -> String {
    format!("\"{}\" --listen", built_bin("mkrew-fakeagent").display())
}

fn port_list(client: &mut Client) -> Vec<PortRow> {
    client.call(Request::PortList).into_result().unwrap()
}

/// Waits until `session_id` holds a listening port and returns its row.
fn port_of(client: &mut Client, session_id: Ulid) -> PortRow {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Some(row) = port_list(client)
            .into_iter()
            .find(|r| r.session_id == Some(session_id))
        {
            return row;
        }
        assert!(
            Instant::now() < deadline,
            "no port for session {session_id}"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Waits until nothing holds `port`.
fn wait_closed(client: &mut Client, port: u16) {
    let deadline = Instant::now() + TIMEOUT;
    while port_list(client).iter().any(|r| r.port == port) {
        assert!(Instant::now() < deadline, ":{port} still listens");
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn stop(client: &mut Client, row: &PortRow) -> Outcome {
    client.call(Request::PortStop(PortStopParams {
        port: row.port,
        pid: row.pid,
    }))
}

#[test]
fn a_run_command_port_is_listed_and_stopped_with_its_session() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let web = run_command("web", ".", &listener_line());
    client
        .call(set_runs(&project, vec![web], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client
        .call(run_start(&project, None))
        .into_result()
        .unwrap();
    let id = started.sessions[0].session_id;

    let row = port_of(&mut client, id);
    assert_eq!(row.project_id, Some(project.id));
    assert_eq!(row.run_command.as_deref(), Some("web"));
    assert_eq!(row.worktree_task_id, None);
    // Linux shortens process names to 15 characters.
    assert!(row.process.starts_with("mkrew-fakeagen"), "{}", row.process);

    stop(&mut client, &row).into_result::<()>().unwrap();
    client.wait_state(id, SessionState::Exited);
    wait_closed(&mut client, row.port);
    client.stop();
}

#[test]
fn stopping_a_shell_pane_port_keeps_the_pane() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    client
        .call(set_default_shell(&project, Some("shell")))
        .into_result::<Project>()
        .unwrap();
    let shell = client
        .call(spawn_params(&project, SessionKind::Shell, None))
        .into_result::<SessionSpawnResult>()
        .unwrap()
        .session_id;
    client
        .input(shell, format!("{}\r", listener_line()).as_bytes())
        .into_result::<()>()
        .unwrap();

    let row = port_of(&mut client, shell);
    assert_eq!(row.project_id, Some(project.id));
    assert_eq!(row.run_command, None);

    stop(&mut client, &row).into_result::<()>().unwrap();
    wait_closed(&mut client, row.port);
    assert_ne!(
        client.session(shell).state,
        SessionState::Exited,
        "the pane stays"
    );
    client.stop();
}

#[test]
fn a_stale_pid_is_not_found() {
    let env = Env::new();
    let (_daemon, mut client) = env.start();
    let outcome = client.call(Request::PortStop(PortStopParams {
        port: 1,
        pid: u32::MAX - 1,
    }));
    assert_eq!(error_code(outcome), ErrorCode::NotFound);
    client.stop();
}
