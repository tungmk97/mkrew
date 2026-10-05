//! Project file watcher (SPEC §8, M5): one recursive `notify` watcher per project with live
//! sessions. Changed paths pass [`Filter`] and are delivered in debounced batches.
//!
//! [`Filter`] is pure (apart from reading the root `.gitignore` once) and unit-tested; the
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

/// Which changed paths of one project are reported.
pub struct Filter {
    root: PathBuf,
    gitignore: Gitignore,
}

impl Filter {
    /// Reads `<root>/.gitignore` (missing or partly invalid files ignore nothing / the bad lines).
    pub fn new(root: &Path) -> Self {
        let (gitignore, error) = Gitignore::new(root.join(".gitignore"));
        if let Some(e) = error.filter(|_| root.join(".gitignore").exists()) {
            tracing::debug!(root = %root.display(), "partly invalid .gitignore: {e}");
        }
        Self {
            root: root.to_path_buf(),
            gitignore,
        }
    }

    /// `path` relative to the root with `/` separators, or `None` when it is not reported:
    /// outside the root, the root itself, under an always-ignored directory or git-ignored.
    /// Markdown files are reported even when git-ignored: `doc.list` lists them (plans often
    /// live in ignored folders), so `doc.changed` must follow them.
    /// `is_dir` only matters for directory-only patterns (`build/`); `None` = unknown (the path
    /// is gone): ignored if it would be as a file or as a directory.
    pub fn relative(&self, path: &Path, is_dir: Option<bool>) -> Option<String> {
        let relative = path.strip_prefix(&self.root).ok()?;
        let mut parts = Vec::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => {
                    let part = part.to_str()?;
                    if ALWAYS_IGNORED.contains(&part) {
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
        if is_dir != Some(true) && crate::docs::is_markdown(&relative_path) {
            return Some(relative_path);
        }
        // Relative paths: `matched_path_or_any_parents` panics on paths outside its root.
        let ignored = |is_dir| {
            self.gitignore
                .matched_path_or_any_parents(relative, is_dir)
                .is_ignore()
        };
        let ignored = match is_dir {
            Some(is_dir) => ignored(is_dir),
            None => ignored(false) || ignored(true),
        };
        if ignored {
            return None;
        }
        Some(relative_path)
    }
}

/// A running project watcher; dropping it stops watching (a pending batch is still delivered).
pub struct Watch {
    _watcher: RecommendedWatcher,
}

/// Watches `root` recursively and calls `on_batch` with the sorted, de-duplicated relative
/// paths of the files changed (created, modified, removed, renamed) in each batch. A removed
/// directory cannot be told from a removed file, so it is reported too (T5.6 attributes such
/// paths like files). Non-UTF-8 paths are skipped. Must run inside the tokio runtime.
pub fn watch(
    root: &Path,
    on_batch: impl Fn(Vec<String>) + Send + 'static,
) -> ::notify::Result<Watch> {
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
    on_batch: impl Fn(Vec<String>),
) {
    let mut batch = BTreeSet::new();
    let add = |batch: &mut BTreeSet<String>, paths: Vec<PathBuf>| {
        // Files only: a directory "changes" whenever an entry in it does.
        batch.extend(paths.iter().filter_map(|path| {
            let is_dir = path.metadata().ok().map(|m| m.is_dir());
            if is_dir == Some(true) {
                return None;
            }
            filter.relative(path, is_dir)
        }));
    };
    while let Some(paths) = rx.recv().await {
        add(&mut batch, paths);
        let deadline = tokio::time::Instant::now() + MAX_DELAY;
        let open = loop {
            let quiet_end = (tokio::time::Instant::now() + QUIET).min(deadline);
            match tokio::time::timeout_at(quiet_end, rx.recv()).await {
                Ok(Some(paths)) => add(&mut batch, paths),
                Ok(None) => break false,
                Err(_) => break true,
            }
        };
        if !batch.is_empty() {
            on_batch(std::mem::take(&mut batch).into_iter().collect());
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
        let task = tokio::spawn(debounce(Filter::new(&root), rx, move |batch| {
            let _ = out_tx.send((tokio::time::Instant::now(), batch));
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
            seen.extend(batch);
        }
        // Not `src` either, although creating `src/lib.rs` changes the directory.
        assert_eq!(seen, BTreeSet::from(["src/lib.rs".to_owned()]));
    }
}
