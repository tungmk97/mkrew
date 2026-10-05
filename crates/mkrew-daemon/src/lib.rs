//! Mkrew daemon (SPEC §8): single instance per data dir, serves the local socket.
//!
//! The binary is a thin wrapper around [`run`]; the library also exposes [`ipc::name`] so
//! clients and tests resolve the endpoint exactly like the daemon does.

pub mod ipc;
pub mod lock;
mod server;

use anyhow::Context;
use mkrew_core::Paths;
use mkrew_store::Store;

pub use lock::{InstanceLock, LockError};

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

    let runtime = tokio::runtime::Runtime::new().context("start tokio runtime")?;
    runtime.block_on(async {
        let listener =
            ipc::listen(&paths.socket).with_context(|| format!("listen on {}", paths.socket))?;
        tracing::info!(socket = %paths.socket, "listening");
        server::Daemon::new(store).serve(listener).await;
        anyhow::Ok(())
    })?;
    tracing::info!("daemon stopped");
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Other(anyhow::Error),
}
