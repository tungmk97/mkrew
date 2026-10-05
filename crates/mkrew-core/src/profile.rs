//! Agent profiles (SPEC §6): one TOML file per agent, the file stem is the profile id.
//!
//! [`load_dir`] never fails as a whole: every broken file becomes a [`ProfileError`] with the
//! file and, when known, the line/column, so one bad profile cannot take the daemon down.

use std::collections::BTreeMap;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use toml::Spanned;

/// A parsed, validated agent profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    /// File stem, e.g. `claude` for `claude.toml`.
    pub id: String,
    pub name: String,
    pub kind: ProfileKind,
    /// Program to run. Empty only for [`ProfileKind::Shell`], meaning the OS default shell.
    pub command: String,
    pub args: Vec<String>,
    pub prompt_mode: PromptMode,
    pub env: BTreeMap<String, String>,
    /// Resume command template with `{session_id}`; `None` = no resume.
    pub resume: Option<String>,
    pub state: StateConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileKind {
    #[default]
    Agent,
    Shell,
}

/// How the task prompt reaches the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptMode {
    /// Append the prompt as the last argument.
    Arg,
    /// Type the prompt into the PTY once the agent is ready, then Enter.
    Type,
    /// Do not send.
    #[default]
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum StateMode {
    #[serde(rename = "hook")]
    Hook,
    #[serde(rename = "screen")]
    Screen,
    #[serde(rename = "hook+screen")]
    HookScreen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateConfig {
    pub mode: StateMode,
    pub idle_after_ms: u64,
    /// Regexes matched against the last lines of the emulated screen; all validated at parse time.
    pub needs_input_patterns: Vec<String>,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            mode: StateMode::Screen,
            idle_after_ms: DEFAULT_IDLE_AFTER_MS,
            needs_input_patterns: Vec::new(),
        }
    }
}

const DEFAULT_IDLE_AFTER_MS: u64 = 3000;

/// A profile file that could not be loaded. Displays as `file[:line[:column]]: message`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}{}: {message}", file.display(), position(*line, *column))]
pub struct ProfileError {
    pub file: PathBuf,
    /// 1-based; `None` when the error is not tied to a position (e.g. unreadable file).
    pub line: Option<usize>,
    /// 1-based, in characters.
    pub column: Option<usize>,
    pub message: String,
}

fn position(line: Option<usize>, column: Option<usize>) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!(":{line}:{column}"),
        (Some(line), None) => format!(":{line}"),
        _ => String::new(),
    }
}

/// Result of loading a profile directory: good profiles plus one error per broken file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileSet {
    /// Sorted by id.
    pub profiles: Vec<AgentProfile>,
    /// Sorted by file.
    pub errors: Vec<ProfileError>,
}

impl ProfileSet {
    pub fn get(&self, id: &str) -> Option<&AgentProfile> {
        self.profiles.iter().find(|p| p.id == id)
    }
}

/// Loads every `*.toml` file in `dir`. A missing directory yields an empty set.
pub fn load_dir(dir: &Path) -> ProfileSet {
    let mut set = ProfileSet::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return set,
        Err(e) => {
            set.errors.push(io_error(dir, &e));
            return set;
        }
    };
    for entry in entries {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(e) => {
                set.errors.push(io_error(dir, &e));
                continue;
            }
        };
        let is_toml = path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"));
        if !is_toml || !path.is_file() {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
            set.errors.push(ProfileError {
                file: path.clone(),
                line: None,
                column: None,
                message: "file name is not valid UTF-8".into(),
            });
            continue;
        };
        let id = id.to_owned();
        match std::fs::read_to_string(&path) {
            Ok(src) => match parse_profile(&id, &path, &src) {
                Ok(profile) => set.profiles.push(profile),
                Err(e) => set.errors.push(e),
            },
            Err(e) => set.errors.push(io_error(&path, &e)),
        }
    }
    set.profiles.sort_by(|a, b| a.id.cmp(&b.id));
    set.errors.sort_by(|a, b| a.file.cmp(&b.file));
    set
}

/// Parses one profile. `file` is only used in error messages.
pub fn parse_profile(id: &str, file: &Path, src: &str) -> Result<AgentProfile, ProfileError> {
    let err = |span: Option<Range<usize>>, message: String| {
        let (line, column) = match span {
            Some(span) => {
                let (l, c) = line_col(src, span.start);
                (Some(l), Some(c))
            }
            None => (None, None),
        };
        ProfileError {
            file: file.to_path_buf(),
            line,
            column,
            message,
        }
    };

    let raw: RawProfile =
        toml::from_str(src).map_err(|e| err(e.span(), e.message().trim_end().to_owned()))?;

    let (command_span, command) = match raw.command {
        Some(c) => (Some(c.span()), c.into_inner().trim().to_owned()),
        None => (None, String::new()),
    };
    if command.is_empty() && raw.kind == ProfileKind::Agent {
        return Err(err(
            command_span,
            "`command` is required and must not be empty for an agent profile".into(),
        ));
    }

    let mut needs_input_patterns = Vec::with_capacity(raw.state.needs_input_patterns.len());
    for pattern in raw.state.needs_input_patterns {
        let span = pattern.span();
        let pattern = pattern.into_inner();
        if let Err(e) = regex::Regex::new(&pattern) {
            return Err(err(
                Some(span),
                format!("invalid needs_input_patterns regex `{pattern}`: {e}"),
            ));
        }
        needs_input_patterns.push(pattern);
    }

    Ok(AgentProfile {
        id: id.to_owned(),
        name: raw.name.unwrap_or_else(|| id.to_owned()),
        kind: raw.kind,
        command,
        args: raw.args,
        prompt_mode: raw.prompt_mode,
        env: raw.env,
        resume: raw.resume.filter(|r| !r.trim().is_empty()),
        state: StateConfig {
            mode: raw.state.mode,
            idle_after_ms: raw.state.idle_after_ms,
            needs_input_patterns,
        },
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawProfile {
    name: Option<String>,
    #[serde(default)]
    kind: ProfileKind,
    command: Option<Spanned<String>>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    prompt_mode: PromptMode,
    #[serde(default)]
    env: BTreeMap<String, String>,
    resume: Option<String>,
    #[serde(default)]
    state: RawState,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawState {
    #[serde(default = "default_mode")]
    mode: StateMode,
    #[serde(default = "default_idle_after_ms")]
    idle_after_ms: u64,
    #[serde(default)]
    needs_input_patterns: Vec<Spanned<String>>,
}

impl Default for RawState {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            idle_after_ms: DEFAULT_IDLE_AFTER_MS,
            needs_input_patterns: Vec::new(),
        }
    }
}

fn default_mode() -> StateMode {
    StateMode::Screen
}

fn default_idle_after_ms() -> u64 {
    DEFAULT_IDLE_AFTER_MS
}

fn io_error(path: &Path, e: &io::Error) -> ProfileError {
    ProfileError {
        file: path.to_path_buf(),
        line: None,
        column: None,
        message: e.to_string(),
    }
}

/// 1-based line and character column of byte `offset` in `src`.
fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(src.len());
    let before = src.get(..offset).unwrap_or(src);
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Result<AgentProfile, ProfileError> {
        parse_profile("broken", Path::new("agents/broken.toml"), src)
    }

    fn parse_err(src: &str) -> String {
        parse(src).unwrap_err().to_string()
    }

    #[test]
    fn parses_full_profile() {
        let src = r#"
name    = "Claude Code"
command = "claude"
args    = ["--verbose"]
prompt_mode = "arg"
env = { FOO = "bar" }
resume = "claude --resume {session_id}"

[state]
mode = "hook+screen"
idle_after_ms = 2500
needs_input_patterns = ['(?i)do you want to', '(?i)\(y/n\)']
"#;
        let p = parse_profile("claude", Path::new("claude.toml"), src).unwrap();
        assert_eq!(p.id, "claude");
        assert_eq!(p.name, "Claude Code");
        assert_eq!(p.kind, ProfileKind::Agent);
        assert_eq!(p.command, "claude");
        assert_eq!(p.args, vec!["--verbose"]);
        assert_eq!(p.prompt_mode, PromptMode::Arg);
        assert_eq!(p.env.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(p.resume.as_deref(), Some("claude --resume {session_id}"));
        assert_eq!(p.state.mode, StateMode::HookScreen);
        assert_eq!(p.state.idle_after_ms, 2500);
        assert_eq!(p.state.needs_input_patterns.len(), 2);
    }

    #[test]
    fn defaults_apply() {
        let p = parse("command = \"x\"").unwrap();
        assert_eq!(p.name, "broken");
        assert_eq!(p.prompt_mode, PromptMode::None);
        assert_eq!(p.resume, None);
        assert_eq!(p.state, StateConfig::default());
    }

    #[test]
    fn empty_resume_means_none() {
        let p = parse("command = \"x\"\nresume = \"\"").unwrap();
        assert_eq!(p.resume, None);
    }

    #[test]
    fn shell_profile_may_omit_command() {
        let p = parse("kind = \"shell\"").unwrap();
        assert_eq!(p.kind, ProfileKind::Shell);
        assert_eq!(p.command, "");
    }

    #[test]
    fn error_has_line_and_column() {
        let e = parse("command = \"x\"\n\n[state]\nidle_after_ms = \"soon\"\n").unwrap_err();
        assert_eq!(e.line, Some(4));
        assert_eq!(e.column, Some(17));
    }

    #[test]
    fn line_col_counts_chars() {
        assert_eq!(line_col("a\n❯ b", 6), (2, 3));
        assert_eq!(line_col("abc", 99), (1, 4));
    }

    #[test]
    fn snapshot_syntax_error() {
        insta::assert_snapshot!(parse_err("name = \"Claude\ncommand = \"claude\"\n"));
    }

    #[test]
    fn snapshot_unknown_field() {
        insta::assert_snapshot!(parse_err("command = \"claude\"\npromt_mode = \"arg\"\n"));
    }

    #[test]
    fn snapshot_bad_prompt_mode() {
        insta::assert_snapshot!(parse_err("command = \"claude\"\nprompt_mode = \"paste\"\n"));
    }

    #[test]
    fn snapshot_bad_state_mode() {
        insta::assert_snapshot!(parse_err("command = \"c\"\n[state]\nmode = \"hooks\"\n"));
    }

    #[test]
    fn snapshot_missing_command() {
        insta::assert_snapshot!(parse_err("name = \"Claude\"\n"));
    }

    #[test]
    fn snapshot_bad_regex() {
        insta::assert_snapshot!(parse_err(
            "command = \"c\"\n[state]\nneeds_input_patterns = ['ok', '(unclosed']\n"
        ));
    }

    #[test]
    fn load_dir_collects_profiles_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("good.toml"), "command = \"good\"").unwrap();
        std::fs::write(dir.path().join("bad.toml"), "command = \n").unwrap();
        std::fs::write(dir.path().join("notes.txt"), "not a profile").unwrap();
        let set = load_dir(dir.path());
        assert_eq!(set.profiles.len(), 1);
        assert_eq!(set.get("good").unwrap().command, "good");
        assert_eq!(set.errors.len(), 1);
        let e = &set.errors[0];
        assert_eq!(e.file, dir.path().join("bad.toml"));
        assert_eq!(e.line, Some(1));
        assert!(e.column.is_some());
    }

    #[test]
    fn load_dir_missing_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let set = load_dir(&dir.path().join("nope"));
        assert_eq!(set, ProfileSet::default());
    }

    #[test]
    fn repo_sample_profiles_parse() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../agents");
        let set = load_dir(&dir);
        assert!(set.errors.is_empty(), "{:?}", set.errors);
        let ids: Vec<_> = set.profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["claude", "codex", "fake", "omp", "shell"]);
        assert_eq!(set.get("shell").unwrap().kind, ProfileKind::Shell);
        assert_eq!(set.get("fake").unwrap().command, "mkrew-fakeagent");
    }
}
