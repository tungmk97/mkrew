//! Readable turns from agent transcripts (thread view, ADR 0057), read incrementally.
//!
//! Pure, like [`crate::usage::UsageReader`]: the caller reads the bytes appended to the
//! transcript at [`ThreadReader::offset`] and hands them to [`ThreadReader::feed`]. A partial
//! last line is kept until its newline arrives. Unknown or malformed lines are skipped.

use serde_json::Value;

use crate::profile::UsageFormat;

/// Turns the session panel's Thread section shows (ADR 0057).
pub const THREAD_TURNS: usize = 20;
/// Turns the Chat view keeps (ADR 0104).
pub const CHAT_TURNS: usize = 50;
/// Characters of a tool call's target.
pub const TARGET_MAX: usize = 120;
/// Bytes kept of a call's arguments, of its result and of a thinking block; a longer text is
/// cut on a char boundary and ends with `… (cut)`.
pub const INPUT_MAX: usize = 4 * 1024;
pub const RESULT_MAX: usize = 8 * 1024;
pub const THINKING_MAX: usize = 8 * 1024;
/// What follows a text that was cut.
const CUT: &str = "… (cut)";
/// Input fields a tool call's target is taken from; the first present wins.
const TARGET_KEYS: [&str; 6] = ["file_path", "path", "pattern", "command", "url", "query"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Turn {
    pub prompt: String,
    pub items: Vec<Item>,
    /// The latest Claude `TodoWrite` input or Codex `update_plan` arguments of the turn.
    pub plan: Option<Vec<PlanStep>>,
    /// The last `AskUserQuestion` of the turn that has no result yet.
    pub question: Option<Question>,
    pub subagents: Vec<Subagent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Agent text (Markdown).
    Text(String),
    /// A thinking block (Claude, omp), cut to [`THINKING_MAX`].
    Thinking(String),
    /// Consecutive tool calls.
    Tools(Vec<ToolCall>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    /// The tool_use / call id; empty if the line had none.
    pub id: String,
    pub name: String,
    /// What it acted on (a path, pattern, command…), one line; empty if none.
    pub target: String,
    /// The arguments as pretty JSON (a patch or other raw text as it is), cut to
    /// [`INPUT_MAX`]; empty if none.
    pub input: String,
    /// Set when the line carrying the call's result arrives.
    pub result: Option<ToolResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    /// Cut to [`RESULT_MAX`], trailing blanks trimmed.
    pub text: String,
    pub is_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStatus {
    Pending,
    InProgress,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanStep {
    pub text: String,
    pub status: PlanStatus,
}

/// A question the agent asked (Claude `AskUserQuestion`) that has no result yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The tool_use id, to match its result.
    pub id: String,
    pub text: String,
    pub options: Vec<String>,
}

/// A subagent the turn started (Claude `Task` / `Agent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subagent {
    pub id: String,
    pub description: String,
    pub subagent_type: String,
    /// Its result arrived.
    pub done: bool,
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
    Thinking(String),
    Tool(ToolCall),
    Plan(Vec<PlanStep>),
    Question(Question),
    Subagent(Subagent),
    /// The result of the call `id`.
    Result {
        id: String,
        result: ToolResult,
    },
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
    max_turns: usize,
}

impl ThreadReader {
    /// A reader for `format` keeping the newest `max_turns` turns (at least one); an unknown
    /// format is an error (like usage).
    pub fn new(format: &UsageFormat, max_turns: usize) -> Result<Self, String> {
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
            max_turns: max_turns.max(1),
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
        let line = match line {
            Line::Prompt(prompt) => {
                self.turns.push(Turn {
                    prompt,
                    ..Turn::default()
                });
                if self.turns.len() > self.max_turns {
                    self.turns.remove(0);
                    self.first_turn += 1;
                    self.truncated = true;
                }
                return true;
            }
            line => line,
        };
        // Lines before the first prompt in the window have no turn.
        let Some(turn) = self.turns.last_mut() else {
            return false;
        };
        match line {
            // Handled above.
            Line::Prompt(_) => false,
            Line::Text(text) => {
                match turn.items.last_mut() {
                    Some(Item::Text(last)) => {
                        last.push_str("\n\n");
                        last.push_str(&text);
                    }
                    _ => turn.items.push(Item::Text(text)),
                }
                true
            }
            Line::Thinking(text) => {
                turn.items.push(Item::Thinking(text));
                true
            }
            Line::Tool(call) => {
                match turn.items.last_mut() {
                    Some(Item::Tools(calls)) => calls.push(call),
                    _ => turn.items.push(Item::Tools(vec![call])),
                }
                true
            }
            Line::Plan(steps) => {
                turn.plan = Some(steps);
                true
            }
            Line::Question(question) => {
                turn.question = Some(question);
                true
            }
            Line::Subagent(subagent) => {
                turn.subagents.push(subagent);
                true
            }
            Line::Result { id, result } => turn.set_result(&id, result),
        }
    }
}

impl Turn {
    /// Fills the call `id` of this turn with `result`; false (nothing changes) when the turn
    /// has no such call.
    fn set_result(&mut self, id: &str, result: ToolResult) -> bool {
        if id.is_empty() {
            return false;
        }
        let mut filled = false;
        for item in self.items.iter_mut().rev() {
            if let Item::Tools(calls) = item
                && let Some(call) = calls.iter_mut().rev().find(|call| call.id == id)
            {
                call.result = Some(result);
                filled = true;
                break;
            }
        }
        if !filled {
            return false;
        }
        if self.question.as_ref().is_some_and(|q| q.id == id) {
            self.question = None;
        }
        for subagent in self.subagents.iter_mut().filter(|s| s.id == id) {
            subagent.done = true;
        }
        true
    }
}

/// `text` cut to at most `max` bytes on a char boundary, with [`CUT`] after a cut.
fn clip(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let end = (0..=max)
        .rev()
        .find(|&at| text.is_char_boundary(at))
        .unwrap_or(0);
    format!("{}{CUT}", &text[..end])
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

/// The tool's target: the first of [`TARGET_KEYS`] in `input`, one line, cut. Failing that,
/// omp's `input` string (`[path#hash]` header of an edit, or plain text) as one line.
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
        .or_else(|| input["input"].as_str().map(edit_header))
        .unwrap_or_default()
}

/// [`one_line`] of `text` without a leading `[` and a trailing `#hash]`/`]`.
fn edit_header(text: &str) -> String {
    let line = one_line(text);
    let line = line.strip_prefix('[').unwrap_or(&line);
    let line = match line.rsplit_once('#') {
        Some((path, hash)) if hash.ends_with(']') => path,
        _ => line.strip_suffix(']').unwrap_or(line),
    };
    line.to_owned()
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

fn tool(id: &Value, name: &Value, target: String, input: &Value) -> Line {
    Line::Tool(ToolCall {
        id: id.as_str().unwrap_or_default().to_owned(),
        name: name.as_str().unwrap_or_default().to_owned(),
        target,
        input: input_text(input),
        result: None,
    })
}

/// The arguments as shown: pretty JSON, or the raw text of a patch; cut.
fn input_text(input: &Value) -> String {
    match input {
        Value::Null => String::new(),
        Value::String(text) => clip(text, INPUT_MAX),
        other => clip(
            &serde_json::to_string_pretty(other).unwrap_or_default(),
            INPUT_MAX,
        ),
    }
}

fn thinking(block: &Value) -> Option<Line> {
    let text = block["thinking"].as_str()?.trim();
    (!text.is_empty()).then(|| Line::Thinking(clip(text, THINKING_MAX)))
}

fn result(id: &Value, text: &str, is_error: bool) -> Line {
    Line::Result {
        id: id.as_str().unwrap_or_default().to_owned(),
        result: ToolResult {
            text: clip(text.trim_end(), RESULT_MAX),
            is_error,
        },
    }
}

/// A result's `content`: a string, or the `text` of its blocks joined by newlines.
fn result_text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// A Codex tool output: a JSON string `{"output", "metadata": {"exit_code"}}`, or plain text.
/// A non-zero exit code makes it an error: from the metadata, or from the text's
/// `Exit code: N` / `Process exited with code N` line. An array output (content items) is
/// read like a result's `content`.
fn codex_output(output: &Value) -> (String, bool) {
    let Some(raw) = output.as_str() else {
        return (result_text(output), false);
    };
    if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(raw)
        && let Some(text) = map.get("output").and_then(Value::as_str)
    {
        let failed = map
            .get("metadata")
            .and_then(|metadata| metadata["exit_code"].as_i64())
            .is_some_and(|code| code != 0);
        return (text.to_owned(), failed);
    }
    let failed = raw.lines().any(|line| {
        ["Exit code: ", "Process exited with code "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
            .and_then(|rest| rest.trim().parse::<i64>().ok())
            .is_some_and(|code| code != 0)
    });
    (raw.to_owned(), failed)
}

/// The trimmed text between `<name>` and `</name>`, when non-empty.
fn tag<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let rest = text.split_once(&format!("<{name}>"))?.1;
    let inner = rest.split_once(&format!("</{name}>"))?.0.trim();
    (!inner.is_empty()).then_some(inner)
}

/// A plan from `steps` (an array of objects whose `text_key` is the step and `status` one of
/// `pending`, `in_progress`, `completed`); `None` when empty.
fn plan_of(steps: &Value, text_key: &str) -> Option<Vec<PlanStep>> {
    let steps: Vec<PlanStep> = steps
        .as_array()?
        .iter()
        .filter_map(|step| {
            let text = step[text_key].as_str()?.trim();
            (!text.is_empty()).then(|| PlanStep {
                text: text.to_owned(),
                status: match step["status"].as_str() {
                    Some("in_progress") => PlanStatus::InProgress,
                    Some("completed") => PlanStatus::Done,
                    _ => PlanStatus::Pending,
                },
            })
        })
        .collect();
    (!steps.is_empty()).then_some(steps)
}

fn question_of(id: &str, input: &Value) -> Option<Question> {
    // ponytail: the first of several questions; the card shows one. Add the rest if Claude
    // asks several at once often enough to matter.
    let first = input["questions"].as_array()?.first()?;
    let text = first["question"].as_str()?.trim();
    (!text.is_empty()).then(|| Question {
        id: id.to_owned(),
        text: text.to_owned(),
        options: first["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|option| option["label"].as_str())
            .map(str::to_owned)
            .collect(),
    })
}

/// The call itself, then what its name adds: a plan, the question asked, a subagent.
fn claude_tool(block: &Value) -> Vec<Line> {
    let (id, name, input) = (&block["id"], &block["name"], &block["input"]);
    let mut lines = vec![tool(id, name, target_of(input), input)];
    let id = id.as_str().unwrap_or_default();
    match name.as_str() {
        Some("TodoWrite") => lines.extend(plan_of(&input["todos"], "content").map(Line::Plan)),
        Some("AskUserQuestion") => lines.extend(question_of(id, input).map(Line::Question)),
        Some("Task" | "Agent") => lines.push(Line::Subagent(Subagent {
            id: id.to_owned(),
            description: input["description"].as_str().unwrap_or_default().to_owned(),
            subagent_type: input["subagent_type"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            done: false,
        })),
        _ => {}
    }
    lines
}

fn claude_line(value: &Value) -> Vec<Line> {
    if value["isSidechain"] == true {
        return Vec::new();
    }
    let content = &value["message"]["content"];
    match value["type"].as_str() {
        Some("user") => {
            let results: Vec<Line> = content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b["type"] == "tool_result")
                .map(|b| {
                    result(
                        &b["tool_use_id"],
                        &result_text(&b["content"]),
                        b["is_error"] == true,
                    )
                })
                .collect();
            if !results.is_empty() {
                return results;
            }
            if value["isMeta"] == true {
                return Vec::new();
            }
            let Some(text) = block_text(content, "text") else {
                return Vec::new();
            };
            // Injected lines: compaction summary, background-task and SDK-peer messages,
            // interrupts, command stdout.
            let injected = value["isCompactSummary"] == true
                || value["origin"]["kind"]
                    .as_str()
                    .is_some_and(|k| k != "human")
                || text.starts_with("<task-notification>")
                || text.starts_with("[Request interrupted")
                || text.starts_with("<local-command-");
            if injected {
                return Vec::new();
            }
            // A slash command or skill: `/name args`.
            if text.starts_with("<command-") {
                let name = tag(&text, "command-name");
                let args = tag(&text, "command-args");
                return name
                    .map(|name| {
                        Line::Prompt(
                            format!("{name} {args}", args = args.unwrap_or(""))
                                .trim()
                                .to_owned(),
                        )
                    })
                    .into_iter()
                    .collect();
            }
            vec![Line::Prompt(text)]
        }
        Some("assistant") => content
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|block| match block["type"].as_str() {
                Some("text") => own_text(block).map(Line::Text).into_iter().collect(),
                Some("thinking") => thinking(block).into_iter().collect(),
                Some("tool_use") => claude_tool(block),
                _ => Vec::new(),
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
            let raw = payload["arguments"].as_str().unwrap_or_default();
            let arguments = serde_json::from_str::<Value>(raw)
                .unwrap_or_else(|_| Value::String(raw.to_owned()));
            let mut lines = vec![tool(
                &payload["call_id"],
                &payload["name"],
                target_of(&arguments),
                &arguments,
            )];
            if payload["name"] == "update_plan" {
                lines.extend(plan_of(&arguments["plan"], "step").map(Line::Plan));
            }
            lines
        }
        (Some("response_item"), Some("custom_tool_call")) => {
            // Raw input (e.g. a patch): its first line that is not the `*** Begin` header.
            let input = payload["input"].as_str().unwrap_or_default();
            let line = input
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with("*** Begin"))
                .unwrap_or("");
            vec![tool(
                &payload["call_id"],
                &payload["name"],
                one_line(line),
                &payload["input"],
            )]
        }
        (Some("response_item"), Some("function_call_output" | "custom_tool_call_output")) => {
            let (text, failed) = codex_output(&payload["output"]);
            vec![result(&payload["call_id"], &text, failed)]
        }
        _ => Vec::new(),
    }
}

fn omp_line(value: &Value) -> Vec<Line> {
    if value["type"] == "custom_message" && value["customType"] == "skill-prompt" {
        let details = &value["details"];
        let Some(name) = details["name"].as_str() else {
            return Vec::new();
        };
        let args = details["args"]
            .as_str()
            .and_then(|a| a.lines().map(str::trim).find(|l| !l.is_empty()));
        let prompt = format!("/skill:{name} {}", args.unwrap_or(""));
        return vec![Line::Prompt(prompt.trim().to_owned())];
    }
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
            .flat_map(|block| match block["type"].as_str() {
                Some("text") => own_text(block).map(Line::Text).into_iter().collect(),
                Some("thinking") => thinking(block).into_iter().collect(),
                Some("toolCall") => vec![tool(
                    &block["id"],
                    &block["name"],
                    target_of(&block["arguments"]),
                    &block["arguments"],
                )],
                _ => Vec::new(),
            })
            .collect(),
        Some("toolResult") => vec![result(
            &message["toolCallId"],
            &result_text(content),
            message["isError"] == true,
        )],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &[u8] = include_bytes!("../tests/fixtures/thread/claude.jsonl");
    const CODEX: &[u8] = include_bytes!("../tests/fixtures/thread/codex.jsonl");
    const OMP: &[u8] = include_bytes!("../tests/fixtures/thread/omp.jsonl");
    const CLAUDE_CHAT: &[u8] = include_bytes!("../tests/fixtures/thread/claude_chat.jsonl");
    const CODEX_CHAT: &[u8] = include_bytes!("../tests/fixtures/thread/codex_chat.jsonl");
    const OMP_CHAT: &[u8] = include_bytes!("../tests/fixtures/thread/omp_chat.jsonl");

    fn tool(name: &str, target: &str) -> ToolCall {
        ToolCall {
            id: String::new(),
            name: name.into(),
            target: target.into(),
            input: String::new(),
            result: None,
        }
    }

    fn text(t: &str) -> Item {
        Item::Text(t.into())
    }

    fn turn(prompt: &str, items: Vec<Item>) -> Turn {
        Turn {
            prompt: prompt.into(),
            items,
            ..Turn::default()
        }
    }

    fn read(format: UsageFormat, bytes: &[u8]) -> ThreadReader {
        let mut reader = ThreadReader::new(&format, THREAD_TURNS).unwrap();
        reader.feed(bytes);
        reader
    }

    /// The turns as the first version of the reader gave them: no thinking, no call details.
    fn plain(reader: &ThreadReader) -> Vec<Turn> {
        reader
            .turns()
            .iter()
            .map(|t| {
                let items = t.items.iter().filter_map(|item| match item {
                    Item::Thinking(_) => None,
                    Item::Text(text) => Some(Item::Text(text.clone())),
                    Item::Tools(calls) => Some(Item::Tools(
                        calls.iter().map(|c| tool(&c.name, &c.target)).collect(),
                    )),
                });
                turn(&t.prompt, items.collect())
            })
            .collect()
    }

    /// Every call of the turn, in order.
    fn calls(turn: &Turn) -> Vec<&ToolCall> {
        turn.items
            .iter()
            .filter_map(|item| match item {
                Item::Tools(calls) => Some(calls.iter()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn prompts(count: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        for i in 0..count {
            bytes.extend_from_slice(
                format!(
                    "{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"p{i}\"}}]}}}}\n"
                )
                .as_bytes(),
            );
        }
        bytes
    }

    /// Tool results, sidechain, meta, command stdout and injected lines (task notification,
    /// interrupt, compaction summary, peer message) are left out of the turns; a slash command
    /// starts a turn as `/name args`; lines before the first prompt are dropped; consecutive
    /// tool calls form one group.
    #[test]
    fn claude_turns() {
        let reader = read(UsageFormat::ClaudeJsonl, CLAUDE);
        assert_eq!(
            plain(&reader),
            [
                turn(
                    "Fix the login bug",
                    vec![
                        text("Looking at **auth**."),
                        Item::Tools(vec![tool("Read", "src/auth.rs"), tool("Grep", "src")]),
                        text("Fixed it."),
                    ],
                ),
                turn(
                    "Now add a test",
                    vec![Item::Tools(vec![tool("Bash", "cargo test")])],
                ),
                turn("/context", vec![text("Still the context turn.")]),
                turn("/superpowers:writing-plans do X", vec![text("Planning.")],),
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
            plain(&reader),
            [
                turn(
                    "Add retries",
                    vec![
                        text("I'll add retries."),
                        Item::Tools(vec![
                            tool("shell_command", "rg retry"),
                            tool("apply_patch", "*** Add File: a.rs"),
                        ]),
                        text("Done."),
                    ],
                ),
                turn(
                    "List files",
                    vec![Item::Tools(vec![tool("shell", "pwsh -c ls")])],
                ),
            ]
        );
    }

    /// A skill invocation starts a turn (`/skill:name` + first line of args); an `edit` call's
    /// target is the path of its `[path#hash]` header.
    #[test]
    fn omp_turns() {
        let reader = read(UsageFormat::OmpJsonl, OMP);
        assert_eq!(
            plain(&reader),
            [
                turn(
                    "Check the plan",
                    vec![
                        Item::Tools(vec![tool("grep", "docs"), tool("read", "docs/PLAN.md")]),
                        text("Not done yet."),
                    ],
                ),
                turn(
                    "/skill:writing-plans Plan the thing",
                    vec![Item::Tools(vec![tool("edit", "docs/PLAN.md")])],
                ),
                turn("/skill:brainstorming", vec![]),
            ]
        );
    }

    #[test]
    fn claude_results_fill_their_calls_and_thinking_is_kept() {
        let reader = read(UsageFormat::ClaudeJsonl, CLAUDE_CHAT);
        let [only] = reader.turns() else {
            panic!("{:?}", reader.turns());
        };
        assert_eq!(only.prompt, "Run the tests");
        assert_eq!(
            only.items.len(),
            4,
            "thinking, text, one group, text: {only:?}"
        );
        assert_eq!(
            only.items[0],
            Item::Thinking("Check the suite first.".into())
        );
        assert_eq!(only.items[1], text("Running them."));
        assert_eq!(only.items[3], text("Two tests fail."));
        let calls = calls(only);
        assert_eq!(calls.len(), 2);
        assert_eq!(
            (
                calls[0].id.as_str(),
                calls[0].name.as_str(),
                calls[0].target.as_str()
            ),
            ("b1", "Bash", "cargo test")
        );
        assert!(
            calls[0].input.contains("\"command\": \"cargo test\""),
            "{}",
            calls[0].input
        );
        assert_eq!(
            calls[0].result,
            Some(ToolResult {
                text: "2 failed".into(),
                is_error: true
            })
        );
        assert_eq!(
            calls[1].result,
            Some(ToolResult {
                text: "fn main() {}".into(),
                is_error: false
            }),
            "an array of text blocks, trailing newline trimmed"
        );
    }

    #[test]
    fn codex_outputs_fill_their_calls() {
        let reader = read(UsageFormat::CodexJsonl, CODEX_CHAT);
        let calls = calls(&reader.turns()[0]);
        assert_eq!(calls.len(), 6);
        assert_eq!(calls[0].id, "k1");
        assert!(calls[0].input.contains("\"command\": \"cargo build\""));
        assert_eq!(
            calls[0].result,
            Some(ToolResult {
                text: "error: boom".into(),
                is_error: true
            }),
            "a non-zero exit code is an error"
        );
        assert_eq!(
            calls[1].result,
            Some(ToolResult {
                text: "a.rs\nb.rs".into(),
                is_error: false
            }),
            "plain text output"
        );
        assert_eq!(calls[2].input, "*** Begin Patch\n*** Add File: a.rs\n");
        assert_eq!(
            calls[2].result,
            Some(ToolResult {
                text: "Success".into(),
                is_error: false
            })
        );
        assert_eq!(
            calls[3].result,
            Some(ToolResult {
                text: "Exit code: 0\nWall time: 0.1 seconds\nOutput:\nok".into(),
                is_error: false
            }),
            "exit code 0 in the plain text"
        );
        assert_eq!(
            calls[4].result,
            Some(ToolResult {
                text: "Exit code: 2\nWall time: 0.3 seconds\nOutput:\nnope".into(),
                is_error: true
            }),
            "a non-zero `Exit code:` line"
        );
        assert_eq!(
            calls[5].result,
            Some(ToolResult {
                text: "one\ntwo".into(),
                is_error: false
            }),
            "an array of content items keeps its text"
        );
        let unified = codex_output(&Value::String(
            "Chunk ID: 1\nProcess exited with code 3\nOutput:\nx".into(),
        ));
        assert!(unified.1, "unified exec exit line");
    }

    #[test]
    fn omp_results_and_thinking() {
        let reader = read(UsageFormat::OmpJsonl, OMP_CHAT);
        let turn = &reader.turns()[0];
        assert_eq!(turn.items[0], Item::Thinking("hm".into()));
        let calls = calls(turn);
        assert_eq!(
            (calls[0].id.as_str(), calls[0].target.as_str()),
            ("o1", "src")
        );
        assert_eq!(
            calls[0].result,
            Some(ToolResult {
                text: "bad regex".into(),
                is_error: true
            })
        );
    }

    /// A result for an id the turn does not hold, or one before any prompt, changes nothing.
    #[test]
    fn a_result_without_its_call_is_dropped() {
        let orphan = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"x\",\"content\":\"late\"}]}}\n";
        let mut reader = ThreadReader::new(&UsageFormat::ClaudeJsonl, THREAD_TURNS).unwrap();
        assert!(!reader.feed(orphan.as_bytes()), "before any prompt");
        reader.feed(CLAUDE_CHAT);
        let before = reader.turns().to_vec();
        assert!(!reader.feed(orphan.as_bytes()), "an unknown id");
        assert_eq!(reader.turns(), before);
    }

    #[test]
    fn clip_keeps_short_text_and_marks_a_cut() {
        assert_eq!(clip("short", 5), "short");
        assert_eq!(clip("é", 1), CUT, "the first char does not fit");
        assert_eq!(clip("ééé", 3), format!("é{CUT}"));
    }

    #[test]
    fn huge_inputs_results_and_thinking_are_cut_on_a_char_boundary() {
        let (big, huge) = ("é".repeat(5000), "é".repeat(9000));
        let jsonl = format!(
            "{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"go\"}}]}}}}\n\
             {{\"type\":\"message\",\"message\":{{\"role\":\"assistant\",\"content\":[\
             {{\"type\":\"thinking\",\"thinking\":\"{huge}\"}},\
             {{\"type\":\"toolCall\",\"id\":\"h1\",\"name\":\"bash\",\"arguments\":{{\"command\":\"{big}\"}}}}]}}}}\n\
             {{\"type\":\"message\",\"message\":{{\"role\":\"toolResult\",\"toolCallId\":\"h1\",\"content\":[{{\"type\":\"text\",\"text\":\"{huge}\"}}]}}}}\n"
        );
        let reader = read(UsageFormat::OmpJsonl, jsonl.as_bytes());
        let turn = &reader.turns()[0];
        let Item::Thinking(thought) = &turn.items[0] else {
            panic!("{turn:?}");
        };
        let call = calls(turn)[0];
        let result = call.result.as_ref().unwrap();
        for (shown, max) in [
            (thought.as_str(), THINKING_MAX),
            (call.input.as_str(), INPUT_MAX),
            (result.text.as_str(), RESULT_MAX),
        ] {
            let kept = shown
                .strip_suffix(CUT)
                .unwrap_or_else(|| panic!("not cut: {} bytes", shown.len()));
            assert!(
                kept.len() <= max && kept.len() + 2 > max,
                "{} of {max}",
                kept.len()
            );
        }
    }

    #[test]
    fn input_targets_drop_the_edit_header_brackets() {
        assert_eq!(edit_header("[a/b.rs#1A2B]\nPUT 1:\n+x"), "a/b.rs");
        assert_eq!(edit_header("[a/b.rs]"), "a/b.rs");
        assert_eq!(edit_header("plain first line\nmore"), "plain first line");
        let query = serde_json::json!({"query": "rust gpui", "input": "ignored"});
        assert_eq!(target_of(&query), "rust gpui");
    }

    /// The agent may be mid-write when we read: any split gives the same turns as one feed.
    #[test]
    fn a_line_split_across_feeds_is_read_once() {
        let whole = read(UsageFormat::ClaudeJsonl, CLAUDE);
        for at in (1..CLAUDE.len()).step_by(7) {
            let mut reader = ThreadReader::new(&UsageFormat::ClaudeJsonl, THREAD_TURNS).unwrap();
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
        let mut reader = ThreadReader::new(&UsageFormat::ClaudeJsonl, THREAD_TURNS).unwrap();
        reader.start_mid_file(prompt as u64);
        assert_eq!(reader.offset(), prompt as u64);
        reader.feed(&CLAUDE[prompt..]);
        let prompts: Vec<_> = reader.turns().iter().map(|t| t.prompt.as_str()).collect();
        assert_eq!(
            prompts,
            [
                "Now add a test",
                "/context",
                "/superpowers:writing-plans do X"
            ]
        );
        assert!(reader.truncated());
        assert_eq!(reader.offset(), CLAUDE.len() as u64);
    }

    #[test]
    fn only_the_newest_turns_are_kept() {
        let reader = read(UsageFormat::OmpJsonl, &prompts(THREAD_TURNS + 5));
        assert_eq!(reader.turns().len(), THREAD_TURNS);
        assert_eq!(reader.turns()[0].prompt, "p5");
        assert_eq!(reader.first_turn(), 5);
        assert!(reader.truncated());
    }

    #[test]
    fn the_turn_limit_is_a_reader_argument() {
        let bytes = prompts(CHAT_TURNS + 5);
        let mut chat = ThreadReader::new(&UsageFormat::OmpJsonl, CHAT_TURNS).unwrap();
        chat.feed(&bytes);
        assert_eq!(chat.turns().len(), CHAT_TURNS);
        assert_eq!(chat.first_turn(), 5);
        let mut few = ThreadReader::new(&UsageFormat::OmpJsonl, 3).unwrap();
        few.feed(&bytes);
        assert_eq!(few.turns().len(), 3);
        assert_eq!(few.turns()[0].prompt, format!("p{}", CHAT_TURNS + 2));
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

    /// Lines that change nothing shown report no change; a result that fills a call does.
    #[test]
    fn feed_reports_whether_turns_changed() {
        let mut reader = ThreadReader::new(&UsageFormat::OmpJsonl, THREAD_TURNS).unwrap();
        let lines: Vec<&[u8]> = OMP.split_inclusive(|b| *b == b'\n').collect();
        assert!(!reader.feed(lines[0]), "session header");
        assert!(reader.feed(lines[1]), "prompt");
        assert!(reader.feed(lines[2]), "tool call");
        assert!(reader.feed(lines[3]), "tool result fills its call");
        let unknown = b"{\"type\":\"message\",\"message\":{\"role\":\"toolResult\",\"toolCallId\":\"nope\",\"content\":[]}}\n";
        assert!(!reader.feed(unknown), "a result of an unknown call");
    }

    #[test]
    fn unknown_formats_are_refused() {
        assert!(ThreadReader::new(&UsageFormat::Unknown("x".into()), THREAD_TURNS).is_err());
    }

    const CLAUDE_AGENTS: &[u8] = include_bytes!("../tests/fixtures/thread/claude_agents.jsonl");
    const CODEX_PLAN: &[u8] = include_bytes!("../tests/fixtures/thread/codex_plan.jsonl");

    fn step(text: &str, status: PlanStatus) -> PlanStep {
        PlanStep {
            text: text.into(),
            status,
        }
    }

    /// The first `count` lines of `bytes`.
    fn head(bytes: &[u8], count: usize) -> Vec<u8> {
        bytes
            .split_inclusive(|b| *b == b'\n')
            .take(count)
            .flatten()
            .copied()
            .collect()
    }

    #[test]
    fn the_plan_is_the_latest_todowrite_of_the_turn() {
        let reader = read(UsageFormat::ClaudeJsonl, CLAUDE_AGENTS);
        assert_eq!(
            reader.turns()[0].plan,
            Some(vec![
                step("Read code", PlanStatus::Done),
                step("Write fix", PlanStatus::Done),
                step("Run tests", PlanStatus::InProgress),
            ])
        );
        assert_eq!(reader.turns()[1].plan, None);
        // The TodoWrite call stays a (collapsed) tool row with its result.
        let first = calls(&reader.turns()[0]);
        assert_eq!(first[0].name, "TodoWrite");
        assert_eq!(
            first[0].result.as_ref().map(|r| r.text.as_str()),
            Some("Todos modified")
        );
    }

    #[test]
    fn codex_update_plan_gives_the_plan() {
        let reader = read(UsageFormat::CodexJsonl, CODEX_PLAN);
        assert_eq!(
            reader.turns()[0].plan,
            Some(vec![
                step("Design", PlanStatus::Done),
                step("Build", PlanStatus::InProgress),
                step("Ship", PlanStatus::Pending),
            ])
        );
    }

    #[test]
    fn a_question_is_pending_until_its_result_arrives() {
        let mut reader = ThreadReader::new(&UsageFormat::ClaudeJsonl, THREAD_TURNS).unwrap();
        reader.feed(&head(CLAUDE_AGENTS, 7));
        assert_eq!(
            reader.turns()[0].question,
            Some(Question {
                id: "q1".into(),
                text: "Which database?".into(),
                options: vec!["SQLite".into(), "Postgres".into()],
            })
        );
        let answer = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"q1\",\"content\":\"User has answered: SQLite\"}]}}\n";
        assert!(reader.feed(answer.as_bytes()));
        assert_eq!(reader.turns()[0].question, None);
    }

    /// The second question of the fixture is answered in the same transcript.
    #[test]
    fn an_answered_question_is_not_pending() {
        let reader = read(UsageFormat::ClaudeJsonl, CLAUDE_AGENTS);
        assert_eq!(reader.turns()[1].question, None);
        // A window that starts after the question: its result has no call and is dropped.
        let mut late = ThreadReader::new(&UsageFormat::ClaudeJsonl, THREAD_TURNS).unwrap();
        late.feed(&head(CLAUDE_AGENTS, 1));
        let answer = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"q1\",\"content\":\"x\"}]}}\n";
        assert!(!late.feed(answer.as_bytes()));
        assert_eq!(late.turns()[0].question, None);
    }

    #[test]
    fn task_and_agent_calls_are_subagents_until_their_result() {
        let mut reader = ThreadReader::new(&UsageFormat::ClaudeJsonl, THREAD_TURNS).unwrap();
        reader.feed(&head(CLAUDE_AGENTS, 7));
        let agent = |id: &str, description: &str, kind: &str, done| Subagent {
            id: id.into(),
            description: description.into(),
            subagent_type: kind.into(),
            done,
        };
        assert_eq!(
            reader.turns()[0].subagents,
            [
                agent("s1", "Explore the repo", "Explore", true),
                agent("s2", "Review the diff", "reviewer", false),
            ]
        );
        let done = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"s2\",\"content\":\"ok\"}]}}\n";
        reader.feed(done.as_bytes());
        assert!(reader.turns()[0].subagents.iter().all(|s| s.done));
    }
}
