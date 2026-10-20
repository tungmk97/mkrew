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
    GitRemoteParams, GitSetBaseParams, GitSetViewedParams, GitShowParams, GitStageParams,
    GitStatusParams, ProjectRef,
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
    /// Returns the [`TerminalTab`]; also broadcast as `tab.updated`.
    #[serde(rename = "tab.pin")]
    TabPin(TabPinParams),
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
    /// Returns the [`Session`]; also broadcast as `session.updated`.
    #[serde(rename = "session.pin")]
    SessionPin(SessionPinParams),
    /// Returns the [`Session`] with the new `position`; also broadcast as `session.updated`.
    /// A non-finite position is `invalid_request`.
    #[serde(rename = "session.move")]
    SessionMove(SessionMoveParams),
    /// Returns the [`Session`] with the new color, also announced as `session.updated`; `null`
    /// clears it, a name outside [`WORKSPACE_COLORS`] is `invalid_request` (ADR 0037).
    #[serde(rename = "session.set_color")]
    SessionSetColor(SessionSetColorParams),
    /// Returns [`SessionHistory`]; works for exited sessions too.
    #[serde(rename = "session.history")]
    SessionHistory(SessionRef),
    /// Returns [`SessionSpawnResult`]: relaunches an exited agent session into its conversation
    /// with the profile's `resume` command, as a new session (ADR 0055).
    #[serde(rename = "session.resume")]
    SessionResume(SessionResumeParams),
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
    /// Returns `ViewedFile[]`: the files of the listed changes whose viewed mark still stands;
    /// marks that no longer match are deleted (ADR 0065). Same params as `git.status`.
    #[serde(rename = "git.viewed")]
    GitViewed(GitStatusParams),
    /// Returns nothing (`null`): marks the file as viewed (or clears the mark) for the diff the
    /// params describe (ADR 0065).
    #[serde(rename = "git.set_viewed")]
    GitSetViewed(GitSetViewedParams),
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
    /// Returns the [`Session`] (also announced as `session.updated`): links the Markdown file
    /// `path` as the plan of a live session, or unlinks it (`null`). The daemon announces
    /// `session.plan` at once and after each change of the file. `invalid_request` for a path
    /// that is not a readable `.md` file (ADR 0039).
    #[serde(rename = "session.link_plan")]
    SessionLinkPlan(SessionLinkPlanParams),
    /// Returns `SessionPlanEvent[]`: the checklist of each live session of the project that
    /// has a plan (for a client that connects later).
    #[serde(rename = "session.plan")]
    SessionPlan(ProjectRef),
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
    /// A session's title, color, pin or plan link changed (`session.rename`,
    /// `session.set_color`, `session.pin`, `session.link_plan`; ADR 0031, ADR 0037, ADR 0039);
    /// carries the whole session.
    #[serde(rename = "session.updated")]
    SessionUpdated(Box<Session>),
    /// An agent session's terminal title changed, cleaned (`None` = cleared; ADR 0031).
    #[serde(rename = "session.terminal_title")]
    SessionTerminalTitle(SessionTerminalTitleEvent),
    /// The checklist of a linked plan changed (ADR 0039).
    #[serde(rename = "session.plan")]
    SessionPlan(SessionPlanEvent),
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

/// Workspace color names (SPEC §14, ADR 0020); clients map them to theme tokens. The order is
/// the palette order of `Theme.workspace_colors`; names are only ever added, because they are
/// stored in `project.color`.
pub const WORKSPACE_COLORS: [&str; 9] = [
    "red", "orange", "yellow", "lime", "green", "teal", "blue", "purple", "pink",
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
    /// Pinned workspaces lead the sidebar (SPEC §10).
    #[serde(default)]
    pub pinned: bool,
    /// Jira project keys linked to this workspace (SPEC §20I); empty = none.
    #[serde(default)]
    pub jira_keys: Vec<String>,
    /// Sidebar order inside its pinned or unpinned group, ascending; 0 = never moved (then by
    /// age). Set with `project.update` `position` (a drag in the sidebar).
    #[serde(default)]
    pub position: i64,
}

/// One command of a project's Run button (SPEC §10, ADR 0018): typed into its own shell in
/// `dir`, with `env_file` (relative to the project folder, not `dir`) then `env` added to the
/// environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCommand {
    pub name: String,
    /// Relative to the project folder; `.` = the folder itself.
    pub dir: String,
    pub command: String,
    /// Relative to the project folder (not `dir`), e.g. `EA-BE/.env`.
    #[serde(default)]
    pub env_file: Option<String>,
    /// Applied after `env_file`.
    #[serde(default)]
    pub env: Vec<(String, String)>,
    /// Type an exit after the command, so the pane closes when it ends (off: the shell stays).
    #[serde(default)]
    pub exit_after: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// One of [`WORKSPACE_COLORS`] (ADR 0037): the pane's color; `None` = no color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
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
    /// Pinned sessions lead their workspace's sidebar list (SPEC §10). Set with `session.pin`.
    #[serde(default)]
    pub pinned: bool,
    /// Sidebar order inside its workspace group, ascending; 0 = never moved (newest first among equals). Set with `session.move`.
    #[serde(default)]
    pub position: f64,
    /// The linked plan `.md` (`session.link_plan`): relative to the project folder with `/`
    /// when it lies inside, else absolute. `None` = no plan (SPEC §5, ADR 0039).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_path: Option<String>,
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
    /// Pinned tabs sit left of the others, each group in `position` order (SPEC §10). Set with
    /// `tab.pin`, which leaves `position` alone, so unpinning puts the tab back.
    #[serde(default)]
    pub pinned: bool,
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
    /// Pin or unpin the workspace.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    /// Replaces the Jira project keys; `[]` clears them. Normalised by the daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jira_keys: Option<Vec<String>>,
    /// New sidebar order (a drag in the sidebar; the UI numbers its group `1..=n`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
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

/// `tab.pin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabPinParams {
    pub tab_id: Ulid,
    pub pinned: bool,
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

/// `session.resume`: the exited agent session whose conversation goes on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionResumeParams {
    pub session_id: Ulid,
    /// The tab the pane goes into when that tab has no panes (like `session.spawn`).
    #[serde(default)]
    pub tab_id: Option<Ulid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRenameParams {
    pub session_id: Ulid,
    /// Trimmed by the daemon; `None` or blank clears the title.
    #[serde(default)]
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionLinkPlanParams {
    pub session_id: Ulid,
    /// A path picked by the user (absolute) or relative to the project folder; `None` unlinks.
    #[serde(default)]
    pub path: Option<String>,
}

/// `session.pin`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPinParams {
    pub session_id: Ulid,
    pub pinned: bool,
}

/// `session.move`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionMoveParams {
    pub session_id: Ulid,
    pub position: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSetColorParams {
    pub session_id: Ulid,
    /// A [`WORKSPACE_COLORS`] name; `None` clears the color.
    #[serde(default)]
    pub color: Option<String>,
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

/// Phone access (ADR 0053): `[web] enabled` and whether the daemon's `mk web` child runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebStatus {
    /// `[web] enabled` in `settings.toml`.
    pub enabled: bool,
    /// The `mk web` child is alive.
    pub running: bool,
    /// Why it is not running while enabled (the last line `mk web` printed to stderr, or that
    /// `mk` was not found); cleared while it runs and when disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
    /// Phone access state (ADR 0053); absent from a daemon that predates it.
    #[serde(default)]
    pub web: WebStatus,
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
    /// It can answer `ai.generate` `ask` (an `ask` or `oneshot` command, ADR 0061).
    #[serde(default)]
    pub ask: bool,
    /// It has an `ask_web` command (`ask` with `web`).
    #[serde(default)]
    pub ask_web: bool,
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

/// Token totals of an agent session, read from its transcript (SPEC §20E.3, ADR 0035).
/// `input + cache_read + cache_write + output` is every token of the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsageEvent {
    pub session_id: Ulid,
    /// Fresh input tokens over the agent's whole conversation, cache excluded. An event stored
    /// before protocol 33 still has the cache in here (and 0 in both cache fields).
    pub input: u64,
    /// Output tokens over the agent's whole conversation.
    pub output: u64,
    /// Input tokens served from the prompt cache.
    #[serde(default)]
    pub cache_read: u64,
    /// Input tokens written to the prompt cache (0 for agents without that figure).
    #[serde(default)]
    pub cache_write: u64,
    /// Context size of the newest message.
    pub context: u64,
    /// The profile's `context_window`, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    /// The model of the newest message, when the transcript names one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The agent's transcript file (an absolute path on the daemon's machine, which is the
    /// UI's: the socket is local, SPEC §3), once located. Sent again when it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript: Option<String>,
}

/// A run session's ports (SPEC §10): every port it printed so far, at most 4, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPortsEvent {
    pub session_id: Ulid,
    pub ports: Vec<u16>,
}

/// One `- [ ]` / `- [x]` line of a plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanItem {
    pub done: bool,
    pub text: String,
}

/// The checklist of a session's linked plan (ADR 0039).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPlanEvent {
    pub session_id: Ulid,
    /// The file name without `.md`.
    pub name: String,
    pub done: u32,
    pub total: u32,
    /// The first items in file order (at most 200); `total` counts all of them.
    pub items: Vec<PlanItem>,
    /// Why the file could not be read (then the counts are 0); the link stays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
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
mod tests;
