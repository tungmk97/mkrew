//! `mk hook <agent> <event>`: called by agent hooks inside a session's PTY (SPEC §6, §9).
//!
//! - `claude <event>` / `copilot <event>`: the hook JSON comes on stdin.
//! - `codex <json>`: Codex `notify` appends the JSON as the last argument; the event is its `type`.
//! - anything else (`omp`, `fake`): `<event> [json]`.
//!
//! Without `MKREW_SESSION_ID` the agent runs outside Mkrew: exit 0 without doing anything.
//! Inside a pane `MKREW_MK` names the daemon's `mk`; when that is another file, it runs instead.
//! Failures go to `<data>/logs/hook.log`.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, bail};
use mkrew_core::Paths;
use mkrew_core::paths::ENV_MK;
use mkrew_protocol::{HookReportParams, Request};
use serde_json::Value;
use ulid::Ulid;

const ENV_SESSION_ID: &str = "MKREW_SESSION_ID";
/// How long a hook waits for the daemon before giving up (agents block on their hooks).
const REPORT_TIMEOUT: Duration = Duration::from_secs(5);

/// `hook.log` is rotated to `hook.log.1` above this size.
const HOOK_LOG_CAP: u64 = 1024 * 1024;

pub fn hook(agent: String, args: Vec<String>) -> anyhow::Result<ExitCode> {
    let Some(id) = std::env::var(ENV_SESSION_ID)
        .ok()
        .filter(|id| !id.is_empty())
    else {
        return Ok(ExitCode::SUCCESS);
    };
    let me = std::env::current_exe().unwrap_or_default();
    if let Some(mk) = forward_target(std::env::var_os(ENV_MK), &me) {
        // The pane's daemon has its own `mk`; the hook config may name another one (ADR 0055).
        match std::process::Command::new(&mk)
            .arg("hook")
            .arg(&agent)
            .args(&args)
            .status()
        {
            Ok(status) => {
                let code = status
                    .code()
                    .and_then(|c| u8::try_from(c).ok())
                    .unwrap_or(1);
                return Ok(ExitCode::from(code));
            }
            Err(e) => log_to_data_dir(&agent, &anyhow::anyhow!("run {}: {e}", mk.display())),
        }
    }
    let result = report(&id, agent.clone(), &args);
    if let Err(e) = &result {
        log_to_data_dir(&agent, e);
    }
    result.map(|()| ExitCode::SUCCESS)
}

/// `MKREW_MK` when it is an existing file other than `me` (canonical paths, so no loop).
fn forward_target(mk: Option<OsString>, me: &Path) -> Option<PathBuf> {
    let mk = PathBuf::from(mk?);
    if !mk.is_absolute() {
        return None;
    }
    let target = dunce::canonicalize(&mk).ok()?;
    let me = dunce::canonicalize(me).ok();
    (Some(&target) != me.as_ref()).then_some(target)
}

fn log_to_data_dir(agent: &str, error: &anyhow::Error) {
    if let Ok(paths) = Paths::from_env() {
        log_failure(&paths.logs_dir(), agent, error);
    }
}

/// Appends one line to `<logs>/hook.log`: agents drop a hook's stderr, so this is the only
/// trace (ADR 0055). Every I/O error is ignored: logging must never fail the hook.
fn log_failure(logs: &Path, agent: &str, error: &anyhow::Error) {
    use std::io::Write as _;
    let _ = std::fs::create_dir_all(logs);
    let file = logs.join("hook.log");
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > HOOK_LOG_CAP) {
        let _ = std::fs::rename(&file, logs.join("hook.log.1"));
    }
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let exe = std::env::current_exe()
        .ok()
        .and_then(|p| dunce::canonicalize(p).ok())
        .map_or_else(|| "mk".to_owned(), |p| p.display().to_string());
    let error = format!("{error:#}").replace('\n', " ");
    let line = format!(
        "{ms} {exe} mk {} hook {agent}: {error}\n",
        env!("CARGO_PKG_VERSION")
    );
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
    {
        let _ = f.write_all(line.as_bytes());
    }
}

fn report(id: &str, agent: String, args: &[String]) -> anyhow::Result<()> {
    let mkrew_session_id: Ulid = id
        .parse()
        .with_context(|| format!("{ENV_SESSION_ID}={id:?} is not a session id"))?;
    let (event, payload) = event_and_payload(&agent, args, || {
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
    Ok(())
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
    fn forward_target_ignores_relative_missing_and_itself() {
        let dir = tempfile::tempdir().unwrap();
        let me = std::env::current_exe().unwrap();
        assert_eq!(
            forward_target(Some("mk".into()), &me),
            None,
            "bare fallback"
        );
        assert_eq!(
            forward_target(Some(dir.path().join("gone.exe").into()), &me),
            None
        );
        assert_eq!(forward_target(Some(me.clone().into()), &me), None, "itself");
        assert_eq!(forward_target(None, &me), None);
        let other = dir.path().join("mk.exe");
        std::fs::write(&other, b"").unwrap();
        assert_eq!(
            forward_target(Some(other.clone().into()), &me),
            Some(dunce::canonicalize(&other).unwrap())
        );
    }

    #[test]
    fn hook_log_rotates_above_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        log_failure(&logs, "omp", &anyhow::anyhow!("first"));
        let log = std::fs::read_to_string(logs.join("hook.log")).unwrap();
        assert!(log.ends_with(" hook omp: first\n"), "{log}");
        std::fs::write(
            logs.join("hook.log"),
            vec![b'x'; (HOOK_LOG_CAP + 1) as usize],
        )
        .unwrap();
        log_failure(&logs, "claude", &anyhow::anyhow!("second"));
        assert_eq!(
            std::fs::metadata(logs.join("hook.log.1")).unwrap().len(),
            HOOK_LOG_CAP + 1
        );
        let log = std::fs::read_to_string(logs.join("hook.log")).unwrap();
        assert!(
            log.ends_with(" hook claude: second\n") && log.lines().count() == 1,
            "{log}"
        );
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
