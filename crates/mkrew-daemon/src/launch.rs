//! Profile → PTY command (SPEC §6, §8 "Command lookup", §17 Windows notes).
//!
//! portable-pty's own Windows lookup replaces the extension when trying `PATHEXT` and cannot
//! run `.ps1` shims, so we resolve the full path ourselves ([`mkrew_core::lookup`]) and hand it
//! an absolute program.

use std::ffi::OsStr;
use std::path::Path;

use mkrew_core::lookup::{self, has_extension};
use mkrew_core::{AgentProfile, ProfileKind};
use portable_pty::CommandBuilder;

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
    let resolved = lookup::find(OsStr::new(&program)).ok_or(LaunchError::NotFound {
        program,
        searched: lookup::SEARCHED,
    })?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

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
