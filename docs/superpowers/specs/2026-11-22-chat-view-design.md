# Chat view - Design

- Date: 2026-11-22
- Status: approved under the user's standing approval of the controller's recommendations (2026-11-22, "assume OK with your recommendations"); sub-project 5 of the Tern-inspired update (Polish 1, Theme gallery 2, Widget bar 3, Review flow 4).
- ADR: `docs/decisions/0104-chat-view.md` (0101 Theme gallery, 0102 Widget bar, 0103 Review flow); it extends ADR 0057 (thread view reads transcripts).
- Protocol: no change (the UI already reads the transcript through `session.transcript`).

## 1. Goal

Tern can show an agent as a native chat: messages, tool calls with their arguments and results, a plan card, the agent's questions as cards, running subagents in a tray, and a composer at the bottom.
The user liked it.
Mkrew already parses Claude, Codex and omp transcripts into a read-only Thread section in the session panel (ADR 0057), and already has a composer with a send queue, slash, prompt and `@` pickers, and attachments (SPEC §20B).
This sub-project turns that into a Chat mode of an agent pane, next to its terminal.

Success means:

- An agent pane can switch between Terminal and Chat (a pill in the pane header and `Ctrl+Alt+T`); the choice is kept per session.
- Chat shows the turns as messages, tool calls with arguments and results (collapsed), thinking (collapsed), the latest plan as a card, a pending question as a card, and the agent's running subagents in a tray.
- The composer is docked under the chat and always open there; sending, queueing and the pickers work as today.
- The terminal keeps running underneath: nothing about how the agent runs, resumes or reports state changes.

## 2. Decisions (standing approval)

| Topic | Decision |
|---|---|
| Data source | The transcript, as today (ADR 0057). ACP is rejected: it runs the agent headless, so the PTY, resume, hooks and screen patterns go, and it needs a Node adapter per agent with no omp or Copilot adapter. |
| Where | A mode of the agent pane, not a new panel. The session panel keeps its Thread section. |
| Default | Terminal. Chat is opt-in per session; `[ui] chat_default = false` makes new agent panes open in Chat when true. |
| Agents | Claude, Codex and omp (the formats `ThreadReader` reads). Other agents have no Chat pill. |
| Turns kept | 50 in Chat (the Thread section keeps 20); older ones behind "Earlier turns: Open transcript". |
| Questions | Shown as a card with the question and its options; the answer is given in the terminal: the card's "Answer in terminal" button switches the pane to Terminal. Answering from the card by typed keys is out of scope (one fragile key map per agent TUI). |
| Subagents | Claude only: the tray lists the turn's `Task` / `Agent` tool calls with their description and type, running until their result arrives. Codex and omp have none. |
| Thinking | Collapsed rows "Thinking" that open to the text, Claude and omp only. |

## 3. Parser (`mkrew_core::thread`)

- `ThreadReader` keeps its incremental shape and its tests; new fields are additive.
- `ToolCall` gains:
  - `id: String` (the tool_use / call id, empty if none),
  - `input: String` (the arguments as pretty JSON, cut to 4 KB on a char boundary),
  - `result: Option<ToolResult>` with `text` (cut to 8 KB) and `is_error`.
- A result line (Claude `tool_result`, Codex `function_call_output`, omp tool result) fills the call with the same id in the current turn; a result with no matching call is dropped.
- `Item` gains `Thinking(String)` (cut to 8 KB).
- `Turn` gains `plan: Option<Vec<PlanStep>>`, the latest checklist in the turn, with `PlanStep { text, status: Pending | InProgress | Done }`.
  It comes from Claude `TodoWrite` (the whole list), Codex `update_plan` (the whole list) or Claude's task tools, which current Claude Code uses by default instead of `TodoWrite`.
  For the task tools the reader keeps one session checklist: a `TaskCreate` result adds `{id, subject}` (the id from the entry's `toolUseResult.task.id`, else `#N` in the result text), `TaskUpdate` changes the step with its `taskId` (`status`, `subject`; `deleted` removes it), and a `TaskList` result replaces the list.
  Every change sets the current turn's `plan` to a copy of the checklist.
  An empty list sets `Some(empty)`, which the UI shows as no plan.
  Steps are capped like `mkrew_core::plan` (`MAX_ITEMS` steps, `MAX_TEXT_CHARS` per text).
- `Turn` gains `question: Option<Question>`: the last Claude `AskUserQuestion` tool call of the turn that has no result yet, with its question text and option labels (text capped like plan steps).
- `Turn` gains `subagents: Vec<Subagent>`: Claude `Task` / `Agent` calls with `description`, `subagent_type` and `done`.
  A foreground result marks it done.
  A background launch (result `toolUseResult.status` `async_launched`, or text starting "Async agent launched") keeps it running and records its `agentId`.
  A later `<task-notification>` user line marks it done (any `<status>`: completed, failed, stopped), matched by `<tool-use-id>` to the call id, else by `<task-id>` to the recorded `agentId`, in any kept turn; the notification is still not a prompt.
- `MAX_TURNS` becomes a reader argument: the Thread section passes 20, Chat 50.
- Claude `isSidechain` lines stay skipped.

## 4. UI

### 4.1 Mode switch

- The pane header gets a two-segment `Terminal | Chat` control (`Look::Chip`) for sessions whose profile has a readable format; a single pane shows it next to its pill (ADR 0054).
- `Command::ToggleChat`, default key `ctrl-alt-t` (free today), and a palette entry "Toggle chat view".
- The mode is per session and is kept across restarts in the persisted tab layout (`tabs.rs` `Node` JSON, `tab.set_layout`): the pane's leaf gains an optional `chat: true`, absent meaning Terminal, so older layouts read unchanged.
- In Chat mode the terminal element is not drawn but its session keeps running and its screen is still read for state.

### 4.2 Chat page

- New file `crates/mkrew-ui/src/shell/chat_view.rs`; it reuses the Thread section's polling (`session.transcript`, 1 s, byte offset) through a shared reader per session, so the panel and the pane do not read twice.
- Layout, top to bottom: the turns (scrolling, follows the bottom unless the user scrolled up), the subagent tray (only when a subagent runs), the question card (only when one is pending), the composer.
- A turn: the prompt as a right-aligned card (`theme.card`), the agent's text as Markdown with the existing renderer, tool groups as rows `name target` with a chevron; an open row shows the arguments and the result in monospace, the result in `theme.danger_text` when `is_error`.
- The plan card: one row per step with a status mark (empty ring, half ring, check), the in-progress step in `theme.text`, others `theme.text_muted`.
- The question card: the question, the options as a list, and the "Answer in terminal" button (`pressable`, Look::Raised).
- The subagent tray: one chip per subagent with its description and a spinner or a check.
- Width follows `[ui] reading_width` like the Docs view.
- Empty state: "No messages yet. Type below to start." over the composer.
- Copy: text in messages is selectable like the Markdown preview (ADR 0072).
- All colors, spacing and sizes are `theme.rs` tokens; every clickable element is a widget or `.pressable(..)`; dark and light.

### 4.3 Composer

- In Chat mode the composer is docked and always visible; Esc moves focus to the chat list instead of hiding it.
- Enter sends and Shift+Enter adds a newline in the docked composer (chat convention); the overlay composer of Terminal mode keeps Ctrl+Enter.
- The queued texts show under the composer as today.

## 5. Error handling

| Case | Behavior |
|---|---|
| No transcript found yet | Chat shows the empty state; the composer works. |
| Unknown transcript format | No Chat pill. |
| Transcript format drift (lines not understood) | Skipped as today; the chat shows what it understands. |
| Tool result for an unknown id | Dropped. |
| Huge arguments or results | Cut at 4 KB / 8 KB with "… (cut)"; "Open transcript" shows all. |
| Session exits | The chat stays readable; the composer shows the existing "exited" state. |

## 6. Testing

- Core: fixture lines per agent for a tool call with its result, an error result, thinking, a TodoWrite / update_plan plan, a pending and an answered AskUserQuestion, a Task subagent running and done, cutting at 4 KB / 8 KB on a char boundary, and the turn limit argument.
- UI: the chat's view model from a `Turn` list as a pure function (rows, open state, plan marks, question shown only while pending, tray only while running); the mode toggle per session and its persistence; the docked composer's Enter vs Shift+Enter.
- No test runs a real agent.
- `docs/HUMAN_CHECKS.md`: a real Claude and Codex session in Chat, dark and light, with a tool error, a plan, a question and a subagent.

## 7. Docs

- ADR 0104 (decisions above; ACP rejected and why; answering from the card left out).
- SPEC: §11 (Chat mode of a session), §13 (`Ctrl+Alt+T`), §20B (docked composer keys); ADR 0057 gets a "Superseded in part by 0104" note for the excluded tool output and thinking.

## 8. Out of scope

- ACP or any headless agent mode.
- Answering a question or permission prompt from the card.
- Subagents for Codex and omp; a subagent's own messages.
- Chat for Copilot or other formats.
- Editing or re-running a past message.
