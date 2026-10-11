use mkrew_protocol::{SessionKind, StateSource, TaskStatus};

use super::*;

fn context(view: View) -> Context {
    Context {
        workspace: true,
        view,
        workspaces: 2,
        tabs: 2,
        terminal: false,
        discard: false,
        task_changes: false,
        fan_out: false,
        agents: false,
        agent_pane: false,
        run_stopped: false,
        run_live: false,
        features: FeatureSettings {
            docs: true,
            turns: true,
        },
    }
}

#[test]
fn features_off_hide_docs_and_turns() {
    let off = Context {
        agent_pane: true,
        features: FeatureSettings::default(),
        ..context(View::Terminals)
    };
    assert!(!commands(&off).contains(&Command::ShowDocs));
    assert!(commands(&off).contains(&Command::ShowFiles));
    assert!(!Extra::Turns.available(&off));
    assert!(Extra::InsertPrompt.available(&off));
}

fn commands(context: &Context) -> Vec<Command> {
    entries(&Keymap::defaults(), context, &[], &[], &[])
        .into_iter()
        .filter_map(|entry| match entry.action {
            Action::Run(command) => Some(command),
            Action::Extra(_) | Action::GoTo(_) => None,
        })
        .collect()
}

fn project(name: &str, archived: bool) -> Project {
    Project {
        id: Ulid::new(),
        name: name.into(),
        path: format!("C:/{name}"),
        run_commands: Vec::new(),
        run_autostart: false,
        default_agent: None,
        default_shell: None,
        review_globs: None,
        created_at: 0,
        archived,
        base_ref: None,
        color: None,
        pinned: false,
    }
}

fn session(project_id: Ulid, title: Option<&str>, state: SessionState) -> Session {
    Session {
        id: Ulid::new(),
        task_id: None,
        project_id,
        agent_profile: "claude".into(),
        kind: SessionKind::Agent,
        title: title.map(Into::into),
        terminal_title: None,
        color: None,
        native_session_id: None,
        state,
        state_source: StateSource::default(),
        state_changed_at: 0,
        exit_code: None,
        cols: 80,
        rows: 24,
        started_at: 0,
        ended_at: None,
        pinned: false,
        plan_path: None,
    }
}

fn task(project_id: Ulid, title: &str) -> Task {
    Task {
        id: Ulid::new(),
        project_id,
        title: title.into(),
        prompt: String::new(),
        agent_profile: None,
        status: TaskStatus::Todo,
        position: 0.0,
        checkpoint: None,
        error_flag: false,
        worktree: false,
        worktree_path: None,
        branch: None,
        fanout_of: None,
        created_at: 0,
        updated_at: 0,
    }
}

#[test]
fn every_command_has_a_distinct_title() {
    let titles: Vec<String> = Command::all().into_iter().map(title).collect();
    for (i, title) in titles.iter().enumerate() {
        assert!(!title.is_empty());
        assert!(!titles[..i].contains(title), "{title}");
    }
}

#[test]
fn keymap_errors_open_the_keymap_settings() {
    let (keymap, errors) =
        Keymap::parse("close_pane = \"ctrl-shift-k\"\nnope = \"ctrl-1\"\nfind = \"\"\n");
    let listed = keymap_errors(&keymap, &errors);
    assert_eq!(listed.len(), 3);
    for entry in &listed {
        assert_eq!(entry.action, Action::Run(Command::OpenKeymapSettings));
        assert_eq!(entry.shortcut, "Ctrl+,");
    }
    let titles: Vec<&str> = listed.iter().map(|entry| entry.title.as_str()).collect();
    assert!(titles.contains(
        &"Keymap error: `close_pane = \"ctrl-shift-k\"` is already bound to `add_workspace`"
    ));
    assert!(titles.contains(&"Keymap error: unknown command `nope` in keymap.toml"));
    assert!(titles.contains(&"Keymap error: `find = \"\"` is not a valid keystroke"));
    // Typing "keymap" finds them and the settings command.
    let mut all = listed;
    all.extend(entries(&keymap, &context(View::Terminals), &[], &[], &[]));
    let mut palette = Palette::new(all);
    palette.insert("keymap");
    assert_eq!(palette.rows().len(), 4);
}

#[test]
fn a_settings_error_opens_the_settings() {
    let error = "settings.toml:2:13: font_size must be 8–32 px, got 4";
    let listed = settings_error(Some(error));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].action, Action::Extra(Extra::OpenSettings));
    assert_eq!(listed[0].title, format!("Settings error: {error}"));
    assert!(settings_error(None).is_empty());
}

#[test]
fn commands_carry_their_shortcut() {
    let (keymap, errors) = Keymap::parse("new_shell_tab = \"ctrl-shift-g\"\n");
    assert_eq!(errors, []);
    let all = entries(&keymap, &context(View::Terminals), &[], &[], &[]);
    let shell = all
        .iter()
        .find(|entry| entry.action == Action::Run(Command::NewShellTab))
        .unwrap();
    assert_eq!(shell.title, "Tabs: New shell tab");
    assert_eq!(shell.shortcut, "Ctrl+Shift+G");
}

#[test]
fn availability_follows_the_view() {
    let terminals = commands(&context(View::Terminals));
    for command in [
        Command::NewShellTab,
        Command::CloseTab,
        Command::ClosePane,
        Command::RenamePane,
        Command::Tab(2),
        Command::NextTab,
        Command::Workspace(2),
        Command::ShowGit,
        Command::ToggleInspector,
    ] {
        assert!(terminals.contains(&command), "{command:?}");
    }
    for command in [
        Command::CommandPalette,
        Command::Tab(3),
        Command::Workspace(3),
        Command::Copy,
        Command::RunTask,
        Command::GitStage,
        Command::ToggleDocSource,
        Command::AddComment,
        Command::ToggleIgnored,
        Command::SaveFile,
        Command::FindInFile,
        Command::ReplaceInFile,
    ] {
        assert!(!terminals.contains(&command), "{command:?}");
    }
    let focused = commands(&Context {
        terminal: true,
        ..context(View::Terminals)
    });
    assert!(focused.contains(&Command::Copy));
    assert!(focused.contains(&Command::Find));
    assert!(focused.contains(&Command::SendSelection));
    assert!(
        !terminals.contains(&Command::SendSelection),
        "no terminal focused"
    );
    assert!(!focused.contains(&Command::OpenComposer), "a shell pane");
    let agent = commands(&Context {
        terminal: true,
        agent_pane: true,
        ..context(View::Terminals)
    });
    assert!(agent.contains(&Command::OpenComposer));

    let git = commands(&context(View::Git));
    assert!(git.contains(&Command::GitStage));
    assert!(git.contains(&Command::AddComment));
    assert!(git.contains(&Command::SendSelection));
    assert!(
        !git.contains(&Command::ClosePane),
        "panes are not on screen"
    );
    assert!(
        !git.contains(&Command::RenamePane),
        "panes are not on screen"
    );
    assert!(!git.contains(&Command::RunTask));
    assert!(git.contains(&Command::NewShellTab), "shows the terminals");
    assert!(
        git.contains(&Command::ToggleInspector),
        "shows the terminals"
    );
    // Like the buttons: from any view (they show Terminals first), once a tab exists.
    for command in [Command::SplitAgent, Command::SplitShell] {
        assert!(terminals.contains(&command), "{command:?}");
        assert!(git.contains(&command), "{command:?}");
    }
    let no_tabs = commands(&Context {
        tabs: 0,
        ..context(View::Terminals)
    });
    assert!(!no_tabs.contains(&Command::SplitAgent));
    assert!(commands(&context(View::Board)).contains(&Command::RunTask));
    let docs = commands(&context(View::Docs));
    assert!(docs.contains(&Command::ToggleDocSource));
    assert!(docs.contains(&Command::SendComments));
    assert!(commands(&context(View::Files)).contains(&Command::ToggleIgnored));
    assert!(commands(&context(View::Files)).contains(&Command::SaveFile));
    assert!(commands(&context(View::Files)).contains(&Command::ReplaceInFile));

    let lone = commands(&Context {
        workspaces: 1,
        tabs: 1,
        ..context(View::Terminals)
    });
    assert!(!lone.contains(&Command::NextWorkspace));
    assert!(!lone.contains(&Command::NextTab));
    assert!(lone.contains(&Command::Tab(1)));

    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        agents: true,
        run_stopped: true,
        run_live: true,
        ..context(View::Terminals)
    };
    assert_eq!(
        commands(&none),
        [
            Command::AddWorkspace,
            Command::NextInbox,
            Command::Peek,
            Command::OpenKeymapSettings,
            Command::ToggleTheme,
            Command::ToggleFullScreen,
            Command::ToggleSidebar,
            Command::ToggleCompactSidebar
        ]
    );
    assert_eq!(extras(&none), []);
    assert!(
        entries(&Keymap::defaults(), &none, &[], &[], &[])
            .iter()
            .any(|e| e.action == Action::Extra(Extra::OpenSettings)),
        "settings open without a workspace"
    );
}

fn extras(context: &Context) -> Vec<Extra> {
    entries(&Keymap::defaults(), context, &[], &[], &[])
        .into_iter()
        .filter_map(|entry| match entry.action {
            Action::Extra(extra) if !matches!(extra, Extra::RunSetup | Extra::OpenSettings) => {
                Some(extra)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn palette_only_actions_follow_the_view_and_state() {
    assert_eq!(extras(&context(View::Terminals)), []);
    assert_eq!(
        extras(&context(View::Git)),
        [Extra::GitFetch, Extra::GitPush]
    );
    let git = Context {
        discard: true,
        task_changes: true,
        fan_out: true,
        ..context(View::Git)
    };
    assert_eq!(
        extras(&git),
        [Extra::GitFetch, Extra::GitPush, Extra::GitDiscard]
    );
    let board = Context {
        discard: true,
        task_changes: true,
        ..context(View::Board)
    };
    assert_eq!(extras(&board), [Extra::ViewTaskChanges]);
    let fan_out = Context {
        fan_out: true,
        ..context(View::Board)
    };
    assert_eq!(extras(&fan_out), [Extra::FanOut]);
    let agents = Context {
        agents: true,
        ..context(View::Board)
    };
    assert_eq!(extras(&agents), [Extra::Broadcast]);
    let agent_pane = Context {
        agent_pane: true,
        ..context(View::Terminals)
    };
    assert_eq!(
        extras(&agent_pane),
        [Extra::InsertPrompt, Extra::Turns, Extra::FollowInZed]
    );
    let elsewhere = Context {
        agent_pane: true,
        ..context(View::Board)
    };
    assert_eq!(extras(&elsewhere), [], "the composer is not on screen");
    let stopped = Context {
        run_stopped: true,
        ..context(View::Docs)
    };
    assert_eq!(extras(&stopped), [Extra::RunStart]);
    let live = Context {
        run_live: true,
        ..context(View::Docs)
    };
    assert_eq!(
        extras(&live),
        [Extra::RunRestart, Extra::RunStop, Extra::RunShowTab]
    );
    let all = entries(&Keymap::defaults(), &context(View::Board), &[], &[], &[]);
    assert!(
        all.iter()
            .any(|e| matches!(e.action, Action::Extra(Extra::RunSetup)))
    );

    let commands: Vec<String> = Command::all().into_iter().map(title).collect();
    let titles: Vec<&str> = Extra::ALL.into_iter().map(Extra::title).collect();
    for (i, title) in titles.iter().enumerate() {
        assert!(!titles[..i].contains(title), "{title}");
        assert!(!commands.iter().any(|command| command == title), "{title}");
    }
    assert!(titles.contains(&"Global: Open settings"));
    // Palette-only actions come after the commands, before "Go to".
    let all = entries(
        &Keymap::defaults(),
        &git,
        &[project("app", false)],
        &[],
        &[],
    );
    let kinds: Vec<u8> = all
        .iter()
        .map(|entry| match entry.action {
            Action::Run(_) => 0,
            Action::Extra(_) => 1,
            Action::GoTo(_) => 2,
        })
        .collect();
    assert!(kinds.is_sorted(), "{kinds:?}");
    assert!(kinds.contains(&1) && kinds.contains(&2));
}

#[test]
fn go_to_workspaces_live_sessions_and_tasks() {
    let app = project("app", false);
    let old = project("old", true);
    let sessions = [
        session(app.id, Some("fix login"), SessionState::Working),
        session(app.id, None, SessionState::Idle),
        session(app.id, Some("gone"), SessionState::Exited),
        session(old.id, Some("archived"), SessionState::Working),
    ];
    let tasks = [task(app.id, "Add tests"), task(old.id, "Archived task")];
    let all = entries(
        &Keymap::defaults(),
        &context(View::Terminals),
        &[app.clone(), old],
        &sessions,
        &tasks,
    );
    let go_to: Vec<(Action, &str)> = all
        .iter()
        .filter(|entry| matches!(entry.action, Action::GoTo(_)))
        .map(|entry| (entry.action, entry.title.as_str()))
        .collect();
    assert_eq!(
        go_to,
        [
            (
                Action::GoTo(Target::Workspace(app.id)),
                "Go to workspace: app"
            ),
            (
                Action::GoTo(Target::Session(sessions[0].id)),
                "Go to session: fix login · app"
            ),
            (
                Action::GoTo(Target::Session(sessions[1].id)),
                "Go to session: claude · app"
            ),
            (
                Action::GoTo(Target::Task {
                    project_id: app.id,
                    task_id: tasks[0].id
                }),
                "Go to task: Add tests · app"
            ),
        ]
    );
    // Commands come first.
    assert!(matches!(all[0].action, Action::Run(_)));
}

#[test]
fn fuzzy_matches_subsequences_ignoring_case_and_spaces() {
    let (_, indices) = fuzzy("nst", "Tabs: New shell tab").unwrap();
    assert_eq!(indices, [6, 10, 16]);
    assert!(fuzzy("git stage", "Git: Stage selected file or hunk").is_some());
    assert_eq!(fuzzy("zz", "Git: Stage"), None);
    assert_eq!(fuzzy("", "anything"), Some((0, vec![])));
    assert_eq!(fuzzy("é", "Café"), Some((1, vec![3])));
    // The best alignment, not the first chars found: "Split", not the "s" of "Panes".
    let (_, indices) = fuzzy("split", "Panes: Split right").unwrap();
    assert_eq!(indices, [7, 8, 9, 10, 11]);
    let (_, indices) = fuzzy("ab", "xab ab").unwrap();
    assert_eq!(indices, [4, 5], "the word start wins");
    let (_, indices) = fuzzy("ab", "ab ab").unwrap();
    assert_eq!(indices, [0, 1], "ties go to the earliest chars");
    // Word starts and runs beat scattered chars.
    let start = fuzzy("st", "Git: Stage").unwrap().0;
    let scattered = fuzzy("st", "Last item").unwrap().0;
    assert!(start > scattered, "{start} vs {scattered}");
    let camel = fuzzy("z", "OpenInZed").unwrap().0;
    let inner = fuzzy("z", "Puzzle").unwrap().0;
    assert!(camel > inner);
}

#[test]
fn typing_filters_ranks_and_resets_the_pick() {
    let mut palette = Palette::new(entries(
        &Keymap::defaults(),
        &context(View::Git),
        &[],
        &[],
        &[],
    ));
    let all = palette.rows().len();
    assert_eq!(palette.picked(), Some(Action::Run(Command::AddWorkspace)));
    palette.step(-1);
    assert_eq!(palette.selected, all - 1, "up from the top wraps");

    palette.key(Key::Insert("vg".into()));
    assert_eq!(palette.selected, 0);
    // "View: Git" (two word starts) beats later scattered matches.
    assert_eq!(palette.picked(), Some(Action::Run(Command::ShowGit)));
    let rows = palette.rows();
    assert_eq!(rows[0].entry.title, "View: Git");
    assert_eq!(rows[0].ranges, [0..1, 6..7]);
    assert_eq!(rows[0].entry.shortcut, "Ctrl+Shift+5");

    palette.key(Key::Insert("zzz".into()));
    assert!(palette.rows().is_empty());
    assert_eq!(palette.picked(), None);
    palette.step(1);
    palette.pick(0);
    assert_eq!(palette.selected, 0);

    for _ in 0..3 {
        palette.key(Key::Backspace);
    }
    assert_eq!(palette.query(), "vg");
    palette.key(Key::Move(1));
    palette.pick(0);
    assert_eq!(palette.picked(), Some(Action::Run(Command::ShowGit)));
    palette.pick(999);
    assert_eq!(palette.selected, 0, "out of range ignored");

    palette.insert("\r\n");
    assert_eq!(palette.query(), "vg");
    palette.key(Key::Paste);
    palette.key(Key::Open);
    palette.key(Key::Close);
    assert_eq!(palette.query(), "vg");
}

#[test]
fn the_query_takes_select_all_and_ctrl_backspace() {
    let mut palette = Palette::new(Vec::new());
    palette.insert("git push");
    palette.key(Key::DeleteWord);
    assert_eq!(palette.query(), "git ");
    palette.key(Key::SelectAll);
    assert!(palette.query_selected());
    palette.key(Key::Insert("view".into()));
    assert_eq!((palette.query(), palette.query_selected()), ("view", false));
}
