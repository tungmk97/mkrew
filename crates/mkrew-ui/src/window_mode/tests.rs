use gpui::Keystroke;
use mkrew_core::settings::{OpenWorkspace, WindowMode};
use mkrew_protocol::{SessionState, StateSource};
use ulid::Ulid;

use super::*;
use crate::detached::SpaceKey;
use crate::inbox::Entry;

fn ids<const N: usize>() -> [Ulid; N] {
    std::array::from_fn(|_| Ulid::new())
}

/// A key press as the platform sends it: an unmodified printable key carries its text.
fn press(picker: &mut WorkspacePicker, keys: &str) -> PickOutcome {
    let mut key = Keystroke::parse(keys).unwrap();
    if !key.modifiers.modified() {
        match key.key.as_str() {
            "space" => key.key_char = Some(" ".into()),
            c if c.chars().count() == 1 => key.key_char = Some(c.into()),
            _ => {}
        }
    }
    picker.key(&key)
}

fn rows(names: &[&str]) -> Vec<PickRow> {
    names
        .iter()
        .map(|name| PickRow {
            id: Ulid::new(),
            name: (*name).to_owned(),
            color: None,
            working: 0,
            asking: 0,
        })
        .collect()
}

#[test]
fn split_keeps_the_shown_workspace_when_ticked_else_the_first_ticked() {
    let [a, b, c] = ids();
    let listed = [a, b, c];
    assert_eq!(
        plan_split(&listed, &[c, b], Some(b)),
        Some(SplitPlan {
            keep: b,
            open: vec![c]
        })
    );
    assert_eq!(
        plan_split(&listed, &[c, b], Some(a)),
        Some(SplitPlan {
            keep: b,
            open: vec![c]
        }),
        "list order, not tick order"
    );
    assert_eq!(plan_split(&listed, &[], Some(a)), None);
    assert_eq!(
        plan_split(&listed, &[a], None),
        Some(SplitPlan {
            keep: a,
            open: vec![]
        })
    );
}

#[test]
fn combine_closes_every_other_space() {
    let spaces = [SpaceKey(0), SpaceKey(3), SpaceKey(7)];
    assert_eq!(
        plan_combine(SpaceKey(3), &spaces),
        vec![SpaceKey(0), SpaceKey(7)]
    );
    assert_eq!(
        plan_combine(SpaceKey(0), &[SpaceKey(0)]),
        Vec::<SpaceKey>::new()
    );
}

#[test]
fn open_target_follows_mode_window_preference_and_ctrl() {
    let [w, other] = ids();
    let none = [(SpaceKey(0), Some(other))];
    let shown = [(SpaceKey(0), Some(other)), (SpaceKey(4), Some(w))];
    use OpenTarget::*;
    use OpenWorkspace::{Ask as AskPref, NewWindow, Replace as ReplacePref};
    use WindowMode::*;
    assert_eq!(open_target(w, Single, AskPref, false, &none), Switch);
    assert_eq!(open_target(w, Single, NewWindow, true, &none), Switch);
    assert_eq!(
        open_target(w, PerWorkspace, AskPref, false, &shown),
        Focus(SpaceKey(4))
    );
    assert_eq!(
        open_target(w, PerWorkspace, ReplacePref, true, &shown),
        Focus(SpaceKey(4))
    );
    assert_eq!(open_target(w, PerWorkspace, AskPref, false, &none), Ask);
    assert_eq!(open_target(w, PerWorkspace, AskPref, true, &none), New);
    assert_eq!(open_target(w, PerWorkspace, NewWindow, false, &none), New);
    assert_eq!(
        open_target(w, PerWorkspace, NewWindow, true, &none),
        Replace
    );
    assert_eq!(
        open_target(w, PerWorkspace, ReplacePref, false, &none),
        Replace
    );
    assert_eq!(open_target(w, PerWorkspace, ReplacePref, true, &none), New);
}

#[test]
fn open_target_counts_pending_windows() {
    // A window being opened is already listed (its HostUi exists before the OS window):
    // a second request focuses it instead of opening another.
    let w = Ulid::new();
    let pending = [(SpaceKey(9), Some(w))];
    assert_eq!(
        open_target(
            w,
            WindowMode::PerWorkspace,
            OpenWorkspace::NewWindow,
            false,
            &pending
        ),
        OpenTarget::Focus(SpaceKey(9))
    );
}

#[test]
fn a_remembered_choice_is_the_matching_preference() {
    assert_eq!(Choice::NewWindow.remembered(), OpenWorkspace::NewWindow);
    assert_eq!(Choice::ThisWindow.remembered(), OpenWorkspace::Replace);
}

#[test]
fn the_picker_starts_on_the_shown_workspace_ticked() {
    let list = rows(&["api", "web", "docs"]);
    let picker = WorkspacePicker::new(list.clone(), Some(list[1].id));
    assert!(picker.is_ticked(list[1].id));
    assert!(!picker.is_ticked(list[0].id));
    assert_eq!(picker.button_label(), "Switch");
    assert!(picker.can_confirm());
}

#[test]
fn arrows_hand_the_keyboard_to_the_list_and_space_ticks_there() {
    let list = rows(&["api", "web", "docs"]);
    let mut picker = WorkspacePicker::new(list.clone(), None);
    assert_eq!(picker.button_label(), "Pick a workspace");
    assert!(!picker.can_confirm());
    press(&mut picker, "space");
    assert_eq!(
        picker.filter(),
        " ",
        "space types while the filter has the keyboard"
    );
    press(&mut picker, "backspace");
    press(&mut picker, "down");
    assert!(picker.in_list());
    assert_eq!(picker.cursor(), 1);
    press(&mut picker, "space");
    assert!(picker.is_ticked(list[1].id));
    press(&mut picker, "down");
    press(&mut picker, "down");
    assert_eq!(picker.cursor(), 2, "no wrap");
    press(&mut picker, "space");
    assert_eq!(picker.button_label(), "Open 2 windows");
    press(&mut picker, "w");
    assert!(!picker.in_list(), "typing takes the keyboard back");
    assert_eq!(picker.filter(), "w");
}

#[test]
fn the_filter_matches_every_word_and_keeps_hidden_ticks() {
    let list = rows(&["api server", "web", "api docs"]);
    let mut picker = WorkspacePicker::new(list.clone(), Some(list[1].id));
    picker.insert("api d");
    let shown: Vec<&str> = picker
        .visible()
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    assert_eq!(shown, ["api docs"]);
    assert!(picker.is_ticked(list[1].id), "a hidden tick stays");
}

#[test]
fn alt_a_ticks_every_shown_row_then_unticks_them() {
    let list = rows(&["api", "web", "docs"]);
    let mut picker = WorkspacePicker::new(list.clone(), Some(list[0].id));
    press(&mut picker, "alt-a");
    assert!(list.iter().all(|row| picker.is_ticked(row.id)));
    press(&mut picker, "alt-a");
    assert!(list.iter().all(|row| !picker.is_ticked(row.id)));
}

#[test]
fn enter_with_several_ticked_asks_first_then_splits() {
    let list = rows(&["api", "web", "docs"]);
    let mut picker = WorkspacePicker::new(list.clone(), Some(list[0].id));
    picker.toggle(list[1].id);
    picker.toggle(list[2].id);
    assert_eq!(press(&mut picker, "enter"), PickOutcome::Edited);
    assert!(picker.confirming());
    let name = |id: Ulid| list.iter().find(|row| row.id == id).unwrap().name.clone();
    assert_eq!(
        picker.confirmation(name),
        "api stays in this window. web and docs each open in a new window."
    );
    press(&mut picker, "escape");
    assert!(!picker.confirming(), "Esc on the confirmation goes back");
    press(&mut picker, "enter");
    assert_eq!(
        press(&mut picker, "enter"),
        PickOutcome::Done(SplitPlan {
            keep: list[0].id,
            open: vec![list[1].id, list[2].id]
        })
    );
}

#[test]
fn enter_with_one_ticked_splits_at_once_and_with_none_does_nothing() {
    let list = rows(&["api", "web"]);
    let mut one = WorkspacePicker::new(list.clone(), Some(list[1].id));
    assert_eq!(
        press(&mut one, "enter"),
        PickOutcome::Done(SplitPlan {
            keep: list[1].id,
            open: vec![]
        })
    );
    let mut none = WorkspacePicker::new(list, None);
    assert_eq!(press(&mut none, "enter"), PickOutcome::Edited);
    assert!(!none.confirming());
    assert_eq!(press(&mut none, "escape"), PickOutcome::Close);
}

#[test]
fn confirmation_names_one_two_and_three_new_windows() {
    let list = rows(&["a", "b", "c", "d"]);
    let name = |id: Ulid| list.iter().find(|row| row.id == id).unwrap().name.clone();
    let mut picker = WorkspacePicker::new(list.clone(), Some(list[0].id));
    picker.toggle(list[1].id);
    assert_eq!(
        picker.confirmation(name),
        "a stays in this window. b opens in a new window."
    );
    picker.toggle(list[2].id);
    picker.toggle(list[3].id);
    assert_eq!(
        picker.confirmation(name),
        "a stays in this window. b, c and d each open in a new window."
    );
}

fn entry(project: Ulid, workspace: &str, title: &str, at: i64) -> Entry {
    Entry {
        session_id: Ulid::new(),
        project_id: project,
        workspace: workspace.to_owned(),
        agent: "claude".to_owned(),
        title: Some(title.to_owned()),
        state: SessionState::NeedsInput,
        state_source: StateSource::Hook,
        state_changed_at: at,
    }
}

#[test]
fn need_you_counts_other_workspaces_only_newest_first() {
    let [api, web, docs] = ids();
    let entries = [
        entry(api, "api", "fix login", 1),
        entry(web, "web", "tests", 2),
        entry(web, "web", "lint", 5),
    ];
    let one = need_you(&entries, Some(api)).unwrap();
    assert_eq!(one.count, 2);
    assert_eq!(one.label, "2 need you · web");
    assert_eq!(one.rows[0].text, "web · lint · needs input");
    let mixed = [
        entries[0].clone(),
        entry(docs, "docs", "x", 3),
        entries[1].clone(),
    ];
    assert_eq!(
        need_you(&mixed, Some(web)).unwrap().label,
        "2 need you · 2 workspaces"
    );
    assert_eq!(need_you(&entries[..1], Some(api)), None);
}
