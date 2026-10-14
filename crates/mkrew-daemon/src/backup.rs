//! Daily database backups (SPEC §8): `<data>/backups/mkrew-YYYY-MM-DD.db` (UTC date), made at
//! startup when today's is missing; only the newest [`KEEP`] stay. Later days are caught by a
//! check every [`EVERY`] (not 24 h: monotonic timers pause during suspend and would drift past
//! whole days); a check finding today's file does nothing.
//!
//! The policy lives in [`run`], which takes the clock as an argument so tests can drive days.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use mkrew_store::Store;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

use crate::attachment;
use crate::retention::Purged;
use crate::session::Hub;

pub const KEEP: usize = 7;
pub const EVERY: Duration = Duration::from_secs(60 * 60);

const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Checks for today's backup now and then every [`EVERY`]; each check also deletes old
/// attachments in `attachments` ([`attachment::cleanup`], SPEC §20B.5) and runs retention
/// over the database and `sessions` (ADR 0060). Runs until aborted.
pub fn start(
    hub: Arc<Hub>,
    dir: PathBuf,
    attachments: PathBuf,
    sessions: PathBuf,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(EVERY);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            let (hub, dir, attachments, sessions) = (
                Arc::clone(&hub),
                dir.clone(),
                attachments.clone(),
                sessions.clone(),
            );
            let done = tokio::task::spawn_blocking(move || {
                let now = crate::server::now_ms();
                let backed_up = run(&hub.store(), &dir, now);
                let cleaned = attachment::cleanup(&attachments, now, |id| {
                    hub.store().get_session(id).ok().and_then(|s| s.ended_at)
                });
                let retained = crate::retention::run(&hub.store(), &sessions, now);
                (backed_up, cleaned, retained)
            })
            .await;
            let (backed_up, cleaned, retained) = match done {
                Ok(done) => done,
                Err(e) => {
                    tracing::warn!("database backup task failed: {e}");
                    continue;
                }
            };
            match backed_up {
                Ok(Some(path)) => tracing::info!(path = %path.display(), "database backed up"),
                Ok(None) => {}
                Err(e) => tracing::warn!("database backup failed: {e:#}"),
            }
            match cleaned {
                Ok(removed) => {
                    for path in removed {
                        tracing::info!(path = %path.display(), "old attachments deleted");
                    }
                }
                Err(e) => tracing::warn!("attachment cleanup failed: {e}"),
            }
            match retained {
                Ok(p) if p != Purged::default() => tracing::info!(?p, "retention"),
                Ok(_) => {}
                Err(e) => tracing::warn!("retention failed: {e}"),
            }
        }
    })
}

/// Copies the database to today's file unless it already exists, then prunes to [`KEEP`].
/// Returns the file it created, if any.
pub fn run(store: &Store, dir: &Path, now_ms: i64) -> anyhow::Result<Option<PathBuf>> {
    std::fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
    let path = dir.join(file_name(now_ms));
    let created = if path.exists() {
        None
    } else {
        // Written under another name first so a crash never leaves a half file that counts
        // as today's backup.
        let partial = path.with_extension("db.partial");
        match std::fs::remove_file(&partial) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => {
                return Err(e).with_context(|| format!("remove {}", partial.display()));
            }
            _ => {}
        }
        store
            .backup_to(&partial)
            .with_context(|| format!("write {}", partial.display()))?;
        std::fs::rename(&partial, &path)
            .with_context(|| format!("rename to {}", path.display()))?;
        Some(path)
    };
    prune(dir, KEEP).with_context(|| format!("prune {}", dir.display()))?;
    Ok(created)
}

/// Deletes all but the newest `keep` backups in `dir` (names sort by date); other files are
/// left alone. Returns the deleted paths.
pub fn prune(dir: &Path, keep: usize) -> io::Result<Vec<PathBuf>> {
    let mut backups = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(is_backup)
        {
            backups.push(path);
        }
    }
    backups.sort();
    let excess = backups.len().saturating_sub(keep);
    let removed: Vec<PathBuf> = backups.drain(..excess).collect();
    for path in &removed {
        std::fs::remove_file(path)?;
    }
    Ok(removed)
}

/// `mkrew-YYYY-MM-DD.db` for the UTC day of `now_ms`.
pub fn file_name(now_ms: i64) -> String {
    format!("mkrew-{}.db", date(now_ms))
}

fn is_backup(name: &str) -> bool {
    name.strip_prefix("mkrew-")
        .and_then(|rest| rest.strip_suffix(".db"))
        .is_some_and(|d| {
            d.len() == 10
                && d.bytes().enumerate().all(|(i, b)| match i {
                    4 | 7 => b == b'-',
                    _ => b.is_ascii_digit(),
                })
        })
}

/// UTC calendar date of a unix-ms timestamp (`civil_from_days`, H. Hinnant).
fn date(now_ms: i64) -> String {
    let z = now_ms.div_euclid(DAY_MS) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2026-10-06T12:00:00Z.
    const NOON: i64 = 1_791_288_000_000;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn dates_are_utc_calendar_days() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(DAY_MS - 1), "1970-01-01");
        assert_eq!(date(-1), "1969-12-31");
        assert_eq!(date(NOON), "2026-10-06");
        assert_eq!(date(1_709_164_800_000), "2024-02-29");
        assert_eq!(date(1_709_164_800_000 + DAY_MS), "2024-03-01");
        assert_eq!(file_name(NOON), "mkrew-2026-10-06.db");
    }

    #[test]
    fn only_backup_names_count() {
        assert!(is_backup("mkrew-2026-10-06.db"));
        for name in [
            "mkrew.db",
            "mkrew-2026-10-06.db.partial",
            "mkrew-2026-1-06.db",
            "mkrew-2026x10-06.db",
            "notes-2026-10-06.db",
        ] {
            assert!(!is_backup(name), "{name}");
        }
    }

    #[test]
    fn one_backup_per_day_and_the_newest_seven_are_kept() {
        let data = tempfile::tempdir().unwrap();
        let store = Store::open(&data.path().join("mkrew.db")).unwrap();
        let dir = data.path().join("backups");

        let first = run(&store, &dir, NOON).unwrap();
        assert_eq!(first, Some(dir.join("mkrew-2026-10-06.db")));
        Store::open(&dir.join("mkrew-2026-10-06.db")).unwrap();
        // Later the same day (another daemon start): nothing new.
        assert_eq!(run(&store, &dir, NOON + 60_000).unwrap(), None);

        std::fs::write(dir.join("readme.txt"), "keep me").unwrap();
        for day in 1..=9 {
            assert!(run(&store, &dir, NOON + day * DAY_MS).unwrap().is_some());
        }
        // Days 06..=15 were backed up; 09..=15 are the newest seven.
        let mut expected: Vec<String> = (9..=15)
            .map(|d| format!("mkrew-2026-10-{d:02}.db"))
            .collect();
        expected.push("readme.txt".into());
        assert_eq!(names(&dir), expected);
    }

    #[test]
    fn a_leftover_partial_file_is_replaced() {
        let data = tempfile::tempdir().unwrap();
        let store = Store::open(&data.path().join("mkrew.db")).unwrap();
        let dir = data.path().join("backups");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("mkrew-2026-10-06.db.partial"), "half").unwrap();

        assert!(run(&store, &dir, NOON).unwrap().is_some());
        assert_eq!(names(&dir), ["mkrew-2026-10-06.db"]);
        Store::open(&dir.join("mkrew-2026-10-06.db")).unwrap();
    }

    #[test]
    fn prune_keeps_everything_under_the_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("mkrew-2026-10-01.db"), "").unwrap();
        assert!(prune(dir.path(), KEEP).unwrap().is_empty());
        assert_eq!(names(dir.path()), ["mkrew-2026-10-01.db"]);
    }
}
