//! Mkrew persistence: SQLite in WAL mode with hand-written numbered migrations (SPEC §5).
//!
//! Rows are returned as the `mkrew-protocol` model types so the daemon can hand them to
//! clients unchanged.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mkrew_protocol::{
    CommentContext, CommentId, CommentStatus, DocComment, Project, RunCommand, Session,
    SessionKind, SessionState, StateSource, Task, TaskStatus, TerminalTab, Turn, TurnFile,
};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, params};
use ulid::Ulid;

/// Numbered migrations; `PRAGMA user_version` records how many have run. Append only.
const MIGRATIONS: &[&str] = &[
    include_str!("../migrations/0001_init.sql"),
    include_str!("../migrations/0002_terminals.sql"),
    include_str!("../migrations/0003_session_state.sql"),
    include_str!("../migrations/0004_checkpoint_at.sql"),
    include_str!("../migrations/0005_doc_review.sql"),
    include_str!("../migrations/0006_files_git.sql"),
    include_str!("../migrations/0007_dev_server.sql"),
    include_str!("../migrations/0008_turns.sql"),
    include_str!("../migrations/0009_worktrees.sql"),
    include_str!("../migrations/0010_session_interrupted.sql"),
    include_str!("../migrations/0011_run_commands.sql"),
    include_str!("../migrations/0012_session_hidden.sql"),
    include_str!("../migrations/0013_project_color.sql"),
    include_str!("../migrations/0014_project_default_shell.sql"),
    include_str!("../migrations/0015_session_color.sql"),
    include_str!("../migrations/0016_pinned.sql"),
    include_str!("../migrations/0017_session_plan.sql"),
    include_str!("../migrations/0018_run_env_file_from_workspace.sql"),
];

/// Columns of [`RawProject::from_row`], in its order.
const PROJECT_COLUMNS: &str = "id, name, path, default_agent, review_globs, created_at, archived, base_ref, run_autostart, color, default_shell, pinned";

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
    #[error("comment {0} not found")]
    CommentNotFound(CommentId),
    #[error("turn {n} of session {session_id} not found")]
    TurnNotFound { session_id: Ulid, n: u32 },
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

/// `Store::update_project` changes: `None` leaves a field unchanged, `Some(None)` clears it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectChanges<'a> {
    pub name: Option<&'a str>,
    pub default_agent: Option<Option<&'a str>>,
    /// A shell profile id (checked by the daemon); `Some(None)` clears it.
    pub default_shell: Option<Option<&'a str>>,
    /// Replaces the whole list (already checked by the caller).
    pub run_commands: Option<&'a [RunCommand]>,
    pub run_autostart: Option<bool>,
    /// A `WORKSPACE_COLORS` name (checked by the daemon); `Some(None)` clears it.
    pub color: Option<Option<&'a str>>,
    /// `Some(false)` unpins; `None` leaves it.
    pub pinned: Option<bool>,
}

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

    /// Writes a consistent copy of the database to `path` (`VACUUM INTO`, safe while in use).
    /// SQLite refuses to overwrite an existing non-empty file.
    pub fn backup_to(&self, path: &Path) -> Result<()> {
        let target = path.to_str().ok_or_else(|| StoreError::Path {
            path: path.to_path_buf(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "path is not UTF-8"),
        })?;
        self.conn.execute("VACUUM INTO ?1", [target])?;
        Ok(())
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
            run_commands: Vec::new(),
            run_autostart: false,
            default_agent: None,
            default_shell: None,
            review_globs: None,
            created_at: now_ms(),
            archived: false,
            base_ref: None,
            color: None,
            pinned: false,
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
                // A removed (archived) workspace comes back with its tasks and history.
                let revived = self.conn.execute(
                    "UPDATE project SET archived = 0 WHERE path = ?1 AND archived = 1",
                    [&project.path],
                )?;
                match self.project_at(&project.path)? {
                    Some(project) if revived > 0 => Ok(project),
                    _ => Err(StoreError::DuplicateProject(project.path)),
                }
            }
            Err(e) => Err(e.into()),
        }
    }

    /// All projects (archived ones too: clients hide them), oldest first.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut projects: Vec<Project> = {
            let mut stmt = self.conn.prepare(&format!(
                "SELECT {PROJECT_COLUMNS} FROM project ORDER BY created_at, id"
            ))?;
            let rows = stmt.query_map([], RawProject::from_row)?;
            rows.map(|r| r?.into_project()).collect::<Result<_>>()?
        };
        // One query for every project's run commands instead of one per project.
        let mut commands = std::collections::HashMap::<String, Vec<RunCommand>>::new();
        for (project_id, command) in self.run_commands_where(None)? {
            commands.entry(project_id).or_default().push(command);
        }
        for project in &mut projects {
            project.run_commands = commands.remove(&project.id.to_string()).unwrap_or_default();
        }
        Ok(projects)
    }

    pub fn get_project(&self, id: Ulid) -> Result<Project> {
        self.project_where("id = ?1", &id.to_string())?
            .ok_or(StoreError::ProjectNotFound(id))
    }

    /// The project at `path` (as stored: canonical).
    fn project_at(&self, path: &str) -> Result<Option<Project>> {
        self.project_where("path = ?1", path)
    }

    fn project_where(&self, condition: &str, value: &str) -> Result<Option<Project>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {PROJECT_COLUMNS} FROM project WHERE {condition}"
        ))?;
        let mut rows = stmt.query_map([value], RawProject::from_row)?;
        rows.next()
            .map(|row| {
                let mut project = row?.into_project()?;
                project.run_commands = self.run_commands(project.id)?;
                Ok(project)
            })
            .transpose()
    }

    /// Archives the project (SPEC §7 `project.archive`, ADR 0019): clients leave it out; its
    /// tasks, sessions and comments stay. Adding its folder again un-archives it.
    pub fn archive_project(&self, id: Ulid) -> Result<Project> {
        let mut project = self.get_project(id)?;
        project.archived = true;
        self.conn.execute(
            "UPDATE project SET archived = 1 WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(project)
    }

    /// The project's run commands, in order.
    fn run_commands(&self, project_id: Ulid) -> Result<Vec<RunCommand>> {
        Ok(self
            .run_commands_where(Some(project_id))?
            .into_iter()
            .map(|(_, command)| command)
            .collect())
    }

    /// Run commands of one project (`Some`) or of all of them, with their project id, in order.
    fn run_commands_where(&self, project_id: Option<Ulid>) -> Result<Vec<(String, RunCommand)>> {
        let mut stmt = self.conn.prepare(
            "SELECT project_id, name, dir, command, env_file, env FROM run_command
             WHERE ?1 IS NULL OR project_id = ?1 ORDER BY project_id, position",
        )?;
        let rows = stmt.query_map([project_id.map(|id| id.to_string())], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (project_id, name, dir, command, env_file, env) = row?;
            let env = serde_json::from_str(&env).map_err(|_| corrupt("run_command.env", env))?;
            Ok((
                project_id,
                RunCommand {
                    name,
                    dir,
                    command,
                    env_file,
                    env,
                },
            ))
        })
        .collect()
    }

    /// Applies `changes` and returns the updated project.
    pub fn update_project(&self, id: Ulid, changes: ProjectChanges<'_>) -> Result<Project> {
        let mut project = self.get_project(id)?;
        if let Some(name) = changes.name {
            project.name = name.to_owned();
        }
        if let Some(default_agent) = changes.default_agent {
            project.default_agent = default_agent.map(str::to_owned);
        }
        if let Some(default_shell) = changes.default_shell {
            project.default_shell = default_shell.map(str::to_owned);
        }
        if let Some(run_autostart) = changes.run_autostart {
            project.run_autostart = run_autostart;
        }
        if let Some(color) = changes.color {
            project.color = color.map(str::to_owned);
        }
        if let Some(pinned) = changes.pinned {
            project.pinned = pinned;
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE project SET name = ?2, default_agent = ?3, run_autostart = ?4, color = ?5, default_shell = ?6, pinned = ?7 WHERE id = ?1",
            params![
                id.to_string(),
                project.name,
                project.default_agent,
                project.run_autostart,
                project.color,
                project.default_shell,
                project.pinned
            ],
        )?;
        if let Some(commands) = changes.run_commands {
            tx.execute(
                "DELETE FROM run_command WHERE project_id = ?1",
                [id.to_string()],
            )?;
            for (position, command) in commands.iter().enumerate() {
                // A Vec of string pairs always serializes.
                #[allow(clippy::expect_used)]
                let env = serde_json::to_string(&command.env).expect("string pairs serialize");
                tx.execute(
                    "INSERT INTO run_command (project_id, position, name, dir, command, env_file, env)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        id.to_string(),
                        position as i64,
                        command.name,
                        command.dir,
                        command.command,
                        command.env_file,
                        env
                    ],
                )?;
            }
            project.run_commands = commands.to_vec();
        }
        tx.commit()?;
        Ok(project)
    }

    /// Sets the Git view base of a project; `None` = auto-detect (SPEC §12B).
    pub fn set_project_base(&self, id: Ulid, base_ref: Option<&str>) -> Result<Project> {
        let mut project = self.get_project(id)?;
        project.base_ref = base_ref.map(str::to_owned);
        self.conn.execute(
            "UPDATE project SET base_ref = ?2 WHERE id = ?1",
            params![id.to_string(), project.base_ref],
        )?;
        Ok(project)
    }

    /// Tabs of a project in tab-bar order.
    pub fn list_tabs(&self, project_id: Ulid) -> Result<Vec<TerminalTab>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, position, layout, pinned FROM terminal_tab
             WHERE project_id = ?1 ORDER BY position, id",
        )?;
        let rows = stmt.query_map([project_id.to_string()], RawTab::from_row)?;
        rows.map(|r| r?.into_tab()).collect()
    }

    pub fn get_tab(&self, id: Ulid) -> Result<TerminalTab> {
        let mut stmt = self.conn.prepare(
            "SELECT id, project_id, title, position, layout, pinned FROM terminal_tab WHERE id = ?1",
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
            pinned: false,
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

    /// Pins or unpins the tab. `position` stays, so unpinning puts it back in place.
    pub fn set_tab_pinned(&self, id: Ulid, pinned: bool) -> Result<TerminalTab> {
        self.update_tab(
            id,
            "UPDATE terminal_tab SET pinned = ?2 WHERE id = ?1",
            pinned,
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
    /// `agent_profile` `None` = the project's default agent at run time; `worktree` = run in a
    /// git worktree (SPEC §20E.1).
    pub fn create_task(
        &self,
        project_id: Ulid,
        title: &str,
        prompt: &str,
        agent_profile: Option<&str>,
        position: Option<f64>,
        worktree: bool,
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
            worktree,
            worktree_path: None,
            branch: None,
            fanout_of: None,
            created_at: now,
            updated_at: now,
        };
        self.conn.execute(
            "INSERT INTO task (id, project_id, title, prompt, agent_profile, status, position,
                error_flag, created_at, updated_at, worktree)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8, ?9)",
            params![
                task.id.to_string(),
                project_id.to_string(),
                task.title,
                task.prompt,
                agent_profile.unwrap_or(""),
                task_status_str(task.status),
                task.position,
                now,
                worktree,
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
        worktree: Option<bool>,
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
        if let Some(worktree) = worktree {
            task.worktree = worktree;
        }
        task.updated_at = now_ms();
        self.conn.execute(
            "UPDATE task SET title = ?2, prompt = ?3, agent_profile = ?4, updated_at = ?5,
                worktree = ?6
             WHERE id = ?1",
            params![
                id.to_string(),
                task.title,
                task.prompt,
                task.agent_profile.as_deref().unwrap_or(""),
                task.updated_at,
                task.worktree
            ],
        )?;
        Ok(task)
    }

    /// Records the task's created worktree folder and branch (SPEC §20E.1); `None` = removed.
    pub fn set_task_worktree(&self, id: Ulid, worktree: Option<(&str, &str)>) -> Result<Task> {
        let changed = self.conn.execute(
            "UPDATE task SET worktree_path = ?2, branch = ?3, updated_at = ?4 WHERE id = ?1",
            params![
                id.to_string(),
                worktree.map(|(path, _)| path),
                worktree.map(|(_, branch)| branch),
                now_ms()
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::TaskNotFound(id));
        }
        self.get_task(id)
    }

    /// Makes the task a fan-out child of `parent` (SPEC §20E.2).
    pub fn set_task_fanout_of(&self, id: Ulid, parent: Ulid) -> Result<Task> {
        self.get_task(parent)?;
        let changed = self.conn.execute(
            "UPDATE task SET fanout_of = ?2, updated_at = ?3 WHERE id = ?1",
            params![id.to_string(), parent.to_string(), now_ms()],
        )?;
        if changed == 0 {
            return Err(StoreError::TaskNotFound(id));
        }
        self.get_task(id)
    }

    /// The fan-out children of `parent`, oldest first.
    pub fn list_fanout_children(&self, parent: Ulid) -> Result<Vec<Task>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM task WHERE fanout_of = ?1 ORDER BY created_at, id"
        ))?;
        let rows = stmt.query_map([parent.to_string()], RawTask::from_row)?;
        rows.map(|r| r?.into_task()).collect()
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
    /// Fan-out children stay as ordinary tasks.
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
        tx.execute(
            "UPDATE task SET fanout_of = NULL WHERE fanout_of = ?1",
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
                state_source, state_changed_at, color, pinned, plan_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
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
                session.color,
                session.pinned,
                session.plan_path,
            ],
        )?;
        Ok(())
    }

    /// Overwrites the mutable columns of an existing session (everything but ids,
    /// profile, kind and `started_at`).
    pub fn update_session(&self, session: &Session) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE session SET title = ?2, native_session_id = ?3, state = ?4, exit_code = ?5,
                cols = ?6, rows = ?7, ended_at = ?8, state_source = ?9, state_changed_at = ?10,
                color = ?11, pinned = ?12, plan_path = ?13
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
                session.color,
                session.pinned,
                session.plan_path,
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::SessionNotFound(session.id));
        }
        Ok(())
    }

    /// Marks every session not yet `exited` as exited and interrupted: no PTY survives a
    /// daemon restart, and a row still live here means a crash or reboot (SPEC §8).
    /// Returns how many rows changed.
    pub fn mark_stale_sessions_exited(&self) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE session SET state = 'exited', ended_at = COALESCE(ended_at, ?1),
                state_source = 'daemon', state_changed_at = ?1, interrupted = 1
             WHERE state <> 'exited'",
            [now_ms()],
        )?)
    }

    /// The daemon stopped while `id` ran: resumable at the next start (SPEC §8).
    pub fn mark_interrupted(&self, id: Ulid) -> Result<()> {
        self.conn.execute(
            "UPDATE session SET interrupted = 1 WHERE id = ?1",
            [id.to_string()],
        )?;
        Ok(())
    }

    /// The sessions the last daemon stop interrupted. Clears the mark, so each one is
    /// resumed at most once.
    pub fn take_interrupted_sessions(&self) -> Result<Vec<Ulid>> {
        let mut stmt = self
            .conn
            .prepare("UPDATE session SET interrupted = 0 WHERE interrupted = 1 RETURNING id")?;
        let ids = stmt.query_map([], |row| row.get::<_, String>(0))?;
        ids.map(|id| parse_ulid("session.id", id?)).collect()
    }

    /// Sessions not removed (`hidden`), oldest first, optionally limited to one project.
    pub fn list_sessions(&self, project_id: Option<Ulid>) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, project_id, agent_profile, kind, title, native_session_id,
                state, exit_code, cols, rows, started_at, ended_at, state_source, state_changed_at,
                color, pinned, plan_path
             FROM session WHERE (?1 IS NULL OR project_id = ?1) AND hidden = 0
             ORDER BY started_at, id",
        )?;
        let rows = stmt.query_map([project_id.map(|id| id.to_string())], RawSession::from_row)?;
        rows.map(|r| r?.into_session()).collect()
    }

    /// One session, live or exited (exited sessions are kept as history).
    pub fn get_session(&self, id: Ulid) -> Result<Session> {
        let mut stmt = self.conn.prepare(
            "SELECT id, task_id, project_id, agent_profile, kind, title, native_session_id,
                state, exit_code, cols, rows, started_at, ended_at, state_source, state_changed_at,
                color, pinned, plan_path
             FROM session WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map([id.to_string()], RawSession::from_row)?;
        match rows.next() {
            Some(row) => row?.into_session(),
            None => Err(StoreError::SessionNotFound(id)),
        }
    }

    /// Leaves the session out of [`Store::list_sessions`] from now on (SPEC §7
    /// `session.remove`, ADR 0019); its row, events, turns and log stay as history.
    pub fn hide_session(&self, id: Ulid) -> Result<()> {
        let changed = self.conn.execute(
            "UPDATE session SET hidden = 1 WHERE id = ?1",
            [id.to_string()],
        )?;
        if changed == 0 {
            return Err(StoreError::SessionNotFound(id));
        }
        Ok(())
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

    /// Events of `kind` of every session of a project at or after `since` (Unix ms), oldest
    /// first, with their session id.
    pub fn project_session_events(
        &self,
        project_id: Ulid,
        kind: &str,
        since: i64,
    ) -> Result<Vec<(Ulid, SessionEvent)>> {
        let mut stmt = self.conn.prepare(
            "SELECT e.session_id, e.payload, e.at FROM session_event e
             JOIN session s ON s.id = e.session_id
             WHERE s.project_id = ?1 AND e.kind = ?2 AND e.at >= ?3
             ORDER BY e.at, e.id",
        )?;
        let rows = stmt.query_map(params![project_id.to_string(), kind, since], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        rows.map(|r| {
            let (session, payload, at) = r?;
            let session = parse_ulid("session_event.session_id", session)?;
            let payload = serde_json::from_str(&payload)
                .map_err(|_| corrupt("session_event.payload", payload))?;
            Ok((session, SessionEvent { payload, at }))
        })
        .collect()
    }

    /// Records the start of turn `n` of a session (SPEC §20C.1); `start_tree` is `None`
    /// outside a git repo.
    pub fn start_turn(
        &self,
        session_id: Ulid,
        n: u32,
        started_at: i64,
        start_tree: Option<&str>,
    ) -> Result<Turn> {
        self.get_session(session_id)?;
        self.conn.execute(
            "INSERT INTO turn (session_id, n, started_at, start_tree) VALUES (?1, ?2, ?3, ?4)",
            params![session_id.to_string(), n, started_at, start_tree],
        )?;
        self.get_turn(session_id, n)
    }

    /// Records the end of turn `n` with its end snapshot and the files changed during it.
    pub fn end_turn(
        &self,
        session_id: Ulid,
        n: u32,
        ended_at: i64,
        end_tree: Option<&str>,
        files: &[TurnFile],
    ) -> Result<Turn> {
        // Infallible: `TurnFile` is plain strings and integers.
        let files = serde_json::to_string(files).unwrap_or_else(|_| "[]".to_owned());
        let changed = self.conn.execute(
            "UPDATE turn SET ended_at = ?3, end_tree = ?4, files = ?5
             WHERE session_id = ?1 AND n = ?2",
            params![session_id.to_string(), n, ended_at, end_tree, files],
        )?;
        if changed == 0 {
            return Err(StoreError::TurnNotFound { session_id, n });
        }
        self.get_turn(session_id, n)
    }

    pub fn get_turn(&self, session_id: Ulid, n: u32) -> Result<Turn> {
        self.conn
            .query_row(
                &format!("SELECT {TURN_COLUMNS} FROM turn WHERE session_id = ?1 AND n = ?2"),
                params![session_id.to_string(), n],
                RawTurn::from_row,
            )
            .optional()?
            .ok_or(StoreError::TurnNotFound { session_id, n })?
            .into_turn()
    }

    /// A session's turns, oldest first.
    pub fn list_turns(&self, session_id: Ulid) -> Result<Vec<Turn>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TURN_COLUMNS} FROM turn WHERE session_id = ?1 ORDER BY n"
        ))?;
        let rows = stmt.query_map([session_id.to_string()], RawTurn::from_row)?;
        rows.map(|r| r?.into_turn()).collect()
    }

    /// Turns of the project's sessions that ended at or after `since` (Unix ms), with their
    /// session, oldest first (SPEC §20C.4).
    pub fn project_turns(&self, project_id: Ulid, since: i64) -> Result<Vec<(Ulid, Turn)>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TURN_COLUMNS}, session_id FROM turn
             WHERE session_id IN (SELECT id FROM session WHERE project_id = ?1)
               AND ended_at >= ?2
             ORDER BY ended_at, session_id, n"
        ))?;
        let rows = stmt.query_map(params![project_id.to_string(), since], |row| {
            Ok((row.get::<_, String>(6)?, RawTurn::from_row(row)?))
        })?;
        rows.map(|r| {
            let (session, turn) = r?;
            Ok((parse_ulid("turn.session_id", session)?, turn.into_turn()?))
        })
        .collect()
    }

    /// Sessions that ended before `cutoff` (Unix ms) and still have turn trees, with their
    /// project: their `refs/mkrew/turns/<sid>/*` refs are due for deletion (SPEC §20C.1).
    pub fn sessions_with_expired_turns(&self, cutoff: i64) -> Result<Vec<(Ulid, Ulid)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.project_id FROM session s
             WHERE s.ended_at IS NOT NULL AND s.ended_at < ?1
               AND EXISTS (SELECT 1 FROM turn t WHERE t.session_id = s.id
                           AND (t.start_tree IS NOT NULL OR t.end_tree IS NOT NULL))
             ORDER BY s.ended_at, s.id",
        )?;
        let rows = stmt.query_map([cutoff], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|r| {
            let (session, project) = r?;
            Ok((
                parse_ulid("session.id", session)?,
                parse_ulid("session.project_id", project)?,
            ))
        })
        .collect()
    }

    /// Clears the trees of a session's turns once their refs are deleted (the trees may be
    /// garbage-collected); the turns stay as history and, like turns outside git, cannot be
    /// restored.
    pub fn forget_turn_trees(&self, session_id: Ulid) -> Result<()> {
        self.conn.execute(
            "UPDATE turn SET start_tree = NULL, end_tree = NULL WHERE session_id = ?1",
            [session_id.to_string()],
        )?;
        Ok(())
    }

    /// Review comments of a project, optionally of one file: thread roots and replies in the
    /// order they were written.
    pub fn list_comments(
        &self,
        project_id: Ulid,
        file_path: Option<&str>,
    ) -> Result<Vec<DocComment>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COMMENT_COLUMNS} FROM doc_comment
             WHERE project_id = ?1 AND (?2 IS NULL OR file_path = ?2)
             ORDER BY created_at, rowid"
        ))?;
        let rows = stmt.query_map(
            params![project_id.to_string(), file_path],
            RawComment::from_row,
        )?;
        rows.map(|r| r?.into_comment()).collect()
    }

    pub fn get_comment(&self, id: CommentId) -> Result<DocComment> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COMMENT_COLUMNS} FROM doc_comment WHERE id = ?1"
        ))?;
        let mut rows = stmt.query_map([id.to_string()], RawComment::from_row)?;
        match rows.next() {
            Some(row) => row?.into_comment(),
            None => Err(StoreError::CommentNotFound(id)),
        }
    }

    /// Starts an `open` thread by the user, anchored to lines `start_line..=end_line`
    /// (1-based) quoting `quote`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_comment(
        &self,
        project_id: Ulid,
        file_path: &str,
        context: CommentContext,
        start_line: u32,
        end_line: u32,
        quote: &str,
        body: &str,
    ) -> Result<DocComment> {
        self.get_project(project_id)?;
        let now = now_ms();
        let comment = DocComment {
            id: CommentId::new(),
            project_id,
            file_path: file_path.to_owned(),
            context,
            parent_id: None,
            author: DocComment::USER.to_owned(),
            body: body.to_owned(),
            start_line: Some(start_line),
            end_line: Some(end_line),
            quote: Some(quote.to_owned()),
            status: CommentStatus::Open,
            sent_to: None,
            sent_snapshot: None,
            created_at: now,
            updated_at: now,
        };
        self.insert_comment(&comment)?;
        Ok(comment)
    }

    /// Adds a reply by `author` to the thread of `parent` (a reply to a reply joins the same
    /// thread). Thread statuses are left to the caller.
    pub fn reply_comment(&self, parent: CommentId, author: &str, body: &str) -> Result<DocComment> {
        let parent = self.get_comment(parent)?;
        let now = now_ms();
        let reply = DocComment {
            id: CommentId::new(),
            project_id: parent.project_id,
            file_path: parent.file_path,
            context: parent.context,
            parent_id: Some(parent.parent_id.unwrap_or(parent.id)),
            author: author.to_owned(),
            body: body.to_owned(),
            start_line: None,
            end_line: None,
            quote: None,
            status: CommentStatus::Open,
            sent_to: None,
            sent_snapshot: None,
            created_at: now,
            updated_at: now,
        };
        self.insert_comment(&reply)?;
        Ok(reply)
    }

    pub fn set_comment_status(&self, id: CommentId, status: CommentStatus) -> Result<DocComment> {
        self.update_comment(
            id,
            "UPDATE doc_comment SET status = ?2, updated_at = ?3 WHERE id = ?1",
            params![id.to_string(), comment_status_str(status), now_ms()],
        )
    }

    /// Moves a thread root's anchor to lines `start_line..=end_line` (1-based) with `status`
    /// (re-anchoring after the file changed).
    pub fn set_comment_anchor(
        &self,
        id: CommentId,
        start_line: u32,
        end_line: u32,
        status: CommentStatus,
    ) -> Result<DocComment> {
        self.update_comment(
            id,
            "UPDATE doc_comment SET start_line = ?2, end_line = ?3, status = ?4, updated_at = ?5
             WHERE id = ?1",
            params![
                id.to_string(),
                start_line,
                end_line,
                comment_status_str(status),
                now_ms()
            ],
        )
    }

    /// Marks the comment `sent` to `session_id`, keeping the file content at send time.
    pub fn mark_comment_sent(
        &self,
        id: CommentId,
        session_id: Ulid,
        snapshot: &str,
    ) -> Result<DocComment> {
        self.get_session(session_id)?;
        self.update_comment(
            id,
            "UPDATE doc_comment SET status = 'sent', sent_to = ?2, sent_snapshot = ?3,
                updated_at = ?4 WHERE id = ?1",
            params![id.to_string(), session_id.to_string(), snapshot, now_ms()],
        )
    }

    fn insert_comment(&self, c: &DocComment) -> Result<()> {
        self.conn.execute(
            &format!(
                "INSERT INTO doc_comment ({COMMENT_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"
            ),
            params![
                c.id.to_string(),
                c.project_id.to_string(),
                c.file_path,
                comment_context_str(c.context),
                c.parent_id.map(|id| id.to_string()),
                c.author,
                c.body,
                c.start_line,
                c.end_line,
                c.quote,
                comment_status_str(c.status),
                c.sent_to.map(|id| id.to_string()),
                c.sent_snapshot,
                c.created_at,
                c.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Runs `sql` (`?1` = comment id), then returns the updated comment.
    fn update_comment(
        &self,
        id: CommentId,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<DocComment> {
        if self.conn.execute(sql, params)? == 0 {
            return Err(StoreError::CommentNotFound(id));
        }
        self.get_comment(id)
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

fn comment_status_str(status: CommentStatus) -> &'static str {
    match status {
        CommentStatus::Open => "open",
        CommentStatus::Sent => "sent",
        CommentStatus::Addressed => "addressed",
        CommentStatus::Resolved => "resolved",
        CommentStatus::Outdated => "outdated",
    }
}

fn parse_comment_status(s: String) -> Result<CommentStatus> {
    Ok(match s.as_str() {
        "open" => CommentStatus::Open,
        "sent" => CommentStatus::Sent,
        "addressed" => CommentStatus::Addressed,
        "resolved" => CommentStatus::Resolved,
        "outdated" => CommentStatus::Outdated,
        _ => return Err(corrupt("doc_comment.status", s)),
    })
}

fn comment_context_str(context: CommentContext) -> &'static str {
    match context {
        CommentContext::Doc => "doc",
        CommentContext::Diff => "diff",
    }
}

fn parse_comment_context(s: String) -> Result<CommentContext> {
    Ok(match s.as_str() {
        "doc" => CommentContext::Doc,
        "diff" => CommentContext::Diff,
        _ => return Err(corrupt("doc_comment.context", s)),
    })
}

fn parse_comment_id(column: &'static str, s: String) -> Result<CommentId> {
    s.parse().map_err(|_| corrupt(column, s))
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
    default_agent: Option<String>,
    review_globs: Option<String>,
    created_at: i64,
    archived: bool,
    base_ref: Option<String>,
    run_autostart: bool,
    color: Option<String>,
    default_shell: Option<String>,
    pinned: bool,
}

impl RawProject {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            default_agent: row.get(3)?,
            review_globs: row.get(4)?,
            created_at: row.get(5)?,
            archived: row.get(6)?,
            base_ref: row.get(7)?,
            run_autostart: row.get(8)?,
            color: row.get(9)?,
            default_shell: row.get(10)?,
            pinned: row.get(11)?,
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
            run_commands: Vec::new(),
            run_autostart: self.run_autostart,
            default_agent: self.default_agent,
            default_shell: self.default_shell,
            review_globs,
            created_at: self.created_at,
            archived: self.archived,
            base_ref: self.base_ref,
            color: self.color,
            pinned: self.pinned,
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
    color: Option<String>,
    pinned: bool,
    plan_path: Option<String>,
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
            color: row.get(15)?,
            pinned: row.get(16)?,
            plan_path: row.get(17)?,
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
            terminal_title: None,
            color: self.color,
            native_session_id: self.native_session_id,
            state: parse_state(self.state)?,
            state_source: parse_source(self.state_source)?,
            state_changed_at: self.state_changed_at,
            exit_code: self.exit_code,
            cols: self.cols,
            rows: self.rows,
            started_at: self.started_at,
            ended_at: self.ended_at,
            pinned: self.pinned,
            plan_path: self.plan_path,
        })
    }
}

struct RawTab {
    id: String,
    project_id: String,
    title: String,
    position: f64,
    layout: String,
    pinned: bool,
}

impl RawTab {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            project_id: row.get(1)?,
            title: row.get(2)?,
            position: row.get(3)?,
            layout: row.get(4)?,
            pinned: row.get(5)?,
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
            pinned: self.pinned,
        })
    }
}

const TASK_COLUMNS: &str = "id, project_id, title, prompt, agent_profile, status, position,
    checkpoint, error_flag, created_at, updated_at, worktree, worktree_path, branch, fanout_of";

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
    worktree: bool,
    worktree_path: Option<String>,
    branch: Option<String>,
    fanout_of: Option<String>,
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
            worktree: row.get(11)?,
            worktree_path: row.get(12)?,
            branch: row.get(13)?,
            fanout_of: row.get(14)?,
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
            worktree: self.worktree,
            worktree_path: self.worktree_path,
            branch: self.branch,
            fanout_of: self
                .fanout_of
                .map(|id| parse_ulid("task.fanout_of", id))
                .transpose()?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const COMMENT_COLUMNS: &str = "id, project_id, file_path, context, parent_id, author, body,
    start_line, end_line, quote, status, sent_to, sent_snapshot, created_at, updated_at";

struct RawComment {
    id: String,
    project_id: String,
    file_path: String,
    context: String,
    parent_id: Option<String>,
    author: String,
    body: String,
    start_line: Option<u32>,
    end_line: Option<u32>,
    quote: Option<String>,
    status: String,
    sent_to: Option<String>,
    sent_snapshot: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl RawComment {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            project_id: row.get(1)?,
            file_path: row.get(2)?,
            context: row.get(3)?,
            parent_id: row.get(4)?,
            author: row.get(5)?,
            body: row.get(6)?,
            start_line: row.get(7)?,
            end_line: row.get(8)?,
            quote: row.get(9)?,
            status: row.get(10)?,
            sent_to: row.get(11)?,
            sent_snapshot: row.get(12)?,
            created_at: row.get(13)?,
            updated_at: row.get(14)?,
        })
    }

    fn into_comment(self) -> Result<DocComment> {
        Ok(DocComment {
            id: parse_comment_id("doc_comment.id", self.id)?,
            project_id: parse_ulid("doc_comment.project_id", self.project_id)?,
            file_path: self.file_path,
            context: parse_comment_context(self.context)?,
            parent_id: self
                .parent_id
                .map(|id| parse_comment_id("doc_comment.parent_id", id))
                .transpose()?,
            author: self.author,
            body: self.body,
            start_line: self.start_line,
            end_line: self.end_line,
            quote: self.quote,
            status: parse_comment_status(self.status)?,
            sent_to: self
                .sent_to
                .map(|id| parse_ulid("doc_comment.sent_to", id))
                .transpose()?,
            sent_snapshot: self.sent_snapshot,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

const TURN_COLUMNS: &str = "n, started_at, ended_at, start_tree, end_tree, files";

struct RawTurn {
    n: u32,
    started_at: i64,
    ended_at: Option<i64>,
    start_tree: Option<String>,
    end_tree: Option<String>,
    files: String,
}

impl RawTurn {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            n: row.get(0)?,
            started_at: row.get(1)?,
            ended_at: row.get(2)?,
            start_tree: row.get(3)?,
            end_tree: row.get(4)?,
            files: row.get(5)?,
        })
    }

    fn into_turn(self) -> Result<Turn> {
        let files =
            serde_json::from_str(&self.files).map_err(|_| corrupt("turn.files", self.files))?;
        Ok(Turn {
            n: self.n,
            started_at: self.started_at,
            ended_at: self.ended_at,
            start_tree: self.start_tree,
            end_tree: self.end_tree,
            files,
        })
    }
}

#[cfg(test)]
mod tests;
