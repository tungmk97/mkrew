//! Run commands of a workspace (SPEC §10, ADR 0018): the env file parser, the port scanner,
//! the line typed into a run session's shell and the layout of the `run` tab. Pure.

use std::sync::LazyLock;

use mkrew_protocol::{Session, SessionKind};
use regex::Regex;
use serde_json::{Value, json};
use ulid::Ulid;

/// Title of the tab holding the run sessions.
pub const TAB_TITLE: &str = "run";
/// A run session is a shell titled `run:<name>`.
pub const SESSION_PREFIX: &str = "run:";
/// Ports kept per run session.
pub const MAX_PORTS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error("line {line}: expected KEY=VALUE")]
    EnvLine { line: usize },
}

/// `KEY=VALUE` lines like a `.env` file: blank lines and `#` comments skipped, a leading BOM
/// and an `export ` prefix dropped, key and value trimmed, one pair of matching quotes around
/// the value removed. No interpolation, no multi-line values.
pub fn parse_env(text: &str) -> Result<Vec<(String, String)>, RunError> {
    let mut vars = Vec::new();
    for (index, line) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        let bad = RunError::EnvLine { line: index + 1 };
        let Some((key, value)) = line.split_once('=') else {
            return Err(bad);
        };
        let key = key.trim();
        if !valid_key(key) {
            return Err(bad);
        }
        vars.push((key.to_owned(), unquote(value.trim()).to_owned()));
    }
    Ok(vars)
}

/// `[A-Za-z_][A-Za-z0-9_]*`.
pub(crate) fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

fn unquote(value: &str) -> &str {
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return &value[1..value.len() - 1];
        }
    }
    value
}

// Constant patterns, compiled in the tests: `expect` cannot fire.
#[allow(clippy::expect_used)]
static URL_PORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bhttps?://(?:\[[^\]\s]*\]|[^\s/:\[\]]+):(\d{1,5})\b").expect("valid regex")
});
#[allow(clippy::expect_used)]
static WORD_PORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bports?(?:\(s\))?\s*[:=]?\s*(\d{1,5})\b").expect("valid regex")
});

/// The ports a server prints on `line`, in order, without repeats: `http(s)://host:<port>`
/// and `port <n>` / `port: <n>` / `port=<n>` / `port(s): <n>`; 1–65535 only.
pub fn scan_ports(line: &str) -> Vec<u16> {
    let mut found: Vec<(usize, u16)> = [&*URL_PORT, &*WORD_PORT]
        .into_iter()
        .flat_map(|re| {
            re.captures_iter(line).filter_map(|caps| {
                let digits = caps.get(1)?;
                let port = digits.as_str().parse::<u16>().ok().filter(|p| *p > 0)?;
                Some((digits.start(), port))
            })
        })
        .collect();
    found.sort_unstable();
    let mut ports = Vec::new();
    for (_, port) in found {
        if !ports.contains(&port) {
            ports.push(port);
        }
    }
    ports
}

/// The line typed into a run session's `shell` (its program path): the command, then an exit
/// in that shell's syntax, so the session ends with the command.
pub fn typed_line(shell: &str, command: &str) -> String {
    let file = shell.rsplit(['/', '\\']).next().unwrap_or(shell);
    let stem = file.split('.').next().unwrap_or(file).to_ascii_lowercase();
    match stem.as_str() {
        "cmd" => format!("{command} & exit\r"),
        "pwsh" | "powershell" => format!("{command}; exit $LASTEXITCODE\r"),
        _ => format!("{command}; exit\r"),
    }
}

/// Title of the session running the command `name`.
pub fn session_title(name: &str) -> String {
    format!("{SESSION_PREFIX}{name}")
}

/// The run command `session` runs, if it is a run session.
pub fn command_name(session: &Session) -> Option<&str> {
    if session.kind != SessionKind::Shell {
        return None;
    }
    session.title.as_deref()?.strip_prefix(SESSION_PREFIX)
}

/// A tab layout (SPEC §10 split tree) with `sessions` side by side, equal widths.
pub fn layout(sessions: &[Ulid]) -> Value {
    match sessions {
        [] => Value::Null,
        [only] => json!({"session": only.to_string()}),
        [first, rest @ ..] => json!({
            "split": "vertical",
            "ratio": 1.0 / sessions.len() as f64,
            "children": [{"session": first.to_string()}, layout(rest)],
        }),
    }
}

#[cfg(test)]
mod tests {
    use mkrew_protocol::{Session, SessionKind, SessionState, StateSource};
    use serde_json::json;
    use ulid::Ulid;

    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn env_files_read_like_dotenv() {
        let text =
            "\u{feff}# comment\r\n\r\nexport A = 1 \r\nB=\"two words\"\nC='x=y'\nD=\nE=a#b\n";
        assert_eq!(
            parse_env(text).unwrap(),
            vars(&[
                ("A", "1"),
                ("B", "two words"),
                ("C", "x=y"),
                ("D", ""),
                ("E", "a#b")
            ])
        );
        assert_eq!(
            parse_env("A=1\nnot a pair\n"),
            Err(RunError::EnvLine { line: 2 })
        );
        assert_eq!(parse_env("1A=x"), Err(RunError::EnvLine { line: 1 }));
        assert_eq!(parse_env("A-B=x"), Err(RunError::EnvLine { line: 1 }));
        assert_eq!(parse_env(""), Ok(Vec::new()));
    }

    #[test]
    fn ports_come_from_urls_and_port_words() {
        assert_eq!(
            scan_ports("INFO:     Uvicorn running on http://0.0.0.0:8080 (Press CTRL+C to quit)"),
            [8080]
        );
        assert_eq!(scan_ports("  ➜  Local:   http://localhost:5180/"), [5180]);
        assert_eq!(
            scan_ports("Tomcat started on port(s): 8080 (http) with context path ''"),
            [8080]
        );
        assert_eq!(scan_ports("Now listening on: http://[::]:5000"), [5000]);
        assert_eq!(
            scan_ports("Listening on port 3000, PORT=3001 and http://127.0.0.1:3000"),
            [3000, 3001]
        );
        for none in [
            "12:34:56 started",
            "version 1.2.3:4",
            "port 99999",
            "port 123456",
            "report 42",
            "port 0",
            "http://localhost/",
        ] {
            assert_eq!(scan_ports(none), Vec::<u16>::new(), "{none}");
        }
    }

    #[test]
    fn the_typed_line_exits_the_shell_when_the_command_ends() {
        assert_eq!(
            typed_line(r"C:\Windows\System32\cmd.exe", "npm run dev"),
            "npm run dev & exit\r"
        );
        assert_eq!(
            typed_line("pwsh", "npm run dev"),
            "npm run dev; exit $LASTEXITCODE\r"
        );
        assert_eq!(
            typed_line("C:/Program Files/PowerShell/7/PWSH.EXE", "x"),
            "x; exit $LASTEXITCODE\r"
        );
        assert_eq!(typed_line("powershell.exe", "x"), "x; exit $LASTEXITCODE\r");
        assert_eq!(typed_line("/bin/bash", "cargo run"), "cargo run; exit\r");
    }

    fn shell(kind: SessionKind, title: Option<&str>) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: Ulid::new(),
            agent_profile: "shell".into(),
            kind,
            title: title.map(Into::into),
            native_session_id: None,
            state: SessionState::Working,
            state_source: StateSource::Screen,
            state_changed_at: 0,
            exit_code: None,
            cols: 80,
            rows: 24,
            started_at: 0,
            ended_at: None,
        }
    }

    #[test]
    fn run_sessions_are_shells_titled_run_colon_name() {
        assert_eq!(session_title("BE"), "run:BE");
        assert_eq!(
            command_name(&shell(SessionKind::Shell, Some("run:BE"))),
            Some("BE")
        );
        assert_eq!(
            command_name(&shell(SessionKind::Agent, Some("run:BE"))),
            None
        );
        assert_eq!(command_name(&shell(SessionKind::Shell, Some("dev"))), None);
        assert_eq!(command_name(&shell(SessionKind::Shell, None)), None);
    }

    #[test]
    fn the_layout_puts_sessions_side_by_side_with_equal_widths() {
        let [a, b, c] = [Ulid::new(), Ulid::new(), Ulid::new()];
        assert_eq!(layout(&[]), serde_json::Value::Null);
        assert_eq!(layout(&[a]), json!({"session": a.to_string()}));
        assert_eq!(
            layout(&[a, b, c]),
            json!({"split": "vertical", "ratio": 1.0 / 3.0, "children": [
                {"session": a.to_string()},
                {"split": "vertical", "ratio": 0.5, "children": [
                    {"session": b.to_string()}, {"session": c.to_string()}
                ]}
            ]})
        );
    }
}
