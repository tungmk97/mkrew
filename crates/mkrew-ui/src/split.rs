//! Split tree of a terminal tab (SPEC §5 `terminal_tab.layout`, §10): leaves are sessions, files
//! or the diff, inner nodes split their area in two. Pure; serializes to exactly the §5 JSON
//! shape.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// How a split divides its area (SPEC §5 example: `vertical` = side by side).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    /// Children side by side: first left, second right.
    Vertical,
    /// Children stacked: first on top, second below.
    Horizontal,
}

/// Direction for moving focus to a neighbouring pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// One open file: its path (relative to the root with `/` separators, or absolute for a note in
/// a notes folder outside it) and the folder it is of: `None` = the project folder, `Some` = the
/// project's folder inside that task's worktree. The same path in two folders is two files.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileKey {
    pub path: String,
    pub task: Option<Ulid>,
}

impl FileKey {
    pub fn new(path: impl Into<String>, task: Option<Ulid>) -> FileKey {
        FileKey {
            path: path.into(),
            task,
        }
    }

    /// A file of the project folder.
    pub fn project(path: impl Into<String>) -> FileKey {
        FileKey::new(path, None)
    }
}

/// What a pane of the tab shows, as the key that finds it: one session, one file (its path and
/// folder), or the diff. A tab holds a key at most once, and so does a workspace for files and
/// the diff (ADR 0108).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PaneKey {
    Session(Ulid),
    File(FileKey),
    Diff,
    /// A browser pane (ADR 0111), by its leaf's own id: two panes may show one URL.
    Web(Ulid),
}

impl PaneKey {
    /// The session of a session pane.
    pub fn session(&self) -> Option<Ulid> {
        match self {
            PaneKey::Session(id) => Some(*id),
            PaneKey::File(_) | PaneKey::Diff | PaneKey::Web(_) => None,
        }
    }
}

/// One pane of the split tree and what is saved on it. The JSON is untagged: the keys decide
/// the kind, so every layout of an older version (sessions only) still reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Leaf {
    /// A pane showing one session. `chat` is the pane's saved mode (ADR 0104): `Some(true)`
    /// Chat, `Some(false)` Terminal, absent = not set (`[ui] chat_default` decides). `tsp` is
    /// the omp view the daemon saved for it (ADR 0121): `Some(false)` omp's own terminal UI,
    /// absent = the profile's.
    Session {
        session: Ulid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chat: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tsp: Option<bool>,
    },
    /// A file of the workspace: its path relative to the root with `/` separators (an
    /// absolute path for a note in a notes folder outside it). `task` is the task worktree it
    /// is of, absent = the project folder (layouts of protocol 58/60 have none). `md` is the
    /// pane's own Markdown mode, absent = `[ui] md_mode`. `preview` marks the workspace's
    /// preview tab.
    File {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<Ulid>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        md: Option<mkrew_core::settings::MdMode>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        preview: bool,
    },
    /// The workspace's diff. `diff` is always `true` on the wire.
    Diff { diff: bool },
    /// A browser pane (ADR 0111): `web` is the page it shows (`""` before the first load),
    /// `id` names the pane.
    Web { web: String, id: Ulid },
}

impl Leaf {
    pub fn session(id: Ulid) -> Leaf {
        Leaf::Session {
            session: id,
            chat: None,
            tsp: None,
        }
    }

    /// A file of the project folder.
    pub fn file(path: impl Into<String>) -> Leaf {
        Leaf::of_file(FileKey::project(path))
    }

    pub fn of_file(file: FileKey) -> Leaf {
        Leaf::File {
            file: file.path,
            task: file.task,
            md: None,
            preview: false,
        }
    }

    pub fn diff() -> Leaf {
        Leaf::Diff { diff: true }
    }

    /// A new browser pane on `url` (`""`: the address field waits for one).
    pub fn web(url: impl Into<String>) -> Leaf {
        Leaf::Web {
            web: url.into(),
            id: Ulid::new(),
        }
    }

    pub fn key(&self) -> PaneKey {
        match self {
            Leaf::Session { session, .. } => PaneKey::Session(*session),
            Leaf::File { file, task, .. } => PaneKey::File(FileKey::new(file.clone(), *task)),
            Leaf::Diff { .. } => PaneKey::Diff,
            Leaf::Web { id, .. } => PaneKey::Web(*id),
        }
    }

    /// Whether this leaf is the one `key` names (no allocation, unlike `key()`).
    pub fn has_key(&self, key: &PaneKey) -> bool {
        match (self, key) {
            (Leaf::Session { session, .. }, PaneKey::Session(id)) => session == id,
            (Leaf::File { file, task, .. }, PaneKey::File(key)) => {
                *file == key.path && *task == key.task
            }
            (Leaf::Diff { .. }, PaneKey::Diff) => true,
            (Leaf::Web { id, .. }, PaneKey::Web(key)) => id == key,
            _ => false,
        }
    }

    /// A stored leaf, or `None` (with a warning) when it is of a shape this version does not
    /// know or is broken by hand. `{"diff": false}` is never written, so it is broken too.
    fn from_json(value: &serde_json::Value) -> Option<Leaf> {
        match Leaf::deserialize(value) {
            Ok(Leaf::Diff { diff: false }) => {
                tracing::warn!(%value, "dropping a layout leaf: `diff` is not true");
                None
            }
            Ok(leaf) => Some(leaf),
            Err(error) => {
                tracing::warn!(%value, "dropping a layout leaf that does not parse: {error}");
                None
            }
        }
    }
}

/// The side of a pane a dropped leaf goes to: it takes that half of the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    /// The edge nearest to the pointer at (`x`, `y`), fractions of the pane (0..1): the
    /// smallest of `x`, `1 - x`, `y`, `1 - y`. A tie goes Right, then Bottom, then Left, then
    /// Top, so the middle of a pane splits it to the right like the old drop did.
    pub fn nearest(x: f32, y: f32) -> Edge {
        [
            (1.0 - x, Edge::Right),
            (1.0 - y, Edge::Bottom),
            (x, Edge::Left),
            (y, Edge::Top),
        ]
        .into_iter()
        // `min_by` keeps the first of equal elements, which is the tie order above.
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map_or(Edge::Right, |(_, edge)| edge)
    }

    /// The split that holds the old pane and the new one.
    pub fn axis(self) -> Axis {
        match self {
            Edge::Left | Edge::Right => Axis::Vertical,
            Edge::Top | Edge::Bottom => Axis::Horizontal,
        }
    }

    /// The new leaf is the first child of the split (left of / above the old pane).
    pub fn leads(self) -> bool {
        matches!(self, Edge::Left | Edge::Top)
    }
}

/// A node of the split tree.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Node {
    Leaf(Leaf),
    /// `ratio` is the share of the first child.
    Split {
        split: Axis,
        ratio: f64,
        children: Box<[Node; 2]>,
    },
}

/// The wire shape of a split, read from a `serde_json::Value` so one bad child does not lose
/// the whole layout ([`Node::from_json`]).
#[derive(Deserialize)]
struct SplitShape {
    split: Axis,
    ratio: f64,
    children: [serde_json::Value; 2],
}

/// A pane's area as fractions of the whole tab (0..1 on both axes).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// A split's divider, for dragging it: `index` is the split's place in tree order
/// ([`Node::set_ratio`]), `area` the split's whole area (fractions of the tab).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Divider {
    pub index: usize,
    pub axis: Axis,
    pub area: Rect,
    pub ratio: f64,
}

impl Divider {
    /// The ratio that puts this divider at point (`x`, `y`), fractions of the tab.
    pub fn ratio_at(&self, x: f64, y: f64) -> f64 {
        match self.axis {
            Axis::Vertical => (x - self.area.x) / self.area.w,
            Axis::Horizontal => (y - self.area.y) / self.area.h,
        }
    }
}

/// A ready-made layout for a whole tab (SPEC §10, the split button's menu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    /// Two panes side by side.
    Columns2,
    /// Two panes stacked.
    Rows2,
    /// Three equal panes side by side.
    Columns3,
    /// One pane left, two stacked right.
    MainAndStack,
    /// Two by two.
    Grid,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::Columns2,
        Preset::Rows2,
        Preset::Columns3,
        Preset::MainAndStack,
        Preset::Grid,
    ];

    /// Shown in the menu and the command palette.
    pub fn label(self) -> &'static str {
        match self {
            Preset::Columns2 => "2 columns",
            Preset::Rows2 => "2 rows",
            Preset::Columns3 => "3 columns",
            Preset::MainAndStack => "1 + 2",
            Preset::Grid => "2 × 2",
        }
    }

    /// Its name in `keymap.toml` after `layout_`.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Columns2 => "columns_2",
            Preset::Rows2 => "rows_2",
            Preset::Columns3 => "columns_3",
            Preset::MainAndStack => "main_stack",
            Preset::Grid => "grid",
        }
    }

    pub fn panes(self) -> usize {
        match self {
            Preset::Columns2 | Preset::Rows2 => 2,
            Preset::Columns3 | Preset::MainAndStack => 3,
            Preset::Grid => 4,
        }
    }

    /// The tree with `leaves` as its panes in tree order ([`Node::leaves`]: left to right,
    /// top to bottom within a column); `None` unless there are exactly [`Self::panes`] of them.
    pub fn build(self, leaves: &[Leaf]) -> Option<Node> {
        if leaves.len() != self.panes() {
            return None;
        }
        let pane = |at: usize| Node::leaf(leaves[at].clone());
        let split = |split: Axis, ratio: f64, first: Node, second: Node| Node::Split {
            split,
            ratio,
            children: Box::new([first, second]),
        };
        let (side, stack) = (Axis::Vertical, Axis::Horizontal);
        Some(match self {
            Preset::Columns2 => split(side, 0.5, pane(0), pane(1)),
            Preset::Rows2 => split(stack, 0.5, pane(0), pane(1)),
            Preset::Columns3 => split(side, 1.0 / 3.0, pane(0), split(side, 0.5, pane(1), pane(2))),
            Preset::MainAndStack => split(side, 0.5, pane(0), split(stack, 0.5, pane(1), pane(2))),
            Preset::Grid => split(
                side,
                0.5,
                split(stack, 0.5, pane(0), pane(1)),
                split(stack, 0.5, pane(2), pane(3)),
            ),
        })
    }

    /// Each pane's area (fractions of the tab), for drawing the layout in the menu.
    pub fn rects(self) -> Vec<Rect> {
        let leaves: Vec<Leaf> = (0..self.panes() as u128)
            .map(|n| Leaf::session(Ulid(n)))
            .collect();
        self.build(&leaves)
            .map(|node| node.rects().into_iter().map(|(_, rect)| rect).collect())
            .unwrap_or_default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("invalid layout JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("split ratio {0} is outside (0, 1)")]
    Ratio(f64),
    #[error("pane {0:?} appears more than once")]
    Duplicate(PaneKey),
}

/// Result of [`Node::close`].
#[derive(Debug, Clone, PartialEq)]
pub struct Closed {
    /// What is left of the tree; `None` when the closed pane was the last one.
    pub remaining: Option<Node>,
    /// The pane that took the closed pane's place, to focus next.
    pub focus: Option<PaneKey>,
}

/// Tolerance for comparing summed fractions.
const EPSILON: f64 = 1e-9;

/// How far one resize shortcut moves a divider (share of the split's area).
pub const RESIZE_STEP: f64 = 0.05;

/// Resizing never shrinks a split's child below this share, so no pane disappears.
pub const MIN_RATIO: f64 = 0.1;

impl Node {
    pub fn pane(session: Ulid) -> Self {
        Node::Leaf(Leaf::session(session))
    }

    pub fn leaf(leaf: Leaf) -> Self {
        Node::Leaf(leaf)
    }

    /// Parses a stored layout; JSON `null` is a tab without panes.
    /// A leaf that does not parse (an unknown shape, a broken value) is dropped with a warning
    /// and a split with one dropped child becomes the other child, so one bad leaf never
    /// loses the rest of the tab (ADR 0108); a layout with no good leaf is an empty tab.
    /// `Err` only for a split that is not a split (not two children, bad axis or ratio type),
    /// a ratio outside (0, 1), and a key that appears twice.
    pub fn from_json(value: &serde_json::Value) -> Result<Option<Node>, LayoutError> {
        if value.is_null() {
            return Ok(None);
        }
        let Some(node) = Node::parse(value)? else {
            return Ok(None);
        };
        let mut seen = HashSet::new();
        for key in node.keys() {
            if seen.contains(&key) {
                return Err(LayoutError::Duplicate(key));
            }
            seen.insert(key);
        }
        Ok(Some(node))
    }

    fn parse(value: &serde_json::Value) -> Result<Option<Node>, LayoutError> {
        if value.get("split").is_none() && value.get("children").is_none() {
            return Ok(Leaf::from_json(value).map(Node::Leaf));
        }
        let shape = SplitShape::deserialize(value)?;
        if !(shape.ratio > 0.0 && shape.ratio < 1.0) {
            return Err(LayoutError::Ratio(shape.ratio));
        }
        let [first, second] = &shape.children;
        Ok(match (Node::parse(first)?, Node::parse(second)?) {
            (Some(first), Some(second)) => Some(Node::Split {
                split: shape.split,
                ratio: shape.ratio,
                children: Box::new([first, second]),
            }),
            (one, other) => one.or(other),
        })
    }

    /// The stored form of `layout` (`null` without panes).
    pub fn to_json(layout: Option<&Node>) -> serde_json::Value {
        // Serializing this enum cannot fail: no maps with non-string keys, ratios are finite.
        serde_json::to_value(layout).unwrap_or(serde_json::Value::Null)
    }

    /// Calls `visit` on every leaf in tree order.
    fn walk<'a>(&'a self, visit: &mut impl FnMut(&'a Leaf)) {
        match self {
            Node::Leaf(leaf) => visit(leaf),
            Node::Split { children, .. } => children.iter().for_each(|child| child.walk(visit)),
        }
    }

    /// Keys of all panes, left-to-right / top-to-bottom in tree order.
    pub fn keys(&self) -> Vec<PaneKey> {
        let mut out = Vec::new();
        self.walk(&mut |leaf| out.push(leaf.key()));
        out
    }

    /// Every leaf in tree order, whole: what a preset rebuilds the tree from.
    pub fn leaves(&self) -> Vec<Leaf> {
        let mut out = Vec::new();
        self.walk(&mut |leaf| out.push(leaf.clone()));
        out
    }

    /// Session ids of the session panes, in tree order.
    pub fn sessions(&self) -> Vec<Ulid> {
        let mut out = Vec::new();
        self.walk(&mut |leaf| {
            if let Leaf::Session { session, .. } = leaf {
                out.push(*session);
            }
        });
        out
    }

    /// Whether any leaf satisfies `test`.
    pub fn any_leaf(&self, test: impl Fn(&Leaf) -> bool) -> bool {
        let mut found = false;
        self.walk(&mut |leaf| found |= test(leaf));
        found
    }

    pub fn contains(&self, key: &PaneKey) -> bool {
        self.leaf_of(key).is_some()
    }

    pub fn leaf_of(&self, key: &PaneKey) -> Option<&Leaf> {
        match self {
            Node::Leaf(leaf) => leaf.has_key(key).then_some(leaf),
            Node::Split { children, .. } => children.iter().find_map(|child| child.leaf_of(key)),
        }
    }

    pub fn leaf_mut(&mut self, key: &PaneKey) -> Option<&mut Leaf> {
        match self {
            Node::Leaf(leaf) => leaf.has_key(key).then_some(leaf),
            Node::Split { children, .. } => {
                children.iter_mut().find_map(|child| child.leaf_mut(key))
            }
        }
    }

    /// The mode saved on the pane of `session`; `None` = not set (or no such pane).
    pub fn chat_of(&self, session: Ulid) -> Option<bool> {
        match self.leaf_of(&PaneKey::Session(session))? {
            Leaf::Session { chat, .. } => *chat,
            Leaf::File { .. } | Leaf::Diff { .. } | Leaf::Web { .. } => None,
        }
    }

    /// The omp view saved on the pane of `session` (see [`Leaf::Session`]).
    pub fn tsp_of(&self, session: Ulid) -> Option<bool> {
        match self.leaf_of(&PaneKey::Session(session))? {
            Leaf::Session { tsp, .. } => *tsp,
            Leaf::File { .. } | Leaf::Diff { .. } | Leaf::Web { .. } => None,
        }
    }

    /// Saves `value` on the pane of `session`; false if it is not in the tree or already has it.
    pub fn set_chat(&mut self, session: Ulid, value: Option<bool>) -> bool {
        match self.leaf_mut(&PaneKey::Session(session)) {
            Some(Leaf::Session { chat, .. }) => {
                let changed = *chat != value;
                *chat = value;
                changed
            }
            _ => false,
        }
    }

    fn first(&self) -> PaneKey {
        match self {
            Node::Leaf(leaf) => leaf.key(),
            Node::Split { children, .. } => children[0].first(),
        }
    }

    fn last(&self) -> PaneKey {
        match self {
            Node::Leaf(leaf) => leaf.key(),
            Node::Split { children, .. } => children[1].last(),
        }
    }

    /// Splits the pane of `target` in half along `axis`; `new` (one pane or a whole layout)
    /// goes right of / below it. Returns false (and changes nothing) if `target` is not in the
    /// tree or a pane of `new` already is.
    pub fn split_with(&mut self, target: &PaneKey, axis: Axis, new: Node) -> bool {
        self.place(target, axis, false, new)
    }

    /// Splits the pane of `target` 50 / 50 and puts `new` on `edge`'s side of it: left or
    /// above (the first child) for `Left` and `Top`, right or below for `Right` and `Bottom`.
    /// Returns false (and changes nothing) if `target` is not in the tree or a pane of `new`
    /// already is.
    pub fn insert_at(&mut self, target: &PaneKey, edge: Edge, new: Node) -> bool {
        self.place(target, edge.axis(), edge.leads(), new)
    }

    fn place(&mut self, target: &PaneKey, axis: Axis, lead: bool, new: Node) -> bool {
        if !self.contains(target) || new.keys().iter().any(|key| self.contains(key)) {
            return false;
        }
        self.place_inner(target, axis, lead, &mut Some(new))
    }

    /// `new` is taken by the pane of `target`.
    fn place_inner(
        &mut self,
        target: &PaneKey,
        axis: Axis,
        lead: bool,
        new: &mut Option<Node>,
    ) -> bool {
        match self {
            Node::Leaf(leaf) if leaf.has_key(target) => {
                let Some(new) = new.take() else {
                    return false;
                };
                // The split keeps the leaf as it is: its own mode, Markdown mode and flags.
                let old = Node::Leaf(leaf.clone());
                let children = if lead { [new, old] } else { [old, new] };
                *self = Node::Split {
                    split: axis,
                    ratio: 0.5,
                    children: Box::new(children),
                };
                true
            }
            Node::Leaf(_) => false,
            Node::Split { children, .. } => children
                .iter_mut()
                .any(|child| child.place_inner(target, axis, lead, new)),
        }
    }

    /// Removes the pane of `target`; its sibling takes the parent split's place. Closing a
    /// pane that is not in the tree leaves it unchanged.
    pub fn close(self, target: &PaneKey) -> Closed {
        if !self.contains(target) {
            return Closed {
                remaining: Some(self),
                focus: None,
            };
        }
        match self {
            Node::Leaf(_) => Closed {
                remaining: None,
                focus: None,
            },
            Node::Split {
                split,
                ratio,
                children,
            } => {
                let [first, second] = *children;
                let in_first = first.contains(target);
                let (closed, keep) = if in_first {
                    (first.close(target), second)
                } else {
                    (second.close(target), first)
                };
                match closed.remaining {
                    // The sibling replaces the split; focus its pane nearest to the closed one.
                    None => Closed {
                        focus: Some(if in_first { keep.first() } else { keep.last() }),
                        remaining: Some(keep),
                    },
                    Some(rest) => Closed {
                        remaining: Some(Node::Split {
                            split,
                            ratio,
                            children: Box::new(if in_first { [rest, keep] } else { [keep, rest] }),
                        }),
                        focus: closed.focus,
                    },
                }
            }
        }
    }

    /// Moves the divider of the innermost split around `target` whose axis matches
    /// `direction` (left/right: side-by-side splits, up/down: stacked) by `step` that way,
    /// keeping both children at least [`MIN_RATIO`]. Returns false if nothing changed.
    pub fn resize(&mut self, target: &PaneKey, direction: Direction, step: f64) -> bool {
        let (axis, delta) = match direction {
            Direction::Left => (Axis::Vertical, -step),
            Direction::Right => (Axis::Vertical, step),
            Direction::Up => (Axis::Horizontal, -step),
            Direction::Down => (Axis::Horizontal, step),
        };
        self.resize_inner(target, axis, delta) == Some(true)
    }

    /// `None` = no matching split around `target` in this subtree (an outer one may match).
    fn resize_inner(&mut self, target: &PaneKey, axis: Axis, delta: f64) -> Option<bool> {
        let Node::Split {
            split,
            ratio,
            children,
        } = self
        else {
            return None;
        };
        let child = children.iter_mut().find(|c| c.contains(target))?;
        if let Some(done) = child.resize_inner(target, axis, delta) {
            return Some(done);
        }
        if *split != axis {
            return None;
        }
        // Rounded so repeated steps do not drift (0.44999…) and the stored JSON stays short.
        let next = ((*ratio + delta) * 1e4).round() / 1e4;
        let next = next.clamp(MIN_RATIO, 1.0 - MIN_RATIO);
        let changed = (next - *ratio).abs() > EPSILON;
        *ratio = next;
        Some(changed)
    }

    /// Gives every pane of a run of same-axis splits the same share (`A | (B | C)` becomes
    /// thirds); a nested split of the other axis counts as one pane. Returns false if no
    /// ratio changed.
    pub fn equalize(&mut self) -> bool {
        let Node::Split {
            split,
            ratio,
            children,
        } = self
        else {
            return false;
        };
        let [first, second] = &mut **children;
        let (a, b) = (first.slots(*split) as f64, second.slots(*split) as f64);
        let next = a / (a + b);
        let changed = (next - *ratio).abs() > EPSILON;
        *ratio = next;
        // `|`, not `||`: both children are equalized.
        changed | first.equalize() | second.equalize()
    }

    /// Panes side by side along `axis` within this node.
    fn slots(&self, axis: Axis) -> usize {
        match self {
            Node::Split {
                split, children, ..
            } if *split == axis => children.iter().map(|c| c.slots(axis)).sum(),
            _ => 1,
        }
    }

    /// Sets the ratio of the `index`-th split in tree order (see [`Self::dividers`]), kept
    /// within [`MIN_RATIO`]. Returns false if there is no such split or nothing changed.
    pub fn set_ratio(&mut self, index: usize, ratio: f64) -> bool {
        // A tab with no size yet gives NaN; a NaN ratio would break the stored layout.
        if !ratio.is_finite() {
            return false;
        }
        let next = ((ratio * 1e4).round() / 1e4).clamp(MIN_RATIO, 1.0 - MIN_RATIO);
        self.set_ratio_inner(&mut { index }, next) == Some(true)
    }

    /// `left` counts down the splits still to skip; `None` = not in this subtree.
    fn set_ratio_inner(&mut self, left: &mut usize, next: f64) -> Option<bool> {
        let Node::Split {
            ratio, children, ..
        } = self
        else {
            return None;
        };
        if *left == 0 {
            let changed = (next - *ratio).abs() > EPSILON;
            *ratio = next;
            return Some(changed);
        }
        *left -= 1;
        children
            .iter_mut()
            .find_map(|child| child.set_ratio_inner(left, next))
    }

    /// Every split with its area, in tree order.
    pub fn dividers(&self) -> Vec<Divider> {
        let mut out = Vec::new();
        self.dividers_inner(Rect::WHOLE, &mut out);
        out
    }

    fn dividers_inner(&self, area: Rect, out: &mut Vec<Divider>) {
        if let Node::Split {
            split,
            ratio,
            children,
        } = self
        {
            out.push(Divider {
                index: out.len(),
                axis: *split,
                area,
                ratio: *ratio,
            });
            let (a, b) = halves(*split, *ratio, area);
            children[0].dividers_inner(a, out);
            children[1].dividers_inner(b, out);
        }
    }

    /// Every pane with its area, in tree order.
    pub fn rects(&self) -> Vec<(PaneKey, Rect)> {
        let mut out = Vec::new();
        self.rects_inner(Rect::WHOLE, &mut out);
        out
    }

    fn rects_inner(&self, area: Rect, out: &mut Vec<(PaneKey, Rect)>) {
        match self {
            Node::Leaf(leaf) => out.push((leaf.key(), area)),
            Node::Split {
                split,
                ratio,
                children,
            } => {
                let (a, b) = halves(*split, *ratio, area);
                children[0].rects_inner(a, out);
                children[1].rects_inner(b, out);
            }
        }
    }

    /// The pane next to `from` in `direction`: the closest pane on that side whose span
    /// overlaps `from`'s, preferring the largest overlap, then the top / left one.
    pub fn neighbour(&self, from: &PaneKey, direction: Direction) -> Option<PaneKey> {
        let rects = self.rects();
        let (_, origin) = rects.iter().find(|(key, _)| key == from)?;
        let origin = *origin;
        rects
            .iter()
            .filter(|(key, _)| key != from)
            .filter_map(|(key, rect)| {
                // (gap to the candidate, overlap on the other axis, position on the other axis)
                let (gap, overlap, along) = match direction {
                    Direction::Right => (
                        rect.x - (origin.x + origin.w),
                        overlap(origin.y, origin.h, rect.y, rect.h),
                        rect.y,
                    ),
                    Direction::Left => (
                        origin.x - (rect.x + rect.w),
                        overlap(origin.y, origin.h, rect.y, rect.h),
                        rect.y,
                    ),
                    Direction::Down => (
                        rect.y - (origin.y + origin.h),
                        overlap(origin.x, origin.w, rect.x, rect.w),
                        rect.x,
                    ),
                    Direction::Up => (
                        origin.y - (rect.y + rect.h),
                        overlap(origin.x, origin.w, rect.x, rect.w),
                        rect.x,
                    ),
                };
                (gap > -EPSILON && overlap > EPSILON).then_some((key, gap, overlap, along))
            })
            .min_by(|a, b| {
                let gap = if (a.1 - b.1).abs() <= EPSILON {
                    std::cmp::Ordering::Equal
                } else {
                    a.1.total_cmp(&b.1)
                };
                gap.then(b.2.total_cmp(&a.2)).then(a.3.total_cmp(&b.3))
            })
            .map(|(key, ..)| key.clone())
    }
}

impl Rect {
    /// The whole tab.
    pub const WHOLE: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };
}

/// The two children's areas of a split of `area`.
fn halves(axis: Axis, ratio: f64, area: Rect) -> (Rect, Rect) {
    match axis {
        Axis::Vertical => {
            let w = area.w * ratio;
            (
                Rect { w, ..area },
                Rect {
                    x: area.x + w,
                    w: area.w - w,
                    ..area
                },
            )
        }
        Axis::Horizontal => {
            let h = area.h * ratio;
            (
                Rect { h, ..area },
                Rect {
                    y: area.y + h,
                    h: area.h - h,
                    ..area
                },
            )
        }
    }
}

/// Length shared by the spans `[a, a + a_len)` and `[b, b + b_len)`.
fn overlap(a: f64, a_len: f64, b: f64, b_len: f64) -> f64 {
    ((a + a_len).min(b + b_len) - a.max(b)).max(0.0)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn ids<const N: usize>() -> [Ulid; N] {
        std::array::from_fn(|_| Ulid::new())
    }

    fn k(id: Ulid) -> PaneKey {
        PaneKey::Session(id)
    }

    fn leaves(ids: &[Ulid]) -> Vec<Leaf> {
        ids.iter().map(|id| Leaf::session(*id)).collect()
    }

    /// `Node::split` took a session; the tests keep that shape.
    fn split(node: &mut Node, target: Ulid, axis: Axis, new: Ulid) -> bool {
        node.split_with(&k(target), axis, Node::pane(new))
    }

    fn pane_json(id: Ulid) -> serde_json::Value {
        json!({"session": id.to_string()})
    }

    fn halves_json(
        axis: &str,
        first: serde_json::Value,
        second: serde_json::Value,
    ) -> serde_json::Value {
        json!({"split": axis, "ratio": 0.5, "children": [first, second]})
    }

    #[test]
    fn a_whole_layout_splits_in_beside_a_pane() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        let moved = Node::Split {
            split: Axis::Horizontal,
            ratio: 0.3,
            children: Box::new([Node::pane(b), Node::pane(c)]),
        };
        // A pane of `moved` already shown, or a missing target: nothing changes.
        assert!(
            !node
                .clone()
                .split_with(&k(a), Axis::Vertical, Node::pane(a))
        );
        assert!(!node.split_with(&k(b), Axis::Vertical, moved.clone()));
        assert!(node.split_with(&k(a), Axis::Vertical, moved.clone()));
        assert_eq!(
            node,
            Node::Split {
                split: Axis::Vertical,
                ratio: 0.5,
                children: Box::new([Node::pane(a), moved]),
            }
        );
    }

    #[test]
    fn presets_build_their_trees_in_pane_order() {
        let [a, b, c, d] = [Ulid::new(), Ulid::new(), Ulid::new(), Ulid::new()];
        let half = |axis: &str, first: serde_json::Value, second: serde_json::Value| json!({"split": axis, "ratio": 0.5, "children": [first, second]});
        let pane = |id: Ulid| json!({"session": id.to_string()});
        let json = |preset: Preset, sessions: &[Ulid]| {
            Node::to_json(preset.build(&leaves(sessions)).as_ref())
        };
        assert_eq!(
            json(Preset::Columns2, &[a, b]),
            half("vertical", pane(a), pane(b))
        );
        assert_eq!(
            json(Preset::Rows2, &[a, b]),
            half("horizontal", pane(a), pane(b))
        );
        assert_eq!(
            json(Preset::MainAndStack, &[a, b, c]),
            half("vertical", pane(a), half("horizontal", pane(b), pane(c)))
        );
        assert_eq!(
            json(Preset::Grid, &[a, b, c, d]),
            half(
                "vertical",
                half("horizontal", pane(a), pane(b)),
                half("horizontal", pane(c), pane(d))
            )
        );
        // Re-applied to its own panes (in tree order), a preset keeps every pane in place.
        let grid = Preset::Grid.build(&leaves(&[a, b, c, d])).unwrap();
        assert_eq!(Preset::Grid.build(&grid.leaves()), Some(grid));
        // Only the preset's number of panes.
        assert_eq!(Preset::Columns3.build(&leaves(&[a, b])), None);
        assert_eq!(Preset::Columns2.build(&leaves(&[a, b, c])), None);
    }

    #[test]
    fn three_columns_are_equal() {
        let rects = Preset::Columns3.rects();
        let xs: Vec<f64> = rects.iter().map(|rect| rect.x).collect();
        for (x, want) in xs.iter().zip([0.0, 1.0 / 3.0, 2.0 / 3.0]) {
            assert!((x - want).abs() < 1e-9, "{xs:?}");
        }
        assert!(
            rects
                .iter()
                .all(|rect| (rect.w - 1.0 / 3.0).abs() < 1e-9 && rect.h == 1.0)
        );
        assert!(
            Preset::ALL
                .iter()
                .all(|preset| preset.rects().len() == preset.panes())
        );
    }

    #[test]
    fn equalize_shares_each_run_of_splits_evenly() {
        let [a, b, c, d] = ids();
        // Split right twice from the new pane: 50 / 25 / 25 becomes thirds.
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Vertical, b);
        split(&mut node, b, Axis::Vertical, c);
        assert!(node.equalize());
        for (_, rect) in node.rects() {
            assert!((rect.w - 1.0 / 3.0).abs() < 1e-9, "{:?}", node.rects());
        }
        assert!(!node.equalize(), "already even");
        // Another axis counts as one pane: A | (B / (C | D)) halves at every level.
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Vertical, b);
        split(&mut node, b, Axis::Horizontal, c);
        split(&mut node, c, Axis::Vertical, d);
        node.resize(&k(a), Direction::Right, 0.3);
        node.equalize();
        let rects: std::collections::HashMap<PaneKey, Rect> = node.rects().into_iter().collect();
        assert!((rects[&k(a)].w - 0.5).abs() < 1e-9);
        assert!((rects[&k(b)].h - 0.5).abs() < 1e-9 && (rects[&k(b)].w - 0.5).abs() < 1e-9);
        assert!((rects[&k(c)].w - 0.25).abs() < 1e-9 && (rects[&k(d)].w - 0.25).abs() < 1e-9);
        assert!(!Node::pane(a).equalize());
    }

    #[test]
    fn dividers_and_set_ratio_address_splits_in_tree_order() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Vertical, b);
        split(&mut node, b, Axis::Horizontal, c);
        let dividers = node.dividers();
        assert_eq!(dividers.len(), 2);
        assert_eq!((dividers[0].index, dividers[0].axis), (0, Axis::Vertical));
        assert_eq!(dividers[0].area, Rect::WHOLE);
        assert_eq!((dividers[1].index, dividers[1].axis), (1, Axis::Horizontal));
        assert_eq!(dividers[1].area.x, 0.5);
        assert!(node.set_ratio(1, 0.7));
        assert_eq!(node.dividers()[1].ratio, 0.7);
        assert_eq!(node.dividers()[0].ratio, 0.5);
        // Kept within MIN_RATIO; unknown index or same ratio changes nothing.
        assert!(node.set_ratio(0, 0.01));
        assert_eq!(node.dividers()[0].ratio, MIN_RATIO);
        assert!(!node.set_ratio(0, 0.0));
        assert!(!node.set_ratio(2, 0.5));
        assert!(!node.set_ratio(0, f64::NAN));
        // The pointer's position inside the split's own area.
        let [side, stack] = [node.dividers()[0], node.dividers()[1]];
        assert!((side.ratio_at(0.3, 0.9) - 0.3).abs() < 1e-9);
        assert!((stack.ratio_at(0.9, 0.25) - 0.25).abs() < 1e-9);
        let inner = Divider {
            area: Rect {
                x: 0.5,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            },
            ..side
        };
        assert!((inner.ratio_at(0.625, 0.0) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn spec_example_round_trips() {
        let [a, b, c] = ids();
        let stored = json!({"split":"vertical","ratio":0.5,"children":[
            {"session": a.to_string()},
            {"split":"horizontal","ratio":0.6,"children":[
                {"session": b.to_string()}, {"session": c.to_string()}
            ]}
        ]});
        let node = Node::from_json(&stored).unwrap().unwrap();
        assert_eq!(node.sessions(), vec![a, b, c]);
        assert_eq!(Node::to_json(Some(&node)), stored);
    }

    #[test]
    fn null_is_an_empty_tab() {
        assert_eq!(Node::from_json(&serde_json::Value::Null).unwrap(), None);
        assert_eq!(Node::to_json(None), serde_json::Value::Null);
    }

    #[test]
    fn invalid_layouts_are_rejected() {
        let [a, b] = ids();
        let three = json!({"split":"vertical","ratio":0.5,"children":[
            {"session": a.to_string()}, {"session": b.to_string()}, {"session": a.to_string()}
        ]});
        assert!(matches!(Node::from_json(&three), Err(LayoutError::Json(_))));
        let ratio = json!({"split":"vertical","ratio":1.5,"children":[
            {"session": a.to_string()}, {"session": b.to_string()}
        ]});
        assert!(matches!(
            Node::from_json(&ratio),
            Err(LayoutError::Ratio(_))
        ));
        let twice = json!({"split":"horizontal","ratio":0.5,"children":[
            {"session": a.to_string()}, {"session": a.to_string()}
        ]});
        assert!(matches!(
            Node::from_json(&twice),
            Err(LayoutError::Duplicate(key)) if key == PaneKey::Session(a)
        ));
        // A leaf that is not a pane is dropped, not an error (ADR 0108): the tab opens empty.
        assert_eq!(
            Node::from_json(&json!({"session": "not-a-ulid"})).unwrap(),
            None
        );
        assert_eq!(Node::from_json(&json!("pane")).unwrap(), None);
    }

    #[test]
    fn split_puts_the_new_pane_right_or_below() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        assert!(split(&mut node, a, Axis::Vertical, b));
        assert!(split(&mut node, b, Axis::Horizontal, c));
        assert_eq!(
            Node::to_json(Some(&node)),
            json!({"split":"vertical","ratio":0.5,"children":[
                {"session": a.to_string()},
                {"split":"horizontal","ratio":0.5,"children":[
                    {"session": b.to_string()}, {"session": c.to_string()}
                ]}
            ]})
        );
    }

    #[test]
    fn split_of_unknown_target_or_existing_session_is_refused() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        assert!(!split(&mut node, c, Axis::Vertical, b));
        assert!(!split(&mut node, a, Axis::Vertical, a));
        assert_eq!(node, Node::pane(a));
    }

    #[test]
    fn close_promotes_the_sibling_and_focuses_the_nearest_pane() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Vertical, b);
        split(&mut node, b, Axis::Horizontal, c);

        // Closing the left pane: the right column takes the whole tab, its top pane gets focus.
        let closed = node.clone().close(&k(a));
        assert_eq!(closed.focus, Some(k(b)));
        let remaining = closed.remaining.unwrap();
        assert_eq!(remaining.sessions(), vec![b, c]);
        assert!(matches!(
            remaining,
            Node::Split {
                split: Axis::Horizontal,
                ..
            }
        ));

        // Closing the bottom right pane: focus moves to the pane above it.
        let closed = node.clone().close(&k(c));
        assert_eq!(closed.focus, Some(k(b)));
        let remaining = closed.remaining.unwrap();
        assert_eq!(remaining.sessions(), vec![a, b]);
        assert!(matches!(
            remaining,
            Node::Split {
                split: Axis::Vertical,
                ..
            }
        ));
    }

    #[test]
    fn close_keeps_child_order_and_ratio() {
        let [a, b, c] = ids();
        // a | (b / c) with ratio 0.3: closing b keeps a left of c with the same ratio.
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Vertical, b);
        split(&mut node, b, Axis::Horizontal, c);
        if let Node::Split { ratio, .. } = &mut node {
            *ratio = 0.3;
        }
        let closed = node.close(&k(b));
        assert_eq!(closed.focus, Some(k(c)));
        assert_eq!(
            Node::to_json(closed.remaining.as_ref()),
            json!({"split":"vertical","ratio":0.3,"children":[
                {"session": a.to_string()}, {"session": c.to_string()}
            ]})
        );
    }

    #[test]
    fn close_the_last_pane_or_an_unknown_one() {
        let [a, b] = ids();
        let closed = Node::pane(a).close(&k(a));
        assert_eq!(closed.remaining, None);
        assert_eq!(closed.focus, None);
        let closed = Node::pane(a).close(&k(b));
        assert_eq!(closed.remaining, Some(Node::pane(a)));
        assert_eq!(closed.focus, None);
    }

    #[test]
    fn rects_follow_ratios() {
        let [a, b, c] = ids();
        let node = Node::from_json(&json!({"split":"vertical","ratio":0.25,"children":[
            {"session": a.to_string()},
            {"split":"horizontal","ratio":0.5,"children":[
                {"session": b.to_string()}, {"session": c.to_string()}
            ]}
        ]}))
        .unwrap()
        .unwrap();
        let rects = node.rects();
        assert_eq!(
            rects[0],
            (
                k(a),
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.25,
                    h: 1.0
                }
            )
        );
        assert_eq!(
            rects[1],
            (
                k(b),
                Rect {
                    x: 0.25,
                    y: 0.0,
                    w: 0.75,
                    h: 0.5
                }
            )
        );
        assert_eq!(
            rects[2],
            (
                k(c),
                Rect {
                    x: 0.25,
                    y: 0.5,
                    w: 0.75,
                    h: 0.5
                }
            )
        );
    }

    #[test]
    fn neighbour_moves_across_nested_splits() {
        // ┌───┬───┐
        // │   │ b │
        // │ a ├───┤
        // │   │ c │
        // ├───┴───┤
        // │   d   │
        // └───────┘
        let [a, b, c, d] = ids();
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Horizontal, d);
        split(&mut node, a, Axis::Vertical, b);
        split(&mut node, b, Axis::Horizontal, c);

        assert_eq!(node.neighbour(&k(a), Direction::Right), Some(k(b)));
        assert_eq!(node.neighbour(&k(c), Direction::Left), Some(k(a)));
        assert_eq!(node.neighbour(&k(b), Direction::Left), Some(k(a)));
        assert_eq!(node.neighbour(&k(b), Direction::Down), Some(k(c)));
        assert_eq!(node.neighbour(&k(c), Direction::Down), Some(k(d)));
        assert_eq!(node.neighbour(&k(a), Direction::Down), Some(k(d)));
        assert_eq!(node.neighbour(&k(c), Direction::Up), Some(k(b)));
        // d spans both columns and overlaps a and c equally: the left one wins.
        assert_eq!(node.neighbour(&k(d), Direction::Up), Some(k(a)));
        assert_eq!(node.neighbour(&k(a), Direction::Left), None);
        assert_eq!(node.neighbour(&k(b), Direction::Up), None);
        assert_eq!(node.neighbour(&k(d), Direction::Down), None);
        assert_eq!(node.neighbour(&k(Ulid::new()), Direction::Up), None);
    }

    fn ratios(node: &Node) -> Vec<f64> {
        match node {
            Node::Leaf(_) => Vec::new(),
            Node::Split {
                ratio, children, ..
            } => {
                let mut out = vec![*ratio];
                out.extend(children.iter().flat_map(ratios));
                out
            }
        }
    }

    #[test]
    fn resize_moves_the_innermost_matching_divider() {
        // a | (b / c)
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Vertical, b);
        split(&mut node, b, Axis::Horizontal, c);

        // Left/right from c skips the stacked split and moves the outer divider.
        assert!(node.resize(&k(c), Direction::Right, 0.1));
        assert_ratios(&node, &[0.6, 0.5]);
        assert!(node.resize(&k(a), Direction::Left, 0.2));
        assert_ratios(&node, &[0.4, 0.5]);
        // Up/down moves the inner divider, the same way from either pane of it.
        assert!(node.resize(&k(b), Direction::Down, 0.1));
        assert!(node.resize(&k(c), Direction::Down, 0.1));
        assert_ratios(&node, &[0.4, 0.7]);
        // a has no stacked split around it.
        assert!(!node.resize(&k(a), Direction::Up, 0.1));
        assert!(!node.resize(&k(Ulid::new()), Direction::Left, 0.1));
        assert!(!Node::pane(a).resize(&k(a), Direction::Left, 0.1));
        assert_ratios(&node, &[0.4, 0.7]);
    }

    fn assert_ratios(node: &Node, expected: &[f64]) {
        let actual = ratios(node);
        assert_eq!(actual.len(), expected.len(), "{actual:?}");
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 1e-9, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn resize_stops_at_the_minimum_share() {
        let [a, b] = ids();
        let mut node = Node::pane(a);
        split(&mut node, a, Axis::Vertical, b);
        assert!(node.resize(&k(a), Direction::Right, 0.45));
        assert_ratios(&node, &[1.0 - MIN_RATIO]);
        assert!(!node.resize(&k(b), Direction::Right, RESIZE_STEP));
        assert!(node.resize(&k(b), Direction::Left, 2.0));
        assert_ratios(&node, &[MIN_RATIO]);
        // The clamped layout still round-trips.
        let stored = Node::to_json(Some(&node));
        assert_eq!(Node::from_json(&stored).unwrap(), Some(node));
    }

    #[test]
    fn neighbour_prefers_the_largest_overlap() {
        // a | (b 0.2 / c 0.8): from a, right goes to c (overlap 0.8 > 0.2) although b is on top.
        let [a, b, c] = ids();
        let node = Node::from_json(&json!({"split":"vertical","ratio":0.5,"children":[
            {"session": a.to_string()},
            {"split":"horizontal","ratio":0.2,"children":[
                {"session": b.to_string()}, {"session": c.to_string()}
            ]}
        ]}))
        .unwrap()
        .unwrap();
        assert_eq!(node.neighbour(&k(a), Direction::Right), Some(k(c)));
    }

    #[test]
    fn a_leaf_keeps_its_chat_flag_through_json_and_absent_stays_absent() {
        let [a, b] = ids();
        let json = serde_json::json!({"split":"vertical","ratio":0.5,"children":[
            {"session": a.to_string(), "chat": true},
            {"session": b.to_string()}
        ]});
        let node = Node::from_json(&json).unwrap().unwrap();
        assert_eq!(node.chat_of(a), Some(true));
        assert_eq!(node.chat_of(b), None);
        assert_eq!(
            Node::to_json(Some(&node)),
            json,
            "nothing is added to a leaf without a flag"
        );
    }

    #[test]
    fn set_chat_changes_only_that_pane_and_reports_a_change() {
        let [a, b, other] = ids();
        let mut node = Node::pane(a);
        assert!(split(&mut node, a, Axis::Vertical, b));
        assert!(node.set_chat(b, Some(true)));
        assert!(!node.set_chat(b, Some(true)), "unchanged");
        assert_eq!((node.chat_of(a), node.chat_of(b)), (None, Some(true)));
        assert!(node.set_chat(b, Some(false)));
        assert_eq!(node.chat_of(b), Some(false));
        assert!(node.set_chat(b, None));
        assert!(!node.set_chat(other, Some(true)), "not in the tree");
        // Closing a sibling leaves the flag on the surviving pane.
        node.set_chat(a, Some(true));
        let closed = node.close(&k(b));
        assert_eq!(closed.remaining.unwrap().chat_of(a), Some(true));
    }

    #[test]
    fn splitting_a_pane_keeps_its_chat_flag() {
        let [a, b] = ids();
        let mut node = Node::pane(a);
        node.set_chat(a, Some(true));
        assert!(split(&mut node, a, Axis::Vertical, b));
        assert_eq!((node.chat_of(a), node.chat_of(b)), (Some(true), None));
    }

    #[test]
    fn the_three_leaf_kinds_round_trip() {
        let id = Ulid::new();
        let session = json!({"session": id.to_string(), "chat": true});
        let file = json!({"file": "src/a.rs", "md": "split", "preview": true});
        let diff = json!({"diff": true});
        for stored in [session, file, diff] {
            let leaf: Leaf = serde_json::from_value(stored.clone()).unwrap();
            assert_eq!(serde_json::to_value(&leaf).unwrap(), stored);
        }
        // Optional keys stay out of the stored form.
        assert_eq!(
            serde_json::to_value(Leaf::session(id)).unwrap(),
            json!({"session": id.to_string()})
        );
        assert_eq!(
            serde_json::to_value(Leaf::file("a.md")).unwrap(),
            json!({"file": "a.md"})
        );
        assert_eq!(
            serde_json::to_value(Leaf::diff()).unwrap(),
            json!({"diff": true})
        );
    }

    #[test]
    fn a_web_leaf_round_trips_and_has_its_id_as_key() {
        let id = Ulid::new();
        let stored = json!({"web": "https://example.com/", "id": id.to_string()});
        let leaf: Leaf = serde_json::from_value(stored.clone()).unwrap();
        assert_eq!(leaf.key(), PaneKey::Web(id));
        assert!(leaf.has_key(&PaneKey::Web(id)));
        assert!(!leaf.has_key(&PaneKey::Web(Ulid::new())));
        assert_eq!(serde_json::to_value(&leaf).unwrap(), stored);
        // Not loaded yet: an empty URL.
        let empty = json!({"web": "", "id": id.to_string()});
        let node = Node::from_json(&empty).unwrap().expect("a layout");
        assert_eq!(Node::to_json(Some(&node)), empty);
        assert_eq!(PaneKey::Web(id).session(), None);
        // A fresh leaf gets a fresh id.
        assert_ne!(Leaf::web("a").key(), Leaf::web("a").key());
    }

    #[test]
    fn broken_web_leaves_are_dropped_and_the_same_id_twice_is_refused() {
        let [a] = ids();
        let load = |value: serde_json::Value| Node::from_json(&value).unwrap();
        for bad in [
            json!({"web": 3, "id": Ulid::new().to_string()}),
            json!({"web": "https://x.dev"}),
            json!({"web": "https://x.dev", "id": "bad"}),
        ] {
            assert_eq!(
                load(halves_json("vertical", pane_json(a), bad)),
                Some(Node::pane(a))
            );
        }
        let id = Ulid::new().to_string();
        let twice = halves_json(
            "vertical",
            json!({"web": "https://a.dev", "id": id}),
            json!({"web": "https://b.dev", "id": id}),
        );
        assert!(matches!(
            Node::from_json(&twice),
            Err(LayoutError::Duplicate(PaneKey::Web(_)))
        ));
    }

    #[test]
    fn a_leaf_has_the_key_of_its_kind() {
        let id = Ulid::new();
        assert_eq!(Leaf::session(id).key(), PaneKey::Session(id));
        assert_eq!(
            Leaf::file("src/a.rs").key(),
            PaneKey::File(FileKey::project("src/a.rs"))
        );
        assert_eq!(Leaf::diff().key(), PaneKey::Diff);
        // The flags are not part of the key: one file has one key.
        let flagged = Leaf::File {
            file: "src/a.rs".into(),
            task: None,
            md: Some(mkrew_core::settings::MdMode::Split),
            preview: true,
        };
        assert_eq!(flagged.key(), Leaf::file("src/a.rs").key());
        assert_eq!(PaneKey::Session(id).session(), Some(id));
        assert_eq!(PaneKey::File(FileKey::project("a")).session(), None);
        assert_eq!(PaneKey::Diff.session(), None);
    }

    #[test]
    fn the_nearest_edge_is_the_smallest_distance() {
        assert_eq!(Edge::nearest(0.9, 0.5), Edge::Right);
        assert_eq!(Edge::nearest(0.1, 0.5), Edge::Left);
        assert_eq!(Edge::nearest(0.5, 0.1), Edge::Top);
        assert_eq!(Edge::nearest(0.5, 0.9), Edge::Bottom);
        // Off-axis pointers still pick the closest side.
        assert_eq!(Edge::nearest(0.2, 0.3), Edge::Left);
        assert_eq!(Edge::nearest(0.7, 0.2), Edge::Top);
        assert_eq!(Edge::nearest(0.6, 0.9), Edge::Bottom);
        assert_eq!(Edge::nearest(0.8, 0.7), Edge::Right);
        // On the pane's own border.
        assert_eq!(Edge::nearest(0.0, 0.5), Edge::Left);
        assert_eq!(Edge::nearest(1.0, 0.5), Edge::Right);
    }

    #[test]
    fn edge_ties_go_right_then_bottom_then_left_then_top() {
        // The very middle: all four are equal.
        assert_eq!(Edge::nearest(0.5, 0.5), Edge::Right);
        // Right and Bottom (the bottom right corner), Right and Top.
        assert_eq!(Edge::nearest(0.75, 0.75), Edge::Right);
        assert_eq!(Edge::nearest(0.75, 0.25), Edge::Right);
        // Bottom and Left (the bottom left corner).
        assert_eq!(Edge::nearest(0.25, 0.75), Edge::Bottom);
        // Left and Top (the top left corner).
        assert_eq!(Edge::nearest(0.25, 0.25), Edge::Left);
        // Left and Right at the horizontal middle, Top clearly nearer.
        assert_eq!(Edge::nearest(0.5, 0.25), Edge::Top);
    }

    #[test]
    fn an_edge_knows_its_axis_and_whether_it_leads() {
        assert_eq!(Edge::Left.axis(), Axis::Vertical);
        assert_eq!(Edge::Right.axis(), Axis::Vertical);
        assert_eq!(Edge::Top.axis(), Axis::Horizontal);
        assert_eq!(Edge::Bottom.axis(), Axis::Horizontal);
        assert!(Edge::Left.leads());
        assert!(Edge::Top.leads());
        assert!(!Edge::Right.leads());
        assert!(!Edge::Bottom.leads());
    }

    #[test]
    fn a_layout_with_all_three_leaf_kinds_round_trips() {
        let a = Ulid::new();
        let stored = json!({"split":"vertical","ratio":0.5,"children":[
            {"session": a.to_string(), "chat": true},
            {"split":"horizontal","ratio":0.6,"children":[
                {"file": "src/a.rs", "md": "split", "preview": true},
                {"diff": true}
            ]}
        ]});
        let node = Node::from_json(&stored).unwrap().unwrap();
        assert_eq!(
            node.keys(),
            [
                k(a),
                PaneKey::File(FileKey::project("src/a.rs")),
                PaneKey::Diff
            ]
        );
        assert_eq!(node.sessions(), [a]);
        assert_eq!(Node::to_json(Some(&node)), stored);
        // A file with no flags stores no flags.
        let plain = json!({"file": "README.md"});
        let node = Node::from_json(&plain).unwrap().unwrap();
        assert_eq!(Node::to_json(Some(&node)), plain);
    }

    #[test]
    fn a_protocol_58_layout_reads_unchanged() {
        let [a, b, c] = ids();
        // Sessions only, with and without the `chat` flag: what every stored layout looks like.
        let stored = json!({"split":"vertical","ratio":0.25,"children":[
            {"session": a.to_string(), "chat": false},
            {"split":"horizontal","ratio":0.5,"children":[
                {"session": b.to_string()}, {"session": c.to_string(), "chat": true}
            ]}
        ]});
        let node = Node::from_json(&stored).unwrap().unwrap();
        let expected = Node::Split {
            split: Axis::Vertical,
            ratio: 0.25,
            children: Box::new([
                Node::leaf(Leaf::Session {
                    session: a,
                    chat: Some(false),
                    tsp: None,
                }),
                Node::Split {
                    split: Axis::Horizontal,
                    ratio: 0.5,
                    children: Box::new([
                        Node::pane(b),
                        Node::leaf(Leaf::Session {
                            session: c,
                            chat: Some(true),
                            tsp: None,
                        }),
                    ]),
                },
            ]),
        };
        assert_eq!(node, expected);
        assert_eq!(Node::to_json(Some(&node)), stored);
        assert_eq!(node.chat_of(a), Some(false));
        assert_eq!(node.chat_of(b), None);
        assert_eq!(node.chat_of(c), Some(true));
    }

    #[test]
    fn leaves_that_do_not_parse_are_dropped_and_the_rest_loads() {
        let [a, b] = ids();
        let load = |value: serde_json::Value| Node::from_json(&value).unwrap();
        // One leaf of an unknown shape, one hand-broken: only those go.
        assert_eq!(
            load(halves_json("vertical", pane_json(a), json!({"canvas": 1}))),
            Some(Node::pane(a))
        );
        assert_eq!(
            load(halves_json("vertical", json!({"file": 3}), pane_json(a))),
            Some(Node::pane(a))
        );
        // `diff` is always true on the wire.
        assert_eq!(
            load(halves_json(
                "vertical",
                json!({"diff": false}),
                pane_json(a)
            )),
            Some(Node::pane(a))
        );
        // A split whose child is bad becomes the good child, at any depth.
        let deep = halves_json(
            "vertical",
            pane_json(a),
            halves_json(
                "horizontal",
                json!({"session": "not-a-ulid"}),
                halves_json("horizontal", pane_json(b), json!({"diff": false})),
            ),
        );
        assert_eq!(
            Node::to_json(load(deep).as_ref()),
            halves_json("vertical", pane_json(a), pane_json(b))
        );
        // Both children bad: the split is gone.
        assert_eq!(
            load(halves_json(
                "vertical",
                json!({"canvas": 1}),
                json!({"file": 3})
            )),
            None
        );
    }

    #[test]
    fn a_root_that_is_entirely_bad_loads_as_an_empty_tab() {
        for bad in [
            json!({"canvas": 1}),
            json!({"file": 3}),
            json!({"diff": false}),
            json!({"session": "not-a-ulid"}),
            json!({}),
            json!([]),
            json!("pane"),
            json!(7),
        ] {
            assert_eq!(Node::from_json(&bad).unwrap(), None, "{bad}");
        }
    }

    #[test]
    fn a_malformed_split_or_ratio_is_still_an_error() {
        let [a, b] = ids();
        let three = json!({"split":"vertical","ratio":0.5,"children":[
            pane_json(a), pane_json(b), pane_json(a)
        ]});
        assert!(matches!(Node::from_json(&three), Err(LayoutError::Json(_))));
        let axis = json!({"split":"diagonal","ratio":0.5,"children":[pane_json(a), pane_json(b)]});
        assert!(matches!(Node::from_json(&axis), Err(LayoutError::Json(_))));
        let text =
            json!({"split":"vertical","ratio":"half","children":[pane_json(a), pane_json(b)]});
        assert!(matches!(Node::from_json(&text), Err(LayoutError::Json(_))));
        // The ratio is checked even when a child is dropped.
        let ratio = json!({"split":"vertical","ratio":1.5,"children":[
            pane_json(a), json!({"canvas": 1})
        ]});
        assert!(matches!(
            Node::from_json(&ratio),
            Err(LayoutError::Ratio(r)) if r == 1.5
        ));
    }

    #[test]
    fn the_same_key_twice_in_a_layout_is_refused() {
        let a = Ulid::new();
        let twice = halves_json("horizontal", pane_json(a), pane_json(a));
        assert!(matches!(
            Node::from_json(&twice),
            Err(LayoutError::Duplicate(key)) if key == k(a)
        ));
        // A file is one key whatever its flags.
        let files = halves_json(
            "vertical",
            json!({"file": "a.md", "preview": true}),
            json!({"file": "a.md", "md": "source"}),
        );
        assert!(matches!(
            Node::from_json(&files),
            Err(LayoutError::Duplicate(PaneKey::File(file))) if file == FileKey::project("a.md")
        ));
        let diffs = halves_json("vertical", json!({"diff": true}), json!({"diff": true}));
        assert!(matches!(
            Node::from_json(&diffs),
            Err(LayoutError::Duplicate(PaneKey::Diff))
        ));
        // Two different files are fine.
        let two = halves_json("vertical", json!({"file": "a.md"}), json!({"file": "b.md"}));
        assert!(Node::from_json(&two).unwrap().is_some());
    }

    #[test]
    fn insert_at_puts_the_leaf_on_each_of_the_four_edges() {
        let a = Ulid::new();
        let file = || Node::leaf(Leaf::file("src/a.rs"));
        let want = |axis: &str, first: serde_json::Value, second: serde_json::Value| {
            halves_json(axis, first, second)
        };
        let new = json!({"file": "src/a.rs"});
        for (edge, expected) in [
            (Edge::Right, want("vertical", pane_json(a), new.clone())),
            (Edge::Left, want("vertical", new.clone(), pane_json(a))),
            (Edge::Bottom, want("horizontal", pane_json(a), new.clone())),
            (Edge::Top, want("horizontal", new.clone(), pane_json(a))),
        ] {
            let mut node = Node::pane(a);
            assert!(node.insert_at(&k(a), edge, file()), "{edge:?}");
            assert_eq!(Node::to_json(Some(&node)), expected, "{edge:?}");
        }
    }

    #[test]
    fn insert_at_splits_an_inner_leaf_and_keeps_its_flags() {
        let [a, b] = ids();
        let mut node = Node::pane(a);
        assert!(split(&mut node, a, Axis::Vertical, b));
        assert!(node.set_chat(b, Some(true)));
        assert!(node.insert_at(&k(b), Edge::Top, Node::leaf(Leaf::diff())));
        assert_eq!(
            Node::to_json(Some(&node)),
            halves_json(
                "vertical",
                pane_json(a),
                halves_json(
                    "horizontal",
                    json!({"diff": true}),
                    json!({"session": b.to_string(), "chat": true})
                )
            )
        );
        assert_eq!(node.keys(), [k(a), PaneKey::Diff, k(b)]);
    }

    #[test]
    fn insert_at_refuses_a_missing_target_or_a_key_already_there() {
        let [a, b] = ids();
        let mut node = Node::pane(a);
        assert!(node.insert_at(&k(a), Edge::Right, Node::leaf(Leaf::file("x"))));
        let before = node.clone();
        // No such target.
        assert!(!node.insert_at(&k(b), Edge::Right, Node::leaf(Leaf::diff())));
        assert!(!node.insert_at(
            &PaneKey::File(FileKey::project("other")),
            Edge::Left,
            Node::leaf(Leaf::diff())
        ));
        // The key is in the tree already, alone or inside a whole layout.
        assert!(!node.insert_at(&k(a), Edge::Left, Node::leaf(Leaf::file("x"))));
        assert!(!node.insert_at(&k(a), Edge::Left, Node::pane(a)));
        let layout = Node::Split {
            split: Axis::Vertical,
            ratio: 0.5,
            children: Box::new([Node::leaf(Leaf::diff()), Node::pane(a)]),
        };
        assert!(!node.insert_at(&k(a), Edge::Left, layout));
        assert_eq!(node, before, "nothing changed");
    }

    #[test]
    fn a_whole_layout_goes_in_on_the_leading_edge() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        let moved = Node::Split {
            split: Axis::Horizontal,
            ratio: 0.3,
            children: Box::new([Node::pane(b), Node::leaf(Leaf::file("x"))]),
        };
        assert!(node.insert_at(&k(a), Edge::Left, moved.clone()));
        assert_eq!(
            node,
            Node::Split {
                split: Axis::Vertical,
                ratio: 0.5,
                children: Box::new([moved, Node::pane(a)]),
            }
        );
        assert!(!node.contains(&k(c)));
    }

    #[test]
    fn file_and_diff_leaves_lay_out_close_move_and_resize_like_sessions() {
        // a | (x / diff)
        let a = Ulid::new();
        let x = PaneKey::File(FileKey::project("x"));
        let mut node = Node::pane(a);
        assert!(node.split_with(&k(a), Axis::Vertical, Node::leaf(Leaf::file("x"))));
        assert!(node.split_with(&x, Axis::Horizontal, Node::leaf(Leaf::diff())));
        let keys: Vec<PaneKey> = node.rects().into_iter().map(|(key, _)| key).collect();
        assert_eq!(keys, [k(a), x.clone(), PaneKey::Diff]);
        assert!(node.contains(&PaneKey::Diff));
        assert!(!node.contains(&PaneKey::File(FileKey::project("y"))));
        // Neighbours across kinds: a's right neighbour is the upper one of the column.
        assert_eq!(node.neighbour(&k(a), Direction::Right), Some(x.clone()));
        assert_eq!(node.neighbour(&x, Direction::Down), Some(PaneKey::Diff));
        assert_eq!(node.neighbour(&PaneKey::Diff, Direction::Left), Some(k(a)));
        // Resize by a file leaf moves the stacked divider.
        assert!(node.resize(&x, Direction::Down, 0.1));
        assert_ratios(&node, &[0.5, 0.6]);
        // Closing a leaf promotes the sibling and says which leaf takes the focus.
        let closed = node.clone().close(&x);
        assert_eq!(closed.focus, Some(PaneKey::Diff));
        assert_eq!(closed.remaining.unwrap().keys(), [k(a), PaneKey::Diff]);
        let closed = node.clone().close(&PaneKey::Diff);
        assert_eq!(closed.focus, Some(x.clone()));
        let closed = node.close(&k(a));
        assert_eq!(closed.focus, Some(x));
        assert_eq!(
            closed.remaining.unwrap().keys(),
            [PaneKey::File(FileKey::project("x")), PaneKey::Diff]
        );
    }

    #[test]
    fn chat_flags_only_live_on_session_leaves() {
        let [a, other] = ids();
        let mut node = Node::pane(a);
        assert!(node.split_with(&k(a), Axis::Vertical, Node::leaf(Leaf::file("x"))));
        assert!(node.set_chat(a, Some(true)));
        assert_eq!(node.chat_of(a), Some(true));
        assert!(!node.set_chat(other, Some(true)), "not in the tree");
        assert_eq!(node.chat_of(other), None);
        // The file leaf has no chat flag and is not touched.
        assert_eq!(
            node.leaf_of(&PaneKey::File(FileKey::project("x"))),
            Some(&Leaf::file("x"))
        );
    }

    #[test]
    fn leaf_mut_changes_a_leaf_in_place() {
        let a = Ulid::new();
        let mut node = Node::pane(a);
        assert!(node.split_with(&k(a), Axis::Vertical, Node::leaf(Leaf::file("x"))));
        let key = PaneKey::File(FileKey::project("x"));
        if let Some(Leaf::File { md, preview, .. }) = node.leaf_mut(&key) {
            *md = Some(mkrew_core::settings::MdMode::Source);
            *preview = true;
        }
        assert_eq!(
            Node::to_json(Some(&node))["children"][1],
            json!({"file": "x", "md": "source", "preview": true})
        );
        assert!(node.leaf_mut(&PaneKey::Diff).is_none());
        assert_eq!(node.leaves().len(), 2);
        assert!(node.any_leaf(|leaf| matches!(leaf, Leaf::File { preview: true, .. })));
        assert!(!node.any_leaf(|leaf| matches!(leaf, Leaf::Diff { .. })));
    }

    #[test]
    fn a_preset_builds_with_whole_leaves() {
        let a = Ulid::new();
        let file = Leaf::File {
            file: "README.md".into(),
            task: None,
            md: Some(mkrew_core::settings::MdMode::Split),
            preview: true,
        };
        let built = Preset::Columns2
            .build(&[Leaf::session(a), file.clone()])
            .unwrap();
        assert_eq!(
            Node::to_json(Some(&built)),
            halves_json(
                "vertical",
                pane_json(a),
                json!({"file": "README.md", "md": "split", "preview": true})
            )
        );
        assert_eq!(built.leaves(), [Leaf::session(a), file]);
    }

    /// A layout of protocol 58 / 60 has no `task` on its file leaves: they are project files.
    #[test]
    fn a_file_leaf_without_a_task_is_a_project_file() {
        let old = json!({"file": "src/a.rs", "md": "source", "preview": true});
        let node = Node::from_json(&old).unwrap().expect("a layout");
        let key = PaneKey::File(FileKey::project("src/a.rs"));
        assert_eq!(
            node.leaf_of(&key),
            Some(&Leaf::File {
                file: "src/a.rs".into(),
                task: None,
                md: Some(mkrew_core::settings::MdMode::Source),
                preview: true,
            })
        );
        // Written back as it was: no `task` for a project file.
        assert_eq!(Node::to_json(Some(&node)), old);
    }

    #[test]
    fn a_worktree_file_leaf_keeps_its_task_through_json() {
        let task = Ulid::new();
        let stored = json!({"file": "src/a.rs", "task": task.to_string()});
        let node = Node::from_json(&stored).unwrap().expect("a layout");
        let key = PaneKey::File(FileKey::new("src/a.rs", Some(task)));
        assert_eq!(
            node.leaf_of(&key),
            Some(&Leaf::of_file(FileKey::new("src/a.rs", Some(task))))
        );
        assert_eq!(Node::to_json(Some(&node)), stored);
        // It is not the project's file of the same path.
        assert!(
            node.leaf_of(&PaneKey::File(FileKey::project("src/a.rs")))
                .is_none()
        );
    }

    /// The same relative path in the project and in a worktree are two files: two keys, and
    /// one layout may hold both.
    #[test]
    fn the_same_path_in_two_folders_is_two_keys() {
        let task = Ulid::new();
        let project = FileKey::project("a.md");
        let worktree = FileKey::new("a.md", Some(task));
        assert_ne!(
            PaneKey::File(project.clone()),
            PaneKey::File(worktree.clone())
        );
        assert_ne!(
            Leaf::of_file(project.clone()).key(),
            Leaf::of_file(worktree.clone()).key()
        );
        assert!(Leaf::of_file(worktree.clone()).has_key(&PaneKey::File(worktree.clone())));
        assert!(!Leaf::of_file(worktree.clone()).has_key(&PaneKey::File(project)));
        let both = halves_json(
            "vertical",
            json!({"file": "a.md"}),
            json!({"file": "a.md", "task": task.to_string()}),
        );
        let node = Node::from_json(&both).unwrap().expect("two leaves");
        assert_eq!(node.leaves().len(), 2);
        assert_eq!(Node::to_json(Some(&node)), both);
    }
}
