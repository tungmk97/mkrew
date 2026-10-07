//! Turn snapshots (SPEC §20C.1): one at a time per project, each kept reachable by a ref.
//! Blocking: callers run [`Snapshots::take`] in `spawn_blocking`.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ulid::Ulid;

use crate::git::GitBackend;

/// The ref keeping turn `n`'s start snapshot of `session` (outside `refs/heads`: no branch).
pub fn turn_ref(session: Ulid, n: u32) -> String {
    format!("refs/mkrew/turns/{session}/{n}")
}

/// Per-project snapshot locks: a later request for the same project waits for the running one.
/// Each lock guards the index path of every folder (checkout or worktree) snapshotted in the
/// project, resolved once: every git process costs ~50 ms on Windows, and the budget is 300 ms.
#[derive(Default)]
pub struct Snapshots {
    projects: Mutex<HashMap<Ulid, Arc<Mutex<IndexPaths>>>>,
}

/// Folder (absolute) → its index file.
type IndexPaths = HashMap<PathBuf, PathBuf>;

impl Snapshots {
    /// Snapshots the repo containing `dir` (absolute, in `project`) and points `ref_name` at
    /// the tree.
    pub fn take(
        &self,
        git: &dyn GitBackend,
        project: Ulid,
        dir: &Path,
        ref_name: &str,
    ) -> io::Result<String> {
        self.with_index(project, dir, git, |index| {
            git.snapshot(dir, index)
                .and_then(|tree| git.update_ref(dir, ref_name, &tree).map(|()| tree))
        })
    }

    /// A snapshot of the repo containing `dir` that no ref keeps (git may prune it later).
    pub fn tree(&self, git: &dyn GitBackend, project: Ulid, dir: &Path) -> io::Result<String> {
        self.with_index(project, dir, git, |index| git.snapshot(dir, index))
    }

    /// Writes `paths` (relative to `dir`) back as they are in `tree`; waits for running
    /// snapshots of the project, so none sees a half-restored folder.
    pub fn checkout(
        &self,
        git: &dyn GitBackend,
        project: Ulid,
        dir: &Path,
        tree: &str,
        paths: &[String],
    ) -> io::Result<()> {
        self.with_index(project, dir, git, |index| {
            git.checkout_tree(dir, index, tree, paths)
        })
    }

    /// Runs `run` with `dir`'s index path, one at a time per project.
    fn with_index<T>(
        &self,
        project: Ulid,
        dir: &Path,
        git: &dyn GitBackend,
        run: impl FnOnce(&Path) -> io::Result<T>,
    ) -> io::Result<T> {
        self.one_at_a_time(project, |indexes| {
            let index = match indexes.get(dir) {
                Some(index) => index,
                None => indexes
                    .entry(dir.to_path_buf())
                    .or_insert(git.index_path(dir)?),
            };
            let done = run(index);
            if done.is_err() {
                // The folder may have changed repo: resolve again next time.
                indexes.remove(dir);
            }
            done
        })
    }

    fn one_at_a_time<T>(&self, project: Ulid, run: impl FnOnce(&mut IndexPaths) -> T) -> T {
        let project = Arc::clone(lock(&self.projects).entry(project).or_default());
        let mut indexes = lock(&project);
        run(&mut indexes)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Maps of locks / cached paths: still consistent if a holder panicked.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::git::GitCli;

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            // `status` must not refresh the index the tests compare byte for byte.
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    /// Everything a snapshot must leave alone: HEAD, branch, refs/heads, the index (bytes and
    /// what it stages), every file's content.
    #[derive(Debug, PartialEq)]
    struct State {
        head: String,
        branch: String,
        heads: String,
        index: Vec<u8>,
        staged: String,
        status: String,
        files: Vec<(PathBuf, Vec<u8>)>,
    }

    fn state(root: &Path, git_dir: &Path) -> State {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.file_name().is_some_and(|n| n == ".git") {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else {
                    files.push((path.clone(), std::fs::read(&path).unwrap()));
                }
            }
        }
        files.sort();
        State {
            head: git(root, &["rev-parse", "HEAD"]),
            branch: git(root, &["symbolic-ref", "HEAD"]),
            heads: git(root, &["for-each-ref", "refs/heads"]),
            index: std::fs::read(git_dir.join("index")).unwrap(),
            staged: git(root, &["diff", "--cached", "--name-status"]),
            status: git(root, &["status", "--porcelain", "--ignored"]),
            files,
        }
    }

    /// Paths and blob contents of `tree`.
    fn tree_files(dir: &Path, tree: &str) -> Vec<String> {
        git(dir, &["ls-tree", "-r", "--name-only", tree])
            .lines()
            .map(|p| {
                format!(
                    "{p}={}",
                    git(dir, &["cat-file", "-p", &format!("{tree}:{p}")])
                )
            })
            .collect()
    }

    #[test]
    fn a_snapshot_records_the_working_state_and_changes_nothing() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "core.autocrlf", "false"]);
        let app = root.join("app");
        std::fs::create_dir(&app).unwrap();
        std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        std::fs::write(app.join("a.txt"), "a\n").unwrap();
        std::fs::write(app.join("b.txt"), "b\n").unwrap();
        std::fs::write(root.join("gone.txt"), "g\n").unwrap();
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "one"]);
        // Staged, unstaged, deleted, untracked and ignored changes, from the project folder.
        std::fs::write(app.join("a.txt"), "staged\n").unwrap();
        git(root, &["add", "app/a.txt"]);
        std::fs::write(app.join("a.txt"), "staged and more\n").unwrap();
        std::fs::write(app.join("b.txt"), "unstaged\n").unwrap();
        std::fs::remove_file(root.join("gone.txt")).unwrap();
        std::fs::write(app.join("new.txt"), "n\n").unwrap();
        std::fs::write(app.join("debug.log"), "ignored\n").unwrap();
        let git_dir = root.join(".git");
        let before = state(root, &git_dir);

        let snapshots = Snapshots::default();
        let session = Ulid::new();
        let name = turn_ref(session, 1);
        let tree = snapshots.take(&GitCli, Ulid::new(), &app, &name).unwrap();

        assert_eq!(state(root, &git_dir), before);
        assert_eq!(
            tree_files(root, &tree),
            [
                ".gitignore=*.log",
                "app/a.txt=staged and more",
                "app/b.txt=unstaged",
                "app/new.txt=n"
            ]
        );
        assert_eq!(git(root, &["rev-parse", &name]), tree);
        assert_eq!(name, format!("refs/mkrew/turns/{session}/1"));
        // No temp index left behind.
        let leftovers: Vec<_> = std::fs::read_dir(&git_dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("mkrew"))
            .collect();
        assert_eq!(leftovers, Vec::<String>::new());
    }

    /// A linked worktree has its own index (`.git/worktrees/<name>/index`): that one is copied,
    /// and neither worktree's index or HEAD moves.
    #[test]
    fn a_snapshot_in_a_worktree_uses_its_own_index() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path().join("main");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "core.autocrlf", "false"]);
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "one"]);
        let tree_dir = repo.path().join("wt");
        git(
            &root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "wt",
                tree_dir.to_str().unwrap(),
            ],
        );
        std::fs::write(tree_dir.join("b.txt"), "b\n").unwrap();
        git(&tree_dir, &["add", "b.txt"]);
        std::fs::write(tree_dir.join("a.txt"), "edited\n").unwrap();
        let wt_git_dir = PathBuf::from(git(&tree_dir, &["rev-parse", "--absolute-git-dir"]));
        let main_before = state(&root, &root.join(".git"));
        let wt_before = state(&tree_dir, &wt_git_dir);

        let name = turn_ref(Ulid::new(), 2);
        let tree = Snapshots::default()
            .take(&GitCli, Ulid::new(), &tree_dir, &name)
            .unwrap();

        assert_eq!(state(&root, &root.join(".git")), main_before);
        assert_eq!(state(&tree_dir, &wt_git_dir), wt_before);
        assert_eq!(tree_files(&root, &tree), ["a.txt=edited", "b.txt=b"]);
        // Refs are shared by all worktrees.
        assert_eq!(git(&root, &["rev-parse", &name]), tree);
    }

    /// A fake whose snapshot sleeps and counts how many run at once.
    #[derive(Default)]
    struct Slow {
        running: AtomicUsize,
        most: AtomicUsize,
    }

    impl Slow {
        fn snapshot(&self) -> io::Result<String> {
            let now = self.running.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(30));
            self.running.fetch_sub(1, Ordering::SeqCst);
            Ok("tree".into())
        }
    }

    #[test]
    fn snapshots_of_one_project_run_one_at_a_time() {
        let snapshots = Snapshots::default();
        let one = Ulid::new();
        let run = |project: Ulid, fake: &Slow| {
            let tree = snapshots
                .one_at_a_time(project, |_| fake.snapshot())
                .unwrap();
            assert_eq!(tree, "tree");
        };
        let same = Slow::default();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| run(one, &same));
            }
        });
        assert_eq!(same.most.load(Ordering::SeqCst), 1);
        // Different projects do not wait for each other.
        let different = Slow::default();
        std::thread::scope(|s| {
            for _ in 0..4 {
                s.spawn(|| run(Ulid::new(), &different));
            }
        });
        assert!(different.most.load(Ordering::SeqCst) > 1);
    }

    /// SPEC §20C.1 / M11: a snapshot of a 10,000-file repo takes < 300 ms. Asserted in release
    /// builds only; debug builds log the time.
    #[test]
    fn snapshot_of_a_10000_file_repo_meets_the_budget() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["config", "core.autocrlf", "false"]);
        // Same generated repo as the `git.status` budget test (`git_view.rs`).
        let mut stream = String::from("commit refs/heads/main\ncommitter t <t@t> 0 +0000\n");
        stream.push_str("data 5\nfiles\n");
        for d in 0..100 {
            for f in 0..100 {
                let content = format!("{d} {f}\n");
                stream.push_str(&format!("M 100644 inline d{d:02}/f{f:02}.txt\n"));
                stream.push_str(&format!("data {}\n{content}\n", content.len()));
            }
        }
        let stream_file = root.join(".git").join("stream");
        std::fs::write(&stream_file, stream).unwrap();
        let imported = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["fast-import", "--quiet"])
            .stdin(std::fs::File::open(&stream_file).unwrap())
            .status()
            .unwrap();
        assert!(imported.success());
        git(root, &["-c", "checkout.workers=0", "reset", "-q", "--hard"]);
        for d in 0..10 {
            std::fs::write(root.join(format!("d{d:02}/f00.txt")), "changed\n").unwrap();
        }
        std::fs::write(root.join("new.txt"), "new\n").unwrap();
        // The user's own git keeps the index fresh (here: rehashes the files the checkout left
        // racily clean), which the helper's `GIT_OPTIONAL_LOCKS=0` would prevent.
        let refreshed = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["status", "-s"])
            .output()
            .unwrap();
        assert!(refreshed.status.success());

        let snapshots = Snapshots::default();
        let project = Ulid::new();
        let session = Ulid::new();
        let mut times: Vec<_> = (0..5)
            .map(|n| {
                let started = Instant::now();
                snapshots
                    .take(&GitCli, project, root, &turn_ref(session, n))
                    .unwrap();
                started.elapsed()
            })
            .collect();
        let tree = git(root, &["rev-parse", &turn_ref(session, 0)]);
        assert_eq!(
            git(root, &["ls-tree", "-r", "--name-only", &tree])
                .lines()
                .count(),
            10_001
        );
        times.sort();
        let median = times[2];
        eprintln!("snapshot of 10,000 files: median {median:?} of {times:?}");
        if !cfg!(debug_assertions) {
            assert!(median < Duration::from_millis(300), "{median:?}");
        }
    }
}
