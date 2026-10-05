//! Finding a program on `PATH` the way a shell would (SPEC §8 "Command lookup", §17 Windows
//! notes): on Windows a bare name matches with the `PATHEXT` extensions, then `.ps1` shims.
//! Shared by the daemon (agent commands) and the UI (`zed`).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

/// Used when `PATHEXT` is unset (the Windows default list).
const DEFAULT_PATHEXT: &str = ".COM;.EXE;.BAT;.CMD";

/// What [`find`] searched, for "not found" messages.
pub const SEARCHED: &str = if cfg!(windows) {
    "PATH with PATHEXT"
} else {
    "PATH"
};

/// `program` resolved with this process's `PATH` (and `PATHEXT` on Windows).
pub fn find(program: &OsStr) -> Option<PathBuf> {
    let path = std::env::var_os("PATH");
    let pathext = cfg!(windows)
        .then(|| std::env::var_os("PATHEXT").unwrap_or_else(|| OsString::from(DEFAULT_PATHEXT)));
    resolve(program, path.as_deref(), pathext.as_deref())
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

/// Whether `path` has the extension `ext` (any case).
pub fn has_extension(path: &Path, ext: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext))
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
}
