//! Mkrew persistence: SQLite in WAL mode with hand-written numbered migrations (SPEC §5).
//!
//! Rows are returned as the `mkrew-protocol` model types so the daemon can hand them to
//! clients unchanged.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mkrew_protocol::{
    Project, Session, SessionKind, SessionState, StateSource, Task, TaskStatus, TerminalTab,
};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use ulid::Ulid;

/// Numbered migrations; `PRAGMA user_version` records how many have run. Append only.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_terminals.sql"),
    include_str!("../migrations/0003_session_state.sql"),
    include_str!("../migrations/0004_checkpoint_at.sql"),
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
    #[error("task {0} not found")]
    TaskNotFound(Ulid),
    #[error("task {0} still has a running session; stop it first")]
    TaskHasLiveSession(Ulid),
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

    /// Tasks in board order (ascending position), optionally limited to a project and/or
    /// a status column.
    pub fn list_tasks(
        &self,
        project_id: Option<Ulid>,
        status: Option<TaskStatus>,
    ) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM task
             WHERE (?1 IS NULL OR project_id = ?1) AND (?2 IS NULL OR status = ?2)
             ORDER BY position, id"
        ))?;
        let rows = stmt.query_map(
            params![
                project_id.map(|id| id.to_string()),
                status.map(task_status_str)
            ],
            RawTask::from_row,
        )?;
        rows.map(|r| r?.into_task()).collect()
    }

    pub fn get_task(&self, id: Ulid) -> Result<Task> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {TASK_COLUMNS} FROM task WHERE id = ?1"))?;
        let mut rows = stmt.query_map([id.to_string()], RawTask::from_row)?;
        match rows.next() {
            Some(row) => row?.into_task(),
            None => Err(StoreError::TaskNotFound(id)),
        }
    }

    /// Adds a Todo task; `position` defaults to after the project's last Todo task.
    /// `agent_profile` `None` = the project's default agent at run time.
    pub fn create_task(
        &self,
        project_id: Ulid,
        title: &str,
        prompt: &str,
        agent_profile: Option<&str>,
        position: Option<f64>,
    ) -> Result<Task> {
        self.get_project(project_id)?;
        let position = match position {
            Some(position) => position,
            None => self.conn.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM task
                 WHERE project_id = ?1 AND status = 'todo'",
                [project_id.to_string()],
                |row| row.get(0),
            )?,
        };
        let now = now_ms();
        let task = Task {
            id: Ulid::new(),
            project_id,
            title: title.to_owned(),
            prompt: prompt.to_owned(),
            agent_profile: agent_profile.map(str::to_owned),
            status: TaskStatus::Todo,
            position,
            checkpoint: None,
            error_flag: false,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            "INSERT INTO task (id, project_id, title, prompt, agent_profile, status, position,
                error_flag, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)",
            params![
                task.id.to_string(),
                project_id.to_string(),
                task.title,
                task.prompt,
                agent_profile.unwrap_or(""),
                task_status_str(task.status),
                task.position,
                now,
            ],
        )?;
        Ok(task)
    }

    /// Sets the given fields; `None` leaves a field unchanged, `Some(None)` resets
    /// `agent_profile` to the project's default agent.
    pub fn update_task(
        &self,
        id: Ulid,
        title: Option<&str>,
        prompt: Option<&str>,
        agent_profile: Option<Option<&str>>,
    ) -> Result<Task> {
        let mut task = self.get_task(id)?;
        if let Some(title) = title {
            task.title = title.to_owned();
        }
        if let Some(prompt) = prompt {
            task.prompt = prompt.to_owned();
        }
        if let Some(agent_profile) = agent_profile {
            task.agent_profile = agent_profile.map(str::to_owned);
        }
        task.updated_at = now_ms();
        self.conn.execute(
            "UPDATE task SET title = ?2, prompt = ?3, agent_profile = ?4, updated_at = ?5
             WHERE id = ?1",
            params![
                id.to_string(),
                task.title,
                task.prompt,
                task.agent_profile.as_deref().unwrap_or(""),
                task.updated_at
            ],
        )?;
        Ok(task)
    }

    /// Puts the task into column `status` at `position`.
    pub fn move_task(&self, id: Ulid, status: TaskStatus, position: f64) -> Result<Task> {
        let changed = self.conn.execute(
            "UPDATE task SET status = ?2, position = ?3, updated_at = ?4 WHERE id = ?1",
            params![id.to_string(), task_status_str(status), position, now_ms()],
        )?;
        if changed == 0 {
            return Err(StoreError::TaskNotFound(id));
        }
        self.get_task(id)
    }

    /// Sets the status and error flag from the run lifecycle (SPEC §5); the card keeps its
    /// position.
    pub fn set_task_status(&self, id: Ulid, status: TaskStatus, error_flag: bool) -> Result<Task> {
        let changed = self.conn.execute(
            "UPDATE task SET status = ?2, error_flag = ?3, updated_at = ?4 WHERE id = ?1",
            params![
                id.to_string(),
                task_status_str(status),
                error_flag,
                now_ms()
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::TaskNotFound(id));
        }
        self.get_task(id)
    }

    /// Records the git HEAD taken before the latest run (SPEC §5) and when it was taken (Unix
    /// ms); `None` = no checkpoint.
    pub fn set_task_checkpoint(&self, id: Ulid, checkpoint: Option<&str>, at: i64) -> Result<Task> {
        let changed = self.conn.execute(
            "UPDATE task SET checkpoint = ?2, checkpoint_at = ?3, updated_at = ?4 WHERE id = ?1",
            params![id.to_string(), checkpoint, checkpoint.map(|_| at), now_ms()],
        )?;
        if changed == 0 {
            return Err(StoreError::TaskNotFound(id));
        }
        self.get_task(id)
    }

    /// When the task's checkpoint was taken (Unix ms); `None` without a checkpoint.
    pub fn task_checkpoint_at(&self, id: Ulid) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT checkpoint_at FROM task WHERE id = ?1",
                [id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StoreError::TaskNotFound(id))
    }

    /// Deletes the task and returns it. Refused while one of its sessions has not exited;
    /// exited sessions are kept (history) and become free terminals (`task_id` = NULL).
    pub fn delete_task(&self, id: Ulid) -> Result<Task> {
        let task = self.get_task(id)?;
        let tx = self.conn.unchecked_transaction()?;
        let live: i64 = tx.query_row(
            "SELECT COUNT(*) FROM session WHERE task_id = ?1 AND state <> 'exited'",
            [id.to_string()],
            |row| row.get(0),
        )?;
        if live > 0 {
            return Err(StoreError::TaskHasLiveSession(id));
        }
        tx.execute(
            "UPDATE session SET task_id = NULL WHERE task_id = ?1",
            [id.to_string()],
        )?;
        tx.execute("DELETE FROM task WHERE id = ?1", [id.to_string()])?;
        tx.commit()?;
        Ok(task)
    }

    pub fn insert_session(&self, session: &Session) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session (id, task_id, project_id, agent_profile, kind, title,
                native_session_id, state, exit_code, cols, rows, started_at, ended_at,
                state_source, state_changed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
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
                source_str(session.state_source),
                session.state_changed_at,
            ],
        )?;
        Ok(())
    }

    /// Overwrites the mutable columns of an existing session (everything but ids,
    /// profile, kind and `started_at`).
    pub fn update_session(&self, session: &Session) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE session SET title = ?2, native_session_id = ?3, state = ?4, exit_code = ?5,
                cols = ?6, rows = ?7, ended_at = ?8, state_source = ?9, state_changed_at = ?10
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
                source_str(session.state_source),
                session.state_changed_at,
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
            "UPDATE session SET state = 'exited', ended_at = COALESCE(ended_at, ?1),
                state_source = 'daemon', state_changed_at = ?1
             WHERE state <> 'exited'",
            [now_ms()],
        )?)
    }

    /// Sessions, oldest first, optionally limited to one project.
    pub fn list_sessions(&self, project_id: Option<Ulid>) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, project_id, agent_profile, kind, title, native_session_id,
                state, exit_code, cols, rows, started_at, ended_at, state_source, state_changed_at
             FROM session WHERE ?1 IS NULL OR project_id = ?1
             ORDER BY started_at, id",
        )?;
        let rows = stmt.query_map([project_id.map(|id| id.to_string())], RawSession::from_row)?;
        rows.map(|r| r?.into_session()).collect()
    }

    /// One session, live or exited (exited sessions are kept as history).
    pub fn get_session(&self, id: Ulid) -> Result<Session> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, project_id, agent_profile, kind, title, native_session_id,
                state, exit_code, cols, rows, started_at, ended_at, state_source, state_changed_at
             FROM session WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id.to_string()], RawSession::from_row)?;
        match rows.next() {
            Some(row) => row?.into_session(),
            None => Err(StoreError::SessionNotFound(id)),
        }
    }

    /// Appends a `session_event` row (SPEC §5); `payload` is stored as JSON text.
    pub fn insert_session_event(
        &self,
        session_id: Ulid,
        kind: &str,
        payload: &serde_json::Value,
        at: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO session_event (session_id, kind, payload, at) VALUES (?1, ?2, ?3, ?4)",
            params![session_id.to_string(), kind, payload.to_string(), at],
        )?;
        Ok(())
    }

    /// A session's events of `kind`, oldest first.
    pub fn session_events(&self, session_id: Ulid, kind: &str) -> Result<Vec<SessionEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT payload, at FROM session_event WHERE session_id = ?1 AND kind = ?2 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![session_id.to_string(), kind], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.map(|r| {
            let (payload, at) = r?;
            let payload = serde_json::from_str(&payload)
                .map_err(|_| corrupt("session_event.payload", payload))?;
            Ok(SessionEvent { payload, at })
        })
        .collect()
    }
}

/// One `session_event` row as read back by [`Store::session_events`].
#[derive(Debug, Clone, PartialEq)]
pub struct SessionEvent {
    pub payload: serde_json::Value,
    /// Unix ms.
    pub at: i64,
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

fn source_str(source: StateSource) -> &'static str {
    match source {
        StateSource::Hook => "hook",
        StateSource::Screen => "screen",
        StateSource::Daemon => "daemon",
    }
}

fn parse_source(s: String) -> Result<StateSource> {
    Ok(match s.as_str() {
        "hook" => StateSource::Hook,
        "screen" => StateSource::Screen,
        "daemon" => StateSource::Daemon,
        _ => return Err(corrupt("session.state_source", s)),
    })
}

fn task_status_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "todo",
        TaskStatus::Running => "running",
        TaskStatus::NeedsInput => "needs_input",
        TaskStatus::Review => "review",
        TaskStatus::Done => "done",
    }
}

fn parse_task_status(s: String) -> Result<TaskStatus> {
    Ok(match s.as_str() {
        "todo" => TaskStatus::Todo,
        "running" => TaskStatus::Running,
        "needs_input" => TaskStatus::NeedsInput,
        "review" => TaskStatus::Review,
        "done" => TaskStatus::Done,
        _ => return Err(corrupt("task.status", s)),
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
    state_source: String,
    state_changed_at: i64,
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
            state_source: row.get(13)?,
            state_changed_at: row.get(14)?,
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
            state_source: parse_source(self.state_source)?,
            state_changed_at: self.state_changed_at,
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

const TASK_COLUMNS: &str = "id, project_id, title, prompt, agent_profile, status, position,
    checkpoint, error_flag, created_at, updated_at";

struct RawTask {
    id: String,
    project_id: String,
    title: String,
    prompt: String,
    /// `''` = the project's default agent (the column is NOT NULL).
    agent_profile: String,
    status: String,
    position: f64,
    checkpoint: Option<String>,
    error_flag: bool,
    created_at: i64,
    updated_at: i64,
}

impl RawTask {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            project_id: row.get(1)?,
            title: row.get(2)?,
            prompt: row.get(3)?,
            agent_profile: row.get(4)?,
            status: row.get(5)?,
            position: row.get(6)?,
            checkpoint: row.get(7)?,
            error_flag: row.get(8)?,
            created_at: row.get(9)?,
            updated_at: row.get(10)?,
        })
    }

    fn into_task(self) -> Result<Task> {
        Ok(Task {
            id: parse_ulid("task.id", self.id)?,
            project_id: parse_ulid("task.project_id", self.project_id)?,
            title: self.title,
            prompt: self.prompt,
            agent_profile: Some(self.agent_profile).filter(|p| !p.is_empty()),
            status: parse_task_status(self.status)?,
            position: self.position,
            checkpoint: self.checkpoint,
            error_flag: self.error_flag,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

#[cfg(test)]
mod tests;
