//! Picking lines with the keyboard (SPEC §12, §12B): what Shift+↓ / Shift+↑ pick in the shown
//! doc or diff. Pure and tested headlessly; the shell builds the units of what is shown, keeps
//! the result in [`Comments`](crate::comments::Comments) and scrolls it into view.

use crate::comments::Selection;

/// One thing a key press picks: a source line, a Markdown block or a new-side diff line.
/// 1-based, inclusive source lines; the units of one view are sorted and do not overlap.
pub type Unit = (u32, u32);

/// The end of a pick of several units that Shift+↑ / Shift+↓ move (the other one stays).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Head {
    Start,
    #[default]
    End,
}

/// What Shift+↓ (`down`) or Shift+↑ does to the `current` pick over `units`.
/// Nothing picked (or a pick no unit touches any more, after an edit): the unit `start` (clamped
/// to the last one) is picked, whatever the direction.
/// One unit picked: the direction decides which end grows.
/// Several: the `head` end moves one unit, so the pick grows or shrinks; it never goes past the
/// first or last unit and never flips over its other end.
/// `None` only when there are no units.
pub fn step(
    units: &[Unit],
    current: Option<Selection>,
    head: Head,
    down: bool,
    start: usize,
) -> Option<(Selection, Head)> {
    let last = units.len().checked_sub(1)?;
    let covered = current.and_then(|pick| {
        let lo = units.iter().position(|unit| unit.1 >= pick.start)?;
        let hi = units.iter().rposition(|unit| unit.0 <= pick.end)?;
        (lo <= hi).then_some((lo, hi))
    });
    let (lo, hi, head) = match covered {
        None => {
            let at = start.min(last);
            (at, at, Head::End)
        }
        Some((lo, hi)) => {
            let head = match (lo == hi, down) {
                (true, true) => Head::End,
                (true, false) => Head::Start,
                (false, _) => head,
            };
            match (head, down) {
                (Head::End, true) => (lo, (hi + 1).min(last), head),
                (Head::End, false) => (lo, hi.saturating_sub(1).max(lo), head),
                (Head::Start, true) => ((lo + 1).min(hi), hi, head),
                (Head::Start, false) => (lo.saturating_sub(1), hi, head),
            }
        }
    };
    Some((
        Selection {
            start: units[lo].0,
            end: units[hi].1,
        },
        head,
    ))
}

/// The first row a list of `row`-high rows shows when it is scrolled by `offset_y` (gpui: zero
/// at the top, negative once scrolled down).
pub fn first_row(offset_y: f32, row: f32) -> usize {
    if row <= 0.0 || offset_y >= 0.0 {
        return 0;
    }
    (-offset_y / row).floor() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(n: u32) -> Vec<Unit> {
        (1..=n).map(|n| (n, n)).collect()
    }

    /// One key press on `pick`; returns the picked `(start, end)`.
    fn press(
        units: &[Unit],
        pick: &mut Option<Selection>,
        head: &mut Head,
        down: bool,
    ) -> (u32, u32) {
        let (selection, moved) = step(units, *pick, *head, down, 0).expect("units");
        *pick = Some(selection);
        *head = moved;
        (selection.start, selection.end)
    }

    #[test]
    fn the_first_press_picks_the_start_unit_whatever_the_direction() {
        let units = lines(5);
        for down in [true, false] {
            assert_eq!(
                step(&units, None, Head::End, down, 2),
                Some((Selection { start: 3, end: 3 }, Head::End))
            );
        }
        // A start point past the end is the last unit.
        assert_eq!(
            step(&units, None, Head::End, true, 99).map(|(s, _)| (s.start, s.end)),
            Some((5, 5))
        );
        // Nothing to pick.
        assert_eq!(step(&[], None, Head::End, true, 0), None);
    }

    #[test]
    fn down_grows_the_end_up_shrinks_it_and_then_grows_the_start() {
        let units = lines(5);
        let (mut pick, mut head) = (Some(Selection { start: 3, end: 3 }), Head::End);
        let mut run = |down| press(&units, &mut pick, &mut head, down);
        assert_eq!(run(true), (3, 4));
        assert_eq!(run(true), (3, 5));
        assert_eq!(run(true), (3, 5), "stops at the last unit");
        assert_eq!(run(false), (3, 4));
        assert_eq!(run(false), (3, 3));
        assert_eq!(run(false), (2, 3), "one unit: up grows the start");
        assert_eq!(run(false), (1, 3));
        assert_eq!(run(false), (1, 3), "stops at the first unit");
        assert_eq!(run(true), (2, 3), "down now shrinks the start");
    }

    #[test]
    fn blocks_are_picked_whole() {
        let units = [(1, 1), (3, 6), (8, 8)];
        let (mut pick, mut head) = (None, Head::End);
        let mut run = |down| press(&units, &mut pick, &mut head, down);
        assert_eq!(run(true), (1, 1));
        assert_eq!(run(true), (1, 6));
        assert_eq!(run(true), (1, 8));
        assert_eq!(run(false), (1, 6));
        // A click inside a block picked part of its lines: the whole block counts.
        let clicked = Some(Selection { start: 4, end: 4 });
        assert_eq!(
            step(&units, clicked, Head::End, true, 0).map(|(s, _)| (s.start, s.end)),
            Some((3, 8))
        );
    }

    #[test]
    fn a_pick_grows_across_the_gap_between_hunks() {
        let units = [(10, 10), (11, 11), (40, 40)];
        let pick = Some(Selection { start: 11, end: 11 });
        assert_eq!(
            step(&units, pick, Head::End, true, 0).map(|(s, _)| (s.start, s.end)),
            Some((11, 40))
        );
    }

    #[test]
    fn a_stale_pick_starts_over() {
        // Lines 7-9 are gone (the file was edited): the start point is picked instead.
        let stale = Some(Selection { start: 7, end: 9 });
        assert_eq!(
            step(&lines(3), stale, Head::Start, true, 1),
            Some((Selection { start: 2, end: 2 }, Head::End))
        );
    }

    #[test]
    fn a_list_scrolled_by_pixels_shows_whole_rows() {
        assert_eq!(first_row(0.0, 20.0), 0);
        assert_eq!(first_row(-45.0, 20.0), 2);
        assert_eq!(first_row(6.0, 20.0), 0, "overscroll");
        assert_eq!(first_row(-10.0, 0.0), 0, "no row height");
    }
}
