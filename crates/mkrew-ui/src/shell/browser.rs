//! Browser panes (ADR 0110): the Shell's page state per web leaf, and the pane's body.

use crate::*;
use mkrew_ui::browser::Page;

/// One browser pane's live state, keyed by its leaf id in `Shell.browsers`.
pub(crate) struct Browser {
    pub(crate) page: Page,
}

impl Shell {
    /// The workspace whose tabs hold browser pane `id`.
    pub(crate) fn web_project(&self, id: Ulid) -> Option<Ulid> {
        let key = PaneKey::Web(id);
        (self.tabs.iter())
            .find(|(_, tabs)| tabs.tab_with(&key).is_some())
            .map(|(project, _)| *project)
    }

    /// Every web leaf of every layout has its state, and only those: a closed pane's page goes.
    pub(crate) fn sync_browsers(&mut self) {
        let mut leaves: HashMap<Ulid, String> = HashMap::new();
        for (project, tabs) in &self.tabs {
            // Tabs still loading list no leaves yet: nothing may be dropped.
            if self.loading_tabs.contains(project) {
                return;
            }
            for tab in tabs.tabs() {
                let Some(layout) = tab.layout() else { continue };
                for key in tab.keys() {
                    if let (PaneKey::Web(id), Some(Leaf::Web { web, .. })) =
                        (&key, layout.leaf_of(&key))
                    {
                        leaves.insert(*id, web.clone());
                    }
                }
            }
        }
        self.browsers.retain(|id, _| leaves.contains_key(id));
        for (id, url) in leaves {
            self.browsers.entry(id).or_insert_with(|| Browser {
                page: Page::new(url),
            });
        }
    }

    /// Closes browser pane `id` (its tab goes with its last pane).
    pub(crate) fn close_web_pane(
        &mut self,
        id: Ulid,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if let Some(project_id) = self.web_project(id) {
            self.remove_leaf(project_id, PaneKey::Web(id), window, cx);
        }
    }

    /// The body of browser pane `id`: the empty state until a page is drawn.
    pub(crate) fn render_web_pane(
        &self,
        _project_id: Ulid,
        _id: Ulid,
        _is_focused: bool,
        theme: &Theme,
        _window: &Window,
        _cx: &mut Context<'_, Self>,
    ) -> gpui::AnyElement {
        empty_state(
            theme,
            None,
            None,
            "Type an address or paste a link".into(),
            None,
        )
        .size_full()
        .into_any_element()
    }
}
