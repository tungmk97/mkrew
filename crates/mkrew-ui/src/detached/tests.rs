use super::*;

fn term() -> DockTab {
    DockTab::Terminal {
        project: Ulid::new(),
        tab_id: Ulid::new(),
    }
}

#[test]
fn a_new_window_holds_its_tab_and_shows_it() {
    let mut detached = Detached::new();
    let tab = term();
    let key = detached.open(tab);
    assert_eq!(detached.host_of(&tab), HostId::Detached(key));
    assert_eq!(detached.active(key), Some(tab));
    assert!(detached.is_detached(&tab));
}

#[test]
fn a_tab_lives_in_one_window_only() {
    let mut detached = Detached::new();
    let (a, b) = (term(), term());
    let first = detached.open(a);
    let second = detached.open(b);
    detached.move_tab(a, HostId::Detached(second), Some(0));
    assert_eq!(detached.host_of(&a), HostId::Detached(second));
    assert_eq!(detached.tabs_of(second), vec![a, b]);
    assert!(
        detached.window(first).is_none(),
        "the emptied window is dropped"
    );
}

#[test]
fn moving_inserts_at_the_index_and_selects_the_tab() {
    let mut detached = Detached::new();
    let (a, b, c) = (term(), term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    detached.move_tab(c, HostId::Detached(key), Some(1));
    assert_eq!(detached.tabs_of(key), vec![a, c, b]);
    assert_eq!(detached.active(key), Some(c));
}

#[test]
fn moving_to_main_forgets_the_tab_and_reports_the_emptied_window() {
    let mut detached = Detached::new();
    let a = term();
    let key = detached.open(a);
    let moved = detached.move_tab(a, HostId::Main, None);
    assert_eq!(moved.from, HostId::Detached(key));
    assert_eq!(moved.emptied, Some(key));
    assert_eq!(detached.host_of(&a), HostId::Main);
}

#[test]
fn the_active_tab_passes_right_then_left_when_it_leaves() {
    let mut detached = Detached::new();
    let (a, b, c) = (term(), term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    detached.move_tab(c, HostId::Detached(key), None);
    assert!(detached.select(key, 1));
    detached.move_tab(b, HostId::Main, None);
    assert_eq!(detached.active(key), Some(c));
    detached.move_tab(c, HostId::Main, None);
    assert_eq!(detached.active(key), Some(a));
}

#[test]
fn reorder_and_cycle_stay_inside_the_window() {
    let mut detached = Detached::new();
    let (a, b, c) = (term(), term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    detached.move_tab(c, HostId::Detached(key), None);
    detached.move_within(key, 2, 0);
    assert_eq!(detached.tabs_of(key), vec![c, a, b]);
    assert!(detached.select(key, 2));
    detached.cycle(key, true);
    assert_eq!(detached.active(key), Some(c), "wraps to the first");
    detached.cycle(key, false);
    assert_eq!(detached.active(key), Some(b), "wraps to the last");
    assert!(!detached.select(key, 9));
}

#[test]
fn prune_keeps_tabs_of_unloaded_workspaces() {
    let mut detached = Detached::new();
    let (gone, unknown, kept) = (term(), term(), term());
    let key = detached.open(gone);
    detached.move_tab(unknown, HostId::Detached(key), None);
    let other = detached.open(kept);
    let emptied = detached.prune(|tab| match *tab {
        t if t == gone => Keep::No,
        t if t == unknown => Keep::Unknown,
        _ => Keep::Yes,
    });
    assert!(emptied.is_empty());
    assert_eq!(detached.tabs_of(key), vec![unknown]);
    assert_eq!(detached.tabs_of(other), vec![kept]);
    let emptied = detached.prune(|_| Keep::No);
    assert_eq!(emptied.len(), 2);
    assert!(detached.windows().is_empty());
}

#[test]
fn hidden_windows_show_no_tab() {
    let mut detached = Detached::new();
    let (a, b) = (term(), term());
    let one = detached.open(a);
    let two = detached.open(b);
    detached.set_visible(two, false);
    assert_eq!(detached.visible_tabs(), vec![a]);
    detached.set_visible(two, true);
    assert_eq!(detached.visible_tabs(), vec![a, b]);
    let _ = one;
}

#[test]
fn quit_closes_no_tab() {
    let mut detached = Detached::new();
    detached.open(term());
    assert!(detached.quit_closes().is_empty());
}

#[test]
fn removing_a_window_returns_its_tabs_in_order() {
    let mut detached = Detached::new();
    let (a, b) = (term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    assert_eq!(detached.remove_window(key), vec![a, b]);
    assert!(detached.window(key).is_none());
}

#[test]
fn window_keys_are_never_reused() {
    let mut detached = Detached::new();
    let a = term();
    let first = detached.open(a);
    detached.move_tab(a, HostId::Main, None);
    let second = detached.open(a);
    assert_ne!(first, second);
}

#[test]
fn moving_onto_its_own_single_tab_window_keeps_it() {
    let mut detached = Detached::new();
    let a = term();
    let key = detached.open(a);
    let moved = detached.move_tab(a, HostId::Detached(key), None);
    assert_eq!(
        moved,
        Moved {
            from: HostId::Detached(key),
            emptied: None
        }
    );
    assert_eq!(detached.tabs_of(key), vec![a]);
    assert_eq!(detached.active(key), Some(a));
}

#[test]
fn moving_onto_its_own_window_reorders_and_selects_it() {
    let mut detached = Detached::new();
    let (a, b, c) = (term(), term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    detached.move_tab(c, HostId::Detached(key), None);
    let moved = detached.move_tab(a, HostId::Detached(key), Some(2));
    assert_eq!(
        moved,
        Moved {
            from: HostId::Detached(key),
            emptied: None
        }
    );
    assert_eq!(detached.tabs_of(key), vec![b, c, a]);
    assert_eq!(detached.active(key), Some(a));
}

#[test]
fn moving_to_an_unknown_window_changes_nothing() {
    let mut detached = Detached::new();
    let (a, b) = (term(), term());
    let key = detached.open(a);
    let moved = detached.move_tab(a, HostId::Detached(WindowKey(99)), None);
    assert_eq!(
        moved,
        Moved {
            from: HostId::Detached(key),
            emptied: None
        }
    );
    assert_eq!(detached.tabs_of(key), vec![a]);
    let moved = detached.move_tab(b, HostId::Detached(WindowKey(99)), None);
    assert_eq!(
        moved,
        Moved {
            from: HostId::Main,
            emptied: None
        }
    );
    assert_eq!(detached.host_of(&b), HostId::Main);
    assert_eq!(detached.windows().len(), 1);
}

#[test]
fn open_takes_the_tab_from_its_window() {
    let mut detached = Detached::new();
    let (a, b) = (term(), term());
    let first = detached.open(a);
    detached.move_tab(b, HostId::Detached(first), None);
    let second = detached.open(b);
    assert_eq!(detached.tabs_of(first), vec![a]);
    assert_eq!(detached.host_of(&b), HostId::Detached(second));
    let third = detached.open(a);
    assert!(
        detached.window(first).is_none(),
        "the emptied window is dropped"
    );
    assert_eq!(detached.host_of(&a), HostId::Detached(third));
}

#[test]
fn move_within_keeps_the_shown_tab_selected() {
    let mut detached = Detached::new();
    let (a, b, c) = (term(), term(), term());
    let key = detached.open(a);
    detached.move_tab(b, HostId::Detached(key), None);
    detached.move_tab(c, HostId::Detached(key), None);
    assert!(detached.select(key, 1));
    detached.move_within(key, 0, 2);
    assert_eq!(detached.tabs_of(key), vec![b, c, a]);
    assert_eq!(detached.active(key), Some(b));
}

#[test]
fn detached_commands_are_an_explicit_list() {
    use crate::keymap::Command;
    use crate::split::{Direction, Preset};
    for command in [
        Command::NextTab,
        Command::PreviousTab,
        Command::Tab(3),
        Command::CloseTab,
        Command::MoveTabLeft,
        Command::MoveTabRight,
        Command::SplitRight,
        Command::SplitDown,
        Command::SplitAgent,
        Command::SplitShell,
        Command::Layout(Preset::Grid),
        Command::EqualizePanes,
        Command::Focus(Direction::Left),
        Command::Resize(Direction::Up),
        Command::ZoomPane,
        Command::ClosePane,
        Command::RenamePane,
        Command::RenameTab,
        Command::OpenComposer,
        Command::CommandPalette,
        Command::ContextMenu,
        Command::ToggleFullScreen,
        Command::MoveTabToNewWindow,
    ] {
        assert!(handles(command), "{command:?}");
    }
    for command in [
        Command::NewShellTab,
        Command::ShowBoard,
        Command::ToggleSidebar,
        Command::Workspace(1),
        Command::SaveFile,
        Command::VoiceInput,
        Command::NextInbox,
    ] {
        assert!(!handles(command), "{command:?}");
    }
}

fn saved(tabs: Vec<DockTab>) -> SavedWindow {
    SavedWindow {
        bounds: SavedBounds {
            x: 1920.0,
            y: 80.0,
            width: 1100.0,
            height: 760.0,
        },
        maximized: false,
        tabs,
        active: 0,
    }
}

#[test]
fn windows_round_trip() {
    let windows = vec![saved(vec![term(), term()])];
    let json = to_json(&windows);
    assert!(json.contains("\"version\":1"), "{json}");
    assert!(json.contains("\"terminal\""), "{json}");
    assert_eq!(from_json(&json), Ok(windows));
}

#[test]
fn unknown_version_and_broken_files_are_errors() {
    assert!(from_json(r#"{"version":2,"windows":[]}"#).is_err());
    assert!(from_json("not json").is_err());
    assert!(from_json("").is_err());
}

#[test]
fn unknown_tab_kinds_are_skipped_and_empty_windows_dropped() {
    let project = Ulid::new();
    let tab_id = Ulid::new();
    let json = format!(
        r#"{{"version":1,"windows":[
            {{"bounds":{{"x":0,"y":0,"width":800,"height":600}},"maximized":true,
              "tabs":[{{"file":{{"project":"{project}","path":"a.rs"}}}},
                      {{"terminal":{{"project":"{project}","tab_id":"{tab_id}"}}}}],"active":1}},
            {{"bounds":{{"x":0,"y":0,"width":800,"height":600}},"maximized":false,
              "tabs":[{{"file":{{"project":"{project}","path":"b.rs"}}}}],"active":0}}]}}"#
    );
    let windows = from_json(&json).unwrap();
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].tabs, vec![DockTab::Terminal { project, tab_id }]);
    assert_eq!(windows[0].active, 0, "re-based onto the kept tabs");
    assert!(windows[0].maximized);
}

#[test]
fn save_writes_through_a_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(WINDOWS_FILE);
    let windows = vec![saved(vec![term()])];
    save(&path, &windows).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(from_json(&text), Ok(windows));
    let names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "no temp file left: {names:?}");
}

#[test]
fn a_machine_window_has_its_own_file() {
    assert_eq!(windows_file(None), "windows.json");
    let file = windows_file(Some("machine:box"));
    assert!(
        file.starts_with("windows-") && file.ends_with(".json"),
        "{file}"
    );
    assert_ne!(file, windows_file(Some("machine:other")));
}

#[test]
fn restore_rebuilds_windows_and_skips_duplicates() {
    let (a, b) = (term(), term());
    let mut detached = Detached::new();
    let keys = detached.restore(&[
        SavedWindow {
            active: 1,
            ..saved(vec![a, b])
        },
        saved(vec![a]),
    ]);
    assert_eq!(
        keys.len(),
        1,
        "a tab already restored is not restored twice"
    );
    assert_eq!(detached.tabs_of(keys[0]), vec![a, b]);
    assert_eq!(detached.active(keys[0]), Some(b));
}
