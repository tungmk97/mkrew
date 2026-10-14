//! Mkrew daemon (SPEC §8): single instance per data dir, serves the local socket.
//!
//! The binary is a thin wrapper around [`run`]; the library also exposes [`ipc::name`] so
//! clients and tests resolve the endpoint exactly like the daemon does.

mod attach;
mod attachment;
mod awake;
mod backup;
mod docs;
mod emulator;
mod files;
mod git;
mod git_view;
pub mod ipc;
mod launch;
pub mod lock;
mod notify;
mod plan;
mod server;
mod session;
mod session_log;
mod settings;
mod snapshot;
pub mod spawn;
mod tabs;
mod turns;
mod undo;
mod usage;
mod watch;
mod web;
mod worktree;

use anyhow::Context;
use mkrew_core::Paths;
use mkrew_store::Store;

pub use lock::{InstanceLock, LockError};
pub use notify::ENV_NOTIFY;
pub use spawn::ENV_MK_EXE;
pub use usage::transcript_path;

/// Process exit code when another daemon already owns the data dir.
pub const EXIT_ALREADY_RUNNING: u8 = 2;

/// Startup (SPEC §8): lock → migrations → stale sessions → IPC endpoint → serve. Stale
/// sessions are marked before the endpoint opens so no client ever sees them as live.
/// Returns after `daemon.stop` (or Ctrl+C); the lock is released on return.
pub fn run(paths: &Paths) -> Result<(), RunError> {
    std::fs::create_dir_all(&paths.data_dir)
        .with_context(|| format!("create {}", paths.data_dir.display()))
        .map_err(RunError::Other)?;
    let _lock = InstanceLock::acquire(&paths.lock_file())?;
    tracing::info!(pid = std::process::id(), data_dir = %paths.data_dir.display(), "daemon starting");
    serve(paths).map_err(RunError::Other)
}

fn serve(paths: &Paths) -> anyhow::Result<()> {
    std::fs::create_dir_all(paths.sessions_dir())
        .with_context(|| format!("create {}", paths.sessions_dir().display()))?;
    let store = Store::open(&paths.db_file())
        .with_context(|| format!("open {}", paths.db_file().display()))?;
    let stale = store
        .mark_stale_sessions_exited()
        .context("mark stale sessions")?;
    if stale > 0 {
        tracing::info!(stale, "marked sessions from a previous run as exited");
    }
    seed_profiles(&paths.agents_dir())?;

    let runtime = tokio::runtime::Runtime::new().context("start tokio runtime")?;
    runtime.block_on(async {
        let listener =
            ipc::listen(&paths.socket).with_context(|| format!("listen on {}", paths.socket))?;
        tracing::info!(socket = %paths.socket, "listening");
        server::Daemon::new(store, paths.clone())
            .serve(listener, notify::from_env())
            .await;
        anyhow::Ok(())
    })?;
    tracing::info!("daemon stopped");
    Ok(())
}

/// The repo's sample profiles (SPEC §6), copied into the config dir when absent.
const SAMPLE_PROFILES: &[(&str, &str)] = &[
    ("bash.toml", include_str!("../../../agents/bash.toml")),
    ("claude.toml", include_str!("../../../agents/claude.toml")),
    ("cmd.toml", include_str!("../../../agents/cmd.toml")),
    ("codex.toml", include_str!("../../../agents/codex.toml")),
    ("copilot.toml", include_str!("../../../agents/copilot.toml")),
    ("fake.toml", include_str!("../../../agents/fake.toml")),
    (
        "git-bash.toml",
        include_str!("../../../agents/git-bash.toml"),
    ),
    ("omp.toml", include_str!("../../../agents/omp.toml")),
    (
        "powershell.toml",
        include_str!("../../../agents/powershell.toml"),
    ),
    ("pwsh.toml", include_str!("../../../agents/pwsh.toml")),
    ("shell.toml", include_str!("../../../agents/shell.toml")),
];

/// Writes each sample profile that does not exist yet; never overwrites user edits.
fn seed_profiles(dir: &std::path::Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    for (file, contents) in SAMPLE_PROFILES {
        let path = dir.join(file);
        if !path.exists() {
            std::fs::write(&path, contents).with_context(|| format!("write {}", path.display()))?;
        }
    }
    Ok(())
}

/// The shipped sample's `icon` for profile `id` (ADR 0025): profile files copied before the key
/// existed are never overwritten, so they borrow it. Data from the TOML samples, not per-agent
/// code (§2.1).
pub(crate) fn sample_icon(id: &str) -> Option<String> {
    let (file, src) = SAMPLE_PROFILES
        .iter()
        .find(|(file, _)| file.strip_suffix(".toml") == Some(id))?;
    mkrew_core::parse_profile(id, std::path::Path::new(file), src)
        .ok()?
        .icon
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Other(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_icon_comes_from_the_shipped_profile_of_the_same_id() {
        assert_eq!(sample_icon("claude").as_deref(), Some("✻"));
        assert_eq!(sample_icon("omp").as_deref(), Some("π"));
        // A user's own profile id has no shipped sample: the UI shows its first letter.
        assert_eq!(sample_icon("mybot"), None);
    }
}
