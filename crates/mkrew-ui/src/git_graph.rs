//! The lane graph beside the Git History (SPEC §12B): for each listed commit, the lane of its
//! dot and the lines entering it from above and leaving it below. Pure. The commits come
//! newest first from `git log --topo-order --parents`: a commit is listed before its parents.

use mkrew_protocol::Commit;

/// Lanes drawn at most; a wider history is cut at the right (dots sit in the last lane).
// ponytail: fixed cap; scroll the graph sideways if 8 lanes ever hide real work.
pub const MAX_LANES: usize = 8;

/// One commit's row of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRow {
    /// The lane of the commit's dot.
    pub lane: usize,
    /// Lines in the row's top half, `(lane at the top, lane at the dot)`: lanes passing by,
    /// and the lines of this commit's children ending in it.
    pub up: Vec<(usize, usize)>,
    /// Lines in the bottom half, `(lane at the dot, lane at the bottom)`: lanes passing by
    /// and the lines to this commit's parents. Sorted.
    pub down: Vec<(usize, usize)>,
}

impl GraphRow {
    /// The lanes leaving the row at the bottom, each once, in order: they run straight on
    /// through the rows listed under the commit (its files).
    pub fn below(&self) -> Vec<usize> {
        let mut lanes: Vec<usize> = self.down.iter().map(|&(_, to)| to).collect();
        lanes.sort_unstable();
        lanes.dedup();
        lanes
    }
}

/// What the graph column of one History row draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphCell {
    Commit(GraphRow),
    /// A row listed under a commit (a file, a note): these lanes run straight through it.
    Through(Vec<usize>),
}

/// The graph of `commits` (children before parents), one row per commit.
pub fn layout(commits: &[Commit]) -> Vec<GraphRow> {
    // Per lane, the commit its line leads down to; `None` = free.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    for commit in commits {
        let hash = commit.hash.as_str();
        let lane = match lanes.iter().position(|waits| *waits == Some(hash)) {
            Some(lane) => lane,
            // A tip no listed commit leads to.
            None => free_lane(&mut lanes),
        };
        let up: Vec<(usize, usize)> = lanes
            .iter()
            .enumerate()
            .filter_map(|(at, waits)| {
                waits.map(|waits| (at, if waits == hash { lane } else { at }))
            })
            .collect();
        let mut down: Vec<(usize, usize)> = up
            .iter()
            .filter(|&&(from, to)| from == to && from != lane)
            .copied()
            .collect();
        for waits in &mut lanes {
            if *waits == Some(hash) {
                *waits = None;
            }
        }
        for (nth, parent) in commit.parents.iter().enumerate() {
            let parent = parent.as_str();
            let to = if nth == 0 {
                lane
            } else if let Some(waiting) = lanes.iter().position(|waits| *waits == Some(parent)) {
                waiting
            } else {
                free_lane(&mut lanes)
            };
            lanes[to] = Some(parent);
            down.push((lane, to));
        }
        down.sort_unstable();
        down.dedup();
        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        rows.push(GraphRow { lane, up, down });
    }
    rows
}

/// Lanes the graph of `rows` needs (0 for none), at most [`MAX_LANES`].
pub fn lanes(rows: &[GraphRow]) -> usize {
    rows.iter()
        .flat_map(|row| {
            let edges = row
                .up
                .iter()
                .chain(&row.down)
                .flat_map(|&(from, to)| [from, to]);
            std::iter::once(row.lane).chain(edges)
        })
        .max()
        .map_or(0, |widest| (widest + 1).min(MAX_LANES))
}

/// The first free lane, or a new one at the right.
fn free_lane(lanes: &mut Vec<Option<&str>>) -> usize {
    match lanes.iter().position(Option::is_none) {
        Some(lane) => lane,
        None => {
            lanes.push(None);
            lanes.len() - 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, parents: &[&str]) -> Commit {
        Commit {
            hash: hash.into(),
            parents: parents.iter().map(|parent| (*parent).into()).collect(),
            author_name: String::new(),
            author_email: String::new(),
            authored_at: 0,
            summary: String::new(),
            body: String::new(),
        }
    }

    fn row(lane: usize, up: &[(usize, usize)], down: &[(usize, usize)]) -> GraphRow {
        GraphRow {
            lane,
            up: up.to_vec(),
            down: down.to_vec(),
        }
    }

    #[test]
    fn a_straight_history_is_one_lane() {
        let rows = layout(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);
        assert_eq!(
            rows,
            [
                row(0, &[], &[(0, 0)]),
                row(0, &[(0, 0)], &[(0, 0)]),
                row(0, &[(0, 0)], &[])
            ]
        );
        assert_eq!(lanes(&rows), 1);
        assert_eq!(lanes(&[]), 0);
    }

    /// m merges f (a side branch off a) into b:
    ///   m      lane 0, opens lane 1 for f
    ///   | f    lane 1
    ///   b |    lane 0, lane 1 passes by
    ///   a      both lanes end in a
    #[test]
    fn a_merge_opens_a_lane_that_ends_in_the_fork_point() {
        let rows = layout(&[
            commit("m", &["b", "f"]),
            commit("f", &["a"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ]);
        assert_eq!(
            rows,
            [
                row(0, &[], &[(0, 0), (0, 1)]),
                row(1, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)]),
                row(0, &[(0, 0), (1, 1)], &[(0, 0), (1, 1)]),
                row(0, &[(0, 0), (1, 0)], &[]),
            ]
        );
        assert_eq!(rows[0].below(), [0, 1]);
        assert_eq!(lanes(&rows), 2);
    }

    /// The log stops at 50 commits: x's parents are not listed.
    #[test]
    fn lanes_to_unlisted_parents_run_off_the_bottom() {
        let rows = layout(&[commit("y", &["x"]), commit("x", &["p", "q"])]);
        assert_eq!(rows[1], row(0, &[(0, 0)], &[(0, 0), (0, 1)]));
        assert_eq!(rows[1].below(), [0, 1]);
    }

    #[test]
    fn lanes_are_capped() {
        let parents: Vec<String> = (0..12).map(|n| format!("p{n}")).collect();
        let parents: Vec<&str> = parents.iter().map(String::as_str).collect();
        let rows = layout(&[commit("octopus", &parents)]);
        assert_eq!(rows[0].down.len(), 12);
        assert_eq!(lanes(&rows), MAX_LANES);
    }
}
