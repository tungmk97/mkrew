//! Project file watcher (SPEC §8, M5): one recursive `notify` watcher per project with live
//! sessions. Changed paths pass [`Filter`] and are delivered in debounced batches.
//!
//! [`Filter`] is pure (apart from reading `.gitignore` files on first use) and unit-tested; the
//! watcher itself is tested against a temp dir.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use ::notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use ignore::gitignore::Gitignore;
use tokio::sync::mpsc;

/// A batch is delivered once no change arrived for this long.
pub const QUIET: Duration = Duration::from_millis(300);
/// A batch is delivered after this long even while changes keep coming.
pub const MAX_DELAY: Duration = Duration::from_secs(2);
/// Directories never reported, whatever `.gitignore` says.
pub const ALWAYS_IGNORED: [&str; 3] = [".git", "node_modules", "target"];

/// Never reported by the watcher (Mkrew and users put git worktrees there). The file tree
/// still shows it, so it is not in [`ALWAYS_IGNORED`].
const WATCH_IGNORED: [&str; 1] = [".worktrees"];

/// Which changed paths of one project are reported.
// ponytail: the global `core.excludesFile` is not read (machine config would leak into
// tests); add it via `GitignoreBuilder` if users report noise it would remove.
pub struct Filter {
    root: PathBuf,
    /// The root `.gitignore` plus `.git/info/exclude` (`.gitignore` lines win, added last).
    gitignore: Gitignore,
    /// Nested `.gitignore` files by folder relative to the root, read on first use. Never
    /// re-read, like the root one: the watcher restarts with its project.
    nested: std::sync::Mutex<std::collections::HashMap<PathBuf, Gitignore>>,
}

impl Filter {
    /// Reads `<root>/.git/info/exclude` and `<root>/.gitignore` (missing or partly invalid
    /// files ignore nothing / the bad lines).
    pub fn new(root: &Path) -> Self {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
        for file in [root.join(".git/info/exclude"), root.join(".gitignore")] {
            if let Some(e) = builder.add(&file).filter(|_| file.exists()) {
                tracing::debug!(file = %file.display(), "partly invalid ignore file: {e}");
            }
        }
        let gitignore = builder.build().unwrap_or_else(|e| {
            tracing::debug!(root = %root.display(), "ignore rules dropped: {e}");
            Gitignore::empty()
        });
        Self {
            root: root.to_path_buf(),
            gitignore,
            nested: Default::default(),
        }
    }

    /// Like git: each prefix of `relative` (relative to the root), top down, is decided by
    /// the deepest `.gitignore` of the folders above it with a line matching that exact path,
    /// then by the root rules; the path is ignored once a prefix is. A folder's `.gitignore`
    /// is only read when the walk reaches it, so none inside an ignored folder is read.
    fn ignored(&self, relative: &Path, is_dir: bool) -> bool {
        let mut nested = match self.nested.lock() {
            Ok(nested) => nested,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut prefixes: Vec<&Path> = relative
            .ancestors()
            .filter(|p| !p.as_os_str().is_empty())
            .collect();
        prefixes.reverse();
        let last = prefixes.len().saturating_sub(1);
        for (i, prefix) in prefixes.iter().enumerate() {
            let is_dir = i < last || is_dir;
            let mut decided = None;
            for dir in prefix.ancestors().skip(1) {
                if dir.as_os_str().is_empty() {
                    break;
                }
                if !nested.contains_key(dir) {
                    let rules = Gitignore::new(self.root.join(dir).join(".gitignore")).0;
                    nested.insert(dir.to_path_buf(), rules);
                }
                let Some(rules) = nested.get(dir) else {
                    continue;
                };
                let Ok(inner) = prefix.strip_prefix(dir) else {
                    continue;
                };
                let found = rules.matched(inner, is_dir);
                if !found.is_none() {
                    decided = Some(found.is_ignore());
                    break;
                }
            }
            let ignored =
                decided.unwrap_or_else(|| self.gitignore.matched(prefix, is_dir).is_ignore());
            if ignored {
                return true;
            }
        }
        false
    }

    /// `path` relative to the root with `/` separators, or `None` when it is not reported:
    /// outside the root, the root itself, under an always-ignored directory, under
    /// `.worktrees`, or git-ignored (root and nested `.gitignore`, `.git/info/exclude`).
    /// Markdown files are reported even when git-ignored: `doc.list` lists them (plans often
    /// live in ignored folders), so `doc.changed` must follow them.
    /// `is_dir` only matters for directory-only patterns (`build/`); `None` = unknown (the path
    /// is gone): ignored if it would be as a file or as a directory.
    pub fn relative(&self, path: &Path, is_dir: Option<bool>) -> Option<String> {
        self.relative_with(path, || is_dir)
    }

    /// [`Filter::relative`] asking `is_dir` only for a path the cheap checks keep, so an event
    /// storm under `.git`, `target` or `node_modules` costs no `stat` per path.
    fn relative_with(&self, path: &Path, is_dir: impl FnOnce() -> Option<bool>) -> Option<String> {
        let relative = path.strip_prefix(&self.root).ok()?;
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => {
                    let part = part.to_str()?;
                    if ALWAYS_IGNORED.contains(&part) || WATCH_IGNORED.contains(&part) {
                        return None;
                    }
                    parts.push(part);
                }
                _ => return None,
            }
        }
        if parts.is_empty() {
            return None;
        }
        let relative_path = parts.join("/");
        let is_dir = is_dir();
        if is_dir != Some(true) && crate::docs::is_markdown(&relative_path) {
            return Some(relative_path);
        }
        let ignored = |is_dir| self.ignored(relative, is_dir);
        let ignored = match is_dir {
            Some(is_dir) => ignored(is_dir),
            None => ignored(false) || ignored(true),
        };
        if ignored {
            return None;
        }
        Some(relative_path)
    }

    /// `path` is a file of the root's `.git` dir, or of a direct child folder's (a repository
    /// of its own, ADR 0029), that moves when commits, branches or the index change (`HEAD`,
    /// `index`, `packed-refs`, `refs/…`): the git state changed even when no working file did.
    pub fn is_git_state(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        let in_child = || {
            let mut parts = relative.components();
            parts.next();
            parts.as_path().strip_prefix(".git").ok()
        };
        let Some(relative) = relative.strip_prefix(".git").ok().or_else(in_child) else {
            return false;
        };
        let mut parts = relative.components().map(|c| c.as_os_str());
        match parts.next().and_then(|p| p.to_str()) {
            Some("HEAD" | "index" | "packed-refs") => parts.next().is_none(),
            Some("refs") => parts.next().is_some(),
            _ => false,
        }
    }
}

/// One debounced batch of changes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Batch {
    /// The changed files reported by [`Filter::relative`], sorted and de-duplicated.
    pub files: Vec<String>,
    /// A [`Filter::is_git_state`] file changed.
    pub git: bool,
}

/// A running project watcher; dropping it stops watching (a pending batch is still delivered).
pub struct Watch {
    _watcher: RecommendedWatcher,
}

/// Watches `root` recursively and calls `on_batch` with the sorted, de-duplicated relative
/// paths of the files changed (created, modified, removed, renamed) in each batch, and whether
/// the git state changed. A removed directory cannot be told from a removed file, so it is
/// reported too (T5.6 attributes such paths like files). Non-UTF-8 paths are skipped. Must run
/// inside the tokio runtime.
pub fn watch(root: &Path, on_batch: impl Fn(Batch) + Send + 'static) -> ::notify::Result<Watch> {
    let (tx, rx) = mpsc::unbounded_channel();
    let mut watcher = ::notify::recommended_watcher(
        move |event: ::notify::Result<::notify::Event>| match event {
            Ok(event) if !event.kind.is_access() => {
                let _ = tx.send(event.paths);
            }
            Ok(_) => {}
            Err(e) => tracing::debug!("file watcher error: {e}"),
        },
    )?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    tokio::spawn(debounce(Filter::new(root), rx, on_batch));
    Ok(Watch { _watcher: watcher })
}

/// Collects paths until [`QUIET`] passes without a change (or [`MAX_DELAY`] since the first
/// one), then delivers the batch. Ends when the watcher is dropped.
async fn debounce(
    filter: Filter,
    mut rx: mpsc::UnboundedReceiver<Vec<PathBuf>>,
    on_batch: impl Fn(Batch),
) {
    let mut batch = BTreeSet::new();
    let mut git = false;
    let add = |batch: &mut BTreeSet<String>, git: &mut bool, paths: Vec<PathBuf>| {
        *git |= paths.iter().any(|path| filter.is_git_state(path));
        // Files only: a directory "changes" whenever an entry in it does.
        batch.extend(paths.iter().filter_map(|path| {
            let mut is_dir = None;
            let relative = filter.relative_with(path, || {
                is_dir = path.metadata().ok().map(|m| m.is_dir());
                is_dir
            });
            relative.filter(|_| is_dir != Some(true))
        }));
    };
    while let Some(paths) = rx.recv().await {
        add(&mut batch, &mut git, paths);
        let deadline = tokio::time::Instant::now() + MAX_DELAY;
        let open = loop {
            let quiet_end = (tokio::time::Instant::now() + QUIET).min(deadline);
            match tokio::time::timeout_at(quiet_end, rx.recv()).await {
                Ok(Some(paths)) => add(&mut batch, &mut git, paths),
                Ok(None) => break false,
                Err(_) => break true,
            }
        };
        if !batch.is_empty() || git {
            on_batch(Batch {
                files: std::mem::take(&mut batch).into_iter().collect(),
                git: std::mem::take(&mut git),
            });
        }
        if !open {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(gitignore: &str) -> (tempfile::TempDir, Filter) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(".gitignore"), gitignore).unwrap();
        let filter = Filter::new(dir.path());
        (dir, filter)
    }

    #[test]
    fn honours_nested_gitignore_and_info_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs/jira/runs")).unwrap();
        std::fs::create_dir_all(root.join(".git/info")).unwrap();
        std::fs::write(
            root.join("docs/jira/.gitignore"),
            "/runs/\n!runs/keep.txt\n",
        )
        .unwrap();
        std::fs::write(root.join(".git/info/exclude"), "pr*-run*.log\n").unwrap();
        let filter = Filter::new(root);
        let rel = |p: &str| filter.relative(&root.join(p), Some(false));
        assert_eq!(
            rel("docs/jira/runs/out.txt"),
            None,
            "nested, anchored to its folder"
        );
        assert_eq!(
            rel("runs/out.txt"),
            Some("runs/out.txt".into()),
            "not anchored at the root"
        );
        assert_eq!(rel("api/pr571-karate-run2.log"), None, "info/exclude");
        assert_eq!(
            rel("docs/jira/runs/plan.md"),
            Some("docs/jira/runs/plan.md".into()),
            "md stays"
        );
        assert_eq!(rel("src/lib.rs"), Some("src/lib.rs".into()));
    }

    /// Writes `files` (path, content) under a temp root and returns the filter.
    fn tree(files: &[(&str, &str)]) -> (tempfile::TempDir, Filter) {
        let dir = tempfile::tempdir().unwrap();
        for (path, content) in files {
            let file = dir.path().join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, content).unwrap();
        }
        let filter = Filter::new(dir.path());
        (dir, filter)
    }

    #[test]
    fn a_whitelisted_folder_does_not_skip_the_other_levels() {
        let (dir, filter) = tree(&[
            (".gitignore", "*.log\n"),
            ("app/.gitignore", "/*\n!/src/\n"),
        ]);
        let rel = |p: &str| filter.relative(&dir.path().join(p), Some(false));
        assert_eq!(rel("app/src/debug.log"), None, "root *.log still applies");
        assert_eq!(rel("app/src/main.rs"), Some("app/src/main.rs".into()));
        assert_eq!(rel("app/other.txt"), None);
    }

    #[test]
    fn gitignore_inside_an_ignored_folder_is_not_read() {
        let (dir, filter) = tree(&[
            ("docs/.gitignore", "runs/\n"),
            ("docs/runs/.gitignore", "!keep.txt\n"),
        ]);
        let rel = |p: &str| filter.relative(&dir.path().join(p), Some(false));
        assert_eq!(rel("docs/runs/keep.txt"), None);
    }

    #[test]
    fn nested_gitignore_can_reinclude_a_file_the_root_ignores() {
        let (dir, filter) = tree(&[(".gitignore", "*.log\n"), ("sub/.gitignore", "!keep.log\n")]);
        let rel = |p: &str| filter.relative(&dir.path().join(p), Some(false));
        assert_eq!(rel("sub/keep.log"), Some("sub/keep.log".into()));
        assert_eq!(rel("sub/other.log"), None);
    }

    #[test]
    fn never_reports_worktrees() {
        let (dir, filter) = filter("");
        let root = dir.path();
        assert_eq!(
            filter.relative(&root.join(".worktrees/fix/src/a.rs"), Some(false)),
            None
        );
        assert_eq!(
            filter.relative(&root.join(".worktrees/fix/README.md"), Some(false)),
            None
        );
    }

    #[test]
    fn always_ignores_git_node_modules_and_target() {
        let (dir, filter) = filter("");
        let root = dir.path();
        for ignored in [
            ".git/index",
            "node_modules/a/index.js",
            "target/debug/mk.exe",
            "crates/x/target/out",
            "web/node_modules/b.js",
        ] {
            assert_eq!(
                filter.relative(&root.join(ignored), Some(false)),
                None,
                "{ignored}"
            );
        }
        assert_eq!(filter.relative(&root.join(".git"), Some(true)), None);
        assert_eq!(
            filter.relative(&root.join("src").join("main.rs"), Some(false)),
            Some("src/main.rs".to_owned())
        );
        // Only whole components: `targets/` and `.github/` are reported.
        assert_eq!(
            filter.relative(&root.join("targets/a"), Some(false)),
            Some("targets/a".to_owned())
        );
        assert_eq!(
            filter.relative(&root.join(".github/ci.yml"), Some(false)),
            Some(".github/ci.yml".to_owned())
        );
    }

    #[test]
    fn respects_gitignore_patterns_negations_and_directories() {
        let (dir, filter) = filter("*.log\n!keep.log\nbuild/\n/dist\n");
        let root = dir.path();
        assert_eq!(filter.relative(&root.join("a.log"), Some(false)), None);
        assert_eq!(filter.relative(&root.join("sub/b.log"), Some(false)), None);
        assert_eq!(
            filter.relative(&root.join("keep.log"), Some(false)),
            Some("keep.log".to_owned())
        );
        // A file under an ignored directory is ignored through its parent.
        assert_eq!(
            filter.relative(&root.join("build/out.o"), Some(false)),
            None
        );
        assert_eq!(
            filter.relative(&root.join("dist/app.js"), Some(false)),
            None
        );
        // A directory-only pattern: a file named `build` is reported, a removed one is not.
        assert_eq!(
            filter.relative(&root.join("build"), Some(false)),
            Some("build".to_owned())
        );
        assert_eq!(filter.relative(&root.join("build"), Some(true)), None);
        assert_eq!(filter.relative(&root.join("build"), None), None);
        assert_eq!(
            filter.relative(&root.join("gone.rs"), None),
            Some("gone.rs".to_owned())
        );
        assert_eq!(
            filter.relative(&root.join("src/dist/x.js"), Some(false)),
            Some("src/dist/x.js".to_owned())
        );
        // Markdown under an ignored directory is reported (reviewable plans), but not under
        // the always-ignored ones.
        assert_eq!(
            filter.relative(&root.join("build/plan.md"), Some(false)),
            Some("build/plan.md".to_owned())
        );
        assert_eq!(
            filter.relative(&root.join("node_modules/x.md"), Some(false)),
            None
        );
        // `.gitignore` itself is reported.
        assert_eq!(
            filter.relative(&root.join(".gitignore"), Some(false)),
            Some(".gitignore".to_owned())
        );
    }

    #[test]
    fn paths_outside_the_root_and_the_root_itself_are_not_reported() {
        let (dir, filter) = filter("*.log\n");
        let root = dir.path();
        assert_eq!(filter.relative(root, Some(true)), None);
        assert_eq!(
            filter.relative(&root.join("..").join("x.rs"), Some(false)),
            None
        );
        let other = tempfile::tempdir().unwrap();
        assert_eq!(
            filter.relative(&other.path().join("a.log"), Some(false)),
            None
        );
        // No `.gitignore` at all: everything but the fixed directories is reported.
        let plain = Filter::new(other.path());
        assert_eq!(
            plain.relative(&other.path().join("a.log"), Some(false)),
            Some("a.log".to_owned())
        );
    }

    #[tokio::test(start_paused = true)]
    async fn debounce_batches_until_quiet_and_caps_the_delay() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, mut out) = mpsc::unbounded_channel();
        let task = tokio::spawn(debounce(Filter::new(&root), rx, move |batch: Batch| {
            assert!(!batch.git);
            let _ = out_tx.send((tokio::time::Instant::now(), batch.files));
        }));
        let start = tokio::time::Instant::now();
        tx.send(vec![root.join("b.rs"), root.join("target/x")])
            .unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
        tx.send(vec![root.join("a.rs"), root.join("b.rs")]).unwrap();
        let (at, batch) = out.recv().await.unwrap();
        assert_eq!(batch, ["a.rs", "b.rs"]);
        assert_eq!(at - start, Duration::from_millis(500));

        // Changes every 100 ms never go quiet: the batch still comes after MAX_DELAY.
        let start = tokio::time::Instant::now();
        for i in 0..30 {
            tx.send(vec![root.join(format!("f{i}"))]).unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok((at, batch)) = out.try_recv() {
                assert_eq!(at - start, MAX_DELAY);
                // The change sent right at the deadline may land in either batch.
                assert!((20..=21).contains(&batch.len()), "{batch:?}");
                break;
            }
            assert!(i < 25, "no batch after {i} changes");
        }

        // Only ignored paths: no batch. Dropping the sender delivers what is pending and ends.
        tx.send(vec![root.join("node_modules/x")]).unwrap();
        tokio::time::sleep(QUIET * 2).await;
        tx.send(vec![root.join("last.rs")]).unwrap();
        drop(tx);
        task.await.unwrap();
        let rest: Vec<_> = std::iter::from_fn(|| out.try_recv().ok())
            .map(|(_, batch)| batch)
            .collect();
        assert!(
            rest.last()
                .is_some_and(|b| b.ends_with(&["last.rs".to_owned()])),
            "{rest:?}"
        );
        assert!(
            rest.iter()
                .flatten()
                .all(|p| !p.starts_with("node_modules"))
        );
    }

    #[test]
    fn git_state_is_head_index_and_refs_of_the_root_or_a_child_git_dir() {
        let (dir, filter) = filter("");
        for git in [dir.path().join(".git"), dir.path().join("fe").join(".git")] {
            for state in [
                "HEAD",
                "index",
                "packed-refs",
                "refs/heads/main",
                "refs/tags/v1",
            ] {
                assert!(
                    filter.is_git_state(&git.join(state)),
                    "{}",
                    git.join(state).display()
                );
            }
            for other in [
                "index.lock",
                "refs",
                "objects/ab/cdef",
                "logs/HEAD",
                "HEAD/x",
            ] {
                assert!(!filter.is_git_state(&git.join(other)), "{other}");
            }
        }
        // Only direct child repositories (ADR 0029), never deeper ones or plain files.
        assert!(!filter.is_git_state(&dir.path().join("apps/web/.git/HEAD")));
        assert!(!filter.is_git_state(&dir.path().join("HEAD")));
        assert!(!filter.is_git_state(&dir.path().join("fe/HEAD")));
    }

    #[tokio::test(start_paused = true)]
    async fn debounce_flags_git_state_changes_without_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (tx, rx) = mpsc::unbounded_channel();
        let (out_tx, mut out) = mpsc::unbounded_channel();
        let task = tokio::spawn(debounce(Filter::new(&root), rx, move |batch| {
            let _ = out_tx.send(batch);
        }));
        tx.send(vec![root.join(".git/index.lock"), root.join(".git/index")])
            .unwrap();
        tx.send(vec![root.join(".git/objects/ab/cd")]).unwrap();
        let batch = out.recv().await.unwrap();
        assert_eq!(
            batch,
            Batch {
                files: vec![],
                git: true
            }
        );
        // Reset after each batch.
        tx.send(vec![root.join("a.rs")]).unwrap();
        drop(tx);
        task.await.unwrap();
        assert_eq!(
            out.recv().await.unwrap(),
            Batch {
                files: vec!["a.rs".into()],
                git: false
            }
        );
    }

    #[tokio::test]
    async fn watcher_reports_project_changes_but_not_ignored_ones() {
        let dir = tempfile::tempdir().unwrap();
        // Canonical: notify reports paths under the watched path as given (8.3 names on Windows).
        let root = dunce::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        for sub in ["src", "target", "node_modules", ".git"] {
            std::fs::create_dir(root.join(sub)).unwrap();
        }
        let (tx, mut batches) = mpsc::unbounded_channel();
        let _watch = watch(&root, move |batch| {
            let _ = tx.send(batch);
        })
        .unwrap();
        // Ignored writes first, so a batch containing the tracked file has seen them too.
        for ignored in ["target/a.o", "node_modules/b.js", ".git/index", "debug.log"] {
            std::fs::write(root.join(ignored), "x").unwrap();
        }
        std::fs::write(root.join("src/lib.rs"), "fn main() {}").unwrap();
        let mut seen = BTreeSet::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        while !seen.contains("src/lib.rs") {
            let batch = tokio::time::timeout_at(deadline, batches.recv())
                .await
                .expect("no batch with src/lib.rs")
                .unwrap();
            seen.extend(batch.files);
        }
        // Not `src` either, although creating `src/lib.rs` changes the directory.
        assert_eq!(seen, BTreeSet::from(["src/lib.rs".to_owned()]));
    }
}
