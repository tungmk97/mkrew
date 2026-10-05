//! Mkrew persistence: SQLite in WAL mode with hand-written numbered migrations (SPEC §5).
//!
//! Rows are returned as the `mkrew-protocol` model types so the daemon can hand them to
//! clients unchanged.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mkrew_protocol::{Project, Session, SessionKind, SessionState, TerminalTab};
use rusqlite::{Connection, Row, TransactionBehavior, params};
use ulid::Ulid;

/// Numbered migrations; `PRAGMA user_version` records how many have run. Append only.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_terminals.sql"),
];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("cannot resolve project path {path}: {source}")]
    Path {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("project path is not a directory: {0}")]
    NotADirectory(PathBuf),
    #[error("a project already exists at {0}")]
    DuplicateProject(String),
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: usize, supported: usize },
    #[error("corrupt value in column {column}: {value:?}")]
    Corrupt { column: &'static str, value: String },
    #[error("no session with id {0}")]
    SessionNotFound(Ulid),
    #[error("project {0} not found")]
    ProjectNotFound(Ulid),
    #[error("tab {0} not found")]
    TabNotFound(Ulid),
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens (creating if needed) the database at `path`, enables WAL and runs migrations.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// Number of migrations applied (`PRAGMA user_version`).
    pub fn schema_version(&self) -> Result<usize> {
        user_version(&self.conn)
    }

    /// Adds a project for the directory at `path` (canonicalized with `dunce`). `name`
    /// defaults to the directory name.
    pub fn add_project(&self, path: &Path, name: Option<&str>) -> Result<Project> {
        // canonicalize returns the on-disk casing, which makes the UNIQUE path column
        // case-insensitive in effect on Windows (SPEC §17).
        let canonical = dunce::canonicalize(path).map_err(|source| StoreError::Path {
            path: path.to_path_buf(),
            source,
        })?;
        if !canonical.is_dir() {
            return Err(StoreError::NotADirectory(canonical));
        }
        let path_str = canonical.to_string_lossy().into_owned();
        let name = match name {
            Some(name) => name.to_owned(),
            None => canonical
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path_str.clone()),
        };
        let project = Project {
            id: Ulid::new(),
            name,
            path: path_str,
            dev_command: None,
            default_agent: None,
            review_globs: None,
            created_at: now_ms(),
            archived: false,
        };
        let inserted = self.conn.execute(
            "INSERT INTO project (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                project.id.to_string(),
                project.name,
                project.path,
                project.created_at
            ],
        );
        match inserted {
            Ok(_) => Ok(project),
            Err(rusqlite::Error::SqliteFailure(e, _))
                if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
            {
                Err(StoreError::DuplicateProject(project.path))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// All projects, oldest first.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, dev_command, default_agent, review_globs, created_at, archived
             FROM project ORDER BY created_at, id",
        )?;
        let rows = stmt.query_map([], RawProject::from_row)?;
        rows.map(|r| r?.into_project()).collect()
    }

    pub fn get_project(&self, id: Ulid) -> Result<Project> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, path, dev_command, default_agent, review_globs, created_at, archived
             FROM project WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id.to_string()], RawProject::from_row)?;
        match rows.next() {
            Some(row) => row?.into_project(),
            None => Err(StoreError::ProjectNotFound(id)),
        }
    }

    /// Sets the given fields; `None` leaves a field unchanged, `Some(None)` clears
    /// `default_agent`.
    pub fn update_project(
        &self,
        id: Ulid,
        name: Option<&str>,
        default_agent: Option<Option<&str>>,
    ) -> Result<Project> {
        let mut project = self.get_project(id)?;
        if let Some(name) = name {
            project.name = name.to_owned();
        }
        if let Some(default_agent) = default_agent {
            project.default_agent = default_agent.map(str::to_owned);
        }
        self.conn.execute(
            "UPDATE project SET name = ?2, default_agent = ?3 WHERE id = ?1",
            params![id.to_string(), project.name, project.default_agent],
        )?;
        Ok(project)
    }

    /// Tabs of a project in tab-bar order.
    pub fn list_tabs(&self, project_id: Ulid) -> Result<Vec<TerminalTab>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, position, layout FROM terminal_tab
             WHERE project_id = ?1 ORDER BY position, id",
        )?;
        let rows = stmt.query_map([project_id.to_string()], RawTab::from_row)?;
        rows.map(|r| r?.into_tab()).collect()
    }

    pub fn get_tab(&self, id: Ulid) -> Result<TerminalTab> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, position, layout FROM terminal_tab WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id.to_string()], RawTab::from_row)?;
        match rows.next() {
            Some(row) => row?.into_tab(),
            None => Err(StoreError::TabNotFound(id)),
        }
    }

    /// Adds a tab with a `null` layout; `position` defaults to after the project's last tab.
    pub fn create_tab(
        &self,
        project_id: Ulid,
        title: &str,
        position: Option<f64>,
    ) -> Result<TerminalTab> {
        self.get_project(project_id)?;
        let position = match position {
            Some(position) => position,
            None => self.conn.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM terminal_tab WHERE project_id = ?1",
                [project_id.to_string()],
                |row| row.get(0),
            )?,
        };
        let tab = TerminalTab {
            id: Ulid::new(),
            project_id,
            title: title.to_owned(),
            position,
            layout: serde_json::Value::Null,
        };
        self.conn.execute(
            "INSERT INTO terminal_tab (id, project_id, title, position, layout)
             VALUES (?1, ?2, ?3, ?4, 'null')",
            params![
                tab.id.to_string(),
                project_id.to_string(),
                tab.title,
                tab.position
            ],
        )?;
        Ok(tab)
    }

    pub fn rename_tab(&self, id: Ulid, title: &str) -> Result<TerminalTab> {
        self.update_tab(
            id,
            "UPDATE terminal_tab SET title = ?2 WHERE id = ?1",
            title,
        )
    }

    pub fn move_tab(&self, id: Ulid, position: f64) -> Result<TerminalTab> {
        self.update_tab(
            id,
            "UPDATE terminal_tab SET position = ?2 WHERE id = ?1",
            position,
        )
    }

    pub fn set_tab_layout(&self, id: Ulid, layout: &serde_json::Value) -> Result<TerminalTab> {
        let json = layout.to_string();
        self.update_tab(
            id,
            "UPDATE terminal_tab SET layout = ?2 WHERE id = ?1",
            json,
        )
    }

    /// Deletes the tab; its sessions are not touched.
    pub fn close_tab(&self, id: Ulid) -> Result<()> {
        let changed = self
            .conn
            .execute("DELETE FROM terminal_tab WHERE id = ?1", [id.to_string()])?;
        if changed == 0 {
            return Err(StoreError::TabNotFound(id));
        }
        Ok(())
    }

    /// Runs `sql` with `?1` = tab id and `?2` = `value`, then returns the updated tab.
    fn update_tab(&self, id: Ulid, sql: &str, value: impl rusqlite::ToSql) -> Result<TerminalTab> {
        if self.conn.execute(sql, params![id.to_string(), value])? == 0 {
            return Err(StoreError::TabNotFound(id));
        }
        self.get_tab(id)
    }

    pub fn insert_session(&self, session: &Session) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session (id, task_id, project_id, agent_profile, kind, title,
                native_session_id, state, exit_code, cols, rows, started_at, ended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                session.id.to_string(),
                session.task_id.map(|id| id.to_string()),
                session.project_id.to_string(),
                session.agent_profile,
                kind_str(session.kind),
                session.title,
                session.native_session_id,
                state_str(session.state),
                session.exit_code,
                session.cols,
                session.rows,
                session.started_at,
                session.ended_at,
            ],
        )?;
        Ok(())
    }

    /// Overwrites the mutable columns of an existing session (everything but ids,
    /// profile, kind and `started_at`).
    pub fn update_session(&self, session: &Session) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE session SET title = ?2, native_session_id = ?3, state = ?4, exit_code = ?5,
                cols = ?6, rows = ?7, ended_at = ?8
             WHERE id = ?1",
            params![
                session.id.to_string(),
                session.title,
                session.native_session_id,
                state_str(session.state),
                session.exit_code,
                session.cols,
                session.rows,
                session.ended_at,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::SessionNotFound(session.id));
        }
        Ok(())
    }

    /// Marks every session not yet `exited` as exited (no PTY survives a daemon restart,
    /// SPEC §8). Returns how many rows changed.
    pub fn mark_stale_sessions_exited(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE session SET state = 'exited', ended_at = COALESCE(ended_at, ?1)
             WHERE state <> 'exited'",
            [now_ms()],
        )?)
    }

    /// Sessions, oldest first, optionally limited to one project.
    pub fn list_sessions(&self, project_id: Option<Ulid>) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, project_id, agent_profile, kind, title, native_session_id,
                state, exit_code, cols, rows, started_at, ended_at
             FROM session WHERE ?1 IS NULL OR project_id = ?1
             ORDER BY started_at, id",
        )?;
        let rows = stmt.query_map([project_id.map(|id| id.to_string())], RawSession::from_row)?;
        rows.map(|r| r?.into_session()).collect()
    }
}

fn user_version(conn: &Connection) -> Result<usize> {
    let v: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    Ok(usize::try_from(v).unwrap_or(0))
}

/// Runs every migration past `user_version` in one IMMEDIATE transaction, so two processes
/// opening a fresh database cannot both apply the same migration.
fn migrate(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let current = user_version(&tx)?;
    if current > MIGRATIONS.len() {
        return Err(StoreError::SchemaTooNew {
            found: current,
            supported: MIGRATIONS.len(),
        });
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", index as i64 + 1)?;
    }
    tx.commit()?;
    Ok(())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn kind_str(kind: SessionKind) -> &'static str {
    match kind {
        SessionKind::Agent => "agent",
        SessionKind::Shell => "shell",
    }
}

fn parse_kind(s: String) -> Result<SessionKind> {
    match s.as_str() {
        "agent" => Ok(SessionKind::Agent),
        "shell" => Ok(SessionKind::Shell),
        _ => Err(corrupt("session.kind", s)),
    }
}

fn state_str(state: SessionState) -> &'static str {
    match state {
        SessionState::Starting => "starting",
        SessionState::Working => "working",
        SessionState::NeedsInput => "needs_input",
        SessionState::Idle => "idle",
        SessionState::Done => "done",
        SessionState::Exited => "exited",
        SessionState::Unknown => "unknown",
    }
}

fn parse_state(s: String) -> Result<SessionState> {
    Ok(match s.as_str() {
        "starting" => SessionState::Starting,
        "working" => SessionState::Working,
        "needs_input" => SessionState::NeedsInput,
        "idle" => SessionState::Idle,
        "done" => SessionState::Done,
        "exited" => SessionState::Exited,
        "unknown" => SessionState::Unknown,
        _ => return Err(corrupt("session.state", s)),
    })
}

fn parse_ulid(column: &'static str, s: String) -> Result<Ulid> {
    Ulid::from_string(&s).map_err(|_| corrupt(column, s))
}

fn corrupt(column: &'static str, value: String) -> StoreError {
    StoreError::Corrupt { column, value }
}

/// Column values as SQLite stores them; converted to model types outside the rusqlite
/// row callback so conversion errors stay `StoreError`s.
struct RawProject {
    id: String,
    name: String,
    path: String,
    dev_command: Option<String>,
    default_agent: Option<String>,
    review_globs: Option<String>,
    created_at: i64,
    archived: bool,
}

impl RawProject {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            dev_command: row.get(3)?,
            default_agent: row.get(4)?,
            review_globs: row.get(5)?,
            created_at: row.get(6)?,
            archived: row.get(7)?,
        })
    }

    fn into_project(self) -> Result<Project> {
        let review_globs = match self.review_globs {
            Some(json) => Some(
                serde_json::from_str(&json).map_err(|_| corrupt("project.review_globs", json))?,
            ),
            None => None,
        };
        Ok(Project {
            id: parse_ulid("project.id", self.id)?,
            name: self.name,
            path: self.path,
            dev_command: self.dev_command,
            default_agent: self.default_agent,
            review_globs,
            created_at: self.created_at,
            archived: self.archived,
        })
    }
}

struct RawSession {
    id: String,
    task_id: Option<String>,
    project_id: String,
    agent_profile: String,
    kind: String,
    title: Option<String>,
    native_session_id: Option<String>,
    state: String,
    exit_code: Option<i32>,
    cols: u16,
    rows: u16,
    started_at: i64,
    ended_at: Option<i64>,
}

impl RawSession {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            task_id: row.get(1)?,
            project_id: row.get(2)?,
            agent_profile: row.get(3)?,
            kind: row.get(4)?,
            title: row.get(5)?,
            native_session_id: row.get(6)?,
            state: row.get(7)?,
            exit_code: row.get(8)?,
            cols: row.get(9)?,
            rows: row.get(10)?,
            started_at: row.get(11)?,
            ended_at: row.get(12)?,
        })
    }

    fn into_session(self) -> Result<Session> {
        Ok(Session {
            id: parse_ulid("session.id", self.id)?,
            task_id: self
                .task_id
                .map(|id| parse_ulid("session.task_id", id))
                .transpose()?,
            project_id: parse_ulid("session.project_id", self.project_id)?,
            agent_profile: self.agent_profile,
            kind: parse_kind(self.kind)?,
            title: self.title,
            native_session_id: self.native_session_id,
            state: parse_state(self.state)?,
            exit_code: self.exit_code,
            cols: self.cols,
            rows: self.rows,
            started_at: self.started_at,
            ended_at: self.ended_at,
        })
    }
}

struct RawTab {
    id: String,
    project_id: String,
    title: String,
    position: f64,
    layout: String,
}

impl RawTab {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            project_id: row.get(1)?,
            title: row.get(2)?,
            position: row.get(3)?,
            layout: row.get(4)?,
        })
    }

    fn into_tab(self) -> Result<TerminalTab> {
        Ok(TerminalTab {
            id: parse_ulid("terminal_tab.id", self.id)?,
            project_id: parse_ulid("terminal_tab.project_id", self.project_id)?,
            title: self.title,
            position: self.position,
            layout: serde_json::from_str(&self.layout)
                .map_err(|_| corrupt("terminal_tab.layout", self.layout))?,
        })
    }
}

#[cfg(test)]
mod tests;
