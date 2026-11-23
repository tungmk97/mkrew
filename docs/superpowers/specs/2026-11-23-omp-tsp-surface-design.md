# omp native surface (TSP), part 1 - Design

- Date: 2026-11-23
- Status: design approved in chat by the user (2026-11-23), section by section; this file waits for the user's review.
- ADR: `docs/decisions/0107-omp-native-surface.md` (0106 is taken by `feat/tern-layout`).
- Protocol: 57 on `master`, 58 on `feat/tern-layout` (unmerged), so this change takes 59.
- Migration: none.
- Inputs, committed with this spec:
  - `2026-11-23-omp-tsp/tsp-reference-omp-18.13.2.md`: the Tern Surface Protocol (TSP) as omp 18.13.2 implements it, read from the JavaScript embedded in `omp.exe`, every claim cited by byte offset.
  - `2026-11-23-omp-tsp/omp-18.13.2-startup.tsp.jsonl`: omp's own record (`PI_TUI_TSP_RECORD`) of a real startup: `o`, `t`, 10 frames and their acks.

## 1. Goal

The user wants omp panes in Mkrew to look and work like omp in Tern: the composer of omp (model chip, effort, context meter, tok/s, cost, Send and Stop), the subagent and job pills, and later the Agent Hub and the Jobs panel with Kill, Revive and Cancel.

Tern gets all of this from omp itself.
omp describes its whole interface as a tree of nodes and sends it over TSP, an escape-sequence protocol inside the PTY stream; Tern draws the tree and sends clicks back.
So the way to match Tern is a TSP renderer in Mkrew, not a copy of Tern's screens.

The work has three parts, each with its own spec and plan:

| Part | Content |
|---|---|
| P1 (this spec) | Transport in the daemon, the node document, protocol to the UI, drawing `main`, `dock` and `layer` with the common kinds, clicks back to omp. |
| P2 | `picker`, `tree`, `tool`, `checklist`, `prefs`, hovercards: the Agent Hub as Tern's table with Kill, Revive, Open transcript; the full Jobs panel with Cancel. |
| P3 | The remaining kinds and features: blobs cache, `media`, `chart`, `diff` and `math` drawn natively, local text editing (`edit`, `undo`, `send`). |

Success for P1, in an omp pane with TSP on, dark and light:

- The pane shows omp's conversation as native rows (cards, Markdown, code, status) and omp's dock at the bottom: HUD pills, status, context meter, editor, the bar with model, effort, rate, cost and Send or Stop.
- Typing, omp's shortcuts and pasting work as in a terminal pane today.
- Clicking the model chip, Stop, the agent pill or the jobs pill does what it does in Tern (omp opens its own selector or overlay).
- Without TSP (turned off, an omp without it, a ConPTY that drops it) the pane is today's terminal pane; nothing is blank.
- Model token use does not change: TSP only changes how omp draws.

## 2. What the user said, and what was assumed

Said by the user (2026-11-23):

- Follow Tern's composer and Tern's subagent management view (Agents dialog, Background jobs dialog, pill hover texts).
- "Giống Tern hoàn toàn": do it through TSP, not by reading files, after the alternatives (files only, omp RPC mode) were shown.
- Agents: omp and Claude Code. TSP exists only in omp; Claude Code keeps today's Chat view (said in chat, not contradicted).
- TSP is a setting per agent profile, on by default; no Terminal | Chat switch inside an omp TSP pane.
- The composer is omp's dock, as in Tern.
- Colors come from Mkrew's theme, not from omp's palette.
- The split into P1, P2, P3 after a spike.

Assumed (stated in chat, not contradicted):

- `mk web` shows a notice for a TSP pane in P1 instead of drawing it.
- The pane follows the Chat page's centered 760 px column.

## 3. Spike result (2026-11-23)

A throwaway probe ran `omp.exe` 18.13.2 in a pseudo console with `PI_TUI_NATIVE=1`, answered the hello and acked every frame.

| ConPTY | Flags | APC reaches the terminal | omp opens a surface and gets acks |
|---|---|---|---|
| system (`kernel32`), as Mkrew today | `0x7` | no | no, it draws ANSI rows |
| `conpty.dll` + `OpenConsole.exe` 1.24.2607.10001 (Windows Terminal, copied from Tern) | `0x7` | yes (`q o t f`) | yes (10 frames, 10 acks) |
| the same | `0xF` (passthrough) | yes | yes |
| system | `0xF` | no | no |

So TSP works on Windows when Mkrew ships the newer `conpty.dll` and `OpenConsole.exe` next to the daemon, which is what Tern does.
`portable-pty` 0.9 already loads a `conpty.dll` found next to the executable (`psuedocon.rs:55`), with the flags `0x7`, so the PTY code does not change.
macOS and Linux PTYs pass APC through unchanged.

## 4. ConPTY packaging (Windows)

- Mkrew ships `conpty.dll` and `OpenConsole.exe` from Microsoft's `Microsoft.Windows.Console.ConPTY` NuGet package (MIT), pinned to one version (1.24 or newer) and checked by SHA-256.
- `scripts/fetch-conpty.sh` (and `.ps1`) downloads the pinned package once into a cache and copies the two files next to the binaries in `target/debug` and `target/release`; `scripts/verify.sh` and the packaging scripts call it.
- `packaging/mkrew.iss` installs both files into `{app}\bin`.
- This applies to every PTY the daemon opens (shells, Claude, Codex), not only omp.
  The newer OpenConsole is the one Windows Terminal ships; the plan includes a regression pass over existing panes (typing, resize, Vietnamese IME, paste, exit, the startup resume).
- When the files are missing, the daemon runs as today on the system ConPTY and TSP never starts (the hello never arrives); the daemon logs once at startup which ConPTY it loaded.

## 5. Turning TSP on

- Profile key `tsp` (bool, optional) in SPEC §6.
  Absent: on when the profile's `[usage] format` is `omp_jsonl`, off otherwise, so existing omp profiles (never overwritten by `seed_profiles`) get it without an edit.
  `tsp = false` turns it off; `tsp = true` on a profile whose agent does not speak TSP does nothing visible (no hello ever comes).
- The shipped `agents/omp.toml` gets a commented `# tsp = true` line that explains the default.
- With TSP on, the daemon starts the agent with `PI_TUI_NATIVE=1` (the spike's path) and answers its hello.
  With TSP off (every other profile too, shells included) it sets `PI_TUI_NATIVE=0`, so an omp started there, even by hand inside a shell pane, sends no hello and draws ANSI rows as today.
  (Without that variable omp still probes every direct terminal; the hello would reach the emulator, which ignores APC, and omp would fall back when DA1 is answered; setting it avoids the 300 ms first-paint wait.)
- The value is read when a session starts; changing it affects new sessions only.

## 6. Daemon

### 6.1 Transport (`mkrew_core::tsp::wire`, pure)

- A streaming splitter takes PTY output chunks and returns, in order, plain bytes and complete TSP messages.
  It recognizes `ESC _ tsp ; <verb> {; k=v} ; <body> ESC \` across chunk boundaries and the `ESC ] 877 ; tsp ;` envelope; any other APC passes through as plain bytes.
  An unterminated TSP message longer than 4 MiB is dropped with a warning and the splitter resets.
- Chunked messages (`c=<id>`, `m=1`) are joined before they are returned.
- Messages are encoded the other way (verbs `r` and `e`) with bodies of at most the hello's `apc` bytes; the hello reply is never chunked.
- Message types: `Query{q: hello|blobs}`, `Open`, `Close{keep}`, `Theme`, `Frame{sf, s, ops}`, `Blob{id, mime, data}`; unknown verbs are ignored.

### 6.2 Session output path (`session.rs` `on_output`)

For a session with TSP on, each output chunk goes through the splitter first:

1. Plain bytes go to the emulator, the session log, the state scanner and the clients exactly as today.
   TSP bytes never reach them.
2. A hello query gets the hello reply written to the PTY input before the emulator's replies for the bytes that follow it, so it always precedes the DA1 answer omp waits behind it.
3. `o`, `t`, `f`, `x`, `b` go to the session's `tsp::Surfaces` (6.3); every applied frame is acked with `{"ev":"ack","sf":…,"s":…}`.

Hello reply: `v` 1, `term` "mkrew", `kinds` the list of section 7.1, `features` empty (no `blobs`, `scroll`, `edit`, `undo`, `send` in P1), `apc` 65536, `credits` 2, `cols` the PTY columns, `dark` from `[ui] theme` (System counts as dark until a UI reports its theme, 6.4), `reduceMotion` false, `hour12` false.

When the PTY is resized, the daemon sends `{"ev":"resize","cols":N}` to a live surface.

### 6.3 Document (`mkrew_core::tsp::doc`, pure)

- `Surfaces` holds one `Document` per open surface id (`s:1` inline, more for `screen` surfaces) and the inline blobs (images); omp's `t` palette message is read and dropped (section 7.2).
- `Document` is an arena `id -> Node { kind, props (JSON object), children }` with the roots `main`, `dock`, `layer` added by omp's first frame.
- `apply(frame)` runs every op of section 3.2 of the reference (`add set text splice move del settle focus reveal scroll`, plus `suspend`/`resume` as no-ops).
  A failing op (unknown id, duplicate id, bad index, splice outside the text, unknown kind for `add`) is skipped; the rest of the frame still runs, as omp's own reference document does.
  Each frame with failures sends one `{"ev":"error","msg":…}` and logs a `warn`.
- `x {keep:false}` closes a surface and drops its document; `x {keep:true}` keeps it read-only.
- Bounds: at most 50,000 nodes and 6 MiB of text per document, and 4 MiB of images per session (the oldest images go first; a snapshot carries only images an `image` node still uses), so a `tsp.snapshot` reply stays under the 16 MiB protocol frame limit.
  Past either, the daemon evicts whole top-level children of `main`, oldest first, until the document is under 80% of both, and sends `{"ev":"gone","ids":[…]}` for them; omp then never sends ops for those ids again (reference §3.5).
  `main` gets one synthetic first row "Earlier messages were trimmed" (kind `text`, an id under a reserved `mkrew.` prefix that omp never uses), so the UI shows the cut.
  The dock, the layer and `screen` surfaces are never evicted.

### 6.4 Protocol 59

| Message | Kind | Content |
|---|---|---|
| `tsp.snapshot` | request -> reply | `{session_id}` -> `{state, surfaces: [{id, mode, role, title, read_only, seq, root: Node}], blobs}`; no surfaces when none is open. |
| `tsp.update` | event | `{session_id, surface, change}` where `change` is `frame {seq, ops, blobs}` after the daemon applied a frame (`seq` is omp's frame `s`; `ops` are the frame's ops minus the failed ones, plus the daemon's own eviction ops; `blobs` the images that frame needs), `open {surface}` when omp opens a surface, or `closed {keep}`. |
| `tsp.event` | request | `{session_id, surface, event}`; the daemon checks the `ev` against the P1 list (section 7.4) and writes it to the PTY input. |
| `Session.tsp` | field | `"off" | "waiting" | "live" | "fallback"` in session info and `session.updated`, so the UI and `mk web` know what to draw. |

A client attaching to a session first asks `tsp.snapshot` (which carries each surface's last `seq`), then applies `tsp.update` events in order; a client that sees a `seq` gap asks for a snapshot again.
A UI sends `{"ev":"theme","dark":…}` through `tsp.event` when it attaches and when its theme changes; the daemon keeps the last value for the next hello.

### 6.5 State

The state scanner (`scan.rs`) watches screen text, which stays still while a surface is live.
For a session whose `tsp` is `live`, state comes from omp's hooks only (`mk integration install omp`); without hooks the state stays `unknown` (SPEC §6 already documents the hooks as the setup for omp state; P1 adds no new hint).
When the surface closes or the session falls back, screen detection resumes.

## 7. UI

### 7.1 Kinds in P1

Advertised in the hello:

| Group | Kinds | Drawing |
|---|---|---|
| Layout | `col row card section rule spacer overlay toast` | flex boxes; `card` on `surface_raised` with `radius::MEDIUM`; `overlay` is a modal inside the pane anchored per its `anchor`; `toast` goes to Mkrew's toast layer. |
| Text | `text md code ansi rows shimmer` | `md` and `code` through `preview.rs` with `Flavor::Chat`; `ansi` and `rows` through the terminal cell renderer; `shimmer` as plain text. |
| Data | `kv table list item tabs badge kbd icon status seg` | Mkrew components (`Pressable` for anything with actions). |
| Live | `spinner elapsed progress rate meter effort` | updated on the 500 ms status tick (ADR 0086, no looping animation). |
| Input | `editor input image` | `editor` and `input` show omp's text, placeholder, decorations and caret; keys go to the PTY. `image` shows inline blob bytes. |

Not advertised in P1 (omp then sends its card or list variants): `picker tree tool checklist media prefs`.
Kinds omp sends whatever the hello says (`agent chart diff math prefs`) and any unknown kind are drawn generically: the node's text-like props (`text`, `title`, `head`, `label`, `name`) as a muted line, then its children.

### 7.2 Theme mapping (`mkrew_ui::tsp::style`, pure)

- Span style tokens and node `tone` and `role` map to `theme.rs` tokens through one table in that module, unit tested.
  Examples: `accent` -> `accent`, `muted` and `dim` -> `text_muted`, `error` -> `danger_text`, `warning` and `pending` -> `status_needs_input`, `success` -> `status_done`, `strong` -> `weight::SEMIBOLD`, `em` -> italic, `mono`, `code`, `path` -> the terminal font; an unknown token -> `text`.
- omp's `t` palette is ignored for colors (SPEC §14); its dark or light flag is not used either: the UI tells omp its own theme.
- Private-use glyphs (omp forces Nerd Font symbols while a surface is live) are drawn with the terminal font and `theme::TERMINAL_GLYPH_FALLBACKS`, as the terminal does today.
- Named `icon` nodes map to Mkrew `Icon`s through a table; a name without an entry draws nothing (logged once at `debug`).

### 7.3 Pane (`shell/tsp_view.rs`)

- An omp pane whose session has `tsp` `live` draws the surface instead of the terminal grid; the header has no `Terminal | Chat` switch.
- `main` is a scrolling list in the Chat page's centered column (`CHAT_COLUMN_WIDTH`), following the end like Chat, with the scroll-to-end button; `reveal` and `scroll` ops move it.
- `dock` sits under it in the same column, always visible.
- `layer` children and `overlay` nodes draw as a modal inside the pane; a `screen` surface covers the whole pane until omp closes it.
- Text of `md` and `code` nodes (omp's answers, prose and code) is selectable inside one node as in Chat; Ctrl+C copies a selection and Esc clears it; with no selection both keys go to omp, as in a terminal pane (ADR 0022).
  Labels and status text (`text`, `ansi`, `rows`) are not selectable in P1.
- Tooltips: a node's `title` shows as the Mkrew tooltip; `hovercard` is ignored in P1.
- Keys: while the pane has focus every key goes to the PTY, exactly like a terminal pane (Mkrew's own shortcuts first); paste, image paste and file drops type paths into the PTY as in a terminal pane.

| `Session.tsp` | The pane shows |
|---|---|
| `off` | today's terminal pane with `Terminal | Chat` |
| `waiting` (TSP on, no surface yet) | the starting state of an agent pane |
| `live` | the surface |
| `fallback` (no hello within 2 s of the session's first output, or omp closed the surface with `x {keep:false}`, which it does when it goes back to drawing rows) | today's terminal grid without the Chat switch, and one `toast_info` "omp is drawing as text: <reason>". Once in `fallback` the daemon no longer answers TSP for that session, so a late hello falls back on omp's side too. |
| exited | the last document, read-only, with the existing exited notice |

### 7.4 Events sent in P1

| `ev` | When |
|---|---|
| `action` `{id, act, value?}` | a click on a node with `actions.click` (model chip, Stop, Send, agent pill `agents.open`, jobs pill `jobs.open`, context meter, cost); `act` is the action string of the node |
| `toggle` `{id, collapsed}` | expanding or collapsing a collapsible node |
| `select` / `activate` `{id, item}` | a click / double-click on an `item` of a `list`; `id` is the list, `item` the item's id |
| `focus` `{id}` | a click inside an `editor` or `input` |
| `theme` `{dark}` | on attach and when the Mkrew theme changes |

Every event carries `sf` (the surface id) and uses the field names of section 5 of the reference.
Events on nodes owned directly by a surface root (`main.…`, `dock.…`, `layer.…`) have no component in omp and are not sent.

`ack`, `resize` and `gone` (eviction, 6.3) are sent by the daemon; `edit`, `undo`, `send`, `motion` and `visible` are not sent in P1.

### 7.5 `mk web`

A TSP-live session shows "This omp session uses Mkrew's native view. Open it on the desktop." in place of the terminal; its state, Stop and the reply chips keep working.

## 8. Error handling

| Case | Behavior |
|---|---|
| ConPTY files missing (Windows) | No hello reaches the daemon; 2 s after the first output, `fallback` with the reason "no TSP reply from the terminal layer". Logged once at startup. |
| omp without TSP or too old (hello `v` other than 1) | No surface; `fallback` with the reason. |
| Malformed message or JSON | Dropped, `warn` logged; the stream continues. |
| Failing ops | Skipped, one `ev:error` per frame (6.3). |
| A client misses updates | It sees a sequence gap and asks `tsp.snapshot`. |
| Document bound exceeded | The oldest `main` rows are evicted and reported `gone`; the pane shows "Earlier messages were trimmed" at the top. |
| No UI attached | omp never stalls: the daemon acks each frame after applying it. |
| Daemon restart (startup resume) | The resumed omp starts a new surface; nothing is restored from the old one. |

## 9. Testing

No test runs a real agent or touches the network.

- `mkrew_core::tsp::wire`: every chunk split position of a stream mixing ANSI, other APCs and TSP; chunked bodies; the OSC 877 envelope; the 4 MiB bound; encoding never chunks a hello reply.
- `mkrew_core::tsp::doc`: each op; failure cases skip one op and keep the rest; `move` order; `x` with and without `keep`; eviction keeps the newest rows, the dock and the layer, and lists exactly the evicted ids.
- Fixture: the committed startup record applies without errors and yields `main`, `dock`, `layer`, an `editor` with the placeholder, a `meter` and the bar.
- Daemon unit: the output path writes the hello reply before the emulator's DA1 reply when both arrive in one chunk and when split.
- Daemon e2e with `mkrew-fakeagent` in a new `tsp` mode (prints a hello, then frames from the fixture, then records what it reads): reply and acks arrive; `tsp.snapshot` and `tsp.update` reach a client; `tsp.event` reaches the agent; `Session.tsp` goes `waiting` -> `live`; a fake agent that never sends a hello goes to `fallback`.
  On Windows the e2e needs the fetched ConPTY files next to the test binaries (section 4).
- UI pure: style and icon tables, the generic drawing of an unknown kind, the pane state table.
- Smoke on the real app (screenshots in `docs/screenshots/2026-11-23-omp-tsp-*.png`, dark and light, next to Tern on the same session): startup dock, one prompt and answer, model chip click, Stop while working, the agent pill opening the Agent Hub card variant, a narrow pane, a UI restart (snapshot), TSP off in the profile.
- Regression on the new ConPTY: shell, Claude and Codex panes (typing, resize, IME, paste, exit, startup resume).
- `docs/HUMAN_CHECKS.md`: the same items with a real omp, plus macOS.

## 10. Docs

- ADR 0107 "omp native surface": TSP handled in the daemon, the shipped ConPTY files, the `tsp` profile key and its default, Mkrew colors, protocol 59, the P1 / P2 / P3 split, and why omp RPC mode and file reading were not chosen.
- SPEC: §6 (profile key `tsp`), §7 (protocol 59 messages, `Session.tsp`), §11 (an omp TSP pane), §14 (the TSP style mapping table), packaging section (the two ConPTY files).
- `docs/HUMAN_CHECKS.md` entry "2026-11-23 omp native surface".

## 11. Order of work

Each step is one commit and passes `scripts/verify.sh`.

1. ConPTY files: fetch script, installer, regression pass, ADR 0107 draft.
2. `mkrew_core::tsp` wire and document with the fixture tests.
3. Daemon: output path, hello, acks, resize, the `tsp` profile key, `PI_TUI_NATIVE`, protocol 59, `Session.tsp`, fake agent `tsp` mode and e2e.
4. UI: `mkrew_ui::tsp::style`, `shell/tsp_view.rs`, events, pane states, fallback toast.
5. Hook-only state for live surfaces, `mk web` notice.
6. SPEC, HUMAN_CHECKS, screenshots.

If `feat/tern-layout` merges first (it changes pane headers), this work rebases on it and keeps its protocol 59 and ADR 0107.

## 12. Out of scope

- P2 and P3 content (section 1).
- Claude Code and Codex: they have no TSP; their Chat view stays as ADR 0104 and 0105 made it.
- Switching one running pane between TSP and the terminal grid.
- omp's palette, and omp's RPC mode.
