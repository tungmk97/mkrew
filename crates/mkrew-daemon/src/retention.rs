//! Retention (SPEC §8, ADR 0060): the log files of sessions that ended more than
//! [`attachment::KEEP_MS`] ago and that no tab shows, and older `files_changed`, `hook` and
//! `edited` events, are deleted. Session rows, turns and `usage` stay (resume, usage totals).

use std::collections::HashSet;
use std::path::Path;

use mkrew_store::{Store, StoreError};
use ulid::Ulid;

use crate::{attachment, server::replace_session, session_log};

/// Event kinds pruned by age. `usage` is not: usage totals read every row (`since` = 0).
pub const PRUNED_EVENT_KINDS: [&str; 3] = ["files_changed", "hook", "edited"];

/// What one run deleted.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Purged {
    pub log_files: usize,
    pub events: usize,
}

/// One retention pass at `now_ms`. A file that cannot be deleted is logged and retried next
/// time.
// ponytail: one fixed window (7 days, shared with attachments) and no total-size cap; add a
// `[storage]` setting or a cap if users need longer history or still run out of disk.
pub fn run(store: &Store, sessions_dir: &Path, now_ms: i64) -> Result<Purged, StoreError> {
    let before = now_ms - attachment::KEEP_MS;
    let ended = store.sessions_ended_before(before)?;
    let shown = shown_in_tabs(store, &ended)?;
    let mut purged = Purged::default();
    for id in ended.iter().filter(|id| !shown.contains(id)) {
        for file in session_log::files(&session_log::path(sessions_dir, id)) {
            match std::fs::remove_file(&file) {
                Ok(()) => purged.log_files += 1,
                Err(e) => tracing::warn!(file = %file.display(), "session log not deleted: {e}"),
            }
        }
    }
    purged.events = store.prune_session_events(before, &PRUNED_EVENT_KINDS)?;
    Ok(purged)
}

/// Those of `ids` that a tab of any project (archived ones too) still shows.
fn shown_in_tabs(store: &Store, ids: &[Ulid]) -> Result<HashSet<Ulid>, StoreError> {
    let mut shown = HashSet::new();
    for project in store.list_projects()? {
        for tab in store.list_tabs(project.id)? {
            shown.extend(
                ids.iter()
                    .filter(|id| replace_session(&tab.layout, **id, **id).is_some()),
            );
        }
    }
    Ok(shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mkrew_protocol::{Session, SessionKind, SessionState, StateSource};

    const DAY: i64 = 24 * 60 * 60 * 1000;

    #[test]
    fn run_purges_old_logs_but_keeps_live_and_tab_shown_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("mkrew.db")).unwrap();
        let sessions_dir = dir.path().join("sessions");
        std::fs::create_dir_all(&sessions_dir).unwrap();
        let project = store.add_project(dir.path(), None).unwrap();
        let now = 100 * DAY;
        let make = |ended_at: Option<i64>| {
            let mut s = test_session(project.id);
            s.ended_at = ended_at;
            s.state = if ended_at.is_some() {
                SessionState::Exited
            } else {
                SessionState::Working
            };
            store.insert_session(&s).unwrap();
            s.id
        };
        let old = make(Some(now - 8 * DAY));
        let old_hidden = make(Some(now - 30 * DAY));
        let old_in_tab = make(Some(now - 30 * DAY));
        let recent = make(Some(now - DAY));
        let live = make(None);
        let gone = make(Some(now - 30 * DAY)); // never had a log
        store.hide_session(old_hidden).unwrap();
        let tab = store.create_tab(project.id, "T", None).unwrap();
        store
            .set_tab_layout(
                tab.id,
                &serde_json::json!({"session": old_in_tab.to_string()}),
            )
            .unwrap();
        for id in [old, old_hidden, old_in_tab, recent, live] {
            let log = session_log::path(&sessions_dir, id);
            std::fs::write(&log, "x").unwrap();
            let mut rotated = log.into_os_string();
            rotated.push(".1");
            std::fs::write(rotated, "y").unwrap();
        }
        let v = serde_json::json!({});
        store
            .insert_session_event(live, "files_changed", &v, now - 8 * DAY)
            .unwrap();
        store
            .insert_session_event(live, "usage", &v, now - 8 * DAY)
            .unwrap();

        let purged = run(&store, &sessions_dir, now).unwrap();

        let has_log = |id| !session_log::files(&session_log::path(&sessions_dir, id)).is_empty();
        assert!(!has_log(old) && !has_log(old_hidden), "old logs purged");
        assert!(
            has_log(old_in_tab) && has_log(recent) && has_log(live),
            "kept"
        );
        assert!(!has_log(gone));
        assert_eq!(
            purged.log_files, 4,
            "two files each of `old` and `old_hidden`"
        );
        assert_eq!(purged.events, 1);
        assert_eq!(store.session_events(live, "usage").unwrap().len(), 1);
        // `session.history` of a purged session: the row is still there, `files` is empty.
        assert!(store.get_session(old_hidden).is_ok());
        assert_eq!(
            run(&store, &sessions_dir, now).unwrap().log_files,
            0,
            "idempotent"
        );
    }

    fn test_session(project_id: Ulid) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id,
            agent_profile: "fake".into(),
            kind: SessionKind::Agent,
            title: None,
            terminal_title: None,
            color: None,
            pinned: false,
            position: 0.0,
            native_session_id: None,
            state: SessionState::Exited,
            state_source: StateSource::Daemon,
            state_changed_at: 1,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 1,
            ended_at: None,
            plan_path: None,
        }
    }
}
