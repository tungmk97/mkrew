//! Readable turns from agent transcripts (thread view, ADR 0057), read incrementally.
//!
//! Pure, like [`crate::usage::UsageReader`]: the caller reads the bytes appended to the
//! transcript at [`ThreadReader::offset`] and hands them to [`ThreadReader::feed`]. A partial
//! last line is kept until its newline arrives. Unknown or malformed lines are skipped.

use std::collections::HashMap;

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
pub const CUT: &str = "… (cut)";
/// Calls of one turn that keep their arguments and result; older calls keep name, target and
/// error mark only, so a long autonomous turn stays bounded (and so does every page copy).
pub const DETAILED_CALLS: usize = 50;
/// Input fields a tool call's target is taken from; the first present wins.
const TARGET_KEYS: [&str; 7] = [
    "file_path",
    "path",
    "pattern",
    "command",
    "cmd",
    "url",
    "query",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Turn {
    pub prompt: String,
    pub items: Vec<Item>,
    /// The latest Claude `TodoWrite` input or Codex `update_plan` arguments of the turn, or
    /// the session checklist of Claude's task tools after their latest change in the turn.
    /// `Some(empty)` is an emptied list.
    pub plan: Option<Vec<PlanStep>>,
    /// The last `AskUserQuestion` of the turn that has no result yet.
    pub question: Option<Question>,
    pub subagents: Vec<Subagent>,
    /// The prompt line's timestamp (epoch ms); `None` when that line has none.
    pub started_at: Option<i64>,
    /// The newest timestamp of a line of the turn (epoch ms): a later line with an older time
    /// never lowers it.
    pub ended_at: Option<i64>,
    /// Output tokens of the turn's assistant messages. Claude lines that share one
    /// `message.id` count once (the newest figure), as in the usage reader.
    pub output_tokens: u64,
    /// Milliseconds the model spent writing: for each assistant line, its time minus the time
    /// of the line before it in the turn (never negative). tok/s is `output_tokens * 1000 /
    /// generation_ms`.
    pub generation_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// Agent text (Markdown).
    Text(String),
    /// A thinking block (Claude, omp), cut to [`THINKING_MAX`]. `ms` is how long it took: the
    /// block's line time minus the line before it. Claude's redacted thinking has empty text.
    Thinking { text: String, ms: Option<u64> },
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
    /// Launched in the background (its result said so): it can outlive its turn.
    pub background: bool,
    /// The id of a background launch (`toolUseResult.agentId`), to match its notification.
    pub agent_id: Option<String>,
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
    /// The result of the call `id`, with what the entry's `toolUseResult` says (Claude).
    Result {
        id: String,
        result: ToolResult,
        structured: Option<Structured>,
    },
    /// A `TaskCreate` call: the step is added when its result gives the task id.
    TaskCreate {
        call_id: String,
        subject: String,
    },
    /// A `TaskUpdate` call.
    TaskUpdate {
        id: String,
        status: Option<String>,
        subject: Option<String>,
    },
    /// A `<task-notification>`: a background subagent ended.
    Notification {
        tool_use_id: Option<String>,
        task_id: Option<String>,
    },
}

/// A step of the checklist Claude's task tools build.
#[derive(Debug, Clone)]
struct Task {
    id: String,
    text: String,
    status: PlanStatus,
}

/// The parts of a Claude entry's `toolUseResult` the reader uses.
#[derive(Debug, Default)]
struct Structured {
    /// `task.id` of a `TaskCreate` result.
    task_id: Option<String>,
    /// `tasks` of a `TaskList` result.
    tasks: Option<Vec<Task>>,
    /// Every `tasks` entry is deleted or has an id and a subject.
    tasks_whole: bool,
    /// `agentId` of a background launch.
    agent_id: Option<String>,
    /// `status == "async_launched"`.
    async_launched: bool,
}

/// The times around the line being pushed: its own, and the newest of the line before it in
/// the turn.
#[derive(Debug, Clone, Copy, Default)]
struct Clock {
    now: Option<i64>,
    prev: Option<i64>,
}

/// What a line says about the turn's time and tokens (not about its items).
#[derive(Debug, Default)]
struct Facts {
    /// An assistant line: the model was writing since the line before.
    assistant: bool,
    /// Output tokens, with the id that de-duplicates them (a Claude `message.id`).
    output: Option<(Option<String>, u64)>,
}

/// What a transcript says about the session, the newest value seen of each (ADR 0105). A
/// reader that started mid-file (`start_mid_file`) holds only what it has seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionMeta {
    /// The model as the agent names it (see [`short_model`]).
    pub model: Option<String>,
    /// Codex `reasoning_effort`, omp `thinkingLevel`; none for Claude (only in API requests).
    pub effort: Option<String>,
    /// Claude `permissionMode`, Codex `approval_policy`; none for omp.
    pub permission: Option<String>,
    /// The agent's own title for the session, cleaned by [`clean_title`].
    pub title: Option<String>,
    /// Tokens in the context of the newest message.
    pub context: Option<u64>,
    /// Codex only: `model_context_window`.
    pub context_window: Option<u64>,
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
    /// The task tools' checklist of the session.
    checklist: Vec<Task>,
    /// `TaskCreate` calls (tool-use id, subject) whose result has not come yet.
    creates: Vec<(String, String)>,
    /// The clock of the line being read.
    clock: Clock,
    /// The newest time of the previous line of the current turn.
    last: Option<i64>,
    /// Claude `message.id` -> the output tokens it counted in this turn.
    seen: HashMap<String, u64>,
    /// The `total_tokens` of the last Codex `token_count`: an equal one is a repeat.
    codex_total: Option<u64>,
    meta: SessionMeta,
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
            checklist: Vec::new(),
            creates: Vec::new(),
            clock: Clock::default(),
            last: None,
            seen: HashMap::new(),
            codex_total: None,
            meta: SessionMeta::default(),
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
        self.last = None;
    }

    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    /// The newest model, effort, permission, title and context seen.
    pub fn meta(&self) -> &SessionMeta {
        &self.meta
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
            // A Claude sidechain line is a subagent's: it has no part in the turn's times.
            let ts = (value["isSidechain"] != true)
                .then(|| value["timestamp"].as_str().and_then(stamp_ms))
                .flatten();
            let facts = match self.format {
                Format::Claude => claude_facts(&value),
                Format::Codex => codex_facts(&value, &mut self.codex_total),
                Format::Omp => omp_facts(&value),
            };
            update_meta(&mut self.meta, self.format, &value);
            let lines = match self.format {
                Format::Claude => claude_line(&value),
                Format::Codex => codex_line(&value),
                Format::Omp => omp_line(&value),
            };
            self.clock = Clock {
                now: ts,
                prev: self.last,
            };
            for line in lines {
                changed |= self.push(line);
            }
            changed |= self.account(ts, facts);
            if ts.is_some() {
                self.last = ts;
            }
        }
        changed
    }

    fn push(&mut self, line: Line) -> bool {
        let clock = self.clock;
        let line = match line {
            Line::Prompt(prompt) => {
                self.turns.push(Turn {
                    prompt,
                    started_at: clock.now,
                    ended_at: clock.now,
                    ..Turn::default()
                });
                self.seen.clear();
                self.last = None;
                if self.turns.len() > self.max_turns {
                    self.turns.remove(0);
                    self.first_turn += 1;
                    self.truncated = true;
                }
                return true;
            }
            Line::Notification {
                tool_use_id,
                task_id,
            } => return self.notified(tool_use_id.as_deref(), task_id.as_deref()),
            line => line,
        };
        let tracked = self.track_tasks(&line);
        // Lines before the first prompt in the window have no turn.
        let Some(turn) = self.turns.last_mut() else {
            return false;
        };
        tracked
            | match line {
                // Handled above.
                Line::Prompt(_) | Line::Notification { .. } => false,
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
                    let ms = clock
                        .now
                        .zip(clock.prev)
                        .map(|(now, prev)| u64::try_from((now - prev).max(0)).unwrap_or(0));
                    turn.items.push(Item::Thinking { text, ms });
                    true
                }
                Line::Tool(call) => {
                    match turn.items.last_mut() {
                        Some(Item::Tools(calls)) => calls.push(call),
                        _ => turn.items.push(Item::Tools(vec![call])),
                    }
                    turn.drop_old_bodies();
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
                Line::TaskCreate { .. } | Line::TaskUpdate { .. } => false,
                Line::Result {
                    id,
                    result,
                    structured,
                } => {
                    let structured = structured.unwrap_or_default();
                    turn.set_result(&id, result, structured.async_launched, structured.agent_id)
                }
            }
    }

    /// Adds the time and tokens of the line just pushed to the current turn; true if it changed.
    fn account(&mut self, ts: Option<i64>, facts: Facts) -> bool {
        let clock = self.clock;
        let Some(turn) = self.turns.last_mut() else {
            return false;
        };
        let mut changed = false;
        if let Some(ts) = ts
            && turn.ended_at.is_none_or(|end| ts > end)
        {
            turn.ended_at = Some(ts);
            changed = true;
        }
        if facts.assistant
            && let Some((now, prev)) = clock.now.zip(clock.prev)
        {
            turn.generation_ms += u64::try_from((now - prev).max(0)).unwrap_or(0);
            changed = true;
        }
        if let Some((key, tokens)) = facts.output {
            let old = key
                .and_then(|key| self.seen.insert(key, tokens))
                .unwrap_or(0);
            turn.output_tokens = turn.output_tokens.saturating_sub(old) + tokens;
            changed = true;
        }
        changed
    }

    /// Applies a task-tool line to the session checklist (also before the first turn, which
    /// has nothing to copy it into yet) and copies it into the current turn's plan.
    fn track_tasks(&mut self, line: &Line) -> bool {
        match line {
            Line::TaskCreate { call_id, subject } => {
                if self.creates.len() >= crate::plan::MAX_ITEMS {
                    self.creates.remove(0);
                }
                self.creates.push((call_id.clone(), subject.clone()));
                return false;
            }
            Line::TaskUpdate {
                id,
                status,
                subject,
            } => {
                let Some(at) = self.checklist.iter().position(|t| t.id == *id) else {
                    return false;
                };
                if status.as_deref() == Some("deleted") {
                    self.checklist.remove(at);
                } else {
                    let task = &mut self.checklist[at];
                    if let Some(status) = status.as_deref().and_then(status_of) {
                        task.status = status;
                    }
                    if let Some(subject) = subject {
                        task.text.clone_from(subject);
                    }
                }
            }
            Line::Result {
                id,
                result,
                structured,
            } => {
                if let Some(at) = self.creates.iter().position(|(call, _)| call == id) {
                    let (_, subject) = self.creates.remove(at);
                    // The result text is not fixed: `#N` in it only when there is no
                    // structured output at all.
                    let task_id = match structured {
                        Some(structured) => structured.task_id.clone(),
                        None => hash_number(&result.text),
                    };
                    let Some(task_id) = task_id else {
                        return false;
                    };
                    if result.is_error
                        || self.checklist.len() >= crate::plan::MAX_ITEMS
                        || self.checklist.iter().any(|t| t.id == task_id)
                    {
                        return false;
                    }
                    self.checklist.push(Task {
                        id: task_id,
                        text: subject,
                        status: PlanStatus::Pending,
                    });
                } else if let Some(tasks) = structured.as_ref().and_then(|s| s.tasks.as_ref())
                    && !result.is_error
                    // Without a turn the call is unknown: a `tasks` list is accepted only when
                    // every entry reads, so a foreign `tasks` field cannot blank the checklist.
                    && self.turns.last().map_or(
                        structured.as_ref().is_some_and(|s| s.tasks_whole),
                        |turn| turn.call(id).is_some_and(|c| c.name == "TaskList"),
                    )
                {
                    self.checklist.clone_from(tasks);
                } else {
                    return false;
                }
            }
            _ => return false,
        }
        match self.turns.last_mut() {
            Some(turn) => turn.sync_plan(&self.checklist),
            None => false,
        }
    }

    /// Marks done the subagent a `<task-notification>` names: the one whose call id is
    /// `tool_use_id`, else the one whose `agent_id` is `task_id`, newest first over every kept
    /// turn. False (nothing changes) when none, or when it was done already.
    fn notified(&mut self, tool_use_id: Option<&str>, task_id: Option<&str>) -> bool {
        let locate = |matches: &dyn Fn(&Subagent) -> bool| {
            self.turns
                .iter()
                .enumerate()
                .rev()
                .find_map(|(t, turn)| turn.subagents.iter().rposition(matches).map(|s| (t, s)))
        };
        let at = tool_use_id
            .and_then(|id| locate(&|s| s.id == id))
            .or_else(|| task_id.and_then(|id| locate(&|s| s.agent_id.as_deref() == Some(id))));
        let Some((turn, subagent)) = at else {
            return false;
        };
        !std::mem::replace(&mut self.turns[turn].subagents[subagent].done, true)
    }
}

impl Turn {
    /// Past [`DETAILED_CALLS`] calls, strips the call that just fell out of the newest ones
    /// (name, target and error mark stay; the result becomes the cut marker).
    fn drop_old_bodies(&mut self) {
        let total: usize = self
            .items
            .iter()
            .map(|item| match item {
                Item::Tools(calls) => calls.len(),
                _ => 0,
            })
            .sum();
        let Some(mut old) = total.checked_sub(DETAILED_CALLS + 1) else {
            return;
        };
        for item in &mut self.items {
            let Item::Tools(calls) = item else { continue };
            if old >= calls.len() {
                old -= calls.len();
                continue;
            }
            let call = &mut calls[old];
            call.input.clear();
            if let Some(result) = &mut call.result {
                result.text = CUT.to_owned();
            }
            return;
        }
    }

    /// The call `id` of this turn.
    fn call(&self, id: &str) -> Option<&ToolCall> {
        self.items.iter().rev().find_map(|item| match item {
            Item::Tools(calls) => calls.iter().rev().find(|call| call.id == id),
            _ => None,
        })
    }

    /// Sets `plan` to a copy of `checklist`; false when it already was.
    fn sync_plan(&mut self, checklist: &[Task]) -> bool {
        let plan = Some(
            checklist
                .iter()
                .map(|task| PlanStep {
                    text: task.text.clone(),
                    status: task.status,
                })
                .collect(),
        );
        if self.plan == plan {
            return false;
        }
        self.plan = plan;
        true
    }

    /// Fills the call `id` of this turn with `result`; false (nothing changes) when the turn
    /// has no such call. A subagent of that id is done, unless this is a background launch
    /// (`async_launched`, or the text says so): it keeps running and records its agent id.
    fn set_result(
        &mut self,
        id: &str,
        result: ToolResult,
        async_launched: bool,
        agent_id: Option<String>,
    ) -> bool {
        if id.is_empty() {
            return false;
        }
        let mut filled = false;
        let launched =
            async_launched || result.text.trim_start().starts_with("Async agent launched");
        let agent_id = agent_id.or_else(|| launched.then(|| text_agent_id(&result.text)).flatten());
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
            if launched {
                subagent.background = true;
                subagent.agent_id.clone_from(&agent_id);
            } else {
                subagent.done = true;
            }
        }
        true
    }
}

/// The digits after the first `#` that has some (`Task #3 created`).
fn hash_number(text: &str) -> Option<String> {
    text.split('#')
        .skip(1)
        .map(|rest| {
            rest.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
        })
        .find(|digits| !digits.is_empty())
}

/// The word after `agentId:` in a launch result's text.
fn text_agent_id(text: &str) -> Option<String> {
    let mut words = text.split_whitespace();
    words.find(|word| *word == "agentId:")?;
    words.next().map(str::to_owned)
}

/// A task id: a string or a number.
fn id_str(id: &Value) -> Option<String> {
    match id {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn status_of(status: &str) -> Option<PlanStatus> {
    match status {
        "pending" => Some(PlanStatus::Pending),
        "in_progress" => Some(PlanStatus::InProgress),
        "completed" => Some(PlanStatus::Done),
        _ => None,
    }
}

/// The used parts of an entry's `toolUseResult`; `None` when it is not an object.
fn structured_of(value: &Value) -> Option<Structured> {
    value.as_object()?;
    Some(Structured {
        task_id: id_str(&value["task"]["id"]),
        tasks_whole: value["tasks"].as_array().is_some_and(|tasks| {
            tasks.iter().all(|t| {
                t["status"] == "deleted" || (id_str(&t["id"]).is_some() && t["subject"].is_string())
            })
        }),
        tasks: value["tasks"].as_array().map(|tasks| {
            tasks
                .iter()
                .filter(|task| task["status"] != "deleted")
                .filter_map(|task| {
                    Some(Task {
                        id: id_str(&task["id"])?,
                        text: crate::plan::cut(task["subject"].as_str()?.trim()),
                        status: task["status"]
                            .as_str()
                            .and_then(status_of)
                            .unwrap_or(PlanStatus::Pending),
                    })
                })
                .take(crate::plan::MAX_ITEMS)
                .collect()
        }),
        agent_id: value["agentId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_owned),
        async_launched: value["status"] == "async_launched",
    })
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

/// A thinking block. `keep_redacted`: an empty text with a signature (Claude's redacted
/// thinking) is kept as a block with empty text, so the activity row can say "Thought for 3s".
fn thinking(block: &Value, keep_redacted: bool) -> Option<Line> {
    let text = block["thinking"].as_str()?.trim();
    if text.is_empty() {
        let redacted = keep_redacted && block["signature"].as_str().is_some_and(|s| !s.is_empty());
        return redacted.then(|| Line::Thinking(String::new()));
    }
    Some(Line::Thinking(clip(text, THINKING_MAX)))
}

fn result(id: &Value, text: &str, is_error: bool) -> Line {
    Line::Result {
        id: id.as_str().unwrap_or_default().to_owned(),
        result: ToolResult {
            text: clip(text.trim_end(), RESULT_MAX),
            is_error,
        },
        structured: None,
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
    // Only the lines before `Output:` are the tool's own; a command may print the same words.
    let failed = raw
        .lines()
        .take_while(|line| *line != "Output:")
        .any(|line| {
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
/// `pending`, `in_progress`, `completed`), at most `MAX_ITEMS` steps with cut texts; `None`
/// when `steps` is not an array (an empty array is an emptied plan).
fn plan_of(steps: &Value, text_key: &str) -> Option<Vec<PlanStep>> {
    Some(
        steps
            .as_array()?
            .iter()
            .filter_map(|step| {
                let text = crate::plan::cut(step[text_key].as_str()?.trim());
                (!text.is_empty()).then(|| PlanStep {
                    text,
                    status: step["status"]
                        .as_str()
                        .and_then(status_of)
                        .unwrap_or(PlanStatus::Pending),
                })
            })
            .take(crate::plan::MAX_ITEMS)
            .collect(),
    )
}

fn question_of(id: &str, input: &Value) -> Option<Question> {
    // ponytail: the first of several questions; the card shows one. Add the rest if Claude
    // asks several at once often enough to matter.
    let first = input["questions"].as_array()?.first()?;
    let text = first["question"].as_str()?.trim();
    (!text.is_empty()).then(|| Question {
        id: id.to_owned(),
        text: crate::plan::cut(text),
        options: first["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|option| option["label"].as_str())
            .map(|label| crate::plan::cut(label.trim()))
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
        Some("TaskCreate") => lines.extend(
            input["subject"]
                .as_str()
                .map(|s| crate::plan::cut(s.trim()))
                .filter(|subject| !subject.is_empty() && !id.is_empty())
                .map(|subject| Line::TaskCreate {
                    call_id: id.to_owned(),
                    subject,
                }),
        ),
        Some("TaskUpdate") => lines.extend(id_str(&input["taskId"]).map(|task_id| {
            Line::TaskUpdate {
                id: task_id,
                status: input["status"].as_str().map(str::to_owned),
                subject: input["subject"]
                    .as_str()
                    .map(|s| crate::plan::cut(s.trim()))
                    .filter(|subject| !subject.is_empty()),
            }
        })),
        Some("Task" | "Agent") => lines.push(Line::Subagent(Subagent {
            id: id.to_owned(),
            description: input["description"].as_str().unwrap_or_default().to_owned(),
            subagent_type: input["subagent_type"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            done: false,
            background: false,
            agent_id: None,
        })),
        _ => {}
    }
    lines
}

/// A `<task-notification>` that says a background task ended (any status but `running`).
fn notification(text: &str) -> Vec<Line> {
    if tag(text, "status").is_none_or(|status| status == "running") {
        return Vec::new();
    }
    let (tool_use_id, task_id) = (tag(text, "tool-use-id"), tag(text, "task-id"));
    if tool_use_id.is_none() && task_id.is_none() {
        return Vec::new();
    }
    vec![Line::Notification {
        tool_use_id: tool_use_id.map(str::to_owned),
        task_id: task_id.map(str::to_owned),
    }]
}

fn claude_line(value: &Value) -> Vec<Line> {
    if value["isSidechain"] == true {
        return Vec::new();
    }
    let content = &value["message"]["content"];
    match value["type"].as_str() {
        Some("user") => {
            let mut results: Vec<Line> = content
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
                // The entry's `toolUseResult` belongs to its result when there is just one.
                if let [Line::Result { structured, .. }] = results.as_mut_slice() {
                    *structured = structured_of(&value["toolUseResult"]);
                }
                return results;
            }
            let text = block_text(content, "text");
            if let Some(text) = text
                .as_deref()
                .filter(|t| t.starts_with("<task-notification>"))
            {
                return notification(text);
            }
            if value["isMeta"] == true {
                return Vec::new();
            }
            let Some(text) = text else {
                return Vec::new();
            };
            // Injected lines: compaction summary, SDK-peer messages, interrupts, command
            // stdout.
            let injected = value["isCompactSummary"] == true
                || value["origin"]["kind"]
                    .as_str()
                    .is_some_and(|k| k != "human")
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
                Some("thinking") => thinking(block, true).into_iter().collect(),
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
                Some("thinking") => thinking(block, false).into_iter().collect(),
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

/// `2026-05-11T17:05:53.772Z` as epoch milliseconds; any other form is `None`.
pub(crate) fn stamp_ms(stamp: &str) -> Option<i64> {
    let seconds = crate::usage::utc_seconds(stamp)?;
    let millis = stamp
        .strip_suffix('Z')?
        .split_once('.')
        .map_or(0, |(_, fraction)| {
            let digits: String = fraction.chars().take(3).collect();
            let value: i64 = digits.parse().unwrap_or(0);
            value * 10_i64.pow(3 - u32::try_from(digits.len()).unwrap_or(3))
        });
    Some(seconds * 1000 + millis)
}

fn claude_facts(value: &Value) -> Facts {
    if value["isSidechain"] == true || value["type"] != "assistant" {
        return Facts::default();
    }
    let message = &value["message"];
    Facts {
        assistant: true,
        output: message["usage"]["output_tokens"]
            .as_u64()
            .map(|tokens| (message["id"].as_str().map(str::to_owned), tokens)),
    }
}

fn codex_facts(value: &Value, last_total: &mut Option<u64>) -> Facts {
    let payload = &value["payload"];
    match (value["type"].as_str(), payload["type"].as_str()) {
        (Some("event_msg"), Some("token_count")) => {
            let info = &payload["info"];
            let total = info["total_token_usage"]["total_tokens"].as_u64();
            let repeat = total.is_some() && total == *last_total;
            if total.is_some() {
                *last_total = total;
            }
            Facts {
                assistant: false,
                output: info["last_token_usage"]["output_tokens"]
                    .as_u64()
                    .filter(|_| !repeat)
                    .map(|tokens| (None, tokens)),
            }
        }
        (Some("response_item"), Some("message")) if payload["role"] == "assistant" => Facts {
            assistant: true,
            output: None,
        },
        // Codex writes these event lines just before their `response_item`, same time: the
        // writing gap lands on whichever comes first.
        (Some("event_msg"), Some("agent_message" | "agent_reasoning")) => Facts {
            assistant: true,
            output: None,
        },
        (Some("response_item"), Some("function_call" | "custom_tool_call" | "reasoning")) => {
            Facts {
                assistant: true,
                output: None,
            }
        }
        _ => Facts::default(),
    }
}

fn omp_facts(value: &Value) -> Facts {
    let message = &value["message"];
    if value["type"] != "message" || message["role"] != "assistant" {
        return Facts::default();
    }
    Facts {
        assistant: true,
        output: message["usage"]["output"]
            .as_u64()
            .map(|tokens| (None, tokens)),
    }
}

fn update_meta(meta: &mut SessionMeta, format: Format, value: &Value) {
    match format {
        Format::Claude => claude_meta(meta, value),
        Format::Codex => codex_meta(meta, value),
        Format::Omp => omp_meta(meta, value),
    }
}

/// Keeps `model` unless it is empty or a placeholder like `<synthetic>`.
fn set_model(slot: &mut Option<String>, model: Option<&str>) {
    if let Some(model) = model.filter(|m| !m.is_empty() && !m.starts_with('<'))
        && slot.as_deref() != Some(model)
    {
        *slot = Some(model.to_owned());
    }
}

fn set_title(slot: &mut Option<String>, raw: Option<&str>) {
    let title = clean_title(raw.unwrap_or_default());
    if !title.is_empty() {
        *slot = Some(title);
    }
}

fn claude_meta(meta: &mut SessionMeta, value: &Value) {
    if value["isSidechain"] == true {
        return;
    }
    match value["type"].as_str() {
        Some("ai-title") => set_title(&mut meta.title, value["aiTitle"].as_str()),
        Some("user" | "permission-mode") => {
            if let Some(mode) = value["permissionMode"].as_str() {
                meta.permission = Some(mode.to_owned());
            }
        }
        Some("assistant") => {
            let message = &value["message"];
            set_model(&mut meta.model, message["model"].as_str());
            let usage = &message["usage"];
            let count = |key: &str| usage[key].as_u64().unwrap_or(0);
            let context = count("input_tokens")
                + count("cache_read_input_tokens")
                + count("cache_creation_input_tokens");
            if context > 0 {
                meta.context = Some(context);
            }
        }
        _ => {}
    }
}

fn codex_meta(meta: &mut SessionMeta, value: &Value) {
    let payload = &value["payload"];
    match (value["type"].as_str(), payload["type"].as_str()) {
        (Some("turn_context"), _) => {
            set_model(&mut meta.model, payload["model"].as_str());
            meta.effort = payload["collaboration_mode"]["settings"]["reasoning_effort"]
                .as_str()
                .map(str::to_owned);
            meta.permission = payload["approval_policy"].as_str().map(str::to_owned);
        }
        (Some("event_msg"), Some("token_count")) if !payload["info"].is_null() => {
            let info = &payload["info"];
            if let Some(context) = info["last_token_usage"]["total_tokens"].as_u64() {
                meta.context = Some(context);
            }
            if let Some(window) = info["model_context_window"].as_u64() {
                meta.context_window = Some(window);
            }
        }
        _ => {}
    }
}

fn omp_meta(meta: &mut SessionMeta, value: &Value) {
    match value["type"].as_str() {
        Some("model_change") => set_model(&mut meta.model, value["model"].as_str()),
        Some("thinking_level_change") => {
            if let Some(level) = value["thinkingLevel"].as_str() {
                meta.effort = Some(level.to_owned());
            }
        }
        Some("title" | "title_change") => set_title(&mut meta.title, value["title"].as_str()),
        Some("message") if value["message"]["role"] == "assistant" => {
            let message = &value["message"];
            set_model(&mut meta.model, message["model"].as_str());
            let usage = &message["usage"];
            let count = |key: &str| usage[key].as_u64().unwrap_or(0);
            let context = count("input") + count("cacheRead") + count("cacheWrite");
            if context > 0 {
                meta.context = Some(context);
            }
        }
        _ => {}
    }
}

/// Longest model name the composer chip shows.
const MODEL_MAX: usize = 24;

/// A model name for a chip: the part after the last `/`, `claude-` and a trailing `-YYYYMMDD`
/// dropped, a dash between digits a dot and any other dash a space (`anthropic/claude-opus-5-5`
/// is `opus 5.5`), cut to [`MODEL_MAX`] characters with `…`.
pub fn short_model(name: &str) -> String {
    let base = name.rsplit('/').next().unwrap_or(name).trim();
    let base = base.strip_prefix("claude-").unwrap_or(base);
    let base = match base.rsplit_once('-') {
        Some((head, tail)) if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => base,
    };
    let chars: Vec<char> = base.chars().collect();
    let mut out = String::with_capacity(base.len());
    for (at, c) in chars.iter().enumerate() {
        if *c == '-' {
            let between_digits = at > 0
                && chars[at - 1].is_ascii_digit()
                && chars.get(at + 1).is_some_and(char::is_ascii_digit);
            out.push(if between_digits { '.' } else { ' ' });
        } else {
            out.push(*c);
        }
    }
    if out.chars().count() > MODEL_MAX {
        out = out.chars().take(MODEL_MAX - 1).collect::<String>() + "…";
    }
    out
}

/// A session title without the private-use glyph omp puts first (a Nerd Font icon) and
/// without edge blanks.
pub fn clean_title(raw: &str) -> String {
    let private = |c: char| {
        matches!(
            c,
            '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}' | '\u{100000}'..='\u{10FFFD}'
        )
    };
    raw.trim_start_matches(|c: char| private(c) || c.is_whitespace())
        .trim_end()
        .to_owned()
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
    const CLAUDE_TIMING: &[u8] = include_bytes!("../tests/fixtures/thread/claude_timing.jsonl");
    const CODEX_TIMING: &[u8] = include_bytes!("../tests/fixtures/thread/codex_timing.jsonl");
    const OMP_TIMING: &[u8] = include_bytes!("../tests/fixtures/thread/omp_timing.jsonl");
    /// 2026-01-01T00:00:00Z in epoch milliseconds.
    const T0: i64 = 1_767_225_600_000;

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
                    Item::Thinking { .. } => None,
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
            Item::Thinking {
                text: "Check the suite first.".into(),
                ms: None
            }
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
        assert_eq!(
            turn.items[0],
            Item::Thinking {
                text: "hm".into(),
                ms: None
            }
        );
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
        let Item::Thinking { text: thought, .. } = &turn.items[0] else {
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

    /// A question left unanswered when a new prompt arrives stays on its old turn; the new
    /// last turn has none. An answered one (second turn of the fixture) is not pending.
    #[test]
    fn an_answered_question_is_not_pending() {
        let reader = read(UsageFormat::ClaudeJsonl, CLAUDE_AGENTS);
        assert_eq!(reader.turns()[1].question, None);
        let mut reader = ThreadReader::new(&UsageFormat::ClaudeJsonl, THREAD_TURNS).unwrap();
        reader.feed(&head(CLAUDE_AGENTS, 8));
        let [first, second] = reader.turns() else {
            panic!("{:?}", reader.turns());
        };
        assert_eq!(second.prompt, "Second turn");
        assert_eq!(first.question.as_ref().map(|q| q.id.as_str()), Some("q1"));
        assert_eq!(second.question, None);
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
            background: false,
            agent_id: None,
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

    const CLAUDE_TASKS: &[u8] = include_bytes!("../tests/fixtures/thread/claude_tasks.jsonl");
    const CLAUDE_BACKGROUND: &[u8] =
        include_bytes!("../tests/fixtures/thread/claude_background.jsonl");

    /// Lines `from..to` (0-based) of `bytes`.
    fn slice(bytes: &[u8], from: usize, to: usize) -> Vec<u8> {
        bytes
            .split_inclusive(|b| *b == b'\n')
            .skip(from)
            .take(to - from)
            .flatten()
            .copied()
            .collect()
    }

    fn claude() -> ThreadReader {
        ThreadReader::new(&UsageFormat::ClaudeJsonl, THREAD_TURNS).unwrap()
    }

    /// The checklist is built from create results (the id from `toolUseResult`, else `#N` in
    /// the text), updated, deleted, replaced by a list result and kept across turns.
    #[test]
    fn task_tools_keep_one_checklist() {
        let mut reader = claude();
        let plan = |reader: &ThreadReader, turn: usize| reader.turns()[turn].plan.clone();
        reader.feed(&slice(CLAUDE_TASKS, 0, 2));
        assert_eq!(plan(&reader, 0), None, "a create without its result");
        reader.feed(&slice(CLAUDE_TASKS, 2, 3));
        assert_eq!(
            plan(&reader, 0),
            Some(vec![step("Design", PlanStatus::Pending)]),
            "toolUseResult.task.id wins over `#7` in the text"
        );
        reader.feed(&slice(CLAUDE_TASKS, 3, 7));
        assert_eq!(
            plan(&reader, 0),
            Some(vec![
                step("Design", PlanStatus::Pending),
                step("Build", PlanStatus::Pending),
                step("Ship", PlanStatus::Pending),
            ]),
            "the last id comes from `#3` in the text: no toolUseResult"
        );
        reader.feed(&slice(CLAUDE_TASKS, 7, 9));
        assert_eq!(
            plan(&reader, 0),
            Some(vec![
                step("Design", PlanStatus::Done),
                step("Build", PlanStatus::InProgress),
                step("Ship", PlanStatus::Pending),
            ])
        );
        assert!(reader.feed(&slice(CLAUDE_TASKS, 9, 10)), "a delete");
        let two = Some(vec![
            step("Design", PlanStatus::Done),
            step("Build", PlanStatus::InProgress),
        ]);
        assert_eq!(plan(&reader, 0), two);
        // The call row is new; the unknown id leaves the checklist as it is.
        reader.feed(&slice(CLAUDE_TASKS, 10, 11));
        assert_eq!(plan(&reader, 0), two);

        reader.feed(&slice(CLAUDE_TASKS, 11, 12));
        assert_eq!(plan(&reader, 1), None, "the new turn's plan is not set yet");
        reader.feed(&slice(CLAUDE_TASKS, 12, 14));
        assert_eq!(
            plan(&reader, 1),
            Some(vec![
                step("Build", PlanStatus::Done),
                step("Test", PlanStatus::Pending),
            ]),
            "a list result replaces the checklist"
        );
        assert_eq!(plan(&reader, 0), two, "the old turn keeps its plan");
        reader.feed(&slice(CLAUDE_TASKS, 14, 15));
        assert_eq!(
            plan(&reader, 1),
            Some(vec![
                step("Build", PlanStatus::Done),
                step("Test it", PlanStatus::InProgress),
            ]),
            "an update can rename"
        );
    }

    /// An update in a later turn applies to a task created in an earlier one.
    #[test]
    fn the_checklist_outlives_the_turn() {
        let mut reader = claude();
        reader.feed(&slice(CLAUDE_TASKS, 0, 12));
        let update = "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"u9\",\"name\":\"TaskUpdate\",\"input\":{\"taskId\":\"2\",\"status\":\"completed\"}}]}}\n";
        assert!(reader.feed(update.as_bytes()));
        assert_eq!(
            reader.turns()[1].plan,
            Some(vec![
                step("Design", PlanStatus::Done),
                step("Build", PlanStatus::Done),
            ])
        );
    }

    /// A result with a `toolUseResult` but no `task.id` adds no step (the `#9` in its text is
    /// not used), and neither does an error result.
    #[test]
    fn a_create_without_an_id_or_with_an_error_adds_no_step() {
        let mut reader = claude();
        reader.feed(&slice(CLAUDE_TASKS, 0, 3));
        reader.feed(&slice(CLAUDE_TASKS, 15, 19));
        assert_eq!(
            reader.turns()[0].plan,
            Some(vec![step("Design", PlanStatus::Pending)])
        );
    }

    /// Task tools before the first turn of the window still build the checklist; the first
    /// turn's plan starts from it when it next changes.
    #[test]
    fn the_checklist_builds_before_the_first_turn() {
        let mut reader = claude();
        reader.feed(&slice(CLAUDE_TASKS, 1, 3));
        assert!(reader.turns().is_empty());
        reader.feed(&slice(CLAUDE_TASKS, 11, 12));
        assert_eq!(reader.turns()[0].plan, None);
        let update = "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"u9\",\"name\":\"TaskUpdate\",\"input\":{\"taskId\":\"1\",\"status\":\"completed\"}}]}}\n";
        assert!(reader.feed(update.as_bytes()));
        assert_eq!(
            reader.turns()[0].plan,
            Some(vec![step("Design", PlanStatus::Done)])
        );
    }

    /// Before the first turn a list result is accepted only when every entry reads.
    #[test]
    fn a_list_before_the_first_turn_needs_every_entry_to_read() {
        let result = |tasks: &str| {
            format!(
                "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"l\",\"content\":\"x\"}}]}},\"toolUseResult\":{{\"tasks\":{tasks}}}}}\n"
            )
        };
        let first_plan = |tasks: &str| {
            let mut reader = claude();
            reader.feed(&slice(CLAUDE_TASKS, 1, 3));
            reader.feed(result(tasks).as_bytes());
            reader.feed(&slice(CLAUDE_TASKS, 11, 12));
            let update = "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"u9\",\"name\":\"TaskUpdate\",\"input\":{\"taskId\":\"1\",\"status\":\"completed\"}}]}}\n";
            reader.feed(update.as_bytes());
            reader.turns()[0].plan.clone()
        };
        assert_eq!(
            first_plan(
                "[{\"id\":\"1\",\"subject\":\"Plan\"},{\"id\":\"2\",\"subject\":\"Old\",\"status\":\"deleted\"}]"
            ),
            Some(vec![step("Plan", PlanStatus::Done)]),
            "a whole list replaces the checklist"
        );
        assert_eq!(
            first_plan("[{\"id\":\"1\",\"subject\":\"Plan\"},{\"nope\":true}]"),
            Some(vec![step("Design", PlanStatus::Done)]),
            "a list with an unreadable entry is ignored"
        );
    }

    /// An empty list gives `Some(empty)` (not nothing) for `TodoWrite` and for a list result.
    #[test]
    fn an_emptied_list_is_some_empty() {
        let todo = "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"w\",\"name\":\"TodoWrite\",\"input\":{\"todos\":[]}}]}}\n";
        let mut reader = claude();
        reader.feed(&slice(CLAUDE_TASKS, 0, 1));
        reader.feed(todo.as_bytes());
        assert_eq!(reader.turns()[0].plan, Some(vec![]));

        let codex = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"go\"}}\n\
            {\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"update_plan\",\"call_id\":\"p\",\"arguments\":\"{\\\"plan\\\":[]}\"}}\n";
        let reader = read(UsageFormat::CodexJsonl, codex.as_bytes());
        assert_eq!(reader.turns()[0].plan, Some(vec![]));

        let mut reader = claude();
        reader.feed(&slice(CLAUDE_TASKS, 0, 3));
        reader.feed(&slice(CLAUDE_TASKS, 11, 12));
        let list = "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"tool_use\",\"id\":\"l2\",\"name\":\"TaskList\",\"input\":{}}]}}\n\
            {\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"l2\",\"content\":\"No tasks\"}]},\"toolUseResult\":{\"tasks\":[]}}\n";
        reader.feed(list.as_bytes());
        assert_eq!(reader.turns()[1].plan, Some(vec![]));
    }

    /// Plan steps, question and option texts are cut to `MAX_TEXT_CHARS` characters (never
    /// inside a char) with a `…`; the checklist holds at most `MAX_ITEMS` steps.
    #[test]
    fn long_plan_and_question_texts_are_cut() {
        let long = "é".repeat(crate::plan::MAX_TEXT_CHARS + 30);
        let cut = format!("{}…", "é".repeat(crate::plan::MAX_TEXT_CHARS));
        let assistant = |tool: &str, input: &str| {
            format!(
                "{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"x\",\"name\":\"{tool}\",\"input\":{input}}}]}}}}\n"
            )
        };
        let mut jsonl = slice(CLAUDE_TASKS, 0, 1);
        jsonl.extend(
            assistant(
                "TodoWrite",
                &format!("{{\"todos\":[{{\"content\":\"{long}\",\"status\":\"pending\"}}]}}"),
            )
            .into_bytes(),
        );
        let reader = read(UsageFormat::ClaudeJsonl, &jsonl);
        assert_eq!(
            reader.turns()[0].plan,
            Some(vec![step(&cut, PlanStatus::Pending)])
        );

        let mut jsonl = slice(CLAUDE_TASKS, 0, 1);
        jsonl.extend(assistant(
            "AskUserQuestion",
            &format!("{{\"questions\":[{{\"question\":\"{long}\",\"options\":[{{\"label\":\"{long}\"}}]}}]}}"),
        ).into_bytes());
        let reader = read(UsageFormat::ClaudeJsonl, &jsonl);
        let question = reader.turns()[0].question.as_ref().unwrap();
        assert_eq!(question.text, cut);
        assert_eq!(question.options, std::slice::from_ref(&cut));

        let mut jsonl = slice(CLAUDE_TASKS, 0, 1);
        for i in 0..crate::plan::MAX_ITEMS + 5 {
            let id = format!("c{i}");
            jsonl.extend(format!(
                "{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"{id}\",\"name\":\"TaskCreate\",\"input\":{{\"subject\":\"{long}\"}}}}]}}}}\n\
                 {{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"{id}\",\"content\":\"ok\"}}]}},\"toolUseResult\":{{\"task\":{{\"id\":\"{i}\"}}}}}}\n"
            ).into_bytes());
        }
        let reader = read(UsageFormat::ClaudeJsonl, &jsonl);
        let plan = reader.turns()[0].plan.as_ref().unwrap();
        assert_eq!(plan.len(), crate::plan::MAX_ITEMS);
        assert_eq!(plan[0].text, cut);
    }

    /// A background launch stays running until its notification: matched by tool-use id, and
    /// by task id when that tag is absent. A second notification and one for an unknown id
    /// change nothing; none of them is a prompt.
    #[test]
    fn background_subagents_run_until_their_notification() {
        let mut reader = claude();
        let state = |reader: &ThreadReader| -> Vec<(String, bool)> {
            reader
                .turns()
                .iter()
                .flat_map(|t| &t.subagents)
                .map(|s| (s.id.clone(), s.done))
                .collect()
        };
        let running = |done: [bool; 4]| -> Vec<(String, bool)> {
            ["bg1", "bg2", "bg3", "fg1"]
                .iter()
                .zip(done)
                .map(|(id, done)| ((*id).to_owned(), done))
                .collect()
        };
        reader.feed(&slice(CLAUDE_BACKGROUND, 0, 6));
        assert_eq!(
            state(&reader),
            running([false, false, false, true]),
            "launch results keep them running (structured status, text only, structured only); a plain result is done"
        );
        let agent_ids: Vec<_> = reader.turns()[0]
            .subagents
            .iter()
            .map(|s| s.agent_id.as_deref())
            .collect();
        assert_eq!(
            agent_ids,
            [Some("a111"), Some("a222"), Some("a333"), None],
            "from toolUseResult, from `agentId:` in the text, from toolUseResult"
        );
        reader.feed(&slice(CLAUDE_BACKGROUND, 6, 7));
        assert!(
            reader.feed(&slice(CLAUDE_BACKGROUND, 7, 8)),
            "by tool-use id"
        );
        assert_eq!(state(&reader), running([true, false, false, true]));
        assert!(reader.feed(&slice(CLAUDE_BACKGROUND, 8, 9)), "by task id");
        assert_eq!(state(&reader), running([true, false, true, true]));
        assert!(
            !reader.feed(&slice(CLAUDE_BACKGROUND, 9, 10)),
            "a second one"
        );
        assert!(
            !reader.feed(&slice(CLAUDE_BACKGROUND, 10, 11)),
            "unknown ids"
        );
        assert_eq!(state(&reader), running([true, false, true, true]));
        assert_eq!(reader.turns().len(), 2, "no notification is a prompt");
    }

    /// A command that prints `Exit code: 1` and exits 0 is not an error: the exit lines are
    /// only read before the `Output:` line.
    #[test]
    fn codex_exit_lines_in_the_output_are_not_the_exit_code() {
        let printed = "Exit code: 0\nWall time: 0.1 seconds\nOutput:\nExit code: 1\nbye";
        assert!(!codex_output(&Value::String(printed.into())).1);
        let unified = "Process exited with code 0\nOutput:\nProcess exited with code 2";
        assert!(!codex_output(&Value::String(unified.into())).1);
        let failed = "Exit code: 1\nWall time: 0.1 seconds\nOutput:\nnope";
        assert!(codex_output(&Value::String(failed.into())).1);
    }

    fn claude_status_notification(status: &str) -> String {
        format!(
            "{{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":\"<task-notification>\\n<task-id>a111</task-id>\\n<tool-use-id>bg1</tool-use-id>\\n<status>{status}</status>\\n</task-notification>\"}}}}\n"
        )
    }

    /// Any status word ends a subagent, except `running`.
    #[test]
    fn any_notification_status_ends_a_subagent() {
        for (status, ends) in [("killed", true), ("cancelled", true), ("running", false)] {
            let mut reader = claude();
            reader.feed(&slice(CLAUDE_BACKGROUND, 0, 3));
            reader.feed(claude_status_notification(status).as_bytes());
            let done = reader.turns()[0].subagents[0].done;
            assert_eq!(done, ends, "status {status}");
        }
    }

    /// A pending call pushed out of the newest calls stays pending (`None`), so it is not
    /// shown as a finished call with a cut result.
    #[test]
    fn a_pending_call_pushed_out_stays_pending() {
        let mut text = String::from(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"go\"}}\n",
        );
        for i in 0..=DETAILED_CALLS {
            text.push_str(&format!(
                "{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"t{i}\",\"name\":\"Read\",\"input\":{{\"file_path\":\"f{i}\"}}}}]}}}}\n"
            ));
        }
        let reader = read(UsageFormat::ClaudeJsonl, text.as_bytes());
        let calls = calls(&reader.turns()[0]);
        assert_eq!(calls[0].input, "");
        assert_eq!(calls[0].result, None);
    }

    #[test]
    fn only_background_launches_are_marked_background() {
        let mut reader = claude();
        reader.feed(&slice(CLAUDE_BACKGROUND, 0, 6));
        let flags: Vec<_> = reader.turns()[0]
            .subagents
            .iter()
            .map(|s| (s.id.as_str(), s.background))
            .collect();
        assert_eq!(
            flags,
            [("bg1", true), ("bg2", true), ("bg3", true), ("fg1", false)]
        );
    }

    /// Only the newest `DETAILED_CALLS` calls of a turn keep their arguments and result; older
    /// ones keep name, target and error mark and show the cut marker as their result.
    #[test]
    fn a_turn_keeps_the_bodies_of_its_newest_calls_only() {
        let total = DETAILED_CALLS + 7;
        let mut text = String::from(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"go\"}}\n",
        );
        for i in 0..total {
            text.push_str(&format!(
                "{{\"type\":\"assistant\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"tool_use\",\"id\":\"t{i}\",\"name\":\"Read\",\"input\":{{\"file_path\":\"f{i}\"}}}}]}}}}\n\
                 {{\"type\":\"user\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"tool_result\",\"tool_use_id\":\"t{i}\",\"content\":\"body {i}\",\"is_error\":{}}}]}}}}\n",
                i == 0
            ));
        }
        let reader = read(UsageFormat::ClaudeJsonl, text.as_bytes());
        let calls = calls(&reader.turns()[0]);
        assert_eq!(calls.len(), total);
        let with_body = calls.iter().filter(|c| !c.input.is_empty()).count();
        assert_eq!(with_body, DETAILED_CALLS);
        let old = calls[0];
        assert_eq!((old.name.as_str(), old.target.as_str()), ("Read", "f0"));
        assert_eq!(old.input, "");
        assert_eq!(
            old.result,
            Some(ToolResult {
                text: CUT.into(),
                is_error: true
            }),
            "the error mark stays"
        );
        let newest = calls[total - 1];
        assert!(newest.input.contains(&format!("f{}", total - 1)));
        assert_eq!(
            newest.result.as_ref().map(|r| r.text.as_str()),
            Some(format!("body {}", total - 1).as_str())
        );
    }

    /// Codex's unified `exec_command` takes `cmd` (codex-rs `ExecCommandArgs`).
    #[test]
    fn a_codex_exec_command_shows_its_cmd() {
        let codex = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"go\"}}\n\
            {\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"call_id\":\"e1\",\"arguments\":\"{\\\"cmd\\\":\\\"cargo test\\\",\\\"workdir\\\":\\\"/w\\\"}\"}}\n";
        let reader = read(UsageFormat::CodexJsonl, codex.as_bytes());
        assert_eq!(calls(&reader.turns()[0])[0].target, "cargo test");
    }

    #[test]
    fn stamps_become_epoch_milliseconds() {
        assert_eq!(stamp_ms("2026-01-01T00:00:00Z"), Some(T0));
        assert_eq!(stamp_ms("2026-01-01T00:00:01.250Z"), Some(T0 + 1_250));
        assert_eq!(stamp_ms("2026-01-01T00:00:00.5Z"), Some(T0 + 500));
        assert_eq!(stamp_ms("2026-01-01 00:00:00"), None);
        assert_eq!(stamp_ms("nonsense"), None);
    }

    /// m1 is written in three lines and counts once (its newest figure, 40), m2 adds 20, the
    /// sidechain line at +13 s is a subagent's: it is not in the tokens, `ended_at` or the
    /// clock. The redacted thinking (empty text, a signature) is kept with its time.
    #[test]
    fn claude_turn_timing_tokens_and_redacted_thinking() {
        let reader = read(UsageFormat::ClaudeJsonl, CLAUDE_TIMING);
        let [turn] = reader.turns() else {
            panic!("{:?}", reader.turns());
        };
        assert_eq!(turn.started_at, Some(T0));
        assert_eq!(turn.ended_at, Some(T0 + 12_000));
        assert_eq!(turn.output_tokens, 60);
        // Assistant lines at +2 (after +0), +3, +4 and +12 (after the result at +10).
        assert_eq!(turn.generation_ms, 2_000 + 1_000 + 1_000 + 2_000);
        assert_eq!(turn.items.len(), 4, "{turn:?}");
        assert_eq!(
            turn.items[0],
            Item::Thinking {
                text: String::new(),
                ms: Some(2_000)
            }
        );
    }

    /// A regression pin, not a red-first test: today's `thinking()` already drops empty text, and
    /// this guards that a block without a signature stays dropped once redacted blocks are kept.
    #[test]
    fn a_claude_thinking_block_without_text_or_signature_is_not_kept() {
        let jsonl = "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"go\"}}\n\
             {\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"thinking\",\"thinking\":\"\"},{\"type\":\"text\",\"text\":\"hi\"}]}}\n";
        let reader = read(UsageFormat::ClaudeJsonl, jsonl.as_bytes());
        assert_eq!(reader.turns()[0].items, [text("hi")]);
    }

    /// The repeated `token_count` (same total) is skipped; times come from the Codex lines.
    #[test]
    fn codex_turn_timing_and_tokens() {
        let reader = read(UsageFormat::CodexJsonl, CODEX_TIMING);
        let [turn] = reader.turns() else {
            panic!("{:?}", reader.turns());
        };
        assert_eq!(turn.started_at, Some(T0 + 1_000));
        assert_eq!(turn.ended_at, Some(T0 + 7_000));
        assert_eq!(turn.output_tokens, 50 + 30);
        // The `agent_message` event at +3 (after the prompt at +1; the `response_item` message
        // at the same time adds 0), the call at +6 (after +4.5).
        assert_eq!(turn.generation_ms, 2_000 + 1_500);
    }

    /// omp writes the thinking and the tool call in one line: its time is the message's
    /// generation time (line time minus the line before).
    #[test]
    fn omp_turn_timing_tokens_and_shared_line_thinking() {
        let reader = read(UsageFormat::OmpJsonl, OMP_TIMING);
        let [turn] = reader.turns() else {
            panic!("{:?}", reader.turns());
        };
        assert_eq!(turn.started_at, Some(T0 + 1_000));
        assert_eq!(turn.ended_at, Some(T0 + 10_000));
        assert_eq!(turn.output_tokens, 180);
        assert_eq!(turn.generation_ms, 4_000 + 4_000);
        assert_eq!(
            turn.items[0],
            Item::Thinking {
                text: "hm".into(),
                ms: Some(4_000)
            }
        );
    }

    #[test]
    fn lines_without_timestamps_have_no_times() {
        let reader = read(UsageFormat::OmpJsonl, OMP_CHAT);
        let turn = &reader.turns()[0];
        assert_eq!((turn.started_at, turn.ended_at), (None, None));
        assert_eq!((turn.output_tokens, turn.generation_ms), (0, 0));
        assert!(matches!(turn.items[0], Item::Thinking { ms: None, .. }));
    }

    /// An assistant line older than the line before it adds no time; `ended_at` keeps the
    /// newest time.
    #[test]
    fn a_skewed_clock_never_gives_negative_time() {
        let line = |ts: &str, body: &str| {
            format!("{{\"type\":\"message\",\"timestamp\":\"{ts}\",\"message\":{body}}}\n")
        };
        let jsonl = line(
            "2026-01-01T00:00:10.000Z",
            r#"{"role":"user","content":[{"type":"text","text":"go"}]}"#,
        ) + &line(
            "2026-01-01T00:00:05.000Z",
            r#"{"role":"assistant","usage":{"output":10},"content":[{"type":"thinking","thinking":"hm"},{"type":"text","text":"late"}]}"#,
        );
        let reader = read(UsageFormat::OmpJsonl, jsonl.as_bytes());
        let turn = &reader.turns()[0];
        assert_eq!(turn.ended_at, Some(T0 + 10_000));
        assert_eq!(turn.generation_ms, 0);
        assert_eq!(turn.output_tokens, 10);
        assert!(matches!(turn.items[0], Item::Thinking { ms: Some(0), .. }));
    }

    fn meta_of(format: UsageFormat, bytes: &[u8]) -> SessionMeta {
        read(format, bytes).meta().clone()
    }

    #[test]
    fn claude_meta() {
        assert_eq!(
            meta_of(UsageFormat::ClaudeJsonl, CLAUDE_TIMING),
            SessionMeta {
                model: Some("claude-opus-5-5".into()),
                effort: None,
                // The later `permission-mode` line wins over the user line's `acceptEdits`.
                permission: Some("auto".into()),
                title: Some("Fix login".into()),
                // The newest main-thread message: 5 + 200 + 0 (the sidechain line is ignored).
                context: Some(205),
                context_window: None,
            }
        );
    }

    #[test]
    fn codex_meta() {
        assert_eq!(
            meta_of(UsageFormat::CodexJsonl, CODEX_TIMING),
            SessionMeta {
                model: Some("gpt-5.3-codex".into()),
                effort: Some("high".into()),
                permission: Some("on-request".into()),
                title: None,
                context: Some(1_900),
                context_window: Some(258_400),
            }
        );
    }

    /// The assistant message's model is newer than `model_change`; the glyph in the title goes.
    #[test]
    fn omp_meta() {
        assert_eq!(
            meta_of(UsageFormat::OmpJsonl, OMP_TIMING),
            SessionMeta {
                model: Some("claude-opus-5-5".into()),
                effort: Some("high".into()),
                permission: None,
                title: Some("Check it".into()),
                context: Some(1 + 2_000),
                context_window: None,
            }
        );
    }

    /// A reader that starts in the middle of a file knows only what it has seen: fewer fields,
    /// never wrong ones.
    #[test]
    fn a_reader_started_mid_file_knows_only_what_it_saw() {
        let mut reader = ThreadReader::new(&UsageFormat::OmpJsonl, THREAD_TURNS).unwrap();
        reader.start_mid_file(500);
        let tail = "tail of a cut line\n{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"model\":\"claude-opus-5-5\",\"usage\":{\"input\":1,\"output\":2,\"cacheRead\":3,\"cacheWrite\":0},\"content\":[{\"type\":\"text\",\"text\":\"hi\"}]}}\n";
        reader.feed(tail.as_bytes());
        assert_eq!(
            reader.meta(),
            &SessionMeta {
                model: Some("claude-opus-5-5".into()),
                context: Some(4),
                ..SessionMeta::default()
            }
        );
        assert!(reader.turns().is_empty());
    }

    #[test]
    fn short_model_names() {
        assert_eq!(short_model("anthropic/claude-opus-5-5"), "opus 5.5");
        assert_eq!(short_model("claude-sonnet-4-5-20250929"), "sonnet 4.5");
        assert_eq!(short_model("gpt-5.3-codex"), "gpt 5.3 codex");
        assert_eq!(short_model(""), "");
        // An unknown shape is kept, cut to 24 characters with an ellipsis.
        let long = "x".repeat(40);
        let cut = short_model(&long);
        assert_eq!(cut.chars().count(), 24);
        assert!(cut.ends_with('…'));
        assert_eq!(short_model("a-b"), "a b");
    }

    #[test]
    fn titles_lose_their_private_use_glyph() {
        assert_eq!(clean_title("\u{F04F9} VER: Check it"), "VER: Check it");
        assert_eq!(clean_title("  plain title  "), "plain title");
        assert_eq!(clean_title("\u{E000}\u{F8FF}"), "");
        assert_eq!(clean_title("Tiếng Việt"), "Tiếng Việt");
    }
}
