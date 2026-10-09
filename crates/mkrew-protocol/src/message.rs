//! JSON messages carried in `0x01` frames (SPEC §7).
//!
//! ```json
//! {"type":"request","id":7,"method":"session.kill","params":{"session_id":"01J...","force":false}}
//! {"type":"response","id":7,"result":null}
//! {"type":"response","id":8,"error":{"code":"not_found","message":"session 01J... not found"}}
//! {"type":"event","event":"session.state","data":{"session_id":"01J...","state":"needs_input"}}
//! ```
//!
//! Response results are kept as raw JSON: the client knows which method it called and decodes
//! the matching result type (e.g. [`HelloResult`]) with [`Outcome::into_result`].

use std::path::PathBuf;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::files::{
    AiGenerateParams, AiGeneratedEvent, FsFindParams, FsListParams, FsReadParams, FsWriteParams,
    GitChangedEvent, GitCommitParams, GitDiffParams, GitDiscardParams, GitLogParams,
    GitRemoteParams, GitSetBaseParams, GitShowParams, GitStageParams, GitStatusParams, ProjectRef,
};

/// One JSON frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Request {
        id: u64,
        #[serde(flatten)]
        request: Request,
    },
    Response {
        id: u64,
        #[serde(flatten)]
        outcome: Outcome,
    },
    Event(Event),
}

/// Client → daemon requests (the M1 subset of SPEC §7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    #[serde(rename = "hello")]
    Hello(HelloParams),
    #[serde(rename = "daemon.status")]
    DaemonStatus,
    #[serde(rename = "daemon.stop")]
    DaemonStop,
    #[serde(rename = "project.add")]
    ProjectAdd(ProjectAddParams),
    #[serde(rename = "project.list")]
    ProjectList,
    #[serde(rename = "project.update")]
    ProjectUpdate(ProjectUpdateParams),
    /// Returns [`RunStarted`] (SPEC §10, ADR 0018).
    #[serde(rename = "project.run_start")]
    ProjectRunStart(RunStartParams),
    /// Returns [`RunStarted`]: stops the targets, waits for them, starts them again.
    #[serde(rename = "project.run_restart")]
    ProjectRunRestart(RunStartParams),
    /// Returns nothing (`null`); also when nothing runs.
    #[serde(rename = "project.run_stop")]
    ProjectRunStop(RunStopParams),
    /// Returns the [`Project`] (`archived: true`), also announced as `project.updated`: its
    /// live sessions are asked to stop and clients hide it; `project.add` of its folder
    /// brings it back (SPEC §7, ADR 0019).
    #[serde(rename = "project.archive")]
    ProjectArchive(ProjectRef),
    #[serde(rename = "tab.list")]
    TabList(TabListParams),
    #[serde(rename = "tab.create")]
    TabCreate(TabCreateParams),
    #[serde(rename = "tab.rename")]
    TabRename(TabRenameParams),
    #[serde(rename = "tab.close")]
    TabClose(TabRef),
    #[serde(rename = "tab.move")]
    TabMove(TabMoveParams),
    #[serde(rename = "tab.set_layout")]
    TabSetLayout(TabSetLayoutParams),
    #[serde(rename = "task.list")]
    TaskList(TaskListParams),
    #[serde(rename = "task.create")]
    TaskCreate(TaskCreateParams),
    #[serde(rename = "task.update")]
    TaskUpdate(TaskUpdateParams),
    #[serde(rename = "task.move")]
    TaskMove(TaskMoveParams),
    #[serde(rename = "task.delete")]
    TaskDelete(TaskRef),
    /// Returns [`SessionSpawnResult`].
    #[serde(rename = "task.run")]
    TaskRun(TaskRunParams),
    /// Returns [`UndoPreview`].
    #[serde(rename = "task.undo_preview")]
    TaskUndoPreview(TaskRef),
    /// Returns nothing (`null`).
    #[serde(rename = "task.undo")]
    TaskUndo(TaskUndoParams),
    /// Returns [`TaskMergeResult`] (SPEC §20E.1).
    #[serde(rename = "task.merge")]
    TaskMerge(TaskConfirmParams),
    /// Returns [`RemoveWorktreeResult`] (SPEC §20E.1).
    #[serde(rename = "task.remove_worktree")]
    TaskRemoveWorktree(TaskConfirmParams),
    /// Returns the child `Task[]` (SPEC §20E.2).
    #[serde(rename = "task.fanout")]
    TaskFanout(TaskFanoutParams),
    #[serde(rename = "session.spawn")]
    SessionSpawn(SessionSpawnParams),
    #[serde(rename = "session.list")]
    SessionList(SessionListParams),
    #[serde(rename = "session.attach")]
    SessionAttach(SessionRef),
    #[serde(rename = "session.detach")]
    SessionDetach(SessionRef),
    #[serde(rename = "session.input")]
    SessionInput(SessionInputParams),
    #[serde(rename = "session.send")]
    SessionSend(SessionSendParams),
    /// Removes a queued `session.send` text; `not_found` once written (SPEC §20B.2).
    #[serde(rename = "session.unqueue")]
    SessionUnqueue(SessionUnqueueParams),
    #[serde(rename = "session.wait")]
    SessionWait(SessionWaitParams),
    #[serde(rename = "session.resize")]
    SessionResize(SessionResizeParams),
    #[serde(rename = "session.kill")]
    SessionKill(SessionKillParams),
    /// Returns nothing (`null`): leaves the session out of `session.list` for good (its log
    /// and turns stay); a live one is asked to stop first (SPEC §7, ADR 0019).
    #[serde(rename = "session.remove")]
    SessionRemove(SessionRef),
    /// Returns the renamed [`Session`], also announced as `session.updated`; a `null` or blank
    /// `title` clears it. Run sessions keep their `run:` name (ADR 0031).
    #[serde(rename = "session.rename")]
    SessionRename(SessionRenameParams),
    /// Returns [`SessionHistory`]; works for exited sessions too.
    #[serde(rename = "session.history")]
    SessionHistory(SessionRef),
    /// Returns [`SessionReadResult`]; live sessions only (SPEC §20A).
    #[serde(rename = "session.read")]
    SessionRead(SessionReadParams),
    #[serde(rename = "hook.report")]
    HookReport(HookReportParams),
    #[serde(rename = "agent.profiles")]
    AgentProfiles,
    /// Returns `DocFile[]`, sorted by path.
    #[serde(rename = "doc.list")]
    DocList(DocListParams),
    /// Returns [`DocContent`].
    #[serde(rename = "doc.read")]
    DocRead(DocReadParams),
    /// Returns `DocComment[]`: thread roots and replies, oldest first.
    #[serde(rename = "comment.list")]
    CommentList(CommentListParams),
    /// Returns the new [`DocComment`] (`open`, authored by the user).
    #[serde(rename = "comment.create")]
    CommentCreate(CommentCreateParams),
    /// Returns the reply [`DocComment`].
    #[serde(rename = "comment.reply")]
    CommentReply(CommentReplyParams),
    /// Returns the updated thread root [`DocComment`].
    #[serde(rename = "comment.resolve")]
    CommentResolve(CommentRef),
    /// Returns the updated thread root [`DocComment`].
    #[serde(rename = "comment.reopen")]
    CommentReopen(CommentRef),
    /// Returns the updated thread roots (`sent`), as `DocComment[]` in request order.
    #[serde(rename = "comment.send_to_agent")]
    CommentSendToAgent(CommentSendParams),
    /// Returns [`GitStatus`].
    #[serde(rename = "git.status")]
    GitStatus(GitStatusParams),
    /// Returns [`FileDiff`].
    #[serde(rename = "git.diff")]
    GitDiff(GitDiffParams),
    #[serde(rename = "git.stage")]
    GitStage(GitStageParams),
    #[serde(rename = "git.unstage")]
    GitUnstage(GitStageParams),
    #[serde(rename = "git.discard")]
    GitDiscard(GitDiscardParams),
    /// Returns [`GitCommitResult`].
    #[serde(rename = "git.commit")]
    GitCommit(GitCommitParams),
    /// Returns `Commit[]`, newest first.
    #[serde(rename = "git.log")]
    GitLog(GitLogParams),
    /// Returns [`CommitDetail`].
    #[serde(rename = "git.show")]
    GitShow(GitShowParams),
    /// Returns `string[]`: the git repositories of the project folder (ADR 0029). `""` first
    /// when the folder itself is in a repository, then each direct child folder holding `.git`.
    #[serde(rename = "git.repos")]
    GitRepos(ProjectRef),
    /// One-shot text from an agent CLI (SPEC §6 `oneshot`, §12B, §20B.1). Returns nothing
    /// (`null`) once the run started; the answer follows as `ai.generated`.
    #[serde(rename = "ai.generate")]
    AiGenerate(AiGenerateParams),
    /// Returns nothing (`null`); broadcasts `project.updated`.
    #[serde(rename = "git.set_base")]
    GitSetBase(GitSetBaseParams),
    #[serde(rename = "git.fetch")]
    GitFetch(GitRemoteParams),
    /// Pushes the current branch to its upstream, never with force.
    #[serde(rename = "git.push")]
    GitPush(GitRemoteParams),
    /// Returns `FsEntry[]`.
    #[serde(rename = "fs.list")]
    FsList(FsListParams),
    /// Returns [`FsContent`].
    #[serde(rename = "fs.read")]
    FsRead(FsReadParams),
    /// Returns [`FsWritten`] (ADR 0028).
    #[serde(rename = "fs.write")]
    FsWrite(FsWriteParams),
    /// Returns `FsMatch[]`, best first.
    #[serde(rename = "fs.find")]
    FsFind(FsFindParams),
    /// Returns [`AttachmentSaveResult`] (SPEC §20B.5).
    #[serde(rename = "attachment.save")]
    AttachmentSave(AttachmentSaveParams),
    /// Returns `Turn[]`, oldest first (SPEC §20C).
    #[serde(rename = "turn.list")]
    TurnList(SessionRef),
    /// Returns [`UndoPreview`] with `checkpoint` = the turn's start tree (SPEC §20C.2).
    #[serde(rename = "turn.restore_preview")]
    TurnRestorePreview(TurnRef),
    /// Returns nothing (`null`).
    #[serde(rename = "turn.restore")]
    TurnRestore(TurnRestoreParams),
    /// Returns [`SessionFiles`]: the files each session of the project changed since
    /// `since_ms` (SPEC §20C.4).
    #[serde(rename = "session.files")]
    SessionFiles(SessionFilesParams),
    /// Returns `SessionUsageEvent[]`: the newest token usage of each session of the project
    /// that has one (SPEC §20E.3).
    #[serde(rename = "session.usage")]
    SessionUsage(ProjectRef),
    /// Returns `SessionPortsEvent[]`: the ports of the project's live run sessions (SPEC §10).
    #[serde(rename = "session.ports")]
    SessionPorts(ProjectRef),
}

/// Body of a response: `"result"` on success, `"error"` on failure.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Result(serde_json::Value),
    Error(ErrorBody),
}

impl Outcome {
    /// Successful outcome carrying `result`.
    pub fn ok<T: Serialize>(result: &T) -> Result<Self, serde_json::Error> {
        Ok(Self::Result(serde_json::to_value(result)?))
    }

    /// Failed outcome.
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Error(ErrorBody {
            code,
            message: message.into(),
        })
    }

    /// Decodes a successful result as `T`; a daemon error becomes [`ResponseError::Daemon`].
    pub fn into_result<T: DeserializeOwned>(self) -> Result<T, ResponseError> {
        match self {
            Self::Result(value) => Ok(serde_json::from_value(value)?),
            Self::Error(body) => Err(ResponseError::Daemon(body)),
        }
    }
}

/// Error returned by [`Outcome::into_result`].
#[derive(Debug, thiserror::Error)]
pub enum ResponseError {
    #[error("{} ({:?})", .0.message, .0.code)]
    Daemon(ErrorBody),
    #[error("unexpected result shape: {0}")]
    Decode(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

/// Machine-readable error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// `hello.protocol_version` differs from the daemon's; the client should restart the daemon.
    VersionMismatch,
    /// The first request on a connection was not `hello`.
    HelloRequired,
    /// Unknown method or malformed params.
    InvalidRequest,
    NotFound,
    AlreadyExists,
    /// The request is valid but cannot run in the current state (e.g. session exited).
    InvalidState,
    Timeout,
    Internal,
}

/// Daemon → client push messages. Terminal output is not here: it uses binary frames.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", content = "data")]
pub enum Event {
    #[serde(rename = "session.state")]
    SessionState(SessionStateEvent),
    #[serde(rename = "session.exited")]
    SessionExited(SessionExitedEvent),
    #[serde(rename = "project.updated")]
    ProjectUpdated(Project),
    /// A tab was created, renamed, moved or got a new layout.
    #[serde(rename = "tab.updated")]
    TabUpdated(TerminalTab),
    /// A task was created, edited, moved or changed status.
    #[serde(rename = "task.updated")]
    TaskUpdated(Task),
    #[serde(rename = "task.deleted")]
    TaskDeleted(TaskDeletedEvent),
    /// Sent only to an attached client whose output queue overflowed: its queued output was
    /// dropped and this screen replaces it; output frames continue from this point (SPEC §7).
    #[serde(rename = "session.snapshot")]
    SessionSnapshot(SessionSnapshotEvent),
    /// The user clicked the OS notification of a session (SPEC §8); the UI focuses its pane.
    #[serde(rename = "notification.clicked")]
    NotificationClicked(NotificationClickedEvent),
    /// Two or more sessions of a project edited the same file within 60 s (SPEC §7, M5).
    #[serde(rename = "conflict.detected")]
    ConflictDetected(ConflictDetectedEvent),
    /// A Markdown file of a watched project changed content (SPEC §12); its comments are
    /// re-anchored and announced with `comment.updated` right after.
    #[serde(rename = "doc.changed")]
    DocChanged(DocChangedEvent),
    /// A review comment was created, replied to, re-anchored or changed status (SPEC §12).
    #[serde(rename = "comment.updated")]
    CommentUpdated(Box<DocComment>),
    /// The git state of a workspace changed (debounced, ≤ once per 300 ms; SPEC §12B).
    #[serde(rename = "git.changed")]
    GitChanged(GitChangedEvent),
    /// The answer to the `ai.generate` with this `generation_id`.
    #[serde(rename = "ai.generated")]
    AiGenerated(AiGeneratedEvent),
    /// A turn of an agent session ended (SPEC §20A.3). A turn resumed after `needs_input` ends
    /// again with the same `n`; the later event replaces the earlier one.
    #[serde(rename = "session.turn")]
    SessionTurn(SessionTurnEvent),
    /// A session's send queue changed; carries the whole queue (SPEC §20B.2).
    #[serde(rename = "session.queue")]
    SessionQueue(SessionQueueEvent),
    /// A hook reported that a session's agent edited a file (SPEC §20C.5).
    #[serde(rename = "session.edited")]
    SessionEdited(SessionEditedEvent),
    /// A session's token usage after a turn ended (SPEC §20E.3).
    #[serde(rename = "session.usage")]
    SessionUsage(SessionUsageEvent),
    /// A run session printed a new port (SPEC §10).
    #[serde(rename = "session.ports")]
    SessionPorts(SessionPortsEvent),
    /// A session's title changed (`session.rename`, ADR 0031); carries the whole session.
    #[serde(rename = "session.updated")]
    SessionUpdated(Session),
    /// An agent session's terminal title changed, cleaned (`None` = cleared; ADR 0031).
    #[serde(rename = "session.terminal_title")]
    SessionTerminalTitle(SessionTerminalTitleEvent),
}

// ---- shared models (SPEC §5) ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    Ui,
    Cli,
    Hook,
}

/// Session state (SPEC §5 `session.state`, §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Starting,
    Working,
    NeedsInput,
    Idle,
    Done,
    Exited,
    Unknown,
}

/// What decided the current session state (SPEC §6 "Source"; shown on hover, §11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateSource {
    /// An agent hook (`hook.report`).
    Hook,
    /// Screen detection: output, quiet periods, `needs_input_patterns`.
    Screen,
    /// The daemon itself: spawn, exit, restart.
    #[default]
    Daemon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    #[default]
    Agent,
    Shell,
}

/// Workspace color names (SPEC §14, ADR 0020); clients map them to theme tokens.
pub const WORKSPACE_COLORS: [&str; 8] = [
    "red", "orange", "yellow", "green", "teal", "blue", "purple", "pink",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: Ulid,
    pub name: String,
    pub path: String,
    /// The Run button's commands, in order (SPEC §10, ADR 0018).
    #[serde(default)]
    pub run_commands: Vec<RunCommand>,
    /// Run every command after the daemon starts.
    #[serde(default)]
    pub run_autostart: bool,
    pub default_agent: Option<String>,
    /// Shell profile for new shell panes (ADR 0032); `None` = not picked yet, so a new shell
    /// pane is refused until the user picks one.
    #[serde(default)]
    pub default_shell: Option<String>,
    /// `None` = default globs (SPEC §12).
    pub review_globs: Option<Vec<String>>,
    /// Unix ms.
    pub created_at: i64,
    pub archived: bool,
    /// Git view base; `None` = auto-detect (SPEC §12B). Set with `git.set_base`.
    #[serde(default)]
    pub base_ref: Option<String>,
    /// One of [`WORKSPACE_COLORS`]; `None` = no color.
    #[serde(default)]
    pub color: Option<String>,
}

/// One command of a project's Run button (SPEC §10, ADR 0018): typed into its own shell in
/// `dir`, with `env_file` then `env` added to the environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCommand {
    pub name: String,
    /// Relative to the project folder; `.` = the folder itself.
    pub dir: String,
    pub command: String,
    /// Relative to `dir`, e.g. `.env`.
    #[serde(default)]
    pub env_file: Option<String>,
    /// Applied after `env_file`.
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: Ulid,
    pub task_id: Option<Ulid>,
    pub project_id: Ulid,
    pub agent_profile: String,
    pub kind: SessionKind,
    pub title: Option<String>,
    /// The title the session's agent gave its terminal (OSC 0/2), cleaned; daemon memory
    /// only, filled in `session.list` / `session.rename` / `session.updated`. Shown when
    /// `title` is unset (ADR 0031).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_title: Option<String>,
    /// The agent's own session id, used for resume (SPEC §6).
    pub native_session_id: Option<String>,
    pub state: SessionState,
    pub state_source: StateSource,
    /// Unix ms of the last state change.
    pub state_changed_at: i64,
    pub exit_code: Option<i32>,
    pub cols: u16,
    pub rows: u16,
    /// Unix ms.
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

/// A terminal tab of a workspace (SPEC §5 `terminal_tab`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalTab {
    pub id: Ulid,
    pub project_id: Ulid,
    pub title: String,
    /// Tabs are ordered by ascending position.
    pub position: f64,
    /// Split tree whose leaves are session ids (SPEC §5); opaque to the daemon.
    /// `null` until the first pane is placed.
    pub layout: serde_json::Value,
}

/// Board column of a task (SPEC §5 `task.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    Running,
    NeedsInput,
    Review,
    Done,
}

/// A task card of a workspace board (SPEC §5 `task`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    pub id: Ulid,
    pub project_id: Ulid,
    pub title: String,
    pub prompt: String,
    /// `None` = the project's `default_agent` when the task runs.
    pub agent_profile: Option<String>,
    pub status: TaskStatus,
    /// Cards of a column are ordered by ascending position.
    pub position: f64,
    /// git HEAD before the run (M5).
    pub checkpoint: Option<String>,
    /// The last run exited non-zero (SPEC §5).
    pub error_flag: bool,
    /// Runs in its own git worktree (SPEC §20E.1).
    #[serde(default)]
    pub worktree: bool,
    /// The task's worktree folder once created.
    #[serde(default)]
    pub worktree_path: Option<String>,
    /// The worktree's branch (`mkrew/…`) once created.
    #[serde(default)]
    pub branch: Option<String>,
    /// The original task of a fan-out (SPEC §20E.2).
    #[serde(default)]
    pub fanout_of: Option<Ulid>,
    /// Unix ms.
    pub created_at: i64,
    /// Unix ms.
    pub updated_at: i64,
}

/// Id of a review comment: a ULID written with a `c_` prefix (`c_01JX…`, SPEC §12) so it
/// stands out in the prompt an agent gets. Parsing accepts it with or without the prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommentId(pub Ulid);

impl CommentId {
    pub const PREFIX: &'static str = "c_";

    pub fn new() -> Self {
        Self(Ulid::new())
    }
}

impl Default for CommentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CommentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}{}", Self::PREFIX, self.0)
    }
}

impl std::str::FromStr for CommentId {
    type Err = ulid::DecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ulid::from_string(s.strip_prefix(Self::PREFIX).unwrap_or(s)).map(Self)
    }
}

impl Serialize for CommentId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for CommentId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Review state of a comment thread (SPEC §5 `doc_comment.status`, §12). Replies keep
/// `open`; only the thread root's status means anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentStatus {
    Open,
    /// Typed into an agent session (`comment.send_to_agent`).
    Sent,
    /// The agent answered with `mk comment reply`.
    Addressed,
    Resolved,
    /// The quoted passage is gone from the file; kept visible, never auto-deleted.
    Outdated,
}

/// Where a comment was written (SPEC §5 `doc_comment.context`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentContext {
    /// On a Markdown file in the Docs view (SPEC §12).
    #[default]
    Doc,
    /// On new-side lines of a diff in the Git view (SPEC §12B): the agent fixes the code.
    Diff,
}

/// A review comment (SPEC §5 `doc_comment`) on a Markdown file or on diff lines of any file:
/// a thread root anchored to lines, or a reply (`parent_id` set, no anchor).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocComment {
    pub id: CommentId,
    pub project_id: Ulid,
    /// Relative to the project root, `/` separators.
    pub file_path: String,
    /// Replies carry their thread root's.
    #[serde(default)]
    pub context: CommentContext,
    /// The thread root this reply belongs to; `None` = a thread root.
    pub parent_id: Option<CommentId>,
    /// [`DocComment::USER`] or the agent profile id of the replying session.
    pub author: String,
    pub body: String,
    /// First anchored line, 1-based; `None` on replies.
    pub start_line: Option<u32>,
    /// Last anchored line, 1-based, inclusive.
    pub end_line: Option<u32>,
    /// The selected text the anchor follows when the file changes.
    pub quote: Option<String>,
    pub status: CommentStatus,
    /// The session the comment was last sent to.
    pub sent_to: Option<Ulid>,
    /// The file content when it was sent, for "View changes".
    pub sent_snapshot: Option<String>,
    /// Unix ms.
    pub created_at: i64,
    /// Unix ms.
    pub updated_at: i64,
}

impl DocComment {
    /// `author` of comments written in the UI.
    pub const USER: &'static str = "user";
}

// ---- request params ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloParams {
    pub client: ClientKind,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAddParams {
    pub path: String,
    /// Defaults to the directory name.
    #[serde(default)]
    pub name: Option<String>,
}

/// `project.update`: fields left out stay unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectUpdateParams {
    pub project_id: Ulid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `Some(None)` (JSON `null`) clears the default agent.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub default_agent: Option<Option<String>>,
    /// `Some(None)` (JSON `null`) clears the default shell.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub default_shell: Option<Option<String>>,
    /// Replaces the whole list; checked and trimmed by the daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_commands: Option<Vec<RunCommand>>,
    /// Run every command after the daemon starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_autostart: Option<bool>,
    /// One of [`WORKSPACE_COLORS`]; `Some(None)` (JSON `null`) clears it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub color: Option<Option<String>>,
}

/// `project.run_start` / `project.run_restart`: `name` = one command, none = all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStartParams {
    pub project_id: Ulid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
}

/// `project.run_stop`: `name` = one command, none = every run session of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStopParams {
    pub project_id: Ulid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Answer of `project.run_start` / `project.run_restart`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStarted {
    /// The `run` tab; `None` when it could not be written.
    pub tab_id: Option<Ulid>,
    /// The targeted commands in config order.
    pub sessions: Vec<RunSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSession {
    pub name: String,
    pub session_id: Ulid,
    /// False when it was already running.
    pub started: bool,
}

/// Terminal size used when the client has no pane yet (CLI, auto-start, fan-out children).
pub const DEFAULT_COLS: u16 = 120;
pub const DEFAULT_ROWS: u16 = 30;

fn default_cols() -> u16 {
    DEFAULT_COLS
}

fn default_rows() -> u16 {
    DEFAULT_ROWS
}

/// Deserializes a field that is present (possibly `null`) as `Some`; with
/// `#[serde(default)]` a missing field stays `None`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabListParams {
    pub project_id: Ulid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabCreateParams {
    pub project_id: Ulid,
    pub title: String,
    /// Defaults to after the last tab of the project.
    #[serde(default)]
    pub position: Option<f64>,
}

/// Params of requests that only name a tab (`tab.close`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabRef {
    pub tab_id: Ulid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabRenameParams {
    pub tab_id: Ulid,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TabMoveParams {
    pub tab_id: Ulid,
    pub position: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabSetLayoutParams {
    pub tab_id: Ulid,
    pub layout: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TaskListParams {
    #[serde(default)]
    pub project_id: Option<Ulid>,
    #[serde(default)]
    pub status: Option<TaskStatus>,
}

/// `task.create`: a new card in the project's Todo column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskCreateParams {
    pub project_id: Ulid,
    pub title: String,
    #[serde(default)]
    pub prompt: String,
    /// `None` = the project's `default_agent` at run time; else an agent profile.
    #[serde(default)]
    pub agent_profile: Option<String>,
    /// Defaults to after the last Todo card of the project.
    #[serde(default)]
    pub position: Option<f64>,
    /// Run in a git worktree (SPEC §20E.1).
    #[serde(default)]
    pub worktree: bool,
}

/// `task.update`: fields left out stay unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskUpdateParams {
    pub task_id: Ulid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    /// `Some(None)` (JSON `null`) goes back to the project's default agent.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub agent_profile: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<bool>,
}

/// `task.move`: drop a card into a column at a position (drag & drop, approve).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskMoveParams {
    pub task_id: Ulid,
    pub status: TaskStatus,
    pub position: f64,
}

/// Params of requests that only name a task (`task.delete`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRef {
    pub task_id: Ulid,
}

/// `task.run`: start the task's agent in the project folder with the task's prompt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRunParams {
    pub task_id: Ulid,
    pub cols: u16,
    pub rows: u16,
}

/// `task.undo`: discard everything since the task's checkpoint. Refused unless `confirm`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskUndoParams {
    pub task_id: Ulid,
    #[serde(default)]
    pub confirm: bool,
    /// The previewed [`UndoPreview::created`]: only these untracked files are deleted, so a
    /// file that appeared after the preview was shown survives.
    #[serde(default)]
    pub created: Vec<String>,
    /// The previewed [`UndoPreview::fingerprint`]: refused (`invalid_state`) when the tracked
    /// changes no longer match it, so nothing unseen is restored away.
    #[serde(default)]
    pub fingerprint: String,
}

/// `task.merge` / `task.remove_worktree`: refused unless `confirm`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskConfirmParams {
    pub task_id: Ulid,
    #[serde(default)]
    pub confirm: bool,
}

/// `task.fanout`: run the task with 2–5 agent profiles at once, each in a child task with its
/// own worktree. `cols`/`rows` = the children's terminal size (no pane exists for them yet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskFanoutParams {
    pub task_id: Ulid,
    pub agent_profiles: Vec<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
}

impl TaskFanoutParams {
    /// Fewest and most profiles of one fan-out (SPEC §20E.2).
    pub const MIN_PROFILES: usize = 2;
    pub const MAX_PROFILES: usize = 5;
}

/// `task.merge` result: `merged`, or aborted on `conflicts` (repo-relative paths) with the
/// user's checkout unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskMergeResult {
    pub merged: bool,
    pub conflicts: Vec<String>,
}

/// `task.remove_worktree` result: `branch_kept` = the branch was not merged, so `git branch -d`
/// kept it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveWorktreeResult {
    pub branch_kept: bool,
}

/// A turn of a session (`turn.restore_preview`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRef {
    pub session_id: Ulid,
    pub n: u32,
}

/// `turn.restore`: put the project folder back to how it was before turn `n`. Same rules as
/// [`TaskUndoParams`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRestoreParams {
    pub session_id: Ulid,
    pub n: u32,
    #[serde(default)]
    pub confirm: bool,
    /// The previewed [`UndoPreview::created`]: only these files are deleted.
    #[serde(default)]
    pub created: Vec<String>,
    /// The previewed [`UndoPreview::fingerprint`].
    #[serde(default)]
    pub fingerprint: String,
}

/// `task.undo_preview` result: what `task.undo` would discard (ADR 0005). Paths are
/// project-relative with `/` separators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoPreview {
    pub checkpoint: String,
    /// Opaque hash of the tracked changes (`changed`, `added` and the whole diff).
    pub fingerprint: String,
    /// Tracked files that differ from the checkpoint; restored to it.
    pub changed: Vec<String>,
    /// Files tracked since the checkpoint (absent from it); deleted.
    pub added: Vec<String>,
    /// Untracked, non-ignored files created since the checkpoint; deleted.
    pub created: Vec<String>,
    /// `git diff <checkpoint>` of the tracked files, cut at [`UNDO_DIFF_LIMIT`] bytes.
    pub diff: String,
    pub diff_truncated: bool,
}

/// Largest diff carried by an [`UndoPreview`] (SPEC §12B: bigger diffs show a summary).
pub const UNDO_DIFF_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSpawnParams {
    pub project_id: Ulid,
    /// `None` = the default for `kind`: the project's `default_agent` for an agent, its
    /// `default_shell` for a shell. A named profile decides the session kind itself.
    #[serde(default)]
    pub agent_profile: Option<String>,
    #[serde(default)]
    pub kind: SessionKind,
    /// The tab the pane goes into; a tab without a layout gets this session as its only pane.
    #[serde(default)]
    pub tab_id: Option<Ulid>,
    #[serde(default)]
    pub title: Option<String>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionListParams {
    #[serde(default)]
    pub project_id: Option<Ulid>,
}

/// Params of requests that only name a session (`session.attach`, `session.detach`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRef {
    pub session_id: Ulid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRenameParams {
    pub session_id: Ulid,
    /// Trimmed by the daemon; `None` or blank clears the title.
    #[serde(default)]
    pub title: Option<String>,
}

/// `session.history` result: the session's output log files on disk (SPEC §8), oldest first
/// (`<id>.log.1` then `<id>.log`), only those that exist — empty if nothing was ever written.
/// For a live session this is best effort: its newest output may still sit in the daemon's
/// write buffer, and the log may rotate before the files are read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHistory {
    pub files: Vec<PathBuf>,
}

/// `session.read`: the last `lines` non-blank rows of a live session's screen (SPEC §20A).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionReadParams {
    pub session_id: Ulid,
    /// 1–200.
    #[serde(default = "default_read_lines")]
    pub lines: u16,
}

fn default_read_lines() -> u16 {
    20
}

/// `session.read` result: plain text rows, top to bottom, and the session's current state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionReadResult {
    pub lines: Vec<String>,
    pub state: SessionState,
    /// Unix ms.
    pub state_changed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionInputParams {
    pub session_id: Ulid,
    /// Raw bytes for the PTY, base64 on the wire.
    #[serde(with = "base64_bytes")]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSendParams {
    pub session_id: Ulid,
    pub text: String,
    /// Press Enter after the text.
    pub submit: bool,
}

/// `session.send` result (SPEC §20B.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSendResult {
    pub send_id: Ulid,
    /// The text waits in the session's queue (it was not handed to the PTY right away).
    pub queued: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUnqueueParams {
    pub session_id: Ulid,
    pub send_id: Ulid,
}

/// `attachment.save`: an image pasted into an agent pane (SPEC §20B.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentSaveParams {
    pub session_id: Ulid,
    /// `png`, `jpg`, `gif` or `webp`.
    pub ext: String,
    /// The file content (≤ 10 MiB), base64 on the wire.
    #[serde(with = "base64_bytes")]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachmentSaveResult {
    /// Absolute path of the saved file.
    pub path: String,
}

/// Event `session.queue`: the texts still waiting, in delivery order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionQueueEvent {
    pub session_id: Ulid,
    pub items: Vec<QueuedSend>,
}

/// One waiting `session.send` text, as sent (without the submit Enter).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedSend {
    pub send_id: Ulid,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWaitParams {
    pub session_id: Ulid,
    pub until: Vec<SessionState>,
    /// `None` = wait forever.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResizeParams {
    pub session_id: Ulid,
    pub cols: u16,
    pub rows: u16,
}

/// `hook.report`, sent by `mk hook` from inside a session's PTY (SPEC §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookReportParams {
    /// `MKREW_SESSION_ID` of the PTY; must be a live session.
    pub mkrew_session_id: Ulid,
    /// `claude`, `codex` or `fake`.
    pub agent: String,
    /// The agent's own event name (e.g. `Notification`, `agent-turn-complete`).
    pub event: String,
    /// The JSON the agent handed to the hook; stored as is.
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocListParams {
    pub project_id: Ulid,
}

/// `doc.read`: one file of the project, `file_path` relative to its root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocReadParams {
    pub project_id: Ulid,
    pub file_path: String,
}

/// `comment.list`: the comments of one file, or of every file of the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentListParams {
    pub project_id: Ulid,
    #[serde(default)]
    pub file_path: Option<String>,
    /// Only comments written in this context; `None` = all.
    #[serde(default)]
    pub context: Option<CommentContext>,
}

/// `comment.create`: a new thread anchored to a selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentCreateParams {
    pub project_id: Ulid,
    pub file_path: String,
    /// 1-based, inclusive. Diff comments: new-side lines.
    pub start_line: u32,
    pub end_line: u32,
    pub quote: String,
    pub body: String,
    #[serde(default)]
    pub context: CommentContext,
}

/// `comment.reply`: a reply to a thread (a reply to a reply joins the same thread).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentReplyParams {
    pub comment_id: CommentId,
    pub body: String,
    /// `MKREW_SESSION_ID` when an agent replies via `mk comment reply`; `None` = the user.
    #[serde(default)]
    pub mkrew_session_id: Option<Ulid>,
}

/// Params of requests that only name a comment (`comment.resolve`, `comment.reopen`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentRef {
    pub comment_id: CommentId,
}

/// `comment.send_to_agent`: threads of one file, typed as one prompt into a live session of
/// the same project (SPEC §12). Reply ids stand for their thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentSendParams {
    pub comment_ids: Vec<CommentId>,
    pub session_id: Ulid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionKillParams {
    pub session_id: Ulid,
    #[serde(default)]
    pub force: bool,
}

// ---- results and event payloads ----

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResult {
    pub daemon_version: String,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaemonStatusResult {
    pub daemon_version: String,
    pub protocol_version: u32,
    pub pid: u32,
    /// Unix ms.
    pub started_at: i64,
    pub sessions: u32,
    /// Why `settings.toml` could not be loaded (`file:line:col: message`); the daemon keeps
    /// its last good settings meanwhile (SPEC §20.0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSpawnResult {
    pub session_id: Ulid,
}

/// `session.attach`: the current screen; output frames that follow continue from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAttachResult {
    pub state: SessionState,
    pub snapshot: ScreenSnapshot,
}

/// The visible screen of a session's headless emulator plus the history above it (SPEC §7, §8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSnapshot {
    pub cols: u16,
    pub rows: u16,
    /// One entry per visible row, top to bottom. Trailing default blanks are omitted.
    pub lines: Vec<Vec<CellRun>>,
    /// The main screen's scrollback, oldest first, same row format as `lines` (at most the
    /// daemon's 200 lines). Empty on the alternate screen, which has no history.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<Vec<CellRun>>,
    pub cursor: CursorSnapshot,
    pub modes: TermModes,
}

/// Adjacent cells of one row with the same style. A run is either all single-width or all
/// double-width chars ([`CellRun::WIDE`]), so its column count follows from `text` alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellRun {
    pub text: String,
    pub fg: TermColor,
    pub bg: TermColor,
    /// Bitset of [`CellRun::BOLD`] etc.
    pub flags: u16,
}

impl CellRun {
    pub const BOLD: u16 = 1;
    pub const DIM: u16 = 1 << 1;
    pub const ITALIC: u16 = 1 << 2;
    pub const UNDERLINE: u16 = 1 << 3;
    pub const INVERSE: u16 = 1 << 4;
    pub const HIDDEN: u16 = 1 << 5;
    pub const STRIKEOUT: u16 = 1 << 6;
    /// Every char occupies two columns.
    pub const WIDE: u16 = 1 << 7;
}

/// Cell color: the terminal default, one of the 256 indexed colors (0–15 = ANSI), or RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TermColor {
    Default,
    Indexed(u8),
    Rgb([u8; 3]),
}

/// Cursor in screen coordinates (row 0 = top visible line).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorSnapshot {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
}

/// Terminal modes a client needs to encode input and pick the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TermModes {
    pub alt_screen: bool,
    pub app_cursor: bool,
    pub app_keypad: bool,
    pub bracketed_paste: bool,
    /// Any mouse reporting mode is on.
    pub mouse: bool,
    pub sgr_mouse: bool,
    pub focus_events: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSnapshotEvent {
    pub session_id: Ulid,
    pub snapshot: ScreenSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWaitResult {
    pub state: SessionState,
    pub exit_code: Option<i32>,
}

/// `agent.profiles`: loaded profiles plus one entry per broken profile file (SPEC §6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfilesResult {
    pub profiles: Vec<AgentProfileInfo>,
    pub errors: Vec<ProfileErrorInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentProfileInfo {
    /// File stem, used as `agent_profile` in `session.spawn`.
    pub id: String,
    pub name: String,
    pub kind: SessionKind,
    /// Empty for a shell profile using the OS default shell.
    pub command: String,
    /// The profile's badge (ADR 0025); absent from older daemons.
    #[serde(default)]
    pub icon: Option<String>,
    /// The profile's program is found (PATH with PATHEXT, or its absolute path); the UI lists
    /// only installed shells (ADR 0032).
    #[serde(default)]
    pub installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileErrorInfo {
    pub file: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStateEvent {
    pub session_id: Ulid,
    pub state: SessionState,
    pub source: StateSource,
    /// Unix ms.
    pub changed_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExitedEvent {
    pub session_id: Ulid,
    pub exit_code: Option<i32>,
}

/// Token totals of an agent session, read from its transcript (SPEC §20E.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsageEvent {
    pub session_id: Ulid,
    /// Input tokens over the agent's whole conversation.
    pub input: u64,
    /// Output tokens over the agent's whole conversation.
    pub output: u64,
    /// Context size of the newest message.
    pub context: u64,
    /// The profile's `context_window`, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
}

/// A run session's ports (SPEC §10): every port it printed so far, at most 4, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPortsEvent {
    pub session_id: Ulid,
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTerminalTitleEvent {
    pub session_id: Ulid,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTurnEvent {
    pub session_id: Ulid,
    /// Per session, from 1.
    pub n: u32,
    /// Unix ms.
    pub started_at: i64,
    /// Unix ms.
    pub ended_at: i64,
    /// Files attributed to the session during the turn, sorted, no duplicates.
    pub files: Vec<TurnFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionEditedEvent {
    pub session_id: Ulid,
    /// Relative to the project root, `/` separators.
    pub path: String,
    /// 1-based line of the edit, when the hook payload names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionFilesParams {
    pub project_id: Ulid,
    /// Unix ms.
    pub since_ms: i64,
}

/// `session.files` result: per session, the project-relative paths it changed, sorted.
pub type SessionFiles = std::collections::BTreeMap<Ulid, Vec<String>>;

/// One file of a [`SessionTurnEvent`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnFile {
    /// Relative to the project root, `/` separators.
    pub path: String,
    /// Lines added during the turn (from M11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<u32>,
    /// Lines removed during the turn (from M11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub removed: Option<u32>,
}

/// A persisted turn of a session (SPEC §20C, table `turn`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn {
    /// Per session, from 1.
    pub n: u32,
    /// Unix ms.
    pub started_at: i64,
    /// Unix ms; `None` while the turn runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// Git tree of the working state at turn start; `None` outside a git repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_tree: Option<String>,
    /// Git tree when the turn ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_tree: Option<String>,
    pub files: Vec<TurnFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDeletedEvent {
    pub task_id: Ulid,
    pub project_id: Ulid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationClickedEvent {
    pub session_id: Ulid,
    pub project_id: Ulid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictDetectedEvent {
    pub project_id: Ulid,
    /// Relative to the project root, `/` separators.
    pub path: String,
    /// Every session that edited `path` within the window, sorted.
    pub session_ids: Vec<Ulid>,
    /// The tasks of those sessions (sessions without a task add none), sorted, no duplicates.
    pub task_ids: Vec<Ulid>,
}

/// One reviewable Markdown file of a `doc.list` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocFile {
    /// Relative to the project root, `/` separators.
    pub file_path: String,
    /// Thread roots not yet resolved.
    pub open_comments: u32,
    /// The session the file watcher last saw it change under, if within the recent window.
    pub last_session_id: Option<Ulid>,
    /// Unix ms of that change.
    pub last_changed_at: Option<i64>,
}

/// `doc.read` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocContent {
    pub file_path: String,
    pub content: String,
    /// Opaque; changes whenever the content does (same value as in `doc.changed`). Only
    /// stable within one daemon build: never persist it.
    pub hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocChangedEvent {
    pub project_id: Ulid,
    /// Relative to the project root, `/` separators.
    pub file_path: String,
    /// The new content hash; `None` = the file was deleted.
    pub hash: Option<String>,
}

mod base64_bytes {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sid() -> Ulid {
        Ulid::from_parts(1_700_000_000_000, 7)
    }

    fn round_trip(message: &Message) -> serde_json::Value {
        let value = serde_json::to_value(message).unwrap();
        let back: Message = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(&back, message);
        value
    }

    #[test]
    fn turn_wire_shape_omits_absent_fields() {
        let turn = Turn {
            n: 1,
            started_at: 5,
            ended_at: None,
            start_tree: None,
            end_tree: None,
            files: vec![],
        };
        let value = serde_json::to_value(&turn).unwrap();
        assert_eq!(value, json!({ "n": 1, "started_at": 5, "files": [] }));
        assert_eq!(serde_json::from_value::<Turn>(value).unwrap(), turn);
    }

    #[test]
    fn request_wire_shape_matches_spec() {
        let message = Message::Request {
            id: 7,
            request: Request::SessionResize(SessionResizeParams {
                session_id: sid(),
                cols: 120,
                rows: 40,
            }),
        };
        assert_eq!(
            round_trip(&message),
            json!({
                "type": "request", "id": 7, "method": "session.resize",
                "params": {"session_id": sid().to_string(), "cols": 120, "rows": 40}
            })
        );
    }

    #[test]
    fn param_less_requests_omit_params() {
        let message = Message::Request {
            id: 1,
            request: Request::DaemonStatus,
        };
        assert_eq!(
            round_trip(&message),
            json!({"type": "request", "id": 1, "method": "daemon.status"})
        );
    }

    #[test]
    fn every_m1_request_round_trips() {
        let requests = vec![
            Request::Hello(HelloParams {
                client: ClientKind::Ui,
                protocol_version: crate::PROTOCOL_VERSION,
            }),
            Request::DaemonStatus,
            Request::DaemonStop,
            Request::ProjectAdd(ProjectAddParams {
                path: r"C:\work\demo".into(),
                name: None,
            }),
            Request::ProjectList,
            Request::SessionSpawn(SessionSpawnParams {
                project_id: sid(),
                agent_profile: Some("fake".into()),
                kind: SessionKind::Agent,
                tab_id: Some(sid()),
                title: Some("t".into()),
                cols: 80,
                rows: 24,
            }),
            Request::ProjectUpdate(ProjectUpdateParams {
                project_id: sid(),
                name: Some("demo".into()),
                default_agent: Some(None),
                default_shell: None,
                run_commands: Some(vec![RunCommand {
                    name: "BE".into(),
                    dir: "api".into(),
                    command: "uv run main.py".into(),
                    env_file: Some(".env".into()),
                    env: vec![("ENVIRONMENT".into(), "dev".into())],
                }]),
                run_autostart: Some(true),
                color: Some(Some("teal".into())),
            }),
            Request::ProjectRunStart(RunStartParams {
                project_id: sid(),
                name: Some("BE".into()),
                cols: 100,
                rows: 20,
            }),
            Request::ProjectRunRestart(RunStartParams {
                project_id: sid(),
                name: None,
                cols: 80,
                rows: 24,
            }),
            Request::ProjectRunStop(RunStopParams {
                project_id: sid(),
                name: None,
            }),
            Request::ProjectArchive(ProjectRef { project_id: sid() }),
            Request::TabList(TabListParams { project_id: sid() }),
            Request::TabCreate(TabCreateParams {
                project_id: sid(),
                title: "Agents".into(),
                position: Some(1.5),
            }),
            Request::TabRename(TabRenameParams {
                tab_id: sid(),
                title: "Shells".into(),
            }),
            Request::TabClose(TabRef { tab_id: sid() }),
            Request::TabMove(TabMoveParams {
                tab_id: sid(),
                position: 0.5,
            }),
            Request::TabSetLayout(TabSetLayoutParams {
                tab_id: sid(),
                layout: json!({"split": "vertical", "ratio": 0.5, "children": [
                    {"session": sid().to_string()}, {"session": sid().to_string()}
                ]}),
            }),
            Request::TaskList(TaskListParams {
                project_id: Some(sid()),
                status: Some(TaskStatus::NeedsInput),
            }),
            Request::TaskCreate(TaskCreateParams {
                project_id: sid(),
                title: "Fix login".into(),
                prompt: "make it work".into(),
                agent_profile: Some("claude".into()),
                position: None,
                worktree: true,
            }),
            Request::TaskUpdate(TaskUpdateParams {
                task_id: sid(),
                title: None,
                prompt: Some("".into()),
                agent_profile: Some(None),
                worktree: Some(false),
            }),
            Request::TaskMove(TaskMoveParams {
                task_id: sid(),
                status: TaskStatus::Review,
                position: 2.5,
            }),
            Request::TaskDelete(TaskRef { task_id: sid() }),
            Request::TaskRun(TaskRunParams {
                task_id: sid(),
                cols: 120,
                rows: 40,
            }),
            Request::TaskUndoPreview(TaskRef { task_id: sid() }),
            Request::TaskUndo(TaskUndoParams {
                task_id: sid(),
                confirm: true,
                created: vec!["new.txt".into()],
                fingerprint: "00ff".into(),
            }),
            Request::TaskMerge(TaskConfirmParams {
                task_id: sid(),
                confirm: true,
            }),
            Request::TaskRemoveWorktree(TaskConfirmParams {
                task_id: sid(),
                confirm: false,
            }),
            Request::TaskFanout(TaskFanoutParams {
                task_id: sid(),
                agent_profiles: vec!["claude".into(), "codex".into()],
                cols: 120,
                rows: 30,
            }),
            Request::SessionList(SessionListParams::default()),
            Request::SessionAttach(SessionRef { session_id: sid() }),
            Request::SessionDetach(SessionRef { session_id: sid() }),
            Request::SessionHistory(SessionRef { session_id: sid() }),
            Request::SessionRead(SessionReadParams {
                session_id: sid(),
                lines: 8,
            }),
            Request::SessionInput(SessionInputParams {
                session_id: sid(),
                data: vec![0x1b, b'[', b'A', 0, 0xff],
            }),
            Request::SessionSend(SessionSendParams {
                session_id: sid(),
                text: "y".into(),
                submit: true,
            }),
            Request::SessionUnqueue(SessionUnqueueParams {
                session_id: sid(),
                send_id: sid(),
            }),
            Request::AttachmentSave(AttachmentSaveParams {
                session_id: sid(),
                ext: "png".into(),
                data: vec![0x89, b'P', b'N', b'G'],
            }),
            Request::TurnList(SessionRef { session_id: sid() }),
            Request::SessionFiles(SessionFilesParams {
                project_id: sid(),
                since_ms: 5,
            }),
            Request::SessionUsage(ProjectRef { project_id: sid() }),
            Request::SessionPorts(ProjectRef { project_id: sid() }),
            Request::TurnRestorePreview(TurnRef {
                session_id: sid(),
                n: 2,
            }),
            Request::TurnRestore(TurnRestoreParams {
                session_id: sid(),
                n: 2,
                confirm: true,
                created: vec!["new.txt".into()],
                fingerprint: "00ff".into(),
            }),
            Request::SessionWait(SessionWaitParams {
                session_id: sid(),
                until: vec![SessionState::Idle, SessionState::Done],
                timeout_ms: Some(5_000),
            }),
            Request::SessionKill(SessionKillParams {
                session_id: sid(),
                force: true,
            }),
            Request::SessionRemove(SessionRef { session_id: sid() }),
            Request::SessionRename(SessionRenameParams {
                session_id: sid(),
                title: Some("Fix login".into()),
            }),
            Request::HookReport(HookReportParams {
                mkrew_session_id: sid(),
                agent: "claude".into(),
                event: "Notification".into(),
                payload: json!({"session_id": "abc"}),
            }),
            Request::AgentProfiles,
            Request::DocList(DocListParams { project_id: sid() }),
            Request::DocRead(DocReadParams {
                project_id: sid(),
                file_path: "docs/plan.md".into(),
            }),
            Request::CommentList(CommentListParams {
                project_id: sid(),
                file_path: Some("docs/plan.md".into()),
                context: Some(CommentContext::Diff),
            }),
            Request::CommentCreate(CommentCreateParams {
                project_id: sid(),
                file_path: "docs/plan.md".into(),
                start_line: 12,
                end_line: 14,
                quote: "Use Redis for caching".into(),
                body: "Why not in-memory?".into(),
                context: CommentContext::Doc,
            }),
            Request::CommentReply(CommentReplyParams {
                comment_id: CommentId(sid()),
                body: "Switched to a HashMap".into(),
                mkrew_session_id: Some(sid()),
            }),
            Request::CommentResolve(CommentRef {
                comment_id: CommentId(sid()),
            }),
            Request::CommentReopen(CommentRef {
                comment_id: CommentId(sid()),
            }),
            Request::CommentSendToAgent(CommentSendParams {
                comment_ids: vec![CommentId(sid())],
                session_id: sid(),
            }),
        ];
        for (id, request) in requests.into_iter().enumerate() {
            round_trip(&Message::Request {
                id: id as u64,
                request,
            });
        }
    }

    #[test]
    fn run_start_size_defaults_when_left_out() {
        let id = sid();
        let request: Request = serde_json::from_value(json!({
            "method": "project.run_start",
            "params": {"project_id": id.to_string()}
        }))
        .unwrap();
        assert_eq!(
            request,
            Request::ProjectRunStart(RunStartParams {
                project_id: id,
                name: None,
                cols: DEFAULT_COLS,
                rows: DEFAULT_ROWS,
            })
        );
    }

    #[test]
    fn input_data_is_base64() {
        let request = Request::SessionInput(SessionInputParams {
            session_id: sid(),
            data: b"hi\r".to_vec(),
        });
        let value = serde_json::to_value(&request).unwrap();
        assert_eq!(value["params"]["data"], "aGkN");
    }

    #[test]
    fn optional_params_default_when_missing() {
        let message: Message = serde_json::from_value(json!({
            "type": "request", "id": 3, "method": "session.spawn",
            "params": {"project_id": sid().to_string(), "agent_profile": "shell", "cols": 80, "rows": 24}
        }))
        .unwrap();
        let Message::Request {
            request: Request::SessionSpawn(params),
            ..
        } = message
        else {
            panic!("wrong variant: {message:?}");
        };
        assert_eq!(params.kind, SessionKind::Agent);
        assert_eq!(params.title, None);
        assert_eq!(params.agent_profile.as_deref(), Some("shell"));
    }

    #[test]
    fn session_read_lines_default_to_20() {
        let request: Request = serde_json::from_value(json!({
            "method": "session.read", "params": {"session_id": sid().to_string()}
        }))
        .unwrap();
        assert_eq!(
            request,
            Request::SessionRead(SessionReadParams {
                session_id: sid(),
                lines: 20
            })
        );
    }

    #[test]
    fn project_update_tells_missing_from_null() {
        let parse = |params: serde_json::Value| -> ProjectUpdateParams {
            serde_json::from_value(params).unwrap()
        };
        let id = sid().to_string();
        assert_eq!(parse(json!({"project_id": id})).default_agent, None);
        assert_eq!(
            parse(json!({"project_id": id, "default_agent": null})).default_agent,
            Some(None)
        );
        assert_eq!(
            parse(json!({"project_id": id, "default_agent": "claude"})).default_agent,
            Some(Some("claude".into()))
        );
        assert_eq!(parse(json!({"project_id": id})).default_shell, None);
        assert_eq!(
            parse(json!({"project_id": id, "default_shell": null})).default_shell,
            Some(None)
        );
        assert_eq!(
            parse(json!({"project_id": id, "default_shell": "pwsh"})).default_shell,
            Some(Some("pwsh".into()))
        );
        let autostart = parse(json!({"project_id": id, "run_autostart": true}));
        assert_eq!(autostart.run_autostart, Some(true));
        assert_eq!(autostart.run_commands, None);
        assert_eq!(
            parse(json!({"project_id": id, "run_commands": []})).run_commands,
            Some(Vec::new())
        );
        assert_eq!(parse(json!({"project_id": id})).color, None);
        assert_eq!(
            parse(json!({"project_id": id, "color": null})).color,
            Some(None)
        );
        assert_eq!(
            parse(json!({"project_id": id, "color": "teal"})).color,
            Some(Some("teal".into()))
        );
        // Unchanged fields are left out on the wire, a cleared one is `null`.
        let keep = ProjectUpdateParams {
            project_id: sid(),
            name: None,
            default_agent: None,
            default_shell: None,
            run_commands: None,
            run_autostart: None,
            color: None,
        };
        assert_eq!(
            serde_json::to_value(&keep).unwrap(),
            json!({"project_id": id})
        );
        let clear = ProjectUpdateParams {
            default_agent: Some(None),
            ..keep
        };
        assert_eq!(
            serde_json::to_value(&clear).unwrap(),
            json!({"project_id": id, "default_agent": null})
        );
    }

    #[test]
    fn hook_report_payload_defaults_to_null() {
        let request: Request = serde_json::from_value(json!({
            "method": "hook.report",
            "params": {"mkrew_session_id": sid().to_string(), "agent": "fake", "event": "done"}
        }))
        .unwrap();
        let Request::HookReport(params) = request else {
            panic!("wrong variant: {request:?}");
        };
        assert_eq!(params.payload, serde_json::Value::Null);
    }

    #[test]
    fn unknown_method_is_rejected() {
        let result = serde_json::from_value::<Message>(
            json!({"type": "request", "id": 1, "method": "nope.nope"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn response_result_and_error_shapes() {
        let ok = Message::Response {
            id: 7,
            outcome: Outcome::ok(&SessionSpawnResult { session_id: sid() }).unwrap(),
        };
        assert_eq!(
            round_trip(&ok),
            json!({"type": "response", "id": 7, "result": {"session_id": sid().to_string()}})
        );

        let unit = Message::Response {
            id: 9,
            outcome: Outcome::ok(&()).unwrap(),
        };
        assert_eq!(
            round_trip(&unit),
            json!({"type": "response", "id": 9, "result": null})
        );

        let err = Message::Response {
            id: 8,
            outcome: Outcome::error(ErrorCode::VersionMismatch, "daemon speaks 1, client 2"),
        };
        assert_eq!(
            round_trip(&err),
            json!({
                "type": "response", "id": 8,
                "error": {"code": "version_mismatch", "message": "daemon speaks 1, client 2"}
            })
        );
    }

    #[test]
    fn into_result_decodes_or_surfaces_daemon_error() {
        let hello = HelloResult {
            daemon_version: "0.1.0".into(),
            protocol_version: 1,
        };
        let decoded: HelloResult = Outcome::ok(&hello).unwrap().into_result().unwrap();
        assert_eq!(decoded, hello);

        let err = Outcome::error(ErrorCode::NotFound, "gone")
            .into_result::<HelloResult>()
            .unwrap_err();
        assert!(matches!(
            err,
            ResponseError::Daemon(ErrorBody {
                code: ErrorCode::NotFound,
                ..
            })
        ));
    }

    #[test]
    fn agent_profiles_result_round_trips() {
        let result = AgentProfilesResult {
            profiles: vec![AgentProfileInfo {
                id: "fake".into(),
                name: "Fake agent".into(),
                kind: SessionKind::Agent,
                command: "mkrew-fakeagent".into(),
                icon: Some("ƒ".into()),
                installed: true,
            }],
            errors: vec![ProfileErrorInfo {
                file: "agents/broken.toml".into(),
                line: Some(2),
                column: Some(1),
                message: "unknown field `promt_mode`".into(),
            }],
        };
        let decoded: AgentProfilesResult = Outcome::ok(&result).unwrap().into_result().unwrap();
        assert_eq!(decoded, result);
    }

    #[test]
    fn agent_profile_info_without_icon_decodes() {
        let info: AgentProfileInfo = serde_json::from_value(serde_json::json!({
            "id": "x", "name": "X", "kind": "agent", "command": "x"
        }))
        .unwrap();
        assert_eq!(info.icon, None);
    }

    #[test]
    fn snapshot_wire_shape() {
        let snapshot = ScreenSnapshot {
            cols: 4,
            rows: 2,
            lines: vec![
                vec![CellRun {
                    text: "ok".into(),
                    fg: TermColor::Indexed(1),
                    bg: TermColor::Rgb([1, 2, 3]),
                    flags: CellRun::BOLD,
                }],
                vec![],
            ],
            history: vec![vec![CellRun {
                text: "old".into(),
                fg: TermColor::Default,
                bg: TermColor::Default,
                flags: 0,
            }]],
            cursor: CursorSnapshot {
                row: 1,
                col: 0,
                visible: true,
            },
            modes: TermModes {
                bracketed_paste: true,
                ..TermModes::default()
            },
        };
        let value = round_trip(&Message::Event(Event::SessionSnapshot(
            SessionSnapshotEvent {
                session_id: sid(),
                snapshot: snapshot.clone(),
            },
        )));
        assert_eq!(value["event"], "session.snapshot");
        assert_eq!(
            value["data"]["snapshot"]["lines"][0][0],
            json!({"text": "ok", "fg": {"indexed": 1}, "bg": {"rgb": [1, 2, 3]}, "flags": 1})
        );
        assert_eq!(
            value["data"]["snapshot"]["history"][0][0],
            json!({"text": "old", "fg": "default", "bg": "default", "flags": 0})
        );
        let attach = SessionAttachResult {
            state: SessionState::Idle,
            snapshot,
        };
        let decoded: SessionAttachResult = Outcome::ok(&attach).unwrap().into_result().unwrap();
        assert_eq!(decoded, attach);
        assert_eq!(
            serde_json::to_value(TermColor::Default).unwrap(),
            json!("default")
        );
    }

    #[test]
    fn event_wire_shape_matches_spec() {
        let message = Message::Event(Event::SessionState(SessionStateEvent {
            session_id: sid(),
            state: SessionState::NeedsInput,
            source: StateSource::Hook,
            changed_at: 5,
        }));
        assert_eq!(
            round_trip(&message),
            json!({
                "type": "event", "event": "session.state",
                "data": {
                    "session_id": sid().to_string(), "state": "needs_input",
                    "source": "hook", "changed_at": 5
                }
            })
        );
        round_trip(&Message::Event(Event::SessionExited(SessionExitedEvent {
            session_id: sid(),
            exit_code: Some(0),
        })));
        round_trip(&Message::Event(Event::ProjectUpdated(Project {
            id: sid(),
            name: "demo".into(),
            path: "/tmp/demo".into(),
            run_commands: vec![RunCommand {
                name: "FE".into(),
                dir: ".".into(),
                command: "npm run dev".into(),
                env_file: None,
                env: Vec::new(),
            }],
            run_autostart: true,
            default_agent: Some("claude".into()),
            default_shell: Some("pwsh".into()),
            review_globs: Some(vec!["docs/**/*.md".into()]),
            created_at: 1,
            archived: false,
            base_ref: Some("develop".into()),
            color: Some("teal".into()),
        })));
        let tab = round_trip(&Message::Event(Event::TabUpdated(TerminalTab {
            id: sid(),
            project_id: sid(),
            title: "Agents".into(),
            position: 1.0,
            layout: serde_json::Value::Null,
        })));
        assert_eq!(tab["event"], "tab.updated");
        assert_eq!(tab["data"]["layout"], serde_json::Value::Null);
        let clicked = round_trip(&Message::Event(Event::NotificationClicked(
            NotificationClickedEvent {
                session_id: sid(),
                project_id: sid(),
            },
        )));
        assert_eq!(clicked["event"], "notification.clicked");
        assert_eq!(clicked["data"]["session_id"], sid().to_string());
        let conflict = round_trip(&Message::Event(Event::ConflictDetected(
            ConflictDetectedEvent {
                project_id: sid(),
                path: "src/a.rs".into(),
                session_ids: vec![sid(), sid()],
                task_ids: vec![sid()],
            },
        )));
        assert_eq!(conflict["event"], "conflict.detected");
        assert_eq!(conflict["data"]["path"], "src/a.rs");
        assert_eq!(
            conflict["data"]["session_ids"].as_array().map(Vec::len),
            Some(2)
        );
        let task = round_trip(&Message::Event(Event::TaskUpdated(Task {
            id: sid(),
            project_id: sid(),
            title: "Fix login".into(),
            prompt: String::new(),
            agent_profile: None,
            status: TaskStatus::NeedsInput,
            position: 1.5,
            checkpoint: None,
            error_flag: true,
            worktree: true,
            worktree_path: Some("/data/mkrew/worktrees/p/t".into()),
            branch: Some("mkrew/fix-login-01jx".into()),
            fanout_of: Some(sid()),
            created_at: 1,
            updated_at: 2,
        })));
        assert_eq!(task["event"], "task.updated");
        assert_eq!(task["data"]["worktree"], true);
        assert_eq!(task["data"]["branch"], "mkrew/fix-login-01jx");
        assert_eq!(task["data"]["status"], "needs_input");
        assert_eq!(task["data"]["agent_profile"], serde_json::Value::Null);
        let deleted = round_trip(&Message::Event(Event::TaskDeleted(TaskDeletedEvent {
            task_id: sid(),
            project_id: sid(),
        })));
        assert_eq!(deleted["event"], "task.deleted");
        let doc = round_trip(&Message::Event(Event::DocChanged(DocChangedEvent {
            project_id: sid(),
            file_path: "docs/plan.md".into(),
            hash: None,
        })));
        assert_eq!(doc["event"], "doc.changed");
        assert_eq!(doc["data"]["hash"], serde_json::Value::Null);
        let turn = round_trip(&Message::Event(Event::SessionTurn(SessionTurnEvent {
            session_id: sid(),
            n: 2,
            started_at: 10,
            ended_at: 20,
            files: vec![TurnFile {
                path: "src/a.rs".into(),
                added: None,
                removed: None,
            }],
        })));
        assert_eq!(turn["event"], "session.turn");
        assert_eq!(turn["data"]["files"], json!([{ "path": "src/a.rs" }]));
        let queue = round_trip(&Message::Event(Event::SessionQueue(SessionQueueEvent {
            session_id: sid(),
            items: vec![QueuedSend {
                send_id: sid(),
                text: "fix it".into(),
            }],
        })));
        assert_eq!(queue["event"], "session.queue");
        assert_eq!(queue["data"]["items"][0]["text"], "fix it");
        let edited = round_trip(&Message::Event(Event::SessionEdited(SessionEditedEvent {
            session_id: sid(),
            path: "src/a.rs".into(),
            line: None,
        })));
        assert_eq!(edited["event"], "session.edited");
        assert_eq!(
            edited["data"],
            json!({ "session_id": sid(), "path": "src/a.rs" })
        );
        round_trip(&Message::Event(Event::SessionPorts(SessionPortsEvent {
            session_id: sid(),
            ports: vec![8080, 5180],
        })));
        let updated = round_trip(&Message::Event(Event::SessionUpdated(Session {
            id: sid(),
            task_id: None,
            project_id: sid(),
            agent_profile: "claude".into(),
            kind: SessionKind::Agent,
            title: Some("Fix login".into()),
            terminal_title: None,
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Screen,
            state_changed_at: 1,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 1,
            ended_at: None,
        })));
        let titled = round_trip(&Message::Event(Event::SessionTerminalTitle(
            SessionTerminalTitleEvent {
                session_id: sid(),
                title: Some("Fix login".into()),
            },
        )));
        assert_eq!(titled["event"], "session.terminal_title");
        assert_eq!(updated["event"], "session.updated");
        assert_eq!(updated["data"]["title"], "Fix login");
        // Absent on the wire when unset.
        assert!(updated["data"].get("terminal_title").is_none());
        let usage = round_trip(&Message::Event(Event::SessionUsage(SessionUsageEvent {
            session_id: sid(),
            input: 10,
            output: 2,
            context: 7,
            context_window: None,
        })));
        assert_eq!(usage["event"], "session.usage");
        assert_eq!(
            usage["data"],
            json!({ "session_id": sid(), "input": 10, "output": 2, "context": 7 })
        );
    }

    #[test]
    fn comment_ids_carry_the_c_prefix() {
        let id = CommentId(sid());
        assert_eq!(id.to_string(), format!("c_{}", sid()));
        assert_eq!(serde_json::to_value(id).unwrap(), json!(id.to_string()));
        // Agents may drop the prefix; both forms parse to the same id.
        assert_eq!(id.to_string().parse::<CommentId>().unwrap(), id);
        assert_eq!(sid().to_string().parse::<CommentId>().unwrap(), id);
        assert!("c_nope".parse::<CommentId>().is_err());
        assert!(serde_json::from_value::<CommentId>(json!("x")).is_err());
    }

    #[test]
    fn comment_wire_shapes() {
        let comment = DocComment {
            id: CommentId(sid()),
            project_id: sid(),
            file_path: "docs/plan.md".into(),
            context: CommentContext::Diff,
            parent_id: None,
            author: DocComment::USER.into(),
            body: "Add a rollback step.".into(),
            start_line: Some(40),
            end_line: Some(40),
            quote: Some("M3: deploy".into()),
            status: CommentStatus::Addressed,
            sent_to: Some(sid()),
            sent_snapshot: Some("# Plan\n".into()),
            created_at: 1,
            updated_at: 2,
        };
        let value = round_trip(&Message::Event(Event::CommentUpdated(Box::new(
            comment.clone(),
        ))));
        assert_eq!(value["event"], "comment.updated");
        assert_eq!(value["data"]["id"], format!("c_{}", sid()));
        assert_eq!(value["data"]["status"], "addressed");
        assert_eq!(value["data"]["parent_id"], serde_json::Value::Null);
        let decoded: Vec<DocComment> = Outcome::ok(&vec![comment.clone()])
            .unwrap()
            .into_result()
            .unwrap();
        assert_eq!(decoded, vec![comment.clone()]);

        let id = sid().to_string();
        let list: CommentListParams = serde_json::from_value(json!({"project_id": id})).unwrap();
        assert_eq!(list.file_path, None);
        let reply: CommentReplyParams =
            serde_json::from_value(json!({"comment_id": format!("c_{id}"), "body": "ok"})).unwrap();
        assert_eq!(reply.mkrew_session_id, None);
        assert_eq!(reply.comment_id, CommentId(sid()));

        // `context` defaults to `doc` everywhere, so pre-M7 clients keep working.
        assert_eq!(list.context, None);
        assert_eq!(value["data"]["context"], "diff");
        let create: CommentCreateParams = serde_json::from_value(json!({
            "project_id": id, "file_path": "a.md", "start_line": 1, "end_line": 1,
            "quote": "", "body": "x",
        }))
        .unwrap();
        assert_eq!(create.context, CommentContext::Doc);
        let mut old = serde_json::to_value(&comment).unwrap();
        old.as_object_mut().unwrap().remove("context");
        let old: DocComment = serde_json::from_value(old).unwrap();
        assert_eq!(old.context, CommentContext::Doc);
    }

    #[test]
    fn task_params_default_and_tell_missing_from_null() {
        let id = sid().to_string();
        let create: TaskCreateParams =
            serde_json::from_value(json!({"project_id": id, "title": "t"})).unwrap();
        assert_eq!(create.prompt, "");
        assert_eq!(create.agent_profile, None);
        assert_eq!(create.position, None);
        assert!(!create.worktree);
        let list: TaskListParams = serde_json::from_value(json!({})).unwrap();
        assert_eq!(list, TaskListParams::default());

        let parse = |params: serde_json::Value| -> TaskUpdateParams {
            serde_json::from_value(params).unwrap()
        };
        assert_eq!(parse(json!({"task_id": id})).agent_profile, None);
        assert_eq!(
            parse(json!({"task_id": id, "agent_profile": null})).agent_profile,
            Some(None)
        );
        let keep = parse(json!({"task_id": id}));
        assert_eq!(keep.worktree, None);
        assert_eq!(serde_json::to_value(&keep).unwrap(), json!({"task_id": id}));
        assert_eq!(
            parse(json!({"task_id": id, "worktree": true})).worktree,
            Some(true)
        );

        // Undo is refused unless the client says otherwise.
        let undo: TaskUndoParams = serde_json::from_value(json!({"task_id": id})).unwrap();
        assert!(!undo.confirm);
        assert!(undo.created.is_empty() && undo.fingerprint.is_empty());
        let merge: TaskConfirmParams = serde_json::from_value(json!({"task_id": id})).unwrap();
        assert!(!merge.confirm);
    }
}
