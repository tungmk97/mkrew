//! `mkrew-daemon` binary: logging setup around [`mkrew_daemon::run`].

use std::process::ExitCode;

use anyhow::Context;
use mkrew_core::Paths;
use mkrew_daemon::{EXIT_ALREADY_RUNNING, LockError, RunError};
use tracing_appender::rolling::{Builder, Rotation};
use tracing_subscriber::EnvFilter;

fn main() -> ExitCode {
    let paths = match Paths::from_env() {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("mkrew-daemon: {e}");
            return ExitCode::FAILURE;
        }
    };
    let _log_guard = match init_logging(&paths) {
        Ok(guard) => guard,
        Err(e) => {
            eprintln!("mkrew-daemon: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    match mkrew_daemon::run(&paths) {
        Ok(()) => ExitCode::SUCCESS,
        Err(RunError::Lock(e @ LockError::AlreadyRunning(_))) => {
            tracing::info!("{e}; exiting");
            eprintln!("mkrew-daemon: {e}");
            ExitCode::from(EXIT_ALREADY_RUNNING)
        }
        Err(e) => {
            tracing::error!("{e:#}");
            eprintln!("mkrew-daemon: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// Daily rotating files in `<data>/logs` (7 kept), level from `MKREW_LOG` (default `info`).
/// Panics are logged with a backtrace.
fn init_logging(paths: &Paths) -> anyhow::Result<tracing_appender::non_blocking::WorkerGuard> {
    let dir = paths.logs_dir();
    std::fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
    let appender = Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("daemon")
        .filename_suffix("log")
        .max_log_files(7)
        .build(&dir)
        .with_context(|| format!("open log file in {}", dir.display()))?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let filter = EnvFilter::try_from_env("MKREW_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_env_filter(filter)
        .init();
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!("panic: {info}\n{backtrace}");
    }));
    Ok(guard)
}
