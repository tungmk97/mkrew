use serde_json::json;

use super::*;

fn wire(project_id: Ulid, title: &str, position: f64, layout: serde_json::Value) -> TerminalTab {
    TerminalTab {
        id: Ulid::new(),
        project_id,
        title: title.to_owned(),
        position,
        layout,
        pinned: false,
    }
}

fn pane(id: Ulid) -> serde_json::Value {
    json!({"session": id.to_string()})
}

#[test]
fn load_orders_by_position_and_activates_the_first_tab() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let mut tabs = Tabs::new(project);
    let second = wire(project, "two", 2.0, pane(b));
    let first = wire(project, "one", 1.0, pane(a));
    let foreign = wire(Ulid::new(), "other", 0.0, serde_json::Value::Null);
    tabs.load(vec![second.clone(), first.clone(), foreign]);

    let titles: Vec<&str> = tabs.tabs().iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["one", "two"]);
    assert_eq!(tabs.active().map(|t| t.id), Some(first.id));
    assert_eq!(tabs.visible(), vec![a]);
    assert_eq!(tabs.active().and_then(Tab::focused), Some(a));
}

#[test]
fn load_keeps_active_tab_and_focus() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(b), pane(c)]});
    let two = wire(project, "two", 2.0, split);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    assert!(tabs.focus(c));

    tabs.load(vec![one, two.clone()]);
    assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
    assert_eq!(tabs.active().and_then(Tab::focused), Some(c));
}

#[test]
fn hidden_window_shows_no_session() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "main", 1.0, split)]);

    assert_eq!(on_screen(true, Some(&tabs)), vec![a, b]);
    assert!(on_screen(false, Some(&tabs)).is_empty());
    assert!(on_screen(true, None).is_empty());
    assert!(on_screen(false, None).is_empty());
}

#[test]
fn unreadable_layout_loads_as_an_empty_tab() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(
        project,
        "broken",
        1.0,
        json!({"split": "diagonal"}),
    )]);
    assert_eq!(tabs.tabs()[0].layout(), None);
    assert!(tabs.visible().is_empty());
}

#[test]
fn split_close_and_persist_round_trip() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let tab = wire(project, "main", 1.0, serde_json::Value::Null);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);

    let first = tabs.split_focused(Axis::Vertical, a).unwrap();
    assert_eq!(first.layout, pane(a));
    tabs.split_focused(Axis::Vertical, b).unwrap();
    let write = tabs.split_focused(Axis::Horizontal, c).unwrap();
    assert_eq!(write.tab_id, tab.id);
    assert_eq!(
        write.layout,
        json!({"split":"vertical","ratio":0.5,"children":[
            pane(a), {"split":"horizontal","ratio":0.5,"children":[pane(b), pane(c)]}
        ]})
    );
    assert_eq!(tabs.active().and_then(Tab::focused), Some(c));
    // A session is shown in one pane only.
    assert_eq!(tabs.split_focused(Axis::Vertical, a), None);

    // Reloading what was written restores the same tree (UI restart).
    let mut restored = Tabs::new(project);
    restored.load(vec![TerminalTab {
        layout: write.layout.clone(),
        ..tab.clone()
    }]);
    assert_eq!(restored.visible(), vec![a, b, c]);

    let write = tabs.close_pane(c).unwrap();
    assert_eq!(
        write.layout,
        json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]})
    );
    assert_eq!(tabs.active().and_then(Tab::focused), Some(b));
    tabs.close_pane(b).unwrap();
    let write = tabs.close_pane(a).unwrap();
    assert_eq!(write.layout, serde_json::Value::Null);
    assert_eq!(tabs.active().and_then(Tab::focused), None);
    assert_eq!(tabs.close_pane(a), None);
}

/// A tab `a | b` (side by side), focused on `b`.
fn two_panes() -> (Tabs, Ulid, Ulid) {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "main", 1.0, serde_json::Value::Null)]);
    tabs.split_focused(Axis::Vertical, a).unwrap();
    tabs.split_focused(Axis::Vertical, b).unwrap();
    (tabs, a, b)
}

#[test]
fn arrange_keeps_the_panes_in_order_and_adds_the_new_ones() {
    let project = Ulid::new();
    let [a, b, c, d] = [Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new()];
    let stacked = json!({"split":"horizontal","ratio":0.7,"children":[pane(a), pane(b)]});
    let tab = wire(project, "main", 1.0, stacked);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    assert!(tabs.focus(b));
    assert!(tabs.toggle_zoom());

    let write = tabs.arrange(tab.id, Preset::Grid, &[c, d]).unwrap();
    assert_eq!(
        write.layout,
        json!({"split":"vertical","ratio":0.5,"children":[
            {"split":"horizontal","ratio":0.5,"children":[pane(a), pane(b)]},
            {"split":"horizontal","ratio":0.5,"children":[pane(c), pane(d)]}
        ]})
    );
    let active = tabs.active().unwrap();
    assert_eq!(active.focused(), Some(b), "the focus stays");
    assert_eq!(active.zoomed(), None, "the zoom ends");
    let again = tabs.arrange(tab.id, Preset::Grid, &[]).unwrap();
    assert_eq!(again.layout, write.layout, "every pane stays where it is");
}

/// The tab may change while the new sessions start (Review Focus 5): nothing is lost or
/// doubled, and the tab need not be the active one.
#[test]
fn arrange_refuses_other_sizes_and_works_on_any_tab() {
    let project = Ulid::new();
    let [a, b, c, e] = [Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new()];
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]});
    let one = wire(project, "one", 1.0, split);
    // Tab two: the daemon placed `c`, the first session started for it, already.
    let two = wire(project, "two", 2.0, pane(c));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    assert_eq!(
        tabs.arrange(one.id, Preset::Columns3, &[]),
        None,
        "a pane short"
    );
    assert_eq!(
        tabs.arrange(one.id, Preset::Columns2, &[e]),
        None,
        "a pane too many"
    );
    assert_eq!(
        tabs.arrange(one.id, Preset::Columns3, &[c]),
        None,
        "c is in tab two"
    );
    assert_eq!(tabs.tab(one.id).unwrap().sessions(), [a, b], "unchanged");

    let write = tabs.arrange(two.id, Preset::Columns2, &[c, e]).unwrap();
    assert_eq!(
        write.layout,
        json!({"split":"vertical","ratio":0.5,"children":[pane(c), pane(e)]})
    );
    assert_eq!(
        tabs.active().map(|tab| tab.id),
        Some(one.id),
        "still on tab one"
    );
}

#[test]
fn resize_writes_the_moved_divider() {
    let (mut tabs, a, b) = two_panes();
    let write = tabs.resize_focused(Direction::Left).unwrap();
    assert_eq!(
        write.layout,
        json!({"split":"vertical","ratio":0.45,"children":[pane(a), pane(b)]})
    );
    // No stacked split: nothing moves, nothing is written.
    assert_eq!(tabs.resize_focused(Direction::Up), None);
    // The echo of the write keeps the local layout.
    let echo = TerminalTab {
        id: write.tab_id,
        project_id: tabs.project_id(),
        title: "main".into(),
        position: 1.0,
        layout: write.layout.clone(),
        pinned: false,
    };
    tabs.apply(&Event::TabUpdated(echo));
    let layout = tabs.active().and_then(Tab::layout);
    assert_eq!(Node::to_json(layout), write.layout);
}

#[test]
fn zoom_shows_only_the_focused_pane_until_focus_or_layout_changes() {
    let (mut tabs, a, b) = two_panes();
    assert!(tabs.toggle_zoom());
    assert_eq!(tabs.active().and_then(Tab::zoomed), Some(b));
    assert_eq!(tabs.visible(), vec![b]);
    let rects = tabs.active().unwrap().rects();
    assert_eq!(rects.len(), 1);
    assert_eq!((rects[0].0, rects[0].1.w, rects[0].1.h), (b, 1.0, 1.0));
    // Hidden dividers are not resized while zoomed.
    assert_eq!(tabs.resize_focused(Direction::Left), None);
    assert!(tabs.toggle_zoom());
    assert_eq!(tabs.visible(), vec![a, b]);

    // Moving the focus ends the zoom.
    tabs.toggle_zoom();
    assert!(tabs.focus_neighbour(Direction::Left));
    assert_eq!(tabs.active().and_then(Tab::zoomed), None);
    assert_eq!(tabs.visible(), vec![a, b]);

    // So does a new pane (it must be visible) and closing the zoomed pane.
    tabs.toggle_zoom();
    let c = Ulid::new();
    tabs.split_focused(Axis::Horizontal, c).unwrap();
    assert_eq!(tabs.visible(), vec![a, c, b]);
    tabs.toggle_zoom();
    tabs.close_pane(c).unwrap();
    assert_eq!(tabs.active().and_then(Tab::zoomed), None);
    assert_eq!(tabs.visible(), vec![a, b]);

    // Focusing the zoomed pane from the sidebar keeps the zoom; another pane ends it.
    tabs.toggle_zoom();
    assert!(tabs.focus(a));
    assert_eq!(tabs.visible(), vec![a]);
    assert!(tabs.focus(b));
    assert_eq!(tabs.visible(), vec![a, b]);
}

#[test]
fn a_single_pane_does_not_zoom() {
    let (mut tabs, a, _) = two_panes();
    let mut single = Tabs::new(tabs.project_id());
    assert!(!single.toggle_zoom());
    tabs.close_pane(a).unwrap();
    assert!(!tabs.toggle_zoom());
    assert_eq!(tabs.active().and_then(Tab::zoomed), None);
}

#[test]
fn rename_params_trim_and_skip_blank_or_unchanged_titles() {
    let (tabs, ..) = two_panes();
    let tab = tabs.active().unwrap();
    let params = rename_params(tab, "  build  ").unwrap();
    assert_eq!(params.tab_id, tab.id);
    assert_eq!(params.title, "build");
    assert_eq!(rename_params(tab, "   "), None);
    assert_eq!(rename_params(tab, " main "), None);
}

#[test]
fn focus_moves_between_neighbours() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "main", 1.0, pane(a))]);
    tabs.split_focused(Axis::Vertical, b);
    assert!(tabs.focus_neighbour(Direction::Left));
    assert_eq!(tabs.active().and_then(Tab::focused), Some(a));
    assert!(!tabs.focus_neighbour(Direction::Left));
    assert!(!tabs.focus_neighbour(Direction::Up));
    assert!(tabs.focus_neighbour(Direction::Right));
    assert_eq!(tabs.active().and_then(Tab::focused), Some(b));
}

#[test]
fn echoes_of_own_writes_do_not_revert_newer_local_layouts() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let tab = wire(project, "main", 1.0, serde_json::Value::Null);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    let first = tabs.split_focused(Axis::Vertical, a).unwrap();
    tabs.split_focused(Axis::Vertical, b).unwrap();

    // The echo of the first write arrives after the second local edit.
    let echo = |layout: serde_json::Value| {
        Event::TabUpdated(TerminalTab {
            layout,
            ..tab.clone()
        })
    };
    tabs.apply(&echo(first.layout));
    assert_eq!(tabs.visible(), vec![a, b]);
    tabs.apply(&echo(
        json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]}),
    ));
    assert_eq!(tabs.visible(), vec![a, b]);

    // With nothing pending, an update from elsewhere (e.g. `mk` spawning) applies.
    assert!(tabs.apply(&echo(pane(b))));
    assert_eq!(tabs.visible(), vec![b]);
    assert_eq!(tabs.active().and_then(Tab::focused), Some(b));
}

#[test]
fn failed_write_stops_waiting_for_its_echo() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let tab = wire(project, "main", 1.0, serde_json::Value::Null);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    let write = tabs.split_focused(Axis::Vertical, a).unwrap();
    tabs.write_failed(&write);
    assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
        layout: pane(b),
        ..tab
    })));
    assert_eq!(tabs.visible(), vec![b]);
}

#[test]
fn rename_while_a_write_is_pending_keeps_the_local_layout() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let tab = wire(project, "main", 1.0, pane(a));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    let write = tabs.split_focused(Axis::Vertical, b).unwrap();

    // A rename (e.g. from `mk`) processed before the write carries the old layout.
    assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
        title: "renamed".to_owned(),
        ..tab.clone()
    })));
    assert_eq!(tabs.visible(), vec![a, b]);
    assert_eq!(tabs.active().map(|t| t.title.as_str()), Some("renamed"));

    // The echo still clears the pending write; later remote layouts apply again.
    tabs.apply(&Event::TabUpdated(TerminalTab {
        title: "renamed".to_owned(),
        layout: write.layout,
        ..tab.clone()
    }));
    assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
        title: "renamed".to_owned(),
        ..tab
    })));
    assert_eq!(tabs.visible(), vec![a]);
}

#[test]
fn updates_add_rename_and_reorder_tabs() {
    let project = Ulid::new();
    let one = wire(project, "one", 1.0, serde_json::Value::Null);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone()]);

    let zero = wire(project, "zero", 0.5, serde_json::Value::Null);
    assert!(tabs.apply(&Event::TabUpdated(zero.clone())));
    assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
        title: "renamed".to_owned(),
        ..one.clone()
    })));
    let titles: Vec<&str> = tabs.tabs().iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["zero", "renamed"]);
    // The active tab stays the same when another one is added before it.
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    // Unchanged and foreign updates are no-ops.
    assert!(!tabs.apply(&Event::TabUpdated(zero)));
    assert!(!tabs.apply(&Event::TabUpdated(wire(
        Ulid::new(),
        "x",
        0.0,
        serde_json::Value::Null
    ))));
}

/// Pinned tabs sit left of every unpinned one, each group in position order. Pinning
/// never changes `position`, so unpinning puts the tab back at its own place. The active
/// tab follows its id when the order changes.
#[test]
fn pinned_tabs_lead_and_unpinning_returns_to_position() {
    let project = Ulid::new();
    let [one, two, three] = [
        wire(project, "one", 1.0, serde_json::Value::Null),
        wire(project, "two", 2.0, serde_json::Value::Null),
        wire(project, "three", 3.0, serde_json::Value::Null),
    ];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone(), three.clone()]);
    let titles =
        |tabs: &Tabs| -> Vec<String> { tabs.tabs().iter().map(|t| t.title.clone()).collect() };

    assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
        pinned: true,
        ..three.clone()
    })));
    assert_eq!(titles(&tabs), ["three", "one", "two"]);
    assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
        pinned: true,
        ..two.clone()
    })));
    assert_eq!(titles(&tabs), ["two", "three", "one"]);
    assert!(tabs.tabs()[0].pinned && !tabs.tabs()[2].pinned);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));

    assert!(tabs.apply(&Event::TabUpdated(TerminalTab {
        pinned: false,
        ..three.clone()
    })));
    assert_eq!(titles(&tabs), ["two", "one", "three"]);

    // A fresh list puts pinned tabs first too.
    let mut reloaded = Tabs::new(project);
    reloaded.load(vec![
        one,
        two,
        TerminalTab {
            pinned: true,
            ..three
        },
    ]);
    assert_eq!(titles(&reloaded), ["three", "one", "two"]);
}

#[test]
fn remove_and_cycle_tabs() {
    let project = Ulid::new();
    let [one, two, three] = [
        wire(project, "1", 1.0, serde_json::Value::Null),
        wire(project, "2", 2.0, serde_json::Value::Null),
        wire(project, "3", 3.0, serde_json::Value::Null),
    ];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone(), three.clone()]);
    tabs.activate_offset(-1);
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    tabs.activate_offset(1);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert!(tabs.activate(two.id));
    assert!(!tabs.activate(Ulid::new()));
    assert!(tabs.activate_nth(2));
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    assert!(!tabs.activate_nth(3));
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    assert!(tabs.activate_nth(1));

    tabs.remove(two.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    tabs.remove(three.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    tabs.remove(one.id);
    assert!(tabs.active().is_none());
    assert!(tabs.is_empty());
    tabs.activate_offset(1);
}

#[test]
fn removing_an_inactive_tab_keeps_the_active_one() {
    let project = Ulid::new();
    let [one, two, three] = [
        wire(project, "1", 1.0, serde_json::Value::Null),
        wire(project, "2", 2.0, serde_json::Value::Null),
        wire(project, "3", 3.0, serde_json::Value::Null),
    ];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone(), three.clone()]);
    assert!(tabs.activate(three.id));
    tabs.remove(one.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    assert_eq!(tabs.tabs().len(), 2);
}

/// A dragged tab lands between its new neighbors (or past the ends) of its own pin group, and
/// the strip reorders at once.
#[test]
fn moving_a_tab_stays_in_its_pin_group() {
    let project = Ulid::new();
    let [pin, one, two, three] = [
        TerminalTab {
            pinned: true,
            ..wire(project, "pin", 9.0, serde_json::Value::Null)
        },
        wire(project, "one", 1.0, serde_json::Value::Null),
        wire(project, "two", 2.0, serde_json::Value::Null),
        wire(project, "three", 3.0, serde_json::Value::Null),
    ];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![pin.clone(), one.clone(), two, three.clone()]);
    let titles =
        |tabs: &Tabs| -> Vec<String> { tabs.tabs().iter().map(|t| t.title.clone()).collect() };

    let sent = tabs.move_tab(three.id, 0).unwrap();
    assert_eq!((sent.tab_id, sent.position), (three.id, 0.0));
    assert_eq!(titles(&tabs), ["pin", "three", "one", "two"]);
    assert_eq!(tabs.move_tab(three.id, 1).unwrap().position, 1.5);
    assert_eq!(titles(&tabs), ["pin", "one", "three", "two"]);
    // Past the end clamps to last.
    assert_eq!(tabs.move_tab(one.id, 9).unwrap().position, 3.0);
    assert_eq!(titles(&tabs), ["pin", "three", "two", "one"]);
    // The only pinned tab has nowhere to go.
    assert!(tabs.move_tab(pin.id, 1).is_none());
    assert!(tabs.move_tab(Ulid::new(), 0).is_none());
}

#[test]
fn close_others_and_to_the_right_keep_pinned_tabs() {
    let project = Ulid::new();
    let [pin, one, two, three] = [
        TerminalTab {
            pinned: true,
            ..wire(project, "pin", 9.0, serde_json::Value::Null)
        },
        wire(project, "one", 1.0, serde_json::Value::Null),
        wire(project, "two", 2.0, serde_json::Value::Null),
        wire(project, "three", 3.0, serde_json::Value::Null),
    ];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![pin.clone(), one.clone(), two.clone(), three.clone()]);
    assert_eq!(tabs.close_targets(two.id, false), [one.id, three.id]);
    assert_eq!(tabs.close_targets(two.id, true), [three.id]);
    assert!(tabs.close_targets(three.id, true).is_empty());
    // From a pinned tab: every unpinned one is to its right.
    assert_eq!(tabs.close_targets(pin.id, true), [one.id, two.id, three.id]);
    assert!(tabs.close_targets(Ulid::new(), false).is_empty());
}

#[test]
fn prune_drops_panes_of_unknown_sessions() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let split = json!({"split":"horizontal","ratio":0.5,"children":[pane(a), pane(b)]});
    let one = wire(project, "one", 1.0, split);
    let two = wire(project, "two", 2.0, pane(c));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    let writes = tabs.prune(|id| id == a);
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].tab_id, one.id);
    assert_eq!(writes[0].layout, pane(a));
    assert_eq!(writes[1].tab_id, two.id);
    assert_eq!(writes[1].layout, serde_json::Value::Null);
    assert!(tabs.prune(|_| true).is_empty());
}

#[test]
fn focus_activates_the_holding_tab() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(project, "two", 2.0, pane(b));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one, two.clone()]);
    assert!(tabs.focus(b));
    assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
    assert_eq!(tabs.tab_of(b).map(|t| t.id), Some(two.id));
    assert!(!tabs.focus(Ulid::new()));
}

fn session(project_id: Ulid, state: SessionState, started_at: i64) -> Session {
    Session {
        id: Ulid::new(),
        task_id: None,
        project_id,
        agent_profile: "fake".to_owned(),
        kind: mkrew_protocol::SessionKind::Agent,
        title: None,
        terminal_title: None,
        color: None,
        native_session_id: None,
        state,
        state_source: mkrew_protocol::StateSource::Daemon,
        state_changed_at: started_at,
        exit_code: None,
        cols: 80,
        rows: 24,
        started_at,
        ended_at: None,
        pinned: false,
        position: 0.0,
        plan_path: None,
    }
}

#[test]
fn empty_tab_takes_the_newest_live_unplaced_session() {
    let project = Ulid::new();
    let placed = session(project, SessionState::Working, 50);
    let older = session(project, SessionState::Idle, 10);
    let newest = session(project, SessionState::NeedsInput, 30);
    let exited = session(project, SessionState::Exited, 40);
    let foreign = session(Ulid::new(), SessionState::Idle, 60);
    let sessions = [
        placed.clone(),
        older.clone(),
        newest.clone(),
        exited,
        foreign,
    ];
    let mut tabs = Tabs::new(project);
    assert_eq!(newest_unplaced(&tabs, &sessions), None, "no active tab");
    let one = wire(project, "one", 1.0, pane(placed.id));
    let two = wire(project, "two", 2.0, serde_json::Value::Null);
    tabs.load(vec![one, two.clone()]);
    assert_eq!(
        newest_unplaced(&tabs, &sessions),
        None,
        "active tab has panes"
    );
    tabs.activate(two.id);
    assert_eq!(newest_unplaced(&tabs, &sessions), Some(newest.id));
    tabs.split_focused(Axis::Vertical, newest.id);
    assert_eq!(newest_unplaced(&tabs, &sessions), None);
}

#[test]
fn attach_diff_lists_new_and_hidden_sessions() {
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    assert_eq!(attach_diff(&[a, b], &[b, c]), (vec![c], vec![a]));
    assert_eq!(attach_diff(&[a], &[a]), (vec![], vec![]));
}

/// The icon of a tab follows its focused pane: an agent shows its profile, a shell `>_`,
/// a tab without panes (or with a pane no session is known for) no icon.
#[test]
fn a_tab_shows_the_kind_of_its_focused_pane() {
    let (mut tabs, a, b) = two_panes(); // focused on b
    let project = tabs.project_id();
    let mut agent = session(project, SessionState::Idle, 1);
    agent.id = a;
    let mut shell = session(project, SessionState::Idle, 2);
    shell.id = b;
    shell.kind = SessionKind::Shell;
    let sessions = [agent, shell];

    assert_eq!(tab_kind(tabs.active().unwrap(), &sessions), TabKind::Shell);
    tabs.focus(a);
    assert_eq!(
        tab_kind(tabs.active().unwrap(), &sessions),
        TabKind::Agent("fake".into())
    );
    assert_eq!(tab_kind(tabs.active().unwrap(), &[]), TabKind::Empty);

    let mut empty = Tabs::new(project);
    empty.load(vec![wire(project, "new", 1.0, serde_json::Value::Null)]);
    assert_eq!(tab_kind(empty.active().unwrap(), &sessions), TabKind::Empty);
}

#[test]
fn a_default_named_tab_with_one_pane_on_screen_shows_its_session_title() {
    let project = Ulid::new();
    let mut titled = session(project, SessionState::Idle, 1);
    titled.terminal_title = Some("Fix login".into());
    let untitled = session(project, SessionState::Idle, 2);
    let sessions = [titled.clone(), untitled.clone()];
    let label = |title: &str, layout| {
        let mut tabs = Tabs::new(project);
        tabs.load(vec![wire(project, title, 1.0, layout)]);
        session_label(tabs.active().unwrap(), &sessions).map(str::to_owned)
    };

    assert_eq!(
        label("Terminal", pane(titled.id)).as_deref(),
        Some("Fix login")
    );
    assert_eq!(
        label("Terminal 12", pane(titled.id)).as_deref(),
        Some("Fix login")
    );
    // A name the user gave the tab wins; so does the tab name without a session title.
    for title in ["Backend", "Terminal 3b", "Terminal ", "Terminals"] {
        assert_eq!(label(title, pane(titled.id)), None, "{title}");
    }
    assert_eq!(label("Terminal", pane(untitled.id)), None);

    // Several panes: each header shows its title; zoomed, the lone pane's shows on the tab.
    let split =
        json!({"split":"vertical","ratio":0.5,"children":[pane(untitled.id), pane(titled.id)]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal 2", 1.0, split)]);
    assert_eq!(session_label(tabs.active().unwrap(), &sessions), None);
    tabs.focus(titled.id);
    assert!(tabs.toggle_zoom());
    assert_eq!(
        session_label(tabs.active().unwrap(), &sessions),
        Some("Fix login")
    );
}
