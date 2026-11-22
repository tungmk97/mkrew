//! The newest recap omp wrote for a session (ADR 0105): `session_recaps` of
//! `<omp root>/history.db`, opened read-only. Anything unexpected (no file, no table, another
//! shape, a lock that does not clear in time) is no recap, never an error.

use std::path::Path;
use std::time::Duration;

use mkrew_core::profile::UsageFormat;
use mkrew_protocol::Recap;
use rusqlite::{Connection, OpenFlags, OptionalExtension};

/// How long a locked database is waited for.
const BUSY: Duration = Duration::from_millis(200);

/// The newest recap of the omp session `native` in `root`'s `history.db`; `None` for the other
/// agents' formats and for anything missing or unreadable (logged at `debug`).
// ponytail: one small query per poll of a shown omp session; cache by the file's mtime if it
// ever shows up in a profile.
pub fn latest(format: &UsageFormat, root: &Path, native: &str) -> Option<Recap> {
    if !matches!(format, UsageFormat::OmpJsonl) {
        return None;
    }
    match query(&root.join("history.db"), native) {
        Ok(recap) => recap,
        Err(error) => {
            tracing::debug!("omp recap not read: {error}");
            None
        }
    }
}

fn query(path: &Path, native: &str) -> rusqlite::Result<Option<Recap>> {
    if !path.is_file() {
        return Ok(None);
    }
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    db.busy_timeout(BUSY)?;
    db.query_row(
        "SELECT recap, created_at FROM session_recaps WHERE session_id = ?1 \
         ORDER BY created_at DESC LIMIT 1",
        [native],
        |row| {
            Ok(Recap {
                text: row.get(0)?,
                at: row.get(1)?,
            })
        },
    )
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn omp() -> UsageFormat {
        UsageFormat::OmpJsonl
    }

    /// A `history.db` in `dir` with omp's table and `rows` (session, created_at, recap).
    fn history(dir: &Path, rows: &[(&str, i64, &str)]) {
        let db = Connection::open(dir.join("history.db")).unwrap();
        db.execute(
            "CREATE TABLE session_recaps (session_id TEXT, cwd TEXT, created_at INTEGER, recap TEXT)",
            [],
        )
        .unwrap();
        for (session, at, recap) in rows {
            db.execute(
                "INSERT INTO session_recaps (session_id, cwd, created_at, recap) VALUES (?1, 'c', ?2, ?3)",
                rusqlite::params![session, at, recap],
            )
            .unwrap();
        }
    }

    #[test]
    fn the_newest_recap_of_the_session_wins() {
        let dir = tempfile::tempdir().unwrap();
        history(
            dir.path(),
            &[("s1", 100, "old"), ("s1", 200, "new"), ("s2", 300, "other")],
        );
        assert_eq!(
            latest(&omp(), dir.path(), "s1"),
            Some(Recap {
                text: "new".into(),
                at: 200
            })
        );
        assert_eq!(latest(&omp(), dir.path(), "unknown"), None);
    }

    #[test]
    fn no_file_no_table_or_another_shape_is_no_recap() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(latest(&omp(), dir.path(), "s1"), None, "no file");
        let db = Connection::open(dir.path().join("history.db")).unwrap();
        db.execute("CREATE TABLE other (x TEXT)", []).unwrap();
        drop(db);
        assert_eq!(latest(&omp(), dir.path(), "s1"), None, "no table");
    }

    #[test]
    fn a_locked_or_foreign_history_db_gives_no_recap() {
        let dir = tempfile::tempdir().unwrap();
        // The table exists with other columns: the query fails, the answer is None.
        let db = Connection::open(dir.path().join("history.db")).unwrap();
        db.execute("CREATE TABLE session_recaps (id INTEGER, body BLOB)", [])
            .unwrap();
        drop(db);
        assert_eq!(latest(&omp(), dir.path(), "s1"), None, "another shape");
        // Not a database at all.
        std::fs::write(dir.path().join("history.db"), b"not sqlite").unwrap();
        assert_eq!(latest(&omp(), dir.path(), "s1"), None, "garbage file");
    }

    #[test]
    fn other_agents_never_have_a_recap() {
        let dir = tempfile::tempdir().unwrap();
        history(dir.path(), &[("s1", 100, "recap")]);
        for format in [UsageFormat::ClaudeJsonl, UsageFormat::CodexJsonl] {
            assert_eq!(latest(&format, dir.path(), "s1"), None, "{format:?}");
        }
    }
}
