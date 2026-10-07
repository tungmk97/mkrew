//! Task worktrees (SPEC §20E.1, ADR 0010): a `mkrew/…` branch checked out in its own folder
//! under the data dir, plus copies of the git-ignored files `.worktreeinclude` names.
//! Blocking: callers run [`create`] and [`project_dir`] in `spawn_blocking`.

use std::io;
use std::path::{Component, Path, PathBuf};

use mkrew_core::Paths;
use ulid::Ulid;

use crate::git::GitBackend;

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
}
