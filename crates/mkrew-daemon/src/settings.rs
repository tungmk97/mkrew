//! The daemon's settings (SPEC §20.0): `settings.toml` is loaded at start and re-read after it
//! changes (the config dir is watched, debounced [`QUIET`]). A file that fails to load keeps
//! the last good settings; its error is reported in `daemon.status`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use ::notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use mkrew_core::settings::{self, Settings, SettingsError};
use tokio::sync::mpsc;

/// The file is re-read once no change arrived for this long.
pub const QUIET: Duration = Duration::from_millis(300);

/// The current settings and the error of the last load, if it failed.
pub struct SettingsHolder {
    path: PathBuf,
    state: Mutex<State>,
}

struct State {
    settings: Settings,
    error: Option<SettingsError>,
}

impl SettingsHolder {
    /// Loads `path`; when that fails the settings are the defaults and the error is kept.
    pub fn load(path: PathBuf) -> Self {
        let holder = Self {
            path,
            state: Mutex::new(State {
                settings: Settings::default(),
                error: None,
            }),
        };
        holder.reload();
        holder
    }

    /// Re-reads the file. On error the last good settings stay.
    pub fn reload(&self) {
        let loaded = settings::load(&self.path);
        let mut state = self.lock();
        match loaded {
            Ok(settings) => {
                if state.error.is_some() || state.settings != settings {
                    tracing::info!(file = %self.path.display(), "settings loaded");
                }
                state.settings = settings;
                state.error = None;
            }
            Err(e) => {
                tracing::warn!("settings not loaded, keeping the last good ones: {e}");
                state.error = Some(e);
            }
        }
    }

    /// The last good settings (defaults until a load succeeds).
    pub fn current(&self) -> Settings {
        self.lock().settings.clone()
    }

    /// The error of the last load (`file:line:col: message`), `None` when it succeeded.
    pub fn error(&self) -> Option<String> {
        self.lock().error.as_ref().map(ToString::to_string)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A running settings watcher; dropping it stops watching.
pub struct Watch {
    _watcher: RecommendedWatcher,
}

/// Watches the directory of the settings file and reloads `holder` [`QUIET`] after the file
/// last changed. Must run inside the tokio runtime.
pub fn watch(holder: Arc<SettingsHolder>) -> ::notify::Result<Watch> {
    let dir = holder.path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let name: Option<OsString> = holder.path.file_name().map(Into::into);
    let (tx, rx) = mpsc::unbounded_channel();
    let mut watcher = ::notify::recommended_watcher(
        move |event: ::notify::Result<::notify::Event>| match event {
            Ok(event) if !event.kind.is_access() => {
                // Editors often save via a temp file + rename: any event naming the file counts.
                if event.paths.iter().any(|p| p.file_name() == name.as_deref()) {
                    let _ = tx.send(());
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("settings watcher error: {e}"),
        },
    )?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    tokio::spawn(debounce(rx, move || holder.reload()));
    Ok(Watch { _watcher: watcher })
}

/// Calls `reload` once [`QUIET`] passed without a change. Ends when the watcher is dropped.
async fn debounce(mut rx: mpsc::UnboundedReceiver<()>, reload: impl Fn()) {
    while rx.recv().await.is_some() {
        let open = loop {
            match tokio::time::timeout(QUIET, rx.recv()).await {
                Ok(Some(())) => {}
                Ok(None) => break false,
                Err(_) => break true,
            }
        };
        reload();
        if !open {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn bad_file_keeps_the_last_good_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let holder = SettingsHolder::load(path.clone());
        assert_eq!(holder.current(), Settings::default());
        assert_eq!(holder.error(), None);

        std::fs::write(&path, "[notify]\nsound = false\n").unwrap();
        holder.reload();
        assert!(!holder.current().notify.sound);

        std::fs::write(&path, "[notify]\nsound = 3\n").unwrap();
        holder.reload();
        assert!(!holder.current().notify.sound, "last good settings kept");
        let error = holder.error().unwrap();
        assert!(error.contains("settings.toml:2:"), "{error}");

        std::fs::remove_file(&path).unwrap();
        holder.reload();
        assert_eq!(holder.current(), Settings::default());
        assert_eq!(holder.error(), None);
    }

    #[test]
    fn bad_file_at_start_means_defaults_and_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "nope = 1\n").unwrap();
        let holder = SettingsHolder::load(path);
        assert_eq!(holder.current(), Settings::default());
        assert!(holder.error().unwrap().contains("nope"));
    }

    #[tokio::test(start_paused = true)]
    async fn debounce_reloads_once_after_quiet() {
        let (tx, rx) = mpsc::unbounded_channel();
        let reloads = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&reloads);
        let task = tokio::spawn(debounce(rx, move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }));
        for _ in 0..3 {
            tx.send(()).unwrap();
            tokio::time::sleep(QUIET / 2).await;
        }
        assert_eq!(reloads.load(Ordering::SeqCst), 0, "still changing");
        tokio::time::sleep(QUIET).await;
        assert_eq!(reloads.load(Ordering::SeqCst), 1);

        tx.send(()).unwrap();
        drop(tx);
        task.await.unwrap();
        assert_eq!(
            reloads.load(Ordering::SeqCst),
            2,
            "pending change delivered on drop"
        );
    }
}
