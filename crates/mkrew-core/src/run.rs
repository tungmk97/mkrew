//! Run commands of a workspace (SPEC §10, ADR 0018): the env file parser, the port scanner,
//! the line typed into a run session's shell and the layout of the `run` tab. Pure.

use std::path::{Component, Path};
use std::sync::LazyLock;

use mkrew_protocol::{RunCommand, Session, SessionKind, SessionState};
use regex::Regex;
use serde_json::{Value, json};
use ulid::Ulid;

/// Title of the tab holding the run sessions.
pub const TAB_TITLE: &str = "run";
/// A run session is a shell titled `run:<name>`.
pub const SESSION_PREFIX: &str = "run:";
/// Ports kept per run session.
pub const MAX_PORTS: usize = 4;

/// Commands per project.
pub const MAX_COMMANDS: usize = 8;
/// Characters in a command name.
pub const MAX_NAME_CHARS: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error("line {line}: expected KEY=VALUE")]
    EnvLine { line: usize },
    #[error("at most {MAX_COMMANDS} run commands")]
    TooMany,
    #[error("run command name {0:?} must have 1–{MAX_NAME_CHARS} characters, without / or :")]
    BadName(String),
    #[error("two run commands are named {0:?}")]
    DuplicateName(String),
    #[error("{0}: the command must be one non-empty line")]
    BadCommand(String),
    #[error("{name}: {path:?} must be a relative path inside the project")]
    BadPath { name: String, path: String },
    #[error("{name}: {key:?} is not an environment variable name")]
    BadKey { name: String, key: String },
}

/// The commands to save: fields trimmed (blank `dir` = `.`, blank `env_file` = none) and
/// checked (SPEC §10). The name is checked before the command. Folders and files are checked
/// when a command starts, not here.
pub fn normalize(commands: Vec<RunCommand>) -> Result<Vec<RunCommand>, RunError> {
    if commands.len() > MAX_COMMANDS {
        return Err(RunError::TooMany);
    }
    let mut checked: Vec<RunCommand> = Vec::with_capacity(commands.len());
    for raw in commands {
        let name = raw.name.trim().to_owned();
        if name.is_empty()
            || name.chars().count() > MAX_NAME_CHARS
            || name.contains(['/', ':'])
            || name.chars().any(char::is_control)
        {
            return Err(RunError::BadName(name));
        }
        if checked.iter().any(|c| c.name == name) {
            return Err(RunError::DuplicateName(name));
        }
        let command = raw.command.trim();
        if command.is_empty() || command.contains(['\n', '\r']) {
            return Err(RunError::BadCommand(name));
        }
        let dir = match raw.dir.trim() {
            "" => ".".to_owned(),
            dir => dir.to_owned(),
        };
        let env_file = raw
            .env_file
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty())
            .map(str::to_owned);
        for path in std::iter::once(&dir).chain(env_file.as_ref()) {
            if !inside(path) {
                let path = path.clone();
                return Err(RunError::BadPath { name, path });
            }
        }
        if let Some((key, _)) = raw.env.iter().find(|(key, _)| !valid_key(key)) {
            let key = key.clone();
            return Err(RunError::BadKey { name, key });
        }
        checked.push(RunCommand {
            command: command.to_owned(),
            name,
            dir,
            env_file,
            env: raw.env,
        });
    }
    Ok(checked)
}

/// A relative path that stays inside its base folder.
fn inside(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
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

/// The live run sessions of a project in tab order: configured commands in config order, then
/// the ones whose name left the config (oldest first).
pub fn ordered_live(commands: &[RunCommand], sessions: &[Session]) -> Vec<Ulid> {
    let mut live: Vec<(usize, i64, Ulid)> = sessions
        .iter()
        .filter(|s| s.state != SessionState::Exited)
        .filter_map(|s| {
            let name = command_name(s)?;
            let position = commands
                .iter()
                .position(|c| c.name == name)
                .unwrap_or(usize::MAX);
            Some((position, s.started_at, s.id))
        })
        .collect();
    live.sort_unstable();
    live.into_iter().map(|(_, _, id)| id).collect()
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

    fn command(name: &str, dir: &str, command: &str) -> mkrew_protocol::RunCommand {
        mkrew_protocol::RunCommand {
            name: name.into(),
            dir: dir.into(),
            command: command.into(),
            env_file: None,
            env: Vec::new(),
        }
    }

    #[test]
    fn live_run_sessions_follow_the_config_order() {
        let commands = [command("BE", ".", "x"), command("FE", ".", "y")];
        let mut fe = shell(SessionKind::Shell, Some("run:FE"));
        let mut be = shell(SessionKind::Shell, Some("run:BE"));
        let mut gone = shell(SessionKind::Shell, Some("run:old"));
        let mut exited = shell(SessionKind::Shell, Some("run:BE"));
        let agent = shell(SessionKind::Agent, Some("run:BE"));
        (fe.started_at, be.started_at, gone.started_at) = (1, 2, 0);
        exited.state = SessionState::Exited;
        let sessions = [gone.clone(), fe.clone(), exited, agent, be.clone()];
        assert_eq!(ordered_live(&commands, &sessions), [be.id, fe.id, gone.id]);
    }

    #[test]
    fn run_commands_are_trimmed_and_checked() {
        let raw = mkrew_protocol::RunCommand {
            env_file: Some("  ".into()),
            env: vars(&[("PORT", "1")]),
            ..command(" BE ", " ", " npm run dev ")
        };
        assert_eq!(
            normalize(vec![raw]).unwrap(),
            [mkrew_protocol::RunCommand {
                env: vars(&[("PORT", "1")]),
                ..command("BE", ".", "npm run dev")
            }]
        );
        let file = mkrew_protocol::RunCommand {
            env_file: Some(" .env ".into()),
            ..command("FE", "EA-FE", "x")
        };
        assert_eq!(
            normalize(vec![file]).unwrap()[0].env_file.as_deref(),
            Some(".env")
        );
        assert_eq!(normalize(Vec::new()), Ok(Vec::new()));

        let too_many = (0..=MAX_COMMANDS)
            .map(|i| command(&i.to_string(), ".", "x"))
            .collect();
        assert_eq!(normalize(too_many), Err(RunError::TooMany));
        for name in ["", "a/b", "a:b", "123456789012345678901"] {
            assert_eq!(
                normalize(vec![command(name, ".", "x")]),
                Err(RunError::BadName(name.into())),
                "{name}"
            );
        }
        assert_eq!(
            normalize(vec![command("A", ".", "x"), command("A", ".", "y")]),
            Err(RunError::DuplicateName("A".into()))
        );
        for bad in ["", "a\nb"] {
            assert_eq!(
                normalize(vec![command("A", ".", bad)]),
                Err(RunError::BadCommand("A".into()))
            );
        }
        let windows_only = cfg!(windows).then_some([r"\root", "C:/x", "C:x"]);
        for path in ["..", "a/../b", "/abs"]
            .into_iter()
            .chain(windows_only.into_iter().flatten())
        {
            assert_eq!(
                normalize(vec![command("A", path, "x")]),
                Err(RunError::BadPath {
                    name: "A".into(),
                    path: path.into()
                }),
                "{path}"
            );
        }
        let key = mkrew_protocol::RunCommand {
            env: vars(&[("1X", "v")]),
            ..command("A", ".", "x")
        };
        assert_eq!(
            normalize(vec![key]),
            Err(RunError::BadKey {
                name: "A".into(),
                key: "1X".into()
            })
        );
    }
}
