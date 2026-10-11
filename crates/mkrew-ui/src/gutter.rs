//! The Files editor's change gutter (SPEC §12A, ADR 0047): per line of the edited text, how it
//! differs from the file's text in the git index. Pure; slow on big files, so run it off the
//! UI thread.

use std::time::{Duration, Instant};

use similar::{Algorithm, DiffOp, capture_diff_slices_deadline};

/// How one line differs from the index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    Modified,
    /// Lines were removed just above this one (or, on the last line, after it).
    Deleted,
}

/// A diff taking longer stops here with a coarser (still correct) answer.
// ponytail: fixed budget; make it scale with the file if 2 MB rewrites mark too much.
const DEADLINE: Duration = Duration::from_millis(100);

/// One mark per line of `text` (`str::lines`: `\n` or `\r\n`, so CRLF text matches its LF
/// index text) against `base`.
pub fn changes(base: &str, text: &str) -> Vec<Option<Change>> {
    let old: Vec<&str> = base.lines().collect();
    let new: Vec<&str> = text.lines().collect();
    let mut marks = vec![None; new.len()];
    let Some(last) = new.len().checked_sub(1) else {
        return marks;
    };
    let ops = capture_diff_slices_deadline(
        Algorithm::Myers,
        &old,
        &new,
        Some(Instant::now() + DEADLINE),
    );
    for op in ops {
        match op {
            DiffOp::Equal { .. } => {}
            DiffOp::Insert {
                new_index, new_len, ..
            } => {
                marks[new_index..new_index + new_len].fill(Some(Change::Added));
            }
            DiffOp::Replace {
                new_index, new_len, ..
            } => {
                marks[new_index..new_index + new_len].fill(Some(Change::Modified));
            }
            DiffOp::Delete { new_index, .. } => {
                marks[new_index.min(last)].get_or_insert(Change::Deleted);
            }
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use Change::*;

    #[test]
    fn unchanged_text_has_no_marks() {
        assert_eq!(changes("a\nb\n", "a\nb\n"), [None, None]);
    }

    #[test]
    fn added_changed_and_removed_lines_are_marked() {
        // b → B changed, d removed (marked on e, the line after it), f added.
        let marks = changes("a\nb\nc\nd\ne\n", "a\nB\nc\ne\nf\n");
        assert_eq!(
            marks,
            [None, Some(Modified), None, Some(Deleted), Some(Added)]
        );
    }

    #[test]
    fn lines_removed_at_the_end_mark_the_last_line() {
        assert_eq!(changes("a\nb\n", "a\n"), [Some(Deleted)]);
        assert_eq!(changes("a\n", ""), Vec::<Option<Change>>::new());
    }

    #[test]
    fn an_empty_tracked_file_that_got_text_is_all_added() {
        assert_eq!(changes("", "x\ny\n"), [Some(Added), Some(Added)]);
    }

    /// Windows: `core.autocrlf` keeps LF in the index and CRLF in the working file.
    #[test]
    fn crlf_lines_match_their_lf_base() {
        assert_eq!(changes("a\nb\n", "a\r\nb\r\n"), [None, None]);
    }
}
