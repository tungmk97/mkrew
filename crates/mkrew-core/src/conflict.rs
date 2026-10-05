//! Conflict detection (SPEC §7 `conflict.detected`, §18 M5): which sessions edited the same file
//! of one project within [`WINDOW_MS`]. Pure; the daemon decides who edited a file.

use std::collections::HashMap;

/// Two edits of one file by different sessions this close together are a conflict.
pub const WINDOW_MS: i64 = 60_000;

/// The recent editors of the files of one project. `S` is a session id.
#[derive(Debug)]
pub struct Tracker<S> {
    /// Relative path → each editor with the time (ms) of its last edit.
    edits: HashMap<String, Vec<(S, i64)>>,
}

impl<S> Default for Tracker<S> {
    fn default() -> Self {
        Self {
            edits: HashMap::new(),
        }
    }
}

impl<S: Copy + Ord> Tracker<S> {
    /// Records that `sessions` edited `path` at `now_ms`. Returns the sorted editors of `path`
    /// within the window when this edit makes a new conflict: at least two editors, one of
    /// them new. Further edits by the same editors stay quiet; once an editor's last edit is
    /// older than the window it counts as new again.
    pub fn edit(&mut self, path: &str, sessions: &[S], now_ms: i64) -> Option<Vec<S>> {
        let editors = self.edits.entry(path.to_owned()).or_default();
        editors.retain(|&(_, at)| in_window(at, now_ms));
        let mut joined = false;
        for &session in sessions {
            match editors.iter_mut().find(|(s, _)| *s == session) {
                Some(editor) => editor.1 = editor.1.max(now_ms),
                None => {
                    editors.push((session, now_ms));
                    joined = true;
                }
            }
        }
        if !joined || editors.len() < 2 {
            return None;
        }
        let mut ids: Vec<S> = editors.iter().map(|&(s, _)| s).collect();
        ids.sort();
        Some(ids)
    }

    /// Forgets edits older than the window (call once per batch to bound memory).
    pub fn prune(&mut self, now_ms: i64) {
        self.edits.retain(|_, editors| {
            editors.retain(|&(_, at)| in_window(at, now_ms));
            !editors.is_empty()
        });
    }

    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

fn in_window(at: i64, now_ms: i64) -> bool {
    now_ms.saturating_sub(at) <= WINDOW_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_sessions_within_the_window_conflict_once() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.edit("src/a.rs", &[2], 1_000), None);
        assert_eq!(tracker.edit("src/a.rs", &[2], 2_000), None);
        assert_eq!(tracker.edit("src/a.rs", &[1], 30_000), Some(vec![1, 2]));
        // Same editors again: no repeat.
        assert_eq!(tracker.edit("src/a.rs", &[1], 31_000), None);
        assert_eq!(tracker.edit("src/a.rs", &[2], 32_000), None);
        // A third session joins.
        assert_eq!(tracker.edit("src/a.rs", &[3], 33_000), Some(vec![1, 2, 3]));
    }

    #[test]
    fn edits_outside_the_window_or_of_other_files_do_not_conflict() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.edit("a", &[1], 0), None);
        assert_eq!(tracker.edit("b", &[2], 10), None);
        assert_eq!(tracker.edit("a", &[2], WINDOW_MS + 1), None);
        // Exactly at the window edge still counts.
        assert_eq!(tracker.edit("b", &[1], WINDOW_MS + 10), Some(vec![1, 2]));
    }

    #[test]
    fn an_expired_editor_counts_as_new_again() {
        let mut tracker = Tracker::default();
        tracker.edit("a", &[1], 0);
        assert_eq!(tracker.edit("a", &[2], 1), Some(vec![1, 2]));
        // 1 stops editing; 2 keeps going until 1 drops out of the window.
        assert_eq!(tracker.edit("a", &[2], WINDOW_MS + 5), None);
        assert_eq!(tracker.edit("a", &[1], WINDOW_MS + 6), Some(vec![1, 2]));
    }

    #[test]
    fn one_change_by_several_sessions_is_a_conflict() {
        let mut tracker = Tracker::default();
        assert_eq!(tracker.edit("a", &[2, 1], 0), Some(vec![1, 2]));
        assert_eq!(tracker.edit("a", &[], 1), None);
    }

    #[test]
    fn prune_forgets_old_files() {
        let mut tracker = Tracker::default();
        tracker.edit("a", &[1], 0);
        tracker.edit("b", &[1], 50_000);
        tracker.prune(WINDOW_MS + 1);
        assert!(!tracker.is_empty());
        tracker.prune(50_000 + WINDOW_MS + 1);
        assert!(tracker.is_empty());
    }
}
