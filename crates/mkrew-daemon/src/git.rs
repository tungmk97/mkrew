//! Git access behind a trait (SPEC §12B), so a `gix` backend can replace the CLI later.
//! Only local commands; nothing here touches the network.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mkrew_protocol::{ChangedFile, GitStatus};

pub trait GitBackend: Send + Sync {
    /// The commit HEAD points at for the repo containing `dir`; `None` outside a repo or
    /// before the first commit. `Err` = git could not be run.
    fn head(&self, dir: &Path) -> io::Result<Option<String>>;

    /// Tracked files under `dir` whose working copy differs from `commit`, relative to `dir`.
    fn changed_files(&self, dir: &Path, commit: &str) -> io::Result<Vec<Changed>>;

    /// Unified diff from `commit` to the working copy of the tracked files under `dir`.
    fn diff(&self, dir: &Path, commit: &str) -> io::Result<String>;

    /// Untracked, non-ignored files under `dir`, relative to it.
    fn untracked_files(&self, dir: &Path) -> io::Result<Vec<String>>;

    /// Index and working copy of the tracked files under `dir` back to `commit`.
    fn restore(&self, dir: &Path, commit: &str) -> io::Result<()>;

    /// Deletes the untracked `paths` (relative to `dir`); tracked paths are left alone.
    fn clean(&self, dir: &Path, paths: &[String]) -> io::Result<()>;

    /// Where the repo containing `dir` keeps its data; `None` outside a repo.
    fn repo(&self, dir: &Path) -> io::Result<Option<Repo>>;

    /// Branch header and staged/unstaged/untracked files under `dir` (paths relative to it,
    /// no line counts): one `git status` call.
    fn status(&self, dir: &Path, repo: &Repo) -> io::Result<GitStatus>;

    /// Files under `dir` changed on `side`, with line counts, relative to `dir`.
    fn changes(&self, dir: &Path, side: Side<'_>) -> io::Result<Vec<ChangedFile>>;

    /// The auto-detected base (`origin/HEAD` → `main` → `master`): name and commit.
    fn auto_base(&self, dir: &Path) -> io::Result<Option<(String, String)>>;

    /// The commit `rev` names; `None` if it names none.
    fn commit_of(&self, dir: &Path, rev: &str) -> io::Result<Option<String>>;

    /// `git merge-base a b`; `None` when the histories are unrelated.
    fn merge_base(&self, dir: &Path, a: &str, b: &str) -> io::Result<Option<String>>;

    /// Unified diff of `path` (relative to `dir`) on `side`; empty when unchanged or untracked.
    fn file_diff(&self, dir: &Path, path: &str, side: Side<'_>) -> io::Result<String>;

    /// Unified diff showing the untracked `path` as all-new.
    fn untracked_diff(&self, dir: &Path, path: &str) -> io::Result<String>;

    /// `path` is in the index.
    fn is_tracked(&self, dir: &Path, path: &str) -> io::Result<bool>;
}

/// A repository as seen from a folder inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// The (worktree's) git dir, holding `index`.
    pub git_dir: PathBuf,
    /// The folder relative to the repo root, `/`-separated with a trailing `/`; `""` at the
    /// root. `git status` paths are repo-relative and lose this prefix.
    pub prefix: String,
}

/// Which two trees a diff compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side<'a> {
    /// Index → working tree.
    Unstaged,
    /// HEAD → index.
    Staged,
    /// A commit → working tree.
    Since(&'a str),
    /// What a commit introduced (first parent for merges).
    Commit(&'a str),
}

/// A tracked file that differs from a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Changed {
    pub path: String,
    /// Not in the commit: restoring to it deletes the file.
    pub added: bool,
}

/// Shells out to `git` from `PATH`.
pub struct GitCli;

/// Keeps `git clean` command lines far below the Windows 32 Ki character limit.
const MAX_ARGS_LEN: usize = 8 * 1024;

impl GitBackend for GitCli {
    fn head(&self, dir: &Path) -> io::Result<Option<String>> {
        let output = git(dir)
            .args(["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
            .output()?;
        if !output.status.success() {
            return Ok(None);
        }
        let head = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Ok((!head.is_empty()).then_some(head))
    }

    fn changed_files(&self, dir: &Path, commit: &str) -> io::Result<Vec<Changed>> {
        let mut command = git(dir);
        command
            .args(["diff", "--name-status", "-z", "--no-renames", "--relative"])
            .args([commit, "--", "."]);
        // `-z --name-status` without renames: status, path, status, path, …
        let fields = nul_separated(&run(command)?);
        Ok(fields
            .as_chunks::<2>()
            .0
            .iter()
            .map(|[status, path]| Changed {
                path: path.clone(),
                added: status == "A",
            })
            .collect())
    }

    fn diff(&self, dir: &Path, commit: &str) -> io::Result<String> {
        let mut command = git(dir);
        command
            .args([
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-renames",
                "--relative",
            ])
            .args([commit, "--", "."]);
        Ok(String::from_utf8_lossy(&run(command)?).into_owned())
    }

    fn untracked_files(&self, dir: &Path) -> io::Result<Vec<String>> {
        let mut command = git(dir);
        command.args([
            "ls-files",
            "--others",
            "--exclude-standard",
            "-z",
            "--",
            ".",
        ]);
        Ok(nul_separated(&run(command)?))
    }

    fn restore(&self, dir: &Path, commit: &str) -> io::Result<()> {
        let mut command = git(dir);
        command
            .args(["restore", "--staged", "--worktree"])
            .arg(format!("--source={commit}"))
            .args(["--", "."]);
        run(command).map(drop)
    }

    fn clean(&self, dir: &Path, paths: &[String]) -> io::Result<()> {
        let mut rest = paths;
        while !rest.is_empty() {
            let mut len = 0;
            let count = rest
                .iter()
                .take_while(|p| {
                    len += p.len() + 1;
                    len <= MAX_ARGS_LEN
                })
                .count()
                .max(1);
            let (chunk, tail) = rest.split_at(count);
            let mut command = git(dir);
            // Literal pathspecs: a file named `*` must not match everything.
            command
                .args(["--literal-pathspecs", "clean", "-f", "-q", "--"])
                .args(chunk);
            run(command)?;
            rest = tail;
        }
        Ok(())
    }

    fn repo(&self, dir: &Path) -> io::Result<Option<Repo>> {
        let output = git(dir)
            .args(["rev-parse", "--absolute-git-dir", "--show-prefix"])
            .output()?;
        if !output.status.success() {
            return Ok(None);
        }
        let text = String::from_utf8_lossy(&output.stdout);
        let mut lines = text.lines();
        let git_dir = lines.next().unwrap_or_default().trim();
        if git_dir.is_empty() {
            return Ok(None);
        }
        Ok(Some(Repo {
            git_dir: PathBuf::from(git_dir),
            prefix: lines.next().unwrap_or_default().trim().to_owned(),
        }))
    }

    fn status(&self, dir: &Path, repo: &Repo) -> io::Result<GitStatus> {
        let mut command = view_git(dir);
        command.args([
            "status",
            "--porcelain=v2",
            "--branch",
            "-z",
            "--untracked-files=all",
            "--",
            ".",
        ]);
        let mut status = mkrew_core::git::parse_status(&run(command)?);
        let strip = |path: &mut String| {
            if let Some(rest) = path.strip_prefix(&repo.prefix) {
                *path = rest.to_owned();
            }
        };
        for file in status
            .staged
            .iter_mut()
            .chain(&mut status.unstaged)
            .chain(&mut status.untracked)
        {
            strip(&mut file.path);
            if let Some(old) = &mut file.old_path {
                strip(old);
            }
        }
        Ok(status)
    }

    fn changes(&self, dir: &Path, side: Side<'_>) -> io::Result<Vec<ChangedFile>> {
        let mut command = diff_command(dir, side);
        command.args(["--raw", "--numstat", "-z", "--", "."]);
        Ok(mkrew_core::git::parse_changes(&run(command)?))
    }

    fn auto_base(&self, dir: &Path) -> io::Result<Option<(String, String)>> {
        let mut command = view_git(dir);
        command.args([
            "for-each-ref",
            "--format=%(refname)%09%(objectname)%09%(symref)",
            "refs/remotes/origin/HEAD",
            "refs/heads/main",
            "refs/heads/master",
        ]);
        let output = run(command)?;
        Ok(mkrew_core::git::auto_base(&String::from_utf8_lossy(
            &output,
        )))
    }

    fn commit_of(&self, dir: &Path, rev: &str) -> io::Result<Option<String>> {
        let output = view_git(dir)
            .args(["rev-parse", "--verify", "--quiet"])
            .arg(format!("{rev}^{{commit}}"))
            .output()?;
        let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        Ok((output.status.success() && !commit.is_empty()).then_some(commit))
    }

    fn merge_base(&self, dir: &Path, a: &str, b: &str) -> io::Result<Option<String>> {
        let output = view_git(dir)
            .args(["merge-base", a, b])
            .stderr(Stdio::piped())
            .output()?;
        // Exit 1 without output = no common ancestor.
        let base = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        match output.status.code() {
            Some(0) if !base.is_empty() => Ok(Some(base)),
            Some(1) => Ok(None),
            _ => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                Err(io::Error::other(format!("git failed: {}", stderr.trim())))
            }
        }
    }

    fn file_diff(&self, dir: &Path, path: &str, side: Side<'_>) -> io::Result<String> {
        let mut command = diff_command(dir, side);
        command.args(["--src-prefix=a/", "--dst-prefix=b/", "--", path]);
        Ok(String::from_utf8_lossy(&run(command)?).into_owned())
    }

    fn untracked_diff(&self, dir: &Path, path: &str) -> io::Result<String> {
        let mut command = view_git(dir);
        command
            .args(["diff", "--no-index", "--no-color", "--no-ext-diff"])
            .args(["--no-textconv", "--src-prefix=a/", "--dst-prefix=b/"])
            // Git maps `/dev/null` to "no file" on Windows too.
            .args(["--", "/dev/null", path]);
        let output = command.stderr(Stdio::piped()).output()?;
        // `--no-index` exits 1 when the files differ.
        if !matches!(output.status.code(), Some(0 | 1)) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(io::Error::other(format!("git failed: {}", stderr.trim())));
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn is_tracked(&self, dir: &Path, path: &str) -> io::Result<bool> {
        let mut command = view_git(dir);
        command.args(["--literal-pathspecs", "ls-files", "-z", "--", path]);
        Ok(!run(command)?.is_empty())
    }
}

/// `git diff` (or `git show` for [`Side::Commit`]) between the trees of `side`, before the
/// output options and the pathspec. Renames detected; pathspecs literal.
fn diff_command(dir: &Path, side: Side<'_>) -> Command {
    let mut command = view_git(dir);
    command.arg("--literal-pathspecs");
    match side {
        Side::Commit(_) => command.args(["show", "--format=", "--diff-merges=first-parent"]),
        _ => command.arg("diff"),
    };
    command.args([
        "--no-color",
        "--no-ext-diff",
        "--no-textconv",
        "--relative",
        "-M",
    ]);
    match side {
        Side::Unstaged => {}
        Side::Staged => {
            command.arg("--cached");
        }
        Side::Since(commit) | Side::Commit(commit) => {
            command.arg(commit);
        }
    }
    command
}

/// [`git`] for read-only Git view commands: they skip optional locks (e.g. the index refresh
/// of `git status`) so they never block a user's own git command.
fn view_git(dir: &Path) -> Command {
    let mut command = git(dir);
    command.env("GIT_OPTIONAL_LOCKS", "0");
    command
}

/// Runs `command`; a failure becomes an error carrying git's stderr.
fn run(mut command: Command) -> io::Result<Vec<u8>> {
    let output = command.stderr(Stdio::piped()).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(io::Error::other(format!("git failed: {}", stderr.trim())));
    }
    Ok(output.stdout)
}

fn nul_separated(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .map(|p| String::from_utf8_lossy(p).into_owned())
        .collect()
}

/// `git -C <dir>` with null stdio except stdout, ignoring any repo forced by the daemon's
/// own environment.
fn git(dir: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The daemon has no console; without this every git call would open one.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) -> String {
        let output = git(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?} failed");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    #[test]
    fn head_is_none_outside_a_repo() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(GitCli.head(dir.path()).unwrap(), None);
    }

    #[test]
    fn head_is_none_before_the_first_commit() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        assert_eq!(GitCli.head(dir.path()).unwrap(), None);
    }

    #[test]
    fn head_follows_commits_and_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        run(dir.path(), &["commit", "-q", "--allow-empty", "-m", "one"]);
        let first = run(dir.path(), &["rev-parse", "HEAD"]);
        assert_eq!(GitCli.head(dir.path()).unwrap(), Some(first.clone()));
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        assert_eq!(GitCli.head(&sub).unwrap(), Some(first.clone()));
        run(dir.path(), &["commit", "-q", "--allow-empty", "-m", "two"]);
        let second = GitCli.head(dir.path()).unwrap().unwrap();
        assert_ne!(second, first);
        assert_eq!(second.len(), 40);
    }

    /// The project is a subfolder of the repo: everything stays inside it.
    #[test]
    fn changes_diff_restore_and_clean_stay_in_the_folder() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let dir = root.join("app");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(root.join("outside.txt"), "a\n").unwrap();
        std::fs::write(dir.join("kept.txt"), "one\n").unwrap();
        std::fs::write(dir.join("edited.txt"), "one\n").unwrap();
        std::fs::write(dir.join(".gitignore"), "*.log\n").unwrap();
        run(root, &["init", "-q"]);
        // Byte-exact restores whatever the machine's global autocrlf says.
        run(root, &["config", "core.autocrlf", "false"]);
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "one"]);
        let commit = GitCli.head(root).unwrap().unwrap();

        std::fs::write(root.join("outside.txt"), "b\n").unwrap();
        std::fs::write(dir.join("edited.txt"), "two\n").unwrap();
        std::fs::write(dir.join("staged.txt"), "new\n").unwrap();
        run(&dir, &["add", "staged.txt"]);
        std::fs::create_dir(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub").join("a[1].txt"), "x\n").unwrap();
        std::fs::write(dir.join("a1.txt"), "keep me\n").unwrap();
        std::fs::write(dir.join("build.log"), "ignored\n").unwrap();

        let changed = |path: &str, added| Changed {
            path: path.into(),
            added,
        };
        assert_eq!(
            GitCli.changed_files(&dir, &commit).unwrap(),
            [changed("edited.txt", false), changed("staged.txt", true)]
        );
        let diff = GitCli.diff(&dir, &commit).unwrap();
        assert!(diff.contains("+two"), "{diff}");
        assert!(diff.contains("+++ b/staged.txt"), "{diff}");
        assert!(!diff.contains("outside"), "{diff}");
        assert_eq!(
            GitCli.untracked_files(&dir).unwrap(),
            ["a1.txt", "sub/a[1].txt"]
        );

        GitCli.restore(&dir, &commit).unwrap();
        GitCli
            .clean(&dir, &["sub/a[1].txt".to_owned(), "kept.txt".to_owned()])
            .unwrap();
        assert_eq!(GitCli.changed_files(&dir, &commit).unwrap(), []);
        assert_eq!(
            std::fs::read_to_string(dir.join("edited.txt")).unwrap(),
            "one\n"
        );
        assert!(!dir.join("staged.txt").exists());
        assert!(!dir.join("sub").join("a[1].txt").exists());
        // Literal pathspecs; tracked and ignored files and the rest of the repo are kept.
        assert!(dir.join("a1.txt").exists());
        assert!(dir.join("kept.txt").exists());
        assert!(dir.join("build.log").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("outside.txt")).unwrap(),
            "b\n"
        );
    }

    #[test]
    fn failures_carry_git_stderr() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        let missing = "0123456789abcdef0123456789abcdef01234567";
        let error = GitCli.diff(dir.path(), missing).unwrap_err();
        assert!(error.to_string().starts_with("git failed: "), "{error}");
        assert!(GitCli.restore(dir.path(), missing).is_err());
    }
}
