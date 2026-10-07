# Run Button Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the one-command dev server with a per-workspace Run button that runs a saved list of commands (name, folder, command, env file, env), restarts/stops them, and shows their ports.

**Architecture:** Commands live in a new `run_command` table and travel on `Project.run_commands`. The daemon types each command into its own `shell` session titled `run:<name>` (ADR 0008's mechanism, generalised), lays them side by side in a `run` tab, and scans each run session's screen for ports. Pure logic (env parsing, port scanning, layout, validation) lives in `mkrew_core::run`; the UI keeps pure models in `run.rs` / `run_setup.rs` and wires them in `main.rs`.

**Tech Stack:** Rust 2024, rusqlite (bundled), tokio, portable-pty, gpui, clap, regex (already in mkrew-core).

**Spec:** `docs/superpowers/specs/2026-10-07-run-button-design.md` (read it first; it wins on conflicts with this plan).

## Global Constraints

- Work happens in the worktree `mkrew-run-button` on branch `feat/run-button` (human-approved). Commit each task there with a Conventional Commit message; never push, never touch `master`, never rewrite history.
- Clean cutover: `dev_command`, `dev_autostart`, `DevServer`, `DevStartParams`, `project.dev_start`, `project.dev_stop`, `mk project dev`, `--dev`, `--no-dev`, `--dev-autostart`, `crates/mkrew-ui/src/dev_server.rs`, `crates/mkrew-daemon/src/server/dev.rs` are removed, not aliased.
- `PROTOCOL_VERSION` becomes `25` (once, in Task 2).
- New migration file is `crates/mkrew-store/migrations/0011_run_commands.sql` (0010 exists). Never edit older migrations.
- Run session title: `run:<name>`; run tab title: `run`; max 8 commands; name 1–20 chars without `/` or `:`; max 4 ports per session.
- No `unwrap/expect` outside tests unless provably infallible (comment why). `thiserror` in libraries.
- UI: every color/spacing/radius/font size from `crates/mkrew-ui/src/theme.rs`. Status = icon + text, never color alone.
- No test may call a real agent or the network. Tests must pass on Windows (cmd is the default shell there) and Unix.
- Verification on Windows: per-crate `cargo test -p <crate>`; full suite `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh`; if rustc crashes on mkrew-ui test builds, run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test --workspace -j 4`.
- A locked `mkrew-daemon.exe`/`mkrew-fakeagent.exe` ("Access is denied"): rename it to `*.locked-old.exe`, never kill the user's sessions.

## Review Focus

1. **A command that ends at once** (typo, missing `npm`): its session must exit (typed line ends with an exit), the button must not claim it runs, and Restart of that one command must start it again. → Task 4, test `run_commands_auto_start_after_a_daemon_restart_and_end_with_their_command`.
2. **Real-world env files** (BOM, CRLF, `export`, quotes, `=` in value, `#` inside a value): parsed like dotenv, bad lines named by number. → Task 1, `env_files_read_like_dotenv`.
3. **Port false positives** (times `12:34:56`, versions, `report 42`, `port 99999`): no port shown. → Task 1, `ports_come_from_urls_and_port_words`.
4. **A command removed from the setup while it runs**: "Stop all" still stops it. → Task 4, `run_commands_start_in_their_folders_with_env_side_by_side_restart_and_stop`.
5. **Enter in the Env box** of Run setup adds a line instead of saving; an invalid env line blocks saving with a message. → Task 7, `enter_in_env_adds_a_line_and_bad_env_blocks_saving`.

---

### Task 1: Pure run helpers in mkrew-core

**Files:**
- Create: `crates/mkrew-core/src/run.rs`
- Modify: `crates/mkrew-core/src/lib.rs` (add `pub mod run;` between `pub mod review;` and `pub mod settings;`)

**Interfaces:**
- Consumes: `mkrew_protocol::{Session, SessionKind}` (exist).
- Produces (all in `mkrew_core::run`):
  - `pub const TAB_TITLE: &str = "run";`
  - `pub const SESSION_PREFIX: &str = "run:";`
  - `pub const MAX_PORTS: usize = 4;`
  - `pub enum RunError` (thiserror; Task 2 adds variants)
  - `pub fn parse_env(text: &str) -> Result<Vec<(String, String)>, RunError>`
  - `pub fn scan_ports(line: &str) -> Vec<u16>`
  - `pub fn typed_line(shell: &str, command: &str) -> String`
  - `pub fn session_title(name: &str) -> String`
  - `pub fn command_name(session: &Session) -> Option<&str>`
  - `pub fn layout(sessions: &[ulid::Ulid]) -> serde_json::Value`

- [ ] **Step 1: Write the failing tests** — create `crates/mkrew-core/src/run.rs` with only the test module:

```rust
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
        let text = "\u{feff}# comment\r\n\r\nexport A = 1 \r\nB=\"two words\"\nC='x=y'\nD=\nE=a#b\n";
        assert_eq!(
            parse_env(text).unwrap(),
            vars(&[("A", "1"), ("B", "two words"), ("C", "x=y"), ("D", ""), ("E", "a#b")])
        );
        assert_eq!(parse_env("A=1\nnot a pair\n"), Err(RunError::EnvLine { line: 2 }));
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
        assert_eq!(typed_line("pwsh", "npm run dev"), "npm run dev; exit $LASTEXITCODE\r");
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
        assert_eq!(command_name(&shell(SessionKind::Shell, Some("run:BE"))), Some("BE"));
        assert_eq!(command_name(&shell(SessionKind::Agent, Some("run:BE"))), None);
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
```

Add `pub mod run;` to `crates/mkrew-core/src/lib.rs`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mkrew-core run::`
Expected: compile errors (`parse_env`, `scan_ports`, … not found).

- [ ] **Step 3: Write the implementation** — put this above the test module in `crates/mkrew-core/src/run.rs`:

```rust
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
static URL_PORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\bhttps?://(?:\[[^\]\s]*\]|[^\s/:\[\]]+):(\d{1,5})\b").expect("valid regex")
});
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
```

`mkrew-core/Cargo.toml` already lists `regex`, `serde_json`, `thiserror`, `ulid`; add nothing.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p mkrew-core run::`
Expected: 5 tests PASS.

- [ ] **Step 5: Clippy**

Run: `cargo clippy -p mkrew-core --all-targets -- -D warnings`
Expected: no warnings.

---

### Task 2: Cut over from the dev server to run command data

One cutover across all crates so the workspace compiles at the end of the task. After it, the app has no Run feature yet (Tasks 4–8 add it) and no dev server.

**Files:**
- Create: `crates/mkrew-store/migrations/0011_run_commands.sql`
- Modify: `crates/mkrew-protocol/src/message.rs`, `crates/mkrew-protocol/src/lib.rs`
- Modify: `crates/mkrew-core/src/run.rs`
- Modify: `crates/mkrew-store/src/lib.rs`, `crates/mkrew-store/src/tests.rs`
- Modify: `crates/mkrew-daemon/src/server.rs`; Delete: `crates/mkrew-daemon/src/server/dev.rs`; Modify: `crates/mkrew-daemon/tests/daemon.rs`
- Modify: `crates/mkrew-cli/src/main.rs`, `crates/mkrew-cli/src/project.rs`, `crates/mkrew-cli/tests/cli.rs`, `crates/mkrew-cli/tests/readme.rs`, `README.md`
- Delete: `crates/mkrew-ui/src/dev_server.rs`; Modify: `crates/mkrew-ui/src/lib.rs`, `main.rs`, `palette.rs`, `workspace_menu.rs`, `inbox.rs`, `sidebar.rs`, and `crates/mkrew-ui/tests/{board_shared,connect,docs_view,lifecycle,measure,restore,stress}.rs`

**Interfaces:**
- Consumes: `mkrew_core::run::{RunError, valid_key}` (Task 1).
- Produces:
  - `mkrew_protocol::RunCommand { name: String, dir: String, command: String, env_file: Option<String>, env: Vec<(String, String)> }`
  - `Project { …, run_commands: Vec<RunCommand>, run_autostart: bool }` (no `dev_*`)
  - `ProjectUpdateParams { project_id, name, default_agent, run_commands: Option<Vec<RunCommand>>, run_autostart: Option<bool> }`
  - `ProjectAddParams { path, name }`
  - `mkrew_protocol::{DEFAULT_COLS, DEFAULT_ROWS}: u16` (120, 30) and private `fn default_cols()/default_rows()` for serde defaults
  - `mkrew_core::run::{normalize, MAX_COMMANDS, MAX_NAME_CHARS}`; `normalize(Vec<RunCommand>) -> Result<Vec<RunCommand>, RunError>`
  - `mkrew_store::ProjectChanges { name, default_agent, run_commands: Option<&'a [RunCommand]>, run_autostart: Option<bool> }`; `Store::add_project(path, name)` (no `add_project_with_dev`)
  - CLI: `mk project update <p> --run-autostart on|off` prints `run auto-start of <name>: on|off`

- [ ] **Step 1: Write the failing core test** — append to the test module of `crates/mkrew-core/src/run.rs`:

```rust
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
        assert_eq!(normalize(vec![file]).unwrap()[0].env_file.as_deref(), Some(".env"));
        assert_eq!(normalize(Vec::new()), Ok(Vec::new()));

        let too_many = (0..=MAX_COMMANDS).map(|i| command(&i.to_string(), ".", "x")).collect();
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
        for path in ["..", "a/../b", "/abs"].into_iter().chain(windows_only.into_iter().flatten()) {
            assert_eq!(
                normalize(vec![command("A", path, "x")]),
                Err(RunError::BadPath { name: "A".into(), path: path.into() }),
                "{path}"
            );
        }
        let key = mkrew_protocol::RunCommand {
            env: vars(&[("1X", "v")]),
            ..command("A", ".", "x")
        };
        assert_eq!(
            normalize(vec![key]),
            Err(RunError::BadKey { name: "A".into(), key: "1X".into() })
        );
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p mkrew-core run::`
Expected: compile errors (`RunCommand`, `normalize` not found).

- [ ] **Step 3: Protocol changes** in `crates/mkrew-protocol/src/message.rs`:

1. Replace the `Project` fields `dev_command` and `dev_autostart` (with their doc comments) by:

```rust
    /// The Run button's commands, in order (SPEC §10, ADR 0018).
    #[serde(default)]
    pub run_commands: Vec<RunCommand>,
    /// Run every command after the daemon starts.
    #[serde(default)]
    pub run_autostart: bool,
```

2. Add after `Project`:

```rust
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
```

3. In `ProjectAddParams` delete `dev_command` (and its doc comment).

4. In `ProjectUpdateParams` replace `dev_command` and `dev_autostart` by:

```rust
    /// Replaces the whole list; checked and trimmed by the daemon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_commands: Option<Vec<RunCommand>>,
    /// Run every command after the daemon starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_autostart: Option<bool>,
```

5. Delete `DevStartParams` (struct + impl) and `DevServer` (struct + impl). Add in their place:

```rust
/// Terminal size used when the client has no pane yet (CLI, auto-start, fan-out children).
pub const DEFAULT_COLS: u16 = 120;
pub const DEFAULT_ROWS: u16 = 30;

fn default_cols() -> u16 {
    DEFAULT_COLS
}

fn default_rows() -> u16 {
    DEFAULT_ROWS
}
```

and in `TaskFanoutParams` change `#[serde(default = "DevStartParams::default_cols")]` → `#[serde(default = "default_cols")]`, same for rows.

6. Delete the `Request` variants `ProjectDevStart` and `ProjectDevStop` with their doc comments and renames.

7. Tests in the same file:
   - `every_m1_request_round_trips`: `ProjectAdd(ProjectAddParams { path: r"C:\work\demo".into(), name: None })`; `ProjectUpdate` uses `run_commands: Some(vec![RunCommand { name: "BE".into(), dir: "api".into(), command: "uv run main.py".into(), env_file: Some(".env".into()), env: vec![("ENVIRONMENT".into(), "dev".into())] }]), run_autostart: Some(true)`; delete the `ProjectDevStart` and `ProjectDevStop` entries.
   - Delete `dev_start_size_defaults_when_left_out`.
   - `project_update_tells_missing_from_null`: replace the `dev_command`/`dev_autostart` assertions with:

```rust
        let autostart = parse(json!({"project_id": id, "run_autostart": true}));
        assert_eq!(autostart.run_autostart, Some(true));
        assert_eq!(autostart.run_commands, None);
        assert_eq!(
            parse(json!({"project_id": id, "run_commands": []})).run_commands,
            Some(Vec::new())
        );
```

   and the `keep` literal fields become `run_commands: None, run_autostart: None`.
   - `event_wire_shape_matches_spec`: the `Project` literal gets `run_commands: vec![RunCommand { name: "FE".into(), dir: ".".into(), command: "npm run dev".into(), env_file: None, env: Vec::new() }], run_autostart: true`.

8. `crates/mkrew-protocol/src/lib.rs`: `pub const PROTOCOL_VERSION: u32 = 25;`

- [ ] **Step 4: `normalize` in core** — in `crates/mkrew-core/src/run.rs` add `use std::path::{Component, Path};` and `use mkrew_protocol::RunCommand;`, replace Task 1's `RunError` and add:

```rust
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
```

- [ ] **Step 5: Run the core test**

Run: `cargo test -p mkrew-core run::`
Expected: 6 tests PASS.

- [ ] **Step 6: Store — migration and failing tests**

Create `crates/mkrew-store/migrations/0011_run_commands.sql`:

```sql
-- SPEC §5, Run button (ADR 0018). Never edit: schema changes go in a new numbered migration.
CREATE TABLE run_command (
  project_id TEXT NOT NULL REFERENCES project(id),
  position   INTEGER NOT NULL,           -- order in the menu and in the run tab
  name       TEXT NOT NULL,              -- unique per project, e.g. "BE"
  dir        TEXT NOT NULL DEFAULT '.',  -- relative to the project folder
  command    TEXT NOT NULL,              -- typed into the shell, e.g. "npm run dev"
  env_file   TEXT,                       -- relative to dir, e.g. ".env"
  env        TEXT NOT NULL DEFAULT '[]', -- JSON [[key, value], …], applied after env_file
  PRIMARY KEY (project_id, name)
);
INSERT INTO run_command (project_id, position, name, command)
  SELECT id, 0, 'dev', dev_command FROM project WHERE dev_command IS NOT NULL;
ALTER TABLE project DROP COLUMN dev_command;
ALTER TABLE project RENAME COLUMN dev_autostart TO run_autostart;
```

Append `include_str!("../migrations/0011_run_commands.sql"),` to `MIGRATIONS` in `crates/mkrew-store/src/lib.rs`.

In `crates/mkrew-store/src/tests.rs` replace `project_dev_server_settings_are_set_trimmed_and_cleared` with:

```rust
fn run(name: &str) -> RunCommand {
    RunCommand {
        name: name.into(),
        dir: "api".into(),
        command: "uv run main.py".into(),
        env_file: Some(".env".into()),
        env: vec![("ENVIRONMENT".into(), "dev".into())],
    }
}

#[test]
fn run_commands_are_saved_in_order_and_replaced_as_a_whole() {
    let dir = tempfile::tempdir().unwrap();
    let store = open(&dir);
    let p = store.add_project(dir.path(), None).unwrap();
    assert!(p.run_commands.is_empty() && !p.run_autostart);

    let two = [run("BE"), run("FE")];
    let saved = store
        .update_project(
            p.id,
            ProjectChanges {
                run_commands: Some(&two),
                run_autostart: Some(true),
                ..ProjectChanges::default()
            },
        )
        .unwrap();
    assert_eq!(saved.run_commands, two);
    assert!(saved.run_autostart);
    assert_eq!(open(&dir).get_project(p.id).unwrap(), saved);
    assert_eq!(open(&dir).list_projects().unwrap(), [saved.clone()]);

    // Leaving the list out keeps it; a new list replaces it.
    let renamed = store
        .update_project(p.id, ProjectChanges { name: Some("x"), ..ProjectChanges::default() })
        .unwrap();
    assert_eq!(renamed.run_commands, two);
    let one = [run("FE")];
    let replaced = store
        .update_project(
            p.id,
            ProjectChanges { run_commands: Some(&one), ..ProjectChanges::default() },
        )
        .unwrap();
    assert_eq!(open(&dir).get_project(p.id).unwrap().run_commands, one);
    assert!(replaced.run_autostart);
}

#[test]
fn the_dev_command_becomes_a_run_command_named_dev() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mkrew.db");
    let (with, without) = (Ulid::new(), Ulid::new());
    {
        let conn = Connection::open(&path).unwrap();
        for sql in &MIGRATIONS[..10] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 10).unwrap();
        conn.execute(
            "INSERT INTO project (id, name, path, dev_command, created_at, dev_autostart)
             VALUES (?1, 'a', '/a', 'npm run dev', 0, 1)",
            [with.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO project (id, name, path, created_at) VALUES (?1, 'b', '/b', 1)",
            [without.to_string()],
        )
        .unwrap();
    }
    let store = Store::open(&path).unwrap();
    let a = store.get_project(with).unwrap();
    assert_eq!(
        a.run_commands,
        [RunCommand {
            name: "dev".into(),
            dir: ".".into(),
            command: "npm run dev".into(),
            env_file: None,
            env: Vec::new(),
        }]
    );
    assert!(a.run_autostart);
    let b = store.get_project(without).unwrap();
    assert!(b.run_commands.is_empty() && !b.run_autostart);
}
```

Fix other store tests that use `add_project_with_dev` or `dev_*` fields to the new names.

Run: `cargo test -p mkrew-store`
Expected: compile errors in `lib.rs` (fields renamed) — the failing state.

- [ ] **Step 7: Store implementation** in `crates/mkrew-store/src/lib.rs`:

1. Imports: add `RunCommand` to the `mkrew_protocol::{…}` list.
2. `ProjectChanges`:

```rust
pub struct ProjectChanges<'a> {
    pub name: Option<&'a str>,
    pub default_agent: Option<Option<&'a str>>,
    /// Replaces the whole list (already checked by the caller).
    pub run_commands: Option<&'a [RunCommand]>,
    pub run_autostart: Option<bool>,
}
```

3. Delete `add_project_with_dev`; move its body into `add_project(&self, path: &Path, name: Option<&str>)` with `run_commands: Vec::new(), run_autostart: false` in the `Project` literal and the insert `"INSERT INTO project (id, name, path, created_at) VALUES (?1, ?2, ?3, ?4)"` with `params![project.id.to_string(), project.name, project.path, project.created_at]`.
4. Both SELECTs become `"SELECT id, name, path, default_agent, review_globs, created_at, archived, base_ref, run_autostart FROM project …"`. `RawProject` drops `dev_command`, renames `dev_autostart` → `run_autostart`; `from_row` reads indexes 0–8 in that order; `into_project` sets `run_commands: Vec::new(), run_autostart: self.run_autostart`.
5. Fill the commands after reading:

```rust
    /// All projects, oldest first.
    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let mut projects: Vec<Project> = {
            let mut stmt = self.conn.prepare(
                "SELECT id, name, path, default_agent, review_globs, created_at, archived,
                        base_ref, run_autostart
                 FROM project ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map([], RawProject::from_row)?;
            rows.map(|r| r?.into_project()).collect::<Result<_>>()?
        };
        for project in &mut projects {
            project.run_commands = self.run_commands(project.id)?;
        }
        Ok(projects)
    }
```

`get_project` does the same for its one project (`let mut project = row?.into_project()?; project.run_commands = self.run_commands(id)?; Ok(project)`).

```rust
    /// The project's run commands, in order.
    fn run_commands(&self, project_id: Ulid) -> Result<Vec<RunCommand>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, dir, command, env_file, env FROM run_command
             WHERE project_id = ?1 ORDER BY position",
        )?;
        let rows = stmt.query_map([project_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;
        rows.map(|row| {
            let (name, dir, command, env_file, env) = row?;
            let env = serde_json::from_str(&env).map_err(|_| corrupt("run_command.env", env))?;
            Ok(RunCommand { name, dir, command, env_file, env })
        })
        .collect()
    }
```

6. `update_project`:

```rust
    /// Applies `changes` and returns the updated project.
    pub fn update_project(&self, id: Ulid, changes: ProjectChanges<'_>) -> Result<Project> {
        let mut project = self.get_project(id)?;
        if let Some(name) = changes.name {
            project.name = name.to_owned();
        }
        if let Some(default_agent) = changes.default_agent {
            project.default_agent = default_agent.map(str::to_owned);
        }
        if let Some(run_autostart) = changes.run_autostart {
            project.run_autostart = run_autostart;
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE project SET name = ?2, default_agent = ?3, run_autostart = ?4 WHERE id = ?1",
            params![id.to_string(), project.name, project.default_agent, project.run_autostart],
        )?;
        if let Some(commands) = changes.run_commands {
            tx.execute("DELETE FROM run_command WHERE project_id = ?1", [id.to_string()])?;
            for (position, command) in commands.iter().enumerate() {
                // A Vec of string pairs always serializes.
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
```

7. Delete `non_blank` if nothing else uses it (check with `xd://lsp` references).

Run: `cargo test -p mkrew-store`
Expected: PASS (including `open_runs_migrations_and_enables_wal`, which checks `MIGRATIONS.len()`).

- [ ] **Step 8: Daemon cutover** in `crates/mkrew-daemon/src/server.rs`:

1. Delete `mod dev;`, the `dev_starting` field (+ doc comment) and its initializer, the `self.autostart_dev_servers();` call in `serve()`, and the dispatch arms for `ProjectDevStart`/`ProjectDevStop`. Delete `crates/mkrew-daemon/src/server/dev.rs`. Drop "auto-starts dev servers" from the `serve()` doc comment (Task 4 adds "auto-starts run commands").
2. `project_update`:

```rust
    /// Changes the project; a new `default_agent` must name an agent profile, run commands
    /// are checked and trimmed (SPEC §10).
    fn project_update(&self, params: ProjectUpdateParams) -> Outcome {
        if let Some(Some(id)) = &params.default_agent
            && let Err(outcome) = self.check_agent_profile(id)
        {
            return outcome;
        }
        let run_commands = match params.run_commands.map(mkrew_core::run::normalize).transpose() {
            Ok(commands) => commands,
            Err(e) => return Outcome::error(ErrorCode::InvalidRequest, e.to_string()),
        };
        let updated = self.store().update_project(
            params.project_id,
            ProjectChanges {
                name: params.name.as_deref(),
                default_agent: params.default_agent.as_ref().map(Option::as_deref),
                run_commands: run_commands.as_deref(),
                run_autostart: params.run_autostart,
            },
        );
        if let Ok(project) = &updated {
            let _ = self.hub.events.send(Event::ProjectUpdated(project.clone()));
        }
        store_outcome(updated)
    }
```

3. `project_add`: `self.store().add_project(std::path::Path::new(&params.path), params.name.as_deref())`.

4. `crates/mkrew-daemon/tests/daemon.rs`: remove `DevServer, DevStartParams` imports; every `ProjectUpdateParams`/`ProjectAddParams` literal (≈259, 485, 2736) uses `run_commands: None, run_autostart: None` / no `dev_command`; delete `dev_start`, `dev_stop`, `update_dev`, `dev_sessions` and the test `dev_server_runs_once_in_its_own_tab_stops_and_auto_starts_after_a_restart` (keep `wait_file`; Task 4 uses it). Replace `project_dev_server_settings_are_added_updated_and_cleared` with:

```rust
#[test]
fn project_run_settings_are_checked_trimmed_and_saved() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    assert!(project.run_commands.is_empty() && !project.run_autostart);
    let raw = RunCommand {
        name: " BE ".into(),
        dir: "".into(),
        command: " uv run main.py ".into(),
        env_file: Some(" ".into()),
        env: vec![("ENVIRONMENT".into(), "dev".into())],
    };
    let saved: Project = client
        .call(Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            run_commands: Some(vec![raw]),
            run_autostart: Some(true),
        }))
        .into_result()
        .unwrap();
    assert_eq!(
        saved.run_commands,
        [RunCommand {
            name: "BE".into(),
            dir: ".".into(),
            command: "uv run main.py".into(),
            env_file: None,
            env: vec![("ENVIRONMENT".into(), "dev".into())],
        }]
    );
    assert!(saved.run_autostart);
    client.wait_event(&Event::ProjectUpdated(saved.clone()));
    let bad = RunCommand {
        name: "a/b".into(),
        ..saved.run_commands[0].clone()
    };
    let refused = client.call(Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: None,
        run_commands: Some(vec![bad]),
        run_autostart: None,
    }));
    assert_eq!(error_code(refused), ErrorCode::InvalidRequest);
    client.stop();
}
```

Add `RunCommand` to the test file's `mkrew_protocol` imports.

Run: `cargo test -p mkrew-daemon project_run_settings`
Expected: PASS.

- [ ] **Step 9: CLI cutover**

`crates/mkrew-cli/src/main.rs`, `ProjectCommand`:
- `Add`: delete the `dev` arg.
- `Update`: delete `dev`, `no_dev`, `dev_autostart`; add

```rust
        /// Run the workspace's run commands after the daemon starts.
        #[arg(long, group = "change", value_enum)]
        run_autostart: Option<project::Switch>,
```

- Delete the `Dev` variant.
- Dispatch: `ProjectCommand::Add { path, name } => project::add(&mut client, &path, name)`; `Update { project, base, auto_base, run_autostart } => project::update(&mut client, &project, project::Changes { base: (base.is_some() || auto_base).then_some(base), run_autostart: run_autostart.map(|s| s == project::Switch::On) })`; delete the `Dev` arm. The comment above becomes `// clap rejects \`--base\` with \`--auto-base\`.`

`crates/mkrew-cli/src/project.rs`:
- Module doc: `//! \`mk project add | list | update\` (SPEC §9).` (Task 6 adds `run | restart | stop`.)
- Imports: drop `DevServer, DevStartParams, ProjectRef` (keep what is still used).
- `add(client, path, name)` sends `ProjectAddParams { path, name }`.
- `Switch` doc: `/// \`--run-autostart on|off\`.`
- `Changes { pub base: Option<Option<String>>, pub run_autostart: Option<bool> }`, doc `/// \`mk project update\` changes; \`None\` = unchanged, \`Some(None)\` = auto-detect base.`
- `update`: replace the dev block with

```rust
    if let Some(run_autostart) = changes.run_autostart {
        let updated: Project = client.call(Request::ProjectUpdate(ProjectUpdateParams {
            project_id: project.id,
            name: None,
            default_agent: None,
            run_commands: None,
            run_autostart: Some(run_autostart),
        }))?;
        let state = if updated.run_autostart { "on" } else { "off" };
        println!("run auto-start of {}: {state}", updated.name);
    }
```

  doc comment `/// \`mk project update <project> [--base <ref> | --auto-base] [--run-autostart on|off]\`.`
- Delete `dev()`.

`crates/mkrew-cli/tests/cli.rs`: replace `project_dev_server_is_configured_on_add_and_update` with

```rust
#[test]
fn project_run_autostart_is_switched_on_update() {
    let env = Env::new();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    let on = env.ok(&["project", "update", "demo", "--run-autostart", "on"]);
    assert_eq!(on.trim(), "run auto-start of demo: on");
    let both = env.ok(&["project", "update", "demo", "--auto-base", "--run-autostart", "off"]);
    let lines: Vec<_> = both.lines().collect();
    assert_eq!(lines, ["base of demo: auto-detect", "run auto-start of demo: off"]);
    let bad = env.mk(&["project", "update", "demo", "--run-autostart", "maybe"]);
    assert!(!bad.status.success());
    env.ok(&["daemon", "stop"]);
}
```

`README.md`: rename the heading `### Workspaces, default agent and dev server` → `### Workspaces, default agent and Run button` and replace its bullets with:

```markdown
- Add a project folder as a workspace: "Add workspace" in the sidebar (`Ctrl+Shift+K`) or
  `mk project add <path> [--name N]`.
- Default agent: in the workspace menu pick "Default agent for new panes". New agent panes and
  tasks without their own agent use it.
- Run button (right of the view tabs): the first click opens "Run setup", where each command
  gets a name, a folder (relative to the workspace), the command, an optional env file (e.g.
  `.env`) and extra `KEY=VALUE` lines. After that one click runs every command, each in its own
  pane of the "run" tab; ↻ restarts, ■ stops, ▾ controls one command. Ports the servers print
  (`http://localhost:5180`, `port 8080`) show on the button. `mk project update <project>
  --run-autostart on` runs them whenever the daemon starts.
```

`crates/mkrew-cli/tests/readme.rs`: in `readme_has_the_required_sections` the needle `"--dev"` becomes `"--run-autostart"`.

Run: `cargo test -p mkrew-cli`
Expected: PASS.

- [ ] **Step 10: UI cutover**

1. Delete `crates/mkrew-ui/src/dev_server.rs` and `pub mod dev_server;` from `lib.rs`; drop "workspace's dev server" from the `lib.rs` module doc list.
2. `crates/mkrew-ui/src/main.rs`:
   - Imports: remove `DevServer, DevStartParams` and `use mkrew_ui::dev_server::{self, DevDialog};`.
   - Fields: delete `dev_dialog` and `dev_reveal` (with their resets in the disconnect block, initializers, and `|| self.dev_dialog.is_some()` in `modal_open`).
   - Delete `start_dev_server`, `reveal_dev_tab`, `stop_dev_server`, `open_dev_dialog`, `close_dev_dialog`, `dev_dialog_key_down`, `save_dev_settings`, `dev_server`, `dev_server_item`, `render_dev_dialog`, and the root render child that renders the dev dialog.
   - `dispatch`: delete the `if let Event::TabUpdated(tab) = event && self.dev_reveal == Some(tab.id) { … }` block.
   - Workspace menu: delete `.children(self.dev_server_item(project_id, theme, cx))`, the `dev-server-settings` item and one of the two separators, so "Open in Zed" is followed by one separator and the "Default agent" header.
   - Tab bar (`render_tab_bar`): delete `dev_session`, `(dev_running, dev_command)`, the `stop-dev-server`/`start-dev-server` buttons; the tab label child becomes `.child(tab.title.clone())`.
   - Sidebar: delete `let dev_running = …;` at the `workspace_row` call; `workspace_row` loses its `dev_running` parameter and the "▶ dev" block (Task 8 adds a `run` parameter in the same place).
   - Palette context: delete `let (dev_running, dev_command) = …;` and the two fields; delete the `Extra::StartDevServer`/`StopDevServer` arms.
   - Add workspace: `ProjectAddParams { path, name: None }`.
3. `palette.rs`: delete `Context.dev_command`/`dev_running`, `Extra::StartDevServer`/`StopDevServer` (titles, `available` arms, `ALL` entries → `ALL: [Self; 9]`), and "dev server start / stop" in the module doc; tests: drop the fields from `context()` and `none`, delete the `stopped`/`running`/`both` dev assertions; the `Project` literal (≈557) uses `run_commands: Vec::new(), run_autostart: false`.
4. `workspace_menu.rs` `set_default_agent`: `run_commands: None, run_autostart: None`.
5. Test literals (`inbox.rs`, `sidebar.rs`, `tests/board_shared.rs`, `tests/connect.rs`, `tests/docs_view.rs`, `tests/lifecycle.rs`, `tests/measure.rs`, `tests/restore.rs`, `tests/stress.rs`): `Project` → `run_commands: Vec::new(), run_autostart: false`; `ProjectUpdateParams` → `run_commands: None, run_autostart: None`; `ProjectAddParams` → drop `dev_command`.

Run: `cargo build --workspace --all-targets` then `cargo test -p mkrew-ui palette`
Expected: builds; palette tests PASS.

- [ ] **Step 11: Whole-workspace check**

Run: `cargo fmt --all` then `cargo clippy --workspace --all-targets -- -D warnings` then `cargo test --workspace -j 4`
Expected: all PASS; a grep for `dev_command|dev_autostart|DevServer|DevStartParams|dev_start|dev_stop` under `crates/` finds only migration SQL files.

---

### Task 3: Extra env for spawned sessions

**Files:**
- Modify: `crates/mkrew-daemon/src/launch.rs`, `crates/mkrew-daemon/src/session.rs`, `crates/mkrew-daemon/src/server.rs`, `crates/mkrew-daemon/src/server/resume.rs`

**Interfaces:**
- Produces: `launch::program(profile: &AgentProfile) -> String` (its `command`, or the default shell for a shell profile without one); `Start.env: Vec<(String, String)>`; `SpawnSpec.env: Vec<(String, String)>`, applied after the profile's env and before `MKREW_SESSION_ID`, `MKREW_SOCKET`, `TERM`.

- [ ] **Step 1: Write the failing test** — in the `launch.rs` tests module, next to `shell_profile_without_command_uses_the_default_shell` (≈192, it uses the module's `profile(command, kind)` helper):

```rust
    #[test]
    fn the_program_of_a_shell_profile_without_command_is_the_default_shell() {
        assert_eq!(program(&profile("", ProfileKind::Shell)), default_shell());
        assert_eq!(program(&profile("claude", ProfileKind::Agent)), "claude");
    }
```

Run: `cargo test -p mkrew-daemon launch::`
Expected: FAIL (`program` not found).

- [ ] **Step 2: Implement**

`launch.rs`:

```rust
/// The program `profile` runs: its `command`, or the OS default shell for a shell profile
/// without one.
pub fn program(profile: &AgentProfile) -> String {
    if profile.command.is_empty() && profile.kind == ProfileKind::Shell {
        default_shell()
    } else {
        profile.command.clone()
    }
}
```

and in `command()` replace the `let program = if … else …;` block with `let program = program(profile);`.

`session.rs`: add to `SpawnSpec`

```rust
    /// Extra environment (run commands, SPEC §10); after the profile's, before mkrew's own.
    pub env: Vec<(String, String)>,
```

destructure it in `Hub::spawn` and apply right after `cmd.cwd(&cwd);`:

```rust
        for (key, value) in &env {
            cmd.env(key, value);
        }
```

`server.rs`: add `/// Extra environment for the session (run commands).` `env: Vec<(String, String)>,` to `struct Start`, pass `env: start.env,` into the `SpawnSpec` in `start_session`, and add `env: Vec::new(),` to the `Start` literals in `session_spawn`, `task_run` and `server/resume.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test -p mkrew-daemon launch::` and `cargo build -p mkrew-daemon`
Expected: PASS / builds.

---

### Task 4: Daemon `project.run_start` / `run_restart` / `run_stop` and autostart

**Files:**
- Create: `crates/mkrew-daemon/src/server/run.rs`
- Modify: `crates/mkrew-protocol/src/message.rs`, `crates/mkrew-core/src/run.rs`, `crates/mkrew-daemon/src/server.rs`, `crates/mkrew-daemon/tests/daemon.rs`

**Interfaces:**
- Consumes: Tasks 1–3 (`typed_line`, `session_title`, `command_name`, `layout`, `parse_env`, `TAB_TITLE`, `launch::program`, `Start.env`).
- Produces — protocol (`message.rs`):

```rust
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
```

  `Request` variants (after `ProjectUpdate`):

```rust
    /// Returns [`RunStarted`] (SPEC §10, ADR 0018).
    #[serde(rename = "project.run_start")]
    ProjectRunStart(RunStartParams),
    /// Returns [`RunStarted`]: stops the targets, waits for them, starts them again.
    #[serde(rename = "project.run_restart")]
    ProjectRunRestart(RunStartParams),
    /// Returns nothing (`null`); also when nothing runs.
    #[serde(rename = "project.run_stop")]
    ProjectRunStop(RunStopParams),
```

- Produces — `mkrew_core::run::ordered_live(commands: &[RunCommand], sessions: &[Session]) -> Vec<Ulid>`; daemon `run_starting: tokio::sync::Mutex<()>`, `run_start`/`run_restart`/`autostart_runs` (async), `run_stop`.

- [ ] **Step 1: Protocol + core helper, with tests**

Add the types and variants above. In `message.rs` tests add to `every_m1_request_round_trips`:

```rust
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
            Request::ProjectRunStop(RunStopParams { project_id: sid(), name: None }),
```

and

```rust
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
```

In `crates/mkrew-core/src/run.rs` (import `SessionState`):

```rust
/// The live run sessions of a project in tab order: configured commands in config order, then
/// the ones whose name left the config (oldest first).
pub fn ordered_live(commands: &[RunCommand], sessions: &[Session]) -> Vec<Ulid> {
    let mut live: Vec<(usize, i64, Ulid)> = sessions
        .iter()
        .filter(|s| s.state != SessionState::Exited)
        .filter_map(|s| {
            let name = command_name(s)?;
            let position = commands.iter().position(|c| c.name == name).unwrap_or(usize::MAX);
            Some((position, s.started_at, s.id))
        })
        .collect();
    live.sort_unstable();
    live.into_iter().map(|(_, _, id)| id).collect()
}
```

test:

```rust
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
```

Run: `cargo test -p mkrew-protocol` and `cargo test -p mkrew-core run::`
Expected: PASS.

- [ ] **Step 2: Write the failing daemon e2e tests** — append to `crates/mkrew-daemon/tests/daemon.rs` (import `RunCommand, RunStartParams, RunStarted, RunStopParams`):

```rust
fn run_command(name: &str, dir: &str, command: &str) -> RunCommand {
    RunCommand {
        name: name.into(),
        dir: dir.into(),
        command: command.into(),
        env_file: None,
        env: Vec::new(),
    }
}

/// A command for the test shell: cmd on Windows, sh elsewhere.
fn shell_line(windows: &str, unix: &str) -> String {
    if cfg!(windows) { windows } else { unix }.to_owned()
}

/// `windows`/`unix`, then stays alive for a minute.
fn long(windows: &str, unix: &str) -> String {
    shell_line(
        &format!("{windows} & ping -n 60 127.0.0.1 > nul"),
        &format!("{unix}; sleep 60"),
    )
}

fn set_runs(project: &Project, commands: Vec<RunCommand>, autostart: Option<bool>) -> Request {
    Request::ProjectUpdate(ProjectUpdateParams {
        project_id: project.id,
        name: None,
        default_agent: None,
        run_commands: Some(commands),
        run_autostart: autostart,
    })
}

fn run_params(project: &Project, name: Option<&str>) -> RunStartParams {
    RunStartParams {
        project_id: project.id,
        name: name.map(Into::into),
        cols: 80,
        rows: 24,
    }
}

fn run_start(project: &Project, name: Option<&str>) -> Request {
    Request::ProjectRunStart(run_params(project, name))
}

fn run_restart(project: &Project, name: Option<&str>) -> Request {
    Request::ProjectRunRestart(run_params(project, name))
}

fn run_stop(project: &Project, name: Option<&str>) -> Request {
    Request::ProjectRunStop(RunStopParams {
        project_id: project.id,
        name: name.map(Into::into),
    })
}

/// Every session of the project titled `run:…`, exited ones too.
fn run_sessions(client: &mut Client, project: &Project) -> Vec<Session> {
    let all: Vec<Session> = client
        .call(Request::SessionList(SessionListParams {
            project_id: Some(project.id),
        }))
        .into_result()
        .unwrap();
    all.into_iter()
        .filter(|s| s.title.as_deref().is_some_and(|t| t.starts_with("run:")))
        .collect()
}

/// Waits until `path` has non-blank text and returns it trimmed.
fn wait_text(path: &Path) -> String {
    let deadline = Instant::now() + TIMEOUT;
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.trim().is_empty()
        {
            return text.trim().to_owned();
        }
        assert!(Instant::now() < deadline, "{} never written", path.display());
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn pair(a: Ulid, b: Ulid) -> serde_json::Value {
    json!({"split": "vertical", "ratio": 0.5, "children": [
        {"session": a.to_string()}, {"session": b.to_string()}
    ]})
}

#[test]
fn run_commands_start_in_their_folders_with_env_side_by_side_restart_and_stop() {
    let env = Env::new();
    let repo = env.repo();
    for dir in ["api", "web"] {
        std::fs::create_dir(repo.join(dir)).unwrap();
    }
    std::fs::write(repo.join("api/.env"), "FROM_FILE=file\nBOTH=file\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let be = RunCommand {
        env_file: Some(".env".into()),
        env: vec![("BOTH".into(), "override".into())],
        ..run_command(
            "BE",
            "api",
            &long("echo %FROM_FILE% %BOTH%> env.txt", "echo $FROM_FILE $BOTH > env.txt"),
        )
    };
    let fe = run_command("FE", "web", &long("echo web> web.txt", "echo web > web.txt"));
    client
        .call(set_runs(&project, vec![be, fe], None))
        .into_result::<Project>()
        .unwrap();

    let first: RunStarted = client.call(run_start(&project, None)).into_result().unwrap();
    let started: Vec<_> = first.sessions.iter().map(|s| (s.name.as_str(), s.started)).collect();
    assert_eq!(started, [("BE", true), ("FE", true)]);
    let (be_id, fe_id) = (first.sessions[0].session_id, first.sessions[1].session_id);
    let be_session = client.session(be_id);
    assert_eq!(
        (be_session.kind, be_session.title.as_deref()),
        (SessionKind::Shell, Some("run:BE"))
    );
    // Env file first, then the overrides; each command in its own folder.
    assert_eq!(wait_text(&repo.join("api/env.txt")), "file override");
    assert_eq!(wait_text(&repo.join("web/web.txt")), "web");
    let tabs = list_tabs(&mut client, &project);
    assert_eq!(tabs.len(), 1);
    assert_eq!((tabs[0].title.as_str(), Some(tabs[0].id)), ("run", first.tab_id));
    assert_eq!(tabs[0].layout, pair(be_id, fe_id));

    // Starting again answers with the running ones.
    let again: RunStarted = client.call(run_start(&project, None)).into_result().unwrap();
    let ids: Vec<_> = again.sessions.iter().map(|s| (s.session_id, s.started)).collect();
    assert_eq!(ids, [(be_id, false), (fe_id, false)]);

    // Restarting BE replaces it in the first slot; FE keeps running.
    let restarted: RunStarted = client.call(run_restart(&project, Some("BE"))).into_result().unwrap();
    assert_eq!(restarted.sessions.len(), 1);
    let new_be = restarted.sessions[0].session_id;
    assert!(restarted.sessions[0].started);
    assert_ne!(new_be, be_id);
    assert_eq!(client.session(be_id).state, SessionState::Exited);
    assert_ne!(client.session(fe_id).state, SessionState::Exited);
    assert_eq!(list_tabs(&mut client, &project)[0].layout, pair(new_be, fe_id));

    // A command removed from the setup still stops with "stop all".
    client
        .call(set_runs(&project, Vec::new(), None))
        .into_result::<Project>()
        .unwrap();
    client.call(run_stop(&project, None)).into_result::<()>().unwrap();
    client.wait_state(new_be, SessionState::Exited);
    client.wait_state(fe_id, SessionState::Exited);
    client.stop();
}

#[test]
fn invalid_run_setups_are_refused_before_anything_starts() {
    let env = Env::new();
    let repo = env.repo();
    std::fs::write(repo.join("bad.env"), "not a pair\n").unwrap();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    assert_eq!(error_code(client.call(run_start(&project, None))), ErrorCode::InvalidState);
    client.call(run_stop(&project, None)).into_result::<()>().unwrap();

    let a = run_command("A", ".", "echo a> a.txt");
    let broken = [
        run_command("B", "missing", "echo b"),
        RunCommand { env_file: Some("nope.env".into()), ..run_command("B", ".", "echo b") },
        RunCommand { env_file: Some("bad.env".into()), ..run_command("B", ".", "echo b") },
    ];
    for b in broken {
        client
            .call(set_runs(&project, vec![a.clone(), b.clone()], None))
            .into_result::<Project>()
            .unwrap();
        assert_eq!(
            error_code(client.call(run_start(&project, None))),
            ErrorCode::InvalidRequest,
            "{b:?}"
        );
    }
    assert_eq!(
        error_code(client.call(run_start(&project, Some("Z")))),
        ErrorCode::NotFound
    );
    assert!(run_sessions(&mut client, &project).is_empty());
    assert!(!repo.join("a.txt").exists());
    client.stop();
}

#[test]
fn run_commands_auto_start_after_a_daemon_restart_and_end_with_their_command() {
    let env = Env::new();
    let repo = env.repo();
    let marker = repo.join("started.txt");
    let (mut daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    client
        .call(set_runs(
            &project,
            vec![run_command("dev", ".", "echo started> started.txt")],
            Some(true),
        ))
        .into_result::<Project>()
        .unwrap();
    client.stop();
    daemon.wait_exit();

    let (_daemon, mut client) = env.start();
    wait_file(&marker);
    let sessions = run_sessions(&mut client, &project);
    assert_eq!(sessions.len(), 1);
    // The typed line ends the shell together with the command.
    client.wait_state(sessions[0].id, SessionState::Exited);

    // Restarting an exited command starts it again.
    std::fs::remove_file(&marker).unwrap();
    let again: RunStarted = client.call(run_restart(&project, Some("dev"))).into_result().unwrap();
    assert!(again.sessions[0].started);
    wait_file(&marker);
    client.stop();
}
```

Run: `cargo test -p mkrew-daemon run_`
Expected: FAIL (requests not handled yet).

- [ ] **Step 3: Implement `server/run.rs`**

```rust
//! Run commands per workspace (SPEC §8, §10; ADR 0018): each configured command typed into its
//! own shell session titled `run:<name>`, the live ones side by side in the project's `run`
//! tab. Replaces the single dev server of ADR 0008.

use std::path::{Path, PathBuf};
use std::time::Duration;

use mkrew_core::run;
use mkrew_protocol::{
    ErrorCode, Event, Outcome, Project, RunCommand, RunSession, RunStartParams, RunStarted,
    RunStopParams, Session, SessionState,
};
use tokio::sync::broadcast::error::RecvError;
use ulid::Ulid;

use super::{Daemon, Start, ok, store_outcome};
use crate::session::Command;

/// How long `project.run_restart` waits for the stopped sessions to exit (each one is
/// force-killed after its own 5 s grace).
const RESTART_WAIT: Duration = Duration::from_secs(10);

impl Daemon {
    /// Starts the targeted commands that are not running; answers with all targets.
    pub(super) async fn run_start(&self, params: RunStartParams) -> Outcome {
        let _starting = self.run_starting.lock().await;
        self.start_runs(params)
    }

    /// Stops the targets, waits until they exited, then starts them like `run_start`.
    pub(super) async fn run_restart(&self, params: RunStartParams) -> Outcome {
        let _starting = self.run_starting.lock().await;
        let stopped = match self.stop_runs(params.project_id, params.name.as_deref()) {
            Ok(stopped) => stopped,
            Err(outcome) => return outcome,
        };
        if !self.wait_exited(&stopped, RESTART_WAIT).await {
            tracing::warn!(project_id = %params.project_id, "run commands did not stop in time");
            return Outcome::error(ErrorCode::Timeout, "the run commands did not stop within 10 s");
        }
        self.start_runs(params)
    }

    /// Asks the targets to terminate; ok when nothing runs.
    pub(super) fn run_stop(&self, params: RunStopParams) -> Outcome {
        match self.stop_runs(params.project_id, params.name.as_deref()) {
            Ok(_) => ok(&()),
            Err(outcome) => outcome,
        }
    }

    /// Runs every command of each active project with `run_autostart` (SPEC §8).
    pub(super) async fn autostart_runs(&self) {
        let projects = match self.store().list_projects() {
            Ok(projects) => projects,
            Err(e) => {
                tracing::error!("cannot auto-start run commands: {e}");
                return;
            }
        };
        let wanted = projects
            .iter()
            .filter(|p| p.run_autostart && !p.archived && !p.run_commands.is_empty());
        for project in wanted {
            let params = RunStartParams {
                project_id: project.id,
                name: None,
                cols: mkrew_protocol::DEFAULT_COLS,
                rows: mkrew_protocol::DEFAULT_ROWS,
            };
            if let Outcome::Error(e) = self.run_start(params).await {
                tracing::warn!(project_id = %project.id, "run commands not auto-started: {}", e.message);
            }
        }
    }

    /// `run_start` without the lock (`run_restart` holds it already).
    fn start_runs(&self, params: RunStartParams) -> Outcome {
        if params.cols == 0 || params.rows == 0 {
            return Outcome::error(ErrorCode::InvalidRequest, "cols and rows must be > 0");
        }
        let (project, sessions) = match self.project_sessions(params.project_id) {
            Ok(found) => found,
            Err(outcome) => return outcome,
        };
        if project.run_commands.is_empty() {
            let message = format!("project {} has no run commands", project.name);
            return Outcome::error(ErrorCode::InvalidState, message);
        }
        let targets: Vec<&RunCommand> = match &params.name {
            None => project.run_commands.iter().collect(),
            Some(name) => match project.run_commands.iter().find(|c| &c.name == name) {
                Some(command) => vec![command],
                None => {
                    let message = format!("project {} has no run command {name:?}", project.name);
                    return Outcome::error(ErrorCode::NotFound, message);
                }
            },
        };
        let mut answer = Vec::new();
        let mut to_start = Vec::new();
        for command in targets {
            match live_run(&sessions, &command.name) {
                Some(running) => answer.push(RunSession {
                    name: command.name.clone(),
                    session_id: running.id,
                    started: false,
                }),
                None => match prepare(Path::new(&project.path), command) {
                    Ok(prepared) => to_start.push((command, prepared)),
                    // Everything is checked before anything spawns.
                    Err(message) => return Outcome::error(ErrorCode::InvalidRequest, message),
                },
            }
        }
        if !to_start.is_empty() {
            let profile = match self.load_profile("shell") {
                Ok(profile) => profile,
                Err(outcome) => return outcome,
            };
            let program = crate::launch::program(&profile);
            for (command, (dir, env)) in to_start {
                let start = Start {
                    project: project.clone(),
                    profile: profile.clone(),
                    task_id: None,
                    title: Some(run::session_title(&command.name)),
                    cols: params.cols,
                    rows: params.rows,
                    typed: Some(run::typed_line(&program, &command.command)),
                    native_session_id: None,
                    dir: Some(dir),
                    banner: None,
                    env,
                };
                match self.start_session(start) {
                    Ok(session_id) => {
                        tracing::info!(project_id = %project.id, %session_id, name = %command.name, "run command started");
                        answer.push(RunSession {
                            name: command.name.clone(),
                            session_id,
                            started: true,
                        });
                    }
                    // The ones already started keep running and stay in the tab.
                    Err(outcome) => {
                        self.show_run_tab(&project);
                        return outcome;
                    }
                }
            }
        }
        answer.sort_by_key(|s| project.run_commands.iter().position(|c| c.name == s.name));
        let tab_id = self.show_run_tab(&project);
        ok(&RunStarted {
            tab_id,
            sessions: answer,
        })
    }

    /// Asks the live run sessions of the targets to terminate; returns their ids. A `name`
    /// neither configured nor live is `not_found`.
    fn stop_runs(&self, project_id: Ulid, name: Option<&str>) -> Result<Vec<Ulid>, Outcome> {
        let (project, sessions) = self.project_sessions(project_id)?;
        if let Some(name) = name
            && !project.run_commands.iter().any(|c| c.name == name)
            && live_run(&sessions, name).is_none()
        {
            let message = format!("project {} has no run command {name:?}", project.name);
            return Err(Outcome::error(ErrorCode::NotFound, message));
        }
        let ids: Vec<Ulid> = sessions
            .iter()
            .filter(|s| s.state != SessionState::Exited)
            .filter(|s| run::command_name(s).is_some_and(|n| name.is_none_or(|wanted| wanted == n)))
            .map(|s| s.id)
            .collect();
        for &id in &ids {
            tracing::info!(%project_id, session_id = %id, "stopping a run command");
            self.hub.command(id, Command::Kill { force: false });
        }
        Ok(ids)
    }

    /// Waits until every one of `ids` is `exited` in the store; `false` after `limit`. The
    /// store, not the live map: `finish` leaves the live map before it writes `exited`, so a
    /// start right after could otherwise still see the old session as running.
    async fn wait_exited(&self, ids: &[Ulid], limit: Duration) -> bool {
        // Subscribed before the first check, so an exit in between is not missed.
        let mut events = self.hub.events.subscribe();
        let deadline = tokio::time::Instant::now() + limit;
        loop {
            let exited = {
                let store = self.store();
                ids.iter().all(|&id| {
                    store
                        .get_session(id)
                        .is_ok_and(|s| s.state == SessionState::Exited)
                })
            };
            if exited {
                return true;
            }
            match tokio::time::timeout_at(deadline, events.recv()).await {
                Err(_) | Ok(Err(RecvError::Closed)) => return false,
                // Any event (or a lag) may follow an exit: check again.
                Ok(_) => {}
            }
        }
    }

    fn project_sessions(&self, project_id: Ulid) -> Result<(Project, Vec<Session>), Outcome> {
        let found = {
            let store = self.store();
            store.get_project(project_id).and_then(|project| {
                let sessions = store.list_sessions(Some(project.id))?;
                Ok((project, sessions))
            })
        };
        found.map_err(|e| store_outcome::<()>(Err(e)))
    }

    /// Lays the project's live run sessions side by side in its `run` tab (created when
    /// missing); `None` when the tab could not be written (logged).
    fn show_run_tab(&self, project: &Project) -> Option<Ulid> {
        let placed = {
            // One lock, so a concurrent `tab.set_layout` is not overwritten.
            let store = self.store();
            store.list_sessions(Some(project.id)).and_then(|sessions| {
                let layout = run::layout(&run::ordered_live(&project.run_commands, &sessions));
                let existing = store
                    .list_tabs(project.id)?
                    .into_iter()
                    .find(|t| t.title == run::TAB_TITLE);
                let tab = match existing {
                    Some(tab) => tab,
                    None => store.create_tab(project.id, run::TAB_TITLE, None)?,
                };
                store.set_tab_layout(tab.id, &layout)
            })
        };
        match placed {
            Ok(tab) => {
                let tab_id = tab.id;
                let _ = self.hub.events.send(Event::TabUpdated(tab));
                Some(tab_id)
            }
            Err(e) => {
                tracing::warn!(project_id = %project.id, "run tab not written: {e}");
                None
            }
        }
    }
}

/// The live run session of the command `name`.
fn live_run<'a>(sessions: &'a [Session], name: &str) -> Option<&'a Session> {
    sessions
        .iter()
        .find(|s| s.state != SessionState::Exited && run::command_name(s) == Some(name))
}

/// The folder and extra environment of `command`, or why it cannot start.
fn prepare(project_dir: &Path, command: &RunCommand) -> Result<(PathBuf, Vec<(String, String)>), String> {
    let dir = project_dir.join(&command.dir);
    if !dir.is_dir() {
        return Err(format!("{}: folder {} not found", command.name, dir.display()));
    }
    let mut env = Vec::new();
    if let Some(file) = &command.env_file {
        let path = dir.join(file);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("{}: cannot read {}: {e}", command.name, path.display()))?;
        env = run::parse_env(&text)
            .map_err(|e| format!("{}: {}: {e}", command.name, path.display()))?;
    }
    env.extend(command.env.iter().cloned());
    Ok((dir, env))
}
```

In `server.rs`:
- `mod run;` next to the other `mod` lines.
- `Daemon` field `/// Serializes run starts and restarts, so a command never runs twice.` `run_starting: tokio::sync::Mutex<()>,`, initialised `run_starting: tokio::sync::Mutex::new(()),`.
- `serve()`: after `self.resume_sessions().await;` add `self.autostart_runs().await;`; its doc comment says "…resumes interrupted tasks and agent panes, auto-starts run commands, then serves…".
- Dispatch after `ProjectUpdate`:

```rust
            Request::ProjectRunStart(params) => (self.run_start(params).await, After::Continue),
            Request::ProjectRunRestart(params) => (self.run_restart(params).await, After::Continue),
            Request::ProjectRunStop(params) => (self.run_stop(params), After::Continue),
```

If `Option::is_none_or` is unavailable on the installed stable toolchain, use `name.map_or(true, |wanted| wanted == n)`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-daemon run_` then `cargo test -p mkrew-daemon`
Expected: the three new tests PASS; whole crate PASS.

- [ ] **Step 5: Clippy**

Run: `cargo clippy -p mkrew-daemon --all-targets -- -D warnings`
Expected: clean.

---

### Task 5: Ports of run sessions

**Files:**
- Modify: `crates/mkrew-protocol/src/message.rs`, `crates/mkrew-daemon/src/session.rs`, `crates/mkrew-daemon/src/server.rs`, `crates/mkrew-daemon/tests/daemon.rs`, `crates/mkrew-ui/src/sidebar.rs` (+ any other exhaustive `match` on `Event` the compiler names)

**Interfaces:**
- Produces:

```rust
/// A run session's ports (SPEC §10): every port it printed so far, at most 4, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPortsEvent {
    pub session_id: Ulid,
    pub ports: Vec<u16>,
}
```

  `Event::SessionPorts(SessionPortsEvent)` (`"session.ports"`); `Request::SessionPorts(ProjectRef)` (`"session.ports"`) → `Vec<SessionPortsEvent>` of the project's live sessions with ports; `Hub::session_ports(&self, id: Ulid) -> Option<Vec<u16>>`.

- [ ] **Step 1: Failing e2e test** (import `SessionPortsEvent`):

```rust
#[test]
fn run_sessions_report_the_ports_they_print() {
    let env = Env::new();
    let repo = env.repo();
    let (_daemon, mut client) = env.start();
    let project = client.add_project(&repo);
    let web = run_command(
        "web",
        ".",
        &long("echo Local: http://localhost:5173/", "echo 'Local: http://localhost:5173/'"),
    );
    client
        .call(set_runs(&project, vec![web], None))
        .into_result::<Project>()
        .unwrap();
    let started: RunStarted = client.call(run_start(&project, None)).into_result().unwrap();
    let id = started.sessions[0].session_id;
    client.wait_event_where(
        |e| matches!(e, Event::SessionPorts(p) if p.session_id == id && p.ports == [5173]),
    );
    let ports = |client: &mut Client| -> Vec<SessionPortsEvent> {
        client
            .call(Request::SessionPorts(ProjectRef { project_id: project.id }))
            .into_result()
            .unwrap()
    };
    assert_eq!(
        ports(&mut client),
        [SessionPortsEvent { session_id: id, ports: vec![5173] }]
    );
    client.call(run_stop(&project, None)).into_result::<()>().unwrap();
    client.wait_state(id, SessionState::Exited);
    assert!(ports(&mut client).is_empty());
    client.stop();
}
```

Run: `cargo test -p mkrew-daemon run_sessions_report_the_ports`
Expected: compile error (types missing).

- [ ] **Step 2: Protocol** — add `SessionPortsEvent`; `Event` variant after `SessionUsage`:

```rust
    /// A run session printed a new port (SPEC §10).
    #[serde(rename = "session.ports")]
    SessionPorts(SessionPortsEvent),
```

`Request` variant after `SessionUsage(ProjectRef)`:

```rust
    /// Returns `SessionPortsEvent[]`: the ports of the project's live run sessions (SPEC §10).
    #[serde(rename = "session.ports")]
    SessionPorts(ProjectRef),
```

Tests: add `Request::SessionPorts(ProjectRef { project_id: sid() }),` to `every_m1_request_round_trips` and `round_trip(&Message::Event(Event::SessionPorts(SessionPortsEvent { session_id: sid(), ports: vec![8080, 5180] })));` to `event_wire_shape_matches_spec`.

- [ ] **Step 3: Daemon**

`session.rs` (import `SessionPortsEvent`):
- `Hub` field `/// Ports printed by live run sessions (SPEC §10); memory only.` `pub ports: Mutex<HashMap<Ulid, Vec<u16>>>,`, initialised `ports: Mutex::default(),`.
- Method:

```rust
    /// The ports `id` printed, if it is a live run session that printed some.
    pub fn session_ports(&self, id: Ulid) -> Option<Vec<u16>> {
        lock(&self.ports).get(&id).cloned()
    }
```

- `Actor` field `/// \`Some\` for a run session: the ports it printed so far.` `ports: Option<Vec<u16>>,`. In `Hub::spawn`, before `session` moves into the actor: `let run_ports = mkrew_core::run::command_name(&session).is_some().then(Vec::new);` and `ports: run_ports,` in the `Actor` literal.
- In `on_output`, inside `if self.screen_print.replace(print) != Some(print) {`, after `let lines: Vec<&str> = …;` add `self.scan_ports(&lines);`.
- Method:

```rust
    /// Adds the ports a run session's screen shows (SPEC §10); announces a grown list.
    fn scan_ports(&mut self, lines: &[&str]) {
        let Some(ports) = self.ports.as_mut() else {
            return;
        };
        let before = ports.len();
        for port in lines.iter().flat_map(|line| mkrew_core::run::scan_ports(line)) {
            if ports.len() < mkrew_core::run::MAX_PORTS && !ports.contains(&port) {
                ports.push(port);
            }
        }
        if ports.len() == before {
            return;
        }
        let ports = ports.clone();
        lock(&self.hub.ports).insert(self.session.id, ports.clone());
        let _ = self.hub.events.send(Event::SessionPorts(SessionPortsEvent {
            session_id: self.session.id,
            ports,
        }));
    }
```

- `finish`: right after the log flush, `lock(&self.hub.ports).remove(&self.session.id);` (before `set_state(Exited)`, so `session.ports` never lists an exited session).

`server.rs`: dispatch `Request::SessionPorts(params) => (self.session_ports(params.project_id), After::Continue),` and

```rust
    /// `session.ports`: the ports of the project's live run sessions that printed some.
    fn session_ports(&self, project_id: Ulid) -> Outcome {
        let sessions = match self.store().list_sessions(Some(project_id)) {
            Ok(sessions) => sessions,
            Err(e) => return store_outcome::<()>(Err(e)),
        };
        let ports: Vec<SessionPortsEvent> = sessions
            .iter()
            .filter_map(|s| {
                Some(SessionPortsEvent {
                    session_id: s.id,
                    ports: self.hub.session_ports(s.id)?,
                })
            })
            .collect();
        ok(&ports)
    }
```

UI compile fix only: `crates/mkrew-ui/src/sidebar.rs` `apply` adds `| Event::SessionPorts(_)` to the `Change::None` arm; same for any other exhaustive `match` on `Event` the compiler reports.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mkrew-protocol`, `cargo test -p mkrew-daemon run_sessions_report_the_ports`, `cargo build --workspace --all-targets`
Expected: PASS / builds.

---

### Task 6: CLI `mk project run | restart | stop`

**Files:**
- Modify: `crates/mkrew-cli/src/main.rs`, `crates/mkrew-cli/src/project.rs`, `crates/mkrew-cli/tests/cli.rs`, `README.md`

**Interfaces:**
- Consumes: `RunStartParams`, `RunStopParams`, `RunStarted`, `DEFAULT_COLS`, `DEFAULT_ROWS`.
- Produces: `project::run(client, project: &str, name: Option<String>, restart: bool)`, `project::stop(client, project: &str, name: Option<String>)`. Run/restart print one line per command `<name> <session_id>` (+ ` (already running)`); stop prints `stopped <name> of <project>` or `stopped the run commands of <project>`.

- [ ] **Step 1: Failing CLI test** — append to `crates/mkrew-cli/tests/cli.rs`:

```rust
#[test]
fn project_run_restart_and_stop_reach_the_daemon() {
    let env = Env::new();
    let repo = env.dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    env.ok(&["daemon", "start"]);
    env.ok(&["project", "add", repo.to_str().unwrap(), "--name", "demo"]);
    // Commands are set up in the UI; without any, run and restart fail and say why.
    for verb in ["run", "restart"] {
        let refused = env.mk(&["project", verb, "demo"]);
        assert!(!refused.status.success());
        assert!(
            String::from_utf8_lossy(&refused.stderr).contains("no run commands"),
            "{verb}"
        );
    }
    let unknown = env.mk(&["project", "stop", "demo", "BE"]);
    assert!(!unknown.status.success());
    let stopped = env.ok(&["project", "stop", "demo"]);
    assert_eq!(stopped.trim(), "stopped the run commands of demo");
    env.ok(&["daemon", "stop"]);
}
```

Run: `cargo test -p mkrew-cli project_run_restart_and_stop`
Expected: FAIL (unknown subcommand).

- [ ] **Step 2: Implement**

`main.rs` `ProjectCommand`, after `Update`:

```rust
    /// Run the workspace's run commands (all, or one by name); prints `<name> <session id>`.
    Run {
        /// Project id or name.
        project: String,
        /// One command's name (default: all).
        name: Option<String>,
    },
    /// Stop, then run again the workspace's run commands (all, or one by name).
    Restart {
        /// Project id or name.
        project: String,
        /// One command's name (default: all).
        name: Option<String>,
    },
    /// Stop the workspace's run commands (all, or one by name).
    Stop {
        /// Project id or name.
        project: String,
        /// One command's name (default: all).
        name: Option<String>,
    },
```

Dispatch:

```rust
                ProjectCommand::Run { project, name } => project::run(&mut client, &project, name, false),
                ProjectCommand::Restart { project, name } => project::run(&mut client, &project, name, true),
                ProjectCommand::Stop { project, name } => project::stop(&mut client, &project, name),
```

`project.rs` (module doc `//! \`mk project add | list | update | run | restart | stop\` (SPEC §9).`; import `RunStartParams, RunStarted, RunStopParams, DEFAULT_COLS, DEFAULT_ROWS`):

```rust
/// `mk project run|restart <project> [name]`: prints `<name> <session id>` per command.
pub fn run(client: &mut Client, project: &str, name: Option<String>, restart: bool) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let project = resolve_project(&projects, project)?;
    let params = RunStartParams {
        project_id: project.id,
        name,
        cols: DEFAULT_COLS,
        rows: DEFAULT_ROWS,
    };
    let request = if restart {
        Request::ProjectRunRestart(params)
    } else {
        Request::ProjectRunStart(params)
    };
    let started: RunStarted = client.call(request)?;
    for session in started.sessions {
        let note = if session.started { "" } else { " (already running)" };
        println!("{} {}{note}", session.name, session.session_id);
    }
    Ok(())
}

/// `mk project stop <project> [name]`.
pub fn stop(client: &mut Client, project: &str, name: Option<String>) -> anyhow::Result<()> {
    let projects: Vec<Project> = client.call(Request::ProjectList)?;
    let project = resolve_project(&projects, project)?;
    let () = client.call(Request::ProjectRunStop(RunStopParams {
        project_id: project.id,
        name: name.clone(),
    }))?;
    match name {
        Some(name) => println!("stopped {name} of {}", project.name),
        None => println!("stopped the run commands of {}", project.name),
    }
    Ok(())
}
```

`README.md`: append to the Run button bullet:

```markdown
  From a terminal: `mk project run <project> [name]`, `mk project restart <project> [name]`,
  `mk project stop <project> [name]`.
```

- [ ] **Step 3: Run tests**

Run: `cargo test -p mkrew-cli`
Expected: PASS (including `readme_mk_commands_exist`, which runs `mk project run --help` etc.).

---

### Task 7: UI pure models — run summary, ports, Run setup dialog

**Files:**
- Create: `crates/mkrew-ui/src/run.rs`, `crates/mkrew-ui/src/run_setup.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (`pub mod run;`, `pub mod run_setup;`, "the Run button" in the module doc list)

**Interfaces:**
- Consumes: `mkrew_core::run::{command_name, parse_env, normalize, MAX_PORTS}`, `task_dialog::DialogKey`, `text_box::TextBox`.
- Produces (`mkrew_ui::run`): `RunAction { Start, Restart, Stop }`; `Ports { to_request, request_failed, apply, load, get }`; `RowState { Stopped, Running(SessionState), Exited(Option<i32>) }`; `Row { name, state, session_id: Option<Ulid>, ports: Vec<u16> }`; `Summary { rows: Vec<Row>, stray: usize }` with `live()`, `any_live()`, `warning()`, `ports()`, `label()`, `tooltip()`; `summary(project, sessions, ports) -> Summary`; `state_text(&RowState) -> String`; `pane_title(session, ports) -> Option<String>`.
- Produces (`mkrew_ui::run_setup`): `Card`, `Field`, `Focus`, `Outcome`, `RunSetup { project_id, cards, autostart, focus, busy, error, run_after }` with `new(project, run_after)`, `key(DialogKey) -> Outcome`, `insert(&str)`, `add_card()`, `remove_card(usize)`, `begin() -> Option<ProjectUpdateParams>`, `failed(String)`.

- [ ] **Step 1: Failing tests** — `crates/mkrew-ui/src/run.rs`:

```rust
#[cfg(test)]
mod tests {
    use mkrew_protocol::{Project, RunCommand, SessionExitedEvent, SessionKind, StateSource};

    use super::*;

    fn project(names: &[&str]) -> Project {
        Project {
            id: Ulid::new(),
            name: "EA".into(),
            path: "/ea".into(),
            run_commands: names
                .iter()
                .map(|n| RunCommand {
                    name: (*n).into(),
                    dir: ".".into(),
                    command: "x".into(),
                    env_file: None,
                    env: Vec::new(),
                })
                .collect(),
            run_autostart: false,
            default_agent: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
        }
    }

    fn run(project: &Project, name: &str, state: SessionState, started_at: i64) -> Session {
        Session {
            id: Ulid::new(),
            task_id: None,
            project_id: project.id,
            agent_profile: "shell".into(),
            kind: SessionKind::Shell,
            title: Some(format!("run:{name}")),
            native_session_id: None,
            state,
            state_source: StateSource::Screen,
            state_changed_at: 0,
            exit_code: (state == SessionState::Exited).then_some(1),
            cols: 80,
            rows: 24,
            started_at,
            ended_at: None,
        }
    }

    fn ports_of(entries: &[(Ulid, &[u16])]) -> Ports {
        let mut ports = Ports::default();
        ports.load(
            entries
                .iter()
                .map(|(id, p)| SessionPortsEvent { session_id: *id, ports: p.to_vec() })
                .collect(),
        );
        ports
    }

    #[test]
    fn nothing_running_offers_run() {
        let p = project(&["BE", "FE"]);
        let s = summary(&p, &[], &Ports::default());
        assert_eq!(s.label(), "▶ Run");
        assert!(!s.any_live() && !s.warning());
        assert_eq!(state_text(&s.rows[0].state), "○ stopped");
        assert_eq!(summary(&project(&[]), &[], &Ports::default()).label(), "▶ Run");
    }

    #[test]
    fn running_commands_show_count_and_ports_in_config_order() {
        let p = project(&["BE", "FE"]);
        let fe = run(&p, "FE", SessionState::Working, 2);
        let be = run(&p, "BE", SessionState::Idle, 1);
        let old_be = run(&p, "BE", SessionState::Exited, 0);
        let ports = ports_of(&[(fe.id, &[5180]), (be.id, &[8080, 5180])]);
        let s = summary(&p, &[fe, old_be, be.clone()], &ports);
        assert_eq!(s.label(), "● Running 2/2 · :8080 :5180");
        assert!(!s.warning());
        assert_eq!(s.rows[0].session_id, Some(be.id));
        assert_eq!(s.tooltip(), "BE — :8080 :5180\nFE — :5180");
    }

    #[test]
    fn an_exited_command_while_others_run_is_a_warning() {
        let p = project(&["BE", "FE"]);
        let fe = run(&p, "FE", SessionState::Working, 2);
        let be = run(&p, "BE", SessionState::Exited, 1);
        let s = summary(&p, &[fe, be], &Ports::default());
        assert_eq!(s.label(), "✕ Running 1/2");
        assert!(s.warning());
        assert_eq!(state_text(&s.rows[0].state), "✕ exited 1");
        assert_eq!(s.tooltip(), "BE — exited 1\nFE — no port yet");
    }

    #[test]
    fn more_than_four_ports_are_counted() {
        let p = project(&["A"]);
        let a = run(&p, "A", SessionState::Working, 0);
        let ports = ports_of(&[(a.id, &[1, 2, 3, 4])]);
        let mut s = summary(&p, &[a], &ports);
        s.rows[0].ports.push(5);
        assert_eq!(s.label(), "● Running 1/1 · :1 :2 :3 :4 +1");
    }

    #[test]
    fn a_live_run_whose_command_was_removed_still_counts_as_running() {
        let p = project(&["BE"]);
        let gone = run(&p, "old", SessionState::Working, 0);
        let s = summary(&p, &[gone], &Ports::default());
        assert_eq!(s.stray, 1);
        assert!(s.any_live() && s.warning());
        assert_eq!(s.label(), "✕ Running 0/1");
    }

    #[test]
    fn ports_follow_events_and_drop_on_exit() {
        let id = Ulid::new();
        let mut ports = Ports::default();
        let event = Event::SessionPorts(SessionPortsEvent { session_id: id, ports: vec![8080] });
        assert!(ports.apply(&event));
        assert!(!ports.apply(&event));
        assert_eq!(ports.get(id), [8080]);
        let exited = Event::SessionExited(SessionExitedEvent { session_id: id, exit_code: None });
        assert!(ports.apply(&exited));
        assert!(ports.get(id).is_empty());
        let project_id = Ulid::new();
        assert_eq!(ports.to_request([project_id]), [project_id]);
        assert!(ports.to_request([project_id]).is_empty());
        ports.request_failed(project_id);
        assert_eq!(ports.to_request([project_id]), [project_id]);
    }

    #[test]
    fn the_pane_title_of_a_run_session_is_its_name_and_ports() {
        let p = project(&["BE"]);
        let be = run(&p, "BE", SessionState::Working, 0);
        assert_eq!(pane_title(&be, &[8080]).as_deref(), Some("BE · :8080"));
        assert_eq!(pane_title(&be, &[]).as_deref(), Some("BE"));
        let exited = run(&p, "BE", SessionState::Exited, 0);
        assert_eq!(pane_title(&exited, &[8080]).as_deref(), Some("BE"));
        let shell = Session { title: Some("notes".into()), ..be };
        assert_eq!(pane_title(&shell, &[8080]).as_deref(), Some("notes"));
    }
}
```

`crates/mkrew-ui/src/run_setup.rs`:

```rust
#[cfg(test)]
mod tests {
    use mkrew_protocol::{Project, RunCommand};

    use super::*;

    fn project(commands: Vec<RunCommand>) -> Project {
        Project {
            id: Ulid::new(),
            name: "EA".into(),
            path: "/ea".into(),
            run_commands: commands,
            run_autostart: true,
            default_agent: None,
            review_globs: None,
            created_at: 0,
            archived: false,
            base_ref: None,
        }
    }

    fn be() -> RunCommand {
        RunCommand {
            name: "BE".into(),
            dir: "EA-BE".into(),
            command: r".venv\Scripts\python.exe main.py".into(),
            env_file: Some(".env".into()),
            env: vec![("ENVIRONMENT".into(), "dev".into()), ("PYTHONPATH".into(), ".".into())],
        }
    }

    #[test]
    fn opens_with_the_saved_commands_and_saves_them_back() {
        let p = project(vec![be()]);
        let mut setup = RunSetup::new(&p, false);
        assert_eq!(setup.cards.len(), 1);
        assert_eq!(setup.cards[0].env.text(), "ENVIRONMENT=dev\nPYTHONPATH=.");
        assert!(setup.autostart);
        let params = setup.begin().unwrap();
        assert_eq!(params.project_id, p.id);
        assert_eq!(params.run_commands, Some(vec![be()]));
        assert_eq!(params.run_autostart, Some(true));
        assert!(setup.busy && setup.begin().is_none());
        setup.failed("daemon said no".into());
        assert!(!setup.busy);
        assert_eq!(setup.error.as_deref(), Some("daemon said no"));
    }

    #[test]
    fn a_new_setup_starts_with_one_empty_card_in_the_project_folder() {
        let mut setup = RunSetup::new(&project(Vec::new()), true);
        assert!(setup.run_after);
        assert_eq!(setup.cards.len(), 1);
        assert_eq!(setup.cards[0].dir, ".");
        assert_eq!(setup.focus, Focus::Field(0, Field::Name));
        setup.insert("FE");
        setup.key(DialogKey::SwitchField);
        setup.key(DialogKey::SwitchField);
        setup.insert("npm run dev\r\n");
        let params = setup.begin().unwrap();
        assert_eq!(
            params.run_commands,
            Some(vec![RunCommand {
                name: "FE".into(),
                dir: ".".into(),
                command: "npm run dev".into(),
                env_file: None,
                env: Vec::new(),
            }])
        );
    }

    #[test]
    fn tab_walks_every_field_then_autostart_then_wraps() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        assert_eq!(setup.focus, Focus::Field(1, Field::Name));
        setup.focus = Focus::Field(0, Field::Name);
        let mut seen = vec![setup.focus];
        for _ in 0..11 {
            setup.key(DialogKey::SwitchField);
            seen.push(setup.focus);
        }
        assert_eq!(seen[4], Focus::Field(0, Field::Env));
        assert_eq!(seen[5], Focus::Field(1, Field::Name));
        assert_eq!(seen[10], Focus::Autostart);
        assert_eq!(seen[11], Focus::Field(0, Field::Name));
        setup.focus = Focus::Autostart;
        setup.insert(" ");
        assert!(!setup.autostart);
    }

    #[test]
    fn enter_in_env_adds_a_line_and_bad_env_blocks_saving() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.focus = Focus::Field(0, Field::Env);
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Edited);
        setup.insert("not a pair");
        assert!(setup.cards[0].env.text().ends_with("\nnot a pair"));
        assert_eq!(setup.begin(), None);
        assert_eq!(setup.error.as_deref(), Some("BE: env: line 3: expected KEY=VALUE"));
        assert!(!setup.busy);
        setup.focus = Focus::Field(0, Field::Command);
        assert_eq!(setup.key(DialogKey::Enter), Outcome::Save);
        assert_eq!(setup.key(DialogKey::Save), Outcome::Save);
        assert_eq!(setup.key(DialogKey::Cancel), Outcome::Cancel);
    }

    #[test]
    fn invalid_commands_block_saving_with_the_reason() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        setup.insert("BE");
        assert_eq!(setup.begin(), None);
        assert_eq!(setup.error.as_deref(), Some("two run commands are named \"BE\""));
    }

    #[test]
    fn removing_cards_moves_focus_and_allows_an_empty_list() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.add_card();
        setup.remove_card(1);
        assert_eq!(setup.focus, Focus::Field(0, Field::Name));
        setup.remove_card(0);
        assert!(setup.cards.is_empty());
        assert_eq!(setup.focus, Focus::Autostart);
        assert_eq!(setup.begin().unwrap().run_commands, Some(Vec::new()));
    }

    #[test]
    fn backspace_edits_the_focused_field_only() {
        let mut setup = RunSetup::new(&project(vec![be()]), false);
        setup.focus = Focus::Field(0, Field::Dir);
        setup.key(DialogKey::Backspace);
        assert_eq!(setup.cards[0].dir, "EA-B");
        assert_eq!(setup.cards[0].name, "BE");
    }
}
```

The duplicate test's second card has an empty command; `normalize` checks names first, so the duplicate is reported. If `ProjectUpdateParams` lacks `PartialEq` for `assert_eq!(setup.begin(), None)`, it derives it (`#[derive(Debug, Clone, PartialEq, Eq, …)]`).

Run: `cargo test -p mkrew-ui run`
Expected: compile errors (items missing).

- [ ] **Step 2: Implement `run.rs`** (above the tests):

```rust
//! The Run button (SPEC §10, ADR 0018): per-command state from the session list, the ports
//! each run session printed, and the button/menu/pane texts. Pure; `main.rs` renders it.

use std::collections::{HashMap, HashSet};

use mkrew_core::run::{MAX_PORTS, command_name};
use mkrew_protocol::{Event, Project, Session, SessionPortsEvent, SessionState};
use ulid::Ulid;

/// What a Run control asks the daemon to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunAction {
    Start,
    Restart,
    Stop,
}

/// Ports of live run sessions: loaded per workspace with `session.ports`, then kept live by
/// `session.ports` events; dropped when the session exits.
#[derive(Debug, Default)]
pub struct Ports {
    by_session: HashMap<Ulid, Vec<u16>>,
    requested: HashSet<Ulid>,
}

impl Ports {
    /// The workspaces among `project_ids` not requested yet, marked requested.
    pub fn to_request(&mut self, project_ids: impl IntoIterator<Item = Ulid>) -> Vec<Ulid> {
        project_ids
            .into_iter()
            .filter(|id| self.requested.insert(*id))
            .collect()
    }

    /// `project_id`'s `session.ports` failed: asked again next time.
    pub fn request_failed(&mut self, project_id: Ulid) {
        self.requested.remove(&project_id);
    }

    /// Keeps a `session.ports` event, drops an exited session; whether anything changed.
    pub fn apply(&mut self, event: &Event) -> bool {
        match event {
            Event::SessionPorts(ports) => self.keep(ports.clone()),
            Event::SessionExited(exited) => self.by_session.remove(&exited.session_id).is_some(),
            _ => false,
        }
    }

    /// Keeps one workspace's `session.ports` answer; whether anything changed.
    pub fn load(&mut self, listed: Vec<SessionPortsEvent>) -> bool {
        let mut changed = false;
        for ports in listed {
            changed |= self.keep(ports);
        }
        changed
    }

    /// A session's list only grows, so the longer one wins (an answer read before an event
    /// handled first does not undo it).
    fn keep(&mut self, ports: SessionPortsEvent) -> bool {
        match self.by_session.get(&ports.session_id) {
            Some(known) if known.len() >= ports.ports.len() => false,
            _ => {
                self.by_session.insert(ports.session_id, ports.ports);
                true
            }
        }
    }

    pub fn get(&self, session_id: Ulid) -> &[u16] {
        self.by_session.get(&session_id).map_or(&[], Vec::as_slice)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowState {
    /// Never started (or its sessions are gone).
    Stopped,
    Running(SessionState),
    Exited(Option<i32>),
}

/// One configured command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub name: String,
    pub state: RowState,
    /// Its live session, else its newest one.
    pub session_id: Option<Ulid>,
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    /// The configured commands in order.
    pub rows: Vec<Row>,
    /// Live run sessions whose command left the config.
    pub stray: usize,
}

impl Summary {
    /// Configured commands running now.
    pub fn live(&self) -> usize {
        self.rows
            .iter()
            .filter(|row| matches!(row.state, RowState::Running(_)))
            .count()
    }

    /// Anything to restart or stop.
    pub fn any_live(&self) -> bool {
        self.live() > 0 || self.stray > 0
    }

    /// Something runs but not every configured command.
    pub fn warning(&self) -> bool {
        self.any_live() && self.live() < self.rows.len()
    }

    /// Distinct ports of the running commands, in config order.
    pub fn ports(&self) -> Vec<u16> {
        let mut ports = Vec::new();
        for port in self.rows.iter().flat_map(|row| &row.ports) {
            if !ports.contains(port) {
                ports.push(*port);
            }
        }
        ports
    }

    /// The button: `▶ Run`, else `● Running 2/2 · :8080 :5180` (`✕` when one is down).
    pub fn label(&self) -> String {
        if !self.any_live() {
            return "▶ Run".to_owned();
        }
        let icon = if self.warning() { "✕" } else { "●" };
        let mut label = format!("{icon} Running {}/{}", self.live(), self.rows.len());
        let ports = self.ports();
        if !ports.is_empty() {
            let shown: Vec<String> = ports.iter().take(MAX_PORTS).map(|p| format!(":{p}")).collect();
            label.push_str(" · ");
            label.push_str(&shown.join(" "));
            if ports.len() > MAX_PORTS {
                label.push_str(&format!(" +{}", ports.len() - MAX_PORTS));
            }
        }
        label
    }

    /// One line per command: `BE — :8080`, `FE — no port yet`, `API — stopped`.
    pub fn tooltip(&self) -> String {
        self.rows
            .iter()
            .map(|row| {
                let what = match &row.state {
                    RowState::Running(_) if row.ports.is_empty() => "no port yet".to_owned(),
                    RowState::Running(_) => {
                        row.ports.iter().map(|p| format!(":{p}")).collect::<Vec<_>>().join(" ")
                    }
                    other => state_text(other).trim_start_matches(['○', '✕', ' ']).to_owned(),
                };
                format!("{} — {what}", row.name)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The state of `project`'s run commands.
pub fn summary(project: &Project, sessions: &[Session], ports: &Ports) -> Summary {
    let runs: Vec<(&str, &Session)> = sessions
        .iter()
        .filter(|s| s.project_id == project.id)
        .filter_map(|s| Some((command_name(s)?, s)))
        .collect();
    let rows = project
        .run_commands
        .iter()
        .map(|command| {
            let mine = || runs.iter().filter(|(name, _)| *name == command.name).map(|(_, s)| *s);
            let live = mine().find(|s| s.state != SessionState::Exited);
            let newest = mine().max_by_key(|s| (s.started_at, s.id));
            let name = command.name.clone();
            match (live, newest) {
                (Some(s), _) => Row {
                    name,
                    state: RowState::Running(s.state),
                    session_id: Some(s.id),
                    ports: ports.get(s.id).to_vec(),
                },
                (None, Some(s)) => Row {
                    name,
                    state: RowState::Exited(s.exit_code),
                    session_id: Some(s.id),
                    ports: Vec::new(),
                },
                (None, None) => Row {
                    name,
                    state: RowState::Stopped,
                    session_id: None,
                    ports: Vec::new(),
                },
            }
        })
        .collect();
    let stray = runs
        .iter()
        .filter(|(name, s)| {
            s.state != SessionState::Exited && !project.run_commands.iter().any(|c| c.name == *name)
        })
        .count();
    Summary { rows, stray }
}

/// Icon + text of a command's state (SPEC §11: never color alone).
pub fn state_text(state: &RowState) -> String {
    match state {
        RowState::Stopped => "○ stopped".to_owned(),
        RowState::Running(_) => "● running".to_owned(),
        RowState::Exited(Some(code)) => format!("✕ exited {code}"),
        RowState::Exited(None) => "✕ exited".to_owned(),
    }
}

/// The pane header title: a live run session shows `name · :port …`, an exited one its name,
/// any other session its own title.
pub fn pane_title(session: &Session, ports: &[u16]) -> Option<String> {
    let Some(name) = command_name(session) else {
        return session.title.clone();
    };
    if session.state == SessionState::Exited || ports.is_empty() {
        return Some(name.to_owned());
    }
    let ports: Vec<String> = ports.iter().map(|p| format!(":{p}")).collect();
    Some(format!("{name} · {}", ports.join(" ")))
}
```

- [ ] **Step 3: Implement `run_setup.rs`** (above the tests):

```rust
//! The "Run setup" dialog (SPEC §10, ADR 0018): one card per run command (name, folder,
//! command, env file, env lines) and "Start with the daemon"; saving sends `project.update`.
//! Pure; `main.rs` renders it.

use mkrew_core::run::{normalize, parse_env};
use mkrew_protocol::{Project, ProjectUpdateParams, RunCommand};
use ulid::Ulid;

use crate::task_dialog::DialogKey;
use crate::text_box::TextBox;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Card {
    pub name: String,
    pub dir: String,
    pub command: String,
    pub env_file: String,
    /// `KEY=VALUE` lines.
    pub env: TextBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Dir,
    Command,
    EnvFile,
    Env,
}

impl Field {
    const ORDER: [Field; 5] = [Field::Name, Field::Dir, Field::Command, Field::EnvFile, Field::Env];
}

/// Where typing goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Field(usize, Field),
    Autostart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Edited,
    Cancel,
    Save,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSetup {
    pub project_id: Ulid,
    pub cards: Vec<Card>,
    pub autostart: bool,
    pub focus: Focus,
    pub busy: bool,
    pub error: Option<String>,
    /// Opened from ▶ Run with no command: run everything after a successful save.
    pub run_after: bool,
}

impl RunSetup {
    pub fn new(project: &Project, run_after: bool) -> RunSetup {
        let mut cards: Vec<Card> = project.run_commands.iter().map(card).collect();
        if cards.is_empty() {
            cards.push(empty_card());
        }
        RunSetup {
            project_id: project.id,
            cards,
            autostart: project.run_autostart,
            focus: Focus::Field(0, Field::Name),
            busy: false,
            error: None,
            run_after,
        }
    }

    pub fn key(&mut self, key: DialogKey) -> Outcome {
        match key {
            DialogKey::Cancel => Outcome::Cancel,
            DialogKey::Save => Outcome::Save,
            DialogKey::Enter => match self.focus {
                Focus::Field(i, Field::Env) => {
                    self.cards[i].env.key(DialogKey::Enter);
                    Outcome::Edited
                }
                _ => Outcome::Save,
            },
            DialogKey::SwitchField => {
                self.focus = self.next_focus();
                Outcome::Edited
            }
            DialogKey::Backspace => {
                match self.focus {
                    Focus::Field(i, Field::Env) => {
                        self.cards[i].env.key(DialogKey::Backspace);
                    }
                    Focus::Field(i, field) => {
                        line(&mut self.cards[i], field).pop();
                    }
                    Focus::Autostart => {}
                }
                Outcome::Edited
            }
            // The caller reads the clipboard and calls `insert`.
            DialogKey::Paste => Outcome::Edited,
            DialogKey::Insert(text) => {
                self.insert(&text);
                Outcome::Edited
            }
        }
    }

    /// Typed or pasted text into the focused field; Space flips "Start with the daemon".
    pub fn insert(&mut self, text: &str) {
        match self.focus {
            Focus::Autostart => {
                if text == " " {
                    self.autostart = !self.autostart;
                }
            }
            Focus::Field(i, Field::Env) => self.cards[i].env.insert(text),
            Focus::Field(i, field) => {
                line(&mut self.cards[i], field).extend(text.chars().filter(|c| !c.is_control()));
            }
        }
    }

    pub fn add_card(&mut self) {
        self.cards.push(empty_card());
        self.focus = Focus::Field(self.cards.len() - 1, Field::Name);
    }

    pub fn remove_card(&mut self, index: usize) {
        if index >= self.cards.len() {
            return;
        }
        self.cards.remove(index);
        self.focus = match self.cards.len() {
            0 => Focus::Autostart,
            len => Focus::Field(index.min(len - 1), Field::Name),
        };
    }

    /// The `project.update` to send, or `None` (in flight, or invalid: `error` says why).
    pub fn begin(&mut self) -> Option<ProjectUpdateParams> {
        if self.busy {
            return None;
        }
        let mut commands = Vec::with_capacity(self.cards.len());
        for card in &self.cards {
            let env = match parse_env(card.env.text()) {
                Ok(env) => env,
                Err(e) => {
                    self.error = Some(format!("{}: env: {e}", card.name.trim()));
                    return None;
                }
            };
            commands.push(RunCommand {
                name: card.name.clone(),
                dir: card.dir.clone(),
                command: card.command.clone(),
                env_file: Some(card.env_file.clone()),
                env,
            });
        }
        let commands = match normalize(commands) {
            Ok(commands) => commands,
            Err(e) => {
                self.error = Some(e.to_string());
                return None;
            }
        };
        self.busy = true;
        self.error = None;
        Some(ProjectUpdateParams {
            project_id: self.project_id,
            name: None,
            default_agent: None,
            run_commands: Some(commands),
            run_autostart: Some(self.autostart),
        })
    }

    /// The `project.update` failed; the dialog stays open.
    pub fn failed(&mut self, error: String) {
        self.busy = false;
        self.error = Some(error);
    }

    fn next_focus(&self) -> Focus {
        match self.focus {
            Focus::Autostart if self.cards.is_empty() => Focus::Autostart,
            Focus::Autostart => Focus::Field(0, Field::Name),
            Focus::Field(i, field) => {
                let at = Field::ORDER.iter().position(|f| *f == field).unwrap_or(0);
                match Field::ORDER.get(at + 1) {
                    Some(next) => Focus::Field(i, *next),
                    None if i + 1 < self.cards.len() => Focus::Field(i + 1, Field::Name),
                    None => Focus::Autostart,
                }
            }
        }
    }
}

fn empty_card() -> Card {
    Card {
        dir: ".".to_owned(),
        ..Card::default()
    }
}

fn card(command: &RunCommand) -> Card {
    let mut env = TextBox::default();
    let lines: Vec<String> = command.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
    env.insert(&lines.join("\n"));
    Card {
        name: command.name.clone(),
        dir: command.dir.clone(),
        command: command.command.clone(),
        env_file: command.env_file.clone().unwrap_or_default(),
        env,
    }
}

/// The single-line field `field` of `card` (`Env` never reaches here).
fn line(card: &mut Card, field: Field) -> &mut String {
    match field {
        Field::Name => &mut card.name,
        Field::Dir => &mut card.dir,
        Field::Command => &mut card.command,
        Field::EnvFile | Field::Env => &mut card.env_file,
    }
}
```

`"npm run dev\r\n"` loses its control chars in single-line fields, so the command stays one line.

- [ ] **Step 4: Run tests**

Run: `cargo test -p mkrew-ui run`
Expected: the `run::` and `run_setup::` tests PASS.

---

### Task 8: UI wiring — Run button, menu, Run setup dialog, palette, sidebar, pane header

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs`, `crates/mkrew-ui/src/palette.rs`, `crates/mkrew-ui/src/theme.rs`

**Interfaces:**
- Consumes: Task 7 (`run::*`, `run_setup::*`), protocol requests from Tasks 4–5.
- Produces: `Shell` fields `run_setup: Option<RunSetup>`, `run_menu: Option<Ulid>`, `run_reveal: Option<Ulid>`, `ports: Ports`; methods `run_summary`, `run_main_click`, `run_action`, `show_run_tab`, `reveal_run_tab`, `open_run_setup`, `close_run_setup`, `run_setup_key_down`, `save_run_setup`, `load_ports`, `render_run_button`, `render_run_menu`, `render_run_setup`; palette `Extra::{RunStart, RunRestart, RunStop, RunSetup, RunShowTab}`, `Context.{run_stopped, run_live}`; `theme::RUN_SETUP_MAX_HEIGHT`.

GPUI rendering; the logic was tested in Task 7. Verify by building, the palette unit test, and the human GUI check (Task 9).

- [ ] **Step 1: Palette (test first)** — `palette.rs` `Context` gains

```rust
    /// "Run: Start all" would start something (no command yet: it opens Run setup).
    pub run_stopped: bool,
    /// A run session of the active workspace lives.
    pub run_live: bool,
```

`Extra` gains `RunStart, RunRestart, RunStop, RunSetup, RunShowTab` (enum and `ALL`, now `[Self; 14]`), titles `"Run: Start all"`, `"Run: Restart all"`, `"Run: Stop all"`, `"Run: Setup…"`, `"Run: Show Run tab"`, availability:

```rust
            Self::RunStart => context.workspace && context.run_stopped,
            Self::RunRestart | Self::RunStop | Self::RunShowTab => {
                context.workspace && context.run_live
            }
            Self::RunSetup => context.workspace,
```

Tests: `context()` sets `run_stopped: false, run_live: false`; the `none` literal sets both `true`. `extras` drops `RunSetup` (listed whenever a workspace is active):

```rust
            .filter_map(|entry| match entry.action {
                Action::Extra(extra) if extra != Extra::RunSetup => Some(extra),
                _ => None,
            })
```

Add to `palette_only_actions_follow_the_view_and_state`:

```rust
        let stopped = Context { run_stopped: true, ..context(View::Docs) };
        assert_eq!(extras(&stopped), [Extra::RunStart]);
        let live = Context { run_live: true, ..context(View::Docs) };
        assert_eq!(extras(&live), [Extra::RunRestart, Extra::RunStop, Extra::RunShowTab]);
        let all = entries(&Keymap::defaults(), &context(View::Board), &[], &[], &[]);
        assert!(all.iter().any(|e| matches!(e.action, Action::Extra(Extra::RunSetup))));
```

Run after Step 2 compiles: `cargo test -p mkrew-ui palette` → PASS.

- [ ] **Step 2: State, loading, events** in `main.rs`

- Imports: `use mkrew_ui::run::{self, Ports, RowState, RunAction, Summary};`, `use mkrew_ui::run_setup::{self, RunSetup};`, `dialog_key`/`DialogKey` from `mkrew_ui::task_dialog` if not imported; protocol `RunStartParams, RunStarted, RunStopParams, SessionPortsEvent, DEFAULT_COLS, DEFAULT_ROWS`.
- Fields where `dev_dialog`/`dev_reveal` were:

```rust
    /// The open "Run setup" of a workspace (modal).
    run_setup: Option<RunSetup>,
    /// The workspace whose Run menu (▾) is open.
    run_menu: Option<Ulid>,
    /// The run tab to show once its `tab.updated` arrives (it may trail `project.run_start`).
    run_reveal: Option<Ulid>,
    /// Ports of live run sessions (SPEC §10).
    ports: Ports,
```

  Initialise `None, None, None, Ports::default()`; in the disconnect reset block `run_setup = None; run_menu = None; run_reveal = None; ports = Ports::default();`; `modal_open` gets `|| self.run_setup.is_some()`.
- `refresh`: after `shell.load_usage(window, cx);` add `shell.load_ports(window, cx);`. Write `load_ports` as a copy of `load_usage` (main.rs ≈685) with `self.ports.to_request(…)`, `Request::SessionPorts(ProjectRef { project_id })`, `call::<Vec<SessionPortsEvent>>`, `shell.ports.load(listed)`, `shell.ports.request_failed(project_id)`, log `"session.ports failed: {error:#}"`, doc `/// Reads the ports of each listed workspace not read yet (SPEC §10); events keep them live.`
- `dispatch`: after `if self.usages.apply(event) { cx.notify(); }` add `if self.ports.apply(event) { cx.notify(); }`; after the tab events add

```rust
                if let Event::TabUpdated(tab) = event
                    && self.run_reveal == Some(tab.id)
                {
                    self.reveal_run_tab(tab.project_id, window, cx);
                }
```

- [ ] **Step 3: Actions** in `main.rs`

```rust
    /// The Run state of `project_id` (empty when the workspace is unknown).
    fn run_summary(&self, project_id: Ulid) -> Summary {
        self.sidebar
            .projects()
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| run::summary(project, self.sidebar.sessions(), &self.ports))
            .unwrap_or_default()
    }

    /// The button's main part: Run setup when nothing is configured, the run tab while
    /// something runs, else run everything.
    fn run_main_click(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let summary = self.run_summary(project_id);
        if summary.rows.is_empty() {
            self.open_run_setup(project_id, true, window, cx);
        } else if summary.any_live() {
            self.show_run_tab(project_id, window, cx);
        } else {
            self.run_action(project_id, RunAction::Start, None, window, cx);
        }
    }

    /// `project.run_start` / `run_restart` / `run_stop` (`name` = one command, `None` = all).
    /// Running from all-stopped shows the run tab; restarts keep the view (SPEC §10). A
    /// failure shows in the sidebar notice.
    fn run_action(
        &mut self,
        project_id: Ulid,
        action: RunAction,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        self.run_menu = None;
        cx.notify();
        let Some(connection) = self.connection.clone() else {
            return;
        };
        if action == RunAction::Stop {
            let stopped =
                connection.call::<()>(Request::ProjectRunStop(RunStopParams { project_id, name }));
            cx.spawn_in(window, async move |this, cx| {
                let stopped = stopped.await;
                let _ = this.update(cx, |shell, cx| {
                    shell.sidebar_error = stopped.err().map(|error| {
                        tracing::warn!(%project_id, "project.run_stop failed: {error:#}");
                        format!("Cannot stop: {error:#}")
                    });
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        let reveal = action == RunAction::Start
            && name.is_none()
            && !self.run_summary(project_id).any_live();
        let params = RunStartParams { project_id, name, cols: DEFAULT_COLS, rows: DEFAULT_ROWS };
        let request = match action {
            RunAction::Restart => Request::ProjectRunRestart(params),
            RunAction::Start | RunAction::Stop => Request::ProjectRunStart(params),
        };
        let started = connection.call::<RunStarted>(request);
        cx.spawn_in(window, async move |this, cx| {
            let started = started.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                match started {
                    Ok(run) => {
                        shell.sidebar_error = None;
                        if reveal {
                            shell.views.insert(project_id, View::Terminals);
                            shell.run_reveal = run.tab_id;
                            shell.reveal_run_tab(project_id, window, cx);
                            shell.activate_workspace(project_id, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "run failed: {error:#}");
                        shell.sidebar_error = Some(format!("Cannot run: {error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Switches to the workspace's run tab.
    fn show_run_tab(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.run_menu = None;
        self.run_reveal = self.tabs.get(&project_id).and_then(|tabs| {
            tabs.tabs()
                .iter()
                .find(|tab| tab.title == mkrew_core::run::TAB_TITLE)
                .map(|tab| tab.id)
        });
        self.views.insert(project_id, View::Terminals);
        self.reveal_run_tab(project_id, window, cx);
        self.activate_workspace(project_id, window, cx);
    }

    /// Activates the tab in `run_reveal` once `project_id`'s tabs hold it.
    fn reveal_run_tab(&mut self, project_id: Ulid, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(tab_id) = self.run_reveal else {
            return;
        };
        let Some(tabs) = self.tabs.get_mut(&project_id) else {
            return;
        };
        if tabs.tabs().iter().any(|tab| tab.id == tab_id) {
            tabs.activate(tab_id);
            self.run_reveal = None;
            self.sync_panes(window, cx);
            cx.notify();
        }
    }

    fn open_run_setup(&mut self, project_id: Ulid, run_after: bool, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.workspace_menu = None;
        self.run_menu = None;
        let Some(project) = self.sidebar.projects().iter().find(|p| p.id == project_id) else {
            return;
        };
        self.run_setup = Some(RunSetup::new(project, run_after));
        self.dialog_serial += 1;
        window.focus(&self.dialog_focus, cx);
        cx.notify();
    }

    fn close_run_setup(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.run_setup = None;
        self.focus_pane(window, cx);
        cx.notify();
    }

    fn run_setup_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<'_, Self>) {
        cx.stop_propagation();
        let Some(key) = dialog_key(&event.keystroke) else {
            return;
        };
        let paste = (key == DialogKey::Paste)
            .then(|| cx.read_from_clipboard().and_then(|item| item.text()))
            .flatten();
        let Some(setup) = self.run_setup.as_mut() else {
            return;
        };
        if let Some(text) = paste {
            setup.insert(&text);
        }
        match setup.key(key) {
            run_setup::Outcome::Cancel => return self.close_run_setup(window, cx),
            run_setup::Outcome::Save => return self.save_run_setup(window, cx),
            run_setup::Outcome::Edited => {}
        }
        cx.notify();
    }

    /// `project.update` with the dialog's commands; on success it closes and, when opened from
    /// ▶ Run, runs everything.
    fn save_run_setup(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        let Some(connection) = self.connection.clone() else {
            return;
        };
        let serial = self.dialog_serial;
        let Some(setup) = self.run_setup.as_mut() else {
            return;
        };
        let run_after = setup.run_after;
        let Some(params) = setup.begin() else {
            cx.notify();
            return;
        };
        let project_id = params.project_id;
        let updated = connection.call::<Project>(Request::ProjectUpdate(params));
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let updated = updated.await;
            let _ = this.update_in(cx, |shell, window, cx| {
                if shell.dialog_serial != serial || shell.run_setup.is_none() {
                    return;
                }
                match updated {
                    Ok(project) => {
                        // Seen before `project.updated` arrives, so the run below finds the commands.
                        shell.sidebar.apply(&Event::ProjectUpdated(project.clone()));
                        shell.close_run_setup(window, cx);
                        if run_after && !project.run_commands.is_empty() {
                            shell.run_action(project_id, RunAction::Start, None, window, cx);
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%project_id, "project.update failed: {error:#}");
                        if let Some(setup) = shell.run_setup.as_mut() {
                            setup.failed(format!("{error:#}"));
                        }
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }
```

- [ ] **Step 4: Rendering**

`theme.rs`, next to `DIALOG_WIDTH`:

```rust
/// Height of the Run setup dialog's command list before it scrolls.
pub const RUN_SETUP_MAX_HEIGHT: f32 = 420.0;
```

View bar — replace the trailing `.when_some(…new_task_button…)` in `render_view_bar` with:

```rust
            .when_some(self.workspace, |el, project_id| {
                el.child(div().flex_1())
                    .when(shown == View::Board && self.board.has_tasks(project_id), |el| {
                        el.child(self.new_task_button(project_id, false, theme, cx))
                    })
                    .child(self.render_run_button(project_id, theme, cx))
            })
```

Button and menu:

```rust
    /// The Run button (SPEC §10): `▶ Run`, or the running count and ports with ↻ and ■;
    /// `▾` opens the per-command menu below it.
    fn render_run_button(&self, project_id: Ulid, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement {
        let summary = self.run_summary(project_id);
        let live = summary.any_live();
        let tip = if summary.rows.is_empty() {
            "Set up the commands to run".to_owned()
        } else {
            summary.tooltip()
        };
        let main = button("run-main", summary.label(), theme, !live)
            .flex_none()
            .when(summary.warning(), |el| el.text_color(rgb(theme.status_needs_input)))
            .tooltip(tooltip(theme, &tip, ""))
            .on_click(cx.listener(move |shell, _, window, cx| {
                shell.run_main_click(project_id, window, cx);
            }));
        let open = self.run_menu == Some(project_id);
        let mut bar = div().flex_none().flex().items_center().gap(px(space::XS)).child(main);
        if live {
            for (id, label, tip, action) in [
                ("run-restart", "↻", "Restart all run commands", RunAction::Restart),
                ("run-stop", "■", "Stop all run commands", RunAction::Stop),
            ] {
                bar = bar.child(
                    button(id, label, theme, false)
                        .flex_none()
                        .tooltip(tooltip(theme, tip, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.run_action(project_id, action, None, window, cx);
                        })),
                );
            }
        }
        bar.child(
            div()
                .child(
                    button("run-menu", "▾", theme, false)
                        .tooltip(tooltip(theme, "Run commands", ""))
                        .on_click(cx.listener(move |shell, _, _, cx| {
                            shell.run_menu = (shell.run_menu != Some(project_id)).then_some(project_id);
                            cx.notify();
                        })),
                )
                .when(open, |el| el.child(self.render_run_menu(project_id, &summary, theme, cx))),
        )
        .into_any_element()
    }

    /// One row per command (state, ports, ↻ ■ or ▶), then "Show Run tab" and "Run setup…".
    fn render_run_menu(&self, project_id: Ulid, summary: &Summary, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement {
        let hover = theme.surface_raised;
        let separator = || div().h(px(theme::BORDER_WIDTH)).my(px(space::XS)).bg(rgb(theme.border));
        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        for row in &summary.rows {
            let running = matches!(row.state, RowState::Running(_));
            let color = match row.state {
                RowState::Running(_) => theme.status_done,
                RowState::Exited(_) => theme.status_exited,
                RowState::Stopped => theme.text_muted,
            };
            let ports: Vec<String> = row.ports.iter().map(|p| format!(":{p}")).collect();
            let controls: &[(&'static str, &'static str, RunAction)] = if running {
                &[("↻", "Restart this command", RunAction::Restart), ("■", "Stop this command", RunAction::Stop)]
            } else {
                &[("▶", "Run this command", RunAction::Start)]
            };
            let mut line = div()
                .flex()
                .items_center()
                .gap(px(space::S))
                .px(px(space::S))
                .py(px(space::XS))
                .text_size(px(font_size::BODY))
                .child(div().flex_1().text_color(rgb(theme.text)).child(row.name.clone()))
                .child(div().text_size(px(font_size::META)).text_color(rgb(color)).child(run::state_text(&row.state)))
                .child(div().text_size(px(font_size::META)).text_color(rgb(theme.text_muted)).child(ports.join(" ")));
            for &(label, tip, action) in controls {
                let name = row.name.clone();
                line = line.child(
                    div()
                        .id(SharedString::from(format!("run-{label}-{}", row.name)))
                        .px(px(space::XS))
                        .rounded(px(radius::SMALL))
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgb(hover)))
                        .tooltip(tooltip(theme, tip, ""))
                        .on_click(cx.listener(move |shell, _, window, cx| {
                            shell.run_action(project_id, action, Some(name.clone()), window, cx);
                        }))
                        .child(label),
                );
            }
            rows.push(line.into_any_element());
        }
        let panel = div()
            .id("run-menu-panel")
            .w(px(theme::MENU_WIDTH))
            .flex()
            .flex_col()
            .p(px(space::XS))
            .rounded(px(radius::MEDIUM))
            .border(px(theme::BORDER_WIDTH))
            .border_color(rgb(theme.border))
            .bg(rgb(theme.surface_raised))
            .shadow_md()
            .font_family(theme::UI_FONT)
            .on_mouse_down_out(cx.listener(|shell, _, _, cx| {
                shell.run_menu = None;
                cx.notify();
            }))
            .when(!rows.is_empty(), |el| el.children(rows).child(separator()))
            .child(
                menu_item("run-show-tab".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child("Show Run tab"))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.show_run_tab(project_id, window, cx);
                    })),
            )
            .child(
                menu_item("run-setup".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child("Run setup…"))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.open_run_setup(project_id, false, window, cx);
                    })),
            );
        deferred(
            anchored()
                .snap_to_window_with_margin(px(space::S))
                .child(div().pt(px(space::XS)).child(panel)),
        )
        .into_any_element()
    }
```

Workspace menu: after the "Open in Zed" item and its separator, add

```rust
            .child(
                menu_item("workspace-run-setup".into(), theme)
                    .child(div().flex_1().text_color(rgb(theme.text)).child("Run setup…"))
                    .on_click(cx.listener(move |shell, _, window, cx| {
                        shell.open_run_setup(project_id, false, window, cx);
                    })),
            )
            .child(separator())
```

Run setup dialog — same frame as the deleted `render_dev_dialog` (`git show HEAD:crates/mkrew-ui/src/main.rs`, fn `render_dev_dialog`): panel `div().id("run-setup").w(px(theme::DIALOG_WIDTH)).flex().flex_col().gap(px(space::M)).p(px(space::XL)).rounded(px(radius::LARGE)).border(px(theme::BORDER_WIDTH)).border_color(rgb(theme.border)).bg(rgb(theme.surface)).shadow_md().font_family(theme::UI_FONT).track_focus(&self.dialog_focus).on_key_down(cx.listener(Self::run_setup_key_down)).relative().child(self.typing(&self.dialog_focus, Self::run_setup_key_down, cx))`, wrapped in `Self::modal_shade("run-setup-shade", panel, cx)`. Its children, in order:

```rust
    /// "Run setup": one card per command, "+ Add command", "Start with the daemon", errors,
    /// Cancel and Save (Ctrl+Enter).
    fn render_run_setup(&self, setup: &RunSetup, theme: &Theme, cx: &mut Context<'_, Self>) -> gpui::AnyElement {
        use run_setup::{Field, Focus};
        let label = |text: &'static str| {
            div()
                .text_size(px(font_size::META))
                .font_weight(FontWeight(weight::MEDIUM))
                .text_color(rgb(theme.text_muted))
                .child(text)
        };
        let field = |i: usize, which: Field, id: &'static str, text: &str, placeholder: &'static str| {
            text_field(id, text, placeholder, setup.focus == Focus::Field(i, which), theme).on_click(
                cx.listener(move |shell, _, _, cx| {
                    if let Some(setup) = shell.run_setup.as_mut() {
                        setup.focus = Focus::Field(i, which);
                    }
                    cx.notify();
                }),
            )
        };
        let mut cards = div()
            .id("run-cards")
            .flex()
            .flex_col()
            .gap(px(space::S))
            .max_h(px(theme::RUN_SETUP_MAX_HEIGHT))
            .overflow_y_scroll();
        for (i, card) in setup.cards.iter().enumerate() {
            cards = cards.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(space::XS))
                    .p(px(space::S))
                    .rounded(px(radius::MEDIUM))
                    .border(px(theme::BORDER_WIDTH))
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .flex()
                            .gap(px(space::S))
                            .child(div().flex_1().child(label("Name")).child(field(i, Field::Name, "run-name", &card.name, "e.g. BE")))
                            .child(div().flex_1().child(label("Folder")).child(field(i, Field::Dir, "run-dir", &card.dir, ". or EA-BE"))),
                    )
                    .child(label("Command"))
                    .child(field(i, Field::Command, "run-command", &card.command, "e.g. npm run dev"))
                    .child(label("Env file (optional, relative to the folder)"))
                    .child(field(i, Field::EnvFile, "run-env-file", &card.env_file, "e.g. .env"))
                    .child(label("Env (KEY=VALUE per line, wins over the env file)"))
                    .child(field(i, Field::Env, "run-env", card.env.text(), "e.g. ENVIRONMENT=dev"))
                    .child(
                        div().flex().justify_end().child(
                            quiet_button("run-remove", "Remove", theme, theme.surface_raised).on_click(
                                cx.listener(move |shell, _, _, cx| {
                                    if let Some(setup) = shell.run_setup.as_mut() {
                                        setup.remove_card(i);
                                    }
                                    cx.notify();
                                }),
                            ),
                        ),
                    ),
            );
        }
        // Then build `panel` as described above with these children:
        //   title "Run setup" (font_size::CARD_TITLE, weight::SEMIBOLD, theme.text);
        //   sentence "Each command runs in its own pane of the “run” tab, in a shell started in
        //   its folder." (font_size::BODY);
        //   `cards`;
        //   button("run-add", "+ Add command", theme, false) → `setup.add_card()` + notify;
        //   the autostart row: copy the old `dev-autostart` row with id "run-autostart", label
        //   "Start with the daemon", keys "Space", ✓ when `setup.autostart`, a
        //   `border_color(rgb(theme.pane_focus_border))` when `setup.focus == Focus::Autostart`,
        //   click flips `setup.autostart`;
        //   the old error row for `setup.error`;
        //   buttons: button("run-cancel", "Cancel", …) → close_run_setup (tooltip keys "Esc");
        //   disabled_button("run-save", "Saving…", …) while `setup.busy`, else
        //   button("run-save", "Save", theme, true) → save_run_setup (tooltip
        //   "Save the run commands", keys "Ctrl+Enter").
        // Return `Self::modal_shade("run-setup-shade", panel, cx)`.
    }
```

Root render: `.children(self.run_setup.as_ref().map(|setup| self.render_run_setup(setup, &theme, cx)))` where the dev dialog was.

If GPUI's borrow rules reject the `field` closure (captures `cx` while `cx.listener` is called inside), inline it per field; keep the same elements.

Sidebar row: `workspace_row(row, active, run_tip, tokens, theme, cx)` with `run_tip: Option<String>`, computed at the call site:

```rust
                            let summary = self.run_summary(row.project_id);
                            let run_tip = summary.any_live().then(|| summary.tooltip());
```

and rendered where the "▶ dev" block was:

```rust
        .when_some(run_tip, |el, tip| {
            el.child(
                div()
                    .id(SharedString::from(format!("workspace-run-{project_id}")))
                    .text_size(px(font_size::META))
                    .text_color(rgb(theme.text_muted))
                    .tooltip(tooltip(theme, &tip, ""))
                    .child("▶"),
            )
        })
```

Pane header: add `ports: &[u16]` to `pane_header`'s parameters and change the title cell's `.children(session.title.clone())` to `.children(run::pane_title(session, ports))`; the caller passes `self.ports.get(info.id)`.

Palette context:

```rust
        let run_state = workspace.map(|id| self.run_summary(id)).unwrap_or_default();
```

and fields `run_stopped: workspace.is_some() && (run_state.rows.is_empty() || run_state.live() < run_state.rows.len()), run_live: run_state.any_live(),` (not named `run`: that would shadow the imported `run` module). Extra dispatch:

```rust
                    // The list is built on open: the run state may have changed since.
                    Extra::RunStart => {
                        if self.run_summary(project_id).rows.is_empty() {
                            self.open_run_setup(project_id, true, window, cx);
                        } else {
                            self.run_action(project_id, RunAction::Start, None, window, cx);
                        }
                    }
                    Extra::RunRestart => self.run_action(project_id, RunAction::Restart, None, window, cx),
                    Extra::RunStop => self.run_action(project_id, RunAction::Stop, None, window, cx),
                    Extra::RunSetup => self.open_run_setup(project_id, false, window, cx),
                    Extra::RunShowTab => self.show_run_tab(project_id, window, cx),
```

- [ ] **Step 5: Build and test**

Run: `cargo build -p mkrew-ui`, `cargo test -p mkrew-ui`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: builds, PASS, clean.

- [ ] **Step 6: Smoke run**

GPUI windows cannot be checked headless: ask the human to run `cargo run -p mkrew-ui` and walk the HUMAN_CHECKS entry from Task 9. Meanwhile exercise the daemon path yourself: build, start a scratch daemon with `MKREW_DATA_DIR`/`MKREW_CONFIG_DIR` pointing to a temp folder, add a temp project, set two commands with a temporary `cargo test` (or the UI), then `mk project run <p>` / `restart <p> <name>` / `stop <p>` and observe output and sessions. Report only what was observed.

---

### Task 9: Docs — ADR, SPEC, HUMAN_CHECKS, PROGRESS

**Files:**
- Create: `docs/decisions/0018-run-commands.md`
- Modify: `docs/decisions/0008-dev-server.md` (status line), `docs/SPEC.md`, `docs/HUMAN_CHECKS.md`, `docs/PROGRESS.md`

- [ ] **Step 1: ADR 0018** (format of `docs/decisions/0000-template.md`):

```markdown
# 0018 — Run commands replace the dev server

- Date: 2026-10-07
- Status: accepted (supersedes 0008)
- Milestone: none (outside the milestone loop; spec docs/superpowers/specs/2026-10-07-run-button-design.md)

## Context

A workspace often holds several servers (EA: a Python backend in `EA-BE`, a Vite frontend in
`EA-FE`). ADR 0008 allowed one `dev_command`, run in the project folder, so users wrote wrapper
scripts (`run.ps1`) to set the folder and load `.env`.

## Options

1. Several dev commands, still one string each. Cons: no folder or env; wrapper scripts stay.
2. Run commands with name, folder, command, env file and env, typed into a shell as in 0008.
3. Launch programs directly (no shell). Cons: shell syntax (`&&`, `npm` shims) breaks.

## Decision

Option 2. Table `run_command` (migration 0011; the old `dev_command` becomes a command named
`dev`, `dev_autostart` → `run_autostart`); `Project.run_commands`; requests
`project.run_start | run_restart | run_stop` (`name?`) replace `project.dev_start | dev_stop`;
a run session is a live shell titled `run:<name>`; the live ones lie side by side in the `run`
tab. The typed line ends with an exit for the shell family, so a session ends with its command.
Ports come from the session's screen text (`session.ports` event and request).

## Consequences

Protocol v25. `mk project dev` → `mk project run | restart | stop <p> [name]`; `--dev*` flags
removed, `--run-autostart` added. Ports that are never printed are not shown (reading the OS
socket table is the upgrade). Commands are edited in the UI only.
```

ADR 0008: `- Status: superseded by 0018`.

- [ ] **Step 2: SPEC.md** (find by text; line numbers drift):
  - §5: after the `0007_dev_server.sql` block add the `0011_run_commands.sql` SQL from Task 2 under `-- migrations/0011_run_commands.sql (Run button, ADR 0018)`.
  - §7: in the `project.list/add/update/archive` row replace `dev_command`, `dev_autostart (update only)` by `run_commands`, `run_autostart` (update only), and "`dev_command` is trimmed, blank = cleared" by "`run_commands` replaces the list; each command is trimmed and checked (name 1–20 chars without `/` `:`, unique; one-line command; relative `dir`/`env_file` inside the project; env keys `[A-Za-z_][A-Za-z0-9_]*`; max 8)". Replace the `project.dev_start` / `project.dev_stop` rows by rows for `project.run_start`, `project.run_restart`, `project.run_stop`, request `session.ports` and event `session.ports`, worded from the design spec §3.
  - §8: replace the "Dev servers" bullet with "**Run commands:** after resume, `project.run_start` (all) for every non-archived project with `run_autostart` and at least one command (ADR 0018)."
  - §9: replace `mk project dev <project> [--stop]` with `mk project run|restart|stop <project> [name]` (three lines); `mk project update` flag `--run-autostart on|off`.
  - §10: add a **Run button** bullet summarising design spec §5.
  - §18 M8 criterion text stays.
- [ ] **Step 3: HUMAN_CHECKS.md** — under "To verify (non-blocking)":

```markdown
### 2026-10-07 Run button (plan docs/superpowers/plans/2026-10-07-run-button.md)

- [ ] EA: Run setup with BE (folder `EA-BE`, command `.venv\Scripts\python.exe main.py`, env file
  `.env`, env `ENVIRONMENT=dev` and `PYTHONPATH=.`) and FE (folder `EA-FE`, `npm run dev`).
  ▶ Run opens the "run" tab with two panes; the button shows `● Running 2/2 · :8080 :5180`.
  `[INFERENCE]` uvicorn prints its URL through EA's console log handler; if `:8080` is missing,
  note it.
- [ ] ↻ on BE in the ▾ menu restarts only BE (the FE pane keeps its output); the view stays.
- [ ] Ctrl+C in BE's pane: the button turns `✕ Running 1/2` in the warning color; ▶ on BE in the
  menu starts it again.
- [ ] ■ stops both; the button returns to `▶ Run`; the sidebar ▶ disappears.
- [ ] A command in a missing folder: ▶ Run shows "Cannot run: …" in the sidebar notice; nothing starts.
- [ ] Known gaps: the ▾ menu is mouse-only (the palette has Run: Start/Restart/Stop all, Setup…,
  Show Run tab); the default shell on Windows is cmd, so a `.ps1` script needs
  `pwsh -File .\run.ps1` as its command.
```

- [ ] **Step 4: PROGRESS.md** — in "Notes for the next session" add `- 2026-10-07 Run button (plan docs/superpowers/plans/2026-10-07-run-button.md, ADR 0018): run commands replace the dev server; manual checks in HUMAN_CHECKS "2026-10-07 Run button". Outside the milestone loop.` and change "Protocol is v24" to "Protocol is v25". Keep `status: DONE`.

- [ ] **Step 5: Final verification**

Run: `& "C:\Program Files\Git\bin\bash.exe" scripts/verify.sh` (or the fallback in Global Constraints).
Expected: fmt clean, clippy clean, all tests PASS. Commit.
