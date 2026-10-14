# Thread view in the session panel — Design

- Date: 2026-10-14
- Status: agreed in chat (brainstorming)
- Inspired by: BridgeMind "Thread" mode (https://docs.bridgemind.ai/docs/code-mode#agent-threads)
- Builds on: session panel (SPEC §10, ADR 0038), usage transcripts (SPEC §20E.3)

## Intent

Reading what an agent did means scrolling its terminal: spinners, TUI frames and long tool
output hide the prompt and the conclusion. The user wants to read a session like a chat: what
was asked, what the agent answered, which tools it used (collapsed), updating live while the
agent runs.

The cost must be zero unless the user looks: the transcript is read only while the session
panel is open and shows that session. Opening the panel on an agent that is already running
starts reading then.

Success: the user opens the session panel (`◨` / `Ctrl+Shift+Y`) on a Claude, Codex or omp
pane and sees the last turns as prompt → answer blocks, with `▸ +3 tool calls` rows that expand;
while the agent works, new tool calls and text appear within about a second. Closing the panel
stops all reading.

## Decisions

| Topic | Decision | Why |
| --- | --- | --- |
| Where | A **Thread** section in the session panel, under the Changes card, filling the rest of the panel; it shows the focused pane's session like the other cards | The panel already shows the focused session and has "Open transcript"; Thread is the readable version of that file. No new view |
| Content | User prompts, agent text (Markdown), tool calls as one line each (`name` + target) grouped into a collapsed `▸ +N tool calls` row. Not shown: thinking, tool output, sub-agent (sidechain) lines | User's choice; tool output is most of a transcript's bytes and the terminal still has it |
| Who reads | The UI, directly from the transcript file. No protocol change; the daemon does nothing new | Zero cost when nobody looks; the UI already knows `native_session_id` and depends on `mkrew-daemon` |
| Locating | `mkrew_daemon::usage::{root, locate}` become `pub` (one `transcript_path(format, native) -> Option<PathBuf>` helper); the UI calls it as soon as the session has a `native_session_id` | Same rules as usage (SPEC §20E.3), one implementation; works during the first turn, before any `session.usage` event |
| When it reads | Only while the panel is open **and** its session's profile has `[usage]` **and** it has a `native_session_id`. One background task per shown session; it stops when the panel closes, the focused session changes, or the session exits (one last read first) | The user's rule: load only when looking |
| Live | Every 1 s the task compares the file length with the reader's offset; longer → reads the appended bytes off the UI thread and, if turns changed, notifies the view. No file watcher | Bounded, simple, cheap (one `metadata` call per second while visible) |
| First read | Only the **last 4 MiB** of the file, starting after the first `\n` in that window; at most **20 turns** kept (older ones dropped as new ones arrive). If the window cut turns off, the section starts with a muted line "Earlier turns: Open transcript" | Memory and first-open time stay bounded on long sessions; the full file is one click away in Zed |
| File rewritten / gone | Length < offset → start over (same 4 MiB rule). Missing or unreadable → keep what is shown, locate again at the next tick | Same handling as the usage tracker |
| Not available | Profiles without `[usage]` (Copilot, shells) and sessions without a located transcript show no Thread section (no empty box) | "Empty things hidden" (SPEC §10) |
| Feature flag | None | The panel is closed at every start; closed = nothing runs |
| Turn grouping | A new turn starts at each user prompt line; text and tool calls attach to the current turn; lines before the first prompt in the window are dropped | Matches how the transcript reads; independent of the daemon's turn state machine |

## Parser (`mkrew_core::thread`)

Pure, unit-tested, shaped like `mkrew_core::usage::UsageReader`:

```rust
pub struct ThreadReader { /* format, offset, partial line buffer, turns */ }

impl ThreadReader {
    pub fn new(format: &UsageFormat) -> Result<Self, String>;
    /// Bytes consumed so far (including a pending partial line).
    pub fn offset(&self) -> u64;
    /// Starts reading at `offset` (the 4 MiB window): the first line, up to `\n`, is skipped.
    pub fn start_mid_file(&mut self, offset: u64);
    /// Consumes appended bytes; returns true if the turns changed.
    pub fn feed(&mut self, appended: &[u8]) -> bool;
    pub fn turns(&self) -> &[Turn];
    /// True when turns were dropped (window start or the 20-turn cap).
    pub fn truncated(&self) -> bool;
}

pub struct Turn { pub prompt: String, pub items: Vec<Item> }
pub enum Item { Text(String), Tools(Vec<ToolCall>) }
pub struct ToolCall { pub name: String, pub target: String }
```

- Consecutive tool calls go into one `Item::Tools`; text between them splits the group.
- `target`: the first present of `file_path`, `path`, `pattern`, `command`, `url` in the tool
  input, cut to one line of at most 120 chars; empty if none.
- Per-format rules (field names checked against CURRENT real transcripts when implementing;
  fixtures recorded from real files, anonymised):
  - Claude: `type: "user"` with text content (string or `text` blocks), not `tool_result`, not
    `isSidechain`, not `isMeta` → prompt. `type: "assistant"` non-sidechain: `text` blocks →
    text, `tool_use` blocks → tool calls; `thinking` skipped.
  - Codex: `response_item` `message` role `user` (skipping the injected environment/instructions
    messages) → prompt; role `assistant` `output_text` → text; `function_call` /
    `custom_tool_call` → tool calls (`target` from the parsed `arguments`).
  - omp: `type: "message"` role `user` → prompt; role `assistant` content `text` → text,
    `toolCall` → tool calls.
- Unknown or malformed lines are skipped; an unknown format is an error from `new` (like usage).

## UI

- `crates/mkrew-ui/src/shell/inspector.rs`: `render_thread_section` under the Changes card;
  the reading task lives beside the panel's other state (start on panel open / focus change,
  dropped on close / change; the task holds a weak handle so a dropped panel ends it).
- Layout: section title "Thread" (like `inspector_card` titles); a scrollable column; each turn =
  prompt block on `card` background + answer items. Text through the Docs Markdown render model
  if it fits the panel (checked when implementing; else plain wrapped text). Tool group row
  `▸ +N tool calls` (muted, click toggles; expanded = one row per call, `name` medium weight +
  target in the monospace font, muted). Expanded state is per shown session in UI memory.
- While the session is `working`, a muted "Working…" row ends the list.
- Auto-scroll: stays at the bottom when the user was at the bottom; otherwise keeps position.
- All colors, spacing, radii and font sizes from `theme.rs` (SPEC §14).

## Testing

- `mkrew-core` unit tests on fixtures for Claude, Codex and omp: prompt/text/tool extraction,
  grouping of consecutive tools, sidechain/thinking/tool output skipped, a line split across two
  `feed` calls, `start_mid_file` skipping the partial first line, the 20-turn cap setting
  `truncated`.
- `mkrew-daemon` existing `locate` tests keep covering transcript lookup.
- HUMAN_CHECKS: open the panel on a running Claude session and watch tools appear live; close the
  panel and confirm no reads (no CPU); a long session opens fast and shows "Earlier turns".
- No test calls a real agent or the network.

## Docs

- SPEC §10 session panel: add the Thread section.
- ADR 0057 "Thread view reads transcripts in the UI" (UI reads agent files directly; why not the
  daemon; bounds).
