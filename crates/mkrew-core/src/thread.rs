//! Readable turns from agent transcripts (thread view, ADR 0055), read incrementally.
//!
//! Pure, like [`crate::usage::UsageReader`]: the caller reads the bytes appended to the
//! transcript at [`ThreadReader::offset`] and hands them to [`ThreadReader::feed`]. A partial
//! last line is kept until its newline arrives. Unknown or malformed lines are skipped.

use serde_json::Value;

use crate::profile::UsageFormat;

/// Turns kept; older ones are dropped as new ones arrive.
pub const MAX_TURNS: usize = 20;
/// Characters of a tool call's target.
pub const TARGET_MAX: usize = 120;
/// Input fields a tool call's target is taken from; the first present wins.
const TARGET_KEYS: [&str; 5] = ["file_path", "path", "pattern", "command", "url"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub prompt: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Agent text (Markdown).
    Text(String),
    /// Consecutive tool calls.
    Tools(Vec<ToolCall>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub name: String,
    /// What it acted on (a path, pattern, command…), one line; empty if none.
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Claude,
    Codex,
    Omp,
}

/// One thing a transcript line adds.
enum Line {
    Prompt(String),
    Text(String),
    Tool(ToolCall),
}

/// Incremental reader for one transcript file.
#[derive(Debug, Clone)]
pub struct ThreadReader {
    format: Format,
    /// Bytes of the file consumed so far, including `partial`.
    offset: u64,
    partial: Vec<u8>,
    /// Set by `start_mid_file`: the bytes up to the first `\n` are the end of a cut line.
    skip_first: bool,
    turns: Vec<Turn>,
    truncated: bool,
    first_turn: u64,
}

impl ThreadReader {
    /// A reader for `format`; an unknown format is an error (like usage).
    pub fn new(format: &UsageFormat) -> Result<Self, String> {
        let format = match format {
            UsageFormat::ClaudeJsonl => Format::Claude,
            UsageFormat::CodexJsonl => Format::Codex,
            UsageFormat::OmpJsonl => Format::Omp,
            UsageFormat::Unknown(name) => {
                return Err(format!(
                    "unknown transcript format `{name}`: no thread shown"
                ));
            }
        };
        Ok(Self {
            format,
            offset: 0,
            partial: Vec::new(),
            skip_first: false,
            turns: Vec::new(),
            truncated: false,
            first_turn: 0,
        })
    }

    /// Bytes consumed so far (including a pending partial line).
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Starts reading at `offset` (the window of a long file): the first line, up to `\n`,
    /// is skipped. Turns before it are not seen, so the thread counts as truncated.
    pub fn start_mid_file(&mut self, offset: u64) {
        self.offset = offset;
        self.partial.clear();
        self.skip_first = true;
        self.truncated = true;
    }

    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    /// True when turns were dropped (window start or the turn cap).
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// Turns dropped by the cap so far (turn `i` of [`Self::turns`] is `first_turn() + i`).
    pub fn first_turn(&self) -> u64 {
        self.first_turn
    }

    /// Consumes the bytes appended at [`Self::offset`]; true if the turns changed.
    pub fn feed(&mut self, appended: &[u8]) -> bool {
        self.offset += appended.len() as u64;
        self.partial.extend_from_slice(appended);
        if self.skip_first {
            let Some(end) = self.partial.iter().position(|b| *b == b'\n') else {
                return false;
            };
            self.partial.drain(..=end);
            self.skip_first = false;
        }
        let Some(end) = self.partial.iter().rposition(|b| *b == b'\n') else {
            return false;
        };
        let rest = self.partial.split_off(end + 1);
        let complete = std::mem::replace(&mut self.partial, rest);
        let mut changed = false;
        for line in complete.split(|b| *b == b'\n') {
            let Ok(value) = serde_json::from_slice::<Value>(line) else {
                continue;
            };
            let lines = match self.format {
                Format::Claude => claude_line(&value),
                Format::Codex => codex_line(&value),
                Format::Omp => omp_line(&value),
            };
            for line in lines {
                changed |= self.push(line);
            }
        }
        changed
    }

    fn push(&mut self, line: Line) -> bool {
        let (text, call) = match line {
            Line::Prompt(prompt) => {
                self.turns.push(Turn {
                    prompt,
                    items: Vec::new(),
                });
                if self.turns.len() > MAX_TURNS {
                    self.turns.remove(0);
                    self.first_turn += 1;
                    self.truncated = true;
                }
                return true;
            }
            Line::Text(text) => (Some(text), None),
            Line::Tool(call) => (None, Some(call)),
        };
        // Lines before the first prompt in the window have no turn.
        let Some(turn) = self.turns.last_mut() else {
            return false;
        };
        match (text, call, turn.items.last_mut()) {
            (_, Some(call), Some(Item::Tools(calls))) => calls.push(call),
            (_, Some(call), _) => turn.items.push(Item::Tools(vec![call])),
            (Some(text), _, Some(Item::Text(last))) => {
                last.push_str("\n\n");
                last.push_str(&text);
            }
            (Some(text), _, _) => turn.items.push(Item::Text(text)),
            (None, None, _) => return false,
        }
        true
    }
}

/// The `text` of every block of type `kind` in `content` (or `content` itself when it is a
/// string), joined by blank lines and trimmed; `None` when empty.
fn block_text(content: &Value, kind: &str) -> Option<String> {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == kind)
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n"),
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// A block's own non-empty `text`, trimmed.
fn own_text(block: &Value) -> Option<String> {
    let text = block["text"].as_str()?.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The tool's target: the first of [`TARGET_KEYS`] in `input`, one line, cut.
fn target_of(input: &Value) -> String {
    TARGET_KEYS
        .iter()
        .find_map(|key| match &input[*key] {
            Value::String(s) => Some(one_line(s)),
            Value::Array(words) => Some(one_line(
                &words
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
            )),
            _ => None,
        })
        .unwrap_or_default()
}

/// The first non-empty line of `text`, at most [`TARGET_MAX`] characters.
fn one_line(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    line.chars().take(TARGET_MAX).collect()
}

fn tool(name: &Value, target: String) -> Line {
    Line::Tool(ToolCall {
        name: name.as_str().unwrap_or_default().to_owned(),
        target,
    })
}

fn claude_line(value: &Value) -> Vec<Line> {
    if value["isSidechain"] == true {
        return Vec::new();
    }
    let content = &value["message"]["content"];
    match value["type"].as_str() {
        Some("user") => {
            let tool_result = content
                .as_array()
                .is_some_and(|blocks| blocks.iter().any(|b| b["type"] == "tool_result"));
            if value["isMeta"] == true || tool_result {
                return Vec::new();
            }
            block_text(content, "text")
                .filter(|t| !t.starts_with("<command-") && !t.starts_with("<local-command-"))
                .map(Line::Prompt)
                .into_iter()
                .collect()
        }
        Some("assistant") => content
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| match block["type"].as_str() {
                Some("text") => own_text(block).map(Line::Text),
                Some("tool_use") => Some(tool(&block["name"], target_of(&block["input"]))),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn codex_line(value: &Value) -> Vec<Line> {
    let payload = &value["payload"];
    match (value["type"].as_str(), payload["type"].as_str()) {
        (Some("event_msg"), Some("user_message")) => block_text(&payload["message"], "text")
            .map(Line::Prompt)
            .into_iter()
            .collect(),
        (Some("response_item"), Some("message")) if payload["role"] == "assistant" => {
            block_text(&payload["content"], "output_text")
                .map(Line::Text)
                .into_iter()
                .collect()
        }
        (Some("response_item"), Some("function_call")) => {
            // `arguments` is a JSON object serialized as a string.
            let arguments = payload["arguments"]
                .as_str()
                .and_then(|a| serde_json::from_str::<Value>(a).ok())
                .unwrap_or(Value::Null);
            vec![tool(&payload["name"], target_of(&arguments))]
        }
        (Some("response_item"), Some("custom_tool_call")) => {
            // Raw input (e.g. a patch): its first line that is not the `*** Begin` header.
            let input = payload["input"].as_str().unwrap_or_default();
            let line = input
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with("*** Begin"))
                .unwrap_or("");
            vec![tool(&payload["name"], one_line(line))]
        }
        _ => Vec::new(),
    }
}

fn omp_line(value: &Value) -> Vec<Line> {
    if value["type"] != "message" {
        return Vec::new();
    }
    let message = &value["message"];
    let content = &message["content"];
    match message["role"].as_str() {
        Some("user") => block_text(content, "text")
            .map(Line::Prompt)
            .into_iter()
            .collect(),
        Some("assistant") => content
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|block| match block["type"].as_str() {
                Some("text") => own_text(block).map(Line::Text),
                Some("toolCall") => Some(tool(&block["name"], target_of(&block["arguments"]))),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &[u8] = include_bytes!("../tests/fixtures/thread/claude.jsonl");
    const CODEX: &[u8] = include_bytes!("../tests/fixtures/thread/codex.jsonl");
    const OMP: &[u8] = include_bytes!("../tests/fixtures/thread/omp.jsonl");

    fn tool(name: &str, target: &str) -> ToolCall {
        ToolCall {
            name: name.into(),
            target: target.into(),
        }
    }

    fn text(t: &str) -> Item {
        Item::Text(t.into())
    }

    fn read(format: UsageFormat, bytes: &[u8]) -> ThreadReader {
        let mut reader = ThreadReader::new(&format).unwrap();
        reader.feed(bytes);
        reader
    }

    /// Thinking, tool results, sidechain, meta and slash-command lines are left out; lines
    /// before the first prompt are dropped; consecutive tool calls form one group.
    #[test]
    fn claude_turns() {
        let reader = read(UsageFormat::ClaudeJsonl, CLAUDE);
        assert_eq!(
            reader.turns(),
            [
                Turn {
                    prompt: "Fix the login bug".into(),
                    items: vec![
                        text("Looking at **auth**."),
                        Item::Tools(vec![tool("Read", "src/auth.rs"), tool("Grep", "src")]),
                        text("Fixed it."),
                    ],
                },
                Turn {
                    prompt: "Now add a test".into(),
                    items: vec![Item::Tools(vec![tool("Bash", "cargo test")])],
                },
            ]
        );
        assert!(!reader.truncated());
        assert_eq!(reader.offset(), CLAUDE.len() as u64);
    }

    /// Prompts come from `user_message` events (not the injected user messages); string and
    /// array commands both give a one-line target; `apply_patch` skips its `*** Begin` line.
    #[test]
    fn codex_turns() {
        let reader = read(UsageFormat::CodexJsonl, CODEX);
        assert_eq!(
            reader.turns(),
            [
                Turn {
                    prompt: "Add retries".into(),
                    items: vec![
                        text("I'll add retries."),
                        Item::Tools(vec![
                            tool("shell_command", "rg retry"),
                            tool("apply_patch", "*** Add File: a.rs"),
                        ]),
                        text("Done."),
                    ],
                },
                Turn {
                    prompt: "List files".into(),
                    items: vec![Item::Tools(vec![tool("shell", "pwsh -c ls")])],
                },
            ]
        );
    }

    #[test]
    fn omp_turns() {
        let reader = read(UsageFormat::OmpJsonl, OMP);
        assert_eq!(
            reader.turns(),
            [Turn {
                prompt: "Check the plan".into(),
                items: vec![
                    Item::Tools(vec![tool("grep", "docs"), tool("read", "docs/PLAN.md")]),
                    text("Not done yet."),
                ],
            }]
        );
    }

    /// The agent may be mid-write when we read: any split gives the same turns as one feed.
    #[test]
    fn a_line_split_across_feeds_is_read_once() {
        let whole = read(UsageFormat::ClaudeJsonl, CLAUDE);
        for at in (1..CLAUDE.len()).step_by(7) {
            let mut reader = ThreadReader::new(&UsageFormat::ClaudeJsonl).unwrap();
            reader.feed(&CLAUDE[..at]);
            reader.feed(&CLAUDE[at..]);
            assert_eq!(reader.turns(), whole.turns(), "split at {at}");
            assert_eq!(reader.offset(), CLAUDE.len() as u64);
        }
    }

    /// Starting inside the first prompt's line: that partial line is skipped, the items after
    /// it have no prompt (dropped), and the next prompt starts the first turn.
    #[test]
    fn starting_mid_file_skips_the_partial_first_line() {
        let prompt = CLAUDE
            .windows(b"Fix the login".len())
            .position(|w| w == b"Fix the login")
            .unwrap();
        let mut reader = ThreadReader::new(&UsageFormat::ClaudeJsonl).unwrap();
        reader.start_mid_file(prompt as u64);
        assert_eq!(reader.offset(), prompt as u64);
        reader.feed(&CLAUDE[prompt..]);
        let prompts: Vec<_> = reader.turns().iter().map(|t| t.prompt.as_str()).collect();
        assert_eq!(prompts, ["Now add a test"]);
        assert!(reader.truncated());
        assert_eq!(reader.offset(), CLAUDE.len() as u64);
    }

    #[test]
    fn only_the_newest_turns_are_kept() {
        let mut bytes = Vec::new();
        for i in 0..MAX_TURNS + 5 {
            bytes.extend_from_slice(
                format!(
                    "{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"p{i}\"}}]}}}}\n"
                )
                .as_bytes(),
            );
        }
        let reader = read(UsageFormat::OmpJsonl, &bytes);
        assert_eq!(reader.turns().len(), MAX_TURNS);
        assert_eq!(reader.turns()[0].prompt, "p5");
        assert_eq!(reader.first_turn(), 5);
        assert!(reader.truncated());
    }

    #[test]
    fn targets_are_one_short_line() {
        let long = "x".repeat(300);
        let line = format!(
            "{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"go\"}}]}}}}\n\
             {{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"content\":[\
             {{\"type\":\"toolCall\",\"name\":\"bash\",\"arguments\":{{\"command\":\"{long}\\nsecond\"}}}},\
             {{\"type\":\"toolCall\",\"name\":\"todo\",\"arguments\":{{\"op\":\"view\"}}}}]}}}}\n"
        );
        let reader = read(UsageFormat::OmpJsonl, line.as_bytes());
        let Item::Tools(calls) = &reader.turns()[0].items[0] else {
            panic!("{:?}", reader.turns());
        };
        assert_eq!(calls[0].target.chars().count(), TARGET_MAX);
        assert!(!calls[0].target.contains('\n'));
        assert_eq!(calls[1].target, "");
    }

    /// Lines that change nothing shown (tool output) report no change.
    #[test]
    fn feed_reports_whether_turns_changed() {
        let mut reader = ThreadReader::new(&UsageFormat::OmpJsonl).unwrap();
        let lines: Vec<&[u8]> = OMP.split_inclusive(|b| *b == b'\n').collect();
        assert!(!reader.feed(lines[0]), "session header");
        assert!(reader.feed(lines[1]), "prompt");
        assert!(reader.feed(lines[2]), "tool call");
        assert!(!reader.feed(lines[3]), "tool result");
    }

    #[test]
    fn unknown_formats_are_refused() {
        assert!(ThreadReader::new(&UsageFormat::Unknown("x".into())).is_err());
    }
}
