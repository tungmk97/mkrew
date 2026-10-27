//! Split tree of a terminal tab (SPEC §5 `terminal_tab.layout`, §10): leaves are session ids,
//! inner nodes split their area in two. Pure; serializes to exactly the §5 JSON shape.

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

/// A node of the split tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Node {
    /// A pane showing one session.
    Pane { session: Ulid },
    /// `ratio` is the share of the first child.
    Split {
        split: Axis,
        ratio: f64,
        children: Box<[Node; 2]>,
    },
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

    /// The tree with `sessions` as its panes in tree order ([`Node::sessions`]: left to
    /// right, top to bottom within a column); `None` unless there are exactly
    /// [`Self::panes`] of them.
    pub fn build(self, sessions: &[Ulid]) -> Option<Node> {
        if sessions.len() != self.panes() {
            return None;
        }
        let pane = |at: usize| Node::pane(sessions[at]);
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
        let ids: Vec<Ulid> = (0..self.panes() as u128).map(Ulid).collect();
        self.build(&ids)
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
    #[error("session {0} appears in more than one pane")]
    Duplicate(Ulid),
}

/// Result of [`Node::close`].
#[derive(Debug, Clone, PartialEq)]
pub struct Closed {
    /// What is left of the tree; `None` when the closed pane was the last one.
    pub remaining: Option<Node>,
    /// The pane that took the closed pane's place, to focus next.
    pub focus: Option<Ulid>,
}

/// Tolerance for comparing summed fractions.
const EPSILON: f64 = 1e-9;

/// How far one resize shortcut moves a divider (share of the split's area).
pub const RESIZE_STEP: f64 = 0.05;

/// Resizing never shrinks a split's child below this share, so no pane disappears.
pub const MIN_RATIO: f64 = 0.1;

impl Node {
    pub fn pane(session: Ulid) -> Self {
        Node::Pane { session }
    }

    /// Parses a stored layout; JSON `null` is a tab without panes.
    pub fn from_json(value: &serde_json::Value) -> Result<Option<Node>, LayoutError> {
        if value.is_null() {
            return Ok(None);
        }
        let node = Node::deserialize(value)?;
        let mut seen = HashSet::new();
        node.validate(&mut seen)?;
        Ok(Some(node))
    }

    /// The stored form of `layout` (`null` without panes).
    pub fn to_json(layout: Option<&Node>) -> serde_json::Value {
        // Serializing this enum cannot fail: no maps with non-string keys, ratios are finite.
        serde_json::to_value(layout).unwrap_or(serde_json::Value::Null)
    }

    fn validate(&self, seen: &mut HashSet<Ulid>) -> Result<(), LayoutError> {
        match self {
            Node::Pane { session } => {
                if !seen.insert(*session) {
                    return Err(LayoutError::Duplicate(*session));
                }
            }
            Node::Split {
                ratio, children, ..
            } => {
                if !(*ratio > 0.0 && *ratio < 1.0) {
                    return Err(LayoutError::Ratio(*ratio));
                }
                for child in children.iter() {
                    child.validate(seen)?;
                }
            }
        }
        Ok(())
    }

    /// Session ids of all panes, left-to-right / top-to-bottom in tree order.
    pub fn sessions(&self) -> Vec<Ulid> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<Ulid>) {
        match self {
            Node::Pane { session } => out.push(*session),
            Node::Split { children, .. } => children.iter().for_each(|c| c.collect(out)),
        }
    }

    pub fn contains(&self, session: Ulid) -> bool {
        match self {
            Node::Pane { session: id } => *id == session,
            Node::Split { children, .. } => children.iter().any(|c| c.contains(session)),
        }
    }

    fn first(&self) -> Ulid {
        match self {
            Node::Pane { session } => *session,
            Node::Split { children, .. } => children[0].first(),
        }
    }

    fn last(&self) -> Ulid {
        match self {
            Node::Pane { session } => *session,
            Node::Split { children, .. } => children[1].last(),
        }
    }

    /// Splits the pane of `target` in half along `axis`; `new` goes right of / below it.
    /// Returns false (and changes nothing) if `target` is not in the tree or `new` already is.
    pub fn split(&mut self, target: Ulid, axis: Axis, new: Ulid) -> bool {
        self.split_with(target, axis, Node::pane(new))
    }

    /// Splits the pane of `target` in half along `axis`; `new` (one pane or a whole layout)
    /// goes right of / below it. Returns false (and changes nothing) if `target` is not in the
    /// tree or a pane of `new` already is.
    pub fn split_with(&mut self, target: Ulid, axis: Axis, new: Node) -> bool {
        if !self.contains(target) || new.sessions().into_iter().any(|id| self.contains(id)) {
            return false;
        }
        self.split_inner(target, axis, &mut Some(new))
    }

    /// `new` is taken by the pane of `target`.
    fn split_inner(&mut self, target: Ulid, axis: Axis, new: &mut Option<Node>) -> bool {
        match self {
            Node::Pane { session } if *session == target => {
                let Some(new) = new.take() else {
                    return false;
                };
                *self = Node::Split {
                    split: axis,
                    ratio: 0.5,
                    children: Box::new([Node::pane(target), new]),
                };
                true
            }
            Node::Pane { .. } => false,
            Node::Split { children, .. } => children
                .iter_mut()
                .any(|child| child.split_inner(target, axis, new)),
        }
    }

    /// Removes the pane of `target`; its sibling takes the parent split's place. Closing a
    /// session that is not in the tree leaves it unchanged.
    pub fn close(self, target: Ulid) -> Closed {
        if !self.contains(target) {
            return Closed {
                remaining: Some(self),
                focus: None,
            };
        }
        match self {
            Node::Pane { .. } => Closed {
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
    pub fn resize(&mut self, target: Ulid, direction: Direction, step: f64) -> bool {
        let (axis, delta) = match direction {
            Direction::Left => (Axis::Vertical, -step),
            Direction::Right => (Axis::Vertical, step),
            Direction::Up => (Axis::Horizontal, -step),
            Direction::Down => (Axis::Horizontal, step),
        };
        self.resize_inner(target, axis, delta) == Some(true)
    }

    /// `None` = no matching split around `target` in this subtree (an outer one may match).
    fn resize_inner(&mut self, target: Ulid, axis: Axis, delta: f64) -> Option<bool> {
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
    pub fn rects(&self) -> Vec<(Ulid, Rect)> {
        let mut out = Vec::new();
        self.rects_inner(Rect::WHOLE, &mut out);
        out
    }

    fn rects_inner(&self, area: Rect, out: &mut Vec<(Ulid, Rect)>) {
        match self {
            Node::Pane { session } => out.push((*session, area)),
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
    pub fn neighbour(&self, from: Ulid, direction: Direction) -> Option<Ulid> {
        let rects = self.rects();
        let (_, origin) = rects.iter().find(|(id, _)| *id == from)?;
        let origin = *origin;
        rects
            .iter()
            .filter(|(id, _)| *id != from)
            .filter_map(|(id, rect)| {
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
                (gap > -EPSILON && overlap > EPSILON).then_some((*id, gap, overlap, along))
            })
            .min_by(|a, b| {
                let gap = if (a.1 - b.1).abs() <= EPSILON {
                    std::cmp::Ordering::Equal
                } else {
                    a.1.total_cmp(&b.1)
                };
                gap.then(b.2.total_cmp(&a.2)).then(a.3.total_cmp(&b.3))
            })
            .map(|(id, ..)| id)
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

/// The split a pane dropped on a pane at `(x, y)` (fractions of that pane, 0..1) makes: right
/// of it (`Vertical`) when the pointer is further right than down, else below it.
pub fn drop_axis(x: f32, y: f32) -> Axis {
    if x > y {
        Axis::Vertical
    } else {
        Axis::Horizontal
    }
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

    #[test]
    fn a_drop_leaning_right_splits_right_else_below() {
        assert_eq!(drop_axis(0.9, 0.5), Axis::Vertical);
        assert_eq!(drop_axis(0.6, 0.1), Axis::Vertical);
        assert_eq!(drop_axis(0.5, 0.9), Axis::Horizontal);
        assert_eq!(drop_axis(0.1, 0.6), Axis::Horizontal);
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
        assert!(!node.clone().split_with(a, Axis::Vertical, Node::pane(a)));
        assert!(!node.split_with(b, Axis::Vertical, moved.clone()));
        assert!(node.split_with(a, Axis::Vertical, moved.clone()));
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
        let json =
            |preset: Preset, sessions: &[Ulid]| Node::to_json(preset.build(sessions).as_ref());
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
        let grid = Preset::Grid.build(&[a, b, c, d]).unwrap();
        assert_eq!(Preset::Grid.build(&grid.sessions()), Some(grid));
        // Only the preset's number of panes.
        assert_eq!(Preset::Columns3.build(&[a, b]), None);
        assert_eq!(Preset::Columns2.build(&[a, b, c]), None);
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
        node.split(a, Axis::Vertical, b);
        node.split(b, Axis::Vertical, c);
        assert!(node.equalize());
        for (_, rect) in node.rects() {
            assert!((rect.w - 1.0 / 3.0).abs() < 1e-9, "{:?}", node.rects());
        }
        assert!(!node.equalize(), "already even");
        // Another axis counts as one pane: A | (B / (C | D)) halves at every level.
        let mut node = Node::pane(a);
        node.split(a, Axis::Vertical, b);
        node.split(b, Axis::Horizontal, c);
        node.split(c, Axis::Vertical, d);
        node.resize(a, Direction::Right, 0.3);
        node.equalize();
        let rects: std::collections::HashMap<Ulid, Rect> = node.rects().into_iter().collect();
        assert!((rects[&a].w - 0.5).abs() < 1e-9);
        assert!((rects[&b].h - 0.5).abs() < 1e-9 && (rects[&b].w - 0.5).abs() < 1e-9);
        assert!((rects[&c].w - 0.25).abs() < 1e-9 && (rects[&d].w - 0.25).abs() < 1e-9);
        assert!(!Node::pane(a).equalize());
    }

    #[test]
    fn dividers_and_set_ratio_address_splits_in_tree_order() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        node.split(a, Axis::Vertical, b);
        node.split(b, Axis::Horizontal, c);
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
            Err(LayoutError::Duplicate(id)) if id == a
        ));
        assert!(Node::from_json(&json!({"session": "not-a-ulid"})).is_err());
        assert!(Node::from_json(&json!("pane")).is_err());
    }

    #[test]
    fn split_puts_the_new_pane_right_or_below() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        assert!(node.split(a, Axis::Vertical, b));
        assert!(node.split(b, Axis::Horizontal, c));
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
        assert!(!node.split(c, Axis::Vertical, b));
        assert!(!node.split(a, Axis::Vertical, a));
        assert_eq!(node, Node::pane(a));
    }

    #[test]
    fn close_promotes_the_sibling_and_focuses_the_nearest_pane() {
        let [a, b, c] = ids();
        let mut node = Node::pane(a);
        node.split(a, Axis::Vertical, b);
        node.split(b, Axis::Horizontal, c);

        // Closing the left pane: the right column takes the whole tab, its top pane gets focus.
        let closed = node.clone().close(a);
        assert_eq!(closed.focus, Some(b));
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
        let closed = node.clone().close(c);
        assert_eq!(closed.focus, Some(b));
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
        node.split(a, Axis::Vertical, b);
        node.split(b, Axis::Horizontal, c);
        if let Node::Split { ratio, .. } = &mut node {
            *ratio = 0.3;
        }
        let closed = node.close(b);
        assert_eq!(closed.focus, Some(c));
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
        let closed = Node::pane(a).close(a);
        assert_eq!(closed.remaining, None);
        assert_eq!(closed.focus, None);
        let closed = Node::pane(a).close(b);
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
                a,
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
                b,
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
                c,
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
        node.split(a, Axis::Horizontal, d);
        node.split(a, Axis::Vertical, b);
        node.split(b, Axis::Horizontal, c);

        assert_eq!(node.neighbour(a, Direction::Right), Some(b));
        assert_eq!(node.neighbour(c, Direction::Left), Some(a));
        assert_eq!(node.neighbour(b, Direction::Left), Some(a));
        assert_eq!(node.neighbour(b, Direction::Down), Some(c));
        assert_eq!(node.neighbour(c, Direction::Down), Some(d));
        assert_eq!(node.neighbour(a, Direction::Down), Some(d));
        assert_eq!(node.neighbour(c, Direction::Up), Some(b));
        // d spans both columns and overlaps a and c equally: the left one wins.
        assert_eq!(node.neighbour(d, Direction::Up), Some(a));
        assert_eq!(node.neighbour(a, Direction::Left), None);
        assert_eq!(node.neighbour(b, Direction::Up), None);
        assert_eq!(node.neighbour(d, Direction::Down), None);
        assert_eq!(node.neighbour(Ulid::new(), Direction::Up), None);
    }

    fn ratios(node: &Node) -> Vec<f64> {
        match node {
            Node::Pane { .. } => Vec::new(),
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
        node.split(a, Axis::Vertical, b);
        node.split(b, Axis::Horizontal, c);

        // Left/right from c skips the stacked split and moves the outer divider.
        assert!(node.resize(c, Direction::Right, 0.1));
        assert_ratios(&node, &[0.6, 0.5]);
        assert!(node.resize(a, Direction::Left, 0.2));
        assert_ratios(&node, &[0.4, 0.5]);
        // Up/down moves the inner divider, the same way from either pane of it.
        assert!(node.resize(b, Direction::Down, 0.1));
        assert!(node.resize(c, Direction::Down, 0.1));
        assert_ratios(&node, &[0.4, 0.7]);
        // a has no stacked split around it.
        assert!(!node.resize(a, Direction::Up, 0.1));
        assert!(!node.resize(Ulid::new(), Direction::Left, 0.1));
        assert!(!Node::pane(a).resize(a, Direction::Left, 0.1));
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
        node.split(a, Axis::Vertical, b);
        assert!(node.resize(a, Direction::Right, 0.45));
        assert_ratios(&node, &[1.0 - MIN_RATIO]);
        assert!(!node.resize(b, Direction::Right, RESIZE_STEP));
        assert!(node.resize(b, Direction::Left, 2.0));
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
        assert_eq!(node.neighbour(a, Direction::Right), Some(c));
    }
}
