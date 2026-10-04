//! Command-line parsing and command lookup for launching an agent instead of the shell
//! (SPEC §8 "Command lookup", §17 Windows notes).
//!
//! portable-pty's own Windows lookup tries the bare name before `PATHEXT`, so for npm installs
//! it picks the extensionless `claude` sh script next to `claude.cmd`, and it cannot run `.ps1`
//! shims. We resolve the full path ourselves and hand portable-pty an absolute program.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use portable_pty::CommandBuilder;

/// Used when `PATHEXT` is unset (the Windows default list).
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// What `spike-term` was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub struct Invocation {
    pub mode: Mode,
    /// `-- <command> [args]`; `None` runs the default shell.
    pub command: Option<Vec<String>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    /// Open a window; `stats` prints measurements, `probe_keys` also types synthetic keys.
    Window { stats: bool, probe_keys: bool },
    /// Run headless and print the screen instead of opening a window.
    Dump,
    /// Print `megabytes` MiB of colored output to stdout and exit (no PTY, no window).
    Flood { megabytes: u64 },
}

/// Parses `[--dump | --stats [--probe-keys]] [-- <command> [args]]` or `--flood <MB>`.
/// Returns `None` on anything else.
pub fn parse_args(args: &[String]) -> Option<Invocation> {
    let (flags, command) = match args.iter().position(|arg| arg == "--") {
        Some(split) => (&args[..split], Some(args[split + 1..].to_vec())),
        None => (args, None),
    };
    if command.as_ref().is_some_and(Vec::is_empty) {
        return None;
    }
    let mode = match flags {
        [] => Mode::Window {
            stats: false,
            probe_keys: false,
        },
        [flag] if flag == "--dump" => Mode::Dump,
        [flag, megabytes] if flag == "--flood" && command.is_none() => Mode::Flood {
            megabytes: megabytes.parse().ok()?,
        },
        [flag] if flag == "--stats" => Mode::Window {
            stats: true,
            probe_keys: false,
        },
        [a, b]
            if [a, b] == ["--stats", "--probe-keys"] || [a, b] == ["--probe-keys", "--stats"] =>
        {
            Mode::Window {
                stats: true,
                probe_keys: true,
            }
        }
        _ => return None,
    };
    Some(Invocation { mode, command })
}

/// Builds the PTY command for `argv` (program + args), resolving the program on `PATH`.
pub fn command(argv: &[String]) -> Result<CommandBuilder> {
    let Some((program, args)) = argv.split_first() else {
        bail!("no command given");
    };
    let path = std::env::var_os("PATH");
    let pathext = cfg!(windows)
        .then(|| std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(DEFAULT_PATHEXT)));
    let Some(resolved) = resolve(OsStr::new(program), path.as_deref(), pathext.as_deref()) else {
        let searched = if cfg!(windows) {
            "PATH with PATHEXT"
        } else {
            "PATH"
        };
        bail!("command not found: {program} (searched {searched})");
    };
    let mut cmd = build(&resolved, args);
    if let Ok(cwd) = std::env::current_dir() {
        cmd.cwd(cwd);
    }
    Ok(cmd)
}

/// The PTY command running the already-resolved program `resolved` with `args`.
fn build(resolved: &Path, args: &[String]) -> CommandBuilder {
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
        // CreateProcessW runs `.cmd`/`.bat` through cmd.exe on its own.
        CommandBuilder::new(resolved)
    };
    cmd.args(args);
    cmd
}

/// Finds `program` the way a shell would. `pathext` set = Windows rules: a name without an
/// extension only matches with one of the `PATHEXT` extensions (then `.ps1`), never bare.
/// A name containing a path separator is checked as given, not searched on `path`.
pub fn resolve(program: &OsStr, path: Option<&OsStr>, pathext: Option<&OsStr>) -> Option<PathBuf> {
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

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| (*s).to_owned()).collect()
    }

    /// A fresh, empty temp directory unique to `name`.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("spike-term-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

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

    const WINDOW: Mode = Mode::Window {
        stats: false,
        probe_keys: false,
    };

    #[test]
    fn parses_shell_dump_and_command_forms() {
        assert_eq!(
            parse_args(&[]),
            Some(Invocation {
                mode: WINDOW,
                command: None
            })
        );
        assert_eq!(
            parse_args(&strings(&["--dump"])),
            Some(Invocation {
                mode: Mode::Dump,
                command: None
            })
        );
        assert_eq!(
            parse_args(&strings(&["--", "claude", "--resume", "--"])),
            Some(Invocation {
                mode: WINDOW,
                command: Some(strings(&["claude", "--resume", "--"]))
            })
        );
        assert_eq!(
            parse_args(&strings(&["--dump", "--", "codex"])),
            Some(Invocation {
                mode: Mode::Dump,
                command: Some(strings(&["codex"]))
            })
        );
    }

    #[test]
    fn parses_measurement_flags() {
        assert_eq!(
            parse_args(&strings(&["--flood", "50"])),
            Some(Invocation {
                mode: Mode::Flood { megabytes: 50 },
                command: None
            })
        );
        assert_eq!(
            parse_args(&strings(&["--stats", "--", "spike-term", "--flood", "5"])),
            Some(Invocation {
                mode: Mode::Window {
                    stats: true,
                    probe_keys: false
                },
                command: Some(strings(&["spike-term", "--flood", "5"]))
            })
        );
        let probing = Some(Invocation {
            mode: Mode::Window {
                stats: true,
                probe_keys: true,
            },
            command: None,
        });
        assert_eq!(parse_args(&strings(&["--stats", "--probe-keys"])), probing);
        assert_eq!(parse_args(&strings(&["--probe-keys", "--stats"])), probing);
    }

    #[test]
    fn rejects_unknown_flags_and_empty_command() {
        assert_eq!(parse_args(&strings(&["--"])), None);
        assert_eq!(parse_args(&strings(&["--bogus"])), None);
        assert_eq!(parse_args(&strings(&["claude"])), None);
        assert_eq!(parse_args(&strings(&["--dump", "--dump"])), None);
        assert_eq!(parse_args(&strings(&["--flood"])), None);
        assert_eq!(parse_args(&strings(&["--flood", "lots"])), None);
        assert_eq!(parse_args(&strings(&["--flood", "5", "--", "cmd"])), None);
        assert_eq!(parse_args(&strings(&["--probe-keys"])), None);
        assert_eq!(parse_args(&strings(&["--stats", "--stats"])), None);
        assert_eq!(parse_args(&strings(&["--dump", "--stats"])), None);
    }

    #[test]
    fn windows_rules_prefer_pathext_over_extensionless_npm_shim() {
        let dir = temp_dir("npm-shim");
        touch(&dir, "claude"); // npm's sh script, not runnable on Windows
        let cmd = touch(&dir, "claude.cmd");
        touch(&dir, "claude.ps1");
        let found = resolve(
            OsStr::new("claude"),
            Some(dir.as_os_str()),
            Some(OsStr::new(PATHEXT)),
        );
        assert_eq!(found, Some(cmd));
    }

    #[test]
    fn windows_rules_fall_back_to_ps1_and_search_path_in_order() {
        let first = temp_dir("order-a");
        let second = temp_dir("order-b");
        let ps1 = touch(&first, "omp.ps1");
        touch(&second, "omp.exe");
        let path = std::env::join_paths([&first, &second]).unwrap();
        let found = resolve(OsStr::new("omp"), Some(&path), Some(OsStr::new(PATHEXT)));
        assert_eq!(found, Some(ps1));
    }

    #[test]
    fn windows_rules_accept_explicit_extension_and_paths() {
        let dir = temp_dir("explicit");
        let exe = touch(&dir, "codex.exe");
        let pathext = Some(OsStr::new(PATHEXT));
        assert_eq!(
            resolve(OsStr::new("codex.exe"), Some(dir.as_os_str()), pathext),
            Some(exe.clone())
        );
        assert_eq!(resolve(exe.as_os_str(), None, pathext), Some(exe.clone()));
        let without_ext = dir.join("codex");
        assert_eq!(resolve(without_ext.as_os_str(), None, pathext), Some(exe));
    }

    #[test]
    fn unix_rules_match_the_bare_name_only() {
        let dir = temp_dir("unix");
        let bare = touch(&dir, "claude");
        touch(&dir, "codex.cmd");
        assert_eq!(
            resolve(OsStr::new("claude"), Some(dir.as_os_str()), None),
            Some(bare)
        );
        assert_eq!(
            resolve(OsStr::new("codex"), Some(dir.as_os_str()), None),
            None
        );
    }

    #[test]
    fn missing_command_is_a_clear_error() {
        let err = command(&strings(&["mkrew-no-such-command-xyz"]))
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.contains("command not found: mkrew-no-such-command-xyz"),
            "{err}"
        );
    }

    /// Runs `cmd` on a PTY until `expected` shows up, the child exits, or 20 s pass;
    /// returns the screen text.
    fn screen_of(cmd: CommandBuilder, expected: &str) -> String {
        use crate::emulator::Emulator;
        use crate::pty::PtySession;
        use std::time::{Duration, Instant};

        let mut session = PtySession::spawn(cmd, 60, 5).unwrap();
        let mut emu = Emulator::new(60, 5);
        let end = Instant::now() + Duration::from_secs(20);
        let mut text = String::new();
        while !text.contains(expected) && Instant::now() < end {
            let exited = session
                .pump(&mut emu, Duration::from_millis(200), Duration::from_secs(1))
                .unwrap();
            if exited.is_some() {
                // Output can arrive after the exit event.
                session
                    .pump(&mut emu, Duration::from_millis(500), Duration::from_secs(2))
                    .unwrap();
            }
            text = emu.grid().to_text();
            if exited.is_some() {
                break;
            }
        }
        text
    }

    #[test]
    fn resolved_command_runs_on_the_pty() {
        let argv = if cfg!(windows) {
            strings(&["cmd", "/C", "echo", "mkrew-launch-ok"])
        } else {
            strings(&["sh", "-c", "echo mkrew-launch-ok"])
        };
        let text = screen_of(command(&argv).unwrap(), "mkrew-launch-ok");
        assert!(text.contains("mkrew-launch-ok"), "grid was:\n{text}");
    }

    #[cfg(windows)]
    #[test]
    fn cmd_and_ps1_shims_run_with_their_args() {
        let dir = temp_dir("shims");
        fs::write(dir.join("mk-cmd.cmd"), "@echo cmd-shim %1\r\n").unwrap();
        fs::write(
            dir.join("mk-ps.ps1"),
            "Write-Output \"ps-shim $($args[0])\"\r\n",
        )
        .unwrap();
        let pathext = Some(OsStr::new(PATHEXT));
        for (name, expected) in [("mk-cmd", "cmd-shim ok"), ("mk-ps", "ps-shim ok")] {
            let resolved = resolve(OsStr::new(name), Some(dir.as_os_str()), pathext).unwrap();
            let text = screen_of(build(&resolved, &strings(&["ok"])), expected);
            assert!(text.contains(expected), "{name}: grid was:\n{text}");
        }
    }
}
