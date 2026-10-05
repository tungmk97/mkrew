//! Profile → PTY command (SPEC §6, §8 "Command lookup", §17 Windows notes).
//! Lookup adapted from `spike-term/src/launch.rs`.
//!
//! portable-pty's own Windows lookup replaces the extension when trying `PATHEXT` and cannot
//! run `.ps1` shims, so we resolve the full path ourselves and hand it an absolute program.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use mkrew_core::{AgentProfile, ProfileKind};
use portable_pty::CommandBuilder;

/// Used when `PATHEXT` is unset (the Windows default list).
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// Characters cmd.exe interprets even after portable-pty's quoting. CreateProcessW runs
/// `.cmd`/`.bat` files through `cmd.exe /c`, and portable-pty offers no raw command line,
/// so arguments containing these are refused rather than escaped (BatBadBut).
const CMD_UNSAFE: &[char] = &['"', '%', '&', '|', '<', '>', '^', '!', '\r', '\n', '\0'];

#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("command not found: {program} (searched {searched})")]
    NotFound {
        program: String,
        searched: &'static str,
    },
    #[error("argument {arg:?} of {program} contains a character cmd.exe would interpret")]
    UnsafeCmdArg { program: String, arg: String },
}

/// The command for `profile`: its program resolved on `PATH`, its args and its env.
/// A shell profile without `command` runs the OS default shell. The caller sets cwd and the
/// Mkrew env vars.
pub fn command(profile: &AgentProfile) -> Result<CommandBuilder, LaunchError> {
    let program = if profile.command.is_empty() && profile.kind == ProfileKind::Shell {
        default_shell()
    } else {
        profile.command.clone()
    };
    let path = std::env::var_os("PATH");
    let pathext = cfg!(windows)
        .then(|| std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(DEFAULT_PATHEXT)));
    let resolved = resolve(OsStr::new(&program), path.as_deref(), pathext.as_deref()).ok_or(
        LaunchError::NotFound {
            program,
            searched: if cfg!(windows) {
                "PATH with PATHEXT"
            } else {
                "PATH"
            },
        },
    )?;
    let mut cmd = build(&resolved, &profile.args)?;
    for (key, value) in &profile.env {
        cmd.env(key, value);
    }
    Ok(cmd)
}

fn default_shell() -> String {
    let var = if cfg!(windows) { "COMSPEC" } else { "SHELL" };
    std::env::var(var)
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| if cfg!(windows) { "cmd.exe" } else { "/bin/sh" }.into())
}

/// The PTY command running the already-resolved program `resolved` with `args`.
fn build(resolved: &Path, args: &[String]) -> Result<CommandBuilder, LaunchError> {
    let mut cmd = if has_extension(resolved, "ps1") {
        // PowerShell scripts are not executables; run them through Windows PowerShell.
        let mut cmd = CommandBuilder::new("powershell.exe");
        cmd.args([
            "-NoLogo",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]);
        cmd.arg(resolved);
        cmd
    } else {
        let is_batch = has_extension(resolved, "cmd") || has_extension(resolved, "bat");
        if is_batch && let Some(arg) = args.iter().find(|a| a.contains(CMD_UNSAFE)) {
            return Err(LaunchError::UnsafeCmdArg {
                program: resolved.display().to_string(),
                arg: arg.clone(),
            });
        }
        // CreateProcessW runs `.cmd`/`.bat` through cmd.exe on its own.
        CommandBuilder::new(resolved)
    };
    cmd.args(args);
    Ok(cmd)
}

/// Finds `program` the way a shell would. `pathext` set = Windows rules: a name without an
/// extension only matches with one of the `PATHEXT` extensions (then `.ps1`), never bare.
/// A name containing a path separator is checked as given, not searched on `path`.
fn resolve(program: &OsStr, path: Option<&OsStr>, pathext: Option<&OsStr>) -> Option<PathBuf> {
    let program = Path::new(program);
    if program.components().count() > 1 || program.is_absolute() {
        return candidates(program, pathext)
            .into_iter()
            .find(|c| is_runnable(c));
    }
    std::env::split_paths(path?)
        .flat_map(|dir| candidates(&dir.join(program), pathext))
        .find(|candidate| is_runnable(candidate))
}

/// A file (on Unix: with an execute bit set).
fn is_runnable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Paths to try for `base`, in order.
fn candidates(base: &Path, pathext: Option<&OsStr>) -> Vec<PathBuf> {
    let Some(pathext) = pathext else {
        return vec![base.to_path_buf()];
    };
    let mut out = Vec::new();
    if base.extension().is_some() {
        out.push(base.to_path_buf());
    }
    let pathext = pathext.to_string_lossy();
    let extensions = pathext.split(';').filter(|ext| !ext.is_empty());
    for ext in extensions.chain([".ps1"]) {
        // Append (not replace): `foo.v2` + `.cmd` → `foo.v2.cmd`.
        let mut name = base.as_os_str().to_owned();
        name.push(ext);
        out.push(PathBuf::from(name));
    }
    out
}

fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(dir: &Path, file: &str) -> PathBuf {
        let path = dir.join(file);
        fs::write(&path, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    // Lower case so the expected paths also match on case-sensitive file systems.
    const PATHEXT: &str = ".com;.exe;.bat;.cmd";

    fn profile(command: &str, kind: ProfileKind) -> AgentProfile {
        let src = "name = \"t\"\ncommand = \"t\"\n";
        let parsed = mkrew_core::parse_profile("t", Path::new("t.toml"), src).unwrap();
        AgentProfile {
            command: command.into(),
            kind,
            ..parsed
        }
    }

    #[test]
    fn windows_rules_prefer_pathext_over_extensionless_npm_shim() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "claude"); // npm's sh script, not runnable on Windows
        let cmd = touch(dir.path(), "claude.cmd");
        touch(dir.path(), "claude.ps1");
        let found = resolve(
            OsStr::new("claude"),
            Some(dir.path().as_os_str()),
            Some(OsStr::new(PATHEXT)),
        );
        assert_eq!(found, Some(cmd));
    }

    #[test]
    fn windows_rules_fall_back_to_ps1_and_search_path_in_order() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let ps1 = touch(first.path(), "omp.ps1");
        touch(second.path(), "omp.exe");
        let path = std::env::join_paths([first.path(), second.path()]).unwrap();
        let found = resolve(OsStr::new("omp"), Some(&path), Some(OsStr::new(PATHEXT)));
        assert_eq!(found, Some(ps1));
    }

    #[test]
    fn explicit_paths_and_extensions_resolve() {
        let dir = tempfile::tempdir().unwrap();
        let exe = touch(dir.path(), "codex.exe");
        let pathext = Some(OsStr::new(PATHEXT));
        assert_eq!(
            resolve(
                OsStr::new("codex.exe"),
                Some(dir.path().as_os_str()),
                pathext
            ),
            Some(exe.clone())
        );
        assert_eq!(resolve(exe.as_os_str(), None, pathext), Some(exe.clone()));
        let without_ext = dir.path().join("codex");
        assert_eq!(resolve(without_ext.as_os_str(), None, pathext), Some(exe));
    }

    #[test]
    fn unix_rules_match_the_bare_name_only() {
        let dir = tempfile::tempdir().unwrap();
        let bare = touch(dir.path(), "claude");
        touch(dir.path(), "codex.cmd");
        let path = Some(dir.path().as_os_str());
        assert_eq!(resolve(OsStr::new("claude"), path, None), Some(bare));
        assert_eq!(resolve(OsStr::new("codex"), path, None), None);
    }

    #[test]
    fn missing_command_is_a_clear_error() {
        let err = command(&profile("mkrew-no-such-command-xyz", ProfileKind::Agent))
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.contains("command not found: mkrew-no-such-command-xyz"),
            "{err}"
        );
    }

    #[test]
    fn shell_profile_without_command_uses_the_default_shell() {
        assert!(command(&profile("", ProfileKind::Shell)).is_ok());
    }

    #[test]
    fn cmd_shims_refuse_metacharacter_args() {
        let dir = tempfile::tempdir().unwrap();
        let shim = touch(dir.path(), "claude.cmd");
        let args = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert!(build(&shim, &args(&["--resume", "a b", "x.y"])).is_ok());
        for bad in ["a&calc", "100%", "say \"hi\"", "a|b", "x^y", "l1\nl2"] {
            let err = build(&shim, &args(&[bad])).err().unwrap();
            assert!(matches!(err, LaunchError::UnsafeCmdArg { .. }), "{bad}");
        }
        // Executables get their args verbatim.
        let exe = touch(dir.path(), "codex.exe");
        assert!(build(&exe, &args(&["a&b", "100%"])).is_ok());
    }
}
