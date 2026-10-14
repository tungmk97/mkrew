//! Where Mkrew keeps its files and which IPC endpoint the daemon listens on (SPEC §1, §8).
//!
//! Defaults come from `directories`; tests (and power users) override them with
//! `MKREW_DATA_DIR`, `MKREW_CONFIG_DIR` and `MKREW_SOCKET`. Nothing is created here.

use std::path::{Path, PathBuf};

/// Overrides the data dir (`<data>/mkrew`, the directory itself).
pub const ENV_DATA_DIR: &str = "MKREW_DATA_DIR";
/// Overrides the config dir (`<config>/mkrew`, the directory itself).
pub const ENV_CONFIG_DIR: &str = "MKREW_CONFIG_DIR";
/// Overrides the IPC endpoint. Also set inside every session PTY (SPEC §6).
pub const ENV_SOCKET: &str = "MKREW_SOCKET";
/// The `mk` next to the daemon. Set inside every session PTY (SPEC §6): agent hooks run this
/// `mk`, so they speak the daemon's protocol whatever `PATH` finds first.
pub const ENV_MK: &str = "MKREW_MK";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub config_dir: PathBuf,
    /// Windows: a pipe path `\\.\pipe\mkrew-<user>`. Unix: a socket file path.
    pub socket: String,
}

#[derive(Debug, thiserror::Error)]
#[error("cannot determine the home directory; set {ENV_DATA_DIR} and {ENV_CONFIG_DIR}")]
pub struct NoHomeDir;

impl Paths {
    /// Resolves the paths from the process environment.
    pub fn from_env() -> Result<Self, NoHomeDir> {
        Self::resolve(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
    }

    /// Resolves the paths with `var` as the environment (testable without touching it).
    pub fn resolve(var: impl Fn(&str) -> Option<String>) -> Result<Self, NoHomeDir> {
        // `BaseDirs` + "mkrew" (not `ProjectDirs`, which adds `config`/`data` on Windows)
        // gives `%APPDATA%\mkrew\`, `~/.config/mkrew/`, … as SPEC §1 lists them.
        let dirs = || directories::BaseDirs::new().ok_or(NoHomeDir);
        let data_dir = match var(ENV_DATA_DIR) {
            Some(dir) => PathBuf::from(dir),
            None => dirs()?.data_dir().join("mkrew"),
        };
        let config_dir = match var(ENV_CONFIG_DIR) {
            Some(dir) => PathBuf::from(dir),
            None => dirs()?.config_dir().join("mkrew"),
        };
        let socket = match var(ENV_SOCKET) {
            Some(socket) => socket,
            None => default_socket(&var, &data_dir),
        };
        Ok(Self {
            data_dir,
            config_dir,
            socket,
        })
    }

    pub fn db_file(&self) -> PathBuf {
        self.data_dir.join("mkrew.db")
    }

    /// Single-instance lock (SPEC §3).
    pub fn lock_file(&self) -> PathBuf {
        self.data_dir.join("mkrew.lock")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.data_dir.join("sessions")
    }

    /// Daily copies of `mkrew.db` (SPEC §8).
    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// Pasted images, one dir per session (SPEC §20B.5).
    pub fn attachments_dir(&self) -> PathBuf {
        self.data_dir.join("attachments")
    }

    /// Task worktrees, `<project_id>/<task_id>` (SPEC §20E.1).
    pub fn worktrees_dir(&self) -> PathBuf {
        self.data_dir.join("worktrees")
    }

    pub fn agents_dir(&self) -> PathBuf {
        self.config_dir.join("agents")
    }

    /// UI shortcuts (SPEC §13).
    pub fn keymap_file(&self) -> PathBuf {
        self.config_dir.join("keymap.toml")
    }

    /// User settings (SPEC §20.0).
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.toml")
    }

    /// Prompt library, one `*.md` per prompt (SPEC §20B.3).
    pub fn prompts_dir(&self) -> PathBuf {
        self.config_dir.join("prompts")
    }
}

/// `\\.\pipe\mkrew-<username>`, with anything but ASCII letters and digits in the name
/// replaced by `_`.
#[cfg(windows)]
fn default_socket(var: &impl Fn(&str) -> Option<String>, _data_dir: &Path) -> String {
    let user: String = var("USERNAME")
        .unwrap_or_else(|| "user".into())
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!(r"\\.\pipe\mkrew-{user}")
}

#[cfg(not(windows))]
fn default_socket(var: &impl Fn(&str) -> Option<String>, data_dir: &Path) -> String {
    let dir = var("XDG_RUNTIME_DIR").map_or_else(|| data_dir.to_path_buf(), PathBuf::from);
    dir.join("mkrew.sock").to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn overrides_win() {
        let paths = Paths::resolve(env(&[
            (ENV_DATA_DIR, "/tmp/d"),
            (ENV_CONFIG_DIR, "/tmp/c"),
            (ENV_SOCKET, "sock"),
        ]))
        .unwrap();
        assert_eq!(paths.data_dir, PathBuf::from("/tmp/d"));
        assert_eq!(paths.config_dir, PathBuf::from("/tmp/c"));
        assert_eq!(paths.socket, "sock");
        assert_eq!(paths.db_file(), Path::new("/tmp/d").join("mkrew.db"));
        assert_eq!(paths.lock_file(), Path::new("/tmp/d").join("mkrew.lock"));
        assert_eq!(paths.agents_dir(), Path::new("/tmp/c").join("agents"));
        assert_eq!(paths.keymap_file(), Path::new("/tmp/c").join("keymap.toml"));
        assert_eq!(
            paths.settings_file(),
            Path::new("/tmp/c").join("settings.toml")
        );
        assert_eq!(paths.prompts_dir(), Path::new("/tmp/c").join("prompts"));
        assert_eq!(
            paths.attachments_dir(),
            Path::new("/tmp/d").join("attachments")
        );
        assert_eq!(paths.worktrees_dir(), Path::new("/tmp/d").join("worktrees"));
    }

    #[test]
    fn default_dirs_are_named_mkrew() {
        let paths = Paths::resolve(env(&[])).unwrap();
        assert!(paths.data_dir.ends_with("mkrew"), "{paths:?}");
        assert!(paths.config_dir.ends_with("mkrew"), "{paths:?}");
    }

    #[cfg(windows)]
    #[test]
    fn default_pipe_is_per_user() {
        let paths = Paths::resolve(env(&[
            (ENV_DATA_DIR, "d"),
            (ENV_CONFIG_DIR, "c"),
            ("USERNAME", "Jane Doe"),
        ]))
        .unwrap();
        assert_eq!(paths.socket, r"\\.\pipe\mkrew-Jane_Doe");
    }

    #[cfg(not(windows))]
    #[test]
    fn default_socket_prefers_runtime_dir() {
        let base = [(ENV_DATA_DIR, "/d"), (ENV_CONFIG_DIR, "/c")];
        let paths = Paths::resolve(env(&base)).unwrap();
        assert_eq!(paths.socket, "/d/mkrew.sock");
        let mut with_runtime = base.to_vec();
        with_runtime.push(("XDG_RUNTIME_DIR", "/run/user/1"));
        let paths = Paths::resolve(env(&with_runtime)).unwrap();
        assert_eq!(paths.socket, "/run/user/1/mkrew.sock");
    }
}
