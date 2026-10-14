//! Markdown review files (SPEC §12): which files `doc.list` shows, reading one, and how its
//! comments move after it changed. Pure apart from [`markdown_files`] and [`read`].

use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hasher};
use std::path::{Component, Path, PathBuf};

use mkrew_core::review::{self, Anchor, ReviewGlobs};
use mkrew_protocol::{CommentContext, CommentId, CommentStatus, DocComment, DocFile};
use mkrew_store::SessionEvent;
use ulid::Ulid;

use crate::watch::ALWAYS_IGNORED;

/// Larger files are not read (same cap as `fs.read`, SPEC §7).
pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// Files the watcher saw change this recently are listed even when no review glob matches.
pub const RECENT_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("no such file")]
    NotFound,
    #[error("the file has {0} bytes; at most {MAX_BYTES} are read")]
    TooLarge(u64),
    #[error("the file is not UTF-8 text")]
    NotUtf8,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// `.md` in any case.
pub fn is_markdown(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
}

/// Opaque content hash. `DefaultHasher` uses fixed keys, so it is stable for one daemon build,
/// which is all `doc.changed` needs.
pub fn hash(content: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(content);
    format!("{:016x}", hasher.finish())
}

/// A client-supplied project-relative path with `/` separators, and the same path for joining
/// onto the root; `None` if it is empty, absolute or steps outside (`..`).
pub fn relative_path(file_path: &str) -> Option<(String, PathBuf)> {
    let normalized = file_path.trim().replace('\\', "/");
    let path = PathBuf::from(&normalized);
    let mut components = path.components().peekable();
    components.peek()?;
    components
        .all(|c| matches!(c, Component::Normal(_)))
        .then_some((normalized, path))
}

/// The file's text.
pub fn read(path: &Path) -> Result<String, ReadError> {
    let meta = match path.metadata() {
        Ok(meta) if meta.is_file() => meta,
        Ok(_) => return Err(ReadError::NotFound),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(ReadError::NotFound),
        Err(e) => return Err(e.into()),
    };
    if meta.len() > MAX_BYTES {
        return Err(ReadError::TooLarge(meta.len()));
    }
    String::from_utf8(std::fs::read(path)?).map_err(|_| ReadError::NotUtf8)
}

/// Markdown files under `root` matching `globs`, as sorted project-relative paths. Git-ignored
/// files count (plans often live in ignored folders); `.git`, `node_modules` and `target`
/// are never entered.
pub fn markdown_files(root: &Path, globs: &ReviewGlobs) -> Vec<String> {
    let walk = ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || !entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| ALWAYS_IGNORED.contains(&name))
        })
        .build();
    let mut files: Vec<String> = walk
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|entry| {
            let relative = entry.path().strip_prefix(root).ok()?;
            let parts: Option<Vec<&str>> = relative
                .components()
                .map(|c| c.as_os_str().to_str())
                .collect();
            Some(parts?.join("/"))
        })
        .filter(|path| is_markdown(path) && globs.matches(path))
        .collect();
    files.sort();
    files
}

/// The `doc.list` result: the `listed` (glob-matched) files, plus Markdown files in recent
/// `files_changed` events (`changed`, oldest first) and files with unresolved comments, while
/// they still `exist`. The last change of a file is the newest event naming it; events of
/// several sessions at the same time go to the most recently started one (the greatest ULID):
/// a new batch is stored once, on the newest live session (ADR 0060), and this `(at, session)`
/// tie-break still covers rows written before.
pub fn doc_files(
    listed: Vec<String>,
    changed: &[(Ulid, SessionEvent)],
    comments: &[DocComment],
    exist: impl Fn(&str) -> bool,
) -> Vec<DocFile> {
    let mut files: BTreeMap<String, DocFile> = BTreeMap::new();
    for path in listed {
        entry(&mut files, &path);
    }
    let mut last: HashMap<&str, (i64, Ulid)> = HashMap::new();
    for (session, event) in changed {
        let paths = event.payload["paths"].as_array().into_iter().flatten();
        for path in paths.filter_map(|p| p.as_str()).filter(|p| is_markdown(p)) {
            let change = (event.at, *session);
            let newest = last.entry(path).or_insert(change);
            *newest = change.max(*newest);
        }
    }
    for (path, (at, session)) in last {
        if files.contains_key(path) || exist(path) {
            let file = entry(&mut files, path);
            file.last_session_id = Some(session);
            file.last_changed_at = Some(at);
        }
    }
    // Diff comments (SPEC §12B) belong to the Git view.
    let open = comments.iter().filter(|c| {
        c.parent_id.is_none()
            && c.context == CommentContext::Doc
            && c.status != CommentStatus::Resolved
    });
    for comment in open {
        if files.contains_key(&comment.file_path) || exist(&comment.file_path) {
            entry(&mut files, &comment.file_path).open_comments += 1;
        }
    }
    files.into_values().collect()
}

fn entry<'a>(files: &'a mut BTreeMap<String, DocFile>, path: &str) -> &'a mut DocFile {
    files.entry(path.to_owned()).or_insert_with(|| DocFile {
        file_path: path.to_owned(),
        open_comments: 0,
        last_session_id: None,
        last_changed_at: None,
    })
}

/// A thread root's new anchor: id, first and last line, status.
pub type Moved = (CommentId, u32, u32, CommentStatus);

/// The thread roots among `comments` (all of one file) whose anchor or status changes now
/// that the file holds `content` (`None` = deleted). A passage that is gone makes the comment
/// `outdated` (lines kept); one that is back makes an `outdated` comment `open` again.
/// Resolved comments only follow their passage, they never change status.
pub fn reanchored(comments: &[DocComment], content: Option<&str>) -> Vec<Moved> {
    comments
        .iter()
        .filter(|c| c.parent_id.is_none())
        .filter_map(|c| {
            let (start, end) = (c.start_line?, c.end_line?);
            let quote = c.quote.as_deref().unwrap_or("");
            let anchor = content.map_or(Anchor::Outdated, |text| {
                review::reanchor(text, start, end, quote)
            });
            let moved = match (anchor, c.status) {
                (Anchor::Lines(s, e), CommentStatus::Outdated) => (s, e, CommentStatus::Open),
                (Anchor::Lines(s, e), status) => (s, e, status),
                (Anchor::Outdated, CommentStatus::Resolved) => (start, end, c.status),
                (Anchor::Outdated, _) => (start, end, CommentStatus::Outdated),
            };
            (moved != (start, end, c.status)).then_some((c.id, moved.0, moved.1, moved.2))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comment(
        file: &str,
        lines: (u32, u32),
        quote: &str,
        status: CommentStatus,
        parent: Option<CommentId>,
    ) -> DocComment {
        DocComment {
            id: CommentId::new(),
            project_id: Ulid::nil(),
            file_path: file.into(),
            context: Default::default(),
            parent_id: parent,
            author: DocComment::USER.into(),
            body: "b".into(),
            start_line: parent.is_none().then_some(lines.0),
            end_line: parent.is_none().then_some(lines.1),
            quote: parent.is_none().then(|| quote.into()),
            status,
            sent_to: None,
            sent_snapshot: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn changed(session: Ulid, at: i64, paths: &[&str]) -> (Ulid, SessionEvent) {
        let payload = serde_json::json!({ "paths": paths });
        (session, SessionEvent { payload, at })
    }

    #[test]
    fn markdown_and_relative_paths() {
        assert!(is_markdown("docs/a.md") && is_markdown("README.MD"));
        assert!(!is_markdown("a.mdx") && !is_markdown("md") && !is_markdown("a.md/x.rs"));
        assert_eq!(
            relative_path(r" docs\plan.md "),
            Some(("docs/plan.md".into(), PathBuf::from("docs/plan.md")))
        );
        for bad in [
            "",
            "  ",
            "../x.md",
            "docs/../../x.md",
            "/etc/x.md",
            "./a.md",
        ] {
            assert_eq!(relative_path(bad), None, "{bad}");
        }
        #[cfg(windows)]
        assert_eq!(relative_path(r"C:\x.md"), None);
        assert_ne!(hash(b"a"), hash(b"b"));
        assert_eq!(hash(b"a"), hash(b"a"));
    }

    #[test]
    fn walk_finds_globbed_markdown_and_skips_fixed_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for file in [
            "README.md",
            "notes.txt",
            "src/inner.md",
            "docs/a.md",
            "docs/deep/b.md",
            // Globs are case-sensitive.
            "docs/upper.MD",
            ".claude/plans/p.md",
            "node_modules/docs/x.md",
            "docs/target/t.md",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        // Git-ignored files are still reviewable.
        std::fs::write(root.join(".gitignore"), ".claude/\n").unwrap();
        let globs = ReviewGlobs::for_project(None).unwrap();
        assert_eq!(
            markdown_files(root, &globs),
            [
                ".claude/plans/p.md",
                "README.md",
                "docs/a.md",
                "docs/deep/b.md"
            ]
        );
        let custom = ReviewGlobs::new(&["src/*.md"]).unwrap();
        assert_eq!(markdown_files(root, &custom), ["src/inner.md"]);
    }

    #[test]
    fn doc_files_merge_globs_recent_changes_and_comments() {
        let (old, new) = (Ulid::from_parts(1, 0), Ulid::from_parts(2, 0));
        let listed = vec!["README.md".to_owned(), "docs/a.md".to_owned()];
        let changed = [
            changed(old, 10, &["docs/a.md", "src/lib.rs", "notes/plan.md"]),
            changed(new, 10, &["notes/plan.md"]),
            changed(old, 20, &["docs/a.md"]),
            changed(old, 30, &["gone.md"]),
        ];
        let root = comment("docs/a.md", (1, 1), "x", CommentStatus::Open, None);
        let diff = |file: &str| DocComment {
            context: CommentContext::Diff,
            ..comment(file, (1, 1), "q", CommentStatus::Open, None)
        };
        let comments = [
            // Diff comments neither count nor list their file.
            diff("docs/a.md"),
            diff("src/lib.rs"),
            root.clone(),
            comment("docs/a.md", (1, 1), "", CommentStatus::Open, Some(root.id)),
            comment("docs/a.md", (2, 2), "y", CommentStatus::Outdated, None),
            comment("docs/a.md", (3, 3), "z", CommentStatus::Resolved, None),
            comment("src/x.md", (1, 1), "q", CommentStatus::Addressed, None),
            comment("deleted.md", (1, 1), "q", CommentStatus::Open, None),
        ];
        let exist = |p: &str| p != "gone.md" && p != "deleted.md";
        let files = doc_files(listed, &changed, &comments, exist);
        let got: Vec<(&str, u32, Option<Ulid>, Option<i64>)> = files
            .iter()
            .map(|f| {
                let path = f.file_path.as_str();
                (path, f.open_comments, f.last_session_id, f.last_changed_at)
            })
            .collect();
        assert_eq!(
            got,
            [
                ("README.md", 0, None, None),
                ("docs/a.md", 2, Some(old), Some(20)),
                // Same batch on two sessions: the later-started one.
                ("notes/plan.md", 0, Some(new), Some(10)),
                ("src/x.md", 1, None, None),
            ]
        );
    }

    #[test]
    fn reanchored_moves_outdates_and_revives_comments() {
        let content = "# Plan\nnew line\nUse Redis\nM3: deploy\n";
        let moved = comment("p.md", (2, 2), "Use Redis", CommentStatus::Sent, None);
        let still = comment("p.md", (4, 4), "M3: deploy", CommentStatus::Open, None);
        let gone = comment("p.md", (5, 5), "Postgres", CommentStatus::Addressed, None);
        let back = comment("p.md", (9, 9), "# Plan", CommentStatus::Outdated, None);
        let resolved_gone = comment("p.md", (7, 7), "nope", CommentStatus::Resolved, None);
        let reply = comment("p.md", (1, 1), "", CommentStatus::Open, Some(moved.id));
        let comments = [
            moved.clone(),
            still,
            gone.clone(),
            back.clone(),
            resolved_gone,
            reply,
        ];
        assert_eq!(
            reanchored(&comments, Some(content)),
            [
                (moved.id, 3, 3, CommentStatus::Sent),
                (gone.id, 5, 5, CommentStatus::Outdated),
                (back.id, 1, 1, CommentStatus::Open),
            ]
        );
        // A deleted file outdates every unresolved comment.
        let outdated: Vec<CommentId> = reanchored(&comments, None)
            .into_iter()
            .map(|m| m.0)
            .collect();
        assert_eq!(outdated, [comments[0].id, comments[1].id, comments[2].id]);
    }
}
