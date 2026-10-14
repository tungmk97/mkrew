# Thread View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A live "Thread" section in the session panel that shows the focused agent session's transcript as prompt → answer blocks with collapsed tool-call rows. It reads the transcript only while the panel shows that session.

**Architecture:** A pure incremental parser, `mkrew_core::thread::ThreadReader` (shaped like `mkrew_core::usage::UsageReader`), turns transcript JSONL bytes into at most 20 turns. The UI does the rest: `mkrew_ui::thread_view::ThreadFile` reads the file (the last 4 MiB first, then the appended bytes). The `Shell` runs one 1 s poll task per shown session; the existing 500 ms status tick starts and stops it. The daemon only exposes its existing transcript lookup as `mkrew_daemon::transcript_path`.

**Tech Stack:** Rust, serde_json, GPUI, the existing Docs Markdown model (`docs::Shown` + `preview::blocks`).

**Spec:** `docs/superpowers/specs/2026-10-14-thread-view-design.md`

## Global Constraints

- Zero cost unless the panel is open **and** shows the session: no reads and no task otherwise.
- Shown only when the session's profile has `[usage]` and the session has a `native_session_id`. Otherwise there is no Thread section at all (no empty box).
- Poll every **1 s**: one `metadata` call; read only the appended bytes, off the UI thread.
- First read: only the **last 4 MiB**, starting after the first `\n` in that window. Keep at most **20 turns**.
- File shorter than the offset → start over (same 4 MiB rule). Missing or unreadable → keep what is shown and locate the file again at the next tick.
- Not shown: thinking, tool output, sub-agent (sidechain) lines.
- Tool `target`: the first present of `file_path`, `path`, `pattern`, `command`, `url`; first line only, at most 120 chars; empty if none.
- No protocol change, no feature flag, no new dependency.
- All colors, spacing, radii and font sizes come from `crates/mkrew-ui/src/theme.rs` (SPEC §14). Check both dark and light themes.
- No test calls a real agent or the network. `cargo clippy --workspace --all-targets -- -D warnings` must be clean. No `unwrap/expect` outside tests.
- Commit only your own files with explicit paths or `git add -p`. Never `git add -A`, never push. Use Conventional Commits. The tree usually has other people's WIP.

## Real transcript shapes (checked 2026-10-14 against files on this machine)

These refine the spec's per-format rules. The fixtures in Task 1 follow them.

- **Claude** (`~/.claude/projects/*/<id>.jsonl`):
  - Prompt: `type:"user"`, not `isSidechain`, not `isMeta`. `message.content` is a string or `text` blocks, with **no** `tool_result` block. Text starting with `<command-` or `<local-command-` is a slash-command echo and is skipped.
  - Assistant: `type:"assistant"`, not sidechain. Each line carries ONE content block (`thinking` | `text` | `tool_use{name,input}`).
- **Codex** (`~/.codex/sessions/**/rollout-*.jsonl`):
  - Prompt: `type:"event_msg"`, `payload.type:"user_message"`, `payload.message` (a string). **Not** `response_item` with role `user`: those include injected `<environment_context>` and AGENTS.md text.
  - Assistant text: `response_item` with `payload.type:"message"`, role `assistant`, `content[].type:"output_text"`.
  - Tools: `response_item` `function_call{name, arguments: JSON string}` and `custom_tool_call{name, input: raw string}`.
  - `event_msg agent_message` duplicates the assistant text and is ignored.
- **omp** (`~/.omp/agent/sessions/<slug>/<ts>_<id>.jsonl`): `type:"message"`. Role `user` → prompt (`text` blocks). Role `assistant` → `text` blocks and `toolCall{name, arguments: object}`. Roles `toolResult` and `developer` are ignored.
- Codex `custom_tool_call` (`apply_patch`) target: the first input line that does not start with `*** Begin`; otherwise every patch would read `*** Begin Patch`.
- Codex `command` may be a string or an array of strings (joined with spaces).

## Review Focus

1. **A line split across two polls** (the agent writes while we read) must not lose or duplicate items → Task 1 tests every 7th split point.
2. **Opening a huge transcript** must stay fast and bounded and show "Earlier turns" → Task 1 `start_mid_file` test + Task 3 window test.
3. **A transcript rewritten or deleted while shown** (Claude `/clear`, rotation) must not panic or show stale content forever → Task 3 shrink and missing-file tests.
4. **Focus moves to another pane, the panel closes, or the session exits** → the task must stop (no CPU) → Task 4 `wanted_thread` unit test + the exit rule in `poll_thread` + HUMAN_CHECKS.
5. **Very long tool targets or multi-line commands** must not blow up a row → Task 1 `targets_are_one_short_line`.

---

### Task 1: `mkrew_core::thread` parser

**Files:**
- Create: `crates/mkrew-core/src/thread.rs`
- Create: `crates/mkrew-core/tests/fixtures/thread/claude.jsonl`
- Create: `crates/mkrew-core/tests/fixtures/thread/codex.jsonl`
- Create: `crates/mkrew-core/tests/fixtures/thread/omp.jsonl`
- Modify: `crates/mkrew-core/src/lib.rs` (add `pub mod thread;` before `pub mod turn;`)

**Interfaces:**
- Consumes: `crate::profile::UsageFormat` (`ClaudeJsonl | CodexJsonl | OmpJsonl | Unknown(String)`).
- Produces:
  ```rust
  pub const MAX_TURNS: usize = 20;
  pub const TARGET_MAX: usize = 120;
  pub struct ThreadReader { .. }            // Debug, Clone
  impl ThreadReader {
      pub fn new(format: &UsageFormat) -> Result<Self, String>;
      pub fn offset(&self) -> u64;
      pub fn start_mid_file(&mut self, offset: u64);
      pub fn feed(&mut self, appended: &[u8]) -> bool;
      pub fn turns(&self) -> &[Turn];
      pub fn truncated(&self) -> bool;
      /// Turns dropped by the 20-turn cap so far: turn `i` of `turns()` is turn
      /// `first_turn() + i` of the reader's life (a stable key for UI state).
      pub fn first_turn(&self) -> u64;
  }
  #[derive(Debug, Clone, PartialEq, Eq)] pub struct Turn { pub prompt: String, pub items: Vec<Item> }
  #[derive(Debug, Clone, PartialEq, Eq)] pub enum Item { Text(String), Tools(Vec<ToolCall>) }
  #[derive(Debug, Clone, PartialEq, Eq)] pub struct ToolCall { pub name: String, pub target: String }
  ```

- [ ] **Step 1: Write the fixtures** (one JSON object per line, LF endings, trailing newline).

`crates/mkrew-core/tests/fixtures/thread/claude.jsonl`:
```jsonl
{"type":"permission-mode","permissionMode":"auto","sessionId":"s1"}
{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"text","text":"before any prompt"}]}}
{"type":"user","isSidechain":false,"isMeta":false,"message":{"role":"user","content":"Fix the login bug"}}
{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"thinking","thinking":"secret","signature":"x"}]}}
{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"text","text":"Looking at **auth**."}]}}
{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"src/auth.rs"}}]}}
{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"fn login() {}"}]}}
{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Grep","input":{"pattern":"login","path":"src"}}]}}
{"type":"assistant","isSidechain":true,"message":{"role":"assistant","content":[{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"sub agent"}}]}}
{"type":"user","isSidechain":true,"message":{"role":"user","content":"sub-agent prompt"}}
{"type":"user","isSidechain":false,"isMeta":true,"message":{"role":"user","content":"## Context Usage"}}
{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"text","text":"Fixed it."}]}}
{"type":"user","isSidechain":false,"message":{"role":"user","content":[{"type":"text","text":"Now add a test"}]}}
{"type":"assistant","isSidechain":false,"message":{"role":"assistant","content":[{"type":"tool_use","id":"t4","name":"Bash","input":{"command":"cargo test\n--all"}}]}}
{"type":"user","isSidechain":false,"message":{"role":"user","content":"<command-name>/context</command-name>"}}
not json at all
```

`crates/mkrew-core/tests/fixtures/thread/codex.jsonl`:
```jsonl
{"type":"session_meta","payload":{"id":"019e-thread","cwd":"C:\\w"}}
{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"rules"}]}}
{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"<environment_context>cwd</environment_context>"}]}}
{"type":"event_msg","payload":{"type":"user_message","message":"Add retries","images":[]}}
{"type":"response_item","payload":{"type":"reasoning","summary":[],"encrypted_content":"x"}}
{"type":"event_msg","payload":{"type":"agent_message","message":"I'll add retries."}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"I'll add retries."}]}}
{"type":"response_item","payload":{"type":"function_call","name":"shell_command","arguments":"{\"command\":\"rg retry\",\"workdir\":\"C:\\\\w\"}","call_id":"c1"}}
{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"none"}}
{"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Add File: a.rs\n+fn a() {}\n","call_id":"c2"}}
{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Done."}]}}
{"type":"event_msg","payload":{"type":"user_message","message":"List files"}}
{"type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"pwsh\",\"-c\",\"ls\"]}","call_id":"c3"}}
```

`crates/mkrew-core/tests/fixtures/thread/omp.jsonl`:
```jsonl
{"type":"session","version":3,"id":"01a1-omp"}
{"type":"message","message":{"role":"user","content":[{"type":"text","text":"Check the plan"}]}}
{"type":"message","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hm"},{"type":"toolCall","id":"c1","name":"grep","arguments":{"pattern":"plan","path":"docs"}}]}}
{"type":"message","message":{"role":"toolResult","toolCallId":"c1","toolName":"grep","content":[{"type":"text","text":"none"}]}}
{"type":"message","message":{"role":"assistant","content":[{"type":"toolCall","id":"c2","name":"read","arguments":{"path":"docs/PLAN.md"}}]}}
{"type":"message","message":{"role":"developer","content":[{"type":"text","text":"reminder"}]}}
{"type":"message","message":{"role":"assistant","content":[{"type":"text","text":"Not done yet."}]}}
```

- [ ] **Step 2: Write the failing tests.** Create `crates/mkrew-core/src/thread.rs` with only this test module first, so it fails to compile on the missing items.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const CLAUDE: &[u8] = include_bytes!("../tests/fixtures/thread/claude.jsonl");
    const CODEX: &[u8] = include_bytes!("../tests/fixtures/thread/codex.jsonl");
    const OMP: &[u8] = include_bytes!("../tests/fixtures/thread/omp.jsonl");

    fn tool(name: &str, target: &str) -> ToolCall {
        ToolCall { name: name.into(), target: target.into() }
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
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p mkrew-core thread`
Expected: compile errors (`ThreadReader`, `Turn`, `MAX_TURNS`, `TARGET_MAX` not found).

- [ ] **Step 4: Implement** (above the test module in `thread.rs`):

```rust
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
                return Err(format!("unknown transcript format `{name}`: no thread shown"));
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
                self.turns.push(Turn { prompt, items: Vec::new() });
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
                &words.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
            )),
            _ => None,
        })
        .unwrap_or_default()
}

/// The first non-empty line of `text`, at most [`TARGET_MAX`] characters.
fn one_line(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
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
        (Some("event_msg"), Some("user_message")) => {
            block_text(&payload["message"], "text").map(Line::Prompt).into_iter().collect()
        }
        (Some("response_item"), Some("message")) if payload["role"] == "assistant" => {
            block_text(&payload["content"], "output_text").map(Line::Text).into_iter().collect()
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
        Some("user") => block_text(content, "text").map(Line::Prompt).into_iter().collect(),
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
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p mkrew-core thread`
Expected: all 9 tests PASS. Then `cargo clippy -p mkrew-core --all-targets -- -D warnings` → clean.

- [ ] **Step 6: Commit**

```bash
git add crates/mkrew-core/src/thread.rs crates/mkrew-core/src/lib.rs crates/mkrew-core/tests/fixtures/thread
git commit -m "feat(core): incremental thread reader for agent transcripts"
```

---

### Task 2: `mkrew_daemon::transcript_path`

**Files:**
- Modify: `crates/mkrew-daemon/src/usage.rs` (add a pub fn right after `locate`, ~line 82)
- Modify: `crates/mkrew-daemon/src/lib.rs` (re-export it; `mod usage;` stays private)

**Interfaces:**
- Produces: `pub fn transcript_path(format: &mkrew_core::UsageFormat, native: &str) -> Option<std::path::PathBuf>`, reachable as `mkrew_daemon::transcript_path`.

- [ ] **Step 1: Add the helper** in `usage.rs`:

```rust
/// The transcript of agent session `native` under the agent's own folder (SPEC §20E.3).
/// The UI's thread view uses the same lookup as usage (ADR 0055).
pub fn transcript_path(format: &UsageFormat, native: &str) -> Option<PathBuf> {
    locate(format, &root(format)?, native)
}
```

- [ ] **Step 2: Re-export** in `lib.rs`, next to the existing `pub use` lines (or after the `mod` list if there are none):

```rust
pub use usage::transcript_path;
```

- [ ] **Step 3: Verify**

Run: `cargo test -p mkrew-daemon usage`. The existing `locate` tests cover the lookup; there is no new test because this is a two-line forward.
Run: `cargo clippy -p mkrew-daemon --all-targets -- -D warnings`
Expected: PASS, no warnings (no more dead-code lint either).

- [ ] **Step 4: Commit**

```bash
git add crates/mkrew-daemon/src/usage.rs crates/mkrew-daemon/src/lib.rs
git commit -m "feat(daemon): expose transcript lookup for the UI thread view"
```

---

### Task 3: `mkrew_ui::thread_view` — file reading and the page model

**Files:**
- Create: `crates/mkrew-ui/src/thread_view.rs`
- Modify: `crates/mkrew-ui/src/lib.rs` (add `pub mod thread_view;` in alphabetical order among the `pub mod` lines)

**Interfaces:**
- Consumes: `mkrew_core::thread::{ThreadReader, Item, ToolCall}` (Task 1), `mkrew_core::UsageFormat`, `crate::docs::Shown` (`Shown::from_text(&str) -> Shown`).
- Produces:
  ```rust
  pub const WINDOW: u64 = 4 * 1024 * 1024;
  pub struct ThreadFile { .. }   // Send
  impl ThreadFile {
      pub fn new(format: &UsageFormat, locate: Box<dyn Fn() -> Option<PathBuf> + Send>) -> Result<Self, String>;
      /// Reads what was appended; true if the turns changed.
      pub fn read(&mut self) -> bool;
      pub fn page(&self) -> Page;
  }
  pub struct Page { pub turns: Vec<PageTurn>, pub truncated: bool, pub first_turn: u64 }
  pub struct PageTurn { pub prompt: String, pub items: Vec<PageItem> }
  pub enum PageItem { Text(Shown), Tools(Vec<ToolCall>) }
  ```

- [ ] **Step 1: Write the failing tests** (bottom of `thread_view.rs`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn prompt(text: &str) -> String {
        format!(
            "{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"{text}\"}}]}}}}\n"
        )
    }

    fn reader_at(path: &std::path::Path) -> ThreadFile {
        let path = path.to_owned();
        ThreadFile::new(&UsageFormat::OmpJsonl, Box::new(move || Some(path.clone()))).unwrap()
    }

    fn prompts(file: &ThreadFile) -> Vec<String> {
        file.page().turns.into_iter().map(|t| t.prompt).collect()
    }

    #[test]
    fn reads_only_what_was_appended_and_restarts_when_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut file = reader_at(&path);
        assert!(!file.read(), "no file yet");

        std::fs::write(&path, prompt("one")).unwrap();
        assert!(file.read());
        assert!(!file.read(), "nothing new");
        std::fs::write(&path, prompt("one") + &prompt("two")).unwrap();
        assert!(file.read());
        assert_eq!(prompts(&file), ["one", "two"]);

        // Rewritten shorter (e.g. a new conversation in the same file): read again.
        std::fs::write(&path, prompt("fresh")).unwrap();
        assert!(file.read());
        assert_eq!(prompts(&file), ["fresh"]);
    }

    /// A deleted file keeps what is shown; it is located again at the next read.
    #[test]
    fn a_missing_file_keeps_the_turns() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let mut file = reader_at(&path);
        std::fs::write(&path, prompt("one")).unwrap();
        assert!(file.read());
        std::fs::remove_file(&path).unwrap();
        assert!(!file.read());
        assert_eq!(prompts(&file), ["one"]);
        std::fs::write(&path, prompt("one") + &prompt("two")).unwrap();
        assert!(file.read());
        assert_eq!(prompts(&file), ["one", "two"]);
    }

    /// A long file is read from its last WINDOW bytes only, and says turns are missing.
    #[test]
    fn a_long_file_starts_at_the_window() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let filler = "x".repeat(1024);
        let mut text = prompt("old");
        while (text.len() as u64) < WINDOW + 64 * 1024 {
            text.push_str(&format!("{{\"type\":\"pad\",\"p\":\"{filler}\"}}\n"));
        }
        text.push_str(&prompt("new"));
        std::fs::write(&path, &text).unwrap();
        let mut file = reader_at(&path);
        assert!(file.read());
        assert!(file.page().truncated);
        assert_eq!(prompts(&file), ["new"]);
    }

    #[test]
    fn page_parses_text_as_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.jsonl");
        let answer = "{\"type\":\"message\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"# Done\"}]}}\n";
        std::fs::write(&path, prompt("go") + answer).unwrap();
        let mut file = reader_at(&path);
        file.read();
        let page = file.page();
        let PageItem::Text(shown) = &page.turns[0].items[0] else {
            panic!("text expected");
        };
        assert!(!shown.blocks.is_empty());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p mkrew-ui thread_view`
Expected: compile errors (`ThreadFile`, `WINDOW`, `PageItem` missing).

- [ ] **Step 3: Implement** (top of `thread_view.rs`):

```rust
//! The session panel's Thread section (ADR 0055): reads the shown session's transcript file
//! (the last [`WINDOW`] bytes first, then what was appended) through
//! [`mkrew_core::thread::ThreadReader`], and builds the [`Page`] the panel renders.
//! Runs off the UI thread; the caller polls [`ThreadFile::read`] about once a second.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use mkrew_core::UsageFormat;
use mkrew_core::thread::{Item, ThreadReader, ToolCall};

use crate::docs::Shown;

/// Bytes read on the first look at a transcript (its end).
pub const WINDOW: u64 = 4 * 1024 * 1024;
const CHUNK: usize = 1024 * 1024;

/// What the panel shows: the kept turns, with text parsed as Markdown.
pub struct Page {
    pub turns: Vec<PageTurn>,
    /// Earlier turns exist but are not shown ("Earlier turns: Open transcript").
    pub truncated: bool,
    /// Key of `turns[0]` (see `ThreadReader::first_turn`), for per-turn UI state.
    pub first_turn: u64,
}

pub struct PageTurn {
    pub prompt: String,
    pub items: Vec<PageItem>,
}

pub enum PageItem {
    Text(Shown),
    Tools(Vec<ToolCall>),
}

/// The reader of one shown session's transcript.
pub struct ThreadFile {
    format: UsageFormat,
    locate: Box<dyn Fn() -> Option<PathBuf> + Send>,
    path: Option<PathBuf>,
    reader: ThreadReader,
}

impl ThreadFile {
    pub fn new(
        format: &UsageFormat,
        locate: Box<dyn Fn() -> Option<PathBuf> + Send>,
    ) -> Result<Self, String> {
        Ok(Self {
            format: format.clone(),
            locate,
            path: None,
            reader: ThreadReader::new(format)?,
        })
    }

    /// Reads what was appended since the last read; true if the turns changed. A missing or
    /// unreadable file keeps the turns and is located again at the next read.
    pub fn read(&mut self) -> bool {
        if self.path.is_none() {
            self.path = (self.locate)();
        }
        let Some(path) = self.path.clone() else {
            return false;
        };
        match self.read_appended(&path) {
            Ok(changed) => changed,
            Err(error) => {
                tracing::debug!(path = %path.display(), "reading the transcript failed: {error}");
                self.path = None;
                false
            }
        }
    }

    fn read_appended(&mut self, path: &Path) -> std::io::Result<bool> {
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        let mut changed = false;
        if len < self.reader.offset() {
            // Rewritten from scratch: start over (the format was accepted in `new`).
            if let Ok(reader) = ThreadReader::new(&self.format) {
                self.reader = reader;
                changed = true;
            }
        }
        if self.reader.offset() == 0 && len > WINDOW {
            self.reader.start_mid_file(len - WINDOW);
        }
        file.seek(SeekFrom::Start(self.reader.offset()))?;
        let mut chunk = vec![0; CHUNK];
        loop {
            match file.read(&mut chunk) {
                Ok(0) => return Ok(changed),
                Ok(n) => changed |= self.reader.feed(&chunk[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// The turns as the panel shows them.
    // ponytail: re-parses every kept turn's Markdown on each change (at most 20 turns); cache
    // by text if it ever shows in a profile.
    pub fn page(&self) -> Page {
        Page {
            turns: self
                .reader
                .turns()
                .iter()
                .map(|turn| PageTurn {
                    prompt: turn.prompt.clone(),
                    items: turn
                        .items
                        .iter()
                        .map(|item| match item {
                            Item::Text(text) => PageItem::Text(Shown::from_text(text)),
                            Item::Tools(calls) => PageItem::Tools(calls.clone()),
                        })
                        .collect(),
                })
                .collect(),
            truncated: self.reader.truncated(),
            first_turn: self.reader.first_turn(),
        }
    }
}
```

Check: `Shown` must be `Send`. It is already built on a background thread in `shell/docs.rs:139`, so `Page` can move from the background executor to the UI thread. `tracing` is a dependency of `mkrew-ui` (confirm in `Cargo.toml`; if it is not, drop the log line rather than add a dependency).

- [ ] **Step 4: Run the tests**

Run: `cargo test -p mkrew-ui thread_view`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/mkrew-ui/src/thread_view.rs crates/mkrew-ui/src/lib.rs
git commit -m "feat(ui): thread file reader with a 4 MiB first window"
```

---

### Task 4: Thread section in the session panel (+ SPEC, ADR, HUMAN_CHECKS)

Read the `mkrew-ui-design` skill (`.claude/skills/mkrew-ui-design/SKILL.md`) before starting this task.

**Files:**
- Modify: `crates/mkrew-ui/src/main.rs` (the Shell field next to `inspector_open`, ~line 457; add imports there if missing)
- Modify: `crates/mkrew-ui/src/shell/connection.rs` (init the field next to `inspector_open: false`, ~line 321; call `sync_thread` in the status tick, ~line 94)
- Create: `crates/mkrew-ui/src/shell/thread_panel.rs` (the `impl Shell` for this area: sync, poll task, render) and add `mod thread_panel;` to the list in `crates/mkrew-ui/src/shell.rs`
- Modify: `crates/mkrew-ui/src/shell/inspector.rs` (the `render_inspector` layout, lines 130–165)
- Modify: `crates/mkrew-ui/src/inspector.rs` (the pure `wanted_thread` + its test, next to `session_card`)
- Modify: `crates/mkrew-ui/src/theme.rs` only if a new size token is needed (prefer the existing `space::*`, `radius::*`, `font_size::*`)
- Modify: `docs/SPEC.md` §10 (session panel, after the Plan card paragraph, ~line 851)
- Create: `docs/decisions/0055-thread-view-reads-transcripts-in-the-ui.md`
- Modify: `docs/HUMAN_CHECKS.md`

**Interfaces:**
- Consumes: `mkrew_daemon::transcript_path` (Task 2); `thread_view::{ThreadFile, Page, PageItem}` (Task 3); `mkrew_core::load_dir(&Path) -> ProfileSet` and `ProfileSet::get(id) -> Option<&AgentProfile>` (with `AgentProfile.usage: Option<UsageConfig>` and `UsageConfig.format`); `preview::blocks(&[Block], &Shown, &Theme) -> Vec<AnyElement>`; `inspector_card`, `meta_line`, `Shell::focused_session()`, `Shell::is_exited(Ulid)`, `Shell::open_transcript(id, cx)`; `Shell.config_dir` (the same folder as `Paths::config_dir`; the agents folder is `config_dir.join("agents")`, like `Paths::agents_dir()`).
- Produces (in `crate::inspector`, pure, tested):
  ```rust
  /// The session whose thread the panel should read: an open panel and a focused agent
  /// session with a native id. `None` = no reading.
  pub fn wanted_thread(open: bool, session: Option<&Session>) -> Option<(Ulid, String)>;
  ```

- [ ] **Step 1: Write the failing test for `wanted_thread`** in the tests of `crates/mkrew-ui/src/inspector.rs` (reuse its existing `session(kind, state)` helper):

```rust
#[test]
fn the_thread_is_read_only_for_a_shown_agent_with_a_native_id() {
    let mut agent = session(SessionKind::Agent, SessionState::Working);
    assert_eq!(wanted_thread(true, Some(&agent)), None, "no native id yet");
    agent.native_session_id = Some("n1".into());
    assert_eq!(wanted_thread(true, Some(&agent)), Some((agent.id, "n1".into())));
    assert_eq!(wanted_thread(false, Some(&agent)), None, "panel closed");
    assert_eq!(wanted_thread(true, None), None, "nothing focused");
    let mut shell = session(SessionKind::Shell, SessionState::Working);
    shell.native_session_id = Some("n1".into());
    assert_eq!(wanted_thread(true, Some(&shell)), None, "shells have no transcript");
}
```

Run: `cargo test -p mkrew-ui the_thread_is_read_only` → FAIL (not defined).

- [ ] **Step 2: Implement `wanted_thread`**

```rust
pub fn wanted_thread(open: bool, session: Option<&Session>) -> Option<(Ulid, String)> {
    let session = session.filter(|s| open && s.kind == SessionKind::Agent)?;
    Some((session.id, session.native_session_id.clone()?))
}
```

An exited session stays wanted while it is shown. The poll task (Step 4) does one last read after the exit and then stops. Run the test → PASS.

- [ ] **Step 3: Shell state.** In `main.rs`, next to `inspector_open`:

```rust
/// The session panel's Thread section (ADR 0055): the shown session's reading. `None` while
/// the panel is closed or shows no agent with a native id.
thread: Option<ThreadWatch>,
```

and in `shell/thread_panel.rs`:

```rust
pub(crate) struct ThreadWatch {
    /// The session and native id being read; another pair starts a new watch.
    pub(crate) key: (Ulid, String),
    /// The 1 s poll; dropping it stops reading. Ends by itself after the session exited (one
    /// last read) or when the profile has no `[usage]`.
    task: Option<Task<()>>,
    /// Built off the UI thread on each change; `None` until a read found turns.
    pub(crate) page: Option<Rc<thread_view::Page>>,
    /// Expanded tool groups: (turn key = `page.first_turn + turn index`, item index).
    pub(crate) expanded: HashSet<(u64, usize)>,
    pub(crate) scroll: ScrollHandle,
}
```

Initialize `thread: None` in `shell/connection.rs` next to `inspector_open: false`. `ThreadWatch` must be visible to `main.rs` (`pub(crate)` + a `use` where `struct Shell` is defined, following how other `shell/*` types are referenced there). Check what `use crate::*;` already brings (`Task`, `Rc`, `HashSet`, `ScrollHandle` are likely in `main.rs`'s imports). Add any missing one to `main.rs`'s `use` list rather than starting a second convention.

- [ ] **Step 4: `sync_thread` and the poll task** in `shell/thread_panel.rs`. Call it from the status tick in `connection.rs`, right before `let sessions = shell.sidebar.sessions();`: `shell.sync_thread(cx);`

```rust
impl Shell {
    /// Starts, keeps or stops the Thread reading to match the panel (status tick, 500 ms).
    /// No work unless the panel is open on an agent with a native id.
    pub(crate) fn sync_thread(&mut self, cx: &mut Context<'_, Self>) {
        let session = self
            .focused_session()
            .and_then(|id| self.sidebar.sessions().iter().find(|s| s.id == id));
        let profile = session.map(|s| s.agent_profile.clone());
        match inspector::wanted_thread(self.inspector_open, session) {
            None => {
                if self.thread.take().is_some() {
                    cx.notify();
                }
            }
            Some(key) if self.thread.as_ref().is_some_and(|w| w.key == key) => {}
            Some(key) => {
                let task = self.poll_thread(key.clone(), profile.unwrap_or_default(), cx);
                self.thread = Some(ThreadWatch {
                    key,
                    task: Some(task),
                    page: None,
                    expanded: HashSet::new(),
                    scroll: ScrollHandle::new(),
                });
                cx.notify();
            }
        }
    }
}
```

`poll_thread(key: (Ulid, String), profile_id: String, cx) -> Task<()>` is a `cx.spawn(async move |this, cx| { … })`. The `WeakEntity` ends the loop when the Shell is gone: `this.update(..)` returns `Err` → `return`. The steps:

1. On the background executor (`cx.background_executor().spawn(..).await`): `mkrew_core::load_dir(&agents_dir)` (with `agents_dir = self.config_dir.join("agents")` cloned before spawning), then `get(&profile_id)?.usage?.format`, then `ThreadFile::new(&format, Box::new(move || mkrew_daemon::transcript_path(&format_for_locate, &native)))`. If there is no profile, no `[usage]`, or `new` returns `Err`, return: the watch stays with `page == None`, which renders no section (the spec's "Not available" rule).
2. Loop:
   - Read off the UI thread, moving the file in and out: `let (back, page) = cx.background_executor().spawn(async move { let changed = file.read(); let page = changed.then(|| file.page()); (file, page) }).await; file = back;`
   - Then `let keep_going = this.update(cx, |shell, cx| { … })`. Inside: if the shell's watch still has this `key` and `page` is `Some`, first note whether `watch.scroll` is at the bottom, then set `watch.page = Some(Rc::new(page))`, call `watch.scroll.scroll_to_bottom()` if it was at the bottom, and `cx.notify()`. Return `shell.thread.as_ref().is_some_and(|w| w.key == key) && !shell.is_exited(key.0)`.
   - `Err(_)` or `false` → `return`. This gives exactly one read after the exit: the read happens before the check. Otherwise `cx.background_executor().timer(Duration::from_secs(1)).await`.
3. "At the bottom": `-f32::from(handle.offset().y) >= f32::from(handle.max_offset().y) - 1.0`, the same fields `scrollbar.rs:70` reads. Verify the gpui `ScrollHandle` API in the vendored gpui source before relying on it (AGENTS.md: look up crate APIs, never guess). A watch with `page == None` counts as at the bottom, so the first page opens at the end.
4. Put a `// ponytail:` comment on the loop: locating a transcript that does not exist yet walks the agent's sessions folder once a second while shown; cache the miss if it shows in a profile.

- [ ] **Step 5: Render.** Add `render_thread_section(&self, session: &Session, theme: &Theme, cx) -> Option<gpui::AnyElement>` in `shell/thread_panel.rs`. It returns `None` when there is no watch for `session.id` or its `page` is `None` or has no turns.

Layout (theme tokens only):
- A title "Thread" styled like `inspector_card`'s title (`font_size::META`, `theme.text_muted`). Below it, a column with `.id("inspector-thread")`, `.flex_1().min_h_0().overflow_y_scroll().track_scroll(&watch.scroll)` and gap `space::M`.
- If `page.truncated`: the first row is `meta_line(theme, "Earlier turns: Open transcript".into())`, clickable (`.id("thread-earlier").cursor_pointer().on_click(cx.listener(move |shell, _, _, cx| shell.open_transcript(id, cx)))`).
- Each turn: a prompt block `div().p(px(space::S)).rounded(px(radius::MEDIUM)).bg(rgb(theme.card)).child(prompt)`, then its items:
  - `PageItem::Text(shown)` → `div().flex().flex_col().gap(px(space::XS)).children(preview::blocks(&shown.blocks, shown, theme))`. If the Docs blocks render too large for the 240–560 px panel (headings are sized for a page), wrap them in `.text_size(font_size::BODY)`. If that is not enough, fall back to plain wrapped text (`div().child(text)`) and note it in HUMAN_CHECKS; the spec allows this.
  - `PageItem::Tools(calls)` → a row `▸ +N tool calls` (`▾` when expanded) in `theme.text_muted` and `font_size::META`, with `.id(SharedString::from(format!("thread-tools-{turn_key}-{item_index}")))`. A click toggles `(turn_key, item_index)` in `watch.expanded` and calls `cx.notify()`. When expanded: one row per call, `name` in `FontWeight(weight::MEDIUM)` and the target in `theme.terminal_font_family`, muted, `.truncate()`.
- While `session.state == SessionState::Working`: a final `meta_line(theme, "Working…".into())`.

Wire it into `render_inspector` (`shell/inspector.rs`):
- When the section is `Some`: the body becomes a flex column. The existing cards div loses `size_full()`, gets `.flex_none().max_h(relative(0.5))` and keeps its `overflow_y_scroll`. The thread section takes the rest (`flex_1().min_h_0()`).
- When it is `None`: keep today's layout exactly. The outer panel div already has `.h_full()`.

- [ ] **Step 6: Build and smoke test**

Run: `cargo build -p mkrew-ui`, then `cargo test -p mkrew-ui inspector` and `cargo test -p mkrew-ui thread_view`.

Smoke test (tests never run a real agent, but a manual run is needed before reporting done):
- Start the dev UI against a **separate** data dir. Never use the user's live daemon or installed app, and never kill them.
- Open a pane with an omp or claude profile and press `Ctrl+Shift+Y`.
- Send one prompt and check that a tool row appears within about 1 s.
- Close the panel and check (Task Manager or `tracing` debug logs) that no more reads happen.

If the GUI cannot run in this environment, say so in the report; the steps are in HUMAN_CHECKS.

- [ ] **Step 7: Docs**

`docs/SPEC.md` §10, after the Plan card paragraph:

```markdown
  **Thread** (agent sessions whose profile has `[usage]`, after the cards, filling the rest of
  the panel): the session's transcript read like a chat. Each prompt sits on a card block, the
  agent's text shows as Markdown, and consecutive tool calls form one `▸ +N tool calls` row (a
  click gives one line per call: name + target). Thinking, tool output and sub-agent lines are
  left out. The file is read only while the panel shows the session: the last 4 MiB first (at
  most 20 turns; older ones give "Earlier turns: Open transcript"), then what was appended,
  checked every second. Closing the panel or focusing another pane stops reading. A muted
  "Working…" ends the list while the agent works. The list follows the bottom unless scrolled
  up (ADR 0055).
```

`docs/decisions/0055-thread-view-reads-transcripts-in-the-ui.md`: follow the format of `docs/decisions/0054-simplified-terminals-screen.md` (read it first). Content:
- **Context:** users read sessions by scrolling TUI frames.
- **Decision:** the UI reads the agent's transcript directly with `mkrew_core::thread::ThreadReader` via `mkrew_daemon::transcript_path`; a 1 s poll only while shown; 4 MiB / 20-turn bounds; no protocol change.
- **Alternatives:** the daemon streams turns over IPC (rejected: protocol and daemon work for every session even when nobody looks); a file watcher (rejected: more moving parts than one `metadata` call per second).
- **Consequences:** the UI depends on agent file formats, as usage already does; Codex prompts come from `user_message` events.

`docs/HUMAN_CHECKS.md`: add a `### To verify — 2026-10-14 Thread view in the session panel` entry under "To verify (non-blocking)", in the format of the entries there:
- open the panel on a running Claude and omp session and watch tools appear live;
- expand and collapse a tool group;
- scroll up while the agent runs (position kept), then back to the bottom (follows again);
- close the panel → no CPU;
- open a long session → fast, with "Earlier turns" shown and clickable;
- check the dark and light themes, and that the Markdown text size fits the panel.

- [ ] **Step 8: Full verify and commit**

Run: `sh scripts/verify.sh` (Git Bash). Fallback: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace -j 4`.
Expected: all green.

```bash
git add crates/mkrew-ui/src/shell.rs crates/mkrew-ui/src/shell/thread_panel.rs crates/mkrew-ui/src/shell/inspector.rs crates/mkrew-ui/src/inspector.rs docs/decisions/0055-thread-view-reads-transcripts-in-the-ui.md
git add -p crates/mkrew-ui/src/main.rs crates/mkrew-ui/src/shell/connection.rs docs/SPEC.md docs/HUMAN_CHECKS.md
git commit -m "feat(ui): live Thread section in the session panel"
```
