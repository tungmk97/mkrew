//! Git access behind a trait (SPEC §12B), so a `gix` backend can replace the CLI later.
//! Only local commands; nothing here touches the network.

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

pub trait GitBackend: Send + Sync {
    /// The commit HEAD points at for the repo containing `dir`; `None` outside a repo or
    /// before the first commit. `Err` = git could not be run.
    fn head(&self, dir: &Path) -> io::Result<Option<String>>;
}

/// Shells out to `git` from `PATH`.
pub struct GitCli;

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
}
