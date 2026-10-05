//! `mkrew-fakeagent`: a scriptable fake coding agent (SPEC §16).
//!
//! A scenario is a TOML file with a list of `[[step]]` tables, each tagged by `action`:
//!
//! ```toml
//! [[step]]
//! action = "print"
//! text = "Reading files…"
//! color = "cyan"
//!
//! [[step]]
//! action = "ask"          # prints "Do you want to proceed? (y/n)" and reads one line
//! ```
//!
//! [`run`] is PTY-free: it takes any reader/writer, so tests drive it with in-memory buffers.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

/// Built-in scenarios, usable by name instead of a file path.
pub const BUILTIN: &[(&str, &str)] = &[
    ("ask", include_str!("../scenarios/ask.toml")),
    ("flood", include_str!("../scenarios/flood.toml")),
    ("omp", include_str!("../scenarios/omp.toml")),
];

/// Scenario used when neither `--scenario` nor `MKREW_FAKE_SCENARIO` is given.
pub const DEFAULT_SCENARIO: &str = "ask";

/// Env var naming the scenario (file path or built-in name).
pub const SCENARIO_ENV: &str = "MKREW_FAKE_SCENARIO";

pub const DEFAULT_PROMPT: &str = "Do you want to proceed? (y/n)";

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
const SPINNER_FRAME_MS: u64 = 80;
const RESET: &str = "\x1b[0m";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{source_name}: {message}")]
    Parse {
        source_name: String,
        message: String,
    },
    #[error("cannot read scenario {}: {source}", path.display())]
    Read { path: PathBuf, source: io::Error },
    #[error(
        "unknown scenario `{name}`: not a file and not a built-in ({})",
        builtin_names()
    )]
    UnknownScenario { name: String },
    #[error("cannot write {}: {source}", path.display())]
    WriteFile { path: PathBuf, source: io::Error },
    #[error("cannot run {}: {source}", program.display())]
    Mk { program: PathBuf, source: io::Error },
    #[error("cannot edit {}: {source}", path.display())]
    EditFile { path: PathBuf, source: io::Error },
    #[error("stdin closed while waiting for an answer")]
    StdinClosed,
    #[error(transparent)]
    Io(#[from] io::Error),
}

fn builtin_names() -> String {
    BUILTIN
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    #[serde(rename = "step", default)]
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Print one line, optionally colored.
    Print {
        text: String,
        #[serde(default)]
        color: Option<Color>,
    },
    /// Animate a spinner on one line for `ms`, then leave a "done" line.
    Spinner {
        text: String,
        ms: u64,
    },
    /// Print the prompt (no newline) and block until one line is read from stdin.
    Ask {
        #[serde(default = "default_prompt")]
        text: String,
    },
    Sleep {
        ms: u64,
    },
    /// Print `lines` colored lines as fast as possible (stress test, SPEC §15).
    Flood {
        lines: u64,
    },
    /// Write `content` to `path` (relative to the working directory).
    WriteFile {
        path: PathBuf,
        content: String,
    },
    /// Run `mk hook fake <event>` like an agent hook would (SPEC §6) and wait for it.
    /// `session_id` is reported as the agent's own id (payload `{"session_id": …}`).
    Hook {
        event: String,
        #[serde(default)]
        session_id: Option<String>,
    },
    /// Print the prompt like `ask`, then read a review prompt typed by `comment.send_to_agent`
    /// (SPEC §12) holding `comments` comments: append `Addressed <id>: <body>` per comment to
    /// the named file and run `mk comment reply <id> …` for each. Only each comment's first
    /// body line is read.
    ReplyComments {
        comments: usize,
        #[serde(default = "default_prompt")]
        text: String,
    },
    /// Stop the scenario and exit with `code`.
    Exit {
        code: i32,
    },
}

fn default_prompt() -> String {
    DEFAULT_PROMPT.to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Color {
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    Gray,
}

impl Color {
    const CYCLE: [Color; 6] = [
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
    ];

    fn sgr(self) -> &'static str {
        match self {
            Color::Red => "\x1b[31m",
            Color::Green => "\x1b[32m",
            Color::Yellow => "\x1b[33m",
            Color::Blue => "\x1b[34m",
            Color::Magenta => "\x1b[35m",
            Color::Cyan => "\x1b[36m",
            Color::Gray => "\x1b[90m",
        }
    }
}

/// Parse scenario TOML; `source_name` (file path or built-in name) prefixes errors.
pub fn parse(source_name: &str, text: &str) -> Result<Scenario, Error> {
    toml::from_str(text).map_err(|e| Error::Parse {
        source_name: source_name.to_owned(),
        message: e.to_string(),
    })
}

/// Resolve `spec` as a file path if it exists, else as a built-in name.
pub fn load(spec: &str) -> Result<Scenario, Error> {
    let path = Path::new(spec);
    if path.is_file() {
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.to_owned(),
            source,
        })?;
        return parse(spec, &text);
    }
    match BUILTIN.iter().find(|(name, _)| *name == spec) {
        Some((name, text)) => parse(name, text),
        None => Err(Error::UnknownScenario {
            name: spec.to_owned(),
        }),
    }
}

/// Run `scenario`, returning the process exit code. Output is flushed after every step so a
/// PTY reader sees it before the next sleep or prompt.
pub fn run(
    scenario: &Scenario,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<i32, Error> {
    for step in &scenario.steps {
        match step {
            Step::Print { text, color } => match color {
                Some(color) => writeln!(out, "{}{text}{RESET}", color.sgr())?,
                None => writeln!(out, "{text}")?,
            },
            Step::Spinner { text, ms } => spinner(out, text, Duration::from_millis(*ms))?,
            Step::Ask { text } => {
                write!(out, "{text} ")?;
                out.flush()?;
                let mut answer = String::new();
                if input.read_line(&mut answer)? == 0 {
                    return Err(Error::StdinClosed);
                }
            }
            Step::Sleep { ms } => thread::sleep(Duration::from_millis(*ms)),
            Step::Flood { lines } => {
                for (i, color) in (0..*lines).zip(Color::CYCLE.iter().cycle()) {
                    writeln!(
                        out,
                        "{}[{i:06}] flood: the quick brown fox jumps over the lazy dog{RESET}",
                        color.sgr()
                    )?;
                }
            }
            Step::WriteFile { path, content } => {
                fs::write(path, content).map_err(|source| Error::WriteFile {
                    path: path.clone(),
                    source,
                })?;
            }
            Step::Hook { event, session_id } => {
                out.flush()?;
                hook(event, session_id.as_deref())?;
            }
            Step::ReplyComments { comments, text } => {
                write!(out, "{text} ")?;
                out.flush()?;
                let review = read_review(input, *comments)?;
                address(&review)?;
                for (id, body) in &review.comments {
                    mk(&["comment", "reply", id, &format!("Addressed: {body}")])?;
                }
                writeln!(out)?;
            }
            Step::Exit { code } => {
                out.flush()?;
                return Ok(*code);
            }
        }
        out.flush()?;
    }
    Ok(0)
}

/// The parts of a typed review prompt (SPEC §12 template) the fake acts on.
#[derive(Debug, PartialEq, Eq)]
pub struct Review {
    pub file: PathBuf,
    /// `(id, first body line)` per comment, in prompt order.
    pub comments: Vec<(String, String)>,
}

/// Reads lines until the `Update <file> according to …` line, `count` comment headers
/// (`[c_…] lines …:`) and the last one's body have arrived. A Windows console drops the `\n`s
/// between the typed lines, so the prompt is parsed as one text, not line by line.
pub fn read_review(input: &mut impl BufRead, count: usize) -> Result<Review, Error> {
    let mut typed = String::new();
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Err(Error::StdinClosed);
        }
        typed.push_str(line.trim_end_matches(['\r', '\n']));
        typed.push('\n');
        if let Some(review) = parse_review(&typed, count) {
            return Ok(review);
        }
    }
}

/// `None` until `typed` holds the whole review. A body ends at a line break, two spaces (the
/// indent of a reply glued on by a Windows console, so a body with a double space is cut),
/// an escape (paste end marker) or the next header.
fn parse_review(typed: &str, count: usize) -> Option<Review> {
    // `find`, not `strip_prefix`: paste markers or other noise may come first.
    let start = typed.find("Update ")? + "Update ".len();
    let (file, mut rest) = typed[start..].split_once(" according to")?;
    let mut comments = Vec::new();
    while comments.len() < count {
        let at = rest.find("[c_")?;
        let (id, header) = rest[at + 1..].split_once(']')?;
        let colon = header.find(':')?;
        // A quoted passage may hold colons: then the header ends at `":`.
        let body_at = match header.find(", passage \"").filter(|&p| p < colon) {
            Some(p) => p + header[p..].find("\":")? + 2,
            None => colon + 1,
        };
        let body = header[body_at..].trim_start();
        let end = ["\n", "  ", "\x1b", "[c_"]
            .iter()
            .filter_map(|stop| body.find(stop))
            .min()
            .unwrap_or(body.len());
        comments.push((id.to_owned(), body[..end].trim().to_owned()));
        rest = &body[end..];
    }
    // The last body arrives on the line after its header unless the console glued them.
    let complete = comments.last().is_none_or(|(_, body)| !body.is_empty());
    complete.then(|| Review {
        file: PathBuf::from(file),
        comments,
    })
}

/// Appends `Addressed <id>: <body>` per comment to the review's file.
fn address(review: &Review) -> Result<(), Error> {
    let path = &review.file;
    let edit_error = |source| Error::EditFile {
        path: path.clone(),
        source,
    };
    let mut content = fs::read_to_string(path).map_err(edit_error)?;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    for (id, body) in &review.comments {
        content.push_str(&format!("Addressed {id}: {body}\n"));
    }
    fs::write(path, content).map_err(edit_error)
}

/// Runs `mk hook fake <event> [<json>]`.
fn hook(event: &str, session_id: Option<&str>) -> Result<(), Error> {
    let payload = session_id.map(|id| serde_json::json!({ "session_id": id }).to_string());
    let mut args = vec!["hook", "fake", event];
    args.extend(payload.as_deref());
    mk(&args)
}

/// Runs `mk <args>` and waits for it. Like a real agent, a failing command does not stop the
/// scenario (its stderr reaches the terminal); only a missing `mk` does.
fn mk(args: &[&str]) -> Result<(), Error> {
    let program = mk_program();
    Command::new(&program)
        .args(args)
        // The PTY input belongs to the agent; these `mk` commands read no stdin anyway.
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .map_err(|source| Error::Mk { program, source })?;
    Ok(())
}

/// `mk` next to this binary (a cargo target dir or an install), else `mk` on `PATH`.
fn mk_program() -> PathBuf {
    let name = format!("mk{}", std::env::consts::EXE_SUFFIX);
    std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name(&name))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| PathBuf::from("mk"))
}

fn spinner(out: &mut impl Write, text: &str, duration: Duration) -> io::Result<()> {
    let start = Instant::now();
    for frame in SPINNER_FRAMES.iter().cycle() {
        write!(out, "\r{}{frame}{RESET} {text}", Color::Cyan.sgr())?;
        out.flush()?;
        let left = duration.saturating_sub(start.elapsed());
        if left.is_zero() {
            break;
        }
        thread::sleep(left.min(Duration::from_millis(SPINNER_FRAME_MS)));
    }
    // `\x1b[2K` clears the spinner line before the final mark.
    writeln!(out, "\r\x1b[2K{}✔{RESET} {text}", Color::Green.sgr())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_str(toml: &str, stdin: &str) -> (Result<i32, Error>, String) {
        let scenario = parse("test", toml).unwrap();
        let mut out = Vec::new();
        let code = run(&scenario, &mut stdin.as_bytes(), &mut out);
        (code, String::from_utf8(out).unwrap())
    }

    #[test]
    fn builtins_parse() {
        for (name, _) in BUILTIN {
            let scenario = load(name).unwrap();
            assert!(!scenario.steps.is_empty(), "{name} has no steps");
        }
    }

    #[test]
    fn ask_builtin_prints_four_lines_after_the_answer() {
        // The state machine matches needs_input patterns over the last 4 screen lines; the
        // prompt must scroll out of that tail once answered.
        let scenario = load("ask").unwrap();
        let ask = scenario
            .steps
            .iter()
            .position(|s| matches!(s, Step::Ask { .. }))
            .unwrap();
        let prints = scenario.steps[ask + 1..]
            .iter()
            .filter(|s| matches!(s, Step::Print { .. } | Step::Spinner { .. }))
            .count();
        assert!(prints >= 4, "only {prints} lines after the prompt");
    }

    #[test]
    fn parses_every_step_kind() {
        let scenario = parse(
            "test",
            r#"
            [[step]]
            action = "print"
            text = "hi"
            color = "green"
            [[step]]
            action = "spinner"
            text = "Thinking"
            ms = 10
            [[step]]
            action = "ask"
            [[step]]
            action = "sleep"
            ms = 5
            [[step]]
            action = "flood"
            lines = 3
            [[step]]
            action = "write_file"
            path = "out.txt"
            content = "x"
            [[step]]
            action = "hook"
            event = "needs_input"
            [[step]]
            action = "hook"
            event = "busy"
            session_id = "native-1"
            [[step]]
            action = "exit"
            code = 3
            "#,
        )
        .unwrap();
        assert_eq!(scenario.steps.len(), 9);
        assert_eq!(
            scenario.steps[2],
            Step::Ask {
                text: DEFAULT_PROMPT.into()
            }
        );
        assert_eq!(
            scenario.steps[6],
            Step::Hook {
                event: "needs_input".into(),
                session_id: None
            }
        );
        assert_eq!(
            scenario.steps[7],
            Step::Hook {
                event: "busy".into(),
                session_id: Some("native-1".into())
            }
        );
        assert_eq!(scenario.steps[8], Step::Exit { code: 3 });
    }

    #[test]
    fn parse_errors_name_the_source_and_line() {
        let err = parse(
            "bad.toml",
            "[[step]]\naction = \"print\"\ntext = \"a\"\nbogus = 1\n",
        )
        .unwrap_err()
        .to_string();
        assert!(err.starts_with("bad.toml: "), "{err}");
        assert!(err.contains("bogus"), "{err}");
        assert!(err.contains("line "), "{err}");

        let err = parse("bad.toml", "[[step]]\naction = \"dance\"\n")
            .unwrap_err()
            .to_string();
        assert!(err.contains("dance"), "{err}");
    }

    #[test]
    fn unknown_scenario_lists_builtins() {
        let err = load("no-such-scenario").unwrap_err().to_string();
        assert!(err.contains("ask, flood, omp"), "{err}");
    }

    #[test]
    fn run_prints_asks_writes_and_exits() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("out.txt");
        let toml = format!(
            r#"
            [[step]]
            action = "print"
            text = "plain"
            [[step]]
            action = "print"
            text = "red"
            color = "red"
            [[step]]
            action = "ask"
            [[step]]
            action = "write_file"
            path = '{}'
            content = "written"
            [[step]]
            action = "exit"
            code = 7
            [[step]]
            action = "print"
            text = "never"
            "#,
            file.display()
        );
        let (code, out) = run_str(&toml, "y\n");
        assert_eq!(code.unwrap(), 7);
        assert_eq!(
            out,
            "plain\n\x1b[31mred\x1b[0m\nDo you want to proceed? (y/n) "
        );
        assert_eq!(fs::read_to_string(file).unwrap(), "written");
    }

    #[test]
    fn reply_comments_parses() {
        let scenario = parse(
            "test",
            "[[step]]\naction = \"reply_comments\"\ncomments = 2\n",
        )
        .unwrap();
        assert_eq!(
            scenario.steps,
            [Step::ReplyComments {
                comments: 2,
                text: DEFAULT_PROMPT.into()
            }]
        );
    }

    /// The prompt as `comment.send_to_agent` types it through a PTY: `\n` between lines, `\r`
    /// at the end, a reply after the first comment, bracketed-paste markers around it.
    const TYPED_REVIEW: &str = "\x1b[200~Update docs/plan.md according to the review comments below.\n\
        Only edit this file. When done, for EACH comment run:\n  \
        mk comment reply <id> \"<what you changed, or why you did not>\"\n\n\
        [c_01JX] lines 12–14, passage \"Use Redis for caching\":\n  \
        Why not an in-memory cache first?\n    \
        user replied: Also check TTLs\n\
        [c_01JY] line 40, passage \"M3: deploy\":\n  \
        Add a rollback step.\x1b[201~\r";

    #[test]
    fn read_review_takes_the_file_and_each_comment_header_and_body() {
        let expected = Review {
            file: "docs/plan.md".into(),
            comments: vec![
                ("c_01JX".into(), "Why not an in-memory cache first?".into()),
                ("c_01JY".into(), "Add a rollback step.".into()),
            ],
        };
        assert_eq!(
            read_review(&mut TYPED_REVIEW.as_bytes(), 2).unwrap(),
            expected
        );
        // A Windows console delivers it as one line: the `\n`s are dropped.
        let glued = TYPED_REVIEW.replace('\n', "") + "\n";
        assert_eq!(read_review(&mut glued.as_bytes(), 2).unwrap(), expected);
        // Fewer comments than expected: the agent would wait for more input.
        assert!(matches!(
            read_review(&mut TYPED_REVIEW.as_bytes(), 3),
            Err(Error::StdinClosed)
        ));
    }

    #[test]
    fn read_review_waits_for_the_last_body_and_skips_colons_in_passages() {
        let header_only = "Update a.md according to the review comments below.\n\
            [c_1] line 2, passage \"a: b\":\n";
        assert!(matches!(
            read_review(&mut header_only.as_bytes(), 1),
            Err(Error::StdinClosed)
        ));
        let review = read_review(&mut format!("{header_only}  Keep it\n").as_bytes(), 1).unwrap();
        assert_eq!(review.comments, [("c_1".into(), "Keep it".into())]);
    }

    #[test]
    fn address_appends_one_line_per_comment() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("plan.md");
        fs::write(&file, "# Plan").unwrap();
        let review = Review {
            file: file.clone(),
            comments: vec![
                ("c_1".into(), "Fix A".into()),
                ("c_2".into(), "Fix B".into()),
            ],
        };
        address(&review).unwrap();
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "# Plan\nAddressed c_1: Fix A\nAddressed c_2: Fix B\n"
        );
        let missing = Review {
            file: dir.path().join("missing.md"),
            comments: vec![],
        };
        assert!(matches!(address(&missing), Err(Error::EditFile { .. })));
    }

    #[test]
    fn ask_without_input_fails() {
        let (code, _) = run_str("[[step]]\naction = \"ask\"\n", "");
        assert!(matches!(code, Err(Error::StdinClosed)));
    }

    #[test]
    fn spinner_ends_with_done_line_and_flood_counts_lines() {
        let toml = r#"
            [[step]]
            action = "spinner"
            text = "Thinking"
            ms = 20
            [[step]]
            action = "flood"
            lines = 50
        "#;
        let (code, out) = run_str(toml, "");
        assert_eq!(code.unwrap(), 0);
        let lines: Vec<_> = out.lines().collect();
        assert_eq!(lines.len(), 51);
        assert!(lines[0].ends_with("\r\x1b[2K\x1b[32m✔\x1b[0m Thinking"));
        assert!(lines[50].contains("[000049] flood"));
    }
}
