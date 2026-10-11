//! Git access behind a trait (SPEC §12B), so a `gix` backend can replace the CLI later.
//! Only [`GitBackend::fetch`] and [`GitBackend::push`] touch the network.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mkrew_protocol::{ChangedFile, Commit, GitStatus};

pub trait GitBackend: Send + Sync {
    /// The commit HEAD points at for the repo containing `dir`; `None` outside a repo or
    /// before the first commit. `Err` = git could not be run.
    fn head(&self, dir: &Path) -> io::Result<Option<String>>;

    /// Tracked files under `dir` whose working copy (or tree `to`) differs from `commit` (a
    /// commit or tree), relative to `dir`.
    fn changed_files(&self, dir: &Path, commit: &str, to: Option<&str>)
    -> io::Result<Vec<Changed>>;

    /// Unified diff from `commit` to the working copy of the tracked files under `dir` (or to
    /// tree `to`).
    fn diff(&self, dir: &Path, commit: &str, to: Option<&str>) -> io::Result<String>;

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

    /// The tree `rev` names (a commit's tree too); `None` if it names none.
    fn tree_of(&self, dir: &Path, rev: &str) -> io::Result<Option<String>>;

    /// `git merge-base a b`; `None` when the histories are unrelated.
    fn merge_base(&self, dir: &Path, a: &str, b: &str) -> io::Result<Option<String>>;

    /// Unified diff of `path` (relative to `dir`) on `side`; empty when unchanged or untracked.
    fn file_diff(&self, dir: &Path, path: &str, side: Side<'_>) -> io::Result<String>;

    /// Unified diff showing the untracked `path` as all-new.
    fn untracked_diff(&self, dir: &Path, path: &str) -> io::Result<String>;

    /// `path` is in the index.
    fn is_tracked(&self, dir: &Path, path: &str) -> io::Result<bool>;

    /// Text of `path` (relative to `dir`) in the index; `None` when git cannot give it
    /// (untracked, unknown, conflicted, outside a repository), it is not UTF-8, or it is over
    /// the viewer's 2 MB. One git process.
    fn index_text(&self, dir: &Path, path: &str) -> Option<String>;

    /// Stages the whole `paths` (relative to `dir`): new, modified and deleted files.
    fn stage(&self, dir: &Path, paths: &[String]) -> io::Result<()>;

    /// Puts the index entries of `paths` back to HEAD (out of the index before the first
    /// commit); the working copies are kept.
    fn unstage(&self, dir: &Path, paths: &[String]) -> io::Result<()>;

    /// `git apply --cached [-R]` of `patch`, whose paths are relative to `dir`.
    fn apply_cached(&self, dir: &Path, repo: &Repo, patch: &str, reverse: bool) -> io::Result<()>;

    /// Working copies of the tracked `paths` back to their index version.
    fn discard(&self, dir: &Path, paths: &[String]) -> io::Result<()>;

    /// Commits the index (never amends); returns the new commit.
    fn commit(&self, dir: &Path, message: &str) -> io::Result<String>;

    /// Up to `limit` commits reachable from HEAD that touch `path` (relative to `dir`; the
    /// whole folder when `None`), newest first. HEAD must exist.
    fn log(&self, dir: &Path, limit: u32, path: Option<&str>) -> io::Result<Vec<Commit>>;

    /// The commit `id` (a full commit id) itself.
    fn commit_info(&self, dir: &Path, id: &str) -> io::Result<Commit>;

    /// The branch HEAD is on (also unborn) and its upstream; `None` on a detached HEAD.
    fn current_branch(&self, dir: &Path) -> io::Result<Option<Branch>>;

    /// `git fetch` from the default remote (network).
    fn fetch(&self, dir: &Path) -> io::Result<()>;

    /// Pushes `branch` to `upstream`, fast-forward only (network). Never forces.
    fn push(&self, dir: &Path, branch: &str, upstream: &Upstream) -> io::Result<()>;

    /// The index file of the (worktree of the) repo containing `dir`.
    fn index_path(&self, dir: &Path) -> io::Result<PathBuf>;

    /// A tree of the whole working state of the repo containing `dir` (tracked and untracked,
    /// not ignored), written through a temp copy of its `index`: HEAD, branches, the real index
    /// and the files are untouched (SPEC §20C.1).
    fn snapshot(&self, dir: &Path, index: &Path) -> io::Result<String>;

    /// Points `name` (a full ref outside `refs/heads`) at `object`, keeping it reachable.
    fn update_ref(&self, dir: &Path, name: &str, object: &str) -> io::Result<()>;

    /// Deletes every ref under `prefix` (a full ref prefix ending in `/`).
    fn delete_refs(&self, dir: &Path, prefix: &str) -> io::Result<()>;

    /// Writes the `paths` (relative to `dir`) as they are in `tree` to the working copy,
    /// through a temp index next to `index`: HEAD and the real index are untouched (SPEC
    /// §20C.2).
    fn checkout_tree(
        &self,
        dir: &Path,
        index: &Path,
        tree: &str,
        paths: &[String],
    ) -> io::Result<()>;

    /// The root folder of the (worktree of the) repo containing `dir`.
    fn toplevel(&self, dir: &Path) -> io::Result<PathBuf>;

    /// `git worktree add -b <branch> <path> HEAD` from the repo containing `dir`: a new branch
    /// checked out in a new folder; the user's checkout is untouched (SPEC §20E.1, ADR 0010).
    fn worktree_add(&self, dir: &Path, path: &Path, branch: &str) -> io::Result<()>;

    /// The `paths` (relative to `dir`) that `.gitignore` rules ignore; tracked files never are.
    fn ignored(&self, dir: &Path, paths: &[String]) -> io::Result<Vec<String>>;

    /// Repo-relative paths with staged or unstaged changes anywhere in the (worktree of the)
    /// repo containing `dir`; with `untracked`, untracked non-ignored files too.
    fn uncommitted(&self, dir: &Path, untracked: bool) -> io::Result<Vec<String>>;

    /// `git merge --no-ff --no-edit <branch>` in the checkout containing `dir`. On conflict
    /// runs `git merge --abort` and returns the conflicting repo-relative paths (ADR 0010).
    /// Refused while a merge, cherry-pick or revert is in progress there.
    fn merge(&self, dir: &Path, branch: &str) -> io::Result<Merged>;

    /// `git worktree remove <path>` (never `--force`: git refuses a dirty worktree).
    fn worktree_remove(&self, dir: &Path, path: &Path) -> io::Result<()>;

    /// `git worktree prune`: forgets worktrees whose folder is gone.
    fn worktree_prune(&self, dir: &Path) -> io::Result<()>;

    /// `git branch -d <branch>` (never `-D`); `false` when git keeps it (not merged). A branch
    /// that no longer exists counts as deleted.
    fn delete_branch(&self, dir: &Path, branch: &str) -> io::Result<bool>;
}

/// Result of [`GitBackend::merge`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merged {
    Done,
    /// Aborted; the checkout is as before.
    Conflicts(Vec<String>),
    /// Not done, and why: another merge in progress, or git refused (e.g. an untracked file
    /// in the way).
    Refused(String),
}

/// A local branch: its full ref name (`refs/heads/…`) and where it pushes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    pub upstream: Option<Upstream>,
}

/// `branch.<name>.remote` and `branch.<name>.merge`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    /// A remote name or URL; `.` = this repo.
    pub remote: String,
    /// The full ref name on the remote.
    pub merge: String,
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
    /// One tree (or commit) → another.
    Between(&'a str, &'a str),
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

    fn changed_files(
        &self,
        dir: &Path,
        commit: &str,
        to: Option<&str>,
    ) -> io::Result<Vec<Changed>> {
        let mut command = git(dir);
        command
            .args(["diff", "--name-status", "-z", "--no-renames", "--relative"])
            .arg(commit)
            .args(to)
            .args(["--", "."]);
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

    fn diff(&self, dir: &Path, commit: &str, to: Option<&str>) -> io::Result<String> {
        let mut command = git(dir);
        command
            .args([
                "diff",
                "--no-color",
                "--no-ext-diff",
                "--no-renames",
                "--relative",
            ])
            .arg(commit)
            .args(to)
            .args(["--", "."]);
        run(command).map(text)
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
        peeled(dir, rev, "commit")
    }

    fn tree_of(&self, dir: &Path, rev: &str) -> io::Result<Option<String>> {
        peeled(dir, rev, "tree")
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
        run(command).map(text)
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
        Ok(text(output.stdout))
    }

    fn is_tracked(&self, dir: &Path, path: &str) -> io::Result<bool> {
        let mut command = view_git(dir);
        command.args(["--literal-pathspecs", "ls-files", "-z", "--", path]);
        Ok(!run(command)?.is_empty())
    }

    fn index_text(&self, dir: &Path, path: &str) -> Option<String> {
        // `:./` = stage 0 of the index, relative to `dir` (the project may be a subfolder).
        let mut command = view_git(dir);
        command.args(["cat-file", "blob", &format!(":./{path}")]);
        let bytes = run(command).ok()?;
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > crate::docs::MAX_BYTES {
            return None;
        }
        String::from_utf8(bytes).ok()
    }

    fn stage(&self, dir: &Path, paths: &[String]) -> io::Result<()> {
        let mut command = git(dir);
        command.args(["--literal-pathspecs", "add", "-A"]);
        run_with_paths(command, paths)
    }

    fn unstage(&self, dir: &Path, paths: &[String]) -> io::Result<()> {
        let mut command = git(dir);
        // Also works on an unborn branch (the paths leave the index).
        command.args(["--literal-pathspecs", "reset", "-q"]);
        run_with_paths(command, paths)
    }

    fn apply_cached(&self, dir: &Path, repo: &Repo, patch: &str, reverse: bool) -> io::Result<()> {
        let mut command = git(dir);
        command.args(["apply", "--cached"]);
        if reverse {
            command.arg("-R");
        }
        // Patch paths are read from the repo root.
        let prefix = repo.prefix.trim_end_matches('/');
        if !prefix.is_empty() {
            command.arg(format!("--directory={prefix}"));
        }
        command.arg("-");
        run_with_input(command, patch.as_bytes()).map(drop)
    }

    fn discard(&self, dir: &Path, paths: &[String]) -> io::Result<()> {
        let mut command = git(dir);
        command.args(["--literal-pathspecs", "restore", "--worktree"]);
        run_with_paths(command, paths)
    }

    fn commit(&self, dir: &Path, message: &str) -> io::Result<String> {
        let mut command = git(dir);
        command.args(["commit", "-q", "-F", "-"]);
        run_with_input(command, message.as_bytes())?;
        self.head(dir)?
            .ok_or_else(|| io::Error::other("git commit left no HEAD"))
    }

    fn log(&self, dir: &Path, limit: u32, path: Option<&str>) -> io::Result<Vec<Commit>> {
        let mut command = log_command(dir);
        // The History graph (SPEC §12B) needs parents among the listed commits (`--parents`
        // rewrites them past commits the path filter hides; the format prints them as `%P`)
        // and children before parents. `--topo-order` would give that but makes git walk the
        // whole history when there is no commit-graph, so we fix the order up ourselves.
        command.arg(format!("--max-count={limit}")).args([
            "--parents",
            "HEAD",
            "--",
            path.unwrap_or("."),
        ]);
        Ok(mkrew_core::git::children_first(mkrew_core::git::parse_log(
            &run(command)?,
        )))
    }

    fn commit_info(&self, dir: &Path, id: &str) -> io::Result<Commit> {
        let mut command = log_command(dir);
        command.args(["--no-walk", id, "--"]);
        mkrew_core::git::parse_log(&run(command)?)
            .into_iter()
            .next()
            .ok_or_else(|| io::Error::other(format!("git log showed no commit {id}")))
    }

    fn current_branch(&self, dir: &Path) -> io::Result<Option<Branch>> {
        let output = view_git(dir)
            .args(["symbolic-ref", "-q", "HEAD"])
            .output()?;
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if !output.status.success() || name.is_empty() {
            return Ok(None);
        }
        // Read from the config: `%(upstream:…)` of for-each-ref is empty on an unborn branch.
        let config = |key: &str| -> io::Result<Option<String>> {
            let short = name.strip_prefix("refs/heads/").unwrap_or(&name);
            let output = view_git(dir)
                .args(["config", "--get"])
                .arg(format!("branch.{short}.{key}"))
                .output()?;
            let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            Ok((output.status.success() && !value.is_empty()).then_some(value))
        };
        let upstream = match (config("remote")?, config("merge")?) {
            (Some(remote), Some(merge)) => Some(Upstream { remote, merge }),
            _ => None,
        };
        Ok(Some(Branch { name, upstream }))
    }

    fn fetch(&self, dir: &Path) -> io::Result<()> {
        let mut command = network_git(dir);
        command.args(["fetch", "--quiet"]);
        run(command).map(drop)
    }

    fn push(&self, dir: &Path, branch: &str, upstream: &Upstream) -> io::Result<()> {
        let mut command = network_git(dir);
        // An explicit refspec without `+`: only this branch, only fast-forward, whatever the
        // user's `push.default` or `remote.<name>.push` say.
        command
            .args(["push", "--quiet", "--"])
            .arg(&upstream.remote)
            .arg(format!("{branch}:{}", upstream.merge));
        run(command).map(drop)
    }

    fn index_path(&self, dir: &Path) -> io::Result<PathBuf> {
        // `--git-path` follows worktrees; relative output is relative to `dir` (joining an
        // absolute path keeps it as is).
        let mut command = git(dir);
        command
            .env_remove("GIT_INDEX_FILE")
            .args(["rev-parse", "--git-path", "index"]);
        Ok(dir.join(String::from_utf8_lossy(&run(command)?).trim()))
    }

    fn snapshot(&self, dir: &Path, index: &Path) -> io::Result<String> {
        let temp = TempIndex::copy_of(index)?;
        let with_temp = || {
            let mut command = git(dir);
            command.env("GIT_INDEX_FILE", &temp.0);
            command
        };
        let mut add = with_temp();
        // `:/` = the whole repo, whatever folder `dir` is.
        add.args(["add", "-A", "--", ":/"]);
        run(add)?;
        let mut write = with_temp();
        write.arg("write-tree");
        Ok(String::from_utf8_lossy(&run(write)?).trim().to_owned())
    }

    fn update_ref(&self, dir: &Path, name: &str, object: &str) -> io::Result<()> {
        let mut command = git(dir);
        command.args(["update-ref", name, object]);
        run(command).map(drop)
    }

    fn delete_refs(&self, dir: &Path, prefix: &str) -> io::Result<()> {
        let mut list = git(dir);
        list.args(["for-each-ref", "--format=delete %(refname)", prefix]);
        let deletes = run(list)?;
        if deletes.is_empty() {
            return Ok(());
        }
        let mut delete = git(dir);
        delete.args(["update-ref", "--stdin"]);
        run_with_input(delete, &deletes).map(drop)
    }

    fn checkout_tree(
        &self,
        dir: &Path,
        index: &Path,
        tree: &str,
        paths: &[String],
    ) -> io::Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let temp = TempIndex::next_to(index);
        let with_temp = || {
            let mut command = git(dir);
            command.env("GIT_INDEX_FILE", &temp.0);
            command
        };
        let mut read = with_temp();
        read.args(["read-tree", tree]);
        run(read)?;
        // Paths on stdin are relative to `dir`, like command-line ones.
        let mut checkout = with_temp();
        checkout.args(["checkout-index", "-f", "-z", "--stdin"]);
        run_with_input(checkout, &nul_joined(paths)).map(drop)
    }

    fn toplevel(&self, dir: &Path) -> io::Result<PathBuf> {
        let mut command = git(dir);
        command.args(["rev-parse", "--show-toplevel"]);
        Ok(PathBuf::from(
            String::from_utf8_lossy(&run(command)?).trim(),
        ))
    }

    fn worktree_add(&self, dir: &Path, path: &Path, branch: &str) -> io::Result<()> {
        let mut command = git(dir);
        command
            .args(["worktree", "add", "--quiet", "-b", branch])
            .arg(path)
            .arg("HEAD");
        run(command).map(drop)
    }

    fn ignored(&self, dir: &Path, paths: &[String]) -> io::Result<Vec<String>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let mut command = git(dir);
        command.args(["check-ignore", "-z", "--stdin"]);
        let output = output_with_input(command, &nul_joined(paths))?;
        // Exit 1 = none of them is ignored.
        if output.status.code() == Some(1) {
            return Ok(Vec::new());
        }
        failed_unless_success(output).map(|out| nul_separated(&out))
    }

    fn uncommitted(&self, dir: &Path, untracked: bool) -> io::Result<Vec<String>> {
        let mut command = view_git(dir);
        // `:/` = the whole repo, not just `dir`.
        command
            .args(["status", "--porcelain=v1", "-z", "--ignore-submodules=none"])
            .arg(if untracked {
                "--untracked-files=all"
            } else {
                "--untracked-files=no"
            })
            .args(["--", ":/"]);
        let out = run(command)?;
        let mut paths = Vec::new();
        let mut records = out.split(|b| *b == 0).filter(|r| !r.is_empty());
        while let Some(record) = records.next() {
            // `XY path`; a rename or copy is followed by its source path.
            if record.len() < 4 {
                continue;
            }
            paths.push(String::from_utf8_lossy(&record[3..]).into_owned());
            if matches!(record[0], b'R' | b'C') || matches!(record[1], b'R' | b'C') {
                records.next();
            }
        }
        Ok(paths)
    }

    fn merge(&self, dir: &Path, branch: &str) -> io::Result<Merged> {
        // Never `merge --abort` a merge (or pick, revert) the user has in progress.
        for head in ["MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD"] {
            if peeled(dir, head, "commit")?.is_some() {
                let why = format!("the project's checkout has a {head} in progress");
                return Ok(Merged::Refused(why));
            }
        }
        let output = git(dir)
            .env("GIT_MERGE_AUTOEDIT", "no")
            .args(["merge", "--no-ff", "--no-edit", "--quiet", branch])
            .stderr(Stdio::piped())
            .output()?;
        if output.status.success() {
            return Ok(Merged::Done);
        }
        let mut conflicts = git(dir);
        conflicts.args(["diff", "--name-only", "--diff-filter=U", "-z", "--", ":/"]);
        // Abort first, whatever listing the conflicts gave.
        let conflicts = run(conflicts).map(|out| nul_separated(&out));
        if peeled(dir, "MERGE_HEAD", "commit")?.is_some() {
            let mut abort = git(dir);
            abort.args(["merge", "--abort"]);
            run(abort)?;
        }
        let conflicts = conflicts?;
        if conflicts.is_empty() {
            // Refused before it started (e.g. an untracked file in the way), or failed after.
            let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            return Ok(Merged::Refused(format!("git merge failed: {message}")));
        }
        Ok(Merged::Conflicts(conflicts))
    }

    fn worktree_remove(&self, dir: &Path, path: &Path) -> io::Result<()> {
        let mut command = git(dir);
        command.args(["worktree", "remove"]).arg(path);
        run(command).map(drop)
    }

    fn worktree_prune(&self, dir: &Path) -> io::Result<()> {
        let mut command = git(dir);
        command.args(["worktree", "prune"]);
        run(command).map(drop)
    }

    fn delete_branch(&self, dir: &Path, branch: &str) -> io::Result<bool> {
        if peeled(dir, &format!("refs/heads/{branch}"), "commit")?.is_none() {
            return Ok(true);
        }
        let output = git(dir)
            .args(["branch", "-d", branch])
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            let message = String::from_utf8_lossy(&output.stderr);
            tracing::info!(%branch, "branch kept: {}", message.trim());
        }
        Ok(output.status.success())
    }
}

fn nul_joined(paths: &[String]) -> Vec<u8> {
    let mut input = Vec::new();
    for path in paths {
        input.extend_from_slice(path.as_bytes());
        input.push(0);
    }
    input
}

/// A private copy of an index next to it, deleted on drop.
struct TempIndex(PathBuf);

impl TempIndex {
    /// A fresh name next to `index`; git creates the file.
    fn next_to(index: &Path) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut name = index.as_os_str().to_owned();
        name.push(format!(".mkrew-snapshot-{}-{n}", std::process::id()));
        Self(PathBuf::from(name))
    }

    /// Copies `index` (a missing one, e.g. a fresh repo, stays missing: git starts empty).
    fn copy_of(index: &Path) -> io::Result<Self> {
        let temp = Self::next_to(index);
        match std::fs::copy(index, &temp.0) {
            Ok(_) => Ok(temp),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(temp),
            Err(e) => Err(e),
        }
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        // Missing when git never wrote it; nothing else to do on failure.
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `git log` printing [`mkrew_core::git::LOG_FORMAT`] records, before the revisions. A user's
/// `log.showSignature` (extra output) and `log.follow` (renames) are switched off.
fn log_command(dir: &Path) -> Command {
    let mut command = view_git(dir);
    command
        .args(["--literal-pathspecs", "log", "-z", "--no-color"])
        .args(["--no-show-signature", "--no-follow"])
        .arg(format!("--format={}", mkrew_core::git::LOG_FORMAT));
    command
}

/// [`git`] for commands talking to a remote: no terminal prompt (stdin is null), so missing
/// credentials fail instead of hanging.
fn network_git(dir: &Path) -> Command {
    let mut command = git(dir);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

/// Runs `command` with `paths` passed NUL-separated on stdin (no command-line length limit).
fn run_with_paths(mut command: Command, paths: &[String]) -> io::Result<()> {
    command.args(["--pathspec-from-file=-", "--pathspec-file-nul"]);
    run_with_input(command, &nul_joined(paths)).map(drop)
}

/// [`run`] with `input` written to stdin. Git reads all of it before writing much, so writing
/// first and reading afterwards cannot deadlock.
fn run_with_input(command: Command, input: &[u8]) -> io::Result<Vec<u8>> {
    failed_unless_success(output_with_input(command, input)?)
}

/// Runs `command` with `input` on stdin; any exit status is returned.
fn output_with_input(mut command: Command, input: &[u8]) -> io::Result<std::process::Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write as _;
        match stdin.write_all(input) {
            // Git exited early (e.g. a bad patch): its stderr says why.
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => {}
            result => result?,
        }
    }
    child.wait_with_output()
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
        Side::Between(from, to) => {
            command.args([from, to]);
        }
    }
    command
}

/// The object of type `kind` that `rev` names (`rev^{kind}`); `None` if it names none.
fn peeled(dir: &Path, rev: &str, kind: &str) -> io::Result<Option<String>> {
    let output = view_git(dir)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("{rev}^{{{kind}}}"))
        .output()?;
    let id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((output.status.success() && !id.is_empty()).then_some(id))
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
    failed_unless_success(command.stderr(Stdio::piped()).output()?)
}

/// The output's stdout, or an error carrying git's stderr (its stdout when stderr is empty,
/// e.g. `git commit` with nothing staged).
fn failed_unless_success(output: std::process::Output) -> io::Result<Vec<u8>> {
    if !output.status.success() {
        let mut message = String::from_utf8_lossy(&output.stderr);
        if message.trim().is_empty() {
            message = String::from_utf8_lossy(&output.stdout);
        }
        return Err(io::Error::other(format!("git failed: {}", message.trim())));
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

/// Git's output as text without copying it when it is UTF-8 (diffs can be hundreds of MB).
fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
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
            GitCli.changed_files(&dir, &commit, None).unwrap(),
            [changed("edited.txt", false), changed("staged.txt", true)]
        );
        let diff = GitCli.diff(&dir, &commit, None).unwrap();
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
        assert_eq!(GitCli.changed_files(&dir, &commit, None).unwrap(), []);
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

    /// `git diff` output of `dir` for one side, for building hunk patches.
    fn hunks(dir: &Path, path: &str, side: Side<'_>) -> Vec<mkrew_protocol::Hunk> {
        let text = GitCli.file_diff(dir, path, side).unwrap();
        let mut files = mkrew_core::git::parse_diff(&text);
        assert_eq!(files.len(), 1, "{text}");
        files.remove(0).hunks
    }

    /// The project is a subfolder: hunk patches get the repo prefix; stage/unstage by path
    /// works before the first commit; discard keeps the index; commit returns the new HEAD.
    #[test]
    fn stage_unstage_discard_and_commit_in_a_subfolder() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let dir = root.join("app");
        std::fs::create_dir(&dir).unwrap();
        run(root, &["init", "-q"]);
        run(root, &["config", "core.autocrlf", "false"]);
        run(root, &["config", "user.name", "t"]);
        run(root, &["config", "user.email", "t@t"]);
        run(root, &["config", "commit.gpgsign", "false"]);
        let lines: Vec<String> = (1..=20).map(|n| format!("line {n}\n")).collect();
        std::fs::write(dir.join("a b.txt"), lines.concat()).unwrap();
        std::fs::write(dir.join("c.txt"), "c\n").unwrap();
        let paths = |names: &[&str]| names.iter().map(|&n| n.to_owned()).collect::<Vec<_>>();
        let staged = || run(root, &["diff", "--cached", "--name-only"]);

        // Unborn branch: by path in and out of the index again.
        GitCli.stage(&dir, &paths(&["a b.txt", "c.txt"])).unwrap();
        assert_eq!(staged(), "app/a b.txt\napp/c.txt");
        GitCli.unstage(&dir, &paths(&["c.txt"])).unwrap();
        assert_eq!(staged(), "app/a b.txt");
        let first = GitCli.commit(&dir, "one\n\nbody").unwrap();
        assert_eq!(GitCli.head(&dir).unwrap(), Some(first.clone()));
        assert_eq!(run(root, &["log", "-1", "--format=%B"]), "one\n\nbody");

        // Two hunks; stage only the second, then unstage it with the staged diff's hunk.
        let mut changed = lines.clone();
        changed[1] = "line two\n".into();
        changed[18] = "line nineteen\n".into();
        std::fs::write(dir.join("a b.txt"), changed.concat()).unwrap();
        let unstaged = hunks(&dir, "a b.txt", Side::Unstaged);
        assert_eq!(unstaged.len(), 2);
        let repo_info = GitCli.repo(&dir).unwrap().unwrap();
        let patch = mkrew_core::git::hunk_patch("a b.txt", &unstaged[1]);
        GitCli
            .apply_cached(&dir, &repo_info, &patch, false)
            .unwrap();
        let index = hunks(&dir, "a b.txt", Side::Staged);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].lines, unstaged[1].lines);
        assert_eq!(
            hunks(&dir, "a b.txt", Side::Unstaged),
            [unstaged[0].clone()]
        );
        let patch = mkrew_core::git::hunk_patch("a b.txt", &index[0]);
        GitCli.apply_cached(&dir, &repo_info, &patch, true).unwrap();
        assert_eq!(staged(), "");
        // A hunk that no longer matches is refused with git's reason.
        let error = GitCli
            .apply_cached(&dir, &repo_info, &patch, true)
            .unwrap_err();
        assert!(error.to_string().starts_with("git failed: "), "{error}");

        // Stage one hunk, then discard: the working copy goes back to the index.
        let patch = mkrew_core::git::hunk_patch("a b.txt", &unstaged[0]);
        GitCli
            .apply_cached(&dir, &repo_info, &patch, false)
            .unwrap();
        GitCli.discard(&dir, &paths(&["a b.txt"])).unwrap();
        let mut expected = lines.clone();
        expected[1] = "line two\n".into();
        assert_eq!(
            std::fs::read_to_string(dir.join("a b.txt")).unwrap(),
            expected.concat()
        );
        assert_eq!(staged(), "app/a b.txt");

        let second = GitCli.commit(&dir, "two").unwrap();
        assert_ne!(second, first);
        assert_eq!(staged(), "");
        // Nothing staged: git's explanation is on stdout.
        let error = GitCli.commit(&dir, "three").unwrap_err().to_string();
        assert!(error.len() > "git failed: ".len(), "{error}");
    }

    /// The index's text, not the working tree's or HEAD's, also when the project is a
    /// subfolder of the repo; untracked, unknown and out-of-repo files have none.
    #[test]
    fn index_text_reads_the_staged_version() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let dir = root.join("app");
        std::fs::create_dir(&dir).unwrap();
        run(root, &["init", "-q"]);
        run(root, &["config", "core.autocrlf", "false"]);
        std::fs::write(dir.join("a b.txt"), "one\n").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "one"]);
        std::fs::write(dir.join("a b.txt"), "two\n").unwrap();
        run(root, &["add", "."]);
        std::fs::write(dir.join("a b.txt"), "three\n").unwrap();
        std::fs::write(dir.join("new.txt"), "new\n").unwrap();

        let text = GitCli.index_text(&dir, "a b.txt");
        assert_eq!(text.as_deref(), Some("two\n"));
        assert_eq!(GitCli.index_text(&dir, "new.txt"), None);
        assert_eq!(GitCli.index_text(&dir, "gone.txt"), None);
        let outside = tempfile::tempdir().unwrap();
        assert_eq!(GitCli.index_text(outside.path(), "x"), None);
    }

    /// Commit dates can be wrong (clock skew): the log still lists every commit before its
    /// parents, which the History graph relies on.
    #[test]
    fn log_lists_children_before_their_parents() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        run(root, &["init", "-q", "-b", "main"]);
        run(root, &["config", "core.autocrlf", "false"]);
        let at = |date: &str, args: &[&str]| {
            let output = std::process::Command::new("git")
                .current_dir(root)
                .args(["-c", "user.name=t", "-c", "user.email=t@t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args)
                .env("GIT_AUTHOR_DATE", date)
                .env("GIT_COMMITTER_DATE", date)
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?} failed");
        };
        let change = |name: &str, date: &str| {
            std::fs::write(root.join(name), name).unwrap();
            run(root, &["add", "."]);
            at(date, &["commit", "-q", "-m", name]);
        };
        change("a", "@1000 +0000");
        run(root, &["checkout", "-q", "-b", "side"]);
        change("x", "@500 +0000"); // older than its parent `a`
        run(root, &["checkout", "-q", "main"]);
        change("b", "@2000 +0000");
        at(
            "@3000 +0000",
            &["merge", "-q", "--no-ff", "--no-edit", "side"],
        );

        let commits = GitCli.log(root, 50, None).unwrap();
        assert_eq!(commits.len(), 4);
        let index = |hash: &str| commits.iter().position(|commit| commit.hash == hash);
        for (at, commit) in commits.iter().enumerate() {
            for parent in &commit.parents {
                assert!(
                    index(parent).is_some_and(|p| p > at),
                    "{} before a parent",
                    commit.summary
                );
            }
        }
    }

    /// A project in a subfolder lists only the commits touching it; their parents are the
    /// listed commits (not the hidden ones in between), so graph lanes join.
    #[test]
    fn log_of_a_subfolder_rewrites_parents_to_listed_commits() {
        let repo = tempfile::tempdir().unwrap();
        let root = repo.path();
        let dir = root.join("app");
        std::fs::create_dir(&dir).unwrap();
        run(root, &["init", "-q"]);
        run(root, &["config", "core.autocrlf", "false"]);
        std::fs::write(dir.join("a"), "1").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "one"]);
        std::fs::write(root.join("other"), "x").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "outside"]);
        std::fs::write(dir.join("a"), "2").unwrap();
        run(root, &["add", "."]);
        run(root, &["commit", "-q", "-m", "two"]);

        let commits = GitCli.log(&dir, 50, None).unwrap();
        let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(summaries, ["two", "one"]);
        assert_eq!(commits[0].parents, [commits[1].hash.clone()]);
    }

    #[test]
    fn failures_carry_git_stderr() {
        let dir = tempfile::tempdir().unwrap();
        run(dir.path(), &["init", "-q"]);
        let missing = "0123456789abcdef0123456789abcdef01234567";
        let error = GitCli.diff(dir.path(), missing, None).unwrap_err();
        assert!(error.to_string().starts_with("git failed: "), "{error}");
        assert!(GitCli.restore(dir.path(), missing).is_err());
    }
}
