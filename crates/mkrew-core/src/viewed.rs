//! "Mark as viewed" in the Git view (SPEC §12B, ADR 0065): the pure rules. The daemon hashes
//! the text of a file's diff when the user marks it; the mark stands only while the file's
//! diff still hashes the same. The storage is `mkrew-store`, the git reads are the daemon's.

use std::collections::BTreeMap;

use mkrew_protocol::GitMode;

/// One stored mark: the file (relative to its repository) and the hash of its diff text when
/// it was marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub path: String,
    pub hash: String,
}

/// A mark that still stands, on the staged (index) side or not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standing {
    pub path: String,
    pub staged: bool,
}

/// What [`resolve`] found: the files whose mark stands (project-relative once the caller
/// prefixes the repository) and the marks to delete.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Resolved {
    pub viewed: Vec<Standing>,
    pub stale: Vec<Mark>,
}

/// FNV-1a 64 of the text `git diff` printed for a file, as 16 hex digits; `None` for an empty
/// diff (nothing to mark). Hand-written so the stored value never changes with a Rust or
/// dependency upgrade.
pub fn text_hash(diff_text: &str) -> Option<String> {
    if diff_text.is_empty() {
        return None;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in diff_text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    Some(format!("{hash:016x}"))
}

/// The list a mark belongs to: `working` (Staged, Unstaged and Untracked share it), `base`
/// (Against base), or `since:<checkpoint>` (a task's or turn's changes). Marks of one list are
/// never pruned by looking at another.
pub fn list_key(mode: GitMode, since: Option<&str>) -> String {
    match (mode, since) {
        (GitMode::Working, _) => "working".to_owned(),
        (GitMode::Base, None) => "base".to_owned(),
        (GitMode::Base, Some(since)) => format!("since:{since}"),
    }
}

/// Which `marks` of one list still stand. `sides` are the diffs a file can have in that list
/// (`[false, true]` = unstaged then staged for Working changes, `[false]` otherwise) and
/// `now(path, staged)` the hash of that side's diff text now (`None` = no diff). A mark stands
/// on every side that hashes the same (so staging a viewed file keeps it viewed); a mark no
/// side explains is stale, which also covers a file that no longer changes. Sides are read in
/// order and only while a mark of the path is still unexplained. A failing `now` fails the
/// whole check: nothing may be deleted on a git error.
pub fn resolve<E>(
    marks: Vec<Mark>,
    sides: &[bool],
    mut now: impl FnMut(&str, bool) -> Result<Option<String>, E>,
) -> Result<Resolved, E> {
    let mut by_path: BTreeMap<String, Vec<Mark>> = BTreeMap::new();
    for mark in marks {
        by_path.entry(mark.path.clone()).or_default().push(mark);
    }
    let mut resolved = Resolved::default();
    for (path, mut pending) in by_path {
        for &staged in sides {
            if pending.is_empty() {
                break;
            }
            let hash = now(&path, staged)?;
            pending.retain(|mark| {
                let stands = hash.as_deref() == Some(mark.hash.as_str());
                if stands {
                    resolved.viewed.push(Standing {
                        path: path.clone(),
                        staged,
                    });
                }
                !stands
            });
        }
        resolved.stale.extend(pending);
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mark(path: &str, hash: &str) -> Mark {
        Mark {
            path: path.into(),
            hash: hash.into(),
        }
    }

    fn file(path: &str, staged: bool) -> Standing {
        Standing {
            path: path.into(),
            staged,
        }
    }

    /// `now` answering from a table of (path, staged, hash).
    fn table<'a>(
        rows: &'a [(&'a str, bool, Option<&'a str>)],
    ) -> impl FnMut(&str, bool) -> Result<Option<String>, String> + 'a {
        move |path, staged| {
            Ok(rows
                .iter()
                .find(|(p, s, _)| *p == path && *s == staged)
                .and_then(|(_, _, hash)| hash.map(str::to_owned)))
        }
    }

    #[test]
    fn text_hash_is_fnv1a_64_and_none_for_nothing() {
        // The published FNV-1a 64 test vectors.
        assert_eq!(text_hash("a").as_deref(), Some("af63dc4c8601ec8c"));
        assert_eq!(text_hash("foobar").as_deref(), Some("85944171f73967e8"));
        // Any byte changes it; an empty diff has no hash (nothing to mark).
        assert_ne!(text_hash("+x\n"), text_hash("+y\n"));
        assert_eq!(text_hash(""), None);
    }

    #[test]
    fn list_keys_tell_the_lists_apart() {
        assert_eq!(list_key(GitMode::Working, None), "working");
        // `since` is Against-base only; Working ignores it, like `git.status`.
        assert_eq!(list_key(GitMode::Working, Some("abc")), "working");
        assert_eq!(list_key(GitMode::Base, None), "base");
        assert_eq!(list_key(GitMode::Base, Some("abc")), "since:abc");
    }

    #[test]
    fn a_mark_stands_while_its_hash_is_a_current_hash_and_goes_stale_otherwise() {
        let now = [
            ("a.rs", false, Some("h1")),
            ("a.rs", true, Some("h2")),
            ("b.rs", false, Some("new")),
            ("c.rs", false, None),
        ];
        let marks = vec![
            mark("a.rs", "h1"),
            mark("a.rs", "h2"),
            mark("b.rs", "old"),
            mark("c.rs", "h3"),
            mark("gone.rs", "h4"),
        ];
        let resolved = resolve(marks, &[false, true], table(&now)).unwrap();
        // a.rs is marked on both its sides; b.rs was edited, c.rs no longer changes, gone.rs
        // is not listed at all.
        assert_eq!(
            resolved.viewed,
            vec![file("a.rs", false), file("a.rs", true)]
        );
        assert_eq!(
            resolved.stale,
            vec![
                mark("b.rs", "old"),
                mark("c.rs", "h3"),
                mark("gone.rs", "h4")
            ]
        );
    }

    #[test]
    fn staging_a_viewed_file_moves_its_mark_to_the_staged_side() {
        // Marked unstaged with hash h1; now the unstaged diff is empty and the index holds h1.
        let now = [("a.rs", false, None), ("a.rs", true, Some("h1"))];
        let resolved = resolve(vec![mark("a.rs", "h1")], &[false, true], table(&now)).unwrap();
        assert_eq!(resolved.viewed, vec![file("a.rs", true)]);
        assert!(resolved.stale.is_empty());
    }

    #[test]
    fn the_staged_side_is_only_read_for_marks_the_unstaged_side_did_not_explain() {
        let mut asked = Vec::new();
        let resolved = resolve(vec![mark("a.rs", "h1")], &[false, true], |path, staged| {
            asked.push((path.to_owned(), staged));
            Ok::<_, ()>(Some("h1".to_owned()))
        })
        .unwrap();
        assert_eq!(resolved.viewed, vec![file("a.rs", false)]);
        assert_eq!(asked, [("a.rs".to_owned(), false)]);
        // One list side only (Against base): the staged side is never read.
        let mut asked = Vec::new();
        resolve(vec![mark("a.rs", "zzz")], &[false], |path, staged| {
            asked.push((path.to_owned(), staged));
            Ok::<_, ()>(None)
        })
        .unwrap();
        assert_eq!(asked, [("a.rs".to_owned(), false)]);
    }

    #[test]
    fn a_failing_read_fails_the_whole_check_so_nothing_is_deleted() {
        let result = resolve(
            vec![mark("a.rs", "h1"), mark("b.rs", "h2")],
            &[false],
            |path, _| {
                if path == "b.rs" {
                    Err("git failed")
                } else {
                    Ok(None)
                }
            },
        );
        assert_eq!(result, Err("git failed"));
    }
}
