//! Pure parts of the four-edge drops (spec 6.2, 6.3): the half of a pane a drop fills, and which
//! OS files may open in Mkrew.

use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

use crate::split::{Edge, Rect};

/// The toast of an OS drop that held files outside the workspace and the notes folder.
pub const REFUSED_OS_FILES: &str =
    "Only files inside the workspace or the notes folder open in Mkrew";

/// The half of a pane, as fractions of the pane, on `edge`'s side: what the drop overlay tints
/// and where the dropped leaf will sit.
pub fn edge_half(edge: Edge) -> Rect {
    let (x, y, w, h) = match edge {
        Edge::Left => (0.0, 0.0, 0.5, 1.0),
        Edge::Right => (0.5, 0.0, 0.5, 1.0),
        Edge::Top => (0.0, 0.0, 1.0, 0.5),
        Edge::Bottom => (0.0, 0.5, 1.0, 0.5),
    };
    Rect { x, y, w, h }
}

/// What an OS file drop may open.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct OsFiles {
    /// Layout paths (the `PaneKey::File` string) of the files that open, in drop order.
    pub open: Vec<String>,
    /// How many files were outside the workspace root and the notes folder.
    pub refused: usize,
}

/// Sorts dropped OS `files` (folders already removed by the caller) into those that open and
/// those that do not.
/// A file inside `root` opens by its root-relative path with `/` separators; a file inside the
/// notes folder (`notes`, the absolute folder the daemon accepts, `None` while there is none)
/// but outside the root opens by its absolute path with `/` separators, as the daemon names a
/// new note outside the root.
/// Relative paths and paths with `..` never open: the daemon refuses them too.
pub fn classify_os_files(root: &Path, notes: Option<&Path>, files: &[PathBuf]) -> OsFiles {
    let mut found = OsFiles::default();
    for file in files {
        match leaf_path(root, notes, file) {
            Some(path) => found.open.push(path),
            None => found.refused += 1,
        }
    }
    found
}

fn leaf_path(root: &Path, notes: Option<&Path>, file: &Path) -> Option<String> {
    if !file.is_absolute() || file.components().any(|part| part == Component::ParentDir) {
        return None;
    }
    // The root and its parents are folders, whatever the notes folder holds.
    if relative_to(file, root).is_some() || same_path(root, file) {
        return None;
    }
    if let Some(inside) = relative_to(root, file) {
        return Some(slashed(&inside));
    }
    notes
        .and_then(|notes| relative_to(notes, file))
        .map(|_| slashed(file))
}

/// `path` below `base`, component by component (so `proj2` is not below `proj`); `None` for
/// `base` itself and for anything outside it. Windows compares names without ASCII case.
fn relative_to(base: &Path, path: &Path) -> Option<PathBuf> {
    let base: Vec<Component<'_>> = base.components().collect();
    let path: Vec<Component<'_>> = path.components().collect();
    let below = path.len() > base.len()
        && base
            .iter()
            .zip(&path)
            .all(|(a, b)| same(a.as_os_str(), b.as_os_str()));
    below.then(|| path[base.len()..].iter().collect())
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.components().count() == b.components().count()
        && (a.components().zip(b.components())).all(|(a, b)| same(a.as_os_str(), b.as_os_str()))
}

fn same(a: &OsStr, b: &OsStr) -> bool {
    if cfg!(windows) {
        a.eq_ignore_ascii_case(b)
    } else {
        a == b
    }
}

fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::split::{Edge, Rect};

    const EDGES: [Edge; 4] = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom];

    fn base() -> PathBuf {
        std::env::temp_dir().join("mkrew-drop-zone")
    }

    fn slashed(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    fn contains(rect: Rect, x: f64, y: f64) -> bool {
        rect.x <= x && x <= rect.x + rect.w && rect.y <= y && y <= rect.y + rect.h
    }

    #[test]
    fn each_edge_fills_the_half_on_its_side() {
        let half = |x, y, w, h| Rect { x, y, w, h };
        assert_eq!(edge_half(Edge::Left), half(0.0, 0.0, 0.5, 1.0));
        assert_eq!(edge_half(Edge::Right), half(0.5, 0.0, 0.5, 1.0));
        assert_eq!(edge_half(Edge::Top), half(0.0, 0.0, 1.0, 0.5));
        assert_eq!(edge_half(Edge::Bottom), half(0.0, 0.5, 1.0, 0.5));
    }

    #[test]
    fn the_half_always_holds_the_pointer_that_picked_its_edge() {
        for i in 0..=10 {
            for j in 0..=10 {
                let (x, y) = (i as f32 / 10.0, j as f32 / 10.0);
                let half = edge_half(Edge::nearest(x, y));
                assert!(
                    contains(half, f64::from(x), f64::from(y)),
                    "({x}, {y}) is outside {half:?}"
                );
            }
        }
    }

    #[test]
    fn the_four_halves_are_distinct_and_cover_half_the_pane_each() {
        for (at, edge) in EDGES.iter().enumerate() {
            let half = edge_half(*edge);
            assert!((half.w * half.h - 0.5).abs() < f64::EPSILON);
            for other in &EDGES[at + 1..] {
                assert_ne!(half, edge_half(*other));
            }
        }
    }

    #[test]
    fn files_inside_the_root_open_by_their_root_relative_slashed_path() {
        let root = base().join("proj");
        let files = [root.join("src").join("a.rs"), root.join("README.md")];
        let found = classify_os_files(&root, None, &files);
        assert_eq!(found.open, ["src/a.rs", "README.md"]);
        assert_eq!(found.refused, 0);
    }

    #[test]
    fn files_outside_the_root_and_the_notes_folder_are_refused_and_counted() {
        let root = base().join("proj");
        let files = [
            root.join("a.rs"),
            base().join("other").join("b.txt"),
            base().join("elsewhere").join("c.txt"),
        ];
        let found = classify_os_files(&root, None, &files);
        assert_eq!(found.open, ["a.rs"]);
        assert_eq!(found.refused, 2);
    }

    #[test]
    fn a_path_that_only_shares_a_name_prefix_with_the_root_is_outside() {
        let root = base().join("proj");
        let found = classify_os_files(&root, None, &[base().join("proj2").join("a.rs")]);
        assert!(found.open.is_empty());
        assert_eq!(found.refused, 1);
    }

    #[test]
    fn a_file_in_the_notes_folder_outside_the_root_keeps_its_absolute_path() {
        let root = base().join("proj");
        let notes = base().join("notes");
        let note = notes.join("sub").join("n.md");
        let found = classify_os_files(&root, Some(&notes), std::slice::from_ref(&note));
        assert_eq!(found.open, [slashed(&note)]);
        assert_eq!(found.refused, 0);
    }

    #[test]
    fn the_root_wins_when_the_notes_folder_is_inside_it() {
        let root = base().join("proj");
        let notes = root.join("notes");
        let found = classify_os_files(&root, Some(&notes), &[notes.join("n.md")]);
        assert_eq!(found.open, ["notes/n.md"]);
    }

    #[test]
    fn a_folder_path_itself_and_relative_or_dotdot_paths_are_refused() {
        let root = base().join("proj");
        let files = [
            root.clone(),
            PathBuf::from("src/a.rs"),
            root.join("src").join("..").join("..").join("secret.txt"),
        ];
        let found = classify_os_files(&root, Some(&base()), &files);
        assert!(found.open.is_empty());
        assert_eq!(found.refused, 3);
    }

    #[cfg(windows)]
    #[test]
    fn windows_paths_match_the_root_without_case() {
        let root = Path::new(r"C:\Work\Proj");
        let found = classify_os_files(root, None, &[PathBuf::from(r"c:\work\proj\Src\A.rs")]);
        assert_eq!(found.open, ["Src/A.rs"]);
    }
}
