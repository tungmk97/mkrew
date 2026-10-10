//! Edits of tab layouts the daemon makes itself (SPEC §5: the layout is the UI's split tree,
//! stored as opaque JSON; only leaves and the splits around them are touched here).

use mkrew_protocol::Event;
use mkrew_store::StoreError;
use serde_json::Value;
use ulid::Ulid;

use crate::session::Hub;

/// What removing a pane did to one node of the tree.
enum Cut {
    /// The node was the pane itself.
    Gone,
    /// The node now looks like this (a split collapsed to its other child, or a child changed).
    Became(Value),
}

fn cut(node: &Value, session: &str) -> Option<Cut> {
    if node.get("session").and_then(Value::as_str) == Some(session) {
        return Some(Cut::Gone);
    }
    let children = node.get("children")?.as_array()?;
    // A split always has two children (SPEC §5); anything else is not ours to edit.
    let [first, second] = children.as_slice() else {
        return None;
    };
    for (index, (child, sibling)) in [(first, second), (second, first)].into_iter().enumerate() {
        match cut(child, session) {
            None => {}
            // The sibling takes the split's place, like closing a pane in the UI.
            Some(Cut::Gone) => return Some(Cut::Became(sibling.clone())),
            Some(Cut::Became(new)) => {
                let mut node = node.clone();
                // `index` is 0 or 1 and `children` has two entries (checked above).
                node["children"][index] = new;
                return Some(Cut::Became(node));
            }
        }
    }
    None
}

/// `layout` without the pane of `session`; `None` if it is not in it, `Some(Value::Null)` if
/// it was the last pane.
fn without_session(layout: &Value, session: Ulid) -> Option<Value> {
    match cut(layout, &session.to_string())? {
        Cut::Gone => Some(Value::Null),
        Cut::Became(layout) => Some(layout),
    }
}

/// Every session with a pane in `layout` (the leaves of the split tree).
pub fn layout_sessions(layout: &Value) -> Vec<Ulid> {
    if let Some(session) = layout.get("session").and_then(Value::as_str) {
        return session.parse().into_iter().collect();
    }
    let children = layout.get("children").and_then(Value::as_array);
    children
        .into_iter()
        .flatten()
        .flat_map(layout_sessions)
        .collect()
}

impl Hub {
    /// Rewrites every tab layout of the project with `edit` (`None` = leave that tab alone)
    /// and announces each changed tab as `tab.updated`. One store lock, so a concurrent
    /// `tab.set_layout` is not overwritten.
    pub fn edit_tabs(
        &self,
        project_id: Ulid,
        edit: impl Fn(&Value) -> Option<Value>,
    ) -> Result<(), StoreError> {
        let updated = {
            let store = self.store();
            store.list_tabs(project_id).and_then(|tabs| {
                tabs.into_iter()
                    .filter_map(|tab| {
                        let layout = edit(&tab.layout)?;
                        Some(store.set_tab_layout(tab.id, &layout))
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
        }?;
        for tab in updated {
            let _ = self.events.send(Event::TabUpdated(tab));
        }
        Ok(())
    }

    /// Takes the pane of `session_id` out of every tab of its project (a session that exited
    /// has nothing to show; its output stays reachable in History).
    pub fn leave_tabs(&self, project_id: Ulid, session_id: Ulid) {
        if let Err(e) = self.edit_tabs(project_id, |l| without_session(l, session_id)) {
            tracing::warn!(%session_id, "pane not removed from its tab: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};
    use ulid::Ulid;

    use super::{layout_sessions, without_session};

    fn leaf(id: Ulid) -> Value {
        json!({"session": id.to_string()})
    }

    #[test]
    fn a_leaving_pane_gives_its_place_to_its_sibling() {
        let (a, b, c) = (Ulid::new(), Ulid::new(), Ulid::new());
        let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
            leaf(a),
            {"split": "horizontal", "ratio": 0.6, "children": [leaf(b), leaf(c)]}
        ]});
        // An inner pane: its split collapses to the sibling, the outer ratio stays.
        assert_eq!(
            without_session(&layout, c),
            Some(json!({"split": "vertical", "ratio": 0.5, "children": [leaf(a), leaf(b)]}))
        );
        // An outer pane: the other subtree becomes the whole layout.
        assert_eq!(
            without_session(&layout, a),
            Some(json!({"split": "horizontal", "ratio": 0.6, "children": [leaf(b), leaf(c)]}))
        );
        // The last pane leaves an empty tab (`null`, like a new tab).
        assert_eq!(without_session(&leaf(a), a), Some(Value::Null));
        // Not in the layout: nothing to write.
        assert_eq!(without_session(&layout, Ulid::new()), None);
        assert_eq!(without_session(&Value::Null, a), None);
    }

    #[test]
    fn layout_sessions_lists_every_leaf() {
        let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
        let layout = json!({"split": "vertical", "ratio": 0.5, "children": [
            leaf(a),
            {"split": "horizontal", "ratio": 0.6, "children": [leaf(b), leaf(c)]}
        ]});
        assert_eq!(layout_sessions(&layout), [a, b, c]);
        assert!(layout_sessions(&Value::Null).is_empty());
    }
}
