# 0105 - Chat look: Tern's drawing on Mkrew's transcript reader

- Date: 2026-11-23
- Status: accepted (amends ADR 0104 for the look; amends SPEC sections 7, 11 and 20B)
- Milestone: none (human request 2026-11-23, spec `docs/superpowers/specs/2026-11-23-chat-look-design.md`, plan `docs/superpowers/plans/2026-11-23-chat-look.md`)

## Context

ADR 0104 gave the agent pane a Chat mode and designed the data; it left the look to the Markdown renderer.
The page was a dense full-width list with one row per tool call, one row per thinking block and a flat composer box.
Tern shows the same transcripts as a calm centered column with status chips on the composer.
Checked on the user's machine on 2026-11-23: Tern runs omp in a normal PTY, omp tells Tern its session file with `ESC ] 1337 ; SetUserVar=omp_session_file=<base64 path> BEL` when `TERM_PROGRAM=tern`, and Tern reads that JSONL.
Tern's recap line is omp's idle recap, which omp stores in `~/.omp/agent/history.db`, table `session_recaps(session_id, cwd, created_at, recap)`.
So Tern and Mkrew have the same architecture; the gap is the drawing and a few fields Mkrew's reader dropped.

## Decision

- The look is Chat only: a centered column of 760 px (`CHAT_COLUMN_WIDTH`), a right-aligned prompt bubble, calm spacing, accent headings and bullets, and a floating composer card.
- Thinking and tool calls between two answer texts become one activity row ("Thought for 4s · 3 commands, 2 reads") that opens to the thinking text and the tool rows.
  The opened thinking body is drawn in the UI font, italic, muted.
- The reader keeps per-turn start and end times, output tokens (Claude lines of one `message.id` count once), generation time, the time of each thinking block, Claude's redacted thinking (empty text), and a `SessionMeta` (model, effort, permission, title, context, window).
  The composer chips and the context meter come from them; the profile's `context_window` wins over the transcript's.
- Claude's permission mode is read from the user lines' `permissionMode` and from the separate `permission-mode` lines; the newest wins.
- Codex generation time also counts the `event_msg` lines `agent_message` and `agent_reasoning`.
- The context meter and the header ctx% depend on the daemon's `session.usage` event, which arrives at a turn end; until then the meter is hidden or shows the transcript's last value.
- The data path is unchanged: the UI's `ThreadReader` reads the transcript through `session.transcript`.
  No new reader, no RPC, no ACP.
- omp's recap is read by the daemon from `history.db`, opened `SQLITE_OPEN_READ_ONLY` with a 200 ms busy timeout, and added to the `session.transcript` reply as `recap { text, at }` (protocol 57).
  A missing file, table or row, or any SQLite error, is no recap.
  The UI shows it after the last turn when it is not older than that turn's prompt and the agent is not working.
  A machine window gets the recap of that machine's daemon.
- When a hook reports a new native session id, the daemon also sends `session.updated` with the session.
  Before, the id reached clients only with the next session list, so a Chat pane or the Thread section opened before the agent's first hook stayed "No messages yet" until the UI reconnected.
- OSC 1337 is not used.
  Mkrew already finds the transcript by the hook's native session id, and `TERM_PROGRAM=tern` would also switch on omp's Tern browser backend.
- The Markdown renderer gets a `Flavor` (`Doc` or `Chat`).
  Shared for every view: the code block card (language label, an always visible Copy button that says "Copied" for 1.5 s), path colors in inline code, accent bullet dots.
  Chat only: accent headings one step smaller without a rule, `space::M` between blocks, and clickable path pills.
- The model chip does not list models.
  A click switches the pane to Terminal and types `/model` + Enter with `session.send`, so the agent's own picker opens (like "Answer in terminal" in ADR 0104).
  On an exited session the chip is disabled.
- The branch chip shows only when the session runs at the workspace root; a task worktree session shows no branch chip and no folder chip.
- The composer collapses to a one-line pill while the pane has no keyboard and the draft, the queue and the error are empty.
- The `/` picker shows the current model next to `/model` and the context share next to `/compact`.
- Tern-style details (second pass, 2026-11-23):
  A Chat table has no outer border, no radius and no header fill: the header cells are muted regular text, a 1 px `divider` line runs under the header and between rows, there are no vertical lines, and the first column lines up with the paragraphs.
  The Doc flavor keeps its bordered table.
  Inline code in Chat is drawn in `accent_text` on the raised background; a path pill keeps the folder muted and the name bold.
  The composer chip row reads, left to right: the model as a bordered pill (`control_border`, chevron, name in `accent_text`, the loading dots before the name while the agent works), permission with a lightning icon in `status_needs_input`, effort, the branch with its icon and `name *N` in `syntax_string`, the context tokens with a database icon and the number in `syntax_keyword`, `tok/s` in `text`, the elapsed time with a clock icon, and the local wall-clock time the last turn ended (`HH:MM:SS`, clock icon in `text_subtle`, hidden while the agent works).
  Stats and the branch use the terminal font at META size.
  A narrow pane drops the clock time first, then the stats, permission, effort and branch.
  The placeholder is italic muted with a small sparkle icon.
  The send button is neutral (raised surface, 1 px `control_border`, muted corner-down-left icon) while the draft is blank or busy, and an `accent` fill with an `on_accent` icon when it can send.
  When the chat list is scrolled up from its end, a small round button with a down arrow floats at the right edge of the column above the composer and scrolls to the end.
  The icon set gained clock, corner-down-left, database and zap (Lucide, ISC).
- Terminal or Chat is kept per pane in the tab layout (`chat` on the leaf) and survives a UI restart.
  The daemon's startup resume only swaps the leaf's `session` value, so the `chat` flag carries over to the resumed session (tested in `replace_session` and in the resume integration test).
- Whenever the user picks Terminal or Chat (the header switch, `Ctrl+Alt+T`, the palette), Mkrew also writes `[ui] chat_default = <picked is Chat>` to `settings.toml` through the Settings writer, only when it differs, and reloads settings at once.
  New agent panes and the next start use the last pick; the Settings page General still shows and edits it.
  Panes with no saved mode follow the default, so a pick first pins every such pane to its current mode, and they do not flip.
  The temporary switches to Terminal (the model chip, "Answer in terminal") are not picks and do not change the default.

## Alternatives

- Setting `TERM_PROGRAM=tern` and reading OSC 1337: rejected, see above.
- A separate recap reader in the UI: rejected, the file is on the daemon's machine (SPEC section 3), so a remote window would read the wrong disk.
- A model list inside Mkrew: out of scope; the agents already have pickers.
- Opening `history.db` with `immutable=1` to avoid any side file: rejected, it can misread while omp writes.

## Consequences

- Protocol 57: a UI from before it shows a version mismatch with an old daemon; the user restarts the daemon.
- `mkrew-daemon` depends on `rusqlite` (same version and features as `mkrew-store`, no new package in the tree).
- Inline code and path pills are a background highlight without radius or padding; GPUI's `HighlightStyle` has neither, so the "pill" has square corners.
- `history.db` is opened read-only, but on a WAL database SQLite may leave `history.db-wal` and `history.db-shm` beside it after omp exits; accepted.
- The reader keeps a map of Claude message ids per turn, bounded by the messages of one turn.
- Every poll of a shown omp session runs one small read-only query; cache by the file's mtime if it ever shows in a profile.
- Until the first `session.usage` event of a session (a turn end), the context meter and the header ctx% have no fresh value.
