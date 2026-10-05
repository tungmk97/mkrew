//! Profile → PTY command (SPEC §6, §8 "Command lookup", §17 Windows notes).
//!
//! portable-pty's own Windows lookup replaces the extension when trying `PATHEXT` and cannot
//! run `.ps1` shims, so we resolve the full path ourselves ([`mkrew_core::lookup`]) and hand it
//! an absolute program.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use mkrew_core::lookup::{self, has_extension};
use mkrew_core::{AgentProfile, ProfileKind};
use portable_pty::CommandBuilder;

/// Characters cmd.exe interprets even after portable-pty's quoting. CreateProcessW runs
/// `.cmd`/`.bat` files through `cmd.exe /c`, and portable-pty offers no raw command line,
/// so arguments containing these are refused rather than escaped (BatBadBut). npm shims
/// (`claude.cmd`, `codex.cmd`) avoid this: their target is started directly ([`npm_shim`]).
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
    } else if let Some(cmd) = npm_shim(resolved) {
        // Skips cmd.exe: the target gets its args verbatim, metacharacters included.
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

/// What an npm `cmd-shim` `.cmd` file at `shim` runs, so it can be started without cmd.exe:
/// its `.exe` target, or `node <script>` for a `.js` target (the shim's own `node.exe` if
/// present, else `node` on `PATH`). `None` for anything else.
fn npm_shim(shim: &Path) -> Option<CommandBuilder> {
    if !has_extension(shim, "cmd") {
        return None;
    }
    let text = std::fs::read_to_string(shim).ok()?;
    let target = shim.parent()?.join(shim_target(&text)?);
    if !target.is_file() {
        return None;
    }
    if has_extension(&target, "exe") {
        return Some(CommandBuilder::new(target));
    }
    let script = ["js", "cjs", "mjs"]
        .iter()
        .any(|ext| has_extension(&target, ext));
    if !script {
        return None;
    }
    let bundled = shim.with_file_name("node.exe");
    let node = if bundled.is_file() {
        bundled
    } else {
        node_exe(lookup::find(OsStr::new("node")))?
    };
    let mut cmd = CommandBuilder::new(node);
    cmd.arg(target);
    Some(cmd)
}

/// `node` found on `PATH`, only if it is a real executable: a `node.cmd` wrapper (version
/// managers) would bring cmd.exe and its metacharacters back.
fn node_exe(found: Option<PathBuf>) -> Option<PathBuf> {
    found.filter(|node| has_extension(node, "exe"))
}

/// The shim-relative target of a `cmd-shim` file: the last `"%dp0%\…"` (or older
/// `"%~dp0\…"`) path on the line that forwards `%*`, run as is or through node. Anything else
/// on that line (shebang flags such as `node --no-warnings`, extra args) → `None`, so no
/// argument is silently dropped; the `%dp0%` prefix keeps plain batch files out.
fn shim_target(text: &str) -> Option<&str> {
    let line = text.lines().rev().find(|l| l.trim_end().ends_with("%*"))?;
    let (open, start) = ["\"%dp0%\\", "\"%~dp0\\"]
        .iter()
        .filter_map(|prefix| line.rfind(prefix).map(|at| (at, at + prefix.len())))
        .max()?;
    let rest = &line[start..];
    let close = rest.find('"')?;
    let target = &rest[..close];
    let before = line[..open].trim_end();
    let launcher = before.is_empty()
        || before.ends_with('&')
        || ["\"%_prog%\"", "node", "\\node.exe\""]
            .iter()
            .any(|program| before.ends_with(program));
    let after = rest[close + 1..].trim_end().strip_suffix("%*")?;
    let clean = launcher && after.trim().is_empty();
    (clean && !target.is_empty() && !target.contains('%')).then_some(target)
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

    /// npm cmd-shim output for a node package (current format, trimmed).
    const NODE_SHIM: &str = "@ECHO off\r\nGOTO start\r\n:find_dp0\r\nSET dp0=%~dp0\r\nEXIT /b\r\n\
        :start\r\nSETLOCAL\r\nCALL :find_dp0\r\n\r\nIF EXIST \"%dp0%\\node.exe\" (\r\n  \
        SET \"_prog=%dp0%\\node.exe\"\r\n) ELSE (\r\n  SET \"_prog=node\"\r\n  \
        SET PATHEXT=%PATHEXT:;.JS;=;%\r\n)\r\n\r\nendLocal & goto #_undefined_# 2>NUL || \
        title %COMSPEC% & \"%_prog%\"  \"%dp0%\\node_modules\\@anthropic-ai\\claude-code\\cli.js\" %*\r\n";

    #[test]
    fn shim_targets_are_read_from_cmd_shim_files() {
        assert_eq!(
            shim_target(NODE_SHIM),
            Some(r"node_modules\@anthropic-ai\claude-code\cli.js")
        );
        // An executable target and the older `%~dp0` format.
        assert_eq!(
            shim_target("@ECHO off\r\n\"%dp0%\\..\\tool\\bin\\tool.exe\"   %*\r\n"),
            Some(r"..\tool\bin\tool.exe")
        );
        let old = "@IF EXIST \"%~dp0\\node.exe\" (\n  \"%~dp0\\node.exe\"  \"%~dp0\\node_modules\\x\\cli.js\" %*\n\
            ) ELSE (\n  node  \"%~dp0\\node_modules\\x\\cli.js\" %*\n)\n";
        assert_eq!(shim_target(old), Some(r"node_modules\x\cli.js"));
        // Hand-written batch files are not shims; shebang flags or extra args would be lost.
        for other in [
            "",
            "@echo off\r\nnode cli.js %*\r\n",
            "\"%dp0%\\%TARGET%\" %*",
            "x %*",
            "& \"%_prog%\" --no-warnings \"%dp0%\\cli.js\" %*",
            "\"%dp0%\\tool.exe\" --verbose %*",
            "sh \"%dp0%\\run.sh\" %*",
        ] {
            assert_eq!(shim_target(other), None, "{other:?}");
        }
    }

    #[test]
    fn node_on_path_must_be_an_executable() {
        let exe = PathBuf::from(r"C:\nodejs\node.exe");
        assert_eq!(node_exe(Some(exe.clone())), Some(exe));
        for wrapper in [r"C:\mise\shims\node.cmd", r"C:\scoop\node.ps1"] {
            assert_eq!(node_exe(Some(wrapper.into())), None, "{wrapper}");
        }
        assert_eq!(node_exe(None), None);
    }

    // Shim targets are `\`-separated Windows paths.
    #[cfg(windows)]
    #[test]
    fn npm_shims_run_their_target_without_cmd() {
        let dir = tempfile::tempdir().unwrap();
        let shim = |name: &str, text: &str| {
            let path = dir.path().join(name);
            fs::write(&path, text).unwrap();
            path
        };
        let argv = |cmd: &CommandBuilder| cmd.get_argv().clone();
        let prompt = "fix \"a&b\" 100% ^!|<>".to_owned();

        // A `.js` target runs with the shim's bundled node.exe, the prompt verbatim.
        let node = touch(dir.path(), "node.exe");
        let script = dir
            .path()
            .join(r"node_modules\@anthropic-ai\claude-code\cli.js");
        fs::create_dir_all(script.parent().unwrap()).unwrap();
        fs::write(&script, b"").unwrap();
        let claude = shim("claude.cmd", NODE_SHIM);
        let cmd = build(&claude, std::slice::from_ref(&prompt)).unwrap();
        assert_eq!(
            argv(&cmd),
            [
                node.into_os_string(),
                script.into_os_string(),
                prompt.clone().into()
            ]
        );

        // An `.exe` target runs directly.
        let exe = touch(dir.path(), "tool.exe");
        let tool = shim("tool.cmd", "@ECHO off\r\n\"%dp0%\\tool.exe\"   %*\r\n");
        let cmd = build(&tool, std::slice::from_ref(&prompt)).unwrap();
        assert_eq!(argv(&cmd), [exe.into_os_string(), prompt.clone().into()]);

        // A missing target or a non-script target falls back to the cmd.exe rules.
        touch(dir.path(), "run.sh");
        for (name, text) in [
            ("gone.cmd", "\"%dp0%\\missing.exe\" %*"),
            ("sh.cmd", "\"%dp0%\\run.sh\" %*"),
        ] {
            let path = shim(name, text);
            let err = build(&path, std::slice::from_ref(&prompt)).err().unwrap();
            assert!(matches!(err, LaunchError::UnsafeCmdArg { .. }), "{name}");
        }
    }
}
