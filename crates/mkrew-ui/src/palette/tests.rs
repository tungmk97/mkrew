use mkrew_protocol::{SessionKind, StateSource, TaskStatus};

use super::*;
use crate::split::Preset;
use crate::task_dialog::DialogKey;

fn context(view: View) -> Context {
    Context {
        workspace: true,
        view,
        workspaces: 2,
        tabs: 2,
        file_tabs: 1,
        terminal: false,
        discard: false,
        git: view == View::Diff,
        diff: true,
        task_changes: false,
        worktree_task: false,
        fan_out: false,
        agents: false,
        agent_pane: false,
        markable: false,
        run_stopped: false,
        run_live: false,
        tab_others: false,
        tab_right: false,
        pane: false,
        live_session: false,
        transcript: false,
        plan_linked: false,
        composer_open: false,
        approve_card: false,
        git_base: false,
        git_repos: false,
        jira: false,
        last_error: false,
        toast: false,
        update_ready: false,
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
    assert!(commands(&off).contains(&Command::ShowFileTree));
    assert!(!Extra::Turns.available(&off));
    assert!(Extra::InsertPrompt.available(&off));
}

fn commands(context: &Context) -> Vec<Command> {
    entries(&Keymap::defaults(), context, &[], &[], &[])
        .into_iter()
        .filter_map(|entry| match entry.action {
            Action::Run(command) => Some(command),
            Action::Extra(_)
            | Action::GoTo(_)
            | Action::Theme(_)
            | Action::NewAgentPane(_)
            | Action::RunCommand(..) => None,
        })
        .collect()
}

fn project(name: &str, archived: bool) -> Project {
    Project {
        id: Ulid::new(),
        jira_keys: Vec::new(),
        position: 0,
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
        position: 0.0,
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
        assert_eq!(entry.action, Action::Run(Command::OpenSettings));
        assert_eq!(entry.shortcut, "Ctrl+,");
    }
    let titles: Vec<&str> = listed.iter().map(|entry| entry.title.as_str()).collect();
    assert!(titles.contains(
        &"Keymap error: `close_pane = \"ctrl-shift-k\"` is already bound to `add_workspace`"
    ));
    assert!(titles.contains(&"Keymap error: unknown command `nope` in keymap.toml"));
    assert!(titles.contains(&"Keymap error: `find = \"\"` is not a valid keystroke"));
    // Typing "keymap" finds them (the settings command's title no longer has the word).
    let mut all = listed;
    all.extend(entries(&keymap, &context(View::Terminals), &[], &[], &[]));
    let mut palette = Palette::new(all);
    palette.insert("keymap");
    assert_eq!(palette.rows().len(), 3);
}

#[test]
fn a_settings_error_opens_the_settings() {
    let error = "settings.toml:2:13: font_size must be 8–32 px, got 4";
    let keymap = Keymap::defaults();
    let listed = settings_error(&keymap, Some(error));
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].action, Action::Run(Command::OpenSettings));
    assert_eq!(listed[0].shortcut, "Ctrl+,");
    assert_eq!(listed[0].title, format!("Settings error: {error}"));
    assert!(settings_error(&keymap, None).is_empty());
}

#[test]
fn commands_carry_their_shortcut() {
    let (keymap, errors) = Keymap::parse("new_shell_tab = \"ctrl-shift-u\"\n");
    assert_eq!(errors, []);
    let all = entries(&keymap, &context(View::Terminals), &[], &[], &[]);
    let shell = all
        .iter()
        .find(|entry| entry.action == Action::Run(Command::NewShellTab))
        .unwrap();
    assert_eq!(shell.title, "Tabs: New shell tab");
    assert_eq!(shell.shortcut, "Ctrl+Shift+U");
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
        Command::ShowGitPanel,
        Command::ToggleInspector,
    ] {
        assert!(terminals.contains(&command), "{command:?}");
    }
    for command in [
        Command::CommandPalette,
        Command::Tab(3),
        Command::Workspace(3),
        Command::Copy,
        Command::CopyMode,
        Command::LinkHints,
        Command::RunTask,
        Command::GitStage,
        Command::ToggleDocSource,
        Command::AddComment,
        Command::PickDown,
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
    assert!(focused.contains(&Command::CopyMode));
    assert!(focused.contains(&Command::LinkHints));
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

    let git = commands(&context(View::Diff));
    assert!(git.contains(&Command::GitStage));
    assert!(git.contains(&Command::AddComment));
    assert!(git.contains(&Command::PickUp));
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
    for command in [
        Command::SplitAgent,
        Command::SplitShell,
        Command::Layout(Preset::Columns2),
    ] {
        assert!(terminals.contains(&command), "{command:?}");
        assert!(git.contains(&command), "{command:?}");
    }
    let no_tabs = commands(&Context {
        tabs: 0,
        ..context(View::Terminals)
    });
    assert!(!no_tabs.contains(&Command::SplitAgent));
    assert!(!no_tabs.contains(&Command::Layout(Preset::Columns2)));
    assert!(commands(&context(View::Board)).contains(&Command::RunTask));
    let docs = commands(&context(View::Docs));
    assert!(docs.contains(&Command::ToggleDocSource));
    assert!(docs.contains(&Command::SendComments));
    assert!(docs.contains(&Command::PickDown));
    assert!(commands(&context(View::Terminals)).contains(&Command::ToggleIgnored));
    assert!(commands(&context(View::Editor)).contains(&Command::SaveFile));
    assert!(commands(&context(View::Editor)).contains(&Command::ReplaceInFile));
    assert!(commands(&context(View::Editor)).contains(&Command::CloseFileTab));
    assert!(!commands(&context(View::Terminals)).contains(&Command::CloseFileTab));
    let no_files = Context {
        file_tabs: 0,
        ..context(View::Terminals)
    };
    assert!(!commands(&no_files).contains(&Command::ShowEditor));
    assert!(commands(&context(View::Terminals)).contains(&Command::ShowEditor));

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
            Command::PreviousInbox,
            Command::Peek,
            Command::OpenSettings,
            Command::ToggleTheme,
            Command::SelectTheme,
            Command::ToggleFullScreen,
            Command::ToggleSidebar,
            Command::ToggleCompactSidebar,
            Command::VoiceInput,
            Command::AskAi
        ]
    );
    assert_eq!(extras(&none), []);
    assert!(
        entries(&Keymap::defaults(), &none, &[], &[], &[])
            .iter()
            .any(|e| e.action == Action::Run(Command::OpenSettings)),
        "settings open without a workspace"
    );
}

#[test]
fn phone_access_is_listed_in_every_view_and_without_a_workspace() {
    for view in [
        View::Terminals,
        View::Board,
        View::Docs,
        View::Editor,
        View::Diff,
    ] {
        assert!(Extra::PhoneAccess.available(&context(view)), "{view:?}");
    }
    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        ..context(View::Terminals)
    };
    let listed = entries(&Keymap::defaults(), &none, &[], &[], &[]);
    let entry = listed
        .iter()
        .find(|e| e.action == Action::Extra(Extra::PhoneAccess))
        .expect("listed without a workspace");
    assert_eq!(entry.title, "Global: Phone access…");
    assert_eq!(entry.shortcut, "", "palette-only: no key is bound");
}

#[test]
fn check_updates_is_listed_without_a_workspace() {
    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        ..context(View::Terminals)
    };
    let listed = entries(&Keymap::defaults(), &none, &[], &[], &[]);
    let entry = listed
        .iter()
        .find(|e| e.action == Action::Extra(Extra::CheckUpdates))
        .expect("listed without a workspace");
    assert_eq!(entry.title, "Global: Check for updates");
}

#[test]
fn workspace_extras_need_an_active_workspace() {
    let workspace = [
        Extra::WorkspacePin,
        Extra::WorkspaceExplorer,
        Extra::WorkspaceDefaultAgent,
        Extra::WorkspaceRemove,
        Extra::WorkspaceMenu,
    ];
    let none = Context {
        workspace: false,
        ..context(View::Terminals)
    };
    for extra in workspace {
        assert!(extra.available(&context(View::Terminals)), "{extra:?}");
        assert!(!extra.available(&none), "{extra:?}");
    }
}

fn extras(context: &Context) -> Vec<Extra> {
    // Listed by the "workspace extras" test and the single-action test below.
    const SEPARATE: [Extra; 8] = [
        Extra::RunSetup,
        Extra::PhoneAccess,
        Extra::CheckUpdates,
        Extra::WorkspacePin,
        Extra::WorkspaceExplorer,
        Extra::WorkspaceDefaultAgent,
        Extra::WorkspaceRemove,
        Extra::WorkspaceMenu,
    ];
    const ALWAYS: [Extra; 5] = [
        Extra::TabPin,
        Extra::WorkspaceDefaultShell,
        Extra::WorkspaceMoveUp,
        Extra::WorkspaceMoveDown,
        Extra::History,
    ];
    entries(&Keymap::defaults(), context, &[], &[], &[])
        .into_iter()
        .filter_map(|entry| match entry.action {
            Action::Extra(extra) if !SEPARATE.contains(&extra) && !ALWAYS.contains(&extra) => {
                Some(extra)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn tab_pane_workspace_and_agent_extras_follow_their_state() {
    let always = [
        Extra::TabPin,
        Extra::WorkspaceDefaultShell,
        Extra::WorkspaceMoveUp,
        Extra::WorkspaceMoveDown,
    ];
    let terminals = context(View::Terminals);
    for extra in always {
        assert!(extra.available(&terminals), "{extra:?}");
    }
    let none = Context {
        workspace: false,
        ..context(View::Terminals)
    };
    let no_tabs = Context {
        tabs: 0,
        ..context(View::Terminals)
    };
    for extra in always {
        assert!(!extra.available(&none), "{extra:?}");
    }
    assert!(!Extra::TabPin.available(&no_tabs), "no tab to pin");
    assert!(
        !Extra::TabPin.available(&context(View::Board)),
        "the strip is not on screen"
    );
    assert!(
        Extra::WorkspaceMoveUp.available(&context(View::Board)),
        "any view"
    );

    let gated = [
        (
            Extra::TabCloseOthers,
            Context {
                tab_others: true,
                ..terminals
            },
        ),
        (
            Extra::TabCloseRight,
            Context {
                tab_right: true,
                ..terminals
            },
        ),
        (
            Extra::PanePin,
            Context {
                pane: true,
                ..terminals
            },
        ),
        (
            Extra::PaneReading,
            Context {
                agent_pane: true,
                ..terminals
            },
        ),
        (
            Extra::AgentStop,
            Context {
                live_session: true,
                ..terminals
            },
        ),
        (
            Extra::AgentTranscript,
            Context {
                transcript: true,
                ..terminals
            },
        ),
        (
            Extra::AgentLinkPlan,
            Context {
                agent_pane: true,
                ..terminals
            },
        ),
        (
            Extra::AgentUnlinkPlan,
            Context {
                agent_pane: true,
                plan_linked: true,
                ..terminals
            },
        ),
        (
            Extra::AgentImprove,
            Context {
                composer_open: true,
                ..terminals
            },
        ),
    ];
    for (extra, on) in gated {
        assert!(extra.available(&on), "{extra:?} when its state holds");
        assert!(!extra.available(&terminals), "{extra:?} without it");
        assert!(
            !extra.available(&Context {
                view: View::Board,
                ..on
            }),
            "{extra:?} on the Board"
        );
    }
    let linked_shell = Context {
        plan_linked: true,
        ..terminals
    };
    assert!(
        !Extra::AgentUnlinkPlan.available(&linked_shell),
        "no agent pane"
    );
}

#[test]
fn palette_only_actions_follow_the_view_and_state() {
    assert_eq!(extras(&context(View::Terminals)), []);
    assert_eq!(extras(&context(View::Diff)), []);
    let git = Context {
        discard: true,
        task_changes: true,
        worktree_task: true,
        fan_out: true,
        ..context(View::Diff)
    };
    assert_eq!(extras(&git), [], "Git actions are commands now");
    let board = Context {
        discard: true,
        task_changes: true,
        ..context(View::Board)
    };
    assert_eq!(extras(&board), [Extra::ViewTaskChanges]);
    let worktree = Context {
        worktree_task: true,
        ..context(View::Board)
    };
    assert_eq!(extras(&worktree), [Extra::MergeBack, Extra::RemoveWorktree]);
    let elsewhere = Context {
        worktree_task: true,
        ..context(View::Terminals)
    };
    assert_eq!(extras(&elsewhere), []);
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
        [
            Extra::InsertPrompt,
            Extra::Turns,
            Extra::FollowInZed,
            Extra::PaneReading,
            Extra::AgentLinkPlan
        ]
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
    assert_eq!(extras(&stopped), []);
    assert!(commands(&stopped).contains(&Command::RunStart));
    assert!(!commands(&stopped).contains(&Command::RunStop));
    let live = Context {
        run_live: true,
        ..context(View::Docs)
    };
    assert_eq!(extras(&live), [Extra::RunShowTab]);
    let live_commands = commands(&live);
    assert!(
        live_commands.contains(&Command::RunRestart) && live_commands.contains(&Command::RunStop)
    );
    assert!(!live_commands.contains(&Command::RunStart));
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
    assert!(titles.contains(&"Global: Phone access…"));
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
            Action::Theme(_) => 3,
            Action::NewAgentPane(_) => 4,
            Action::RunCommand(..) => 5,
        })
        .collect();
    assert!(kinds.is_sorted(), "{kinds:?}");
    assert!(kinds.contains(&1) && kinds.contains(&2));
}

#[test]
fn one_entry_per_agent_profile_starts_a_pane_with_it() {
    use crate::lifecycle::pane_agents;
    let profile = |id: &str, name: &str, kind, installed| mkrew_protocol::AgentProfileInfo {
        id: id.into(),
        name: name.into(),
        kind,
        command: id.into(),
        icon: None,
        installed,
        ask: false,
        ask_web: false,
    };
    let profiles = [
        profile("claude", "Claude Code", SessionKind::Agent, true),
        profile("pwsh", "PowerShell", SessionKind::Shell, true),
        profile("gone", "Gone", SessionKind::Agent, false),
        profile("codex", "Codex", SessionKind::Agent, true),
    ];
    let agents = pane_agents(&profiles, Some("codex"));
    let keymap = Keymap::defaults();
    let listed = agent_entries(&keymap, &context(View::Board), &agents);
    let titles: Vec<&str> = listed.iter().map(|e| e.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Agent: New Codex pane", "Agent: New Claude Code pane"],
        "installed agents only, the default first"
    );
    // Each entry carries the profile's position in the shell's list; the default shows its key.
    assert_eq!(listed[0].action, Action::NewAgentPane(3));
    assert_eq!(listed[1].action, Action::NewAgentPane(0));
    assert_eq!(listed[0].shortcut, keymap.label(Command::SplitAgent));
    assert_eq!(listed[1].shortcut, "");
    // The query finds one by its name.
    let mut palette = Palette::new(listed);
    palette.insert("claude");
    assert_eq!(palette.picked(), Some(Action::NewAgentPane(0)));

    // Where "New agent pane" is not offered (no workspace, no tab) neither are they.
    let none = Context {
        workspace: false,
        ..context(View::Board)
    };
    assert!(agent_entries(&keymap, &none, &agents).is_empty());
    let no_tabs = Context {
        tabs: 0,
        ..context(View::Board)
    };
    assert!(agent_entries(&keymap, &no_tabs, &agents).is_empty());
}

#[test]
fn git_jira_and_global_extras_follow_their_state() {
    let gated = [
        (
            Extra::GitChangeBase,
            Context {
                git: true,
                git_base: true,
                ..context(View::Terminals)
            },
        ),
        (
            Extra::GitSwitchRepo,
            Context {
                git: true,
                git_repos: true,
                ..context(View::Terminals)
            },
        ),
        (
            Extra::JiraMine,
            Context {
                jira: true,
                ..context(View::Board)
            },
        ),
        (
            Extra::JiraLayout,
            Context {
                jira: true,
                ..context(View::Board)
            },
        ),
        (
            Extra::JiraRefresh,
            Context {
                jira: true,
                ..context(View::Board)
            },
        ),
        (
            Extra::CopyLastError,
            Context {
                last_error: true,
                ..context(View::Terminals)
            },
        ),
        (
            Extra::DismissToasts,
            Context {
                toast: true,
                ..context(View::Terminals)
            },
        ),
        (
            Extra::InstallUpdate,
            Context {
                update_ready: true,
                ..context(View::Terminals)
            },
        ),
    ];
    for (extra, on) in gated {
        assert!(extra.available(&on), "{extra:?} when its state holds");
        assert!(!extra.available(&context(on.view)), "{extra:?} without it");
    }
    // The Git entries need the Git list on screen, the Jira ones the Board.
    let wrong_view = Context {
        git_base: true,
        git_repos: true,
        ..context(View::Board)
    };
    assert!(!Extra::GitChangeBase.available(&wrong_view));
    assert!(!Extra::GitSwitchRepo.available(&wrong_view));
    assert!(!Extra::JiraRefresh.available(&Context {
        jira: true,
        git: true,
        ..context(View::Diff)
    }));
    // The update and the toasts need no workspace.
    let none = Context {
        workspace: false,
        update_ready: true,
        toast: true,
        ..context(View::Board)
    };
    assert!(Extra::InstallUpdate.available(&none) && Extra::DismissToasts.available(&none));
    // History is the title bar button: it needs a workspace.
    assert!(Extra::History.available(&context(View::Board)));
    assert!(!Extra::History.available(&Context {
        workspace: false,
        ..context(View::Board)
    }));
}

#[test]
fn run_entries_follow_each_commands_state() {
    use crate::run::{Row, RowState, RunAction, Summary};
    let row = |name: &str, state| Row {
        name: name.into(),
        state,
        session_id: None,
        ports: Vec::new(),
    };
    let summary = Summary {
        rows: vec![
            row("api", RowState::Running(SessionState::Working)),
            row("web", RowState::Stopped),
            row("db", RowState::Exited),
        ],
        stray: 0,
    };
    let listed: Vec<(String, Action)> = run_entries(&summary)
        .into_iter()
        .map(|entry| (entry.title, entry.action))
        .collect();
    assert_eq!(
        listed,
        [
            (
                "Run: Restart api".to_owned(),
                Action::RunCommand(RunAction::Restart, 0)
            ),
            (
                "Run: Stop api".to_owned(),
                Action::RunCommand(RunAction::Stop, 0)
            ),
            (
                "Run: Start web".to_owned(),
                Action::RunCommand(RunAction::Start, 1)
            ),
            (
                "Run: Start db".to_owned(),
                Action::RunCommand(RunAction::Start, 2)
            ),
        ]
    );
    assert!(run_entries(&Summary::default()).is_empty());
}

#[test]
fn git_and_board_key_commands_follow_the_view() {
    let git = commands(&context(View::Diff));
    for command in [
        Command::GitStageAll,
        Command::GitUnstageAll,
        Command::GitFetch,
        Command::GitPush,
    ] {
        assert!(git.contains(&command), "{command:?}");
    }
    assert!(
        !git.contains(&Command::GitDiscard),
        "nothing discardable selected"
    );
    let discard = Context {
        discard: true,
        ..context(View::Diff)
    };
    assert!(commands(&discard).contains(&Command::GitDiscard));
    let board = commands(&context(View::Board));
    for key in CardKey::ALL {
        assert!(board.contains(&Command::Card(key)), "{key:?}");
        assert!(!git.contains(&Command::Card(key)), "{key:?}");
    }
    assert!(!board.contains(&Command::GitFetch));
    assert_eq!(title(Command::GitFetch), "Git: Fetch");
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
    let working = all
        .iter()
        .find(|entry| entry.action == Action::GoTo(Target::Session(sessions[0].id)));
    assert_eq!(working.map(|e| e.shortcut.as_str()), Some("••• working"));

    // A session that needs input leads the list, before the commands.
    let asking = session(app.id, Some("deploy?"), SessionState::NeedsInput);
    let all = entries(
        &Keymap::defaults(),
        &context(View::Terminals),
        std::slice::from_ref(&app),
        &[sessions[0].clone(), asking.clone()],
        &[],
    );
    assert_eq!(all[0].action, Action::GoTo(Target::Session(asking.id)));
    assert_eq!(all[0].shortcut, "? needs input");
    assert!(matches!(all[1].action, Action::Run(_)));
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
        &context(View::Diff),
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
    // "View: Git panel" (two word starts) beats later scattered matches.
    assert_eq!(palette.picked(), Some(Action::Run(Command::ShowGitPanel)));
    let rows = palette.rows();
    assert_eq!(rows[0].entry.title, "View: Git panel");
    assert_eq!(rows[0].ranges, [0..1, 6..7]);
    assert_eq!(rows[0].entry.shortcut, "Ctrl+Shift+4");

    palette.key(Key::Insert("zzz".into()));
    assert!(palette.rows().is_empty());
    assert_eq!(palette.picked(), None);
    palette.step(1);
    palette.pick(0);
    assert_eq!(palette.selected, 0);

    for _ in 0..3 {
        palette.key(Key::Edit(DialogKey::Backspace));
    }
    assert_eq!(palette.query(), "vg");
    palette.key(Key::Move(1));
    palette.pick(0);
    assert_eq!(palette.picked(), Some(Action::Run(Command::ShowGitPanel)));
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
fn diff_and_git_commands_follow_what_is_on_screen() {
    let no_diff = Context {
        diff: false,
        ..context(View::Terminals)
    };
    assert!(!commands(&no_diff).contains(&Command::ShowDiff));
    assert!(commands(&context(View::Terminals)).contains(&Command::ShowDiff));
    // The Git tab in the panel next to the terminals: the Git keys act on its list.
    let git_tab = Context {
        git: true,
        ..context(View::Terminals)
    };
    assert!(commands(&git_tab).contains(&Command::GitStage));
    assert!(!commands(&context(View::Terminals)).contains(&Command::GitStage));
    // Diff comments only where the diff is drawn.
    assert!(!commands(&git_tab).contains(&Command::AddComment));
    assert!(commands(&context(View::Diff)).contains(&Command::AddComment));
}

#[test]
fn the_query_takes_select_all_and_ctrl_backspace() {
    let mut palette = Palette::new(Vec::new());
    palette.insert("git push");
    palette.key(Key::Edit(DialogKey::DeleteWord));
    assert_eq!(palette.query(), "git ");
    palette.key(Key::Edit(DialogKey::SelectAll));
    assert!(palette.query().all_selected());
    palette.key(Key::Insert("view".into()));
    assert_eq!(palette.query(), "view");
    assert!(!palette.query().all_selected());
    palette.key(Key::Edit(DialogKey::Undo));
    assert_eq!(palette.query(), "git ", "Ctrl+Z filters again");
}

#[test]
fn mark_unread_is_listed_only_for_a_session_that_can_be_marked() {
    assert!(!Extra::MarkUnread.available(&context(View::Terminals)));
    let marked = Context {
        markable: true,
        ..context(View::Terminals)
    };
    assert!(Extra::MarkUnread.available(&marked));
    let entry = entries(&Keymap::defaults(), &marked, &[], &[], &[])
        .into_iter()
        .find(|entry| entry.action == Action::Extra(Extra::MarkUnread))
        .expect("listed");
    assert_eq!(entry.title, "Agent: Mark unread");
    assert_eq!(entry.shortcut, "", "palette-only: no key is bound");
}

#[test]
fn theme_entries_list_every_theme_with_its_appearance_and_mark_the_current() {
    let entries = theme_entries("Dracula");
    assert_eq!(entries.len(), crate::theme::THEMES.len());
    assert_eq!(entries[0].action, Action::Theme(0));
    assert_eq!(entries[0].title, "Mkrew Dark");
    assert_eq!(entries[0].shortcut, "Dark");
    assert_eq!(entries[2].title, "Dracula");
    assert_eq!(entries[2].shortcut, "✓ Dark");
    assert_eq!(entries[7].shortcut, "Light");
}

#[test]
fn the_picker_starts_on_the_current_theme_and_moves_from_there() {
    let mut palette = Palette::new(theme_entries("Dracula"));
    palette.pick_entry(2);
    assert_eq!(palette.picked(), Some(Action::Theme(2)));
    palette.step(1);
    assert_eq!(palette.picked(), Some(Action::Theme(3)));
    palette.insert("solar");
    assert_eq!(palette.picked(), Some(Action::Theme(10)));
}
