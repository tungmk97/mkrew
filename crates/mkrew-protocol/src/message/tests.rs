use super::*;
use serde_json::json;

/// A stored `project.color` must keep passing the daemon's check after the palette grew.
#[test]
fn workspace_colors_keep_the_names_stored_before_lime_was_added() {
    for old in [
        "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink",
    ] {
        assert!(WORKSPACE_COLORS.contains(&old), "{old}");
    }
    assert!(WORKSPACE_COLORS.contains(&"lime"));
    let mut sorted = WORKSPACE_COLORS;
    sorted.sort_unstable();
    assert!(
        sorted.windows(2).all(|pair| pair[0] != pair[1]),
        "unique names"
    );
}

fn sid() -> Ulid {
    Ulid::from_parts(1_700_000_000_000, 7)
}

fn round_trip(message: &Message) -> serde_json::Value {
    let value = serde_json::to_value(message).unwrap();
    let back: Message = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(&back, message);
    value
}

#[test]
fn turn_wire_shape_omits_absent_fields() {
    let turn = Turn {
        n: 1,
        started_at: 5,
        ended_at: None,
        start_tree: None,
        end_tree: None,
        files: vec![],
    };
    let value = serde_json::to_value(&turn).unwrap();
    assert_eq!(value, json!({ "n": 1, "started_at": 5, "files": [] }));
    assert_eq!(serde_json::from_value::<Turn>(value).unwrap(), turn);
}

#[test]
fn request_wire_shape_matches_spec() {
    let message = Message::Request {
        id: 7,
        request: Request::SessionResize(SessionResizeParams {
            session_id: sid(),
            cols: 120,
            rows: 40,
        }),
    };
    assert_eq!(
        round_trip(&message),
        json!({
            "type": "request", "id": 7, "method": "session.resize",
            "params": {"session_id": sid().to_string(), "cols": 120, "rows": 40}
        })
    );
}

#[test]
fn param_less_requests_omit_params() {
    let message = Message::Request {
        id: 1,
        request: Request::DaemonStatus,
    };
    assert_eq!(
        round_trip(&message),
        json!({"type": "request", "id": 1, "method": "daemon.status"})
    );
}

#[test]
fn every_m1_request_round_trips() {
    let requests = vec![
        Request::Hello(HelloParams {
            client: ClientKind::Ui,
            protocol_version: crate::PROTOCOL_VERSION,
        }),
        Request::DaemonStatus,
        Request::DaemonStop,
        Request::ProjectAdd(ProjectAddParams {
            path: r"C:\work\demo".into(),
            name: None,
        }),
        Request::ProjectList,
        Request::SessionSpawn(SessionSpawnParams {
            project_id: sid(),
            agent_profile: Some("fake".into()),
            kind: SessionKind::Agent,
            tab_id: Some(sid()),
            title: Some("t".into()),
            cols: 80,
            rows: 24,
        }),
        Request::ProjectUpdate(ProjectUpdateParams {
            project_id: sid(),
            jira_keys: None,
            name: Some("demo".into()),
            default_agent: Some(None),
            default_shell: None,
            run_commands: Some(vec![RunCommand {
                name: "BE".into(),
                dir: "api".into(),
                command: "uv run main.py".into(),
                env_file: Some(".env".into()),
                env: vec![("ENVIRONMENT".into(), "dev".into())],
            }]),
            run_autostart: Some(true),
            color: Some(Some("teal".into())),
            pinned: Some(true),
        }),
        Request::ProjectRunStart(RunStartParams {
            project_id: sid(),
            name: Some("BE".into()),
            cols: 100,
            rows: 20,
        }),
        Request::ProjectRunRestart(RunStartParams {
            project_id: sid(),
            name: None,
            cols: 80,
            rows: 24,
        }),
        Request::ProjectRunStop(RunStopParams {
            project_id: sid(),
            name: None,
        }),
        Request::ProjectArchive(ProjectRef { project_id: sid() }),
        Request::TabList(TabListParams { project_id: sid() }),
        Request::TabCreate(TabCreateParams {
            project_id: sid(),
            title: "Agents".into(),
            position: Some(1.5),
        }),
        Request::TabRename(TabRenameParams {
            tab_id: sid(),
            title: "Shells".into(),
        }),
        Request::TabClose(TabRef { tab_id: sid() }),
        Request::TabMove(TabMoveParams {
            tab_id: sid(),
            position: 0.5,
        }),
        Request::TabPin(TabPinParams {
            tab_id: sid(),
            pinned: true,
        }),
        Request::TabSetLayout(TabSetLayoutParams {
            tab_id: sid(),
            layout: json!({"split": "vertical", "ratio": 0.5, "children": [
                {"session": sid().to_string()}, {"session": sid().to_string()}
            ]}),
        }),
        Request::TaskList(TaskListParams {
            project_id: Some(sid()),
            status: Some(TaskStatus::NeedsInput),
        }),
        Request::TaskCreate(TaskCreateParams {
            project_id: sid(),
            title: "Fix login".into(),
            prompt: "make it work".into(),
            agent_profile: Some("claude".into()),
            position: None,
            worktree: true,
        }),
        Request::TaskUpdate(TaskUpdateParams {
            task_id: sid(),
            title: None,
            prompt: Some("".into()),
            agent_profile: Some(None),
            worktree: Some(false),
        }),
        Request::TaskMove(TaskMoveParams {
            task_id: sid(),
            status: TaskStatus::Review,
            position: 2.5,
        }),
        Request::TaskDelete(TaskRef { task_id: sid() }),
        Request::TaskRun(TaskRunParams {
            task_id: sid(),
            cols: 120,
            rows: 40,
        }),
        Request::TaskUndoPreview(TaskRef { task_id: sid() }),
        Request::TaskUndo(TaskUndoParams {
            task_id: sid(),
            confirm: true,
            created: vec!["new.txt".into()],
            fingerprint: "00ff".into(),
        }),
        Request::TaskMerge(TaskConfirmParams {
            task_id: sid(),
            confirm: true,
        }),
        Request::TaskRemoveWorktree(TaskConfirmParams {
            task_id: sid(),
            confirm: false,
        }),
        Request::TaskFanout(TaskFanoutParams {
            task_id: sid(),
            agent_profiles: vec!["claude".into(), "codex".into()],
            cols: 120,
            rows: 30,
        }),
        Request::SessionList(SessionListParams::default()),
        Request::SessionAttach(SessionRef { session_id: sid() }),
        Request::SessionDetach(SessionRef { session_id: sid() }),
        Request::SessionHistory(SessionRef { session_id: sid() }),
        Request::SessionRead(SessionReadParams {
            session_id: sid(),
            lines: 8,
        }),
        Request::SessionInput(SessionInputParams {
            session_id: sid(),
            data: vec![0x1b, b'[', b'A', 0, 0xff],
        }),
        Request::SessionSend(SessionSendParams {
            session_id: sid(),
            text: "y".into(),
            submit: true,
        }),
        Request::SessionUnqueue(SessionUnqueueParams {
            session_id: sid(),
            send_id: sid(),
        }),
        Request::AttachmentSave(AttachmentSaveParams {
            session_id: sid(),
            ext: "png".into(),
            data: vec![0x89, b'P', b'N', b'G'],
        }),
        Request::TurnList(SessionRef { session_id: sid() }),
        Request::SessionFiles(SessionFilesParams {
            project_id: sid(),
            since_ms: 5,
        }),
        Request::SessionUsage(ProjectRef { project_id: sid() }),
        Request::SessionPorts(ProjectRef { project_id: sid() }),
        Request::SessionLinkPlan(SessionLinkPlanParams {
            session_id: sid(),
            path: Some("docs/PLAN.md".into()),
        }),
        Request::SessionLinkPlan(SessionLinkPlanParams {
            session_id: sid(),
            path: None,
        }),
        Request::SessionPlan(ProjectRef { project_id: sid() }),
        Request::TurnRestorePreview(TurnRef {
            session_id: sid(),
            n: 2,
        }),
        Request::TurnRestore(TurnRestoreParams {
            session_id: sid(),
            n: 2,
            confirm: true,
            created: vec!["new.txt".into()],
            fingerprint: "00ff".into(),
        }),
        Request::SessionWait(SessionWaitParams {
            session_id: sid(),
            until: vec![SessionState::Idle, SessionState::Done],
            timeout_ms: Some(5_000),
        }),
        Request::SessionKill(SessionKillParams {
            session_id: sid(),
            force: true,
        }),
        Request::SessionRemove(SessionRef { session_id: sid() }),
        Request::SessionRename(SessionRenameParams {
            session_id: sid(),
            title: Some("Fix login".into()),
        }),
        Request::SessionPin(SessionPinParams {
            session_id: sid(),
            pinned: true,
        }),
        Request::SessionMove(SessionMoveParams {
            session_id: sid(),
            position: 1.5,
        }),
        Request::SessionSetColor(SessionSetColorParams {
            session_id: sid(),
            color: Some("teal".into()),
        }),
        Request::HookReport(HookReportParams {
            mkrew_session_id: sid(),
            agent: "claude".into(),
            event: "Notification".into(),
            payload: json!({"session_id": "abc"}),
        }),
        Request::AgentProfiles,
        Request::DocList(DocListParams { project_id: sid() }),
        Request::DocRead(DocReadParams {
            project_id: sid(),
            file_path: "docs/plan.md".into(),
        }),
        Request::CommentList(CommentListParams {
            project_id: sid(),
            file_path: Some("docs/plan.md".into()),
            context: Some(CommentContext::Diff),
        }),
        Request::CommentCreate(CommentCreateParams {
            project_id: sid(),
            file_path: "docs/plan.md".into(),
            start_line: 12,
            end_line: 14,
            quote: "Use Redis for caching".into(),
            body: "Why not in-memory?".into(),
            context: CommentContext::Doc,
        }),
        Request::CommentReply(CommentReplyParams {
            comment_id: CommentId(sid()),
            body: "Switched to a HashMap".into(),
            mkrew_session_id: Some(sid()),
        }),
        Request::CommentResolve(CommentRef {
            comment_id: CommentId(sid()),
        }),
        Request::CommentReopen(CommentRef {
            comment_id: CommentId(sid()),
        }),
        Request::CommentSendToAgent(CommentSendParams {
            comment_ids: vec![CommentId(sid())],
            session_id: sid(),
        }),
    ];
    for (id, request) in requests.into_iter().enumerate() {
        round_trip(&Message::Request {
            id: id as u64,
            request,
        });
    }
}

#[test]
fn run_start_size_defaults_when_left_out() {
    let id = sid();
    let request: Request = serde_json::from_value(json!({
        "method": "project.run_start",
        "params": {"project_id": id.to_string()}
    }))
    .unwrap();
    assert_eq!(
        request,
        Request::ProjectRunStart(RunStartParams {
            project_id: id,
            name: None,
            cols: DEFAULT_COLS,
            rows: DEFAULT_ROWS,
        })
    );
}

#[test]
fn input_data_is_base64() {
    let request = Request::SessionInput(SessionInputParams {
        session_id: sid(),
        data: b"hi\r".to_vec(),
    });
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["params"]["data"], "aGkN");
}

#[test]
fn optional_params_default_when_missing() {
    let message: Message = serde_json::from_value(json!({
            "type": "request", "id": 3, "method": "session.spawn",
            "params": {"project_id": sid().to_string(), "agent_profile": "shell", "cols": 80, "rows": 24}
        }))
        .unwrap();
    let Message::Request {
        request: Request::SessionSpawn(params),
        ..
    } = message
    else {
        panic!("wrong variant: {message:?}");
    };
    assert_eq!(params.kind, SessionKind::Agent);
    assert_eq!(params.title, None);
    assert_eq!(params.agent_profile.as_deref(), Some("shell"));
}

#[test]
fn session_read_lines_default_to_20() {
    let request: Request = serde_json::from_value(json!({
        "method": "session.read", "params": {"session_id": sid().to_string()}
    }))
    .unwrap();
    assert_eq!(
        request,
        Request::SessionRead(SessionReadParams {
            session_id: sid(),
            lines: 20
        })
    );
}

#[test]
fn project_update_tells_missing_from_null() {
    let parse = |params: serde_json::Value| -> ProjectUpdateParams {
        serde_json::from_value(params).unwrap()
    };
    let id = sid().to_string();
    assert_eq!(parse(json!({"project_id": id})).default_agent, None);
    assert_eq!(
        parse(json!({"project_id": id, "default_agent": null})).default_agent,
        Some(None)
    );
    assert_eq!(
        parse(json!({"project_id": id, "default_agent": "claude"})).default_agent,
        Some(Some("claude".into()))
    );
    assert_eq!(parse(json!({"project_id": id})).default_shell, None);
    assert_eq!(
        parse(json!({"project_id": id, "default_shell": null})).default_shell,
        Some(None)
    );
    assert_eq!(
        parse(json!({"project_id": id, "default_shell": "pwsh"})).default_shell,
        Some(Some("pwsh".into()))
    );
    let autostart = parse(json!({"project_id": id, "run_autostart": true}));
    assert_eq!(autostart.run_autostart, Some(true));
    assert_eq!(autostart.run_commands, None);
    assert_eq!(
        parse(json!({"project_id": id, "run_commands": []})).run_commands,
        Some(Vec::new())
    );
    assert_eq!(parse(json!({"project_id": id})).jira_keys, None);
    assert_eq!(
        parse(json!({"project_id": id, "jira_keys": []})).jira_keys,
        Some(Vec::new())
    );
    assert_eq!(
        parse(json!({"project_id": id, "jira_keys": ["PROJ"]})).jira_keys,
        Some(vec!["PROJ".to_owned()])
    );
    assert_eq!(parse(json!({"project_id": id})).color, None);
    assert_eq!(
        parse(json!({"project_id": id, "color": null})).color,
        Some(None)
    );
    assert_eq!(
        parse(json!({"project_id": id, "color": "teal"})).color,
        Some(Some("teal".into()))
    );
    // A pin is a plain bool: absent = unchanged, `false` = unpin.
    assert_eq!(parse(json!({"project_id": id})).pinned, None);
    assert_eq!(
        parse(json!({"project_id": id, "pinned": false})).pinned,
        Some(false)
    );
    // Unchanged fields are left out on the wire, a cleared one is `null`.
    let keep = ProjectUpdateParams {
        project_id: sid(),
        jira_keys: None,
        name: None,
        default_agent: None,
        default_shell: None,
        run_commands: None,
        run_autostart: None,
        color: None,
        pinned: None,
    };
    assert_eq!(
        serde_json::to_value(&keep).unwrap(),
        json!({"project_id": id})
    );
    let clear = ProjectUpdateParams {
        default_agent: Some(None),
        ..keep
    };
    assert_eq!(
        serde_json::to_value(&clear).unwrap(),
        json!({"project_id": id, "default_agent": null})
    );
}

#[test]
fn hook_report_payload_defaults_to_null() {
    let request: Request = serde_json::from_value(json!({
        "method": "hook.report",
        "params": {"mkrew_session_id": sid().to_string(), "agent": "fake", "event": "done"}
    }))
    .unwrap();
    let Request::HookReport(params) = request else {
        panic!("wrong variant: {request:?}");
    };
    assert_eq!(params.payload, serde_json::Value::Null);
}

#[test]
fn unknown_method_is_rejected() {
    let result = serde_json::from_value::<Message>(
        json!({"type": "request", "id": 1, "method": "nope.nope"}),
    );
    assert!(result.is_err());
}

#[test]
fn response_result_and_error_shapes() {
    let ok = Message::Response {
        id: 7,
        outcome: Outcome::ok(&SessionSpawnResult { session_id: sid() }).unwrap(),
    };
    assert_eq!(
        round_trip(&ok),
        json!({"type": "response", "id": 7, "result": {"session_id": sid().to_string()}})
    );

    let unit = Message::Response {
        id: 9,
        outcome: Outcome::ok(&()).unwrap(),
    };
    assert_eq!(
        round_trip(&unit),
        json!({"type": "response", "id": 9, "result": null})
    );

    let err = Message::Response {
        id: 8,
        outcome: Outcome::error(ErrorCode::VersionMismatch, "daemon speaks 1, client 2"),
    };
    assert_eq!(
        round_trip(&err),
        json!({
            "type": "response", "id": 8,
            "error": {"code": "version_mismatch", "message": "daemon speaks 1, client 2"}
        })
    );
}

#[test]
fn into_result_decodes_or_surfaces_daemon_error() {
    let hello = HelloResult {
        daemon_version: "0.1.0".into(),
        protocol_version: 1,
    };
    let decoded: HelloResult = Outcome::ok(&hello).unwrap().into_result().unwrap();
    assert_eq!(decoded, hello);

    let err = Outcome::error(ErrorCode::NotFound, "gone")
        .into_result::<HelloResult>()
        .unwrap_err();
    assert!(matches!(
        err,
        ResponseError::Daemon(ErrorBody {
            code: ErrorCode::NotFound,
            ..
        })
    ));
}

#[test]
fn agent_profiles_result_round_trips() {
    let result = AgentProfilesResult {
        profiles: vec![AgentProfileInfo {
            id: "fake".into(),
            name: "Fake agent".into(),
            kind: SessionKind::Agent,
            command: "mkrew-fakeagent".into(),
            icon: Some("ƒ".into()),
            installed: true,
            ask: true,
            ask_web: false,
        }],
        errors: vec![ProfileErrorInfo {
            file: "agents/broken.toml".into(),
            line: Some(2),
            column: Some(1),
            message: "unknown field `promt_mode`".into(),
        }],
    };
    let decoded: AgentProfilesResult = Outcome::ok(&result).unwrap().into_result().unwrap();
    assert_eq!(decoded, result);
}

#[test]
fn agent_profile_info_without_icon_decodes() {
    let info: AgentProfileInfo = serde_json::from_value(serde_json::json!({
        "id": "x", "name": "X", "kind": "agent", "command": "x"
    }))
    .unwrap();
    assert_eq!(info.icon, None);
}

#[test]
fn snapshot_wire_shape() {
    let snapshot = ScreenSnapshot {
        cols: 4,
        rows: 2,
        lines: vec![
            vec![CellRun {
                text: "ok".into(),
                fg: TermColor::Indexed(1),
                bg: TermColor::Rgb([1, 2, 3]),
                flags: CellRun::BOLD,
            }],
            vec![],
        ],
        history: vec![vec![CellRun {
            text: "old".into(),
            fg: TermColor::Default,
            bg: TermColor::Default,
            flags: 0,
        }]],
        cursor: CursorSnapshot {
            row: 1,
            col: 0,
            visible: true,
        },
        modes: TermModes {
            bracketed_paste: true,
            ..TermModes::default()
        },
    };
    let value = round_trip(&Message::Event(Event::SessionSnapshot(
        SessionSnapshotEvent {
            session_id: sid(),
            snapshot: snapshot.clone(),
        },
    )));
    assert_eq!(value["event"], "session.snapshot");
    assert_eq!(
        value["data"]["snapshot"]["lines"][0][0],
        json!({"text": "ok", "fg": {"indexed": 1}, "bg": {"rgb": [1, 2, 3]}, "flags": 1})
    );
    assert_eq!(
        value["data"]["snapshot"]["history"][0][0],
        json!({"text": "old", "fg": "default", "bg": "default", "flags": 0})
    );
    let attach = SessionAttachResult {
        state: SessionState::Idle,
        snapshot,
    };
    let decoded: SessionAttachResult = Outcome::ok(&attach).unwrap().into_result().unwrap();
    assert_eq!(decoded, attach);
    assert_eq!(
        serde_json::to_value(TermColor::Default).unwrap(),
        json!("default")
    );
}

#[test]
fn event_wire_shape_matches_spec() {
    let message = Message::Event(Event::SessionState(SessionStateEvent {
        session_id: sid(),
        state: SessionState::NeedsInput,
        source: StateSource::Hook,
        changed_at: 5,
    }));
    assert_eq!(
        round_trip(&message),
        json!({
            "type": "event", "event": "session.state",
            "data": {
                "session_id": sid().to_string(), "state": "needs_input",
                "source": "hook", "changed_at": 5
            }
        })
    );
    round_trip(&Message::Event(Event::SessionExited(SessionExitedEvent {
        session_id: sid(),
        exit_code: Some(0),
    })));
    round_trip(&Message::Event(Event::ProjectUpdated(Project {
        id: sid(),
        jira_keys: vec!["PROJ".into()],
        name: "demo".into(),
        path: "/tmp/demo".into(),
        run_commands: vec![RunCommand {
            name: "FE".into(),
            dir: ".".into(),
            command: "npm run dev".into(),
            env_file: None,
            env: Vec::new(),
        }],
        run_autostart: true,
        default_agent: Some("claude".into()),
        default_shell: Some("pwsh".into()),
        review_globs: Some(vec!["docs/**/*.md".into()]),
        created_at: 1,
        archived: false,
        base_ref: Some("develop".into()),
        color: Some("teal".into()),
        pinned: true,
    })));
    let tab = round_trip(&Message::Event(Event::TabUpdated(TerminalTab {
        id: sid(),
        project_id: sid(),
        title: "Agents".into(),
        position: 1.0,
        layout: serde_json::Value::Null,
        pinned: true,
    })));
    assert_eq!(tab["event"], "tab.updated");
    assert_eq!(tab["data"]["layout"], serde_json::Value::Null);
    assert_eq!(tab["data"]["pinned"], true);
    let clicked = round_trip(&Message::Event(Event::NotificationClicked(
        NotificationClickedEvent {
            session_id: sid(),
            project_id: sid(),
        },
    )));
    assert_eq!(clicked["event"], "notification.clicked");
    assert_eq!(clicked["data"]["session_id"], sid().to_string());
    let conflict = round_trip(&Message::Event(Event::ConflictDetected(
        ConflictDetectedEvent {
            project_id: sid(),
            path: "src/a.rs".into(),
            session_ids: vec![sid(), sid()],
            task_ids: vec![sid()],
        },
    )));
    assert_eq!(conflict["event"], "conflict.detected");
    assert_eq!(conflict["data"]["path"], "src/a.rs");
    assert_eq!(
        conflict["data"]["session_ids"].as_array().map(Vec::len),
        Some(2)
    );
    let task = round_trip(&Message::Event(Event::TaskUpdated(Task {
        id: sid(),
        project_id: sid(),
        title: "Fix login".into(),
        prompt: String::new(),
        agent_profile: None,
        status: TaskStatus::NeedsInput,
        position: 1.5,
        checkpoint: None,
        error_flag: true,
        worktree: true,
        worktree_path: Some("/data/mkrew/worktrees/p/t".into()),
        branch: Some("mkrew/fix-login-01jx".into()),
        fanout_of: Some(sid()),
        created_at: 1,
        updated_at: 2,
    })));
    assert_eq!(task["event"], "task.updated");
    assert_eq!(task["data"]["worktree"], true);
    assert_eq!(task["data"]["branch"], "mkrew/fix-login-01jx");
    assert_eq!(task["data"]["status"], "needs_input");
    assert_eq!(task["data"]["agent_profile"], serde_json::Value::Null);
    let deleted = round_trip(&Message::Event(Event::TaskDeleted(TaskDeletedEvent {
        task_id: sid(),
        project_id: sid(),
    })));
    assert_eq!(deleted["event"], "task.deleted");
    let doc = round_trip(&Message::Event(Event::DocChanged(DocChangedEvent {
        project_id: sid(),
        file_path: "docs/plan.md".into(),
        hash: None,
    })));
    assert_eq!(doc["event"], "doc.changed");
    assert_eq!(doc["data"]["hash"], serde_json::Value::Null);
    let turn = round_trip(&Message::Event(Event::SessionTurn(SessionTurnEvent {
        session_id: sid(),
        n: 2,
        started_at: 10,
        ended_at: 20,
        files: vec![TurnFile {
            path: "src/a.rs".into(),
            added: None,
            removed: None,
        }],
    })));
    assert_eq!(turn["event"], "session.turn");
    assert_eq!(turn["data"]["files"], json!([{ "path": "src/a.rs" }]));
    let queue = round_trip(&Message::Event(Event::SessionQueue(SessionQueueEvent {
        session_id: sid(),
        items: vec![QueuedSend {
            send_id: sid(),
            text: "fix it".into(),
        }],
    })));
    assert_eq!(queue["event"], "session.queue");
    assert_eq!(queue["data"]["items"][0]["text"], "fix it");
    let edited = round_trip(&Message::Event(Event::SessionEdited(SessionEditedEvent {
        session_id: sid(),
        path: "src/a.rs".into(),
        line: None,
    })));
    assert_eq!(edited["event"], "session.edited");
    assert_eq!(
        edited["data"],
        json!({ "session_id": sid(), "path": "src/a.rs" })
    );
    round_trip(&Message::Event(Event::SessionPorts(SessionPortsEvent {
        session_id: sid(),
        ports: vec![8080, 5180],
    })));
    let updated = round_trip(&Message::Event(Event::SessionUpdated(Box::new(Session {
        id: sid(),
        task_id: None,
        project_id: sid(),
        agent_profile: "claude".into(),
        kind: SessionKind::Agent,
        title: Some("Fix login".into()),
        terminal_title: None,
        color: Some("teal".into()),
        native_session_id: None,
        state: SessionState::Working,
        state_source: StateSource::Screen,
        state_changed_at: 1,
        exit_code: None,
        cols: 80,
        rows: 24,
        started_at: 1,
        ended_at: None,
        pinned: true,
        position: 2.5,
        plan_path: None,
    }))));
    let titled = round_trip(&Message::Event(Event::SessionTerminalTitle(
        SessionTerminalTitleEvent {
            session_id: sid(),
            title: Some("Fix login".into()),
        },
    )));
    assert_eq!(titled["event"], "session.terminal_title");
    assert_eq!(updated["event"], "session.updated");
    assert_eq!(updated["data"]["title"], "Fix login");
    assert_eq!(updated["data"]["pinned"], true);
    assert_eq!(updated["data"]["position"], 2.5);
    // Absent on the wire when unset.
    assert!(updated["data"].get("terminal_title").is_none());
    assert_eq!(updated["data"]["color"], "teal");
    // A plan link is absent on the wire when unset.
    assert!(updated["data"].get("plan_path").is_none());
    let plan = round_trip(&Message::Event(Event::SessionPlan(SessionPlanEvent {
        session_id: sid(),
        name: "PLAN".into(),
        done: 1,
        total: 2,
        items: vec![
            PlanItem {
                done: true,
                text: "a".into(),
            },
            PlanItem {
                done: false,
                text: "b".into(),
            },
        ],
        error: None,
    })));
    assert_eq!(plan["event"], "session.plan");
    assert_eq!(
        plan["data"],
        json!({
            "session_id": sid(), "name": "PLAN", "done": 1, "total": 2,
            "items": [{ "done": true, "text": "a" }, { "done": false, "text": "b" }]
        })
    );
    // An older daemon's session has no color: it reads as none.
    let mut old = updated["data"].clone();
    old.as_object_mut().unwrap().remove("color");
    assert_eq!(serde_json::from_value::<Session>(old).unwrap().color, None);
    let usage = round_trip(&Message::Event(Event::SessionUsage(usage_event())));
    assert_eq!(usage["event"], "session.usage");
    assert_eq!(
        usage["data"],
        json!({
            "session_id": sid(), "input": 10, "output": 2, "cache_read": 5,
            "cache_write": 1, "context": 7, "model": "claude-opus-5"
        })
    );
    // Stored by a daemon before the cache split (SPEC §20E.3, ADR 0035): no cache
    // fields, no model. It must still read, with the new fields at their defaults.
    let old: SessionUsageEvent = serde_json::from_value(json!({
        "session_id": sid(), "input": 10, "output": 2, "context": 7, "context_window": 200000
    }))
    .unwrap();
    assert_eq!(
        (
            old.cache_read,
            old.cache_write,
            old.model,
            old.context_window
        ),
        (0, 0, None, Some(200_000))
    );
    let with_path = round_trip(&Message::Event(Event::SessionUsage(SessionUsageEvent {
        transcript: Some("C:\\Users\\a\\.claude\\projects\\p\\s1.jsonl".into()),
        ..usage_event()
    })));
    assert_eq!(
        with_path["data"]["transcript"],
        "C:\\Users\\a\\.claude\\projects\\p\\s1.jsonl"
    );
    // A `session.usage` stored before the field existed still reads.
    let old: SessionUsageEvent = serde_json::from_value(
        json!({ "session_id": sid(), "input": 1, "output": 2, "context": 3 }),
    )
    .unwrap();
    assert_eq!(old.transcript, None);
}

fn usage_event() -> SessionUsageEvent {
    SessionUsageEvent {
        session_id: sid(),
        input: 10,
        output: 2,
        cache_read: 5,
        cache_write: 1,
        context: 7,
        context_window: None,
        model: Some("claude-opus-5".into()),
        transcript: None,
    }
}

#[test]
fn comment_ids_carry_the_c_prefix() {
    let id = CommentId(sid());
    assert_eq!(id.to_string(), format!("c_{}", sid()));
    assert_eq!(serde_json::to_value(id).unwrap(), json!(id.to_string()));
    // Agents may drop the prefix; both forms parse to the same id.
    assert_eq!(id.to_string().parse::<CommentId>().unwrap(), id);
    assert_eq!(sid().to_string().parse::<CommentId>().unwrap(), id);
    assert!("c_nope".parse::<CommentId>().is_err());
    assert!(serde_json::from_value::<CommentId>(json!("x")).is_err());
}

#[test]
fn comment_wire_shapes() {
    let comment = DocComment {
        id: CommentId(sid()),
        project_id: sid(),
        file_path: "docs/plan.md".into(),
        context: CommentContext::Diff,
        parent_id: None,
        author: DocComment::USER.into(),
        body: "Add a rollback step.".into(),
        start_line: Some(40),
        end_line: Some(40),
        quote: Some("M3: deploy".into()),
        status: CommentStatus::Addressed,
        sent_to: Some(sid()),
        sent_snapshot: Some("# Plan\n".into()),
        created_at: 1,
        updated_at: 2,
    };
    let value = round_trip(&Message::Event(Event::CommentUpdated(Box::new(
        comment.clone(),
    ))));
    assert_eq!(value["event"], "comment.updated");
    assert_eq!(value["data"]["id"], format!("c_{}", sid()));
    assert_eq!(value["data"]["status"], "addressed");
    assert_eq!(value["data"]["parent_id"], serde_json::Value::Null);
    let decoded: Vec<DocComment> = Outcome::ok(&vec![comment.clone()])
        .unwrap()
        .into_result()
        .unwrap();
    assert_eq!(decoded, vec![comment.clone()]);

    let id = sid().to_string();
    let list: CommentListParams = serde_json::from_value(json!({"project_id": id})).unwrap();
    assert_eq!(list.file_path, None);
    let reply: CommentReplyParams =
        serde_json::from_value(json!({"comment_id": format!("c_{id}"), "body": "ok"})).unwrap();
    assert_eq!(reply.mkrew_session_id, None);
    assert_eq!(reply.comment_id, CommentId(sid()));

    // `context` defaults to `doc` everywhere, so pre-M7 clients keep working.
    assert_eq!(list.context, None);
    assert_eq!(value["data"]["context"], "diff");
    let create: CommentCreateParams = serde_json::from_value(json!({
        "project_id": id, "file_path": "a.md", "start_line": 1, "end_line": 1,
        "quote": "", "body": "x",
    }))
    .unwrap();
    assert_eq!(create.context, CommentContext::Doc);
    let mut old = serde_json::to_value(&comment).unwrap();
    old.as_object_mut().unwrap().remove("context");
    let old: DocComment = serde_json::from_value(old).unwrap();
    assert_eq!(old.context, CommentContext::Doc);
}

#[test]
fn task_params_default_and_tell_missing_from_null() {
    let id = sid().to_string();
    let create: TaskCreateParams =
        serde_json::from_value(json!({"project_id": id, "title": "t"})).unwrap();
    assert_eq!(create.prompt, "");
    assert_eq!(create.agent_profile, None);
    assert_eq!(create.position, None);
    assert!(!create.worktree);
    let list: TaskListParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(list, TaskListParams::default());

    let parse =
        |params: serde_json::Value| -> TaskUpdateParams { serde_json::from_value(params).unwrap() };
    assert_eq!(parse(json!({"task_id": id})).agent_profile, None);
    assert_eq!(
        parse(json!({"task_id": id, "agent_profile": null})).agent_profile,
        Some(None)
    );
    let keep = parse(json!({"task_id": id}));
    assert_eq!(keep.worktree, None);
    assert_eq!(serde_json::to_value(&keep).unwrap(), json!({"task_id": id}));
    assert_eq!(
        parse(json!({"task_id": id, "worktree": true})).worktree,
        Some(true)
    );

    // Undo is refused unless the client says otherwise.
    let undo: TaskUndoParams = serde_json::from_value(json!({"task_id": id})).unwrap();
    assert!(!undo.confirm);
    assert!(undo.created.is_empty() && undo.fingerprint.is_empty());
    let merge: TaskConfirmParams = serde_json::from_value(json!({"task_id": id})).unwrap();
    assert!(!merge.confirm);
}

#[test]
fn daemon_status_carries_the_phone_access_state() {
    let status = DaemonStatusResult {
        daemon_version: "0.1.4".into(),
        protocol_version: crate::PROTOCOL_VERSION,
        pid: 7,
        started_at: 1,
        sessions: 0,
        settings_error: None,
        web: WebStatus {
            enabled: true,
            running: false,
            error: Some("Only one usage of each socket address is normally permitted".into()),
        },
    };
    let value = serde_json::to_value(&status).unwrap();
    assert_eq!(
        value["web"],
        json!({
            "enabled": true,
            "running": false,
            "error": "Only one usage of each socket address is normally permitted"
        })
    );
    assert_eq!(
        serde_json::from_value::<DaemonStatusResult>(value).unwrap(),
        status
    );

    // No error: the key is absent, like `settings_error`.
    assert_eq!(
        serde_json::to_value(WebStatus::default()).unwrap(),
        json!({ "enabled": false, "running": false })
    );

    // A reply without `web` (a daemon that predates it) still decodes, as "off".
    let old = json!({
        "daemon_version": "0.1.3", "protocol_version": 39, "pid": 7, "started_at": 1, "sessions": 0
    });
    let decoded: DaemonStatusResult = serde_json::from_value(old).unwrap();
    assert_eq!(decoded.web, WebStatus::default());
}
