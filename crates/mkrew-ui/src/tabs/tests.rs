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

fn k(id: Ulid) -> PaneKey {
    PaneKey::Session(id)
}

fn file(path: &str) -> serde_json::Value {
    json!({"file": path})
}

fn file_key(path: &str) -> PaneKey {
    PaneKey::File(path.to_owned())
}

/// A file leaf with every optional key set.
fn full_file() -> serde_json::Value {
    json!({"file": "README.md", "md": "split", "preview": true})
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
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(a));
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
    assert!(tabs.focus(&k(c)));

    tabs.load(vec![one, two.clone()]);
    assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(c));
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
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(c));
    // A session is shown in one pane only.
    assert_eq!(tabs.split_focused(Axis::Vertical, a), None);

    // Reloading what was written restores the same tree (UI restart).
    let mut restored = Tabs::new(project);
    restored.load(vec![TerminalTab {
        layout: write.layout.clone(),
        ..tab.clone()
    }]);
    assert_eq!(restored.visible(), vec![a, b, c]);

    let write = tabs.close_leaf(&k(c)).unwrap();
    assert_eq!(
        write.layout,
        json!({"split":"vertical","ratio":0.5,"children":[pane(a), pane(b)]})
    );
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(b));
    tabs.close_leaf(&k(b)).unwrap();
    let write = tabs.close_leaf(&k(a)).unwrap();
    assert_eq!(write.layout, serde_json::Value::Null);
    assert_eq!(tabs.active().and_then(Tab::focused_session), None);
    assert_eq!(tabs.close_leaf(&k(a)), None);
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
    assert!(tabs.focus(&k(b)));
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
    assert_eq!(active.focused_session(), Some(b), "the focus stays");
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
    assert_eq!(tabs.active().and_then(Tab::zoomed), Some(&k(b)));
    assert_eq!(tabs.visible(), vec![b]);
    let rects = tabs.active().unwrap().rects();
    assert_eq!(rects.len(), 1);
    assert_eq!(
        (rects[0].0.clone(), rects[0].1.w, rects[0].1.h),
        (k(b), 1.0, 1.0)
    );
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
    tabs.close_leaf(&k(c)).unwrap();
    assert_eq!(tabs.active().and_then(Tab::zoomed), None);
    assert_eq!(tabs.visible(), vec![a, b]);

    // Focusing the zoomed pane from the sidebar keeps the zoom; another pane ends it.
    tabs.toggle_zoom();
    assert!(tabs.focus(&k(a)));
    assert_eq!(tabs.visible(), vec![a]);
    assert!(tabs.focus(&k(b)));
    assert_eq!(tabs.visible(), vec![a, b]);
}

#[test]
fn a_single_pane_does_not_zoom() {
    let (mut tabs, a, _) = two_panes();
    let mut single = Tabs::new(tabs.project_id());
    assert!(!single.toggle_zoom());
    tabs.close_leaf(&k(a)).unwrap();
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
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(a));
    assert!(!tabs.focus_neighbour(Direction::Left));
    assert!(!tabs.focus_neighbour(Direction::Up));
    assert!(tabs.focus_neighbour(Direction::Right));
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(b));
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
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(b));
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
    tabs.activate_offset(-1, |_| false);
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    tabs.activate_offset(1, |_| false);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert!(tabs.activate(two.id));
    assert!(!tabs.activate(Ulid::new()));
    assert!(tabs.activate_nth(2, |_| false));
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    assert!(!tabs.activate_nth(3, |_| false));
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    assert!(tabs.activate_nth(1, |_| false));

    tabs.remove(two.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id));
    tabs.remove(three.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    tabs.remove(one.id);
    assert!(tabs.active().is_none());
    assert!(tabs.is_empty());
    tabs.activate_offset(1, |_| false);
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
fn a_tab_moves_before_another_within_its_pin_group() {
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
    let titles =
        |tabs: &Tabs| -> Vec<String> { tabs.tabs().iter().map(|t| t.title.clone()).collect() };

    tabs.move_tab_before(three.id, Some(two.id)).unwrap();
    assert_eq!(titles(&tabs), ["pin", "one", "three", "two"]);
    // No anchor: the end of its group.
    tabs.move_tab_before(one.id, None).unwrap();
    assert_eq!(titles(&tabs), ["pin", "three", "two", "one"]);
    // An anchor in the pinned group puts an unpinned tab first among the unpinned.
    tabs.move_tab_before(one.id, Some(pin.id)).unwrap();
    assert_eq!(titles(&tabs), ["pin", "one", "three", "two"]);
    assert!(tabs.move_tab_before(Ulid::new(), None).is_none());
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
    assert_eq!(
        tabs.close_targets(two.id, false, |_| true),
        [one.id, three.id]
    );
    assert_eq!(tabs.close_targets(two.id, true, |_| true), [three.id]);
    assert!(tabs.close_targets(three.id, true, |_| true).is_empty());
    // From a pinned tab: every unpinned one is to its right.
    assert_eq!(
        tabs.close_targets(pin.id, true, |_| true),
        [one.id, two.id, three.id]
    );
    assert!(tabs.close_targets(Ulid::new(), false, |_| true).is_empty());
    // A tab in another window is not this one's to close.
    assert_eq!(
        tabs.close_targets(two.id, false, |id| id != three.id),
        [one.id]
    );
    assert!(
        tabs.close_targets(two.id, true, |id| id != three.id)
            .is_empty()
    );
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
    assert!(tabs.focus(&k(b)));
    assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
    assert_eq!(tabs.tab_of(b).map(|t| t.id), Some(two.id));
    assert!(!tabs.focus(&k(Ulid::new())));
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

/// An active tab shows one icon per pane in pane order, an inactive one its focused pane's.
#[test]
fn a_tab_shows_pane_icons_active_all_inactive_focused() {
    let (mut tabs, a, b) = two_panes(); // a | b, focused on b
    let project = tabs.project_id();
    let mut agent = session(project, SessionState::Idle, 1);
    agent.id = a;
    let mut shell = session(project, SessionState::Idle, 2);
    shell.id = b;
    shell.kind = SessionKind::Shell;
    let sessions = [agent, shell];
    let tab = tabs.active().unwrap();

    let active = tab_icons(tab, &sessions, true);
    assert_eq!(
        active.panes,
        [PaneIcon::Agent("fake".into()), PaneIcon::Shell]
    );
    assert_eq!(active.more, 0);
    assert_eq!(tab_icons(tab, &sessions, false).panes, [PaneIcon::Shell]);
    tabs.focus(&k(a));
    let tab = tabs.active().unwrap();
    assert_eq!(
        tab_icons(tab, &sessions, false).panes,
        [PaneIcon::Agent("fake".into())]
    );
    // A pane no session is known for has no icon.
    assert!(tab_icons(tab, &[], true).panes.is_empty());
    // The fake agent has no brand logo: the chip draws its glyph in `text_muted`.
    assert!(crate::icons::AgentLogo::of("fake").is_none());
}

/// A tab without panes shows nothing; more than three panes end in `+N`; a run pane has its
/// own icon.
#[test]
fn a_tab_cuts_pane_icons_after_three_and_has_none_when_empty() {
    let project = Ulid::new();
    let mut empty = Tabs::new(project);
    empty.load(vec![wire(project, "new", 1.0, serde_json::Value::Null)]);
    let icons = tab_icons(empty.active().unwrap(), &[], true);
    assert_eq!(
        icons,
        TabIcons {
            status: None,
            panes: vec![],
            more: 0
        }
    );

    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "main", 1.0, serde_json::Value::Null)]);
    let mut sessions = Vec::new();
    for n in 0..5 {
        let mut s = session(project, SessionState::Idle, n);
        if n == 1 {
            s.kind = SessionKind::Shell;
            s.title = Some(mkrew_core::run::session_title("dev"));
        }
        tabs.split_focused(Axis::Vertical, s.id).unwrap();
        sessions.push(s);
    }
    let tab = tabs.active().unwrap();
    let active = tab_icons(tab, &sessions, true);
    assert_eq!(
        active.panes,
        [
            PaneIcon::Agent("fake".into()),
            PaneIcon::Run,
            PaneIcon::Agent("fake".into())
        ]
    );
    assert_eq!(active.more, 2);
    assert_eq!(tab_icons(tab, &sessions, false).more, 0);
}

/// The status icon shows only a state to notice: needs input beats a working agent; idle, done,
/// exited, starting and a live run process show none.
#[test]
fn a_tab_status_shows_needs_input_then_working_and_nothing_else() {
    let (tabs, a, b) = two_panes();
    let project = tabs.project_id();
    let tab = tabs.active().unwrap();
    let status = |first: SessionState, second: SessionState, run: bool| {
        let mut one = session(project, first, 1);
        one.id = a;
        let mut two = session(project, second, 2);
        two.id = b;
        if run {
            two.kind = SessionKind::Shell;
            two.title = Some(mkrew_core::run::session_title("dev"));
        }
        tab_icons(tab, &[one, two], true).status
    };
    use SessionState::*;
    assert_eq!(
        status(Working, NeedsInput, false),
        Some(TabStatus::NeedsInput)
    );
    assert_eq!(
        status(NeedsInput, Working, false),
        Some(TabStatus::NeedsInput)
    );
    assert_eq!(status(Done, Working, false), Some(TabStatus::Working));
    assert_eq!(status(Idle, Idle, false), None);
    assert_eq!(status(Done, Done, false), None);
    assert_eq!(status(Exited, Exited, false), None);
    assert_eq!(status(Idle, Starting, false), None);
    assert_eq!(status(Idle, Idle, true), None, "a live run process");
    // A working shell or run process is not an agent at work.
    assert_eq!(status(Idle, Working, true), None);
}

/// A tab that keeps its default name shows the session title of its focused pane, also among
/// several panes; a name the user gave the tab wins; the label follows the focus.
#[test]
fn a_default_named_tab_shows_the_session_title_of_its_focused_pane() {
    let project = Ulid::new();
    let mut titled = session(project, SessionState::Idle, 1);
    titled.terminal_title = Some("Fix login".into());
    let mut named = session(project, SessionState::Idle, 2);
    named.title = Some("Docs".into());
    let untitled = session(project, SessionState::Idle, 3);
    let sessions = [titled.clone(), named.clone(), untitled.clone()];
    let tree = json!({"split":"vertical","ratio":0.5,"children":[
        pane(titled.id),
        {"split":"horizontal","ratio":0.5,"children":[pane(named.id), pane(untitled.id)]}
    ]});
    // The label of a tab loaded with `layout`; `load` focuses its first pane.
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
    assert_eq!(
        label("Terminal", pane(untitled.id)),
        None,
        "no session title"
    );
    assert_eq!(
        label("Terminal", serde_json::Value::Null),
        None,
        "empty tab"
    );
    // A name the user gave the tab wins, with one pane and with several.
    for title in ["Backend", "Terminal 3b", "Terminal ", "Terminals"] {
        assert_eq!(label(title, pane(titled.id)), None, "{title}");
        assert_eq!(label(title, tree.clone()), None, "{title}");
    }

    // Several panes: the label follows the focus; the user's pane title is the pane's title.
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal 2", 1.0, tree)]);
    let shown = |tabs: &Tabs| session_label(tabs.active().unwrap(), &sessions);
    assert_eq!(shown(&tabs), Some("Fix login"));
    assert!(tabs.focus(&k(named.id)));
    assert_eq!(shown(&tabs), Some("Docs"));
    assert!(tabs.focus(&k(untitled.id)));
    assert_eq!(shown(&tabs), None);
    assert!(tabs.focus(&k(titled.id)));
    assert_eq!(shown(&tabs), Some("Fix login"));
}

/// A double-click renames the focused pane while the tab keeps its default name; the tab
/// itself when the user named it, when it has no panes, or when the focused pane is a run pane
/// (named by its command).
#[test]
fn a_double_click_renames_the_focused_pane_of_a_default_named_tab() {
    let project = Ulid::new();
    let agent = session(project, SessionState::Idle, 1);
    let mut run = session(project, SessionState::Idle, 2);
    run.kind = SessionKind::Shell;
    run.title = Some("run:BE".into());
    let sessions = [agent.clone(), run.clone()];
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(agent.id), pane(run.id)]});
    let renames = |title: &str, focus: Ulid| {
        let mut tabs = Tabs::new(project);
        tabs.load(vec![wire(project, title, 1.0, split.clone())]);
        assert!(tabs.focus(&k(focus)));
        double_click_renames(tabs.active().unwrap(), &sessions)
    };

    assert_eq!(renames("Terminal 2", agent.id), Some(agent.id));
    assert_eq!(
        renames("Terminal", run.id),
        None,
        "a run pane is not renamed"
    );
    assert_eq!(renames("Backend", agent.id), None, "the user's tab name");

    let mut empty = Tabs::new(project);
    empty.load(vec![wire(
        project,
        "Terminal",
        1.0,
        serde_json::Value::Null,
    )]);
    assert_eq!(
        double_click_renames(empty.active().unwrap(), &sessions),
        None
    );
}

#[test]
fn move_active_steps_over_the_visible_tabs_of_its_pin_group() {
    let project = Ulid::new();
    let mut pinned = wire(project, "pin", 1.0, serde_json::Value::Null);
    pinned.pinned = true;
    let [a, d, b] = [
        wire(project, "a", 2.0, serde_json::Value::Null),
        wire(project, "d", 3.0, serde_json::Value::Null),
        wire(project, "b", 4.0, serde_json::Value::Null),
    ];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![pinned.clone(), a.clone(), d.clone(), b.clone()]);
    let titles = |tabs: &Tabs| -> Vec<String> {
        (tabs.tabs().iter()).map(|tab| tab.title.clone()).collect()
    };
    let hidden = |id: Ulid| id == d.id;
    assert!(tabs.activate(pinned.id));
    assert!(tabs.move_active(1, hidden).is_none(), "alone in its group");
    assert!(tabs.activate(b.id));
    assert!(tabs.move_active(1, hidden).is_none(), "last of the visible");
    // [pin, a, d, b]: b left of the hidden d lands before a, the visible neighbour.
    assert!(tabs.move_active(-1, hidden).is_some());
    assert_eq!(titles(&tabs), ["pin", "b", "a", "d"]);
    // And right again: past a, before whatever follows it (the hidden d).
    assert!(tabs.move_active(1, hidden).is_some());
    assert_eq!(titles(&tabs), ["pin", "a", "b", "d"]);
    assert!(tabs.move_active(1, hidden).is_none(), "d is not a place");
}

#[test]
fn activate_offset_and_nth_count_only_visible_tabs() {
    let project = Ulid::new();
    let [a, d, b] = [
        wire(project, "a", 1.0, serde_json::Value::Null),
        wire(project, "d", 2.0, serde_json::Value::Null),
        wire(project, "b", 3.0, serde_json::Value::Null),
    ];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![a.clone(), d.clone(), b.clone()]);
    let hidden = |id: Ulid| id == d.id;
    assert!(tabs.activate(b.id));
    tabs.activate_offset(-1, hidden);
    assert_eq!(tabs.active().map(|t| t.id), Some(a.id), "d is skipped");
    tabs.activate_offset(-1, hidden);
    assert_eq!(tabs.active().map(|t| t.id), Some(b.id), "wraps over d");
    assert!(tabs.activate_nth(1, hidden));
    assert_eq!(tabs.active().map(|t| t.id), Some(b.id), "Ctrl+Alt+2 is b");
    assert!(!tabs.activate_nth(2, hidden), "only two are shown");
}

#[test]
fn split_focused_in_a_non_active_tab_leaves_the_active_one() {
    let project = Ulid::new();
    let [a, b, new] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(project, "two", 2.0, pane(b));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    let write = tabs
        .with_active(two.id, |tabs| tabs.split_focused(Axis::Vertical, new))
        .flatten();
    assert_eq!(write.map(|w| w.tab_id), Some(two.id));
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert!(tabs.tab(two.id).is_some_and(|tab| tab.contains(&k(new))));
    assert!(tabs.tab(one.id).is_some_and(|tab| !tab.contains(&k(new))));
}

#[test]
fn move_pane_in_a_non_active_tab_leaves_the_active_one() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(b), pane(c)]});
    let two = wire(project, "two", 2.0, split);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    let writes = (tabs.with_active(two.id, |tabs| tabs.move_pane(&k(b), &k(c), Edge::Bottom)))
        .flatten()
        .unwrap_or_default();
    assert_eq!(writes.len(), 1, "one tab, one write");
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id), "active restored");
    assert_eq!(tabs.tab(two.id).and_then(Tab::focused_session), Some(b));
}

fn split(axis: &str, first: serde_json::Value, second: serde_json::Value) -> serde_json::Value {
    json!({"split": axis, "ratio": 0.5, "children": [first, second]})
}

#[test]
fn a_pane_moves_beside_another_in_its_tab_or_from_another_tab() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, split("vertical", pane(a), pane(b)));
    let two = wire(project, "two", 2.0, pane(c));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    // Same tab: `a` goes below `b`; one write.
    let writes = tabs.move_pane(&k(a), &k(b), Edge::Bottom).unwrap();
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].tab_id, one.id);
    assert_eq!(writes[0].layout, split("horizontal", pane(b), pane(a)));
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(a));

    // Another tab: the source tab is written first, the target tab is activated.
    let writes = tabs.move_pane(&k(a), &k(c), Edge::Right).unwrap();
    assert_eq!((writes[0].tab_id, &writes[0].layout), (one.id, &pane(b)));
    assert_eq!(
        (writes[1].tab_id, &writes[1].layout),
        (two.id, &split("vertical", pane(c), pane(a)))
    );
    assert_eq!(tabs.active().map(|tab| tab.id), Some(two.id));
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(a));
    assert_eq!(tabs.tab(one.id).and_then(Tab::focused_session), Some(b));

    // Onto itself, or with a pane in no tab: nothing moves.
    assert_eq!(tabs.move_pane(&k(a), &k(a), Edge::Right), None);
    assert_eq!(tabs.move_pane(&k(Ulid::new()), &k(a), Edge::Right), None);
    assert_eq!(tabs.move_pane(&k(a), &k(Ulid::new()), Edge::Right), None);
}

#[test]
fn moving_a_pane_ends_the_zoom_of_its_new_tab() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![
        wire(project, "one", 1.0, split("vertical", pane(a), pane(b))),
        wire(project, "two", 2.0, pane(c)),
    ]);
    assert!(tabs.toggle_zoom());
    tabs.move_pane(&k(c), &k(b), Edge::Right).unwrap();
    assert_eq!(tabs.active().and_then(Tab::zoomed), None);
    assert_eq!(tabs.visible(), vec![a, b, c]);
}

#[test]
fn a_tab_merges_beside_a_pane_of_another_tab_keeping_its_layout() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(project, "two", 2.0, split("horizontal", pane(b), pane(c)));
    let empty = wire(project, "empty", 3.0, serde_json::Value::Null);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone(), empty.clone()]);

    // Onto a pane of its own tab, or an empty tab: nothing moves.
    assert_eq!(tabs.merge_tab(two.id, &k(b), Edge::Right), None);
    assert_eq!(tabs.merge_tab(empty.id, &k(a), Edge::Right), None);

    let [placed, emptied] = tabs.merge_tab(two.id, &k(a), Edge::Right).unwrap();
    assert_eq!(placed.tab_id, one.id);
    assert_eq!(
        placed.layout,
        split("vertical", pane(a), split("horizontal", pane(b), pane(c)))
    );
    assert_eq!(
        (emptied.tab_id, emptied.layout),
        (two.id, serde_json::Value::Null)
    );
    assert_eq!(tabs.active().map(|tab| tab.id), Some(one.id));
    assert_eq!(tabs.active().and_then(Tab::focused_session), Some(b));
    assert!(
        tabs.tab(two.id)
            .is_some_and(|tab| tab.sessions().is_empty())
    );
}

#[test]
fn with_active_acts_on_another_tab_and_restores_the_active_one() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(b), pane(c)]});
    let two = wire(project, "two", 2.0, split);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));

    let zoomed = tabs.with_active(two.id, Tabs::toggle_zoom);
    assert_eq!(zoomed, Some(true));
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id), "active restored");
    assert!(
        tabs.tab(two.id).and_then(Tab::zoomed).is_some(),
        "the change stays"
    );
    assert_eq!(tabs.with_active(Ulid::new(), Tabs::toggle_zoom), None);
}

#[test]
fn focus_pane_leaves_the_active_tab() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let split = json!({"split":"vertical","ratio":0.5,"children":[pane(b), pane(c)]});
    let two = wire(project, "two", 2.0, split);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    assert!(tabs.focus_pane(two.id, &k(c)));
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert_eq!(tabs.tab(two.id).and_then(Tab::focused_session), Some(c));
    assert!(!tabs.focus_pane(two.id, &k(a)), "a is not in that tab");
}

#[test]
fn activate_skipping_moves_off_a_hidden_tab() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(project, "two", 2.0, pane(b));
    let three = wire(project, "three", 3.0, pane(c));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone(), three.clone()]);
    assert!(tabs.activate(two.id));

    tabs.activate_skipping(|id| id == two.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(three.id), "right first");
    tabs.activate_skipping(|id| id == three.id || id == two.id);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id), "else left");
    tabs.activate_skipping(|_| true);
    assert!(tabs.active().is_none(), "every tab hidden");
    tabs.activate_skipping(|_| false);
    assert!(tabs.active().is_none(), "nothing to move off");
}

#[test]
fn a_pane_keeps_its_chat_mode_in_the_layout_json() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let split = json!({"split":"vertical","ratio":0.5,"children":[
        {"session": a.to_string(), "chat": true},
        pane(b)
    ]});
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "main", 1.0, split.clone())]);
    assert_eq!(tabs.chat_of(a), Some(true));
    assert_eq!(
        tabs.chat_of(b),
        None,
        "layouts of older versions have no flag"
    );
    // Any layout write (a divider drag) carries the flag and adds none.
    assert_eq!(tabs.write_active().unwrap().layout, split);

    let write = tabs.set_chat(b, Some(true)).unwrap();
    assert_eq!(
        write.layout["children"][1],
        json!({"session": b.to_string(), "chat": true})
    );
    assert_eq!(
        tabs.set_chat(b, Some(true)),
        None,
        "unchanged: nothing to write"
    );
    let write = tabs.set_chat(a, Some(false)).unwrap();
    assert_eq!(write.layout["children"][0]["chat"], json!(false));
    assert_eq!(tabs.set_chat(Ulid::new(), Some(true)), None, "no such pane");
}

#[test]
fn moving_and_arranging_panes_keeps_their_chat_mode() {
    let project = Ulid::new();
    let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
    let split = json!({"split":"vertical","ratio":0.5,"children":[
        {"session": a.to_string(), "chat": true},
        pane(b)
    ]});
    let tab = wire(project, "main", 1.0, split);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    tabs.move_pane(&k(a), &k(b), Edge::Bottom).unwrap();
    assert_eq!(tabs.chat_of(a), Some(true), "a moved pane");
    assert_eq!(tabs.chat_of(b), None);
    tabs.arrange(tab.id, Preset::Columns3, &[c]).unwrap();
    assert_eq!(tabs.chat_of(a), Some(true), "a re-arranged pane");
    assert_eq!(tabs.chat_of(c), None);
    tabs.close_leaf(&k(b));
    assert_eq!(tabs.chat_of(a), Some(true), "a pane whose sibling closed");
}

#[test]
fn a_cross_tab_move_keeps_the_pane_chat_mode() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let one = wire(
        project,
        "one",
        1.0,
        json!({"session": a.to_string(), "chat": true}),
    );
    let two = wire(project, "two", 2.0, pane(b));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one, two.clone()]);
    tabs.move_pane(&k(a), &k(b), Edge::Bottom).unwrap();
    assert_eq!(tabs.tab_of(a).map(|t| t.id), Some(two.id), "a changed tab");
    assert_eq!(tabs.chat_of(a), Some(true));
    assert_eq!(tabs.chat_of(b), None);
}

#[test]
fn pin_unset_sets_unset_panes_only_with_one_write_per_tab() {
    let project = Ulid::new();
    let [a, b, c, d] = [Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new()];
    let split = json!({"split":"vertical","ratio":0.5,"children":[
        {"session": a.to_string(), "chat": false},
        {"split":"horizontal","ratio":0.5,"children":[pane(b), pane(c)]}
    ]});
    let (one, two, three) = (
        wire(project, "one", 1.0, split),
        wire(project, "two", 2.0, pane(d)),
        wire(
            project,
            "three",
            3.0,
            json!({"session": Ulid::new().to_string(), "chat": true}),
        ),
    );
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone(), three]);

    // `c` has no known mode (not an agent): left unset.
    let writes = tabs.pin_unset(|id| (id != c).then_some(true));
    assert_eq!(writes.len(), 2, "tab three had nothing unset");
    assert_eq!(writes[0].tab_id, one.id);
    assert_eq!(writes[1].tab_id, two.id);
    assert_eq!(
        tabs.chat_of(a),
        Some(false),
        "a saved mode is never touched"
    );
    assert_eq!(tabs.chat_of(b), Some(true));
    assert_eq!(tabs.chat_of(c), None);
    assert_eq!(tabs.chat_of(d), Some(true));
    assert!(
        tabs.pin_unset(|id| (id != c).then_some(false)).is_empty(),
        "pinned panes stay"
    );
}

#[test]
fn a_new_pane_is_born_with_the_given_mode() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "main", 1.0, serde_json::Value::Null)]);
    let first = tabs
        .split_focused_chat(Axis::Vertical, a, Some(true))
        .unwrap();
    assert_eq!(
        first.layout,
        json!({"session": a.to_string(), "chat": true})
    );
    let second = tabs
        .split_focused_chat(Axis::Vertical, b, Some(false))
        .unwrap();
    assert_eq!(second.layout["children"][1]["chat"], json!(false));
    assert_eq!(
        (tabs.chat_of(a), tabs.chat_of(b)),
        (Some(true), Some(false))
    );
}

/// The tab area's children, in paint order: the Diff chip, a rule, the tabs, a rule between
/// the pinned tabs and the rest; never a rule at an end.
#[test]
fn the_tab_area_orders_the_diff_chip_pinned_tabs_and_the_rest() {
    use AreaItem::{Rule, Tab, View as Chip};
    assert_eq!(area_items(false, &[]), []);
    assert_eq!(
        area_items(false, &[true, true, false]),
        [Tab(0), Tab(1), Rule, Tab(2)]
    );
    assert_eq!(
        area_items(false, &[true, true]),
        [Tab(0), Tab(1)],
        "no rule after the last group"
    );
    assert_eq!(
        area_items(true, &[false, false]),
        [Chip(View::Diff), Rule, Tab(0), Tab(1)]
    );
    assert_eq!(
        area_items(true, &[true, false]),
        [Chip(View::Diff), Rule, Tab(0), Rule, Tab(1)]
    );
    assert_eq!(
        area_items(true, &[]),
        [Chip(View::Diff)],
        "no tabs, no rule"
    );
}

/// The child to scroll into view: the active tab while Terminals shows, else the chip of the
/// shown view; nothing for Board and Docs (they are icons, not children).
#[test]
fn the_scroll_target_is_the_shown_chip_or_the_active_tab() {
    let items = area_items(true, &[true, false, false]);
    // [Diff, Rule, Tab(0), Rule, Tab(1), Tab(2)]
    assert_eq!(scroll_index(&items, View::Terminals, Some(2)), Some(5));
    assert_eq!(scroll_index(&items, View::Terminals, Some(0)), Some(2));
    assert_eq!(scroll_index(&items, View::Diff, Some(2)), Some(0));
    assert_eq!(scroll_index(&items, View::Board, Some(2)), None);
    assert_eq!(scroll_index(&items, View::Terminals, None), None);
    let no_diff = area_items(false, &[true, false, false]);
    assert_eq!(
        scroll_index(&no_diff, View::Diff, Some(1)),
        None,
        "no Diff chip"
    );
}

#[test]
fn a_file_leaf_is_a_pane_but_not_a_session() {
    let project = Ulid::new();
    let a = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(
        project,
        "one",
        1.0,
        split("vertical", pane(a), file("src/a.rs")),
    )]);
    let tab = tabs.active().unwrap();
    assert_eq!(tab.keys(), [k(a), file_key("src/a.rs")]);
    assert_eq!(tab.sessions(), [a]);
    assert_eq!(tab.focused(), Some(&k(a)), "the first leaf has the focus");
    assert!(tab.contains(&file_key("src/a.rs")));

    // With the file focused no session has the keyboard, and the zoom shows the file alone.
    assert!(tabs.focus(&file_key("src/a.rs")));
    let tab = tabs.active().unwrap();
    assert_eq!(tab.focused_session(), None);
    assert_eq!(tab.focused(), Some(&file_key("src/a.rs")));
    assert!(tabs.toggle_zoom());
    let tab = tabs.active().unwrap();
    assert_eq!(tab.shown(), [file_key("src/a.rs")]);
    assert!(tab.shown_sessions().is_empty());
    assert!(tabs.visible().is_empty(), "no session is on screen");
    assert_eq!(tab.rects().len(), 1);
    assert_eq!(tab.rects()[0].0, file_key("src/a.rs"));

    // Focusing the session ends the zoom: its terminal is on screen again.
    assert!(tabs.focus(&k(a)));
    assert_eq!(tabs.visible(), vec![a]);
}

#[test]
fn tab_with_finds_any_leaf_in_any_tab() {
    let project = Ulid::new();
    let a = Ulid::new();
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(
        project,
        "two",
        2.0,
        split("vertical", file("src/a.rs"), json!({"diff": true})),
    );
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));

    // A leaf of a tab that is not the active one (a detached window shows it from the same
    // `Tabs`, so this is also how a file in a detached window is found).
    assert_eq!(
        tabs.tab_with(&file_key("src/a.rs")).map(|t| t.id),
        Some(two.id)
    );
    assert_eq!(tabs.tab_with(&PaneKey::Diff).map(|t| t.id), Some(two.id));
    assert_eq!(tabs.tab_with(&k(a)).map(|t| t.id), Some(one.id));
    assert_eq!(tabs.tab_of(a).map(|t| t.id), Some(one.id));
    assert!(tabs.tab_with(&file_key("src/b.rs")).is_none());

    // `focus_pane` reaches a leaf of another tab without activating it.
    assert!(tabs.focus_pane(two.id, &PaneKey::Diff));
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert_eq!(
        tabs.tab(two.id).and_then(Tab::focused),
        Some(&PaneKey::Diff)
    );
    assert!(!tabs.focus_pane(one.id, &PaneKey::Diff), "not in that tab");
    // `focus` activates the tab that holds the leaf.
    assert!(tabs.focus(&file_key("src/a.rs")));
    assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
}

#[test]
fn a_tab_loads_around_a_leaf_it_cannot_read() {
    let project = Ulid::new();
    let a = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(
        project,
        "one",
        1.0,
        split("vertical", pane(a), json!({"canvas": 1})),
    )]);
    let tab = tabs.active().unwrap();
    assert_eq!(tab.layout(), Some(&Node::pane(a)));
    assert_eq!(tabs.visible(), vec![a]);
    // A layout that is bad all the way down is an empty tab, not a lost one.
    tabs.load(vec![wire(project, "two", 2.0, json!({"canvas": 1}))]);
    assert_eq!(tabs.tabs().len(), 1);
    assert_eq!(tabs.tabs()[0].layout(), None);
}

#[test]
fn a_session_that_goes_leaves_the_file_leaf_focused_and_the_tab() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, split("vertical", pane(a), file("x")));
    let two = wire(project, "two", 2.0, pane(b));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two]);
    assert!(tabs.focus(&k(a)));

    // `a` is unknown to the daemon now.
    let writes = tabs.prune(|id| id == b);
    assert_eq!(writes.len(), 1);
    assert_eq!((writes[0].tab_id, &writes[0].layout), (one.id, &file("x")));
    let tab = tabs.tab(one.id).unwrap();
    assert_eq!(tab.focused(), Some(&file_key("x")));
    assert!(tab.sessions().is_empty());
    assert_eq!(tabs.tabs().len(), 2, "the tab stays");
    assert!(tabs.prune(|id| id == b).is_empty());
}

#[test]
fn close_leaf_closes_any_kind_and_moves_the_focus() {
    let project = Ulid::new();
    let a = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(
        project,
        "one",
        1.0,
        split(
            "vertical",
            pane(a),
            split("horizontal", file("x"), json!({"diff": true})),
        ),
    )]);
    assert!(tabs.focus(&file_key("x")));
    let write = tabs.close_leaf(&file_key("x")).unwrap();
    assert_eq!(
        write.layout,
        split("vertical", pane(a), json!({"diff": true}))
    );
    assert_eq!(tabs.active().and_then(Tab::focused), Some(&PaneKey::Diff));
    assert_eq!(tabs.close_leaf(&file_key("x")), None, "already closed");
    tabs.close_leaf(&PaneKey::Diff).unwrap();
    let write = tabs.close_leaf(&k(a)).unwrap();
    assert_eq!(write.layout, serde_json::Value::Null);
    assert_eq!(tabs.active().and_then(Tab::focused), None);
}

#[test]
fn move_pane_carries_the_whole_leaf() {
    let project = Ulid::new();
    let [a, c] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, split("vertical", pane(a), full_file()));
    let two = wire(project, "two", 2.0, pane(c));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    // To another tab, on the left of `c`: `md` and `preview` travel with the file.
    let writes = tabs
        .move_pane(&file_key("README.md"), &k(c), Edge::Left)
        .unwrap();
    assert_eq!(writes.len(), 2);
    assert_eq!((writes[0].tab_id, &writes[0].layout), (one.id, &pane(a)));
    assert_eq!(
        (writes[1].tab_id, &writes[1].layout),
        (two.id, &split("vertical", full_file(), pane(c)))
    );
    assert_eq!(tabs.active().map(|t| t.id), Some(two.id));
    assert_eq!(
        tabs.tab(two.id).and_then(Tab::focused),
        Some(&file_key("README.md"))
    );
    assert_eq!(
        tabs.tab_with(&file_key("README.md")).map(|t| t.id),
        Some(two.id)
    );
    // Onto itself or onto a leaf that is in no tab: nothing moves.
    assert_eq!(
        tabs.move_pane(&file_key("README.md"), &file_key("README.md"), Edge::Top),
        None
    );
    assert_eq!(
        tabs.move_pane(&file_key("README.md"), &file_key("nope"), Edge::Top),
        None
    );
    assert_eq!(tabs.move_pane(&file_key("nope"), &k(c), Edge::Top), None);
}

#[test]
fn move_pane_moves_the_diff_within_a_tab_to_each_edge() {
    let project = Ulid::new();
    let a = Ulid::new();
    let diff = || json!({"diff": true});
    let tab = wire(project, "one", 1.0, split("vertical", pane(a), diff()));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    for (edge, expected) in [
        (Edge::Top, split("horizontal", diff(), pane(a))),
        (Edge::Bottom, split("horizontal", pane(a), diff())),
        (Edge::Left, split("vertical", diff(), pane(a))),
        (Edge::Right, split("vertical", pane(a), diff())),
    ] {
        let writes = tabs.move_pane(&PaneKey::Diff, &k(a), edge).unwrap();
        assert_eq!(writes.len(), 1, "one tab, one write");
        assert_eq!(writes[0].layout, expected, "{edge:?}");
        assert_eq!(tabs.active().and_then(Tab::focused), Some(&PaneKey::Diff));
    }
}

#[test]
fn merge_tab_carries_whole_leaves_to_any_edge() {
    let project = Ulid::new();
    let a = Ulid::new();
    let one = wire(project, "one", 1.0, pane(a));
    let moved = split("vertical", full_file(), json!({"diff": true}));
    let two = wire(project, "two", 2.0, moved.clone());
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);

    let [placed, emptied] = tabs.merge_tab(two.id, &k(a), Edge::Top).unwrap();
    assert_eq!(placed.tab_id, one.id);
    assert_eq!(placed.layout, split("horizontal", moved, pane(a)));
    assert_eq!(
        (emptied.tab_id, emptied.layout),
        (two.id, serde_json::Value::Null)
    );
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id));
    assert_eq!(
        tabs.tab(one.id).and_then(Tab::focused),
        Some(&file_key("README.md")),
        "the first moved leaf has the focus"
    );
    // The same tab, or an empty one: nothing moves.
    assert_eq!(tabs.merge_tab(one.id, &k(a), Edge::Right), None);
    assert_eq!(tabs.merge_tab(two.id, &k(a), Edge::Right), None);
}

#[test]
fn arrange_carries_whole_leaves() {
    let project = Ulid::new();
    let a = Ulid::new();
    let tab = wire(project, "one", 1.0, split("vertical", pane(a), full_file()));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    let write = tabs.arrange(tab.id, Preset::Rows2, &[]).unwrap();
    assert_eq!(write.layout, split("horizontal", pane(a), full_file()));
    // Three leaves need Columns3 or MainAndStack; a new session joins after the file.
    let c = Ulid::new();
    tabs.arrange(tab.id, Preset::MainAndStack, &[c]).unwrap();
    let tab = tabs.tab(tab.id).unwrap();
    assert_eq!(tab.keys(), [k(a), file_key("README.md"), k(c)]);
    assert_eq!(
        tab.layout()
            .and_then(|node| node.leaf_of(&file_key("README.md"))),
        Some(&Leaf::File {
            file: "README.md".into(),
            md: Some(mkrew_core::settings::MdMode::Split),
            preview: true,
        })
    );
}

#[test]
fn insert_leaf_places_a_leaf_and_focuses_it() {
    let project = Ulid::new();
    let a = Ulid::new();
    let one = wire(project, "one", 1.0, pane(a));
    let empty = wire(project, "two", 2.0, serde_json::Value::Null);
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), empty.clone()]);

    // An empty tab takes the leaf as its root (`target` None).
    let write = tabs
        .insert_leaf(empty.id, None, Edge::Right, Leaf::file("src/a.rs"))
        .unwrap();
    assert_eq!((write.tab_id, &write.layout), (empty.id, &file("src/a.rs")));
    assert_eq!(
        tabs.tab(empty.id).and_then(Tab::focused),
        Some(&file_key("src/a.rs"))
    );
    assert_eq!(tabs.active().map(|t| t.id), Some(one.id), "not activated");

    // Next to a pane, on each side.
    let write = tabs
        .insert_leaf(one.id, Some(&k(a)), Edge::Left, Leaf::diff())
        .unwrap();
    assert_eq!(
        write.layout,
        split("vertical", json!({"diff": true}), pane(a))
    );
    assert_eq!(
        tabs.tab(one.id).and_then(Tab::focused),
        Some(&PaneKey::Diff)
    );
    let write = tabs
        .insert_leaf(one.id, Some(&k(a)), Edge::Bottom, Leaf::file("b.rs"))
        .unwrap();
    assert_eq!(
        write.layout,
        split(
            "vertical",
            json!({"diff": true}),
            split("horizontal", pane(a), file("b.rs"))
        )
    );
}

#[test]
fn insert_leaf_refuses_a_key_already_in_any_tab_of_the_workspace() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(
        project,
        "two",
        2.0,
        split("vertical", pane(b), file("src/a.rs")),
    );
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one.clone(), two.clone()]);
    let before = tabs.clone();

    // The file is in the other tab, a session likewise: no second pane for the same key.
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(a)), Edge::Right, Leaf::file("src/a.rs")),
        None
    );
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(a)), Edge::Right, Leaf::session(b)),
        None
    );
    // The same file with other flags is the same key.
    let flagged = Leaf::File {
        file: "src/a.rs".into(),
        md: Some(mkrew_core::settings::MdMode::Source),
        preview: true,
    };
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(a)), Edge::Right, flagged),
        None
    );
    // Unknown tab or target, a target in another tab, a root into a tab with panes.
    assert_eq!(
        tabs.insert_leaf(Ulid::new(), None, Edge::Right, Leaf::diff()),
        None
    );
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(Ulid::new())), Edge::Right, Leaf::diff()),
        None
    );
    assert_eq!(
        tabs.insert_leaf(one.id, Some(&k(b)), Edge::Right, Leaf::diff()),
        None
    );
    assert_eq!(
        tabs.insert_leaf(one.id, None, Edge::Right, Leaf::diff()),
        None
    );
    assert_eq!(tabs, before, "nothing changed");

    // The diff is one per workspace too.
    tabs.insert_leaf(one.id, Some(&k(a)), Edge::Right, Leaf::diff())
        .unwrap();
    assert_eq!(
        tabs.insert_leaf(two.id, Some(&k(b)), Edge::Right, Leaf::diff()),
        None
    );
}

#[test]
fn update_leaf_writes_only_a_change_and_never_moves_the_key() {
    let project = Ulid::new();
    let a = Ulid::new();
    let tab = wire(
        project,
        "one",
        1.0,
        split("vertical", pane(a), file("a.md")),
    );
    let mut tabs = Tabs::new(project);
    tabs.load(vec![tab.clone()]);
    let key = file_key("a.md");
    let set_mode = |leaf: &mut Leaf| {
        if let Leaf::File { md, .. } = leaf {
            *md = Some(mkrew_core::settings::MdMode::Split);
        }
    };

    let write = tabs.update_leaf(&key, set_mode).unwrap();
    assert_eq!(
        write.layout,
        split("vertical", pane(a), json!({"file": "a.md", "md": "split"}))
    );
    // The same value again: nothing to write.
    assert_eq!(tabs.update_leaf(&key, set_mode), None);
    // Another change, then an unknown key.
    let write = tabs
        .update_leaf(&key, |leaf| {
            if let Leaf::File { preview, .. } = leaf {
                *preview = true;
            }
        })
        .unwrap();
    assert_eq!(
        write.layout["children"][1],
        json!({"file": "a.md", "md": "split", "preview": true})
    );
    assert_eq!(tabs.update_leaf(&file_key("nope"), set_mode), None);
    assert_eq!(tabs.update_leaf(&PaneKey::Diff, |_| {}), None);
    // The leaf is found by its key in the tab, whichever tab is active.
    let write = tabs
        .update_leaf(&k(a), |leaf| {
            if let Leaf::Session { chat, .. } = leaf {
                *chat = Some(true);
            }
        })
        .unwrap();
    assert_eq!(write.layout["children"][0]["chat"], json!(true));
}

#[test]
fn update_leaf_refuses_a_change_of_the_key_and_keeps_the_leaf() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "one", 1.0, file("a.md"))]);
    let key = file_key("a.md");
    let renamed = tabs.update_leaf(&key, |leaf| {
        *leaf = Leaf::File {
            file: "b.md".into(),
            md: None,
            preview: false,
        };
    });
    assert_eq!(renamed, None);
    assert!(tabs.tab_with(&key).is_some(), "the leaf is as it was");
    assert!(tabs.tab_with(&file_key("b.md")).is_none());
}

#[test]
fn preview_tab_is_the_tab_holding_the_preview_file() {
    let project = Ulid::new();
    let [a, b] = [Ulid::new(), Ulid::new()];
    let one = wire(project, "one", 1.0, pane(a));
    let two = wire(
        project,
        "two",
        2.0,
        split(
            "vertical",
            pane(b),
            json!({"file": "x.rs", "preview": true}),
        ),
    );
    let three = wire(project, "three", 3.0, file("y.rs"));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![one, two.clone(), three]);
    assert_eq!(tabs.preview_tab().map(|t| t.id), Some(two.id));

    // Kept: the flag goes, so does the preview tab.
    tabs.update_leaf(&file_key("x.rs"), |leaf| {
        if let Leaf::File { preview, .. } = leaf {
            *preview = false;
        }
    })
    .unwrap();
    assert_eq!(tabs.preview_tab(), None);
    assert_eq!(Tabs::new(project).preview_tab(), None);
}

/// Tabs at the given `(position, pinned)` pairs, in the given order; returns their ids.
fn tabs_at(project: Ulid, places: &[(f64, bool)]) -> (Tabs, Vec<Ulid>) {
    let wires: Vec<TerminalTab> = places
        .iter()
        .map(|&(position, pinned)| TerminalTab {
            pinned,
            ..wire(project, "t", position, serde_json::Value::Null)
        })
        .collect();
    let ids = wires.iter().map(|wire| wire.id).collect();
    let mut tabs = Tabs::new(project);
    tabs.load(wires);
    (tabs, ids)
}

#[test]
fn position_after_active_is_the_midpoint_to_the_next_tab() {
    let (mut tabs, ids) = tabs_at(Ulid::new(), &[(1.0, false), (2.0, false), (4.0, false)]);
    assert!(tabs.activate(ids[0]));
    assert_eq!(tabs.position_after_active(), Some(1.5));
    assert!(tabs.activate(ids[1]));
    assert_eq!(tabs.position_after_active(), Some(3.0));
}

#[test]
fn position_after_active_of_the_last_tab_is_one_further() {
    let (mut tabs, ids) = tabs_at(Ulid::new(), &[(1.0, false), (2.0, false), (4.0, false)]);
    assert!(tabs.activate(ids[2]));
    assert_eq!(tabs.position_after_active(), Some(5.0));
    // No tabs, no position.
    assert_eq!(Tabs::new(Ulid::new()).position_after_active(), None);
}

#[test]
fn position_after_active_of_a_pinned_tab_goes_first_among_the_unpinned() {
    // Pinned tabs lead the strip: p at 5, q at 7; then the others at 1 and 2.
    let (mut tabs, ids) = tabs_at(
        Ulid::new(),
        &[(5.0, true), (7.0, true), (1.0, false), (2.0, false)],
    );
    for pinned in [ids[0], ids[1]] {
        assert!(tabs.activate(pinned));
        assert_eq!(
            tabs.position_after_active(),
            Some(0.0),
            "first of the unpinned tabs (1.0 - 1.0)"
        );
    }
    assert!(tabs.activate(ids[2]));
    assert_eq!(tabs.position_after_active(), Some(1.5));
    assert!(tabs.activate(ids[3]));
    assert_eq!(tabs.position_after_active(), Some(3.0));
    // Only pinned tabs: the new tab is the first unpinned one.
    let (mut tabs, ids) = tabs_at(Ulid::new(), &[(5.0, true), (7.0, true)]);
    assert!(tabs.activate(ids[1]));
    assert_eq!(tabs.position_after_active(), Some(1.0));
}

#[test]
fn a_file_leaf_names_and_marks_its_tab_chip() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![
        wire(
            project,
            "Terminal",
            1.0,
            json!({"file": "src/a.rs", "preview": true}),
        ),
        wire(project, "Terminal 2", 2.0, json!({"file": "b.md"})),
    ]);
    let mut docs = crate::documents::Documents::new();
    let label = file_label(&tabs.tabs()[0], Some(&docs)).expect("a file is focused");
    assert_eq!(
        (label.name.as_str(), label.dirty, label.preview),
        ("a.rs", false, true)
    );
    // Unsaved text shows as a dot; a kept file is not italic.
    docs.ensure("b.md", crate::documents::MdMode::Source);
    let kept = file_label(&tabs.tabs()[1], Some(&docs)).expect("a file is focused");
    assert_eq!(
        (kept.name.as_str(), kept.dirty, kept.preview),
        ("b.md", false, false)
    );
    assert!(docs.begin_read("b.md"));
    let content = mkrew_protocol::FsContent {
        path: "b.md".into(),
        content: Some("a".into()),
        size: 1,
        binary: false,
        too_large: false,
        language: None,
        hash: Some("#a".into()),
        index_text: None,
    };
    docs.read("b.md", Ok((content, crate::highlight::Code::plain("a"))));
    docs.get_mut("b.md")
        .unwrap()
        .editor_mut()
        .unwrap()
        .insert("x");
    let dirty = file_label(&tabs.tabs()[1], Some(&docs)).expect("a file is focused");
    assert!(dirty.dirty);
}

#[test]
fn a_name_the_user_gave_the_tab_wins_over_the_file_label() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Notes", 1.0, file("a.rs"))]);
    assert_eq!(file_label(&tabs.tabs()[0], None), None);
}

#[test]
fn a_session_pane_has_no_file_label() {
    let project = Ulid::new();
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, pane(Ulid::new()))]);
    assert_eq!(file_label(&tabs.tabs()[0], None), None);
}

#[test]
fn file_panes_have_chip_icons_and_unknown_sessions_none() {
    let project = Ulid::new();
    let split = split("vertical", pane(Ulid::new()), file("src/a.rs"));
    let mut tabs = Tabs::new(project);
    tabs.load(vec![wire(project, "Terminal", 1.0, split)]);
    let tab = tabs.active().expect("a tab");
    // The unknown session is skipped, as before; the file shows.
    assert_eq!(
        tab_icons(tab, &[], true).panes,
        [PaneIcon::File("src/a.rs".into())]
    );
    // An inactive tab shows its focused leaf only: the first (session) leaf, unknown here.
    assert!(tab_icons(tab, &[], false).panes.is_empty());
}
