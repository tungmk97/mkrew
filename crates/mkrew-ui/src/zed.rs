//! "Open in Zed" (SPEC §10): runs `zed <path>[:line]`, with `zed` found on `PATH` (and
//! `PATHEXT` on Windows). Reading and editing code is Zed's job; Mkrew only hands it the path.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, PoisonError};

use mkrew_core::lookup::{self, has_extension};

/// Shown when `zed` is missing.
pub const INSTALL_HINT: &str = "Install Zed from https://zed.dev, then make sure its `zed` \
     command is on PATH (the Windows installer adds it; on macOS/Linux run \"cli: install\" \
     in Zed) and restart Mkrew.";

#[derive(Debug, thiserror::Error)]
pub enum ZedError {
    #[error("Zed's `zed` command was not found (searched {searched}). {INSTALL_HINT}")]
    NotFound { searched: &'static str },
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
    *found = lookup::find(OsStr::new("zed"));
    open_with(found.clone(), path, line).inspect_err(|_| *found = None)
}

/// [`open`] with `zed` already looked up (`None` = not found).
fn open_with(zed: Option<PathBuf>, path: &Path, line: Option<u32>) -> Result<(), ZedError> {
    let zed = zed.ok_or(ZedError::NotFound {
        searched: lookup::SEARCHED,
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
    command
        .arg(target(path, line))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    mkrew_daemon::spawn::detach(&mut command);
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
}
