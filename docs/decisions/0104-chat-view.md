# 0104 - Chat view: a mode of the agent pane that reads the transcript

- Date: 2026-11-22
- Status: accepted (extends ADR 0057; amends SPEC sections 11, 13 and 20B)
- Milestone: none (human request 2026-11-22, spec `docs/superpowers/specs/2026-11-22-chat-view-design.md`)

## Context

Tern can show an agent as a chat: messages, tool calls with their arguments and results, a plan card, the agent's questions as cards, running subagents in a tray, and a composer at the bottom.
The user liked it.
Mkrew already parses Claude, Codex and omp transcripts into a read-only Thread section of the session panel (ADR 0057) and has a composer with a send queue, pickers and attachments (SPEC section 20B).

## Decision

- Chat is a mode of an agent pane, next to its terminal, not a new panel.
  A `Terminal | Chat` switch in the pane header (and `Ctrl+Alt+T`, palette "Agent: Toggle chat view") flips it.
- The data is the transcript, read by the same `ThreadReader` through `session.transcript`.
  The reader now also keeps tool call ids, arguments (4 KB) and results (8 KB, with an error mark), thinking (8 KB), the latest plan, a pending `AskUserQuestion`, and the turn's `Task` / `Agent` subagents.
  Only the newest 50 calls of a turn keep their arguments and result; older calls keep name, target and error mark, and their body says "Open transcript" like a cut one, so a long autonomous turn stays bounded.
  A cut tool or thinking body shows an "Open transcript" link under it.
  One reader of 50 turns per session serves the panel and Chat (see below); the Thread section shows its newest 20.
- The plan comes from Claude `TodoWrite`, Codex `update_plan`, or the Claude task tools.
  Current Claude Code uses `TaskCreate`, `TaskUpdate` and `TaskList` by default and `TodoWrite` is off.
  For these the reader keeps one checklist for the session: a `TaskCreate` result adds a step (the id from `toolUseResult.task.id`, else `#N` in the result text), `TaskUpdate` changes or deletes the step with its `taskId`, and a `TaskList` result replaces the list.
  Every change sets the current turn's plan to a copy of the checklist.
  Before the first turn of the window a `TaskList` result is taken only when every entry in its `tasks` reads.
- A Claude subagent can run in the background.
  Its result at launch (`toolUseResult.status` is `async_launched`, or the text starts "Async agent launched") does not mean it is done.
  It keeps running in the tray until a later `<task-notification>` user line names it, by `<tool-use-id>` or by the `agentId` recorded at launch, with any status (completed, failed, stopped).
  A foreground result marks it done at once.
  The notification is still not a prompt.
- The Codex tool output shapes follow current Codex: a JSON string `{"output", "metadata": {"exit_code"}}`, or plain text with an `Exit code: N` or `Process exited with code N` line before the `Output:` line.
  A non-zero code marks the result as an error.
  Lines after `Output:` are the command's own and are never read as an exit code.
- One reader per session is shared by the panel and every pane in Chat mode (`Shell::reads`), so a transcript is never read twice.
  The shared reader keeps 50 turns and the Thread section shows the newest 20.
- The mode is saved per pane in the tab layout: the leaf gains `chat: true | false`.
  An absent flag means not set, and then `[ui] chat_default` decides (default false, Terminal).
  So there are three states: Chat, Terminal, and "the default decides".
  The daemon stores layouts as opaque JSON, so no protocol or daemon change is needed and older layouts read unchanged.
- The terminal keeps running underneath: nothing about how the agent runs, resumes or reports state changes; its element is just not drawn.
- The composer is the existing one, docked and always shown in Chat.
  Enter sends and Shift+Enter adds a newline there; Esc moves the keys to the chat list.
  The Terminal-mode composer keeps Ctrl+Enter.
- The chat list is reachable by keyboard: arrows, PageUp, PageDown, Home and End scroll; the tool and thinking row toggles, the "earlier turns" link and "Answer in terminal" are Tab stops with the focus ring that Enter and Space press; Tab leaves the last one for the composer.
- A question is a card with the question and its options.
  Its "Answer in terminal" button switches the pane to Terminal.
- Chat is offered for agents with a readable transcript format: the shipped Claude, Codex and omp profiles, or any agent whose transcript the daemon located.

## Alternatives

- ACP (Agent Client Protocol) as the data source: rejected.
  It runs the agent headless, so the PTY, resume, hooks and screen patterns go, and it needs a Node adapter per agent; omp and Copilot have none.
- Answering a question or permission prompt from the card by typed keys: left out.
  It needs one fragile key map per agent TUI.
- A new panel instead of a pane mode: rejected.
  The pane already owns the session, its status and its composer.

## Consequences

- The UI depends on three more parts of the agents' file formats; a format change shows as a missing row, never as a broken pane.
- The task tool shapes and the Codex output shapes come from the agents' documented formats, not from a recorded session; `docs/HUMAN_CHECKS.md` asks for a check against real Claude Code and Codex sessions.
- ADR 0057's exclusion of tool output and thinking is superseded in part: Chat shows them, the Thread section still does not.
- A text selection works inside one message, not across messages (ADR 0072 selects within one page).
- Every change re-parses the Markdown of at most 50 turns; cache by text if it ever shows.
