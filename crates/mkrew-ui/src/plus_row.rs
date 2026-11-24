//! The tab strip's `+` row (SPEC §10, ADR 0108): which items it shows, what each one starts,
//! and when it is open. Pure and tested headlessly; `shell/plus_row.rs` draws it.

use crate::keymap::Command;
use crate::lifecycle::PaneAgent;

/// What an item of the `+` row starts, by click or by drop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewKind {
    /// An agent; `None` = the workspace's default agent.
    Agent(Option<String>),
    /// The workspace's default shell.
    Shell,
    /// A new note file (`fs.new_note`).
    Note,
    /// A browser pane (ADR 0110).
    Browser,
}

/// One item after `+` in the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowItem {
    /// The workspace's default agent profile.
    Agent {
        id: String,
        name: String,
        icon: Option<String>,
    },
    /// The workspace's default shell; `name` is its profile name when known.
    Shell {
        name: Option<String>,
    },
    Note,
    Browser,
    /// `▾`: today's full menu.
    More,
}

impl RowItem {
    /// What a click or a drop of the item starts; `None` for `▾`.
    pub fn kind(&self) -> Option<NewKind> {
        match self {
            RowItem::Agent { id, .. } => Some(NewKind::Agent(Some(id.clone()))),
            RowItem::Shell { .. } => Some(NewKind::Shell),
            RowItem::Note => Some(NewKind::Note),
            RowItem::Browser => Some(NewKind::Browser),
            RowItem::More => None,
        }
    }

    /// The tooltip text; the main window opens tabs, a detached window splits its tab.
    pub fn label(&self, main: bool) -> String {
        let what = if main { "tab" } else { "pane" };
        match self {
            RowItem::Agent { name, .. } => format!("New {name} {what}"),
            RowItem::Shell { name: Some(name) } => format!("New {name} {what}"),
            RowItem::Shell { name: None } => format!("New shell {what}"),
            RowItem::Note => "New note".to_owned(),
            RowItem::Browser => "New browser".to_owned(),
            RowItem::More => "More…".to_owned(),
        }
    }

    /// The shortcut the tooltip shows: the one that does the same in this window.
    pub fn command(&self, main: bool) -> Option<Command> {
        match (self, main) {
            (RowItem::Agent { .. }, true) => Some(Command::NewAgentTab),
            (RowItem::Agent { .. }, false) => Some(Command::SplitAgent),
            (RowItem::Shell { .. }, true) => Some(Command::NewShellTab),
            (RowItem::Shell { .. }, false) => Some(Command::SplitShell),
            (RowItem::Note, true) => Some(Command::NewNote),
            (RowItem::Browser, false) => Some(Command::SplitBrowser),
            (RowItem::Note, false) | (RowItem::Browser, true) | (RowItem::More, _) => None,
        }
    }
}

/// The agent the row offers: the workspace's marked default (`pane_agents` lists it first).
/// No installed profile, or none marked: no agent item, `▾` still lists them all.
pub fn default_agent(agents: &[PaneAgent]) -> Option<&PaneAgent> {
    agents.first().filter(|agent| agent.default)
}

/// The items after `+`, in order. `main`: the main window, which also has `▾` (a detached
/// window has Split ▾ in its title bar and no new-tab menu). `browser`: pages can show here.
pub fn plus_row_items(
    default_agent: Option<&PaneAgent>,
    shell_name: Option<&str>,
    main: bool,
    browser: bool,
) -> Vec<RowItem> {
    let mut items = Vec::with_capacity(5);
    if let Some(agent) = default_agent {
        items.push(RowItem::Agent {
            id: agent.id.clone(),
            name: agent.name.clone(),
            icon: agent.icon.clone(),
        });
    }
    items.push(RowItem::Shell {
        name: shell_name.map(str::to_owned),
    });
    items.push(RowItem::Note);
    if browser {
        items.push(RowItem::Browser);
    }
    if main {
        items.push(RowItem::More);
    }
    items
}

/// Whether the pointer holds the `+` row open: it opens at once and closes a delay after the
/// pointer left `+` and the row (the serial and timer pattern of `peek::Hover`). A focused
/// item opens it too, but that is read from the focus, not stored here.
#[derive(Debug, Default)]
pub struct Reveal {
    over: bool,
    shown: bool,
    serial: u64,
}

impl Reveal {
    /// The pointer entered `+` or the row: open, and any pending close is stale.
    pub fn enter(&mut self) {
        self.over = true;
        self.shown = true;
        self.serial += 1;
    }

    /// The pointer left both: returns the serial to give [`Reveal::collapse`] after the delay.
    pub fn leave(&mut self) -> u64 {
        self.over = false;
        self.serial += 1;
        self.serial
    }

    /// The delay of leave `serial` ran out: true when the row closed (the pointer did not come
    /// back and no later leave replaced this one).
    pub fn collapse(&mut self, serial: u64) -> bool {
        let due = self.shown && !self.over && serial == self.serial;
        if due {
            self.shown = false;
        }
        due
    }

    /// Esc: closed now, until the pointer enters again.
    pub fn dismiss(&mut self) {
        self.over = false;
        self.shown = false;
        self.serial += 1;
    }

    pub fn shown(&self) -> bool {
        self.shown
    }
}

/// One [`Reveal`] per window (`H` is the host id): hovering `+` in one window never opens the
/// row of another.
#[derive(Debug)]
pub struct Reveals<H> {
    by_host: std::collections::HashMap<H, Reveal>,
}

impl<H> Default for Reveals<H> {
    fn default() -> Self {
        Self {
            by_host: std::collections::HashMap::new(),
        }
    }
}

impl<H: std::hash::Hash + Eq> Reveals<H> {
    pub fn enter(&mut self, host: H) {
        self.by_host.entry(host).or_default().enter();
    }

    pub fn leave(&mut self, host: H) -> u64 {
        self.by_host.entry(host).or_default().leave()
    }

    pub fn collapse(&mut self, host: H, serial: u64) -> bool {
        self.by_host
            .get_mut(&host)
            .is_some_and(|reveal| reveal.collapse(serial))
    }

    pub fn dismiss(&mut self, host: H) {
        self.by_host.entry(host).or_default().dismiss();
    }

    pub fn shown(&self, host: H) -> bool {
        self.by_host.get(&host).is_some_and(Reveal::shown)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, default: bool) -> PaneAgent {
        PaneAgent {
            index: 0,
            id: id.into(),
            name: id.to_uppercase(),
            icon: None,
            default,
        }
    }

    #[test]
    fn the_row_lists_agent_shell_note_and_more() {
        let claude = agent("claude", true);
        let items = plus_row_items(Some(&claude), Some("PowerShell"), true, false);
        assert_eq!(
            items,
            [
                RowItem::Agent {
                    id: "claude".into(),
                    name: "CLAUDE".into(),
                    icon: None
                },
                RowItem::Shell {
                    name: Some("PowerShell".into())
                },
                RowItem::Note,
                RowItem::More,
            ]
        );
    }

    #[test]
    fn without_a_default_agent_the_row_starts_at_the_shell() {
        let items = plus_row_items(None, None, true, false);
        assert_eq!(
            items,
            [RowItem::Shell { name: None }, RowItem::Note, RowItem::More]
        );
    }

    #[test]
    fn the_browser_item_comes_after_note_when_pages_can_show() {
        let items = plus_row_items(None, None, true, true);
        assert_eq!(
            items,
            [
                RowItem::Shell { name: None },
                RowItem::Note,
                RowItem::Browser,
                RowItem::More
            ]
        );
        assert!(!plus_row_items(None, None, true, false).contains(&RowItem::Browser));
        assert_eq!(RowItem::Browser.kind(), Some(NewKind::Browser));
        assert_eq!(RowItem::Browser.label(true), "New browser");
        assert_eq!(RowItem::Browser.command(true), None, "a tab has no key");
        assert_eq!(RowItem::Browser.command(false), Some(Command::SplitBrowser));
        // A detached window splits.
        assert!(plus_row_items(None, None, false, true).contains(&RowItem::Browser));
    }

    #[test]
    fn only_a_marked_default_gives_the_agent_item() {
        assert_eq!(default_agent(&[]), None);
        let unmarked = [agent("codex", false), agent("claude", false)];
        assert_eq!(default_agent(&unmarked), None, "`▾` still lists them");
        let marked = [agent("claude", true), agent("codex", false)];
        assert_eq!(
            default_agent(&marked).map(|agent| agent.id.as_str()),
            Some("claude")
        );
    }

    #[test]
    fn a_detached_window_has_no_menu_item_and_splits_instead() {
        let claude = agent("claude", true);
        let items = plus_row_items(Some(&claude), None, false, false);
        assert!(!items.contains(&RowItem::More));
        assert_eq!(items[0].label(false), "New CLAUDE pane");
        assert_eq!(items[0].command(false), Some(Command::SplitAgent));
        assert_eq!(items[1].command(false), Some(Command::SplitShell));
        assert_eq!(
            RowItem::Note.command(false),
            None,
            "NewNote is main-window only"
        );
    }

    #[test]
    fn labels_and_shortcuts_follow_the_window() {
        let shell = RowItem::Shell {
            name: Some("zsh".into()),
        };
        assert_eq!(shell.label(true), "New zsh tab");
        assert_eq!(RowItem::Shell { name: None }.label(true), "New shell tab");
        assert_eq!(shell.command(true), Some(Command::NewShellTab));
        assert_eq!(RowItem::Note.label(true), "New note");
        assert_eq!(RowItem::Note.command(true), Some(Command::NewNote));
        assert_eq!(RowItem::More.label(true), "More…");
        assert_eq!(RowItem::More.command(true), None);
    }

    #[test]
    fn items_say_what_they_start() {
        let claude = RowItem::Agent {
            id: "claude".into(),
            name: "Claude".into(),
            icon: None,
        };
        assert_eq!(claude.kind(), Some(NewKind::Agent(Some("claude".into()))));
        assert_eq!(RowItem::Shell { name: None }.kind(), Some(NewKind::Shell));
        assert_eq!(RowItem::Note.kind(), Some(NewKind::Note));
        assert_eq!(
            RowItem::More.kind(),
            None,
            "`▾` opens the menu, it is no drag source"
        );
    }

    #[test]
    fn the_pointer_opens_the_row_and_the_delay_closes_it() {
        let mut row = Reveal::default();
        assert!(!row.shown());
        row.enter();
        assert!(row.shown());
        let serial = row.leave();
        assert!(row.shown(), "still open while the delay runs");
        assert!(row.collapse(serial));
        assert!(!row.shown());
    }

    #[test]
    fn coming_back_cancels_the_pending_close() {
        let mut row = Reveal::default();
        row.enter();
        let stale = row.leave();
        row.enter();
        assert!(!row.collapse(stale), "the pointer is over the row again");
        assert!(row.shown());
        let serial = row.leave();
        assert!(
            !row.collapse(serial.wrapping_sub(1)),
            "only the last leave closes"
        );
        assert!(row.collapse(serial));
    }

    #[test]
    fn escape_closes_it_until_the_next_hover() {
        let mut row = Reveal::default();
        row.enter();
        row.dismiss();
        assert!(!row.shown());
        row.enter();
        assert!(row.shown());
    }
}

#[cfg(test)]
mod reveals_tests {
    use super::*;

    #[test]
    fn each_window_has_its_own_row_state() {
        let mut rows = Reveals::<u8>::default();
        rows.enter(1);
        assert!(rows.shown(1));
        assert!(!rows.shown(2), "the other window's row stays closed");
        rows.enter(2);
        let serial = rows.leave(1);
        assert!(rows.collapse(1, serial));
        assert!(!rows.shown(1));
        assert!(rows.shown(2), "closing one leaves the other open");
        rows.dismiss(2);
        assert!(!rows.shown(2));
        assert!(
            !rows.collapse(3, 0),
            "a window never hovered has nothing to close"
        );
    }
}
