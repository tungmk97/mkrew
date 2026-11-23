# Chat look like Tern - Design

- Date: 2026-11-23
- Status: approved under the user's standing overnight permission (2026-11-23, "tự làm tiếp, tôi give permission làm phần này"); the user picked "near all of Tern" (option 3) and "Claude Code, Codex and omp" (option 2) in chat.
- ADR: `docs/decisions/0105-chat-look.md` (0104 is the Chat view; this amends it).
- Protocol: 56 -> 57 (the `session.transcript` reply gains the omp recap). Checked: no branch is unmerged into `master` (`git branch --no-merged master` is empty), so 0105 and 57 are free.

## 1. Goal

The Chat view of ADR 0104 works but looks plain next to Tern.
Its spec designed the data (messages, tools, plan, questions) and left the look to the existing Markdown renderer, so the page is a dense full-width wall with one `> tool` row per call, a `> Thinking` row between each, and a flat composer box.
This sub-project gives the Chat view Tern's look and the composer Tern's status chips, for Claude Code, Codex and omp.

How Tern does it (checked on this machine, 2026-11-23):

- Tern runs omp in a normal PTY (`tern.exe daemon` -> `OpenConsole` -> `pwsh` -> `omp.exe`).
- omp tells Tern its session file with `ESC ] 1337 ; SetUserVar=omp_session_file=<base64 path> BEL` when `TERM_PROGRAM=tern`; Tern reads that JSONL and draws it.
- Tern's recap line is omp's idle recap, stored by omp in `~/.omp/agent/history.db`, table `session_recaps(session_id, cwd, created_at, recap)`.
- So Tern and Mkrew have the same architecture (transcript reading); the gap is the drawing and a few fields Mkrew's reader drops.

Success means, in a Chat pane of a Claude, Codex or omp session, dark and light:

- The conversation reads as a centered column with calm spacing: the prompt is a right-aligned bubble, the answer is Markdown with accent headings and accent bullet dots, inline code and file paths are pills, code blocks are cards with a language label, a Copy button and highlighting.
- Thinking and tool calls between two answer texts collapse into one activity row ("Thought for 4s · 3 commands, 2 reads"), which opens to the thinking text and the tool rows.
- An omp turn ends with the omp recap line when omp wrote one.
- The composer is a floating rounded card with a thin context meter above it and a chip row inside: model, thinking level, permission mode, branch, context tokens, tok/s, elapsed time, and the send button.
- Clicking the model chip opens the agent's own model picker.
- The `/` picker shows the current model next to `/model` and the context share next to `/compact`.
- Docs view, Files preview, the Thread section and Ask AI get the shared renderer improvements (code block card, inline code and path pills, accent bullets, block spacing) but keep their own heading color.

## 2. Decisions (standing approval)

| Topic | Decision |
|---|---|
| Agents | Claude Code, Codex, omp: everything that comes from their transcripts. Recap is omp only (the other two write none). |
| Data path | The UI's `ThreadReader` keeps the extra fields (it already reads every line). No new daemon reader, no RPC or ACP. |
| Recap | The daemon reads omp's `history.db` read-only and adds the newest recap of the session to the `session.transcript` reply (protocol 57). Remote machine windows get it too, since the daemon of that machine reads it. |
| OSC 1337 | Not used. Mkrew already finds the transcript by the hook's native id; setting `TERM_PROGRAM=tern` would also switch on omp's Tern browser backend. |
| Renderer scope | Shared improvements go to every Markdown view; accent headings, chat spacing and the centered column are Chat only (a `Flavor` argument). |
| Model chip | Click: the pane switches to Terminal and `/model` + Enter is sent to the agent, so its own picker opens (like "Answer in terminal" in ADR 0104). No model list is kept in Mkrew. |
| Branch chip | Shown only when the session runs at the workspace root (its folder is the workspace's repo); a task in a worktree shows its folder chip instead of a wrong branch. |
| Composer collapse | When the pane loses focus and the draft is empty, the card shrinks to a one-line pill with the placeholder; a click or focus opens it. |
| Placeholder | The session title in quotes (transcript title, else the pane title), else "Message the agent…". |

## 3. Data (`mkrew_core::thread`)

All additions are additive; existing tests keep passing.

### 3.1 Per turn

`Turn` gains:

- `started_at: Option<i64>` (epoch ms): the prompt line's timestamp.
- `ended_at: Option<i64>`: the newest timestamp seen in the turn.
- `output_tokens: u64`: the sum of output tokens of the turn's assistant messages.
  Claude lines that share one `message.id` count once (the usage reader's rule).
- `generation_ms: u64`: for each assistant message, its line timestamp minus the timestamp of the line before it in the same turn, summed.
  tok/s = `output_tokens * 1000 / generation_ms` when `generation_ms > 0`.

Timestamps per format:

| Format | Line timestamp |
|---|---|
| Claude | `timestamp` (ISO 8601) |
| Codex | `timestamp` (ISO 8601) |
| omp | `timestamp` (ISO 8601) |

Output tokens per format:

| Format | Field |
|---|---|
| Claude | assistant `message.usage.output_tokens` |
| Codex | `event_msg` `token_count` `info.last_token_usage.output_tokens` (each one belongs to the current turn) |
| omp | assistant `message.usage.output` |

`Item::Thinking` becomes `Item::Thinking { text: String, ms: Option<u64> }`.
`ms` is the thinking line's timestamp minus the previous line's (Claude writes each content block as its own line), or the assistant message's generation time when the thinking shares a line with other blocks (omp).
Claude's redacted thinking (`"thinking": ""` with a signature) is now kept as a Thinking item with empty text, so the activity row can say "Thought for 3s"; it does not open.

### 3.2 Per session

`ThreadReader::meta() -> &SessionMeta`, the newest value seen of each:

| Field | Claude | Codex | omp |
|---|---|---|---|
| `model` | assistant `message.model` | `turn_context.payload.model` | `model_change.model`, then assistant `message.model` |
| `effort` | none (only in API requests) | `turn_context.payload.collaboration_mode.settings.reasoning_effort` | `thinking_level_change.thinkingLevel` |
| `permission` | user line `permissionMode` and separate `permission-mode` lines (the newest wins) | `turn_context.payload.approval_policy` | none |
| `title` | `ai-title.aiTitle` | none | `title.title` / `title_change.title` |
| `context` | input + cache read + cache write of the newest assistant message | `last_token_usage.total_tokens` | input + cacheRead + cacheWrite of the newest assistant message |
| `context_window` | none | `token_count.info.model_context_window` | none |

`SessionMeta` lives with the reader, so a reader started mid-file (`start_mid_file`) knows only what it has seen; the composer then shows fewer chips, never wrong ones.
Model names are shown short: the part after the last `/`, with `claude-` dropped and dashes between digits turned into dots (`anthropic/claude-opus-5-5` -> `opus 5.5`); a pure function with tests.

### 3.3 Recap (daemon, omp only)

- `SessionTranscript` reply gains `recap: Option<Recap { text: String, at: i64 }>` (serde default, protocol 57).
- The daemon fills it for `omp_jsonl` sessions with a native id: `SELECT recap, created_at FROM session_recaps WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 1` on `<omp root>/history.db`, opened read-only (`SQLITE_OPEN_READ_ONLY`, busy timeout 200 ms).
- A missing file, table or row, or any SQLite error, gives `None` (logged at `debug`); the transcript reply never fails for it.
- `ponytail:` one small query per poll of a shown omp session; cache by the file's mtime if it ever shows up in a profile.
- The UI shows the recap after the last turn when `recap.at` (seconds) is not older than that turn's `started_at`, and only while the session is not working.

## 4. UI

### 4.1 Chat model (`mkrew_ui::chat`)

Pure view model, unit tested:

- Consecutive `Thinking` and `Tools` items between two `Text` items (or the turn's ends) become one `Row::Activity { key, thinking_ms, counts, children, open }`.
  - `counts` per tool kind: commands (bash, shell, exec_command, Bash), reads (read, Read, view, grep, Grep, glob, Glob, find, search), edits (edit, Edit, write, Write, MultiEdit, apply_patch, ast_edit), web (web_search, WebSearch, WebFetch, fetch, browser), agents (task, Task, Agent), other.
  - Label: `Thought for {s}s` when there is thinking with a time, then `·` and the counts in that order, e.g. "Thought for 4s · 3 commands, 2 reads, 1 edit"; plural rules for 1; "Working…" while the turn is the running one and the row is last.
  - Children: Thinking rows (italic, muted, sans, opens to its text when the text is not empty) and the existing Tool rows (unchanged: name, target, open to input and result).
- `Row::Recap { text }` after the last turn under the rule in 3.3.
- `ComposerStats { model, effort, permission, branch, context: Option<(u64, Option<u64>)>, tok_s: Option<u32>, elapsed_ms: Option<u64> }` from `SessionMeta`, the newest turn, the usage event (`context_window` from the profile wins over the transcript's), the git summary rule in section 2, and `now` (live elapsed while the turn runs).
- `format_elapsed(ms)`: `42s`, `2m 46s`, `1h 03m`.

### 4.2 Markdown renderer (`preview.rs`, shared)

- `Flavor::{Doc, Chat}` argument on `preview::message`, `blocks` and `page`; every caller passes `Doc` except the Chat view.
- Code block: a card (`surface_raised`, `radius::MEDIUM`, 1 px `divider` border) with a header row (language label `META` muted on the left, a Copy `quiet_button` always visible on the right, "Copied" for 1.5 s after a click) and the highlighted body below with `space::M` padding.
  A block without a language shows "text".
- Inline code: a pill (`surface_raised`, `radius::SMALL`, `space::XS` horizontal padding, mono at `doc::CODE`).
- Path pill: an inline code span that looks like a path (has a `/` or `\` or ends in `.<ext>` of 1 to 8 letters or digits, has no spaces, no `://`) shows the folder part muted and the file name in `weight::SEMIBOLD`; in the Chat view it is pressable and opens the file with `open_file_at` in the session's workspace and worktree (`:line` suffix honored); elsewhere it is not clickable.
- Lists: bullets are a small filled dot in `accent`; numbers stay `text_muted`.
- Block spacing: `space::M` between blocks in `Chat`, unchanged in `Doc`; body line height `doc::LINE_HEIGHT`.
- Chat flavor headings: color `accent_text`, sizes one step smaller than Doc (h1 uses `HEADINGS[1]`), no underline.
- `preview_select.rs` piece counting follows every structure change (the existing `debug_assert` must keep holding).

### 4.3 Chat page (`shell/chat_view.rs`)

- A centered column `max_w(CHAT_COLUMN_WIDTH)` (new token, 760) with `space::XL` side padding, used by the list, the tray, the question card and the composer, so they line up.
- Turns are `space::XL` apart; the rows of a turn `space::M`.
- Prompt bubble: right aligned, `max_w(relative(CHAT_PROMPT_MAX_FRACTION))`, `card` background, `radius::LARGE`, `space::S` x `space::M` padding, body text.
- Activity row: chevron, label in `META` muted italic for the thinking part; pressable `Look::Text`; a keyboard stop like today's disclosures.
- Recap row: `Icon::History` (add if missing) and the text in italic `text_muted`, `space::S` gap.
- Empty state unchanged.

### 4.4 Composer card (`shell/composer.rs`, Chat mode only)

Terminal mode keeps today's docked composer.
In Chat mode, inside the centered column, `space::M` above the pane's bottom edge:

1. Context meter: a 2 px track (`divider`) with the used share filled in `accent` (`status_needs_input` from `usage::WARNING_PERCENT`); `META` muted labels: the percent on the left, the window (`format_tokens`) on the right. Hidden without a window.
2. The card: `surface_raised`, 1 px `glass_border`, `radius::LARGE`, `shadow_float`, `space::M` padding, `space::S` gap.
   - The queue rows as today.
   - The field, placeholder per section 2, `COMPOSER_MAX_HEIGHT` as today.
   - The chip row: model chip (`Look::Chip`, chevron-down icon, opens the agent's picker), effort chip, permission chip, branch chip (`Icon::GitBranch`, `branch` plus `*N` changed in `text_muted`), then a flexible gap, then stats in `META` muted mono (`74K`, `102 tok/s`, `2m 46s`), the ✦ button and "Prompts…" as today, and a round send button (`radius::PILL`, `accent` with `on_accent` icon `ArrowUp`, disabled look while the draft is blank or busy; tooltip "Send (Enter)").
   - The status line ("Sending…", "Improving…", an error) replaces the stats while it shows; the key hint moves into the send tooltip.
3. Collapsed (pane not focused, draft empty, no queue): a pill `radius::PILL`, the placeholder in muted text and the send icon; a press focuses the composer.

Narrow panes: chips drop from the right in this order: stats, permission, effort, branch; the model chip and send stay.

### 4.5 Slash picker hints

In the `/` picker rows of a Chat or Terminal session, `/model` shows the short model name and `/compact` shows "context N% used" (from the usage event) in the row's muted right text, when known.

## 5. Error handling

| Case | Behavior |
|---|---|
| A line without a timestamp | It is used for content; times that need it stay `None` and their chips hide. |
| Clock skew (a later line with an older time) | Negative spans count as 0; `ended_at` keeps the newest. |
| `generation_ms` is 0 | No tok/s chip. |
| omp `history.db` missing, locked or of another shape | No recap, no error. |
| Model name in an unknown shape | Shown as is, cut to 24 characters with an ellipsis. |
| Path pill for a file that does not exist | `open_file_at` behaves as for any missing file (its existing message); the pill is still drawn. |
| A session in a worktree | No branch chip (section 2). |
| A machine window (remote daemon) | Same data; the recap comes from that machine's daemon. |

## 6. Testing

- Core: fixture lines per format for timestamps, output tokens (Claude de-duplication by `message.id`), generation time and tok/s, thinking time (Claude own line, omp shared line), redacted Claude thinking kept with empty text, every `SessionMeta` field, a reader started mid-file, skewed and missing timestamps.
- Daemon: recap from a temp `history.db` with two rows (newest wins), no file, no table, and a non-omp format (always `None`).
- UI model: activity grouping (thinking only, tools only, mixed, between texts, at turn end, running turn shows "Working…"), the counts and plural rules, recap shown only per the rule, `ComposerStats` (live elapsed, tok/s rounding, context window precedence, branch only at the root), short model names, `format_elapsed`, path detection (positive and negative cases), chip drop order by width.
- Renderer: `preview_select` piece counts for a code block card and pills (the `debug_assert` runs in tests).
- No test runs a real agent.
- `docs/HUMAN_CHECKS.md`: Chat with a real omp, Claude and Codex session, dark and light, narrow and wide pane; compare with Tern side by side.

## 7. Docs

- ADR 0105 (decisions above; OSC 1337 not used and why; recap from omp's journal).
- SPEC: §11 Chat mode look and composer card, §20B composer chips and model chip, protocol table (57, `recap` field).
- ADR 0104 gets "Amended by 0105" for the look.

## 8. Out of scope

- Answering a question from its card (still ADR 0104's rule).
- A model list or model switching inside Mkrew (the agent's own picker opens).
- Recaps for Claude Code or Codex, or generating recaps with a model.
- Per-session branch for worktree sessions (the folder chip covers it).
- Editing or re-running a past message.
