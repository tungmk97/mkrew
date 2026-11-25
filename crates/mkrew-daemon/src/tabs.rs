//! Edits of tab layouts the daemon makes itself (SPEC §5: the layout is the UI's split tree,
//! stored as opaque JSON; only session leaves and the splits around them are touched here, file
//! and diff leaves pass through).

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

/// Removes the pane of `session`; leaves without a `session` key (files, the diff) never match,
/// and a split around the removed pane collapses to its sibling of any kind.
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

/// Every session with a pane in `layout`: the leaves with a `session` key; file and diff leaves
/// are skipped.
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

    /// Takes the panes of `sessions` out of every tab of the project in one store lock; a tab
    /// they leave without panes closes unless pinned (it held only what a restart could not
    /// bring back). A tab that was empty already stays.
    pub fn drop_panes(&self, project_id: Ulid, sessions: &[Ulid]) -> Result<(), StoreError> {
        let updated = {
            let store = self.store();
            let mut updated = Vec::new();
            for tab in store.list_tabs(project_id)? {
                let cut = sessions.iter().fold(None, |cut: Option<Value>, &session| {
                    without_session(cut.as_ref().unwrap_or(&tab.layout), session).or(cut)
                });
                match cut {
                    None => {}
                    Some(Value::Null) if !tab.pinned => store.close_tab(tab.id)?,
                    Some(layout) => updated.push(store.set_tab_layout(tab.id, &layout)?),
                }
            }
            updated
        };
        for tab in updated {
            let _ = self.events.send(Event::TabUpdated(tab));
        }
        Ok(())
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

    #[test]
    fn a_surviving_pane_keeps_its_chat_flag() {
        let (a, b) = (Ulid::new(), Ulid::new());
        let chat = json!({"session": a.to_string(), "chat": true});
        let layout =
            json!({"split": "vertical", "ratio": 0.5, "children": [chat.clone(), leaf(b)]});
        assert_eq!(without_session(&layout, b), Some(chat));
    }

    fn file(path: &str) -> Value {
        json!({"file": path})
    }

    fn diff() -> Value {
        json!({"diff": true})
    }

    fn split(axis: &str, first: Value, second: Value) -> Value {
        json!({"split": axis, "ratio": 0.5, "children": [first, second]})
    }

    fn web(id: Ulid) -> Value {
        json!({"web": "https://x.dev", "id": id.to_string()})
    }

    #[test]
    fn web_leaves_are_kept_like_file_leaves() {
        let [a, page] = [Ulid::new(), Ulid::new()];
        assert!(layout_sessions(&web(page)).is_empty());
        assert_eq!(layout_sessions(&split("vertical", leaf(a), web(page))), [a]);
        // The session goes, the browser pane stays and its tab with it.
        assert_eq!(
            without_session(&split("vertical", leaf(a), web(page)), a),
            Some(web(page))
        );
        // A web leaf whose id is the session's id is still a web leaf.
        assert_eq!(without_session(&web(a), a), None);
    }

    #[test]
    fn layout_sessions_skips_file_and_diff_leaves() {
        let [a, b] = [Ulid::new(), Ulid::new()];
        let layout = split(
            "vertical",
            leaf(a),
            split(
                "horizontal",
                file("src/a.rs"),
                split("vertical", diff(), leaf(b)),
            ),
        );
        assert_eq!(layout_sessions(&layout), [a, b]);
        assert!(layout_sessions(&file("src/a.rs")).is_empty());
        assert!(layout_sessions(&diff()).is_empty());
        // A tab of files only has no session for `tab.close` to remove.
        assert!(layout_sessions(&split("vertical", file("a"), diff())).is_empty());
        // A file named like a session id is still a file.
        assert!(layout_sessions(&file(&a.to_string())).is_empty());
    }

    #[test]
    fn cut_leaves_file_and_diff_leaves_alone() {
        let a = Ulid::new();
        assert_eq!(without_session(&file("src/a.rs"), a), None);
        assert_eq!(without_session(&diff(), a), None);
        assert_eq!(
            without_session(&split("vertical", file("src/a.rs"), diff()), a),
            None
        );
        // A file leaf with the id as its path is not the session.
        assert_eq!(without_session(&file(&a.to_string()), a), None);
    }

    #[test]
    fn cut_collapses_a_split_around_a_leaf_of_any_kind() {
        let a = Ulid::new();
        // The sibling is a file or the diff: it becomes the whole layout, from either side.
        assert_eq!(
            without_session(&split("vertical", leaf(a), file("x")), a),
            Some(file("x"))
        );
        assert_eq!(
            without_session(&split("vertical", diff(), leaf(a)), a),
            Some(diff())
        );
        // Deeper: the inner split goes, the outer ratio stays.
        let nested = json!({"split": "vertical", "ratio": 0.3, "children": [
            split("horizontal", leaf(a), file("x")),
            diff()
        ]});
        assert_eq!(
            without_session(&nested, a),
            Some(json!({"split": "vertical", "ratio": 0.3, "children": [file("x"), diff()]}))
        );
        // The session beside a whole subtree of files: the subtree takes the tab.
        let beside = split("vertical", leaf(a), split("horizontal", file("x"), diff()));
        assert_eq!(
            without_session(&beside, a),
            Some(split("horizontal", file("x"), diff()))
        );
        // A file leaf keeps its own keys.
        let full = json!({"file": "README.md", "md": "split", "preview": true});
        assert_eq!(
            without_session(&split("vertical", leaf(a), full.clone()), a),
            Some(full)
        );
    }

    #[test]
    fn a_tab_that_loses_its_last_session_keeps_its_file_leaf() {
        // `drop_panes` closes a tab only when the cut layout is `null`: a tab that still holds
        // a file or the diff is written back with that leaf as its root, and stays.
        let a = Ulid::new();
        let cut = without_session(&split("vertical", leaf(a), file("x")), a);
        assert_eq!(cut, Some(file("x")));
        assert_ne!(cut, Some(Value::Null));
        let cut = without_session(&split("vertical", diff(), leaf(a)), a);
        assert_eq!(cut, Some(diff()));
        // What is left has no session: `tab.close` of it removes none.
        assert!(layout_sessions(&cut.unwrap()).is_empty());
        // Only a tab whose last leaf was the session ends up empty (and closes unless pinned).
        assert_eq!(without_session(&leaf(a), a), Some(Value::Null));
    }
}
