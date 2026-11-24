//! "Open in Zed" (SPEC §10): runs `zed <path>[:line]`, with `zed` found on `PATH` (and
//! `PATHEXT` on Windows; on macOS also the usual CLI folders and `open -a`). Reading and
//! editing code is Zed's job; Mkrew only hands it the path.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};

use mkrew_core::lookup::{self, has_extension};

use crate::keymap::Os;

/// What to do when `zed` is missing, for `os`.
pub fn install_hint(os: Os) -> &'static str {
    match os {
        Os::Windows => {
            "Install Zed from https://zed.dev, then make sure its `zed` command is on PATH \
             (the Windows installer adds it) and restart Mkrew."
        }
        Os::Mac => {
            "Install Zed from https://zed.dev and keep it in /Applications: Mkrew opens it from \
             there. For the `zed` command in Terminal, run \"cli: install\" in Zed."
        }
        Os::Linux => {
            "Install Zed from https://zed.dev, then make sure its `zed` command is on PATH \
             (run \"cli: install\" in Zed) and restart Mkrew."
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ZedError {
    #[error("Zed's `zed` command was not found (searched {searched}). {hint}")]
    NotFound {
        searched: &'static str,
        hint: &'static str,
    },
    #[error("cannot start {program}: {source}")]
    Spawn {
        program: String,
        source: std::io::Error,
    },
}

/// The `zed` that last started. [`open`] runs on the UI thread and a `PATH` × `PATHEXT` scan is
/// hundreds of file checks on Windows, so a hit is kept; only a miss or a failed start scans.
static FOUND: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Opens `path` (at `line`) in Zed. Returns once `zed` is started; it hands the path to the
/// running Zed (or starts it) and exits on its own.
pub fn open(path: &Path, line: Option<u32>) -> Result<(), ZedError> {
    let mut found = FOUND.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(zed) = found.clone()
        && open_with(Some(zed), path, line).is_ok()
    {
        return Ok(());
    }
    // Not found yet, or moved/uninstalled since: look again.
    *found = find_zed();
    // Only the app is installed (macOS): `open -a` starts it on the path, without a line.
    if found.is_none()
        && let Some(app) = installed_app()
    {
        return open_app(&app, path);
    }
    open_with(found.clone(), path, line).inspect_err(|_| *found = None)
}

/// `zed` on `PATH`, else (macOS) in the places a Mac keeps it.
fn find_zed() -> Option<PathBuf> {
    lookup::find(OsStr::new("zed")).or_else(|| {
        if Os::CURRENT != Os::Mac {
            return None;
        }
        first_file(&mac_cli_candidates(home().as_deref()), |p| p.is_file())
    })
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Zed's command-line tool on a Mac when `PATH` has none: the link "cli: install" makes,
/// Homebrew's, `~/.local/bin`, and the tool inside the app (stable first, then Preview).
pub fn mac_cli_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    let mut list = vec![
        PathBuf::from("/usr/local/bin/zed"),
        PathBuf::from("/opt/homebrew/bin/zed"),
    ];
    if let Some(home) = home {
        list.push(home.join(".local").join("bin").join("zed"));
    }
    list.extend(
        mac_app_dirs(home)
            .into_iter()
            .map(|app| app.join("Contents").join("MacOS").join("cli")),
    );
    list
}

/// Where Zed's app bundle may be: `/Applications`, then `~/Applications`; stable, then Preview.
pub fn mac_app_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    let mut list = Vec::new();
    for name in ["Zed.app", "Zed Preview.app"] {
        list.push(Path::new("/Applications").join(name));
        list.extend(home.map(|home| home.join("Applications").join(name)));
    }
    list
}

fn first_file(candidates: &[PathBuf], is_file: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    candidates.iter().find(|path| is_file(path)).cloned()
}

fn installed_app() -> Option<PathBuf> {
    if Os::CURRENT != Os::Mac {
        return None;
    }
    mac_app_dirs(home().as_deref())
        .into_iter()
        .find(|dir| dir.is_dir())
}

/// `open -a <app> <path>`: the last resort, it cannot take a line.
pub fn open_app_command(app: &Path, path: &Path) -> Command {
    let mut command = Command::new("open");
    command
        .arg("-a")
        .arg(app)
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn open_app(app: &Path, path: &Path) -> Result<(), ZedError> {
    let mut child = open_app_command(app, path)
        .spawn()
        .map_err(|source| ZedError::Spawn {
            program: "open".into(),
            source,
        })?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// [`open`] with `zed` already looked up (`None` = not found).
fn open_with(zed: Option<PathBuf>, path: &Path, line: Option<u32>) -> Result<(), ZedError> {
    let zed = zed.ok_or(ZedError::NotFound {
        searched: lookup::SEARCHED,
        hint: install_hint(Os::CURRENT),
    })?;
    let mut child = command(&zed, path, line)
        .spawn()
        .map_err(|source| ZedError::Spawn {
            program: zed.display().to_string(),
            source,
        })?;
    // Reaps the short-lived CLI process so it does not linger as a zombie on Unix.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// The command running the already-resolved `zed` program on `path` (at `line`): detached,
/// no console window, no inherited stdio.
pub fn command(zed: &Path, path: &Path, line: Option<u32>) -> Command {
    let mut command = if has_extension(zed, "ps1") {
        // A PowerShell shim is not an executable; run it through Windows PowerShell.
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ]);
        command.arg(zed);
        command
    } else {
        // std escapes the argument for `.cmd`/`.bat` shims (or refuses to run them).
        Command::new(zed)
    };
    // macOS: Zed's CLI (and the editor it starts) get the login shell's `PATH` too.
    if let Some(path) = mkrew_core::login_path::path() {
        command.env("PATH", path);
    }
    command
        .arg(target(path, line))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    mkrew_daemon::spawn::detach(&mut command);
    crate::browser::strip_own_env(&mut command);
    command
}

/// Zed's argument: `path`, or `path:line`.
pub fn target(path: &Path, line: Option<u32>) -> OsString {
    let mut target = path.as_os_str().to_owned();
    if let Some(line) = line {
        target.push(format!(":{line}"));
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &Command) -> Vec<&OsStr> {
        command.get_args().collect()
    }

    #[test]
    fn target_appends_the_line_when_given() {
        let path = Path::new("projects").join("web");
        assert_eq!(target(&path, None), path.as_os_str());
        let mut at_line = path.as_os_str().to_owned();
        at_line.push(":42");
        assert_eq!(target(&path, Some(42)), at_line);
    }

    #[test]
    fn executables_get_the_path_as_their_only_argument() {
        let zed = Path::new("bin").join("zed.exe");
        let project = Path::new("projects").join("my web");
        let command = command(&zed, &project, None);
        assert_eq!(command.get_program(), zed.as_os_str());
        assert_eq!(args(&command), [project.as_os_str()]);
    }

    #[test]
    fn powershell_shims_run_through_powershell() {
        let zed = Path::new("bin").join("zed.PS1");
        let file = Path::new("src").join("main.rs");
        let command = command(&zed, &file, Some(7));
        assert_eq!(command.get_program(), "powershell.exe");
        let args = args(&command);
        let (flags, last_two) = args.split_at(args.len() - 2);
        assert!(flags.contains(&OsStr::new("-File")), "{flags:?}");
        assert_eq!(
            last_two,
            [zed.as_os_str(), target(&file, Some(7)).as_os_str()]
        );
    }

    #[test]
    fn missing_zed_explains_how_to_install_it() {
        let error = open_with(None, Path::new("projects"), None).unwrap_err();
        assert!(matches!(error, ZedError::NotFound { .. }), "{error:?}");
        let message = error.to_string();
        assert!(message.contains("`zed` command was not found"), "{message}");
        assert!(message.contains("https://zed.dev"), "{message}");
    }

    #[test]
    fn a_mac_looks_in_the_usual_cli_folders_then_inside_the_app() {
        let home = Path::new("/Users/me");
        let list = mac_cli_candidates(Some(home));
        assert_eq!(list[0], Path::new("/usr/local/bin/zed"));
        assert!(list.contains(&PathBuf::from("/opt/homebrew/bin/zed")));
        assert!(list.contains(&home.join(".local").join("bin").join("zed")));
        let apps: Vec<_> = list[3..].iter().map(PathBuf::as_path).collect();
        assert_eq!(
            apps,
            [
                Path::new("/Applications/Zed.app/Contents/MacOS/cli"),
                &home.join("Applications/Zed.app/Contents/MacOS/cli"),
                Path::new("/Applications/Zed Preview.app/Contents/MacOS/cli"),
                &home.join("Applications/Zed Preview.app/Contents/MacOS/cli"),
            ]
        );
        assert_eq!(
            mac_cli_candidates(None).len(),
            4,
            "no home: two system links, two system apps"
        );
        assert_eq!(
            mac_app_dirs(Some(home)),
            [
                PathBuf::from("/Applications/Zed.app"),
                home.join("Applications").join("Zed.app"),
                PathBuf::from("/Applications/Zed Preview.app"),
                home.join("Applications").join("Zed Preview.app"),
            ]
        );
    }

    #[test]
    fn the_first_candidate_that_is_a_file_wins() {
        let (a, b, c) = (PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c"));
        let all = [a, b.clone(), c.clone()];
        assert_eq!(first_file(&all, |p| p == b || p == c), Some(b));
        assert_eq!(first_file(&all, |_| false), None);
    }

    #[test]
    fn the_app_fallback_runs_open_with_the_bundle_and_the_path() {
        let app = Path::new("/Applications/Zed.app");
        let file = Path::new("/p/x.rs");
        let command = open_app_command(app, file);
        assert_eq!(command.get_program(), "open");
        assert_eq!(
            args(&command),
            [OsStr::new("-a"), app.as_os_str(), file.as_os_str()]
        );
    }

    #[test]
    fn the_hint_names_the_fix_for_each_os() {
        let mac = install_hint(Os::Mac);
        assert!(
            mac.contains("/Applications") && mac.contains("cli: install"),
            "{mac}"
        );
        assert!(
            !mac.contains("restart"),
            "a Mac finds it again by itself: {mac}"
        );
        assert!(install_hint(Os::Windows).contains("installer"));
        let linux = install_hint(Os::Linux);
        assert!(
            linux.contains("cli: install") && linux.contains("restart Mkrew"),
            "{linux}"
        );
    }
}
