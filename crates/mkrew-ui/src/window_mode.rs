//! Window modes (ADR 0113): the pure decisions behind one window per workspace. Which window
//! keeps which workspace on a split, which windows close on a combine, where opening a
//! workspace goes, the "Open workspaces in windows" picker and the "need you" chip. Pure and
//! tested headlessly; `shell/window_mode.rs` draws and runs them.

use gpui::Keystroke;
use mkrew_core::settings::{OpenWorkspace, WindowMode};
use ulid::Ulid;

use crate::detached::SpaceKey;
use crate::inbox::Entry;
use crate::sidebar;
use crate::task_dialog::{DialogKey, dialog_key};
use crate::text_box::Input;

#[cfg(test)]
mod tests;

/// What a split does: the asking window shows `keep`, each of `open` gets a new window, in
/// this order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitPlan {
    pub keep: Ulid,
    pub open: Vec<Ulid>,
}

/// The split of the `ticked` workspaces (any order) of `listed` (list order). The asking
/// window keeps `shown` when it is ticked, else the first ticked one. `None` with none ticked.
pub fn plan_split(listed: &[Ulid], ticked: &[Ulid], shown: Option<Ulid>) -> Option<SplitPlan> {
    let ordered: Vec<Ulid> = (listed.iter().copied())
        .filter(|id| ticked.contains(id))
        .collect();
    let keep = shown
        .filter(|id| ordered.contains(id))
        .or_else(|| ordered.first().copied())?;
    let open = ordered.into_iter().filter(|id| *id != keep).collect();
    Some(SplitPlan { keep, open })
}

/// The Space windows a combine into `keep` closes.
pub fn plan_combine(keep: SpaceKey, spaces: &[SpaceKey]) -> Vec<SpaceKey> {
    spaces.iter().copied().filter(|key| *key != keep).collect()
}

/// Where opening a workspace goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenTarget {
    /// Mode 1: the single window switches to it.
    Switch,
    /// Its window (open or being opened) comes forward.
    Focus(SpaceKey),
    New,
    /// The asking window shows it instead of its workspace.
    Replace,
    Ask,
}

/// Where `workspace` opens. `spaces` lists every Space window with its workspace, the ones
/// being opened included. Ctrl takes the other way of a saved choice and skips the question.
pub fn open_target(
    workspace: Ulid,
    mode: WindowMode,
    pref: OpenWorkspace,
    ctrl: bool,
    spaces: &[(SpaceKey, Option<Ulid>)],
) -> OpenTarget {
    if mode == WindowMode::Single {
        return OpenTarget::Switch;
    }
    if let Some((key, _)) = spaces.iter().find(|(_, shown)| *shown == Some(workspace)) {
        return OpenTarget::Focus(*key);
    }
    match (pref, ctrl) {
        (OpenWorkspace::Ask, false) => OpenTarget::Ask,
        (OpenWorkspace::Ask | OpenWorkspace::Replace, true) | (OpenWorkspace::NewWindow, false) => {
            OpenTarget::New
        }
        (OpenWorkspace::NewWindow, true) | (OpenWorkspace::Replace, false) => OpenTarget::Replace,
    }
}

/// A button of the "Open <workspace>" question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    NewWindow,
    ThisWindow,
}

impl Choice {
    /// What "Don't ask again" saves.
    pub fn remembered(self) -> OpenWorkspace {
        match self {
            Choice::NewWindow => OpenWorkspace::NewWindow,
            Choice::ThisWindow => OpenWorkspace::Replace,
        }
    }
}

/// One workspace row of the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickRow {
    pub id: Ulid,
    pub name: String,
    pub color: Option<String>,
    /// Working agents and sessions asking for input, as the sidebar card shows them.
    pub working: usize,
    pub asking: usize,
}

/// What a key press in the picker means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    Edited,
    Close,
    Done(SplitPlan),
    /// Ctrl+V: the caller inserts the clipboard text.
    Paste,
}

/// The open "Open workspaces in windows" picker. Keys follow the Broadcast dialog: typing
/// edits the filter; ↑ / ↓ move the highlight and hand the keyboard to the list, where Space
/// ticks; any other key gives it back to the filter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePicker {
    rows: Vec<PickRow>,
    ticked: Vec<Ulid>,
    shown: Option<Ulid>,
    filter: Input,
    /// Index into `visible()`.
    cursor: usize,
    in_list: bool,
    confirming: bool,
}

impl WorkspacePicker {
    /// `rows` in list order; `shown` (the asking window's workspace) starts ticked.
    pub fn new(rows: Vec<PickRow>, shown: Option<Ulid>) -> Self {
        let ticked = (shown.into_iter())
            .filter(|id| rows.iter().any(|row| row.id == *id))
            .collect();
        Self {
            rows,
            ticked,
            shown,
            filter: Input::default(),
            cursor: 0,
            in_list: false,
            confirming: false,
        }
    }

    pub fn rows(&self) -> &[PickRow] {
        &self.rows
    }

    pub fn filter(&self) -> &str {
        self.filter.as_str()
    }

    pub fn filter_input(&self) -> &Input {
        &self.filter
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn in_list(&self) -> bool {
        self.in_list
    }

    pub fn confirming(&self) -> bool {
        self.confirming
    }

    /// The rows whose name holds every word of the filter (any case).
    pub fn visible(&self) -> Vec<&PickRow> {
        let words: Vec<String> = (self.filter.split_whitespace())
            .map(str::to_lowercase)
            .collect();
        (self.rows.iter())
            .filter(|row| {
                let name = row.name.to_lowercase();
                words.iter().all(|word| name.contains(word.as_str()))
            })
            .collect()
    }

    pub fn is_ticked(&self, id: Ulid) -> bool {
        self.ticked.contains(&id)
    }

    /// Ticks or unticks `id` and moves the highlight there when it is shown.
    pub fn toggle(&mut self, id: Ulid) {
        if let Some(at) = self.ticked.iter().position(|ticked| *ticked == id) {
            self.ticked.remove(at);
        } else {
            self.ticked.push(id);
        }
        if let Some(at) = self.visible().iter().position(|row| row.id == id) {
            self.cursor = at;
        }
        self.confirming = false;
    }

    /// Ticks every shown row, or unticks them when all are ticked.
    pub fn toggle_all_shown(&mut self) {
        let shown: Vec<Ulid> = self.visible().iter().map(|row| row.id).collect();
        if shown.iter().all(|id| self.ticked.contains(id)) {
            self.ticked.retain(|id| !shown.contains(id));
        } else {
            for id in shown {
                if !self.ticked.contains(&id) {
                    self.ticked.push(id);
                }
            }
        }
        self.confirming = false;
    }

    pub fn plan(&self) -> Option<SplitPlan> {
        let listed: Vec<Ulid> = self.rows.iter().map(|row| row.id).collect();
        plan_split(&listed, &self.ticked, self.shown)
    }

    pub fn can_confirm(&self) -> bool {
        !self.ticked.is_empty()
    }

    /// The primary button: what is missing, or what happens.
    pub fn button_label(&self) -> String {
        match self.ticked.len() {
            0 => "Pick a workspace".to_owned(),
            1 => "Switch".to_owned(),
            n => format!("Open {n} windows"),
        }
    }

    /// The confirmation step's sentence, naming workspaces with `name`.
    pub fn confirmation(&self, name: impl Fn(Ulid) -> String) -> String {
        let Some(plan) = self.plan() else {
            return String::new();
        };
        let kept = name(plan.keep);
        let names: Vec<String> = plan.open.iter().map(|id| name(*id)).collect();
        match names.as_slice() {
            [] => format!("{kept} stays in this window."),
            [one] => format!("{kept} stays in this window. {one} opens in a new window."),
            [rest @ .., last] => format!(
                "{kept} stays in this window. {} and {last} each open in a new window.",
                rest.join(", ")
            ),
        }
    }

    /// Back from the confirmation step.
    pub fn back(&mut self) {
        self.confirming = false;
    }

    /// Enter or the primary button: nothing with none ticked, the confirmation step first
    /// with several, else the plan.
    pub fn confirm(&mut self) -> PickOutcome {
        let several = self.ticked.len() > 1;
        if several && !self.confirming {
            self.confirming = true;
            return PickOutcome::Edited;
        }
        self.plan().map_or(PickOutcome::Edited, PickOutcome::Done)
    }

    pub fn key(&mut self, keystroke: &Keystroke) -> PickOutcome {
        let m = &keystroke.modifiers;
        if m.alt && !m.control && !m.platform && !m.shift && keystroke.key == "a" {
            self.toggle_all_shown();
            return PickOutcome::Edited;
        }
        if !m.modified() {
            let last = self.visible().len().saturating_sub(1);
            match keystroke.key.as_str() {
                "up" => {
                    self.cursor = self.cursor.saturating_sub(1);
                    self.in_list = true;
                    return PickOutcome::Edited;
                }
                "down" => {
                    self.cursor = (self.cursor + 1).min(last);
                    self.in_list = true;
                    return PickOutcome::Edited;
                }
                "space" if self.in_list => {
                    if let Some(id) = self.visible().get(self.cursor).map(|row| row.id) {
                        self.toggle(id);
                    }
                    return PickOutcome::Edited;
                }
                _ => {}
            }
        }
        self.in_list = false;
        match dialog_key(keystroke) {
            Some(DialogKey::Cancel) if self.confirming => {
                self.confirming = false;
                PickOutcome::Edited
            }
            Some(DialogKey::Cancel) => PickOutcome::Close,
            Some(DialogKey::Enter | DialogKey::Save) => self.confirm(),
            Some(DialogKey::Paste) => PickOutcome::Paste,
            Some(DialogKey::Insert(text)) => {
                self.insert(&text);
                PickOutcome::Edited
            }
            Some(DialogKey::Copy | DialogKey::SwitchField) | None => PickOutcome::Edited,
            Some(key) => {
                if self.filter.edit(&key) {
                    self.cursor = 0;
                }
                PickOutcome::Edited
            }
        }
    }

    /// Typed or pasted filter text (one line).
    pub fn insert(&mut self, text: &str) {
        let line: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        self.filter.insert(&line);
        self.cursor = 0;
        self.confirming = false;
    }
}

/// One session of the "need you" menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedRow {
    pub session: Ulid,
    pub project: Ulid,
    /// `<workspace> · <title> · <state>`.
    pub text: String,
}

/// The chip of a Space window in mode 2: the sessions of other workspaces that need the
/// user ([`crate::inbox::needs_you`], as listed by [`crate::inbox::entries`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NeedYou {
    pub count: usize,
    /// `N need you · web`, or `N need you · M workspaces`.
    pub label: String,
    /// `N`: what stays when the title bar runs out of room.
    pub short: String,
    /// Newest first.
    pub rows: Vec<NeedRow>,
}

/// `None` while no other workspace than `shown` needs the user.
pub fn need_you(entries: &[Entry], shown: Option<Ulid>) -> Option<NeedYou> {
    let mut others: Vec<&Entry> = (entries.iter())
        .filter(|entry| Some(entry.project_id) != shown)
        .collect();
    if others.is_empty() {
        return None;
    }
    others.sort_by_key(|entry| std::cmp::Reverse(entry.state_changed_at));
    let mut workspaces: Vec<Ulid> = others.iter().map(|entry| entry.project_id).collect();
    workspaces.sort();
    workspaces.dedup();
    let count = others.len();
    let place = match workspaces.len() {
        1 => others[0].workspace.clone(),
        n => format!("{n} workspaces"),
    };
    let rows = (others.iter())
        .map(|entry| NeedRow {
            session: entry.session_id,
            project: entry.project_id,
            text: format!(
                "{} · {} · {}",
                entry.workspace,
                entry.title.as_deref().unwrap_or(&entry.agent),
                sidebar::status_display(entry.state).label
            ),
        })
        .collect();
    Some(NeedYou {
        count,
        label: format!("{count} need you · {place}"),
        short: count.to_string(),
        rows,
    })
}
