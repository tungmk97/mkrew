use mkrew_protocol::{
    Branch, Commit, FsMatch, FsSearchResult, SearchHit, SessionKind, StateSource, TaskStatus,
};

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
        file_tree: false,
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
        machine: false,
        detached_window: false,
        features: FeatureSettings {
            docs: true,
            turns: true,
            session: true,
        },
    }
}

#[test]
fn phone_access_and_updates_are_not_offered_in_a_machine_window() {
    let machine = Context {
        machine: true,
        ..context(View::Terminals)
    };
    assert!(!Extra::PhoneAccess.available(&machine));
    assert!(!Extra::CheckUpdates.available(&machine));
    assert!(Extra::PhoneAccess.available(&context(View::Terminals)));
}

#[test]
fn machines_are_added_and_opened_from_this_computers_window_only() {
    let machine = Context {
        machine: true,
        ..context(View::Terminals)
    };
    assert!(Extra::AddMachine.available(&context(View::Terminals)));
    assert!(!Extra::AddMachine.available(&machine));
    let names = ["Home PC".to_owned(), "Work".to_owned()];
    let listed = machine_entries(&context(View::Terminals), &names);
    assert_eq!(
        listed
            .iter()
            .map(|entry| (entry.action.clone(), entry.title.as_str()))
            .collect::<Vec<_>>(),
        [
            (Action::OpenMachine(0), "Machine: Open Home PC"),
            (Action::OpenMachine(1), "Machine: Open Work"),
        ]
    );
    assert!(machine_entries(&machine, &names).is_empty());
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
            | Action::RunCommand(..)
            | Action::OpenMachine(_)
            | Action::OpenFile { .. }
            | Action::OpenSearchHit { .. }
            | Action::Checkout { .. }
            | Action::ShowCommit { .. } => None,
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
        Command::SearchInFiles,
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
    // Move tab: terminal tabs on the panes, file tabs in the Editor view (needs a file tab).
    assert!(commands(&context(View::Terminals)).contains(&Command::MoveTabLeft));
    assert!(commands(&context(View::Editor)).contains(&Command::MoveTabLeft));
    let editor_no_files = Context {
        file_tabs: 0,
        ..context(View::Editor)
    };
    assert!(!commands(&editor_no_files).contains(&Command::MoveTabLeft));
    // Open in a kept tab needs the tree on screen.
    assert!(!commands(&context(View::Terminals)).contains(&Command::OpenKept));
    let with_tree = Context {
        file_tree: true,
        ..context(View::Terminals)
    };
    assert!(commands(&with_tree).contains(&Command::OpenKept));

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
    assert!(!commands(&none).contains(&Command::SearchInFiles));
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
fn the_open_commands_name_the_scope_they_open_on() {
    assert_eq!(scope_for(Command::QuickOpen), Some(Scope::Files));
    assert_eq!(scope_for(Command::SearchInFiles), Some(Scope::Search));
    assert_eq!(scope_for(Command::CommandPalette), Some(Scope::All));
    assert_eq!(scope_for(Command::NewShellTab), None);
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
fn ports_is_listed_without_a_workspace_and_in_a_machine_window() {
    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        machine: true,
        ..context(View::Terminals)
    };
    let listed = entries(&Keymap::defaults(), &none, &[], &[], &[]);
    let entry = listed
        .iter()
        .find(|e| e.action == Action::Extra(Extra::Ports))
        .expect("listed without a workspace");
    assert_eq!(entry.title, "Ports: Show…");
}

#[test]
fn manage_prompts_is_listed_without_a_workspace_and_in_a_machine_window() {
    let none = Context {
        workspace: false,
        workspaces: 0,
        tabs: 0,
        machine: true,
        ..context(View::Terminals)
    };
    assert!(Extra::ManagePrompts.available(&context(View::Terminals)));
    let listed = entries(&Keymap::defaults(), &none, &[], &[], &[]);
    let entry = listed
        .iter()
        .find(|e| e.action == Action::Extra(Extra::ManagePrompts))
        .expect("listed without a workspace");
    assert_eq!(entry.title, "Global: Manage prompts…");
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
    // Listed by the "workspace extras" test and the single-action tests below.
    const SEPARATE: [Extra; 12] = [
        Extra::RunSetup,
        Extra::Ports,
        Extra::PhoneAccess,
        Extra::CheckUpdates,
        Extra::AddMachine,
        Extra::ManagePrompts,
        Extra::WorkspacePin,
        Extra::WorkspaceExplorer,
        Extra::WorkspaceDefaultAgent,
        Extra::WorkspaceRemove,
        Extra::WorkspaceMenu,
        Extra::BoardScope,
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
            Action::OpenMachine(_) => 6,
            Action::OpenFile { .. }
            | Action::OpenSearchHit { .. }
            | Action::Checkout { .. }
            | Action::ShowCommit { .. } => 7,
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
    // The `This workspace | All workspaces` switch heads the Tasks side of the Board only.
    assert!(Extra::BoardScope.available(&context(View::Board)));
    let jira = Context {
        jira: true,
        ..context(View::Board)
    };
    assert!(!Extra::BoardScope.available(&jira));
    assert!(!Extra::BoardScope.available(&context(View::Terminals)));
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
        .map(|entry| (entry.action.clone(), entry.title.as_str()))
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
    assert_eq!(working.and_then(|e| e.detail.as_deref()), Some("● working"));
    assert_eq!(working.map(|e| e.shortcut.as_str()), Some(""));

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
    assert_eq!(all[0].detail.as_deref(), Some("? needs input"));
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

#[test]
fn window_extras_show_only_in_a_detached_window() {
    let main = context(View::Terminals);
    let detached = Context {
        detached_window: true,
        ..context(View::Terminals)
    };
    for extra in [
        Extra::TabMoveToMainWindow,
        Extra::WindowMoveAllBack,
        Extra::WindowClose,
    ] {
        assert!(!extra.available(&main), "{extra:?} in the main window");
        assert!(extra.available(&detached), "{extra:?} in a detached window");
    }
    assert!(available(Command::MoveTabToNewWindow, &main));
    let no_tabs = Context {
        tabs: 0,
        ..context(View::Terminals)
    };
    assert!(!available(Command::MoveTabToNewWindow, &no_tabs));
    assert!(
        !available(Command::MoveTabToNewWindow, &context(View::Editor)),
        "file tabs move in Plan 2"
    );
}

#[test]
fn a_detached_window_lists_only_what_acts_there() {
    let id = Ulid::new();
    for action in [
        Action::Run(Command::SplitRight),
        Action::Run(Command::MoveTabToNewWindow),
        Action::Run(Command::CommandPalette),
        Action::Extra(Extra::TabMoveToMainWindow),
        Action::Extra(Extra::WindowMoveAllBack),
        Action::Extra(Extra::WindowClose),
        Action::Extra(Extra::TabPin),
        Action::GoTo(Target::Session(id)),
    ] {
        assert!(acts_in_detached_window(&action), "{action:?}");
    }
    for action in [
        Action::Run(Command::ShowBoard),
        Action::Run(Command::ToggleSidebar),
        Action::Extra(Extra::TabCloseOthers),
        Action::Extra(Extra::Broadcast),
        Action::GoTo(Target::Workspace(id)),
        Action::Theme(0),
        Action::NewAgentPane(0),
        Action::OpenMachine(0),
    ] {
        assert!(!acts_in_detached_window(&action), "{action:?}");
    }
}

#[test]
fn the_picker_keys() {
    let parse = |text: &str| key(&Keystroke::parse(text).unwrap());
    assert_eq!(parse("up"), Some(Key::Move(-1)));
    assert_eq!(parse("down"), Some(Key::Move(1)));
    assert_eq!(parse("escape"), Some(Key::Close));
    assert_eq!(parse("enter"), Some(Key::Open));
    assert_eq!(parse("ctrl-enter"), Some(Key::Open));
    assert_eq!(parse("backspace"), Some(Key::Edit(DialogKey::Backspace)));
    assert_eq!(parse("ctrl-v"), Some(Key::Paste));
    assert_eq!(
        parse("ctrl-backspace"),
        Some(Key::Edit(DialogKey::DeleteWord))
    );
    assert_eq!(parse("ctrl-a"), Some(Key::Edit(DialogKey::SelectAll)));
    assert_eq!(parse("tab"), None);
    assert_eq!(parse("ctrl-up"), None);
    assert_eq!(parse("ctrl-x"), Some(Key::Edit(DialogKey::Cut)));
}

#[test]
fn matched_chars_become_merged_byte_ranges() {
    assert_eq!(
        match_ranges("src/main.rs", &[4, 5, 6, 9]),
        vec![4..7, 9..10]
    );
    // Multi-byte chars: indices are chars, ranges are bytes.
    assert_eq!(match_ranges("é/ab", &[0, 1, 3]), vec![0..3, 4..5]);
    assert_eq!(match_ranges("ab", &[5]), Vec::<Range<usize>>::new());
}

#[test]
fn tab_walks_the_scopes_in_order_and_wraps() {
    assert_eq!(
        Scope::ORDER,
        [
            Scope::All,
            Scope::Commands,
            Scope::Sessions,
            Scope::Files,
            Scope::Search,
            Scope::Branches,
            Scope::Commits
        ]
    );
    let mut scope = Scope::All;
    let mut walked = Vec::new();
    for _ in 0..7 {
        scope = scope.next();
        walked.push(scope);
    }
    assert_eq!(walked[..6], Scope::ORDER[1..]);
    assert_eq!(scope, Scope::All, "Commits wraps to All");
    assert_eq!(
        Scope::All.prev(),
        Scope::Commits,
        "Shift+Tab from All wraps"
    );
    assert_eq!(Scope::Files.prev(), Scope::Sessions);
}

#[test]
fn tab_and_shift_tab_are_the_scope_keys() {
    let step = |keys: &str| scope_step(&Keystroke::parse(keys).unwrap());
    assert_eq!(step("tab"), Some(1));
    assert_eq!(step("shift-tab"), Some(-1));
    assert_eq!(step("ctrl-tab"), None);
    assert_eq!(step("alt-tab"), None);
    assert_eq!(step("a"), None);
}

#[test]
fn an_entry_belongs_to_the_section_of_its_action() {
    let section = |action: Action| Entry::new(action, "x", "").action.scope();
    assert_eq!(section(Action::Run(Command::NewTask)), Scope::Commands);
    assert_eq!(section(Action::Theme(1)), Scope::Commands);
    assert_eq!(
        section(Action::GoTo(Target::Workspace(Ulid::new()))),
        Scope::Sessions
    );
    assert_eq!(section(Action::OpenFile { path: "a".into() }), Scope::Files);
    assert_eq!(
        section(Action::OpenSearchHit {
            path: "a".into(),
            line: 1
        }),
        Scope::Search
    );
    assert_eq!(
        section(Action::Checkout {
            repo: None,
            branch: "main".into()
        }),
        Scope::Branches
    );
    assert_eq!(
        section(Action::ShowCommit {
            repo: None,
            sha: "abc".into()
        }),
        Scope::Commits
    );
}

#[test]
fn a_row_carries_its_section_detail_and_age() {
    let entry = Entry {
        detail: Some("abc1234".into()),
        age: Some("3d".into()),
        ..Entry::new(
            Action::ShowCommit {
                repo: None,
                sha: "abc1234def".into(),
            },
            "fix login",
            "",
        )
    };
    let palette = Palette::new(vec![entry]);
    let rows = palette.rows();
    assert_eq!(rows[0].section, Scope::Commits);
    assert_eq!(rows[0].detail, Some("abc1234"));
    assert_eq!(rows[0].age, Some("3d"));
}

fn source() -> Source {
    Source::new(Ulid::new(), None, None)
}

fn command(n: usize) -> Entry {
    Entry::new(Action::Theme(n), format!("Theme {n}"), "")
}

fn session_entry(n: usize) -> Entry {
    Entry::new(
        Action::GoTo(Target::Session(Ulid::new())),
        format!("Go to session: s{n}"),
        "",
    )
}

fn matches_of(paths: &[&str]) -> Vec<FsMatch> {
    paths
        .iter()
        .map(|path| FsMatch {
            path: (*path).to_owned(),
            score: 1,
            indices: vec![],
        })
        .collect()
}

fn branch(name: &str, remote: bool, current: bool, at: i64) -> Branch {
    Branch {
        name: name.into(),
        remote,
        current,
        upstream: None,
        last_commit_at: at,
        subject: "work".into(),
    }
}

fn log_commit(hash: &str, summary: &str, at: i64) -> Commit {
    Commit {
        hash: hash.into(),
        parents: vec![],
        author_name: "t".into(),
        author_email: "t@t".into(),
        authored_at: at,
        summary: summary.into(),
        body: String::new(),
    }
}

fn headers(palette: &Palette) -> Vec<&'static str> {
    palette
        .lines()
        .into_iter()
        .filter_map(|line| match line {
            Line::Header(title) => Some(title),
            _ => None,
        })
        .collect()
}

fn scopes(fetches: &[Fetch]) -> Vec<Scope> {
    fetches.iter().map(|fetch| fetch.scope).collect()
}

#[test]
fn a_worktree_never_carries_a_repo() {
    let (project, task) = (Ulid::new(), Ulid::new());
    assert_eq!(
        Source::new(project, Some(task), Some("fe".into())).repo,
        None
    );
    assert_eq!(
        Source::new(project, None, Some("fe".into()))
            .repo
            .as_deref(),
        Some("fe")
    );
}

#[test]
fn all_caps_each_section_and_adds_a_more_row() {
    let entries: Vec<Entry> = (0..8)
        .map(command)
        .chain((0..7).map(session_entry))
        .collect();
    let mut palette = Palette::scoped(entries, None, Scope::All, vec![]);
    palette.insert("e");
    assert_eq!(headers(&palette), ["Commands", "Sessions"]);
    let lines = palette.lines();
    let rows = lines
        .iter()
        .filter(|line| matches!(line, Line::Row { .. }))
        .count();
    assert_eq!(rows, 2 * theme::PALETTE_GROUP_LIMIT);
    let more: Vec<(Scope, usize)> = lines
        .iter()
        .filter_map(|line| match line {
            Line::More { scope, hidden, .. } => Some((*scope, *hidden)),
            _ => None,
        })
        .collect();
    assert_eq!(more, [(Scope::Commands, 3), (Scope::Sessions, 2)]);
}

#[test]
fn the_more_row_switches_to_its_scope() {
    let entries: Vec<Entry> = (0..8).map(command).collect();
    let mut palette = Palette::scoped(entries, None, Scope::All, vec![]);
    palette.insert("e");
    let pick = palette
        .lines()
        .into_iter()
        .find_map(|line| match line {
            Line::More { pick, .. } => Some(pick),
            _ => None,
        })
        .unwrap();
    palette.pick(pick);
    assert_eq!(palette.picked(), None);
    assert_eq!(palette.picked_more(), Some(Scope::Commands));
    palette.set_scope(Scope::Commands);
    assert_eq!(palette.rows().len(), 8);
}

#[test]
fn a_flat_palette_has_no_scopes() {
    let mut palette = Palette::new(vec![command(0)]);
    palette.step_scope(1);
    assert_eq!(palette.scope(), Scope::All);
    assert!(!palette.tabs());
    let mut scoped = Palette::scoped(vec![], None, Scope::All, vec![]);
    scoped.step_scope(1);
    assert_eq!(scoped.scope(), Scope::Commands);
    scoped.step_scope(-1);
    assert_eq!(scoped.scope(), Scope::All);
}

#[test]
fn search_never_runs_or_shows_in_all() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::All, vec![]);
    palette.insert("needle");
    let asked = palette.wanted();
    assert_eq!(
        scopes(&asked),
        [Scope::Files, Scope::Branches, Scope::Commits]
    );
    assert!(!headers(&palette).contains(&"Search"));
}

#[test]
fn files_are_asked_on_open_debounced_and_late_answers_are_dropped() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    let first = palette.wanted().remove(0);
    // Opening Files does not wait.
    assert_eq!(first.delay, Duration::ZERO);
    let Call::Files(params) = &first.call else {
        panic!("{:?}", first.call)
    };
    assert_eq!(
        (params.query.as_str(), params.limit),
        ("", FsFindParams::DEFAULT_LIMIT)
    );
    assert_eq!(palette.wanted(), [], "nothing new to ask");
    assert!(palette.answer(
        first.serial,
        Answer::Files(Ok(matches_of(&["README.md"]))),
        0
    ));

    palette.insert("ma");
    let second = palette.wanted().remove(0);
    assert_eq!(second.delay, theme::PALETTE_FILES_DELAY);
    assert!(palette.is_current(Scope::Files, second.serial));
    assert!(!palette.is_current(Scope::Files, first.serial));
    // The previous matches stay on screen while the new answer is awaited.
    assert_eq!(palette.rows().len(), 1);
    // The first query's answer arrives late: dropped.
    assert!(!palette.answer(first.serial, Answer::Files(Ok(matches_of(&["old.rs"]))), 0));
    assert!(palette.answer(
        second.serial,
        Answer::Files(Ok(matches_of(&["src/main.rs", "Makefile"]))),
        0
    ));
    assert_eq!(
        palette.picked(),
        Some(Action::OpenFile {
            path: "src/main.rs".into()
        })
    );
    palette.step(1);
    assert_eq!(
        palette.picked(),
        Some(Action::OpenFile {
            path: "Makefile".into()
        })
    );
    palette.step(1);
    assert_eq!(palette.selected, 0, "down from the bottom wraps");
}

/// An `@` picker closed and opened again: the old palette's late answer has another number,
/// so it never shows in the new one.
#[test]
fn a_closed_palettes_answer_never_lands_in_the_next() {
    let mut closed = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    closed.wanted();
    closed.insert("a");
    let late = closed.wanted().remove(0);
    let mut open = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    let current = open.wanted().remove(0);
    assert_ne!(late.serial, current.serial);
    assert!(!open.answer(late.serial, Answer::Files(Ok(matches_of(&["stale.rs"]))), 0));
    assert!(open.rows().is_empty());
    assert_eq!(
        open.lines(),
        [Line::Note {
            text: "Loading…".into(),
            error: false
        }]
    );
}

#[test]
fn search_waits_for_two_chars_debounces_and_clears_short_queries() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Search, vec![]);
    assert_eq!(palette.wanted(), []);
    palette.insert("a");
    assert_eq!(palette.wanted(), [], "one char is not searched");
    palette.insert("b");
    let asked = palette.wanted().remove(0);
    assert_eq!(asked.delay, theme::PALETTE_SEARCH_DELAY);
    let Call::Search(params) = &asked.call else {
        panic!("{:?}", asked.call)
    };
    assert_eq!(
        (params.query.as_str(), params.regex, params.limit),
        ("ab", false, 200)
    );
    let hit = SearchHit {
        path: "src/a.rs".into(),
        line: 3,
        preview: "let ab = 1;".into(),
        ranges: vec![(4, 6)],
    };
    let result = FsSearchResult {
        hits: vec![hit],
        truncated: false,
    };
    assert!(palette.answer(asked.serial, Answer::Search(Ok(result.clone())), 0));
    assert_eq!(
        palette.picked(),
        Some(Action::OpenSearchHit {
            path: "src/a.rs".into(),
            line: 3
        })
    );
    let rows = palette.rows();
    let marked = 4..6;
    assert_eq!(rows[0].ranges, [marked]);
    assert_eq!(rows[0].detail, Some("src/a.rs:3"));
    // Back to one char: the hits go, and a late answer for them is dropped.
    palette.key(Key::Edit(DialogKey::Backspace));
    palette.wanted();
    assert!(palette.rows().is_empty());
    assert!(!palette.answer(asked.serial, Answer::Search(Ok(result)), 0));
}

#[test]
fn branches_are_asked_once_and_filtered_here() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Branches, vec![]);
    let asked = palette.wanted();
    assert_eq!(scopes(&asked), [Scope::Branches]);
    let Call::Branches(params) = &asked[0].call else {
        panic!("{:?}", asked[0].call)
    };
    assert_eq!(params.repo, None);
    let listed = vec![
        branch("main", false, true, 1_000),
        branch("feature-x", false, false, 500),
        branch("origin/feature-y", true, false, 400),
    ];
    assert!(palette.answer(asked[0].serial, Answer::Branches(Ok(listed)), 60_000_000));
    palette.insert("fe");
    assert_eq!(palette.wanted(), [], "filtered here, never asked again");
    let rows = palette.rows();
    let titles: Vec<&str> = rows.iter().map(|row| row.entry.title.as_str()).collect();
    assert!(titles.contains(&"feature-x") && titles.contains(&"origin/feature-y"));
    assert!(!titles.contains(&"main"));
    assert!(rows.iter().all(|row| row.age.is_some()));
    assert_eq!(
        palette.picked(),
        Some(Action::Checkout {
            repo: None,
            branch: "feature-x".into()
        })
    );
}

#[test]
fn commits_are_one_log_of_500_found_by_message_or_hash() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Commits, vec![]);
    let asked = palette.wanted().remove(0);
    let Call::Commits(params) = &asked.call else {
        panic!("{:?}", asked.call)
    };
    assert_eq!((params.limit, params.path.clone()), (COMMIT_LIMIT, None));
    let log = vec![
        log_commit("abcdef0123456789", "fix login", 0),
        log_commit("1234567890abcdef", "add tests", 0),
    ];
    assert!(palette.answer(asked.serial, Answer::Commits(Ok(log)), 0));
    palette.insert("abcd");
    assert_eq!(palette.rows().len(), 1, "found by its hash");
    assert_eq!(
        palette.picked(),
        Some(Action::ShowCommit {
            repo: None,
            sha: "abcdef0123456789".into()
        })
    );
    assert_eq!(palette.rows()[0].detail, Some("abcdef0"));
    palette.key(Key::Edit(DialogKey::SelectAll));
    palette.insert("tests");
    assert_eq!(palette.rows()[0].entry.title, "add tests");
}

#[test]
fn all_lists_a_scope_only_once_it_has_rows_and_never_shows_waits() {
    let mut palette = Palette::scoped(vec![command(0)], Some(source()), Scope::All, vec![]);
    palette.insert("t");
    let asked = palette.wanted();
    assert_eq!(
        scopes(&asked),
        [Scope::Files, Scope::Branches, Scope::Commits]
    );
    // Files, Branches and Commits are still loading: All has no "Loading" lines, only rows.
    assert_eq!(headers(&palette), ["Commands"]);
    let branches = asked.iter().find(|f| f.scope == Scope::Branches).unwrap();
    palette.answer(
        branches.serial,
        Answer::Branches(Ok(vec![branch("feature-x", false, false, 0)])),
        0,
    );
    assert_eq!(headers(&palette), ["Commands", "Branches"]);
    // A failed scope shows its error in its own tab only.
    let files = asked.iter().find(|f| f.scope == Scope::Files).unwrap();
    palette.answer(
        files.serial,
        Answer::Files(Err("no such workspace".into())),
        0,
    );
    assert_eq!(headers(&palette), ["Commands", "Branches"]);
    palette.set_scope(Scope::Files);
    assert_eq!(
        palette.lines(),
        [Line::Note {
            text: "no such workspace".into(),
            error: true
        }]
    );
}

#[test]
fn a_background_answer_keeps_the_pick() {
    let entries: Vec<Entry> = (0..3).map(command).collect();
    let mut palette = Palette::scoped(entries, Some(source()), Scope::All, vec![]);
    palette.insert("t");
    let asked = palette.wanted();
    palette.step(1);
    let before = palette.picked();
    assert!(before.is_some());
    let branches = asked.iter().find(|f| f.scope == Scope::Branches).unwrap();
    palette.answer(
        branches.serial,
        Answer::Branches(Ok(vec![branch("t1", false, false, 0)])),
        0,
    );
    assert_eq!(palette.picked(), before);
}

#[test]
fn an_answer_for_another_scope_keeps_the_pick() {
    let entries: Vec<Entry> = (0..3).map(command).collect();
    let mut palette = Palette::scoped(entries, Some(source()), Scope::All, vec![]);
    palette.insert("t");
    let asked = palette.wanted();
    let files = asked.iter().find(|f| f.scope == Scope::Files).unwrap();
    palette.step(1);
    let before = palette.picked();
    assert!(before.is_some());
    assert!(palette.answer(files.serial, Answer::Files(Ok(matches_of(&["t.rs"]))), 0));
    assert_eq!(palette.picked(), before, "Files is not on screen in All");

    // Tab to Branches: a late Files answer does not move its pick either.
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::All, vec![]);
    palette.insert("t");
    let asked = palette.wanted();
    let files = asked.iter().find(|f| f.scope == Scope::Files).unwrap();
    palette.set_scope(Scope::Branches);
    let branches = asked.iter().find(|f| f.scope == Scope::Branches).unwrap();
    let listed = vec![branch("t1", false, false, 0), branch("t2", false, false, 0)];
    assert!(palette.answer(branches.serial, Answer::Branches(Ok(listed)), 0));
    palette.step(1);
    let before = palette.picked();
    assert!(before.is_some());
    assert!(palette.answer(files.serial, Answer::Files(Ok(matches_of(&["t.rs"]))), 0));
    assert_eq!(palette.picked(), before);
}

#[test]
fn a_background_answer_keeps_a_picked_more_row() {
    let entries: Vec<Entry> = (0..8).map(command).collect();
    let mut palette = Palette::scoped(entries, Some(source()), Scope::All, vec![]);
    palette.insert("e");
    let asked = palette.wanted();
    let pick = palette
        .lines()
        .into_iter()
        .find_map(|line| match line {
            Line::More { pick, .. } => Some(pick),
            _ => None,
        })
        .unwrap();
    palette.pick(pick);
    assert_eq!(palette.picked_more(), Some(Scope::Commands));
    let branches = asked.iter().find(|f| f.scope == Scope::Branches).unwrap();
    palette.answer(
        branches.serial,
        Answer::Branches(Ok(vec![branch("e1", false, false, 0)])),
        0,
    );
    assert_eq!(palette.picked_more(), Some(Scope::Commands));
}

#[test]
fn search_counts_trimmed_characters() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Search, vec![]);
    palette.insert("a ");
    assert_eq!(
        palette.wanted(),
        [],
        "one character and a space is not searched"
    );
}

#[test]
fn a_waiting_or_failed_scope_says_so() {
    let mut palette = Palette::scoped(vec![], Some(source()), Scope::Files, vec![]);
    let asked = palette.wanted().remove(0);
    assert_eq!(
        palette.lines(),
        [Line::Note {
            text: "Loading…".into(),
            error: false
        }]
    );
    palette.answer(
        asked.serial,
        Answer::Files(Err("no such workspace".into())),
        0,
    );
    assert_eq!(
        palette.lines(),
        [Line::Note {
            text: "no such workspace".into(),
            error: true
        }]
    );
    assert_eq!(palette.picked(), None);
}

#[test]
fn without_a_workspace_the_daemon_scopes_ask_for_one() {
    for scope in [Scope::Files, Scope::Search, Scope::Branches, Scope::Commits] {
        let mut palette = Palette::scoped(vec![], None, scope, vec![]);
        assert_eq!(palette.wanted(), []);
        assert_eq!(
            palette.lines(),
            [Line::Note {
                text: "Open a workspace to search it.".into(),
                error: false
            }],
            "{scope:?}"
        );
    }
}

#[test]
fn the_at_picker_lists_files_and_hands_the_path_to_its_prompt() {
    let target = crate::composer::Target::TaskDialog;
    let mut palette = Palette::file_ref(source(), target);
    assert_eq!(palette.insert_into(), Some(target));
    assert_eq!((palette.scope(), palette.tabs()), (Scope::Files, false));
    let asked = palette.wanted();
    assert_eq!(scopes(&asked), [Scope::Files]);
    palette.answer(
        asked[0].serial,
        Answer::Files(Ok(matches_of(&["src/a.rs"]))),
        0,
    );
    assert_eq!(
        palette.picked(),
        Some(Action::OpenFile {
            path: "src/a.rs".into()
        })
    );
    assert_eq!(Palette::new(vec![]).insert_into(), None);
}

#[test]
fn an_empty_all_lists_sessions_then_the_recent_commands() {
    let entries = vec![
        command(0),
        command(1),
        command(2),
        session_entry(0),
        session_entry(1),
    ];
    let recent = vec![Action::Theme(2), Action::Theme(0)];
    let palette = Palette::scoped(entries, None, Scope::All, recent);
    assert_eq!(headers(&palette), ["Sessions", "Recent"]);
    let titles: Vec<String> = palette
        .rows()
        .iter()
        .map(|row| row.entry.title.clone())
        .collect();
    assert_eq!(
        titles,
        [
            "Go to session: s0",
            "Go to session: s1",
            "Theme 2",
            "Theme 0"
        ]
    );
}

#[test]
fn every_row_kind_yields_its_action() {
    let one = |scope: Scope, answer: Answer| {
        let mut palette = Palette::scoped(vec![], Some(source()), scope, vec![]);
        palette.insert("ab");
        let serial = palette.wanted().remove(0).serial;
        assert!(palette.answer(serial, answer, 0));
        palette.picked()
    };
    assert_eq!(
        one(Scope::Files, Answer::Files(Ok(matches_of(&["ab.rs"])))),
        Some(Action::OpenFile {
            path: "ab.rs".into()
        })
    );
    let hit = SearchHit {
        path: "ab.rs".into(),
        line: 7,
        preview: "ab".into(),
        ranges: vec![],
    };
    let result = FsSearchResult {
        hits: vec![hit],
        truncated: false,
    };
    assert_eq!(
        one(Scope::Search, Answer::Search(Ok(result))),
        Some(Action::OpenSearchHit {
            path: "ab.rs".into(),
            line: 7
        })
    );
    assert_eq!(
        one(
            Scope::Branches,
            Answer::Branches(Ok(vec![branch("ab", false, false, 0)]))
        ),
        Some(Action::Checkout {
            repo: None,
            branch: "ab".into()
        })
    );
    assert_eq!(
        one(
            Scope::Commits,
            Answer::Commits(Ok(vec![log_commit("ab12", "ab", 0)]))
        ),
        Some(Action::ShowCommit {
            repo: None,
            sha: "ab12".into()
        })
    );
    let mut commands = Palette::scoped(vec![command(4)], None, Scope::Commands, vec![]);
    commands.insert("theme");
    assert_eq!(commands.picked(), Some(Action::Theme(4)));
    let mut sessions = Palette::scoped(vec![session_entry(1)], None, Scope::Sessions, vec![]);
    sessions.insert("s1");
    assert!(matches!(
        sessions.picked(),
        Some(Action::GoTo(Target::Session(_)))
    ));
}

#[test]
fn a_checkout_asks_only_when_something_is_at_risk() {
    assert_eq!(checkout_question("main", 0, 0), None);
    let (message, detail) = checkout_question("main", 2, 0).unwrap();
    assert_eq!(message, "Switch to “main”?");
    assert_eq!(
        detail,
        "2 files with uncommitted changes. Git refuses the switch when a change would be overwritten."
    );
    let (_, detail) = checkout_question("main", 1, 0).unwrap();
    assert!(
        detail.starts_with("1 file with uncommitted changes."),
        "{detail}"
    );
    let (_, detail) = checkout_question("main", 0, 1).unwrap();
    assert_eq!(
        detail,
        "1 live agent session runs in this folder; its files change under it."
    );
    let (_, detail) = checkout_question("main", 0, 3).unwrap();
    assert_eq!(
        detail,
        "3 live agent sessions run in this folder; their files change under them."
    );
    let (_, both) = checkout_question("main", 1, 2).unwrap();
    assert_eq!(both.lines().count(), 2);
    assert!(confirms_checkout(Some(0)));
    assert!(!confirms_checkout(Some(1)) && !confirms_checkout(None));
}

#[test]
fn agents_count_in_the_folder_the_checkout_changes() {
    let (app, other, task) = (Ulid::new(), Ulid::new(), Ulid::new());
    let live = |project, task_id| Session {
        task_id,
        ..session(project, None, SessionState::Working)
    };
    let shell = Session {
        kind: SessionKind::Shell,
        ..session(app, None, SessionState::Idle)
    };
    let sessions = vec![
        live(app, None),
        live(app, Some(task)),
        live(app, Some(Ulid::new())),
        live(other, None),
        session(app, None, SessionState::Exited),
        shell,
    ];
    // A task that has a worktree runs there, not in the project folder.
    let has_worktree = |id: Ulid| id == task;
    assert_eq!(agents_in_folder(&sessions, app, None, has_worktree), 2);
    assert_eq!(
        agents_in_folder(&sessions, app, Some(task), has_worktree),
        1
    );
    assert_eq!(agents_in_folder(&sessions, other, None, has_worktree), 1);
}

#[test]
fn the_recent_commands_are_the_last_five_distinct_runs() {
    let mut recent = Vec::new();
    remember(&mut recent, &Action::GoTo(Target::Workspace(Ulid::new())));
    assert!(recent.is_empty(), "only commands are remembered");
    for command in [
        Command::NewTask,
        Command::OpenSettings,
        Command::NewTask,
        Command::ShowBoard,
        Command::ShowDiff,
        Command::NextTab,
        Command::PreviousTab,
    ] {
        remember(&mut recent, &Action::Run(command));
    }
    assert_eq!(
        recent,
        [
            Action::Run(Command::PreviousTab),
            Action::Run(Command::NextTab),
            Action::Run(Command::ShowDiff),
            Action::Run(Command::ShowBoard),
            Action::Run(Command::NewTask),
        ]
    );
}
