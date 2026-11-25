//! The TSP document (spec 6.3): the node tree frames of ops build, applied the way omp's own
//! reference document does (`packages/tui/src/native/apply.ts`, reference section 3.2).
//!
//! A failing op is skipped and reported; the rest of its frame still runs. Any node kind is
//! accepted: omp sends kinds newer than the hello's list (omp 19's `button` and `menu`), and one
//! unknown kind must not drop a subtree; the UI draws a kind it does not know generically
//! (ADR 0118).

use std::collections::{HashMap, HashSet};

use mkrew_protocol::TspNode;
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// Nodes per document.
pub const MAX_NODES: usize = 20_000;
/// JSON bytes of string props per document (a string counts as its escaped length, see `weight`).
/// The `tsp.snapshot` reply must fit the 16 MiB protocol frame. Measured on real omp: a node is
/// ~110 B of JSON besides its text on average (p95 ~170 B), so one full inline surface plus 5.3 MiB
/// of base64 blobs is ~10 MB, and three full surfaces at the average are ~15 MiB. Beyond that
/// (denser nodes) the daemon drops the blobs, then refuses the snapshot ("snapshot too large")
/// instead of breaking the connection; 170 B nodes are such a case.
pub const MAX_TEXT: usize = 1024 * 1024;
/// Id of the synthetic first row of `main` once old rows were evicted.
pub const TRIM_ID: &str = "mkrew.trimmed";
const TRIM_TEXT: &str = "Earlier messages were trimmed";
/// omp 19.1.1's `TSP_TEXT_KINDS`: the kinds `text` and `splice` ops edit.
pub const TEXT_KINDS: &[&str] = &[
    "text", "md", "code", "ansi", "math", "editor", "input", "shimmer", "doc",
];
/// Deepest allowed nesting below the root; a deeper `add` or `move` fails.
pub const MAX_DEPTH: usize = 64;
/// Stands in for a missing op argument.
static NULL: Value = Value::Null;

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub props: Map<String, Value>,
    pub children: Vec<String>,
    pub parent: Option<String>,
    pub settled: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    nodes: HashMap<String, Node>,
    /// The surface id; the root node is a `col` with this id.
    root: String,
    focus: Option<String>,
    text_bytes: usize,
    max_nodes: usize,
    max_text: usize,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Applied {
    /// The ops that applied, in order (what clients must replay).
    pub ops: Vec<Value>,
    /// One `op <index>: <message>` per op that failed.
    pub errors: Vec<String>,
}

impl Document {
    pub fn new(surface: &str) -> Self {
        let mut doc = Self::bare(surface);
        doc.insert(
            TspNode {
                id: surface.to_owned(),
                k: "col".to_owned(),
                p: Map::new(),
                c: Vec::new(),
            },
            None,
        );
        doc
    }

    /// A mirror built from a snapshot tree. Focus and settled flags are not part of the tree,
    /// so the mirror starts without them.
    pub fn from_tree(root: &TspNode) -> Self {
        let mut doc = Self::bare(&root.id);
        doc.insert(root.clone(), None);
        doc
    }

    pub fn to_tree(&self) -> TspNode {
        // The root is inserted by every constructor and `del`/`move` refuse to touch it.
        self.tree(&self.nodes[&self.root])
    }

    fn tree(&self, node: &Node) -> TspNode {
        TspNode {
            id: node.id.clone(),
            k: node.kind.clone(),
            p: node.props.clone(),
            c: self.children(&node.id).map(|c| self.tree(c)).collect(),
        }
    }

    fn bare(surface: &str) -> Self {
        Self {
            nodes: HashMap::new(),
            root: surface.to_owned(),
            focus: None,
            text_bytes: 0,
            max_nodes: MAX_NODES,
            max_text: MAX_TEXT,
        }
    }

    /// Overrides the eviction bounds (tests of the engine use small ones).
    pub fn set_limits(&mut self, max_nodes: usize, max_text: usize) {
        self.max_nodes = max_nodes;
        self.max_text = max_text;
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn get(&self, id: &str) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn children<'a>(&'a self, id: &str) -> impl Iterator<Item = &'a Node> + use<'a> {
        self.nodes
            .get(id)
            .into_iter()
            .flat_map(|node| node.children.iter())
            .filter_map(|child| self.nodes.get(child))
    }

    /// The `text` prop of a text-bearing node.
    pub fn text(&self, id: &str) -> Option<&str> {
        self.nodes
            .get(id)
            .filter(|node| TEXT_KINDS.contains(&node.kind.as_str()))
            .and_then(|node| node.props.get("text"))
            .and_then(Value::as_str)
    }

    pub fn focus(&self) -> Option<&str> {
        self.focus.as_deref()
    }

    /// Node count; never zero, the root always exists.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn text_bytes(&self) -> usize {
        self.text_bytes
    }

    /// Runs every op; a failing op is skipped, the rest still run.
    pub fn apply(&mut self, ops: &[Value]) -> Applied {
        let mut applied = Applied::default();
        for (index, op) in ops.iter().enumerate() {
            match self.run(op) {
                Ok(()) => applied.ops.push(op.clone()),
                Err(message) => applied.errors.push(format!("op {index}: {message}")),
            }
        }
        applied
    }

    fn run(&mut self, op: &Value) -> Result<(), String> {
        let args = op.as_array().ok_or("op is not an array")?;
        let verb = args
            .first()
            .and_then(Value::as_str)
            .ok_or("op has no verb")?;
        match verb {
            "add" => {
                let node = args.get(4).ok_or("argument 4 must be a node")?;
                self.add(str_at(args, 1)?, str_at(args, 2)?, opt_at(args, 3)?, node)
            }
            "set" => self.set(str_at(args, 1)?, args.get(2).unwrap_or(&NULL)),
            "text" => self.text_op(str_at(args, 1)?, str_at(args, 2)?, str_at(args, 3)?),
            "splice" => self.splice(
                str_at(args, 1)?,
                args.get(2).unwrap_or(&NULL),
                args.get(3).unwrap_or(&NULL),
                args.get(4).unwrap_or(&NULL),
            ),
            "move" => self.mv(str_at(args, 1)?, str_at(args, 2)?, opt_at(args, 3)?),
            "del" => self.del(str_at(args, 1)?),
            "settle" => {
                let id = str_at(args, 1)?;
                self.nodes
                    .get_mut(id)
                    .ok_or_else(|| format!("unknown id {id}"))?
                    .settled = true;
                Ok(())
            }
            "focus" => match opt_at(args, 1)? {
                None => {
                    self.focus = None;
                    Ok(())
                }
                Some(id) => {
                    self.node(id)?;
                    self.focus = Some(id.to_owned());
                    Ok(())
                }
            },
            // Accepted and forwarded; the document does not change (the UI scrolls).
            "reveal" | "scroll" => self.node(str_at(args, 1)?).map(|_| ()),
            "suspend" | "resume" => Ok(()),
            other => Err(format!("unknown op {other}")),
        }
    }

    fn node(&self, id: &str) -> Result<&Node, String> {
        self.nodes.get(id).ok_or_else(|| format!("unknown id {id}"))
    }

    /// Index in `parent`'s children where a node placed `before` another (or last) goes.
    fn index(&self, parent: &str, before: Option<&str>) -> Result<usize, String> {
        let kids = &self.node(parent)?.children;
        match before {
            None => Ok(kids.len()),
            Some(b) => kids
                .iter()
                .position(|c| c == b)
                .ok_or_else(|| format!("{b} is not a child of {parent}")),
        }
    }

    fn add(
        &mut self,
        id: &str,
        parent: &str,
        before: Option<&str>,
        node: &Value,
    ) -> Result<(), String> {
        let node = TspNode::deserialize(node).map_err(|e| format!("bad node: {e}"))?;
        if node.id != id {
            return Err(format!("add id {id} does not match node id {}", node.id));
        }
        self.node(parent)?;
        let at = self.index(parent, before)?;
        self.check(&node, &mut HashSet::new(), self.depth(parent) + 1)?;
        self.insert(node, Some(parent));
        if let Some(parent_node) = self.nodes.get_mut(parent) {
            parent_node.children.insert(at, id.to_owned());
        }
        self.unsettle(parent);
        Ok(())
    }

    /// Validates a whole subtree before anything is inserted, so a failed `add` changes nothing.
    fn check(
        &self,
        node: &TspNode,
        seen: &mut HashSet<String>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("too deep".to_owned());
        }
        if node.id.is_empty() {
            return Err("node without id".to_owned());
        }
        if self.nodes.contains_key(&node.id) || !seen.insert(node.id.clone()) {
            return Err(format!("duplicate id {}", node.id));
        }
        node.c
            .iter()
            .try_for_each(|child| self.check(child, seen, depth + 1))
    }

    /// Edges between a node and the root.
    fn depth(&self, id: &str) -> usize {
        let mut depth = 0;
        let mut at = self.nodes.get(id).and_then(|n| n.parent.as_deref());
        while let Some(parent) = at {
            depth += 1;
            at = self.nodes.get(parent).and_then(|n| n.parent.as_deref());
        }
        depth
    }

    /// Edges between a node and its deepest descendant.
    fn height(&self, id: &str) -> usize {
        let mut tallest = 0;
        let mut stack = vec![(id, 0)];
        while let Some((current, below)) = stack.pop() {
            tallest = tallest.max(below);
            if let Some(node) = self.nodes.get(current) {
                stack.extend(node.children.iter().map(|c| (c.as_str(), below + 1)));
            }
        }
        tallest
    }

    /// Inserts a checked subtree into the arena (not into the parent's child list).
    fn insert(&mut self, node: TspNode, parent: Option<&str>) {
        let TspNode { id, k, mut p, c } = node;
        p.retain(|_, v| !v.is_null());
        self.text_bytes += weight(&p);
        let children = c.iter().map(|child| child.id.clone()).collect();
        self.nodes.insert(
            id.clone(),
            Node {
                id: id.clone(),
                kind: k,
                props: p,
                children,
                parent: parent.map(str::to_owned),
                settled: false,
            },
        );
        for child in c {
            self.insert(child, Some(&id));
        }
    }

    /// Runs `change` on a node, keeps `text_bytes` in step and clears the settled flags up the tree.
    fn edit(
        &mut self,
        id: &str,
        change: impl FnOnce(&mut Node) -> Result<(), String>,
    ) -> Result<(), String> {
        let node = self
            .nodes
            .get_mut(id)
            .ok_or_else(|| format!("unknown id {id}"))?;
        let before = weight(&node.props);
        let result = change(&mut *node);
        let after = weight(&node.props);
        self.text_bytes = (self.text_bytes + after).saturating_sub(before);
        result?;
        self.unsettle(id);
        Ok(())
    }

    /// omp clears `settled` on the changed node and all its ancestors.
    fn unsettle(&mut self, id: &str) {
        let mut at = Some(id.to_owned());
        while let Some(current) = at {
            let Some(node) = self.nodes.get_mut(&current) else {
                break;
            };
            node.settled = false;
            at = node.parent.clone();
        }
    }

    fn set(&mut self, id: &str, props: &Value) -> Result<(), String> {
        let Value::Object(props) = props else {
            return Err("set props must be an object".to_owned());
        };
        self.edit(id, |node| {
            for (key, value) in props {
                if value.is_null() {
                    node.props.remove(key);
                } else {
                    node.props.insert(key.clone(), value.clone());
                }
            }
            Ok(())
        })
    }

    fn text_op(&mut self, id: &str, mode: &str, text: &str) -> Result<(), String> {
        // Streaming appends are the common case: count only the appended piece, not the whole text.
        if mode == "append"
            && let Some(node) = self.nodes.get_mut(id)
            && TEXT_KINDS.contains(&node.kind.as_str())
            && let Some(Value::String(current)) = node.props.get_mut("text")
        {
            current.push_str(text);
            self.text_bytes += str_weight(text);
            self.unsettle(id);
            return Ok(());
        }
        self.edit(id, |node| {
            text_kind(node)?;
            let append = match mode {
                "append" => true,
                "replace" => false,
                other => return Err(format!("unknown text mode {other}")),
            };
            if append && let Some(Value::String(current)) = node.props.get_mut("text") {
                current.push_str(text);
            } else {
                node.props
                    .insert("text".to_owned(), Value::String(text.to_owned()));
            }
            Ok(())
        })
    }

    fn splice(
        &mut self,
        id: &str,
        from: &Value,
        del: &Value,
        insert: &Value,
    ) -> Result<(), String> {
        let (Some(from), Some(del), Some(insert)) = (from.as_u64(), del.as_u64(), insert.as_str())
        else {
            return Err("splice needs integer from and delCount and a string".to_owned());
        };
        self.edit(id, |node| {
            text_kind(node)?;
            let current = node.props.get("text").and_then(Value::as_str).unwrap_or("");
            let len = current.encode_utf16().count();
            let to = from
                .checked_add(del)
                .filter(|&to| to <= len as u64)
                .ok_or_else(|| format!("splice range {from}+{del} outside text of length {len}"))?;
            let (Some(start), Some(end)) = (
                byte_at(current, from as usize),
                byte_at(current, to as usize),
            ) else {
                // ponytail: JS can cut a surrogate pair in half, a Rust String cannot; the op is
                // skipped (and reported) until omp is seen to send such a splice.
                return Err("splice range splits a surrogate pair".to_owned());
            };
            if let Some(Value::String(text)) = node.props.get_mut("text") {
                text.replace_range(start..end, insert);
            } else {
                // No text yet: the range is 0..0 and the insert becomes the text.
                node.props
                    .insert("text".to_owned(), Value::String(insert.to_owned()));
            }
            Ok(())
        })
    }

    fn mv(&mut self, id: &str, parent: &str, before: Option<&str>) -> Result<(), String> {
        if id == self.root {
            return Err("cannot move the root".to_owned());
        }
        let Some(old_parent) = self.node(id)?.parent.clone() else {
            return Err("cannot move the root".to_owned());
        };
        self.node(parent)?;
        let mut up = Some(parent);
        while let Some(ancestor) = up {
            if ancestor == id {
                return Err(format!("cannot move {id} into its own subtree"));
            }
            up = self.nodes.get(ancestor).and_then(|n| n.parent.as_deref());
        }
        if before == Some(id) {
            return Err("move before itself".to_owned());
        }
        let deepest = self.depth(parent) + 1 + self.height(id);
        if deepest > MAX_DEPTH {
            return Err("too deep".to_owned());
        }
        // Validates `before` while the tree is still untouched.
        self.index(parent, before)?;
        if let Some(old) = self.nodes.get_mut(&old_parent) {
            old.children.retain(|c| c != id);
        }
        let at = self.index(parent, before)?;
        if let Some(new) = self.nodes.get_mut(parent) {
            new.children.insert(at, id.to_owned());
        }
        if let Some(node) = self.nodes.get_mut(id) {
            node.parent = Some(parent.to_owned());
        }
        self.unsettle(&old_parent);
        self.unsettle(id);
        Ok(())
    }

    fn del(&mut self, id: &str) -> Result<(), String> {
        if id == self.root {
            return Err("cannot delete the root".to_owned());
        }
        let Some(parent) = self.node(id)?.parent.clone() else {
            return Err("cannot delete the root".to_owned());
        };
        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            parent_node.children.retain(|c| c != id);
        }
        self.drop_subtree(id);
        self.unsettle(&parent);
        Ok(())
    }

    /// Removes a node and everything below it from the arena (not from the parent's child list).
    fn drop_subtree(&mut self, id: &str) {
        let mut stack = vec![id.to_owned()];
        while let Some(current) = stack.pop() {
            let Some(node) = self.nodes.remove(&current) else {
                continue;
            };
            self.text_bytes = self.text_bytes.saturating_sub(weight(&node.props));
            if self.focus.as_deref() == Some(current.as_str()) {
                self.focus = None;
            }
            stack.extend(node.children);
        }
    }

    /// Over the node or text bound: removes the oldest top-level children of `main` (never
    /// `dock`, `layer`, or the trim row) until the document is under 80% of both bounds, with room
    /// for the trim row. Returns the removed ids and the ops to forward to clients: one `del`
    /// per removed id, then the `add` of the trim row at the top of `main` when it was absent.
    pub fn evict(&mut self) -> (Vec<String>, Vec<Value>) {
        self.evict_with(self.max_nodes, self.max_text)
    }

    /// Still over a bound after [`Document::evict`]: only `dock` and `layer` hold the excess.
    pub fn over_bounds(&self) -> bool {
        self.nodes.len() > self.max_nodes || self.text_bytes > self.max_text
    }

    fn evict_with(&mut self, max_nodes: usize, max_text: usize) -> (Vec<String>, Vec<Value>) {
        if self.nodes.len() <= max_nodes && self.text_bytes <= max_text {
            return (Vec::new(), Vec::new());
        }
        let (goal_nodes, goal_text) = (max_nodes / 5 * 4, max_text / 5 * 4);
        let (extra_nodes, extra_text) = if self.nodes.contains_key(TRIM_ID) {
            (0, 0)
        } else {
            (1, TRIM_TEXT.len())
        };
        let Some(main) = self.nodes.get("main") else {
            return (Vec::new(), Vec::new());
        };
        // The newest child is the streaming tail omp still appends to: it is never evicted.
        let oldest: Vec<String> = main
            .children
            .iter()
            .filter(|c| c.as_str() != TRIM_ID)
            .cloned()
            .collect();
        let oldest = &oldest[..oldest.len().saturating_sub(1)];
        let mut removed = Vec::new();
        for id in oldest.iter().cloned() {
            if self.nodes.len() + extra_nodes <= goal_nodes
                && self.text_bytes + extra_text <= goal_text
            {
                break;
            }
            if self.del(&id).is_ok() {
                removed.push(id);
            }
        }
        let mut ops: Vec<Value> = removed.iter().map(|id| json!(["del", id])).collect();
        if !removed.is_empty() && extra_nodes == 1 {
            let first = self
                .nodes
                .get("main")
                .and_then(|main| main.children.first())
                .cloned();
            let trim = json!(["add", TRIM_ID, "main", first,
                {"id": TRIM_ID, "k": "text", "p": {"text": TRIM_TEXT}}]);
            if self.run(&trim).is_ok() {
                ops.push(trim);
            }
        }
        (removed, ops)
    }
}

/// Bytes the document counts for a node: the JSON length of every string value in its props, at
/// any depth (text, spans, table cells, `kv` items, labels, diff hunks), escapes included: an
/// ESC-dense `ansi` text is 6 bytes per character in the snapshot.
fn weight(props: &Map<String, Value>) -> usize {
    props.values().map(value_weight).sum()
}

fn value_weight(value: &Value) -> usize {
    match value {
        Value::String(s) => str_weight(s),
        Value::Array(items) => items.iter().map(value_weight).sum(),
        Value::Object(map) => map.values().map(value_weight).sum(),
        _ => 0,
    }
}

/// The JSON length of a string's content (without the quotes).
fn str_weight(s: &str) -> usize {
    s.chars()
        .map(|c| match c {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            c if c < ' ' => 6,
            c => c.len_utf8(),
        })
        .sum()
}

fn text_kind(node: &Node) -> Result<(), String> {
    if TEXT_KINDS.contains(&node.kind.as_str()) {
        Ok(())
    } else {
        Err(format!("{} ({}) has no primary text", node.id, node.kind))
    }
}

/// Byte offset of UTF-16 index `at` in `s`; `None` past the end or inside a surrogate pair.
fn byte_at(s: &str, at: usize) -> Option<usize> {
    let mut units = 0;
    for (byte, ch) in s.char_indices() {
        if units == at {
            return Some(byte);
        }
        if units > at {
            return None;
        }
        units += ch.len_utf16();
    }
    (units == at).then_some(s.len())
}

fn str_at(args: &[Value], i: usize) -> Result<&str, String> {
    args.get(i)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("argument {i} must be a string"))
}

fn opt_at(args: &[Value], i: usize) -> Result<Option<&str>, String> {
    match args.get(i) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("argument {i} must be a string or null")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tsp::fixture;
    use mkrew_protocol::TspNode;
    use serde_json::json;

    fn roots() -> Vec<Value> {
        ["main", "dock", "layer"]
            .map(|id| json!(["add", id, "s:1", null, {"id": id, "k": "col"}]))
            .to_vec()
    }

    /// A surface `s:1` with the three roots omp's first frame adds.
    fn seeded() -> Document {
        let mut doc = Document::new("s:1");
        let applied = doc.apply(&roots());
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        doc
    }

    fn leaf(id: &str, parent: &str, kind: &str) -> Value {
        json!(["add", id, parent, null, {"id": id, "k": kind}])
    }

    fn ids(doc: &Document, parent: &str) -> Vec<String> {
        doc.children(parent).map(|n| n.id.clone()).collect()
    }

    /// Applies one op that must fail and returns its error; the document must not change size.
    fn fails(doc: &mut Document, op: Value) -> String {
        let (len, bytes) = (doc.len(), doc.text_bytes());
        let applied = doc.apply(std::slice::from_ref(&op));
        assert!(applied.ops.is_empty(), "{op} applied");
        assert_eq!(applied.errors.len(), 1, "{op}");
        assert_eq!(
            (doc.len(), doc.text_bytes()),
            (len, bytes),
            "{op} changed the doc"
        );
        applied.errors.into_iter().next().unwrap_or_default()
    }

    #[test]
    fn add_places_nodes_by_before_and_drops_null_props() {
        let mut doc = seeded();
        let applied = doc.apply(&[
            json!(["add", "a", "main", null, {"id": "a", "k": "col",
                "p": {"gap": "sm", "title": null},
                "c": [{"id": "a.t", "k": "text", "p": {"text": "hi"}}]}]),
            leaf("b", "main", "spacer"),
            json!(["add", "c", "main", "b", {"id": "c", "k": "spacer"}]),
        ]);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        assert_eq!(applied.ops.len(), 3);
        assert_eq!(ids(&doc, "main"), ["a", "c", "b"]);
        assert_eq!(ids(&doc, "a"), ["a.t"]);
        assert_eq!(doc.get("a.t").unwrap().parent.as_deref(), Some("a"));
        assert_eq!(doc.get("a").unwrap().props["gap"], "sm");
        assert!(!doc.get("a").unwrap().props.contains_key("title"));
        assert_eq!(doc.text("a.t"), Some("hi"));
        // root, main, dock, layer, a, a.t, b, c
        assert_eq!(doc.len(), 8);
    }

    #[test]
    fn a_failed_add_changes_nothing() {
        let mut doc = seeded();
        doc.apply(&[leaf("a", "main", "col")]);
        let cases = [
            (leaf("x", "nope", "col"), "unknown id nope"),
            (leaf("a", "main", "col"), "duplicate id a"),
            (
                json!(["add", "x", "main", null, {"id": "x", "k": "col",
                    "c": [{"id": "y", "k": "col"}, {"id": "y", "k": "col"}]}]),
                "duplicate id y",
            ),
            (
                json!(["add", "x", "main", "dock", {"id": "x", "k": "col"}]),
                "dock is not a child of main",
            ),
            (
                json!(["add", "x", "main", null, {"id": "z", "k": "col"}]),
                "does not match",
            ),
        ];
        for (op, needle) in cases {
            let error = fails(&mut doc, op);
            assert!(error.contains(needle), "{error} should contain {needle}");
        }
        assert!(doc.get("x").is_none() && doc.get("y").is_none());
    }

    #[test]
    fn a_kind_omp_added_later_enters_the_tree_and_later_ops_reach_it() {
        // omp 19 sends `button` and `menu` whatever the hello says; one unknown kind used to drop
        // the whole subtree, and every later op naming it failed.
        let mut doc = seeded();
        let applied = doc.apply(&[
            json!(["add", "fh", "main", null, {"id": "fh", "k": "card",
                "c": [{"id": "fh.pill", "k": "button", "p": {"text": "Yes"}}]}]),
            json!(["add", "next", "main", "fh", {"id": "next", "k": "text"}]),
            json!(["set", "fh.pill", {"on": true}]),
        ]);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        assert_eq!(doc.get("fh.pill").map(|n| n.kind.as_str()), Some("button"));
        assert_eq!(ids(&doc, "main"), ["next", "fh"]);
    }

    #[test]
    fn text_ops_reach_a_doc_node() {
        let mut doc = seeded();
        doc.apply(&[json!(["add", "d", "main", null,
            {"id": "d", "k": "doc", "p": {"text": "# a"}}])]);
        let applied = doc.apply(&[json!(["text", "d", "append", "b"])]);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        assert_eq!(doc.text("d"), Some("# ab"));
    }

    #[test]
    fn set_merges_props_and_null_deletes_one() {
        let mut doc = seeded();
        doc.apply(&[json!(["add", "a", "main", null,
            {"id": "a", "k": "col", "p": {"gap": "sm", "role": "r"}}])]);
        let applied =
            doc.apply(&[json!(["set", "a", {"gap": "lg", "role": null, "tone": "info"}])]);
        assert!(applied.errors.is_empty());
        let props = &doc.get("a").unwrap().props;
        assert_eq!(props["gap"], "lg");
        assert_eq!(props["tone"], "info");
        assert!(!props.contains_key("role"));
        assert!(fails(&mut doc, json!(["set", "nope", {}])).contains("unknown id nope"));
        assert!(fails(&mut doc, json!(["set", "a", [1]])).contains("must be an object"));
    }

    #[test]
    fn text_appends_replaces_and_only_works_on_text_kinds() {
        let mut doc = seeded();
        doc.apply(&[
            json!(["add", "t", "main", null, {"id": "t", "k": "md", "p": {"text": "ab"}}]),
            leaf("bare", "main", "text"),
            leaf("c", "main", "col"),
        ]);
        doc.apply(&[json!(["text", "t", "append", "cd"])]);
        assert_eq!(doc.text("t"), Some("abcd"));
        doc.apply(&[json!(["text", "t", "replace", "x"])]);
        assert_eq!(doc.text("t"), Some("x"));
        doc.apply(&[json!(["text", "bare", "append", "q"])]);
        assert_eq!(doc.text("bare"), Some("q"));
        assert!(
            fails(&mut doc, json!(["text", "c", "append", "q"]))
                .contains("c (col) has no primary text")
        );
        assert!(
            fails(&mut doc, json!(["text", "t", "prepend", "q"]))
                .contains("unknown text mode prepend")
        );
        assert!(fails(&mut doc, json!(["text", "t", "append", 5])).contains("must be a string"));
        // `text()` reads the primary text of text kinds only.
        doc.apply(&[
            json!(["add", "n", "main", null, {"id": "n", "k": "toast", "p": {"text": "hi"}}]),
        ]);
        assert_eq!(doc.text("n"), None);
    }

    #[test]
    fn splice_counts_utf16_units_like_javascript() {
        let mut doc = seeded();
        // "a😀b": a = 1 unit, the emoji = 2 units, b = 1 unit.
        doc.apply(&[
            json!(["add", "e", "dock", null, {"id": "e", "k": "editor", "p": {"text": "a😀b"}}]),
        ]);
        doc.apply(&[json!(["splice", "e", 1, 2, "X"])]);
        assert_eq!(doc.text("e"), Some("aXb"));
        doc.apply(&[json!(["splice", "e", 1, 0, "é😀"])]);
        assert_eq!(doc.text("e"), Some("aé😀Xb"));
        // Units now: a0 é1 😀2-3 X4 b5, length 6.
        assert!(fails(&mut doc, json!(["splice", "e", 3, 0, "?"])).contains("surrogate pair"));
        assert!(
            fails(&mut doc, json!(["splice", "e", 5, 2, ""])).contains("outside text of length 6")
        );
        assert!(fails(&mut doc, json!(["splice", "e", -1, 0, ""])).contains("splice needs"));
        doc.apply(&[json!(["splice", "e", 6, 0, "!"])]);
        assert_eq!(doc.text("e"), Some("aé😀Xb!"));
        doc.apply(&[leaf("n", "main", "toast")]);
        assert!(fails(&mut doc, json!(["splice", "n", 0, 0, "x"])).contains("has no primary text"));
    }

    #[test]
    fn a_failing_op_is_skipped_and_the_rest_of_the_frame_still_runs() {
        let mut doc = seeded();
        let ok1 = leaf("a", "main", "col");
        let ok2 = leaf("b", "main", "col");
        let applied = doc.apply(&[ok1.clone(), json!(["set", "nope", {}]), ok2.clone()]);
        assert_eq!(applied.ops, vec![ok1, ok2]);
        assert_eq!(applied.errors, vec!["op 1: unknown id nope".to_owned()]);
        assert_eq!(ids(&doc, "main"), ["a", "b"]);
    }

    #[test]
    fn malformed_ops_are_errors_not_panics() {
        let mut doc = seeded();
        assert!(
            fails(&mut doc, json!(["frobnicate", "a"])).contains("op 0: unknown op frobnicate")
        );
        assert!(fails(&mut doc, json!("add")).contains("op is not an array"));
        assert!(fails(&mut doc, json!([])).contains("op has no verb"));
        assert!(fails(&mut doc, json!(["add", "x"])).contains("argument"));
    }

    #[test]
    fn text_bytes_follows_text_and_span_text_through_every_op() {
        let mut doc = seeded();
        doc.apply(&[
            json!(["add", "t", "main", null, {"id": "t", "k": "text", "p": {"text": "hello"}}]),
        ]);
        assert_eq!(doc.text_bytes(), 5);
        doc.apply(&[json!(["text", "t", "append", " w"])]);
        assert_eq!(doc.text_bytes(), 7);
        doc.apply(&[json!(["splice", "t", 0, 1, "J"])]);
        assert_eq!(doc.text_bytes(), 7);
        doc.apply(&[json!(["add", "s", "main", null,
            {"id": "s", "k": "text", "p": {"spans": [{"t": "ab"}, {"t": "c", "s": "muted"}]}}])]);
        assert_eq!(doc.text_bytes(), 15); // 7 + "ab" "c" "muted"
        doc.apply(&[json!(["set", "t", {"text": null}])]);
        assert_eq!(doc.text_bytes(), 8);
        // Both the spans (8) and the new text (2) are stored on `s`, so both count.
        doc.apply(&[json!(["text", "s", "replace", "xy"])]);
        assert_eq!(doc.text_bytes(), 10);
    }

    #[test]
    fn text_bytes_counts_every_string_in_props_recursively() {
        let mut doc = seeded();
        doc.apply(&[json!(["add", "t", "main", null, {"id": "t", "k": "table",
            "p": {"cols": ["ab", "cd"], "rows": [["e", "fg"]], "n": 5, "x": {"label": "hi"}}}])]);
        assert_eq!(doc.text_bytes(), 9); // ab cd e fg hi
        doc.apply(&[json!(["set", "t", {"x": null}])]);
        assert_eq!(doc.text_bytes(), 7);
        doc.apply(&[json!(["set", "t", {"x": {"label": "hello"}}])]);
        assert_eq!(doc.text_bytes(), 12);
        doc.apply(&[json!(["add", "k", "main", null, {"id": "k", "k": "kv",
            "p": {"items": [{"k": "a", "v": "bcd"}]}}])]);
        assert_eq!(doc.text_bytes(), 16); // 12 + a bcd; the keys "k" and "v" are not counted
        doc.apply(&[json!(["del", "t"])]);
        assert_eq!(doc.text_bytes(), 4);
        doc.apply(&[json!(["del", "k"])]);
        assert_eq!(doc.text_bytes(), 0);
    }

    /// The bound is on what the snapshot will carry, so a string counts as its JSON escaped length.
    #[test]
    fn text_bytes_counts_the_json_escaped_length() {
        let mut doc = seeded();
        doc.apply(&[json!(["add", "t", "main", null, {"id": "t", "k": "text",
            "p": {"text": "a\u{1b}\n\"\\é"}}])]);
        // a 1, ESC 6, newline 2, quote 2, backslash 2, é 2 bytes
        assert_eq!(doc.text_bytes(), 15);
        assert_eq!(
            serde_json::to_string("a\u{1b}\n\"\\é").unwrap().len() - 2,
            15
        );
    }

    #[test]
    fn eviction_triggers_on_non_text_props() {
        let mut doc = seeded();
        let ops: Vec<Value> = (0..10)
            .map(|i| {
                let id = format!("r{i}");
                json!(["add", id, "main", null, {"id": id, "k": "kv",
                    "p": {"items": [{"k": "k", "v": "x".repeat(30)}]}}])
            })
            .collect();
        assert!(doc.apply(&ops).errors.is_empty());
        assert_eq!(doc.text_bytes(), 310);
        let (removed, _) = doc.evict_with(usize::MAX, 100);
        assert_eq!(removed.len(), 9);
        assert_eq!(doc.text_bytes(), 60); // 31 + the 29 byte trim row
    }

    /// A tail of one row that stays over the goal is not evicted: omp still appends to it.
    #[test]
    fn eviction_never_deletes_the_newest_main_child() {
        let mut doc = seeded();
        let ops = vec![
            json!(["add", "old", "main", null, {"id": "old", "k": "text", "p": {"text": "x"}}]),
            json!(["add", "tail", "main", null, {"id": "tail", "k": "text", "p": {"text": "y".repeat(500)}}]),
        ];
        assert!(doc.apply(&ops).errors.is_empty());
        let (removed, _) = doc.evict_with(usize::MAX, 100);
        assert_eq!(removed, ["old"]);
        assert_eq!(ids(&doc, "main"), ["mkrew.trimmed", "tail"]);
        assert!(
            doc.text_bytes() > 100,
            "still over: the engine's bounds check falls back"
        );
    }

    /// Appends add only the piece's weight; the counter always equals a full recount.
    #[test]
    fn text_bytes_after_appends_equals_a_full_recount() {
        let recount =
            |doc: &Document| -> usize { doc.nodes.values().map(|n| weight(&n.props)).sum() };
        let mut doc = seeded();
        doc.apply(&[
            json!(["add", "t", "main", null, {"id": "t", "k": "text", "p": {"text": "a\u{1b}"}}]),
            json!(["add", "b", "main", null, {"id": "b", "k": "md"}]),
            json!(["text", "t", "append", "\"q\"\n\u{1b}é"]),
            json!(["text", "b", "append", "new"]),
            json!(["text", "t", "replace", "r\t"]),
            json!(["text", "t", "append", "\\"]),
            json!(["splice", "t", 0, 1, "\u{1b}"]),
            json!(["set", "b", {"label": "l\"x"}]),
            json!(["text", "b", "append", "!"]),
        ]);
        assert_eq!(doc.text("t"), Some("\u{1b}\t\\"));
        assert_eq!(doc.text_bytes(), recount(&doc));
        doc.apply(&[json!(["del", "t"])]);
        assert_eq!(doc.text_bytes(), recount(&doc));
    }

    /// `n1..n63` nested under `main`: `n63` sits at depth 64 (root 0, `main` 1).
    fn deep() -> Document {
        let mut doc = seeded();
        let ops: Vec<Value> = (1..=63)
            .map(|i| {
                leaf(
                    &format!("n{i}"),
                    &if i == 1 {
                        "main".to_owned()
                    } else {
                        format!("n{}", i - 1)
                    },
                    "col",
                )
            })
            .collect();
        let applied = doc.apply(&ops);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        doc
    }

    #[test]
    fn depth_is_capped_for_add_subtree_add_and_move() {
        let mut doc = deep();
        assert!(fails(&mut doc, leaf("over", "n63", "col")).contains("too deep"));
        // A child of n62 is at depth 64: fine. Its own child would be 65: rejected.
        let two = json!(["add", "x", "n62", null, {"id": "x", "k": "col",
            "c": [{"id": "y", "k": "col"}]}]);
        assert!(fails(&mut doc, two).contains("too deep"));
        assert!(doc.get("x").is_none() && doc.get("y").is_none());
        assert!(doc.apply(&[leaf("fit", "n62", "col")]).errors.is_empty());
        doc.apply(&[json!(["add", "m", "dock", null, {"id": "m", "k": "col",
            "c": [{"id": "m2", "k": "col"}]}])]);
        assert!(fails(&mut doc, json!(["move", "m", "n62", null])).contains("too deep"));
        assert_eq!(doc.get("m").unwrap().parent.as_deref(), Some("dock"));
        assert!(
            doc.apply(&[json!(["move", "m", "n61", null])])
                .errors
                .is_empty()
        );
    }

    #[test]
    fn move_reorders_and_reparents_computing_the_index_after_removal() {
        let mut doc = seeded();
        doc.apply(&[
            leaf("a", "main", "spacer"),
            leaf("b", "main", "spacer"),
            leaf("c", "main", "spacer"),
            leaf("k", "dock", "col"),
        ]);
        doc.apply(&[json!(["move", "c", "main", "a"])]);
        assert_eq!(ids(&doc, "main"), ["c", "a", "b"]);
        doc.apply(&[json!(["move", "c", "main", null])]);
        assert_eq!(ids(&doc, "main"), ["a", "b", "c"]);
        // Same parent, before a later sibling: the slot is counted without the moved node.
        doc.apply(&[json!(["move", "a", "main", "c"])]);
        assert_eq!(ids(&doc, "main"), ["b", "a", "c"]);
        doc.apply(&[json!(["move", "b", "k", null])]);
        assert_eq!(ids(&doc, "main"), ["a", "c"]);
        assert_eq!(ids(&doc, "k"), ["b"]);
        assert_eq!(doc.get("b").unwrap().parent.as_deref(), Some("k"));
        let cases = [
            (json!(["move", "s:1", "main", null]), "cannot move the root"),
            (
                json!(["move", "dock", "k", null]),
                "cannot move dock into its own subtree",
            ),
            (json!(["move", "a", "main", "a"]), "move before itself"),
            (
                json!(["move", "a", "main", "dock"]),
                "dock is not a child of main",
            ),
            (json!(["move", "nope", "main", null]), "unknown id nope"),
            (json!(["move", "a", "nope", null]), "unknown id nope"),
        ];
        for (op, needle) in cases {
            let error = fails(&mut doc, op);
            assert!(error.contains(needle), "{error} should contain {needle}");
        }
        assert_eq!(ids(&doc, "main"), ["a", "c"]);
        assert_eq!(ids(&doc, "k"), ["b"]);
    }

    #[test]
    fn del_removes_the_subtree_and_clears_focus_inside_it() {
        let mut doc = seeded();
        doc.apply(&[json!(["add", "p", "main", null, {"id": "p", "k": "col",
            "c": [{"id": "q", "k": "editor", "p": {"text": "abc"}}, {"id": "r", "k": "col"}]}])]);
        doc.apply(&[json!(["focus", "q"])]);
        assert_eq!(doc.focus(), Some("q"));
        assert_eq!((doc.len(), doc.text_bytes()), (7, 3));
        doc.apply(&[json!(["del", "p"])]);
        assert_eq!((doc.len(), doc.text_bytes()), (4, 0));
        assert!(doc.get("q").is_none() && doc.get("r").is_none());
        assert!(ids(&doc, "main").is_empty());
        assert_eq!(doc.focus(), None);
        assert!(fails(&mut doc, json!(["del", "s:1"])).contains("cannot delete the root"));
        assert!(fails(&mut doc, json!(["del", "p"])).contains("unknown id p"));
    }

    #[test]
    fn focus_sets_clears_and_rejects_unknown_ids() {
        let mut doc = seeded();
        doc.apply(&[leaf("e", "dock", "editor"), json!(["focus", "e"])]);
        assert_eq!(doc.focus(), Some("e"));
        assert!(fails(&mut doc, json!(["focus", "nope"])).contains("unknown id nope"));
        assert_eq!(doc.focus(), Some("e"));
        doc.apply(&[json!(["focus", null])]);
        assert_eq!(doc.focus(), None);
    }

    #[test]
    fn settle_marks_a_node_and_any_change_unsettles_it_and_its_ancestors() {
        let mut doc = seeded();
        doc.apply(&[
            leaf("p", "main", "col"),
            leaf("q", "p", "text"),
            leaf("s", "p", "text"),
            json!(["settle", "q"]),
            json!(["settle", "s"]),
            json!(["settle", "p"]),
        ]);
        let settled = |doc: &Document, id: &str| doc.get(id).unwrap().settled;
        assert!(settled(&doc, "q") && settled(&doc, "s") && settled(&doc, "p"));
        doc.apply(&[json!(["set", "q", {"tone": "info"}])]);
        assert!(!settled(&doc, "q") && !settled(&doc, "p"));
        assert!(settled(&doc, "s"), "a sibling stays settled");
        doc.apply(&[json!(["settle", "p"]), leaf("t", "p", "text")]);
        assert!(
            !settled(&doc, "p"),
            "an add under a settled node unsettles it"
        );
        assert!(fails(&mut doc, json!(["settle", "nope"])).contains("unknown id nope"));
    }

    #[test]
    fn reveal_scroll_suspend_and_resume_apply_without_changing_the_document() {
        let mut doc = seeded();
        doc.apply(&[leaf("a", "main", "col")]);
        let before = doc.clone();
        let ops = [
            json!(["reveal", "a", "end"]),
            json!(["scroll", "a", "start"]),
            json!(["suspend"]),
            json!(["resume"]),
        ];
        let applied = doc.apply(&ops);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        assert_eq!(applied.ops, ops);
        assert_eq!(doc, before);
        assert!(fails(&mut doc, json!(["reveal", "nope", "end"])).contains("unknown id nope"));
    }

    #[test]
    fn to_tree_and_from_tree_round_trip_through_json_like_a_snapshot() {
        let mut doc = seeded();
        doc.apply(&[
            json!(["add", "a", "main", null, {"id": "a", "k": "col", "p": {"gap": "sm"},
                "c": [{"id": "a.t", "k": "text", "p": {"text": "héllo"}}, {"id": "a.u", "k": "spacer"}]}]),
            json!(["add", "e", "dock", null, {"id": "e", "k": "editor", "p": {"text": "x", "cursor": 1}}]),
            leaf("b", "main", "col"),
            json!(["move", "b", "main", "a"]),
        ]);
        let tree = doc.to_tree();
        assert_eq!(tree.id, "s:1");
        let top: Vec<&str> = tree.c.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(top, ["main", "dock", "layer"]);
        let main: Vec<&str> = tree.c[0].c.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(main, ["b", "a"], "child order survives");
        let back: TspNode = serde_json::from_value(serde_json::to_value(&tree).unwrap()).unwrap();
        let mut mirror = Document::from_tree(&back);
        assert_eq!(mirror, doc);
        assert_eq!(mirror.text_bytes(), doc.text_bytes());
        // The mirror keeps following frames.
        let applied = mirror.apply(&[
            json!(["text", "a.t", "append", "!"]),
            leaf("c", "layer", "col"),
        ]);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        assert_eq!(mirror.text("a.t"), Some("héllo!"));
        assert_eq!(mirror.get("c").unwrap().parent.as_deref(), Some("layer"));
    }

    /// `n` rows `<prefix>0..` under `main`, each a `col` with one text child (2 nodes per row).
    fn rows(prefix: &str, n: usize, text: &str) -> Vec<Value> {
        (0..n)
            .map(|i| {
                let id = format!("{prefix}{i}");
                json!(["add", id, "main", null, {"id": id, "k": "col",
                    "c": [{"id": format!("{id}.t"), "k": "text", "p": {"text": text}}]}])
            })
            .collect()
    }

    /// 10 rows plus one editor in `dock` and one node in `layer`: 26 nodes with the roots.
    fn with_rows(text: &str) -> Document {
        let mut doc = seeded();
        let mut ops = rows("r", 10, text);
        ops.push(
            json!(["add", "d1", "dock", null, {"id": "d1", "k": "editor", "p": {"text": "draft"}}]),
        );
        ops.push(leaf("l1", "layer", "col"));
        let applied = doc.apply(&ops);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        doc
    }

    fn trim_row() -> Value {
        json!({"id": "mkrew.trimmed", "k": "text", "p": {"text": "Earlier messages were trimmed"}})
    }

    #[test]
    fn eviction_removes_the_oldest_rows_and_keeps_dock_layer_and_the_newest() {
        let mut doc = with_rows("");
        assert_eq!(doc.len(), 26);
        let mut mirror = doc.clone();
        let (removed, ops) = doc.evict_with(25, usize::MAX);
        // 80% of 25 is 20 and the trim row will take one: down to 18 nodes, so 4 rows go.
        assert_eq!(removed, ["r0", "r1", "r2", "r3"]);
        assert_eq!(
            ops,
            vec![
                json!(["del", "r0"]),
                json!(["del", "r1"]),
                json!(["del", "r2"]),
                json!(["del", "r3"]),
                json!(["add", "mkrew.trimmed", "main", "r4", trim_row()]),
            ]
        );
        assert_eq!(
            ids(&doc, "main"),
            ["mkrew.trimmed", "r4", "r5", "r6", "r7", "r8", "r9"]
        );
        assert_eq!(doc.len(), 19);
        assert!(
            doc.get("r0.t").is_none(),
            "the subtree of an evicted row is gone"
        );
        assert!(doc.get("d1").is_some() && doc.get("l1").is_some());
        assert_eq!(
            doc.text("mkrew.trimmed"),
            Some("Earlier messages were trimmed")
        );
        // What clients are told to do gives them the same document.
        let applied = mirror.apply(&ops);
        assert!(applied.errors.is_empty(), "{:?}", applied.errors);
        assert_eq!(mirror, doc);
    }

    #[test]
    fn a_second_eviction_never_removes_or_re_adds_the_trim_row() {
        let mut doc = with_rows("");
        doc.evict_with(25, usize::MAX);
        let applied = doc.apply(&rows("x", 8, ""));
        assert!(applied.errors.is_empty());
        assert_eq!(doc.len(), 35);
        let (removed, ops) = doc.evict_with(25, usize::MAX);
        // 35 nodes down to 20 (no trim row to add): r4..r9 and x0, x1 go.
        assert_eq!(removed, ["r4", "r5", "r6", "r7", "r8", "r9", "x0", "x1"]);
        assert!(ops.iter().all(|op| op[0] == "del"), "{ops:?}");
        assert_eq!(ids(&doc, "main")[0], TRIM_ID);
        assert_eq!(doc.len(), 19);
    }

    #[test]
    fn eviction_by_text_counts_the_trim_row_and_keeps_the_accounting_exact() {
        let mut doc = with_rows(&"x".repeat(30));
        assert_eq!(doc.text_bytes(), 305); // 10 * 30 + "draft"
        let (removed, _) = doc.evict_with(usize::MAX, 100);
        // Goal 80 bytes; the trim row is 29: rows go until the rest is at most 51, so r9 stays.
        assert_eq!(removed.len(), 9);
        assert_eq!(ids(&doc, "main"), ["mkrew.trimmed", "r9"]);
        assert_eq!(doc.text_bytes(), 64); // 30 + "draft" + 29
    }

    #[test]
    fn eviction_does_nothing_at_the_limit_or_when_main_has_nothing_to_give() {
        let mut doc = with_rows("");
        assert_eq!(
            doc.evict_with(doc.len(), usize::MAX),
            (Vec::new(), Vec::new())
        );
        let mut dock_only = seeded();
        dock_only.apply(&[json!(["add", "big", "dock", null,
            {"id": "big", "k": "editor", "p": {"text": "x".repeat(200)}}])]);
        assert_eq!(
            dock_only.evict_with(usize::MAX, 100),
            (Vec::new(), Vec::new())
        );
        assert!(dock_only.get(TRIM_ID).is_none());
        assert!(dock_only.get("big").is_some());
    }

    #[test]
    fn evict_uses_the_limits_set_on_the_document() {
        let mut doc = with_rows("");
        assert_eq!(doc.evict(), (Vec::new(), Vec::new()));
        doc.set_limits(25, usize::MAX);
        let (removed, _) = doc.evict();
        assert_eq!(removed, ["r0", "r1", "r2", "r3"]);
    }

    #[test]
    fn the_startup_fixture_applies_without_errors() {
        let mut doc = Document::new("s:1");
        for (verb, body) in fixture::out_lines() {
            if verb != b'f' {
                continue;
            }
            let ops = body["ops"].as_array().unwrap();
            let applied = doc.apply(ops);
            assert!(
                applied.errors.is_empty(),
                "frame {}: {:?}",
                body["s"],
                applied.errors
            );
            assert_eq!(applied.ops.len(), ops.len());
        }
        for id in ["main", "dock", "layer"] {
            let node = doc.get(id).unwrap_or_else(|| panic!("{id} is missing"));
            assert_eq!(node.parent.as_deref(), Some("s:1"), "{id}");
        }
        let editor = doc.get("3.line/input").unwrap();
        assert_eq!(editor.kind, "editor");
        assert_eq!(editor.props["placeholder"], "What are we cooking?");
        assert_eq!(doc.focus(), Some("3.line/input"));
        // Set ops merged over the add: the context meter ends with the last frame's props.
        let meter = doc.get("3.context").unwrap();
        assert_eq!(meter.kind, "meter");
        assert_eq!(meter.props["label"], "2%");
        assert_eq!(meter.props["value"], 0.015528);
        // `u` was added, deleted and re-added as a hidden col (frame 9).
        let toast = doc.get("u").unwrap();
        assert_eq!(toast.kind, "col");
        assert_eq!(toast.props["hidden"], true);
        // One `move` in the startup frames: node 3 now lives under `h`.
        assert_eq!(doc.get("3").unwrap().parent.as_deref(), Some("h"));
        assert_eq!(doc.len(), 55);
        // And the same tree comes back through a snapshot mirror.
        assert_eq!(Document::from_tree(&doc.to_tree()).len(), 55);
    }
}
