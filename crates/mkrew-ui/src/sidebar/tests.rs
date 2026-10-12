use mkrew_protocol::{SessionExitedEvent, SessionStateEvent};

use super::*;

fn project(name: &str) -> Project {
    Project {
        id: Ulid::new(),
        name: name.to_owned(),
        path: format!("C:\\src\\{name}"),
        run_commands: Vec::new(),
        run_autostart: false,
        default_agent: None,
        default_shell: None,
        review_globs: None,
        created_at: 0,
        archived: false,
        base_ref: None,
        color: None,
        pinned: false,
    }
}

#[test]
fn the_sidebar_width_follows_the_pointer_within_the_limits() {
    assert_eq!(saved_width(280), 280.0);
    assert_eq!(saved_width(0), theme::SIDEBAR_MIN_WIDTH);
    assert_eq!(saved_width(65_535), theme::SIDEBAR_MAX_WIDTH);
    // Flush with the window's left edge: width = pointer.
    assert_eq!(width_from_pointer(1200.0, 320.0), 320.0);
    assert_eq!(width_from_pointer(1200.0, 10.0), theme::SIDEBAR_MIN_WIDTH);
    assert_eq!(width_from_pointer(1200.0, 900.0), theme::SIDEBAR_MAX_WIDTH);
    // Never more than half of a narrow window (but never below the minimum).
    assert_eq!(width_from_pointer(700.0, 600.0), 350.0);
    assert_eq!(width_from_pointer(300.0, 600.0), theme::SIDEBAR_MIN_WIDTH);
}

fn session(project: &Project, started_at: i64, state: SessionState) -> Session {
    Session {
        id: Ulid::new(),
        task_id: None,
        project_id: project.id,
        agent_profile: "claude".to_owned(),
        kind: SessionKind::Agent,
        title: None,
        terminal_title: None,
        color: None,
        native_session_id: None,
        state,
        state_source: StateSource::Daemon,
        state_changed_at: started_at,
        exit_code: None,
        cols: 80,
        rows: 24,
        started_at,
        ended_at: None,
        pinned: false,
        plan_path: None,
    }
}

fn workspace(row: &Row) -> &WorkspaceRow {
    match row {
        Row::Workspace(row) => row,
        other => panic!("expected a workspace row, got {other:?}"),
    }
}

fn session_ids(rows: &[Row]) -> Vec<Ulid> {
    rows.iter()
        .filter_map(|row| match row {
            Row::Session(row) => Some(row.session_id),
            _ => None,
        })
        .collect()
}

#[test]
fn every_state_has_an_icon_and_text() {
    let states = [
        SessionState::Starting,
        SessionState::Working,
        SessionState::NeedsInput,
        SessionState::Idle,
        SessionState::Done,
        SessionState::Exited,
        SessionState::Unknown,
    ];
    let icons: HashSet<&str> = states.iter().map(|s| status_display(*s).icon).collect();
    assert_eq!(icons.len(), states.len(), "icons must be distinct");
    for state in states {
        let display = status_display(state);
        assert!(!display.icon.is_empty() && !display.label.is_empty());
    }
    assert_eq!(status_display(SessionState::NeedsInput).icon, "?");
    assert_eq!(status_display(SessionState::Done).icon, "✓");
    let theme = Theme::dark();
    assert_eq!(
        status_color(&theme, SessionState::NeedsInput),
        theme.status_needs_input
    );
}

#[test]
fn agent_badge_uses_the_profile_icon_else_the_first_letter() {
    assert_eq!(agent_badge(Some("π"), "omp"), "π");
    assert_eq!(agent_badge(None, "claude"), "C");
    // Uppercasing works beyond ASCII.
    assert_eq!(agent_badge(None, "ølm"), "Ø");
}

#[test]
fn first_load_expands_workspaces_with_live_sessions_newest_first() {
    let (api, web) = (project("api"), project("web"));
    let old = session(&api, 1, SessionState::Idle);
    let new = session(&api, 2, SessionState::Working);
    let exited = session(&web, 3, SessionState::Exited);
    let mut sidebar = Sidebar::default();
    sidebar.load(
        vec![api.clone(), web.clone()],
        vec![old.clone(), new.clone(), exited],
    );

    let rows = sidebar.rows();
    assert_eq!(rows.len(), 4, "api + 2 sessions + collapsed web: {rows:?}");
    assert!(workspace(&rows[0]).expanded);
    assert_eq!(session_ids(&rows), vec![new.id, old.id]);
    let web_row = workspace(&rows[3]);
    assert_eq!(web_row.name, "web");
    assert!(!web_row.expanded);

    // Later loads keep what the user expanded or collapsed.
    sidebar.toggle(api.id);
    sidebar.load(vec![api, web], vec![old, new]);
    assert!(!workspace(&sidebar.rows()[0]).expanded);
}

#[test]
fn a_renamed_session_shows_its_new_title_and_an_unknown_one_refreshes() {
    let api = project("api");
    let live = session(&api, 1, SessionState::Working);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api.clone()], vec![live.clone()]);
    let renamed = Session {
        title: Some("Fix login".into()),
        ..live
    };
    assert_eq!(
        sidebar.apply(&Event::SessionUpdated(Box::new(renamed))),
        Change::Redraw
    );
    let Row::Session(row) = &sidebar.rows()[1] else {
        panic!("expected the session row");
    };
    assert_eq!(row.title.as_deref(), Some("Fix login"));
    let stranger = session(&api, 2, SessionState::Working);
    assert_eq!(
        sidebar.apply(&Event::SessionUpdated(Box::new(stranger))),
        Change::Refresh
    );
}

#[test]
fn a_rename_reply_sets_only_the_title() {
    let api = project("api");
    let live = session(&api, 1, SessionState::Working);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api.clone()], vec![live.clone()]);
    // The state moved on after the reply's snapshot was taken.
    sidebar.apply(&Event::SessionExited(mkrew_protocol::SessionExitedEvent {
        session_id: live.id,
        exit_code: Some(3),
    }));
    sidebar.set_title(live.id, Some("Fix login".into()));
    let session = &sidebar.sessions()[0];
    assert_eq!(session.title.as_deref(), Some("Fix login"));
    assert_eq!(session.state, SessionState::Exited);
    assert_eq!(session.exit_code, Some(3));
}

#[test]
fn a_color_reply_sets_only_the_color() {
    let api = project("api");
    let live = session(&api, 1, SessionState::Working);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api.clone()], vec![live.clone()]);
    sidebar.apply(&Event::SessionExited(mkrew_protocol::SessionExitedEvent {
        session_id: live.id,
        exit_code: Some(0),
    }));
    sidebar.set_color(live.id, Some("teal".into()));
    let shown = &sidebar.sessions()[0];
    assert_eq!(shown.color.as_deref(), Some("teal"));
    assert_eq!(
        shown.state,
        SessionState::Exited,
        "the reply's state is not applied"
    );
}

#[test]
fn workspaces_cycle_in_sidebar_order() {
    let mut sidebar = Sidebar::default();
    assert_eq!(sidebar.workspace_at_offset(None, 1), None);
    let (a, b, c) = (project("a"), project("b"), project("c"));
    let ids = [a.id, b.id, c.id];
    sidebar.load(vec![a, b, c], Vec::new());
    assert_eq!(sidebar.workspace_at_offset(Some(ids[0]), 1), Some(ids[1]));
    assert_eq!(sidebar.workspace_at_offset(Some(ids[2]), 1), Some(ids[0]));
    assert_eq!(sidebar.workspace_at_offset(Some(ids[0]), -1), Some(ids[2]));
    assert_eq!(sidebar.workspace_at_offset(None, -1), Some(ids[0]));
    assert_eq!(
        sidebar.workspace_at_offset(Some(Ulid::new()), 1),
        Some(ids[0])
    );
}

/// Pinned workspaces lead (SPEC §10), each group in its usual (age) order; unpinning puts
/// one back where its age says, not at the head of the unpinned ones. Ctrl+Tab between
/// workspaces follows the same order.
#[test]
fn pinned_workspaces_lead_and_unpinning_restores_the_old_order() {
    let (a, b, c) = (
        Project {
            created_at: 1,
            ..project("a")
        },
        Project {
            created_at: 2,
            ..project("b")
        },
        Project {
            created_at: 3,
            ..project("c")
        },
    );
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![a.clone(), b.clone(), c.clone()], Vec::new());
    let names = |sidebar: &Sidebar| -> Vec<String> {
        sidebar
            .rows()
            .iter()
            .filter_map(|row| match row {
                Row::Workspace(row) => Some(row.name.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(names(&sidebar), ["a", "b", "c"]);

    sidebar.apply(&Event::ProjectUpdated(Project {
        pinned: true,
        ..c.clone()
    }));
    sidebar.apply(&Event::ProjectUpdated(Project {
        pinned: true,
        ..b.clone()
    }));
    assert_eq!(names(&sidebar), ["b", "c", "a"]);
    assert!(workspace(&sidebar.rows()[0]).pinned);
    assert_eq!(sidebar.workspace_at_offset(Some(c.id), 1), Some(a.id));

    sidebar.apply(&Event::ProjectUpdated(Project {
        pinned: false,
        ..b.clone()
    }));
    assert_eq!(names(&sidebar), ["c", "a", "b"]);

    // A fresh list puts pinned workspaces first too.
    let mut reloaded = Sidebar::default();
    reloaded.load(vec![a, Project { pinned: true, ..c }], Vec::new());
    assert_eq!(names(&reloaded), ["c", "a"]);
}

/// A pinned session stays above newer ones; within each group it is newest first.
#[test]
fn pinned_sessions_lead_their_workspace_newest_first() {
    let api = project("api");
    let old = session(&api, 1, SessionState::Idle);
    let mid = session(&api, 2, SessionState::Working);
    let new = session(&api, 3, SessionState::Working);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api], vec![old.clone(), mid.clone(), new.clone()]);
    assert_eq!(session_ids(&sidebar.rows()), [new.id, mid.id, old.id]);

    sidebar.apply(&Event::SessionUpdated(Box::new(Session {
        pinned: true,
        ..old.clone()
    })));
    assert_eq!(session_ids(&sidebar.rows()), [old.id, new.id, mid.id]);
    sidebar.apply(&Event::SessionUpdated(Box::new(Session {
        pinned: true,
        ..mid.clone()
    })));
    assert_eq!(session_ids(&sidebar.rows()), [mid.id, old.id, new.id]);
    let rows = sidebar.rows();
    assert!(matches!(&rows[1], Row::Session(row) if row.pinned));
    assert!(matches!(&rows[3], Row::Session(row) if !row.pinned));
}

#[test]
fn workspace_row_counts_working_and_flags_needs_input() {
    let api = project("api");
    let a = session(&api, 1, SessionState::Working);
    let b = session(&api, 2, SessionState::Working);
    let c = session(&api, 3, SessionState::Idle);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api], vec![a, b, c.clone()]);
    let row = workspace(&sidebar.rows()[0]).clone();
    assert_eq!((row.working, row.needs_input), (2, false));

    let change = sidebar.apply(&Event::SessionState(SessionStateEvent {
        session_id: c.id,
        state: SessionState::NeedsInput,
        source: StateSource::Hook,
        changed_at: 9,
    }));
    assert_eq!(change, Change::Redraw);
    let row = workspace(&sidebar.rows()[0]).clone();
    assert_eq!((row.working, row.needs_input), (2, true));
    let updated = &sidebar.sessions()[2];
    assert_eq!(
        (updated.state_source, updated.state_changed_at),
        (StateSource::Hook, 9)
    );
    // The row carries what its status hover shows.
    let shown = sidebar.rows().iter().find_map(|row| match row {
        Row::Session(row) if row.session_id == c.id => {
            Some((row.state_source, row.state_changed_at))
        }
        _ => None,
    });
    assert_eq!(shown, Some((StateSource::Hook, 9)));
}

#[test]
fn events_newer_than_the_lists_survive_a_load() {
    let api = project("api");
    let s = session(&api, 1, SessionState::Working);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api.clone()], vec![s.clone()]);

    // The state changes after `session.list` was answered, but its event is handled first.
    sidebar.begin_load();
    sidebar.apply(&Event::SessionState(SessionStateEvent {
        session_id: s.id,
        state: SessionState::NeedsInput,
        source: StateSource::Screen,
        changed_at: 2,
    }));
    sidebar.load(vec![api], vec![s]);
    assert!(workspace(&sidebar.rows()[0]).needs_input);
    assert!(sidebar.since_load.is_none());
}

/// An exited session cannot be opened, so it has no row; History, the Inbox and the pane
/// placeholder still read it from `sessions()`. A session this list never saw asks for a
/// refresh.
#[test]
fn an_exited_session_leaves_the_rows_but_stays_listed() {
    let api = project("api");
    let s = session(&api, 1, SessionState::Working);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api.clone()], vec![s.clone()]);
    assert_eq!(session_ids(&sidebar.rows()), vec![s.id]);

    let exited = Event::SessionExited(SessionExitedEvent {
        session_id: s.id,
        exit_code: Some(1),
    });
    assert_eq!(sidebar.apply(&exited), Change::Redraw);
    assert_eq!(sidebar.rows()[1], Row::NoSessions { project_id: api.id });
    let kept = &sidebar.sessions()[0];
    assert_eq!(
        (kept.state, kept.exit_code),
        (SessionState::Exited, Some(1))
    );

    let spawned = Event::SessionState(SessionStateEvent {
        session_id: Ulid::new(),
        state: SessionState::Starting,
        source: StateSource::Daemon,
        changed_at: 1,
    });
    assert_eq!(sidebar.apply(&spawned), Change::Refresh);
}

#[test]
fn added_and_updated_workspaces_show_up_and_archived_ones_leave() {
    let mut sidebar = Sidebar::default();
    sidebar.load(Vec::new(), Vec::new());
    assert!(sidebar.is_empty());

    let mut api = project("api");
    sidebar.add_project(api.clone());
    assert_eq!(
        sidebar.rows(),
        vec![
            Row::Workspace(WorkspaceRow {
                project_id: api.id,
                name: "api".to_owned(),
                path: api.path.clone(),
                expanded: true,
                working: 0,
                needs_input: false,
                summary: None,
                color: None,
                pinned: false,
            }),
            Row::NoSessions { project_id: api.id },
        ]
    );
    // The daemon's echo of the same add does not duplicate it.
    assert_eq!(
        sidebar.apply(&Event::ProjectUpdated(api.clone())),
        Change::Redraw
    );
    assert_eq!(sidebar.rows().len(), 2);

    api.name = "backend".to_owned();
    sidebar.apply(&Event::ProjectUpdated(api.clone()));
    assert_eq!(workspace(&sidebar.rows()[0]).name, "backend");

    api.archived = true;
    sidebar.apply(&Event::ProjectUpdated(api));
    assert!(sidebar.is_empty());
}

#[test]
fn the_filter_matches_workspace_agent_and_title() {
    assert!(matches("api", ["API server", "claude", ""]));
    assert!(matches("codex", ["api", "codex", ""]));
    assert!(matches("login", ["api", "claude", "Fix Login"]));
    assert!(!matches("login", ["api", "claude", ""]));
    // Every word has to hit some field, not the same one.
    assert!(matches("api login", ["api", "claude", "Fix login"]));
    assert!(!matches("api nope", ["api", "claude", "Fix login"]));
    assert!(matches("", ["api", "claude", ""]));
}

#[test]
fn a_filter_keeps_matching_sessions_and_opens_their_workspace() {
    let (api, web) = (project("api"), project("web"));
    let login = Session {
        title: Some("Fix login".into()),
        ..session(&api, 1, SessionState::Idle)
    };
    let other = session(&api, 2, SessionState::Idle);
    let codex = Session {
        agent_profile: "codex".into(),
        ..session(&web, 3, SessionState::Working)
    };
    let mut sidebar = Sidebar::default();
    sidebar.load(
        vec![api.clone(), web.clone()],
        vec![login.clone(), other, codex.clone()],
    );
    sidebar.toggle_all();
    assert!(!sidebar.any_expanded(), "everything was open, now closed");
    assert_eq!(sidebar.rows().len(), 2);

    sidebar.filter_mut().insert("login");
    let rows = sidebar.rows();
    assert_eq!(rows.len(), 2, "api and its one match: {rows:?}");
    assert!(workspace(&rows[0]).expanded, "a filter opens what it keeps");
    assert_eq!(session_ids(&rows), vec![login.id]);

    // A workspace name matches all of its sessions; so does an agent name.
    for query in ["WEB", "codex"] {
        sidebar.filter_mut().clear();
        sidebar.filter_mut().insert(query);
        assert_eq!(session_ids(&sidebar.rows()), vec![codex.id], "{query}");
    }
    sidebar.filter_mut().clear();
    sidebar.filter_mut().insert("zzz");
    assert!(sidebar.rows().is_empty());

    // Clearing the filter brings the user's own open/closed state back.
    sidebar.filter_mut().clear();
    assert!(
        sidebar
            .rows()
            .iter()
            .all(|row| !matches!(row, Row::Session(_)))
    );
    assert_eq!(sidebar.rows().len(), 2);
}

#[test]
fn chips_count_what_matches_the_query_and_narrow_the_rows() {
    let api = project("api");
    let working = session(&api, 1, SessionState::Working);
    let asking = session(&api, 2, SessionState::NeedsInput);
    let done = session(&api, 3, SessionState::Done);
    let seen = session(&api, 4, SessionState::Done);
    let idle = session(&api, 5, SessionState::Idle);
    let mut sidebar = Sidebar::default();
    sidebar.load(
        vec![api.clone()],
        vec![
            working.clone(),
            asking.clone(),
            done.clone(),
            seen.clone(),
            idle,
        ],
    );
    let mut viewed = Viewed::default();
    viewed.view(&seen);

    let counts = sidebar.view(&viewed).counts;
    assert_eq!(
        counts,
        Counts {
            all: 5,
            working: 1,
            needs_you: 2
        }
    );
    // "Needs you" is exactly what the Inbox lists.
    let listed = crate::inbox::entries(sidebar.projects(), sidebar.sessions(), &viewed);
    assert_eq!(counts.needs_you, listed.len());

    sidebar.set_chip(Chip::NeedsYou);
    assert_eq!(
        session_ids(&sidebar.view(&viewed).rows),
        vec![done.id, asking.id]
    );
    sidebar.set_chip(Chip::Working);
    let view = sidebar.view(&viewed);
    assert_eq!(session_ids(&view.rows), vec![working.id]);
    assert_eq!(
        view.counts.all, 5,
        "chips count, they do not shrink their own numbers"
    );

    // The query narrows the counts too.
    sidebar.set_chip(Chip::All);
    sidebar.filter_mut().insert("nothing like it");
    assert_eq!(sidebar.view(&viewed).counts, Counts::default());
}

#[test]
fn the_workspace_summary_names_what_is_going_on() {
    assert_eq!(workspace_summary(1, 0, 0).as_deref(), Some("1 working"));
    assert_eq!(workspace_summary(0, 0, 2).as_deref(), Some("2 idle"));
    assert_eq!(
        workspace_summary(1, 1, 2).as_deref(),
        Some("1 working · 1 needs you · 2 idle")
    );
    assert_eq!(workspace_summary(0, 0, 0), None);
}

#[test]
fn a_workspace_row_carries_its_summary_and_a_session_row_only_a_time() {
    let api = project("api");
    let mut sidebar = Sidebar::default();
    sidebar.load(
        vec![api.clone()],
        vec![
            session(&api, 1, SessionState::Working),
            session(&api, 2, SessionState::Idle),
            session(&api, 3, SessionState::Idle),
        ],
    );
    let rows = sidebar.rows();
    assert_eq!(
        workspace(&rows[0]).summary.as_deref(),
        Some("1 working · 2 idle")
    );

    let row = |state| SessionRow {
        session_id: Ulid::new(),
        project_id: api.id,
        agent: "claude".into(),
        title: None,
        shell: false,
        state,
        state_source: StateSource::Daemon,
        state_changed_at: 0,
        pinned: false,
    };
    // The status icon says the state; the right side is only the time (working / idle).
    assert_eq!(
        row(SessionState::Working).time_text(120_000).as_deref(),
        Some("2m")
    );
    assert_eq!(
        row(SessionState::Idle).time_text(17 * 60_000).as_deref(),
        Some("17m")
    );
    assert_eq!(row(SessionState::NeedsInput).time_text(120_000), None);
    assert_eq!(row(SessionState::Done).time_text(120_000), None);
}

#[test]
fn toggle_all_closes_when_any_is_open_and_reveal_clears_what_hides_a_session() {
    let (api, web) = (project("api"), project("web"));
    let s = session(&web, 1, SessionState::Idle);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api.clone(), web.clone()], vec![s.clone()]);
    assert!(sidebar.any_expanded(), "web has a live session");
    sidebar.toggle_all();
    assert!(!sidebar.any_expanded());
    sidebar.toggle_all();
    assert!(workspace(&sidebar.rows()[0]).expanded && sidebar.any_expanded());

    sidebar.toggle_all();
    sidebar.filter_mut().insert("api");
    sidebar.set_chip(Chip::Working);
    assert!(sidebar.filtering());
    sidebar.reveal(web.id);
    assert!(!sidebar.filtering());
    assert_eq!(session_ids(&sidebar.rows()), vec![s.id]);
}

#[test]
fn layout_keeps_its_width_while_hidden() {
    let mut layout = Layout::default();
    assert!(!layout.hidden() && !layout.compact(), "starts full width");

    layout.toggle_compact();
    assert!(!layout.hidden() && layout.compact());
    layout.toggle_hidden();
    assert!(
        layout.hidden() && !layout.compact(),
        "hidden: no rail on screen"
    );
    layout.show();
    assert!(layout.compact(), "shown again at the rail's width");

    layout.toggle_compact();
    assert!(!layout.compact(), "rail → full width");
    layout.toggle_hidden();
    layout.toggle_hidden();
    assert!(
        !layout.hidden() && !layout.compact(),
        "hide/show keeps full width"
    );

    layout.toggle_hidden();
    layout.toggle_compact();
    assert!(
        !layout.hidden() && layout.compact(),
        "the compact key on a hidden full-width sidebar shows the rail"
    );
}

#[test]
fn clear_filter_drops_the_query_and_the_chip() {
    let api = project("api");
    let idle = session(&api, 1, SessionState::Idle);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api], vec![idle.clone()]);
    sidebar.filter_mut().insert("zzz");
    sidebar.set_chip(Chip::Working);
    assert!(sidebar.rows().is_empty());

    sidebar.clear_filter();
    assert!(!sidebar.filtering());
    assert_eq!(session_ids(&sidebar.rows()), vec![idle.id]);
}

#[test]
fn compact_tips_spell_out_what_the_rail_leaves_out() {
    let (api, web) = (project("api"), project("web"));
    let mut titled = session(&api, 1_000, SessionState::Working);
    titled.title = Some("Fix the very long login redirect bug".into());
    let untitled = session(&web, 1_000, SessionState::Done);
    let mut sidebar = Sidebar::default();
    sidebar.load(vec![api, web], vec![titled, untitled]);

    // The first load opens both (each has a live session): api, its session, web, its session.
    let rows = sidebar.rows();
    let sessions: Vec<&SessionRow> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Session(row) => Some(row),
            _ => None,
        })
        .collect();
    assert_eq!(
        workspace_tip(workspace(&rows[0])),
        "api\n1 working\nC:\\src\\api"
    );
    assert_eq!(
        session_tip(sessions[0], 121_000),
        "claude · Fix the very long login redirect bug\nworking · 2m"
    );
    // No summary (done is not summarised), no title: no empty lines, no dangling `·`.
    assert_eq!(workspace_tip(workspace(&rows[2])), "web\nC:\\src\\web");
    assert_eq!(session_tip(sessions[1], 121_000), "claude\ndone");
}
