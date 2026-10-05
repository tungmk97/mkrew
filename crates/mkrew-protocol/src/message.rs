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

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

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
    #[serde(rename = "session.wait")]
    SessionWait(SessionWaitParams),
    #[serde(rename = "session.resize")]
    SessionResize(SessionResizeParams),
    #[serde(rename = "session.kill")]
    SessionKill(SessionKillParams),
    #[serde(rename = "agent.profiles")]
    AgentProfiles,
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
    /// Sent only to an attached client whose output queue overflowed: its queued output was
    /// dropped and this screen replaces it; output frames continue from this point (SPEC §7).
    #[serde(rename = "session.snapshot")]
    SessionSnapshot(SessionSnapshotEvent),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    #[default]
    Agent,
    Shell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: Ulid,
    pub name: String,
    pub path: String,
    pub dev_command: Option<String>,
    pub default_agent: Option<String>,
    /// `None` = default globs (SPEC §12).
    pub review_globs: Option<Vec<String>>,
    /// Unix ms.
    pub created_at: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: Ulid,
    pub task_id: Option<Ulid>,
    pub project_id: Ulid,
    pub agent_profile: String,
    pub kind: SessionKind,
    pub title: Option<String>,
    /// The agent's own session id, used for resume (SPEC §6).
    pub native_session_id: Option<String>,
    pub state: SessionState,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSpawnParams {
    pub project_id: Ulid,
    /// `None` = the default for `kind`: the project's `default_agent` for an agent, the
    /// `shell` profile for a shell. A named profile decides the session kind itself.
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

/// The visible screen of a session's headless emulator (SPEC §7, §8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenSnapshot {
    pub cols: u16,
    pub rows: u16,
    /// One entry per visible row, top to bottom. Trailing default blanks are omitted.
    pub lines: Vec<Vec<CellRun>>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionExitedEvent {
    pub session_id: Ulid,
    pub exit_code: Option<i32>,
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
            }),
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
            Request::SessionList(SessionListParams::default()),
            Request::SessionAttach(SessionRef { session_id: sid() }),
            Request::SessionDetach(SessionRef { session_id: sid() }),
            Request::SessionInput(SessionInputParams {
                session_id: sid(),
                data: vec![0x1b, b'[', b'A', 0, 0xff],
            }),
            Request::SessionSend(SessionSendParams {
                session_id: sid(),
                text: "y".into(),
                submit: true,
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
            Request::AgentProfiles,
        ];
        for (id, request) in requests.into_iter().enumerate() {
            round_trip(&Message::Request {
                id: id as u64,
                request,
            });
        }
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
        // Unchanged fields are left out on the wire, a cleared one is `null`.
        let keep = ProjectUpdateParams {
            project_id: sid(),
            name: None,
            default_agent: None,
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
        }));
        assert_eq!(
            round_trip(&message),
            json!({
                "type": "event", "event": "session.state",
                "data": {"session_id": sid().to_string(), "state": "needs_input"}
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
            dev_command: None,
            default_agent: Some("claude".into()),
            review_globs: Some(vec!["docs/**/*.md".into()]),
            created_at: 1,
            archived: false,
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
    }
}
