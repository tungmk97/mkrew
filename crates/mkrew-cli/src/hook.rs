//! `mk hook <agent> <event>`: called by agent hooks inside a session's PTY (SPEC §6, §9).
//!
//! - `claude <event>` / `copilot <event>`: the hook JSON comes on stdin.
//! - `codex <json>`: Codex `notify` appends the JSON as the last argument; the event is its `type`.
//! - anything else (`omp`, `fake`): `<event> [json]`.
//!
//! Without `MKREW_SESSION_ID` the agent runs outside Mkrew: exit 0 without doing anything.

use std::io::Read;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, bail};
use mkrew_core::Paths;
use mkrew_protocol::{HookReportParams, Request};
use serde_json::Value;
use ulid::Ulid;

const ENV_SESSION_ID: &str = "MKREW_SESSION_ID";
/// How long a hook waits for the daemon before giving up (agents block on their hooks).
const REPORT_TIMEOUT: Duration = Duration::from_secs(5);

pub fn hook(agent: String, args: Vec<String>) -> anyhow::Result<ExitCode> {
    let Some(id) = std::env::var(ENV_SESSION_ID)
        .ok()
        .filter(|id| !id.is_empty())
    else {
        return Ok(ExitCode::SUCCESS);
    };
    let mkrew_session_id: Ulid = id
        .parse()
        .with_context(|| format!("{ENV_SESSION_ID}={id:?} is not a session id"))?;
    let (event, payload) = event_and_payload(&agent, &args, || {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .context("read hook JSON from stdin")?;
        Ok(text)
    })?;
    let paths = Paths::from_env()?;
    let report = Request::HookReport(HookReportParams {
        mkrew_session_id,
        agent,
        event,
        payload,
    });
    // The agent waits for this hook (Claude up to 60 s per event): a wedged daemon must not
    // stall it, so give up after `REPORT_TIMEOUT`. Exiting ends the stuck thread.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let sent = crate::project::connect(&paths).and_then(|mut c| c.call::<()>(report));
        let _ = tx.send(sent);
    });
    match rx.recv_timeout(REPORT_TIMEOUT) {
        Ok(sent) => sent?,
        Err(_) => bail!(
            "the daemon did not answer within {}s",
            REPORT_TIMEOUT.as_secs()
        ),
    }
    Ok(ExitCode::SUCCESS)
}

/// The event name and JSON payload for `agent`; `stdin` is only read for Claude and Copilot.
fn event_and_payload(
    agent: &str,
    args: &[String],
    stdin: impl FnOnce() -> anyhow::Result<String>,
) -> anyhow::Result<(String, Value)> {
    match (agent, args) {
        ("codex", [.., json]) => {
            let payload = parse_json(json)?;
            let Some(event) = payload.get("type").and_then(Value::as_str) else {
                bail!("codex notify JSON has no `type`");
            };
            Ok((event.to_owned(), payload))
        }
        ("codex", []) => bail!("usage: mk hook codex <json>"),
        ("claude" | "copilot", [event]) => {
            let text = stdin()?;
            let payload = match text.trim() {
                "" => Value::Null,
                json => parse_json(json)?,
            };
            Ok((event.clone(), payload))
        }
        (_, [event]) => Ok((event.clone(), Value::Null)),
        (_, [event, json]) if !matches!(agent, "claude" | "copilot") => {
            Ok((event.clone(), parse_json(json)?))
        }
        _ => bail!("usage: mk hook {agent} <event>"),
    }
}

fn parse_json(text: &str) -> anyhow::Result<Value> {
    serde_json::from_str(text).context("hook payload is not valid JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn no_stdin() -> anyhow::Result<String> {
        panic!("stdin must not be read")
    }

    #[test]
    fn claude_reads_json_from_stdin() {
        let stdin =
            || Ok(r#"{"session_id": "s1", "notification_type": "permission_prompt"}"#.into());
        let (event, payload) =
            event_and_payload("claude", &args(&["Notification"]), stdin).unwrap();
        assert_eq!(event, "Notification");
        assert_eq!(payload["session_id"], "s1");

        let (_, payload) =
            event_and_payload("claude", &args(&["Stop"]), || Ok(" \n".into())).unwrap();
        assert_eq!(payload, Value::Null);
        assert!(event_and_payload("claude", &args(&["Stop"]), || Ok("{".into())).is_err());
        assert!(event_and_payload("claude", &args(&[]), no_stdin).is_err());
        assert!(event_and_payload("claude", &args(&["Stop", "{}"]), no_stdin).is_err());
    }

    #[test]
    fn codex_takes_the_last_argument_as_json() {
        let json = r#"{"type": "agent-turn-complete", "thread-id": "t-1"}"#;
        let (event, payload) = event_and_payload("codex", &args(&[json]), no_stdin).unwrap();
        assert_eq!(event, "agent-turn-complete");
        assert_eq!(payload["thread-id"], "t-1");
        assert!(event_and_payload("codex", &args(&[r#"{"thread-id": "t"}"#]), no_stdin).is_err());
        assert!(event_and_payload("codex", &args(&["not json"]), no_stdin).is_err());
        assert!(event_and_payload("codex", &args(&[]), no_stdin).is_err());
    }

    #[test]
    fn fake_takes_event_and_optional_json() {
        let (event, payload) =
            event_and_payload("fake", &args(&["needs_input"]), no_stdin).unwrap();
        assert_eq!((event.as_str(), payload), ("needs_input", Value::Null));
        let (_, payload) =
            event_and_payload("fake", &args(&["busy", r#"{"session_id": "n"}"#]), no_stdin)
                .unwrap();
        assert_eq!(payload, json!({"session_id": "n"}));
        assert!(event_and_payload("fake", &args(&[]), no_stdin).is_err());
    }

    #[test]
    fn copilot_reads_json_from_stdin() {
        let stdin = || Ok(r#"{"sessionId": "c1", "source": "startup"}"#.into());
        let (event, payload) =
            event_and_payload("copilot", &args(&["sessionStart"]), stdin).unwrap();
        assert_eq!(event, "sessionStart");
        assert_eq!(payload["sessionId"], "c1");
        assert!(event_and_payload("copilot", &args(&["agentStop", "{}"]), no_stdin).is_err());
    }

    #[test]
    fn omp_takes_event_and_json_argument() {
        let (event, payload) =
            event_and_payload("omp", &args(&["busy", r#"{"session_id": "o1"}"#]), no_stdin)
                .unwrap();
        assert_eq!(
            (event.as_str(), payload),
            ("busy", json!({"session_id": "o1"}))
        );
    }
}
