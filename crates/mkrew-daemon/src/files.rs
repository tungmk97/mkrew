//! The Files view's reads and writes (SPEC §12A): `fs.list` (one folder, `.gitignore`-aware,
//! with git markers), `fs.read` (text, binary flag, language, hash), `fs.write` (ADR 0028) and
//! `fs.find` (quick open over a cached file list that the project watcher invalidates).
//! Blocking; the server runs them in `spawn_blocking`.

use std::collections::{HashMap, HashSet};
use std::io::{self, Read as _};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mkrew_protocol::{ChangeKind, FsContent, FsEntry, FsMatch, FsWritten, GitMarker, GitStatus};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use ulid::Ulid;

use crate::docs::{self, MAX_BYTES, ReadError};
use crate::git::GitBackend;
use crate::watch::ALWAYS_IGNORED;

/// A file with a NUL byte in its first this many bytes is binary (like git).
const SNIFF_BYTES: usize = 8000;

/// The files of each project `fs.find` searched, until the watcher reports a change.
#[derive(Default)]
pub struct FileIndex {
    projects: Mutex<HashMap<Ulid, Indexed>>,
}

#[derive(Default)]
struct Indexed {
    /// Bumped by every invalidation, so a walk that raced one is not cached.
    generation: u64,
    files: Option<Arc<Vec<String>>>,
}

impl FileIndex {
    /// Files of `project` changed: walk again next time.
    pub fn invalidate(&self, project: Ulid) {
        let mut projects = lock(&self.projects);
        let indexed = projects.entry(project).or_default();
        indexed.generation += 1;
        indexed.files = None;
    }

    /// The sorted project-relative files under `root` (see [`walk_files`]), cached.
    pub fn files(&self, project: Ulid, root: &Path) -> Arc<Vec<String>> {
        let generation = match lock(&self.projects).get(&project) {
            Some(Indexed {
                files: Some(files), ..
            }) => return Arc::clone(files),
            Some(indexed) => indexed.generation,
            None => 0,
        };
        let files = Arc::new(walk_files(root));
        let mut projects = lock(&self.projects);
        let indexed = projects.entry(project).or_default();
        if indexed.generation == generation {
            indexed.files = Some(Arc::clone(&files));
        }
        files
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Only a cache: whatever a panicking holder left is still valid.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A git-aware walk below `dir`: `.gitignore`, `.git/info/exclude` and the global excludes
/// apply inside a repo; hidden files are kept; `.git`, `node_modules` and `target` are never
/// entered (the watcher does not report them either).
fn walker(dir: &Path) -> ignore::WalkBuilder {
    let mut walk = ignore::WalkBuilder::new(dir);
    walk.hidden(false).filter_entry(|entry| {
        entry.depth() == 0
            || !entry
                .file_name()
                .to_str()
                .is_some_and(|name| ALWAYS_IGNORED.contains(&name))
    });
    walk
}

/// Every not-ignored file under `root`, as sorted `/`-separated relative paths.
pub fn walk_files(root: &Path) -> Vec<String> {
    let mut files: Vec<String> = walker(root)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|t| t.is_file()))
        .filter_map(|entry| relative(root, entry.path()))
        .collect();
    files.sort();
    files
}

fn relative(root: &Path, path: &Path) -> Option<String> {
    let parts: Option<Vec<&str>> = path
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|c| c.as_os_str().to_str())
        .collect();
    Some(parts?.join("/"))
}

/// The marker of each changed file in a working-mode `status` (paths relative to the
/// project): untracked `?`; else `D` if either side deleted it (gone from disk or index);
/// else `A` if it was added; else `M` (modified, renamed, copied, type change, conflict).
pub fn markers(status: &GitStatus) -> HashMap<String, GitMarker> {
    let mut markers = HashMap::new();
    for file in status.staged.iter().chain(&status.unstaged) {
        let marker = match file.kind {
            ChangeKind::Deleted => GitMarker::Deleted,
            ChangeKind::Added => GitMarker::Added,
            _ => GitMarker::Modified,
        };
        let rank = |m: &GitMarker| match m {
            GitMarker::Deleted => 2,
            GitMarker::Added => 1,
            _ => 0,
        };
        let current = markers.entry(file.path.clone()).or_insert(marker);
        if rank(&marker) > rank(current) {
            *current = marker;
        }
    }
    for file in &status.untracked {
        markers.insert(file.path.clone(), GitMarker::Untracked);
    }
    markers
}

/// The direct child folders of `root` that are git repositories of their own (ADR 0029): a
/// `.git` folder or file (worktree, submodule) inside. Sorted; hidden folders and
/// [`ALWAYS_IGNORED`] ones skipped. `.gitignore` is not asked: parents often ignore them.
// ponytail: depth 1 only; walk deeper (apps/web) when a user asks.
pub fn child_repos(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut repos: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.') && !ALWAYS_IGNORED.contains(&name.as_str()))
        .filter(|name| root.join(name).join(".git").exists())
        .collect();
    repos.sort();
    repos
}

/// The index text of the project-relative `path` under `root` (`fs.read` with `git_base`):
/// from the child repository (ADR 0029) the path lies in, else from the project's own.
/// `None` when untracked, outside a repository, or git fails.
pub fn git_base(git: &dyn GitBackend, root: &Path, path: &str) -> Option<String> {
    let (dir, inner) = match path.split_once('/') {
        Some((top, rest)) if child_repos(root).iter().any(|repo| repo == top) => {
            (root.join(top), rest)
        }
        _ => (root.to_path_buf(), path),
    };
    git.index_text(&dir, inner).ok().flatten()
}

/// Names of the not-ignored entries directly in `folder`.
fn visible(folder: &Path) -> HashSet<String> {
    walker(folder)
        .max_depth(Some(1))
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.depth() == 1)
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect()
}

/// The children of folder `dir` (`""` = the root; already checked to stay inside it):
/// folders first, then by name ignoring case. Git-ignored entries (and `node_modules` /
/// `target`) only with `show_ignored`, flagged `ignored`; `.git` never. Deleted files the
/// markers know of are listed too, so their `D` shows.
pub fn list(
    root: &Path,
    dir: &str,
    show_ignored: bool,
    markers: &HashMap<String, GitMarker>,
) -> io::Result<Vec<FsEntry>> {
    let folder = root.join(dir);
    if !folder.is_dir() {
        return Err(io::ErrorKind::NotFound.into());
    }
    let visible = visible(&folder);
    // Below an ignored folder everything is ignored; a walk starting inside it cannot tell.
    let mut parent = root.to_path_buf();
    let mut inside_ignored = false;
    for part in dir.split('/').filter(|p| !p.is_empty()) {
        if !inside_ignored && !self::visible(&parent).contains(part) {
            inside_ignored = true;
        }
        parent.push(part);
    }
    let path_of = |name: &str| match dir {
        "" => name.to_owned(),
        dir => format!("{dir}/{name}"),
    };
    // Every folder holding a changed path, once (not a scan of all markers per entry).
    let changed_dirs: HashSet<&str> = markers
        .keys()
        .flat_map(|p| p.match_indices('/').map(move |(at, _)| &p[..at]))
        .collect();
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&folder)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let ignored = inside_ignored || !visible.contains(&name);
        if name == ".git" || (ignored && !show_ignored) {
            continue;
        }
        // Follows symlinks: a link to a folder opens like one.
        let is_dir = entry.path().is_dir();
        let path = path_of(&name);
        let (marker, has_changes) = if is_dir {
            (None, changed_dirs.contains(path.as_str()))
        } else {
            (markers.get(&path).copied(), false)
        };
        entries.push(FsEntry {
            name,
            path,
            is_dir,
            ignored,
            marker,
            has_changes,
        });
    }
    let listed: HashSet<String> = entries.iter().map(|e| e.path.clone()).collect();
    for (path, _) in markers
        .iter()
        .filter(|(_, marker)| **marker == GitMarker::Deleted)
    {
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        if parent == dir && !listed.contains(path) {
            entries.push(FsEntry {
                name: name.to_owned(),
                path: path.clone(),
                is_dir: false,
                ignored: false,
                marker: Some(GitMarker::Deleted),
                has_changes: false,
            });
        }
    }
    entries.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase(), e.name.clone()));
    Ok(entries)
}

/// The metadata of `file`, which must be an existing regular file.
fn existing_file(file: &Path) -> Result<std::fs::Metadata, ReadError> {
    match file.metadata() {
        Ok(meta) if meta.is_file() => Ok(meta),
        Ok(_) => Err(ReadError::NotFound),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(ReadError::NotFound),
        Err(e) => Err(e.into()),
    }
}

/// `fs.read` of `file` (the project-relative `path`): text up to [`MAX_BYTES`]; larger files
/// and binary ones (a NUL byte near the start, or not UTF-8) come without content.
pub fn read(file: &Path, path: String) -> Result<FsContent, ReadError> {
    let size = existing_file(file)?.len();
    let too_large = size > MAX_BYTES;
    let (binary, content, hash) = if too_large {
        let mut head = Vec::with_capacity(SNIFF_BYTES);
        std::fs::File::open(file)?
            .take(SNIFF_BYTES as u64)
            .read_to_end(&mut head)?;
        (has_nul(&head), None, None)
    } else {
        let bytes = std::fs::read(file)?;
        let hash = docs::hash(&bytes);
        match String::from_utf8(bytes) {
            Ok(text) if !has_nul(text.as_bytes()) => (false, Some(text), Some(hash)),
            _ => (true, None, None),
        }
    };
    Ok(FsContent {
        language: language(&path).map(str::to_owned),
        path,
        content,
        size,
        binary,
        too_large,
        hash,
        git_base: None,
    })
}

/// `fs.write` of `file` (ADR 0028): replaces its text with `content` if its bytes still hash
/// to `expected_hash`; otherwise nothing is written and the disk's hash comes back.
// ponytail: written in place (keeps permissions, symlinks and hard links), not crash-atomic,
// and a writer between the check and the write is overwritten. Write a temp file and rename
// it if torn saves ever show up.
pub fn write(file: &Path, content: &str, expected_hash: &str) -> Result<FsWritten, ReadError> {
    let len = u64::try_from(content.len()).unwrap_or(u64::MAX);
    if len > MAX_BYTES {
        return Err(ReadError::TooLarge(len));
    }
    let size = existing_file(file)?.len();
    if size > MAX_BYTES {
        return Err(ReadError::TooLarge(size));
    }
    // The file may have grown since the check: read at most one byte past the cap.
    let mut bytes = Vec::new();
    std::fs::File::open(file)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(ReadError::TooLarge(bytes.len() as u64));
    }
    let disk = docs::hash(&bytes);
    if disk != expected_hash {
        return Ok(FsWritten {
            saved: false,
            hash: disk,
        });
    }
    std::fs::write(file, content)?;
    Ok(FsWritten {
        saved: true,
        hash: docs::hash(content.as_bytes()),
    })
}

fn has_nul(bytes: &[u8]) -> bool {
    bytes.iter().take(SNIFF_BYTES).any(|&b| b == 0)
}

/// The syntax of `path` guessed from its extension (or a few well-known file names).
pub fn language(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name {
        "Makefile" | "makefile" | "GNUmakefile" => return Some("makefile"),
        "Dockerfile" => return Some("dockerfile"),
        _ => {}
    }
    let (_, ext) = name.rsplit_once('.')?;
    Some(match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "jsx",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "py" => "python",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => "cpp",
        "cs" => "csharp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "rb" => "ruby",
        "php" => "php",
        "lua" => "lua",
        "sh" | "bash" | "zsh" => "shell",
        "ps1" | "psm1" => "powershell",
        "bat" | "cmd" => "batch",
        "sql" => "sql",
        "xml" | "svg" => "xml",
        "ini" | "cfg" => "ini",
        _ => return None,
    })
}

/// The `limit` best fuzzy matches of `query` among `files`, best first (ties: shorter path,
/// then by path); an empty query gives the first `limit` files.
pub fn find(files: &[String], query: &str, limit: usize) -> Vec<FsMatch> {
    if query.trim().is_empty() {
        let first = files.iter().take(limit);
        return first
            .map(|path| FsMatch {
                path: path.clone(),
                score: 0,
                indices: Vec::new(),
            })
            .collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, &String)> = files
        .iter()
        .filter_map(|path| {
            let score = pattern.score(Utf32Str::new(path, &mut buf), &mut matcher)?;
            Some((score, path))
        })
        .collect();
    scored.sort_by(|(a, pa), (b, pb)| b.cmp(a).then(pa.len().cmp(&pb.len())).then(pa.cmp(pb)));
    scored.truncate(limit);
    scored
        .into_iter()
        .map(|(score, path)| {
            let mut indices = Vec::new();
            pattern.indices(Utf32Str::new(path, &mut buf), &mut matcher, &mut indices);
            indices.sort_unstable();
            indices.dedup();
            FsMatch {
                path: path.clone(),
                score,
                indices,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use mkrew_protocol::ChangedFile;

    use super::*;
    use crate::git::{GitBackend, GitCli};

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
    }

    fn write(root: &Path, path: &str, content: &[u8]) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, content).unwrap();
    }

    fn changed(path: &str, kind: ChangeKind) -> ChangedFile {
        ChangedFile {
            path: path.into(),
            old_path: None,
            kind,
            added: None,
            removed: None,
        }
    }

    fn names(entries: &[FsEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn markers_prefer_untracked_then_deleted_then_added() {
        let status = GitStatus {
            staged: vec![
                changed("added.rs", ChangeKind::Added),
                changed("gone.rs", ChangeKind::Added),
                changed("back.rs", ChangeKind::Deleted),
                changed("moved.rs", ChangeKind::Renamed),
            ],
            unstaged: vec![
                changed("added.rs", ChangeKind::Modified),
                changed("gone.rs", ChangeKind::Deleted),
                changed("edit.rs", ChangeKind::Modified),
                changed("both.rs", ChangeKind::Conflicted),
            ],
            untracked: vec![changed("back.rs", ChangeKind::Untracked)],
            ..GitStatus::default()
        };
        let markers = markers(&status);
        let expected = [
            ("added.rs", GitMarker::Added),
            ("gone.rs", GitMarker::Deleted),
            ("back.rs", GitMarker::Untracked),
            ("moved.rs", GitMarker::Modified),
            ("edit.rs", GitMarker::Modified),
            ("both.rs", GitMarker::Modified),
        ];
        assert_eq!(markers.len(), expected.len());
        for (path, marker) in expected {
            assert_eq!(markers[path], marker, "{path}");
        }
    }

    #[test]
    fn child_repos_are_direct_folders_holding_git_sorted() {
        let root = tempfile::tempdir().unwrap();
        let dir = |name: &str| {
            let path = root.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            path
        };
        dir("fe/.git");
        dir("be/.git");
        // A worktree or submodule has a `.git` file.
        std::fs::write(dir("lib").join(".git"), "gitdir: ../x\n").unwrap();
        dir("docs");
        dir(".cache/.git");
        dir("node_modules/pkg/.git");
        dir("target/.git");
        dir("apps/web/.git");
        std::fs::write(root.path().join("file"), "x").unwrap();
        // Hidden, `node_modules`, `target` and depth-2 repos are skipped; a `.git` file counts.
        assert_eq!(child_repos(root.path()), ["be", "fe", "lib"]);
    }

    #[test]
    fn list_respects_gitignore_shows_markers_and_deleted_files() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        git(root, &["init", "-q", "-b", "main"]);
        write(root, ".gitignore", b"build/\n*.log\n");
        write(root, "src/lib.rs", b"one\n");
        write(root, "src/old.rs", b"old\n");
        write(root, "README.md", b"hi\n");
        write(root, "docs/a.md", b"a\n");
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "one"]);
        write(root, "src/lib.rs", b"two\n");
        std::fs::remove_file(root.join("src/old.rs")).unwrap();
        write(root, "src/new.rs", b"new\n");
        write(root, "Zeta.txt", b"z\n");
        write(root, "build/out.bin", b"x\n");
        write(root, "run.log", b"x\n");
        write(root, "node_modules/x/index.js", b"x\n");

        let repo_info = GitCli.repo(root).unwrap().unwrap();
        let markers = markers(&GitCli.status(root, &repo_info).unwrap());

        let top = list(root, "", false, &markers).unwrap();
        assert_eq!(
            names(&top),
            ["docs", "src", ".gitignore", "README.md", "Zeta.txt"]
        );
        let src = top.iter().find(|e| e.name == "src").unwrap();
        assert!(src.is_dir && src.has_changes && src.marker.is_none());
        let docs = top.iter().find(|e| e.name == "docs").unwrap();
        assert!(!docs.has_changes);
        let zeta = top.iter().find(|e| e.name == "Zeta.txt").unwrap();
        assert_eq!(zeta.marker, Some(GitMarker::Untracked));
        assert!(top.iter().all(|e| !e.ignored));

        let all = list(root, "", true, &markers).unwrap();
        let ignored: Vec<&str> = all
            .iter()
            .filter(|e| e.ignored)
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(ignored, ["build", "node_modules", "run.log"]);
        assert!(all.iter().all(|e| e.name != ".git"));
        for dir in ["build", "node_modules", "node_modules/x"] {
            let inside = list(root, dir, true, &markers).unwrap();
            assert!(
                !inside.is_empty() && inside.iter().all(|e| e.ignored),
                "{dir}"
            );
            assert!(
                list(root, dir, false, &markers).unwrap().is_empty(),
                "{dir}"
            );
        }

        let src = list(root, "src", false, &markers).unwrap();
        let marked: Vec<(&str, &str, Option<GitMarker>)> = src
            .iter()
            .map(|e| (e.name.as_str(), e.path.as_str(), e.marker))
            .collect();
        assert_eq!(
            marked,
            [
                ("lib.rs", "src/lib.rs", Some(GitMarker::Modified)),
                ("new.rs", "src/new.rs", Some(GitMarker::Untracked)),
                ("old.rs", "src/old.rs", Some(GitMarker::Deleted)),
            ]
        );

        let err = list(root, "missing", false, &markers).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn read_flags_binary_and_large_files_and_guesses_the_language() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "main.rs", b"fn main() {}\n");
        write(root, "image.png", b"\x89PNG\r\n\x1a\n\0\0\0");
        write(root, "latin1.txt", b"caf\xe9\n");
        let mut big = vec![b'a'; MAX_BYTES as usize + 1];
        big[10] = 0;
        write(root, "big.dat", &big);
        write(root, "big.txt", &vec![b'a'; MAX_BYTES as usize + 1]);

        let text = read(&root.join("main.rs"), "main.rs".into()).unwrap();
        assert_eq!(text.content.as_deref(), Some("fn main() {}\n"));
        assert_eq!((text.size, text.binary, text.too_large), (13, false, false));
        assert_eq!(text.language.as_deref(), Some("rust"));

        let png = read(&root.join("image.png"), "image.png".into()).unwrap();
        assert_eq!(
            (png.content, png.binary, png.too_large),
            (None, true, false)
        );
        let latin = read(&root.join("latin1.txt"), "latin1.txt".into()).unwrap();
        assert!(latin.binary && latin.content.is_none());

        let big = read(&root.join("big.dat"), "big.dat".into()).unwrap();
        assert_eq!((big.content, big.binary, big.too_large), (None, true, true));
        assert_eq!(big.size, MAX_BYTES + 1);
        let big = read(&root.join("big.txt"), "big.txt".into()).unwrap();
        assert_eq!(
            (big.content, big.binary, big.too_large),
            (None, false, true)
        );

        assert!(matches!(
            read(&root.join("nope.rs"), "nope.rs".into()),
            Err(ReadError::NotFound)
        ));
        assert!(matches!(read(root, "".into()), Err(ReadError::NotFound)));
    }

    #[test]
    fn write_saves_only_over_the_version_that_was_read() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.txt", b"one\r\n");
        let read_hash = read(&root.join("a.txt"), "a.txt".into())
            .unwrap()
            .hash
            .unwrap();
        assert_eq!(read_hash, crate::docs::hash(b"one\r\n"));

        let saved = super::write(&root.join("a.txt"), "two\r\n", &read_hash).unwrap();
        assert_eq!(
            (saved.saved, saved.hash.clone()),
            (true, crate::docs::hash(b"two\r\n"))
        );
        assert_eq!(std::fs::read(root.join("a.txt")).unwrap(), b"two\r\n");

        // Someone wrote meanwhile: nothing is written; the disk's hash comes back.
        std::fs::write(root.join("a.txt"), "agent\n").unwrap();
        let stale = super::write(&root.join("a.txt"), "mine\n", &saved.hash).unwrap();
        assert_eq!(
            (stale.saved, stale.hash.clone()),
            (false, crate::docs::hash(b"agent\n"))
        );
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "agent\n"
        );
        // "Keep mine": the disk's hash overwrites that version.
        assert!(
            super::write(&root.join("a.txt"), "mine\n", &stale.hash)
                .unwrap()
                .saved
        );
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "mine\n"
        );

        // Only existing files, at most MAX_BYTES of text.
        assert!(matches!(
            super::write(&root.join("new.txt"), "x", "h"),
            Err(ReadError::NotFound)
        ));
        assert!(!root.join("new.txt").exists());
        assert!(matches!(
            super::write(root, "x", "h"),
            Err(ReadError::NotFound)
        ));
        let big = "a".repeat(MAX_BYTES as usize + 1);
        assert!(matches!(
            super::write(&root.join("a.txt"), &big, &crate::docs::hash(b"mine\n")),
            Err(ReadError::TooLarge(_))
        ));
        assert_eq!(
            std::fs::read_to_string(root.join("a.txt")).unwrap(),
            "mine\n"
        );
        // A big file on disk is refused before it is read or hashed.
        write(root, "huge.txt", &vec![b'a'; MAX_BYTES as usize + 1]);
        assert!(matches!(
            super::write(&root.join("huge.txt"), "x", "h"),
            Err(ReadError::TooLarge(_))
        ));
    }

    #[test]
    fn languages_come_from_extensions_and_well_known_names() {
        assert_eq!(language("a/b/c.TSX"), Some("tsx"));
        assert_eq!(language("x/Makefile"), Some("makefile"));
        assert_eq!(language("Dockerfile"), Some("dockerfile"));
        assert_eq!(language("notes.unknown"), None);
        assert_eq!(language("LICENSE"), None);
    }

    #[test]
    fn find_ranks_fuzzy_matches_with_char_indices() {
        let files: Vec<String> = [
            "crates/mkrew-daemon/src/files.rs",
            "crates/mkrew-daemon/src/server/fs.rs",
            "docs/SPEC.md",
            "README.md",
        ]
        .map(String::from)
        .to_vec();
        let found = find(&files, "spec", 10);
        assert_eq!(found[0].path, "docs/SPEC.md");
        assert_eq!(found[0].indices, [5, 6, 7, 8]);
        assert!(found.iter().all(|m| m.score > 0));

        let found = find(&files, "daemon fs", 10);
        let paths: Vec<&str> = found.iter().map(|m| m.path.as_str()).collect();
        assert!(paths.contains(&"crates/mkrew-daemon/src/server/fs.rs"));
        assert!(!paths.contains(&"README.md"));

        assert!(find(&files, "zzz", 10).is_empty());
        let first = find(&files, "  ", 2);
        assert_eq!(first.len(), 2);
        assert_eq!(first[0].path, files[0]);
        assert_eq!(find(&files, "rs", 1).len(), 1);
    }

    #[test]
    fn the_file_index_walks_again_only_after_an_invalidation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, "a.rs", b"a");
        write(root, ".hidden/b.rs", b"b");
        write(root, "target/debug/c.rs", b"c");
        let index = FileIndex::default();
        let project = Ulid::new();
        assert_eq!(*index.files(project, root), [".hidden/b.rs", "a.rs"]);
        write(root, "d.rs", b"d");
        assert_eq!(index.files(project, root).len(), 2, "cached");
        index.invalidate(project);
        assert_eq!(index.files(project, root).len(), 3);
    }

    #[test]
    fn find_over_10000_files_meets_the_budget() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for i in 0..10_000 {
            write(
                root,
                &format!("pkg{}/mod{}/file_{i}.rs", i % 20, i % 7),
                b"",
            );
        }
        let index = FileIndex::default();
        let project = Ulid::new();
        let started = std::time::Instant::now();
        assert_eq!(index.files(project, root).len(), 10_000);
        let walk = started.elapsed();
        let search = || {
            let started = std::time::Instant::now();
            let found = find(&index.files(project, root), "pkg3 mod5 file_9", 50);
            (started.elapsed(), found)
        };
        let (_, found) = search();
        assert_eq!(found.len(), 50);
        let mut times: Vec<_> = (0..5).map(|_| search().0).collect();
        times.sort();
        let median = times[2];
        eprintln!("fs.find on 10,000 files: median {median:?} of {times:?} (first walk {walk:?})");
        if !cfg!(debug_assertions) {
            assert!(median < std::time::Duration::from_millis(100), "{median:?}");
        }
    }
}
