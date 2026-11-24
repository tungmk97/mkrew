//! Which ConPTY the daemon's PTYs use (ADR 0107).
//!
//! `portable-pty` loads a `conpty.dll` found next to the executable and falls back to the
//! system one. omp's native surface (TSP) needs the sideloaded pair, which
//! `scripts/fetch-conpty.sh` puts next to the binaries.

use std::path::Path;

/// The pair `portable-pty` needs next to the executable for a sideloaded ConPTY.
const FILES: [&str; 2] = ["conpty.dll", "OpenConsole.exe"];

/// True when `conpty.dll` and `OpenConsole.exe` both sit in `exe_dir`.
pub fn sideloaded(exe_dir: &Path) -> bool {
    FILES.iter().all(|name| exe_dir.join(name).is_file())
}

/// Logs once, at startup, which ConPTY the session PTYs will use. Nothing off Windows.
pub fn log_once() {
    #[cfg(windows)]
    {
        let found = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(sideloaded))
            .unwrap_or(false);
        if found {
            tracing::info!(
                conpty = "sideloaded",
                "conpty.dll and OpenConsole.exe found next to the daemon"
            );
        } else {
            tracing::info!(
                conpty = "system",
                "conpty.dll and OpenConsole.exe are not both next to the daemon, so omp's native surface may not start; run scripts/fetch-conpty.sh (scripts/fetch-conpty.ps1 on PowerShell)"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const BOTH: [&str; 2] = ["conpty.dll", "OpenConsole.exe"];

    #[test]
    fn both_files_mean_sideloaded() {
        let dir = tempfile::tempdir().unwrap();
        for name in BOTH {
            fs::write(dir.path().join(name), b"x").unwrap();
        }
        assert!(sideloaded(dir.path()));
    }

    #[test]
    fn one_missing_file_means_system() {
        for missing in BOTH {
            let dir = tempfile::tempdir().unwrap();
            for name in BOTH.into_iter().filter(|name| *name != missing) {
                fs::write(dir.path().join(name), b"x").unwrap();
            }
            assert!(!sideloaded(dir.path()), "{missing} is missing");
        }
        let empty = tempfile::tempdir().unwrap();
        assert!(!sideloaded(empty.path()));
    }

    #[test]
    fn a_folder_with_a_file_name_does_not_count() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("OpenConsole.exe"), b"x").unwrap();
        fs::create_dir(dir.path().join("conpty.dll")).unwrap();
        assert!(!sideloaded(dir.path()));
    }
}
