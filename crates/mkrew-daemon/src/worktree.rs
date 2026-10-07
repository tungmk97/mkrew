//! Task worktrees (SPEC §20E.1, ADR 0010): a `mkrew/…` branch checked out in its own folder
//! under the data dir, plus copies of the git-ignored files `.worktreeinclude` names.
//! Merge back and removal are user actions (ADR 0010). Blocking: callers run every function
//! but the pure ones in `spawn_blocking`.

use std::io;
use std::path::{Component, Path, PathBuf};

use mkrew_core::Paths;
use ulid::Ulid;

use crate::git::{GitBackend, Merged};

/// Longest branch slug (SPEC §20E.1).
const MAX_SLUG: usize = 30;

/// The repo-root file listing what to copy into a new worktree.
const INCLUDE_FILE: &str = ".worktreeinclude";

/// `<data>/worktrees/<project_id>/<task_id>`.
pub fn dir(paths: &Paths, project: Ulid, task: Ulid) -> PathBuf {
    paths
        .worktrees_dir()
        .join(project.to_string())
        .join(task.to_string())
}

/// `mkrew/<slug>-<short ulid>`: the slug of `title`, then the end of the task id (its random
/// part, so tasks created in the same millisecond still differ).
pub fn branch_name(title: &str, task: Ulid) -> String {
    let id = task.to_string().to_ascii_lowercase();
    format!("mkrew/{}-{}", slug(title), &id[id.len() - 8..])
}

/// ASCII lowercase letters and digits, every other run of characters one `-`, no `-` at either
/// end, at most [`MAX_SLUG`] characters; `task` when nothing is left.
pub fn slug(title: &str) -> String {
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    // ASCII only: byte length = character count.
    slug.truncate(MAX_SLUG);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "task".into()
    } else {
        slug.into()
    }
}

/// The entries of a `.worktreeinclude`: one literal relative path per line (a trailing `/` is
/// dropped), blank lines and `#` comments skipped.
pub fn parse_include(text: &str) -> Vec<String> {
    // Some Windows editors start the file with a byte order mark.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.trim_end_matches(['/', '\\']).to_owned())
        .filter(|line| !line.is_empty())
        .collect()
}

/// Adds the worktree of `branch` at `dest` from HEAD of the repo containing `project_dir`, then
/// copies the `.worktreeinclude` entries into it. Returns one warning per entry that was
/// skipped (not a path inside the repo, missing, not ignored, copy failed).
pub fn create(
    git: &dyn GitBackend,
    project_dir: &Path,
    dest: &Path,
    branch: &str,
) -> io::Result<Vec<String>> {
    let root = git.toplevel(project_dir)?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    git.worktree_add(project_dir, dest, branch)?;
    let entries = match std::fs::read_to_string(root.join(INCLUDE_FILE)) {
        Ok(text) => parse_include(&text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Ok(vec![format!("{INCLUDE_FILE}: {e}")]),
    };
    let mut warnings = Vec::new();
    let mut candidates = Vec::new();
    for entry in entries {
        if !inside(&entry) {
            warnings.push(format!("{entry}: not a relative path inside the repo"));
        } else if std::fs::symlink_metadata(root.join(&entry)).is_err() {
            warnings.push(format!("{entry}: not found"));
        } else {
            candidates.push(entry);
        }
    }
    // The worktree exists now: a failure here only skips the copies.
    let ignored = match git.ignored(&root, &candidates) {
        Ok(ignored) => ignored,
        Err(e) => {
            warnings.push(format!("{INCLUDE_FILE}: {e}"));
            return Ok(warnings);
        }
    };
    for entry in candidates {
        if !ignored.contains(&entry) {
            warnings.push(format!("{entry}: not ignored by git"));
        } else if let Err(e) = copy(&root.join(&entry), &dest.join(&entry)) {
            warnings.push(format!("{entry}: {e}"));
        }
    }
    Ok(warnings)
}

/// The project's folder inside the worktree at `root`: the same subfolder of the repo as
/// `project_dir` is of the user's checkout; `None` when `project_dir` is not in a repo.
pub fn project_dir(
    git: &dyn GitBackend,
    project_dir: &Path,
    root: &Path,
) -> io::Result<Option<PathBuf>> {
    let Some(repo) = git.repo(project_dir)? else {
        return Ok(None);
    };
    // One component at a time: no `/` inside a Windows path.
    let parts = repo.prefix.split('/').filter(|part| !part.is_empty());
    Ok(Some(
        parts.fold(root.to_path_buf(), |dir, part| dir.join(part)),
    ))
}

/// Outcome of [`merge`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Merge {
    Merged,
    /// Aborted: the user's checkout is as before. Repo-relative paths.
    Conflicts(Vec<String>),
    /// Not started, and why.
    Refused(String),
}

/// Merges `branch` (checked out in the worktree at `root`) into the user's checkout holding
/// `project_dir` (SPEC §20E.1): only when neither side has uncommitted changes (untracked
/// files count in the worktree: they would not be merged) and the checkout is on a branch.
pub fn merge(
    git: &dyn GitBackend,
    project_dir: &Path,
    root: &Path,
    branch: &str,
) -> io::Result<Merge> {
    if !root.is_dir() {
        return Ok(Merge::Refused(format!(
            "worktree {} is missing",
            root.display()
        )));
    }
    let dirty = git.uncommitted(root, true)?;
    if !dirty.is_empty() {
        return Ok(Merge::Refused(format!(
            "the worktree has uncommitted changes ({}); commit them there first",
            listed(&dirty)
        )));
    }
    let dirty = git.uncommitted(project_dir, false)?;
    if !dirty.is_empty() {
        return Ok(Merge::Refused(format!(
            "the project's checkout has uncommitted changes ({})",
            listed(&dirty)
        )));
    }
    if git.current_branch(project_dir)?.is_none() {
        let message = "the project's checkout is on a detached HEAD";
        return Ok(Merge::Refused(message.into()));
    }
    Ok(match git.merge(project_dir, branch)? {
        Merged::Done => Merge::Merged,
        Merged::Conflicts(files) => Merge::Conflicts(files),
        Merged::Refused(why) => Merge::Refused(why),
    })
}

/// Outcome of [`remove`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remove {
    /// `branch_kept`: git refused `branch -d` (not merged).
    Removed { branch_kept: bool },
    /// Nothing done: the worktree's uncommitted files (repo-relative).
    Dirty(Vec<String>),
}

/// Removes the worktree at `root` of the repo holding `project_dir`, then deletes its branch
/// unless it is not merged (SPEC §20E.1). A folder already deleted by hand is pruned.
/// `release` runs right before git deletes the folder (to close handles into it).
pub fn remove(
    git: &dyn GitBackend,
    project_dir: &Path,
    root: &Path,
    branch: Option<&str>,
    release: impl FnOnce(),
) -> io::Result<Remove> {
    if root.is_dir() {
        let dirty = git.uncommitted(root, true)?;
        if !dirty.is_empty() {
            return Ok(Remove::Dirty(dirty));
        }
        release();
        git.worktree_remove(project_dir, root)?;
    } else {
        git.worktree_prune(project_dir)?;
    }
    let branch_kept = match branch {
        Some(branch) => !git.delete_branch(project_dir, branch)?,
        None => false,
    };
    Ok(Remove::Removed { branch_kept })
}

/// The first few `paths`, comma-separated, with a count of the rest.
pub fn listed(paths: &[String]) -> String {
    const SHOWN: usize = 5;
    let mut text = paths[..paths.len().min(SHOWN)].join(", ");
    if paths.len() > SHOWN {
        text.push_str(&format!(" and {} more", paths.len() - SHOWN));
    }
    text
}

/// Relative, with only normal components: it cannot leave the folder it is joined to.
fn inside(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Copies a file, or a folder recursively (links inside a folder are copied as what they point
/// to; a link to a folder fails). A file already in the worktree is never overwritten: git
/// calls a folder holding tracked files not ignored, so this is only a safety net.
fn copy(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if !std::fs::symlink_metadata(from)?.is_dir() {
        if std::fs::symlink_metadata(to).is_ok() {
            return Ok(());
        }
        return std::fs::copy(from, to).map(drop);
    }
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        copy(&entry.path(), &to.join(entry.file_name()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::git::GitCli;

    #[test]
    fn slugs_are_short_ascii_and_dashed() {
        assert_eq!(slug("Fix the login bug"), "fix-the-login-bug");
        assert_eq!(slug("  Add  OAuth (v2)!! "), "add-oauth-v2");
        assert_eq!(slug("Café — déjà vu"), "caf-d-j-vu");
        assert_eq!(slug("日本語"), "task");
        assert_eq!(slug(""), "task");
        assert_eq!(slug("---"), "task");
        // Cut at 30, then no trailing dash.
        assert_eq!(
            slug("Refactor the session managers now"),
            "refactor-the-session-managers"
        );
        assert_eq!(slug(&"a".repeat(40)), "a".repeat(30));
    }

    #[test]
    fn branch_names_end_with_the_random_part_of_the_task_id() {
        let task: Ulid = "01JA2B3C4D5E6F7G8H9JKMNPQR".parse().unwrap();
        assert_eq!(branch_name("Fix it", task), "mkrew/fix-it-9jkmnpqr");
    }

    #[test]
    fn include_files_list_literal_paths() {
        let text = "# secrets\n.env\n\n  config/local.toml  \nnode_modules/\n#.cache\n/\n";
        assert_eq!(
            parse_include(text),
            [".env", "config/local.toml", "node_modules"]
        );
        assert_eq!(parse_include("\u{feff}.env\r\nb\r\n"), [".env", "b"]);
    }

    #[test]
    fn only_relative_paths_inside_the_repo_count() {
        assert!(inside(".env"));
        assert!(inside("a/./b"));
        assert!(!inside("../outside"));
        assert!(!inside("a/../../b"));
        assert!(!inside("/etc/passwd"));
        #[cfg(windows)]
        assert!(!inside(r"C:\Windows"));
    }

    #[test]
    fn worktree_dirs_live_under_the_data_dir() {
        let paths = Paths {
            data_dir: PathBuf::from("data"),
            config_dir: PathBuf::from("config"),
            socket: String::new(),
        };
        let (project, task) = (Ulid::new(), Ulid::new());
        assert_eq!(
            dir(&paths, project, task),
            Path::new("data")
                .join("worktrees")
                .join(project.to_string())
                .join(task.to_string())
        );
    }

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

    /// Everything adding a worktree must leave alone in the user's checkout.
    #[derive(Debug, PartialEq)]
    struct State {
        head: String,
        branch: String,
        index: Vec<u8>,
        status: String,
        files: Vec<(PathBuf, Vec<u8>)>,
    }

    fn state(root: &Path) -> State {
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
            index: std::fs::read(root.join(".git").join("index")).unwrap(),
            status: git(root, &["status", "--porcelain", "--ignored"]),
            files,
        }
    }

    #[test]
    fn creating_a_worktree_branches_from_head_and_leaves_the_checkout_alone() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        let app = root.join("app");
        std::fs::create_dir_all(&app).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "core.autocrlf", "false"]);
        std::fs::write(root.join(".gitignore"), ".env\nlocal/\ncache/\n*.log\n").unwrap();
        std::fs::write(app.join("a.txt"), "a\n").unwrap();
        // A tracked file inside an ignored folder: git no longer calls the folder ignored.
        std::fs::create_dir_all(root.join("cache")).unwrap();
        std::fs::write(root.join("cache").join("tracked.txt"), "committed\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["add", "-f", "cache/tracked.txt"]);
        git(&root, &["commit", "-q", "-m", "one"]);
        let head = git(&root, &["rev-parse", "HEAD"]);
        // Ignored files to copy, staged and unstaged changes that must stay behind.
        std::fs::write(root.join(".env"), "SECRET=1\n").unwrap();
        std::fs::write(root.join("cache").join("tracked.txt"), "edited\n").unwrap();
        std::fs::create_dir_all(root.join("local").join("deep")).unwrap();
        std::fs::write(root.join("local").join("deep").join("k.txt"), "k\n").unwrap();
        std::fs::write(app.join("debug.log"), "log\n").unwrap();
        std::fs::write(app.join("a.txt"), "staged\n").unwrap();
        git(&root, &["add", "app/a.txt"]);
        std::fs::write(app.join("a.txt"), "unstaged\n").unwrap();
        std::fs::write(
            root.join(INCLUDE_FILE),
            "# copy these\n.env\nlocal/\ncache\napp/debug.log\napp/a.txt\nmissing.txt\n../escape\n",
        )
        .unwrap();
        let before = state(&root);
        let dest = temp
            .path()
            .join("data")
            .join("worktrees")
            .join("p")
            .join("t");
        let branch = "mkrew/fix-it-0123abcd";

        // From the project folder (a subfolder of the repo).
        let warnings = create(&GitCli, &app, &dest, branch).unwrap();
        assert_eq!(
            project_dir(&GitCli, &app, &dest).unwrap(),
            Some(dest.join("app"))
        );
        assert_eq!(
            project_dir(&GitCli, &root, &dest).unwrap(),
            Some(dest.clone())
        );

        assert_eq!(state(&root), before);
        assert_eq!(
            warnings,
            [
                "missing.txt: not found",
                "../escape: not a relative path inside the repo",
                "cache: not ignored by git",
                "app/a.txt: not ignored by git",
            ]
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("cache").join("tracked.txt")).unwrap(),
            "committed\n"
        );
        // The worktree: the new branch at the old HEAD, committed content only, plus the copies.
        assert_eq!(
            git(&dest, &["symbolic-ref", "HEAD"]),
            format!("refs/heads/{branch}")
        );
        assert_eq!(git(&dest, &["rev-parse", "HEAD"]), head);
        assert_eq!(git(&root, &["rev-parse", branch]), head);
        assert_eq!(
            std::fs::read_to_string(dest.join("app").join("a.txt")).unwrap(),
            "a\n"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join(".env")).unwrap(),
            "SECRET=1\n"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("local").join("deep").join("k.txt")).unwrap(),
            "k\n"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("app").join("debug.log")).unwrap(),
            "log\n"
        );
        assert!(!dest.join("missing.txt").exists());
        // Copies are ignored there too: the worktree starts clean.
        assert_eq!(git(&dest, &["status", "--porcelain"]), "");
    }

    #[test]
    fn without_an_include_file_nothing_is_copied() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join(".gitignore"), ".env\n").unwrap();
        std::fs::write(root.join(".env"), "x\n").unwrap();
        git(&root, &["add", ".gitignore"]);
        git(&root, &["commit", "-q", "-m", "one"]);
        let dest = temp.path().join("wt");

        let warnings = create(&GitCli, &root, &dest, "mkrew/task-00000000").unwrap();

        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(dest.join(".gitignore").exists());
        assert!(!dest.join(".env").exists());
    }

    #[test]
    fn an_existing_branch_is_an_error_and_nothing_moves() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "one"]);
        git(&root, &["branch", "mkrew/taken-00000000"]);
        let before = state(&root);

        let dest = temp.path().join("wt");
        assert!(create(&GitCli, &root, &dest, "mkrew/taken-00000000").is_err());

        assert_eq!(state(&root), before);
        assert!(!dest.join("a.txt").exists());
    }

    const BRANCH: &str = "mkrew/task-00000000";

    /// A repo on `main` with `a.txt` committed and its worktree of [`BRANCH`] at `<temp>/wt`.
    /// The repo's own config lets the daemon's `git merge` commit.
    fn repo_with_worktree(temp: &Path) -> (PathBuf, PathBuf) {
        let root = temp.join("repo");
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "core.autocrlf", "false"]);
        git(&root, &["config", "user.name", "t"]);
        git(&root, &["config", "user.email", "t@t"]);
        git(&root, &["config", "commit.gpgsign", "false"]);
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-q", "-m", "one"]);
        let dest = temp.join("wt");
        create(&GitCli, &root, &dest, BRANCH).unwrap();
        (root, dest)
    }

    fn commit_file(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
        git(dir, &["add", name]);
        git(dir, &["commit", "-q", "-m", name]);
    }

    fn has_branch(root: &Path) -> bool {
        !git(root, &["branch", "--list", BRANCH]).is_empty()
    }

    #[test]
    fn a_clean_worktree_merges_with_a_merge_commit_and_is_removed_with_its_branch() {
        let temp = tempfile::tempdir().unwrap();
        let (root, wt) = repo_with_worktree(temp.path());
        commit_file(&wt, "b.txt", "from the agent\n");
        let head = git(&root, &["rev-parse", "HEAD"]);
        let tip = git(&wt, &["rev-parse", "HEAD"]);
        // Untracked files in the user's checkout do not block a merge.
        std::fs::write(root.join("notes.txt"), "mine\n").unwrap();

        assert_eq!(merge(&GitCli, &root, &wt, BRANCH).unwrap(), Merge::Merged);

        assert_eq!(git(&root, &["symbolic-ref", "HEAD"]), "refs/heads/main");
        assert_eq!(
            git(&root, &["rev-parse", "HEAD^1", "HEAD^2"]),
            format!("{head}\n{tip}")
        );
        assert_eq!(
            std::fs::read_to_string(root.join("b.txt")).unwrap(),
            "from the agent\n"
        );
        assert_eq!(git(&root, &["status", "--porcelain"]), "?? notes.txt");

        let removed = remove(&GitCli, &root, &wt, Some(BRANCH), || {}).unwrap();
        assert_eq!(removed, Remove::Removed { branch_kept: false });
        assert!(!wt.exists());
        assert!(!has_branch(&root));
        assert_eq!(
            git(&root, &["worktree", "list", "--porcelain"])
                .matches("worktree ")
                .count(),
            1
        );
    }

    #[test]
    fn a_conflicting_merge_is_aborted_and_leaves_the_checkout_alone() {
        let temp = tempfile::tempdir().unwrap();
        let (root, wt) = repo_with_worktree(temp.path());
        commit_file(&wt, "a.txt", "agent\n");
        commit_file(&wt, "b.txt", "agent\n");
        commit_file(&root, "a.txt", "user\n");
        // `merge --abort` rewrites the index: compare its entries, not its bytes.
        let entries = |root: &Path| git(root, &["ls-files", "-s"]).into_bytes();
        let mut before = state(&root);
        before.index = entries(&root);

        let merged = merge(&GitCli, &root, &wt, BRANCH).unwrap();

        assert_eq!(merged, Merge::Conflicts(vec!["a.txt".into()]));
        let mut after = state(&root);
        after.index = entries(&root);
        assert_eq!(after, before);
        assert!(!root.join(".git").join("MERGE_HEAD").exists());
        assert!(has_branch(&root));
    }

    #[test]
    fn merging_is_refused_when_either_side_is_dirty_or_head_is_detached() {
        let temp = tempfile::tempdir().unwrap();
        let (root, wt) = repo_with_worktree(temp.path());
        commit_file(&wt, "b.txt", "agent\n");

        // An untracked file in the worktree would not be merged.
        std::fs::write(wt.join("new.txt"), "x\n").unwrap();
        let before = state(&root);
        let Merge::Refused(why) = merge(&GitCli, &root, &wt, BRANCH).unwrap() else {
            panic!("merged a dirty worktree");
        };
        assert!(why.contains("new.txt"), "{why}");
        std::fs::remove_file(wt.join("new.txt")).unwrap();

        std::fs::write(root.join("a.txt"), "edited\n").unwrap();
        let Merge::Refused(why) = merge(&GitCli, &root, &wt, BRANCH).unwrap() else {
            panic!("merged into a dirty checkout");
        };
        assert!(why.contains("a.txt"), "{why}");
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        assert_eq!(state(&root), before);

        let head = git(&root, &["rev-parse", "HEAD"]);
        git(&root, &["checkout", "-q", "--detach"]);
        let Merge::Refused(why) = merge(&GitCli, &root, &wt, BRANCH).unwrap() else {
            panic!("merged into a detached HEAD");
        };
        assert!(why.contains("detached"), "{why}");
        assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn a_merge_git_refuses_or_one_already_in_progress_is_left_alone() {
        let temp = tempfile::tempdir().unwrap();
        let (root, wt) = repo_with_worktree(temp.path());
        commit_file(&wt, "b.txt", "agent\n");

        // An untracked file the merge would overwrite: git refuses before starting.
        std::fs::write(root.join("b.txt"), "mine\n").unwrap();
        let before = state(&root);
        let Merge::Refused(why) = merge(&GitCli, &root, &wt, BRANCH).unwrap() else {
            panic!("merged over an untracked file");
        };
        assert!(why.contains("b.txt"), "{why}");
        assert_eq!(state(&root), before);
        std::fs::remove_file(root.join("b.txt")).unwrap();

        // The user's own merge in progress is never aborted.
        let head = git(&root, &["rev-parse", "HEAD"]);
        let merge_head = root.join(".git").join("MERGE_HEAD");
        std::fs::write(&merge_head, format!("{head}\n")).unwrap();
        let Merge::Refused(why) = merge(&GitCli, &root, &wt, BRANCH).unwrap() else {
            panic!("merged during another merge");
        };
        assert!(why.contains("MERGE_HEAD"), "{why}");
        assert!(merge_head.exists());
        assert_eq!(git(&root, &["rev-parse", "HEAD"]), head);
    }

    #[test]
    fn a_dirty_worktree_is_not_removed() {
        let temp = tempfile::tempdir().unwrap();
        let (root, wt) = repo_with_worktree(temp.path());
        std::fs::write(wt.join("a.txt"), "edited\n").unwrap();
        std::fs::write(wt.join("new.txt"), "x\n").unwrap();

        let removed = remove(&GitCli, &root, &wt, Some(BRANCH), || {}).unwrap();

        assert_eq!(
            removed,
            Remove::Dirty(vec!["a.txt".into(), "new.txt".into()])
        );
        assert_eq!(
            std::fs::read_to_string(wt.join("a.txt")).unwrap(),
            "edited\n"
        );
        assert!(has_branch(&root));
    }

    #[test]
    fn removing_keeps_an_unmerged_branch() {
        let temp = tempfile::tempdir().unwrap();
        let (root, wt) = repo_with_worktree(temp.path());
        commit_file(&wt, "b.txt", "agent\n");
        let tip = git(&wt, &["rev-parse", "HEAD"]);
        let before = state(&root);

        let removed = remove(&GitCli, &root, &wt, Some(BRANCH), || {}).unwrap();

        assert_eq!(removed, Remove::Removed { branch_kept: true });
        assert!(!wt.exists());
        assert_eq!(git(&root, &["rev-parse", BRANCH]), tip);
        assert_eq!(state(&root), before);
    }

    #[test]
    fn a_worktree_folder_deleted_by_hand_is_pruned() {
        let temp = tempfile::tempdir().unwrap();
        let (root, wt) = repo_with_worktree(temp.path());
        std::fs::remove_dir_all(&wt).unwrap();

        let removed = remove(&GitCli, &root, &wt, Some(BRANCH), || {}).unwrap();

        assert_eq!(removed, Remove::Removed { branch_kept: false });
        assert!(!has_branch(&root));
        assert_eq!(
            git(&root, &["worktree", "list", "--porcelain"])
                .matches("worktree ")
                .count(),
            1
        );
        // Again: nothing left to do.
        let again = remove(&GitCli, &root, &wt, Some(BRANCH), || {}).unwrap();
        assert_eq!(again, Remove::Removed { branch_kept: false });
    }

    #[test]
    fn long_path_lists_are_cut() {
        let paths: Vec<String> = (1..=7).map(|n| format!("f{n}")).collect();
        assert_eq!(listed(&paths[..2]), "f1, f2");
        assert_eq!(listed(&paths), "f1, f2, f3, f4, f5 and 2 more");
    }
}
