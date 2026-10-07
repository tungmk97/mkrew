//! What an agent hook means for the session (SPEC §6): `hook.report` → [`HookEvent`] and the
//! agent's own session id.
//!
//! Payload formats (checked against the official docs, 2026-10):
//! - Claude Code passes JSON on stdin with `session_id` and `hook_event_name`; `Notification`
//!   adds `notification_type` (`permission_prompt`, `idle_prompt`, …).
//! - Codex `notify` passes JSON as the last argument: `type` (`agent-turn-complete`),
//!   `thread-id`, `turn-id`, `cwd`, ….

use serde_json::Value;

use crate::state::HookEvent;

/// The agents `hook.report` understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookAgent {
    Claude,
    Codex,
    /// The test fake agent (SPEC §16).
    Fake,
}

impl HookAgent {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "fake" => Some(Self::Fake),
            _ => None,
        }
    }

    /// The state change `event` asks for; `None` = record it, leave the state alone
    /// (e.g. `SessionStart`, or an event this build does not know).
    pub fn event(self, event: &str, payload: &Value) -> Option<HookEvent> {
        match (self, event) {
            (Self::Claude, "UserPromptSubmit") => Some(HookEvent::Busy),
            (Self::Claude, "Notification") => claude_notification(payload),
            (Self::Claude, "Stop") => Some(HookEvent::Done),
            (Self::Codex, "agent-turn-complete") => Some(HookEvent::Done),
            (Self::Fake, "busy") => Some(HookEvent::Busy),
            (Self::Fake, "needs_input") => Some(HookEvent::NeedsInput),
            (Self::Fake, "done") => Some(HookEvent::Done),
            _ => None,
        }
    }

    /// The agent's own session id in `payload`, used for resume (SPEC §5 `native_session_id`).
    pub fn native_session_id(self, payload: &Value) -> Option<String> {
        let key = match self {
            Self::Claude | Self::Fake => "session_id",
            Self::Codex => "thread-id",
        };
        payload
            .get(key)
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
    }
}

/// Files the hook says the agent edited, as reported (absolute or relative to the agent's cwd).
/// Claude's `PreToolUse`/`PostToolUse` payloads carry `tool_input.file_path` (Edit, Write,
/// MultiEdit) or `tool_input.notebook_path` (NotebookEdit); Codex `notify` payloads name no
/// files. Read for any agent, so no per-agent code is needed (SPEC §2). A `tool_name` other
/// than an editing tool (e.g. `Read`) names no edit; no `tool_name` at all is trusted.
pub fn edited_paths(payload: &Value) -> Vec<String> {
    const EDITING_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];
    let Some(input) = payload.get("tool_input") else {
        return Vec::new();
    };
    if let Some(tool) = payload.get("tool_name").and_then(Value::as_str)
        && !EDITING_TOOLS.contains(&tool)
    {
        return Vec::new();
    }
    ["file_path", "notebook_path"]
        .iter()
        .filter_map(|key| input.get(key).and_then(Value::as_str))
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The 1-based line of the first edit the hook reports, when it names one: Claude's
/// `PostToolUse` `tool_response.structuredPatch` (undocumented; HUMAN_CHECKS T11.5) — the
/// first hunk's `newStart` plus its leading context lines. Best effort: `None` otherwise.
pub fn edited_line(payload: &Value) -> Option<u32> {
    let hunk = payload
        .get("tool_response")?
        .get("structuredPatch")?
        .get(0)?;
    let start = u32::try_from(hunk.get("newStart")?.as_u64()?).ok()?;
    let context = hunk
        .get("lines")
        .and_then(Value::as_array)
        .map_or(0, |lines| {
            lines
                .iter()
                .take_while(|l| l.as_str().is_some_and(|l| l.starts_with(' ')))
                .count()
        });
    let line = start.saturating_add(u32::try_from(context).ok()?);
    (line > 0).then_some(line)
}

/// Approvals and questions need the user. `idle_prompt` fires ~60 s after `Stop` (already
/// `done`); the remaining types are informational. No type (older builds) = needs input.
fn claude_notification(payload: &Value) -> Option<HookEvent> {
    match payload.get("notification_type").and_then(Value::as_str) {
        None | Some("permission_prompt" | "agent_needs_input") => Some(HookEvent::NeedsInput),
        Some(kind) if kind.starts_with("elicitation") => Some(HookEvent::NeedsInput),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_events_map_to_states() {
        let claude = HookAgent::parse("claude").unwrap();
        let none = Value::Null;
        assert_eq!(claude.event("SessionStart", &none), None);
        assert_eq!(
            claude.event("UserPromptSubmit", &none),
            Some(HookEvent::Busy)
        );
        assert_eq!(claude.event("Stop", &none), Some(HookEvent::Done));
        assert_eq!(claude.event("PreToolUse", &none), None);
    }

    #[test]
    fn claude_notifications_by_type() {
        let claude = HookAgent::Claude;
        let of = |kind: &str| claude.event("Notification", &json!({"notification_type": kind}));
        assert_eq!(of("permission_prompt"), Some(HookEvent::NeedsInput));
        assert_eq!(of("elicitation_dialog"), Some(HookEvent::NeedsInput));
        assert_eq!(of("agent_needs_input"), Some(HookEvent::NeedsInput));
        assert_eq!(of("idle_prompt"), None);
        assert_eq!(of("auth_success"), None);
        assert_eq!(
            claude.event(
                "Notification",
                &json!({"message": "Claude needs your permission"})
            ),
            Some(HookEvent::NeedsInput)
        );
    }

    #[test]
    fn codex_and_fake_events() {
        assert_eq!(
            HookAgent::Codex.event("agent-turn-complete", &Value::Null),
            Some(HookEvent::Done)
        );
        assert_eq!(HookAgent::Codex.event("other", &Value::Null), None);
        let fake = HookAgent::Fake;
        assert_eq!(fake.event("busy", &Value::Null), Some(HookEvent::Busy));
        assert_eq!(
            fake.event("needs_input", &Value::Null),
            Some(HookEvent::NeedsInput)
        );
        assert_eq!(fake.event("done", &Value::Null), Some(HookEvent::Done));
    }

    #[test]
    fn native_session_ids() {
        let claude = json!({"session_id": "abc-123", "hook_event_name": "Stop"});
        assert_eq!(
            HookAgent::Claude.native_session_id(&claude).as_deref(),
            Some("abc-123")
        );
        let codex = json!({"type": "agent-turn-complete", "thread-id": "t-1", "turn-id": "u"});
        assert_eq!(
            HookAgent::Codex.native_session_id(&codex).as_deref(),
            Some("t-1")
        );
        assert_eq!(
            HookAgent::Claude.native_session_id(&json!({"session_id": ""})),
            None
        );
        assert_eq!(HookAgent::Fake.native_session_id(&Value::Null), None);
    }

    #[test]
    fn edited_paths_from_tool_input() {
        let edit = json!({
            "hook_event_name": "PostToolUse",
            "tool_name": "Edit",
            "tool_input": {"file_path": "C:\\p\\src\\a.rs", "old_string": "x"}
        });
        assert_eq!(edited_paths(&edit), vec!["C:\\p\\src\\a.rs"]);
        let notebook = json!({"tool_input": {"notebook_path": "n.ipynb"}});
        assert_eq!(edited_paths(&notebook), vec!["n.ipynb"]);
        assert!(edited_paths(&json!({"tool_input": {"command": "ls"}})).is_empty());
        let read = json!({"tool_name": "Read", "tool_input": {"file_path": "a.rs"}});
        assert!(edited_paths(&read).is_empty());
        assert!(edited_paths(&json!({"tool_input": {"file_path": ""}})).is_empty());
        assert!(edited_paths(&json!({"type": "agent-turn-complete"})).is_empty());
        assert!(edited_paths(&Value::Null).is_empty());
    }

    #[test]
    fn edited_line_is_the_first_changed_line_of_the_patch() {
        let edit = json!({
            "tool_name": "Edit",
            "tool_input": {"file_path": "a.rs"},
            "tool_response": {"structuredPatch": [
                {"oldStart": 355, "newStart": 355, "lines": [" a", " b", "-old", "+new"]},
                {"oldStart": 400, "newStart": 399, "lines": ["+x"]}
            ]}
        });
        assert_eq!(edited_line(&edit), Some(357));
        let created = json!({"tool_response": {"type": "create", "structuredPatch": []}});
        assert_eq!(edited_line(&created), None);
        let pre = json!({"tool_name": "Edit", "tool_input": {"file_path": "a.rs"}});
        assert_eq!(edited_line(&pre), None);
        let bad = json!({"tool_response": {"structuredPatch": [{"newStart": 0, "lines": []}]}});
        assert_eq!(edited_line(&bad), None);
    }

    #[test]
    fn unknown_agents_are_rejected() {
        assert_eq!(HookAgent::parse("omp"), None);
        assert_eq!(HookAgent::parse("Claude"), None);
    }
}
