//! Which shell Claude Code runs a `statusLine` command in: Git Bash when it is installed, else
//! PowerShell on Windows (`sh` elsewhere). `mk integration` writes a command that this shell
//! parses.

#[cfg(any(windows, test))]
use std::path::{Path, PathBuf};

/// Git for Windows' `bash.exe`, the shell Claude Code runs status lines in: the file
/// `CLAUDE_CODE_GIT_BASH_PATH` names, else `<git root>/bin/bash.exe` for a `git.exe` in
/// `<git root>/cmd` or `<git root>/bin` (never `System32\bash.exe`, which is WSL). `None`
/// means PowerShell, which Claude uses when Git Bash is absent.
#[cfg(any(windows, test))]
pub(crate) fn git_bash(
    env: Option<&str>,
    git: Option<&Path>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let set = env.map(Path::new).filter(|path| exists(path));
    if let Some(path) = set {
        return Some(path.to_owned());
    }
    let dir = git?.parent()?;
    let name = dir.file_name()?.to_str()?;
    if !name.eq_ignore_ascii_case("cmd") && !name.eq_ignore_ascii_case("bin") {
        return None;
    }
    let bash = dir.parent()?.join("bin").join("bash.exe");
    exists(&bash).then_some(bash)
}

/// Whether Claude Code runs a status line in a POSIX shell (Git Bash, or `sh` off Windows);
/// false means PowerShell.
#[cfg(windows)]
pub(crate) fn claude_shell_is_bash() -> bool {
    let env = std::env::var("CLAUDE_CODE_GIT_BASH_PATH").ok();
    let git = std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("git.exe"))
            .find(|file| file.is_file())
    });
    git_bash(env.as_deref(), git.as_deref(), Path::is_file).is_some()
}

#[cfg(not(windows))]
pub(crate) fn claude_shell_is_bash() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash_for(env: Option<&str>, git: Option<&str>, files: &[&str]) -> Option<PathBuf> {
        git_bash(env, git.map(Path::new), |path| {
            files.iter().any(|file| Path::new(file) == path)
        })
    }

    #[test]
    fn the_shell_is_the_configured_git_bash() {
        let bash = bash_for(
            Some("D:/tools/bash.exe"),
            Some("C:/Git/cmd/git.exe"),
            &["D:/tools/bash.exe", "C:/Git/bin/bash.exe"],
        );
        assert_eq!(bash, Some(PathBuf::from("D:/tools/bash.exe")));
    }

    #[test]
    fn a_configured_bash_that_is_missing_falls_back_to_git() {
        let bash = bash_for(
            Some("D:/gone/bash.exe"),
            Some("C:/Git/cmd/git.exe"),
            &["C:/Git/bin/bash.exe"],
        );
        assert_eq!(bash, Some(PathBuf::from("C:/Git/bin/bash.exe")));
    }

    #[test]
    fn the_shell_is_found_from_git_in_cmd_or_bin() {
        let files = ["C:/Git/bin/bash.exe"];
        for git in [
            "C:/Git/cmd/git.exe",
            "C:/Git/bin/git.exe",
            "C:/Git/CMD/git.exe",
        ] {
            assert_eq!(
                bash_for(None, Some(git), &files),
                Some(PathBuf::from("C:/Git/bin/bash.exe")),
                "{git}"
            );
        }
    }

    #[test]
    fn without_git_bash_the_shell_is_powershell() {
        let files = ["C:/Git/bin/bash.exe"];
        assert_eq!(bash_for(None, None, &files), None);
        assert_eq!(bash_for(None, Some("C:/Git/mingw64/git.exe"), &files), None);
        assert_eq!(bash_for(None, Some("C:/Other/cmd/git.exe"), &files), None);
        assert_eq!(bash_for(Some(""), Some("git.exe"), &files), None);
    }
}
