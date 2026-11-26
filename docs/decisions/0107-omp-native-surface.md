# 0107 - omp native surface: Tern Surface Protocol in the daemon, drawn by Mkrew

- Date: 2026-11-23
- Status: accepted (follows ADR 0108, protocol 60 -> 61; amends SPEC sections 6, 7, 11, 14, 16 and 17; for an omp pane with TSP it replaces the Terminal and Chat views of ADR 0104 and 0105; amends ADR 0105 for a narrow Chat composer, see Consequences; amended by ADR 0118: any node kind enters the document, one log line per surface for failing frames; amended by ADR 0119: the `picker` sheet is drawn)
- Milestone: none (human request 2026-11-23, spec `docs/superpowers/specs/2026-11-23-omp-tsp-surface-design.md`, plan `docs/superpowers/plans/2026-11-23-omp-tsp-surface.md`)

## Context

The user wants an omp pane in Mkrew to look and work like omp in Tern: the composer (model chip, effort, context meter, tok/s, cost, Send and Stop), the subagent and job pills, and later the Agent Hub and the Jobs panel with Kill, Revive and Cancel.
ADR 0105 matched Tern's Chat page from the transcript file, but the composer facts, the pills and the dialogs are not in any file omp writes.
Tern gets them from omp itself: omp describes its whole interface as a tree of nodes and sends it as escape sequences inside the PTY stream (the Tern Surface Protocol, TSP); Tern draws the tree and sends clicks back.
The protocol as omp 18.13.2 implements it is written down in `docs/superpowers/specs/2026-11-23-omp-tsp/tsp-reference-omp-18.13.2.md`, read from the JavaScript inside `omp.exe` with byte offsets, next to omp's own record of a real startup (`omp-18.13.2-startup.tsp.jsonl`).

The work is split in three, each with its own spec and plan: P1 (this ADR: transport, document, the common kinds plus `checklist`, omp's questions and todo, clicks back to omp), P2 (`picker`, `tree`, `tool`, `prefs`, hovercards: the Agent Hub table, the Jobs panel), P3 (blobs cache, `media`, `chart`, `diff`, `math`, local text editing).

A throwaway probe on 2026-11-23 ran `omp.exe` 18.13.2 in a pseudo console with `PI_TUI_NATIVE=1`, answered the hello and acked every frame:

| ConPTY | Flags | APC reaches the terminal | omp opens a surface and gets acks |
|---|---|---|---|
| system (`kernel32`), as Mkrew had it | `0x7` | no | no, it draws ANSI rows |
| `conpty.dll` + `OpenConsole.exe` 1.24.2607.10001 (Windows Terminal, copied from Tern) | `0x7` | yes (`q o t f`) | yes (10 frames, 10 acks) |
| the same | `0xF` (passthrough) | yes | yes |
| system | `0xF` | no | no |

Older system ConPTYs drop APC; the newer one passes it. On Windows 11 24H2 (build 26100) the system ConPTY was observed passing TSP through (2026-11-24), but the sideload stays the supported path. `portable-pty` 0.9 already loads a `conpty.dll` that sits next to the executable, with the flags `0x7`, so the PTY code does not change. macOS and Linux PTYs pass APC through unchanged.

## Decision

- TSP is handled in the daemon.
  `mkrew_core::tsp::wire` splits PTY output into plain bytes and TSP messages (APC `ESC _ tsp ; ... ESC \` and the `ESC ] 877 ; tsp ;` envelope, chunked messages joined, 4 MiB cap per message).
  `mkrew_core::tsp::engine` answers omp's hello (version 1, the kinds Mkrew draws, `credits` 2, `apc` 65536, the PTY columns, `dark` from the UI), applies every frame to a node `Document` (`tsp::doc`), acks each applied frame, sends `resize`, and falls back when the hello does not come.
  TSP bytes never reach the emulator, the session log, the state scanner or the clients as terminal output.
- The daemon owns the document, not the UI.
  There can be several clients (windows, a machine window, `mk web`) or none, and omp must never stall for want of an ack.
- Protocol 61 (ADR 0108 made it 60, this ADR follows it): requests `tsp.snapshot` and `tsp.event`, event `tsp.update`, and `Session.tsp` (`off`, `waiting`, `live`, `fallback`; daemon memory only, like `terminal_title`).
  A client asks `tsp.snapshot` when it attaches (each surface carries its last `seq`), then applies `tsp.update` events in order, and asks again when it sees a gap in `seq`.
  A UI sends the theme with `tsp.event` `{ev:"theme", dark}` when it attaches and when the theme changes.
- Bounds: a document holds at most 20,000 nodes and 1 MiB of text (counted as JSON, escapes included), at most 3 surfaces stay open (a fourth drops the oldest read-only one, else the oldest), and the engine keeps at most 4 MiB of image blobs.
  Together they keep a `tsp.snapshot` reply under the 16 MiB protocol frame limit: 3 x (1 MiB + 20,000 nodes at about 100 bytes) + 5.3 MiB of base64 is about 14 MiB.
  If a reply still passes the frame, the daemon sends it without the blobs, then answers `internal` ("snapshot too large"); it never closes the connection for this.
  A document that stays over a bound after eviction (the excess is in the dock or the layer) makes the session fall back.
  Past a bound the daemon evicts the oldest top-level children of `main` (never the dock, the layer or a `screen` surface), tells omp with `ev:gone`, and adds one synthetic first row "Earlier messages were trimmed" under the reserved id `mkrew.trimmed`.
- Profile key `tsp` (bool, optional).
  Absent: on when the profile's `[usage] format` is `omp_jsonl`, so existing omp profiles (seeding never overwrites them) get it without an edit.
  With TSP on the daemon starts the agent with `PI_TUI_NATIVE=1`; every other PTY (other agents, shells, an omp started by hand in a shell pane) gets `PI_TUI_NATIVE=0`, so omp draws ANSI rows there and does not wait for a hello.
  The value is read when a session starts.
- Mkrew ships the newer ConPTY on Windows: `conpty.dll` and `OpenConsole.exe` from the MIT-licensed `Microsoft.Windows.Console.ConPTY` NuGet package 1.24.260710001 (the files of the spike's passing row), pinned by SHA-256 and fetched by `scripts/fetch-conpty.sh` (and `.ps1`), installed by `packaging/mkrew.iss` into `{app}\bin` next to `mkrew-daemon.exe`.
  This applies to every PTY the daemon opens, not only omp's.
  The daemon logs once at start which ConPTY it loaded (`conpty = "sideloaded"` or `"system"`).
  Without the files the daemon runs as before on the system ConPTY, no hello arrives, and the pane falls back to the terminal grid 10 s after the session's first output (the wait is long because resumed and concurrent omp starts need over 2 s to send the hello; a hello within 60 s of the first output still switches it to the native surface).
- The pane (`shell/tsp_view.rs`): while `Session.tsp` is `live` the pane draws the surface in the Chat page's centered column instead of the terminal grid.
  Since ADR 0106 a pane is only its content, so the surface fills the pane; the title bar shows no `Terminal | Chat` toggle while that pane has the focus, and `Ctrl+Alt+T` and the palette entry do nothing for it.
  `main` scrolls and follows its end, `dock` stays under it, `layer` and `overlay` are a modal inside the pane, a `screen` surface covers the pane.
  Keys, paste and drops go to the PTY as in a terminal pane; clicks on nodes with actions go to omp as `tsp.event`.
  `fallback` shows the terminal grid and one info toast.
  The hello timer's fallback is provisional: after 10 s the pane shows the terminal grid with a notice, and a hello that still arrives within 60 s of the first output is answered and the pane switches to the native surface (omp's startup can exceed any fixed wait under load); after 60 s, and for the explicit fallbacks (unsupported hello version, omp closing the surface with `x {keep:false}`, closing the last surface), the fallback is final.
  A hello without an `open` by 60 s after the first output is a final fallback too ("omp opened no surface").
  An exited session keeps its last document, read-only.
- The dock is drawn as Tern's composer, from the pieces of the Chat composer card (ADR 0105, `shell/composer.rs`), so Chat and an omp pane look the same with omp as the only source of the content (spec 7.8).
  The pieces are the context meter, the card, the model pill, quiet fact chips (with Chat's fact colors for the stats Chat also shows), the round Send and Stop buttons, the working row with its Stop, the hud as one glass pill split by dividers, and the todo HUD.
  When the bar is too narrow it drops, in this order: the facts by `priority`, the effort chip, the cost, and last the model name truncates; the model pill never shrinks and Send or Stop is never clipped.
  A dock without omp 18.13.2's roles (`omp.editor` and its children, `omp.hud*`, `omp.working*`) is drawn by the generic fallback.
  Text glyphs: Nerd Font private-use glyphs are drawn upright in the terminal font with its fallbacks.
- Questions and todo need no protocol change and no new kind beyond `checklist`.
  omp's `ask` replaces the dock's editor with a `col` of role `omp.editor.ask` built from P1 kinds (`tabs`, `md`, `list` and `item`, `row` buttons, `kv`, `meter`); the UI draws it as the composer card, choosing the look from the node `role` (`omp.ask.questions`, `omp.ask.question`, `omp.ask.options`, `omp.ask.option`, `omp.ask.check`, `omp.ask.other`, `omp.ask.preview`, `omp.ask.notice`, `omp.ask.answers`, `omp.btn`, `omp.editor.ask.hidden`).
  Clicks go back as `select`, `activate` and `action`; keys still go to omp through the PTY, which also owns the "Other..." text field.
  `checklist` is advertised in the hello, so omp sends its todo as a node in three modes: `hud` (one line in the status row: the active item and `done/total`, a click opens the phase list in a glass panel, UI-only), `full` (a card in `main`) and `reminder` (a notice in `main`).
  The phase list shows Roman numeral phase headers with a status mark and `done/count`, collapsible by a click (UI state only), and items marked active, pending, done, dropped or blocked.
  Both are drawn only from omp's roles and props; a role the UI does not know falls back to the generic drawing.
- The colors, spacing and fonts are Mkrew's (`theme.rs`, SPEC section 14); omp's `t` palette message and its dark flag are read and ignored.
  One tested table maps span tokens, tones and named icons to theme tokens and Lucide icons.
- State: while a session is `live` its screen text stays still, so the state comes from omp's hooks only (`StateMachine::set_hook_only`); without hooks it is `unknown`.
  When the surface closes or the session falls back, screen detection resumes.
- `mk web` shows "This omp session uses Mkrew's native view. Open it on the desktop." in place of the terminal of a `live` session that has not exited; its state, Stop, the reply chips and the quick keys keep working.
  An exited session shows its replay.
  The relay does not forward `tsp.update` to the browser.
- Claude Code and Codex have no TSP; their Chat view stays as ADR 0104 and 0105 made it.

## Alternatives

- Read omp's files only (extend the transcript reader with the composer, jobs and agents): rejected.
  The model chip, the context meter, the pills, the live job and subagent state and their actions are not in omp's session file, and a file cannot take a click.
  It would copy Tern's screens by hand and drift with every omp release; TSP gives the real tree.
- omp RPC mode: rejected.
  It is a headless stdin and stdout protocol, so the PTY, resume, hooks, screen state and the terminal fallback go away, and it still does not carry omp's interface description (the dock, the hub).
  This is the same reason ADR 0104 gave against ACP.
- TSP: chosen.
  omp already speaks it, Tern is its reference client, it covers the composer now and the Agent Hub and Jobs later, and with the hello unanswered omp draws plain rows, so the failure mode is today's terminal pane.
  Risks: the protocol is Tern's and was read from one omp build (18.13.2); the hello's version check and the fallback contain a mismatch.
- Keep the system ConPTY and turn on passthrough (`0xF`): rejected, the spike's last row shows the system ConPTY still drops APC (on the spike's Windows; Windows 11 24H2 was later observed passing it, see Context).
- Parse TSP in the UI: rejected, see Decision (several clients, remote windows, no UI attached).
- Use omp's palette (`t`): rejected, SPEC section 14 gives Mkrew one look in both themes.
- `TERM_PROGRAM=tern` and OSC 1337: rejected in ADR 0105 and still rejected; `TERM_PROGRAM=tern` would also switch on omp's Tern browser backend.
- A `Terminal | Chat` toggle for a TSP pane (the title bar toggle of ADR 0106): rejected, the composer is omp's dock, as in Tern; the terminal grid returns only through `fallback`.
- Switching one running pane between TSP and the grid: out of scope.

## Consequences

- Protocol 61 follows ADR 0108 (protocol 60 -> 61, no store migration in either): a UI from before it shows a version mismatch with an old daemon; the user restarts the daemon.
  The branch was built as protocol 59 after ADR 0106; `feat/tern-panes` took 60 first and this one merged second as 61.
- Two more binaries in the install folder, and a newer OpenConsole behind every pane on Windows: shells, Claude and Codex change too.
  A regression pass (typing, resize, Vietnamese IME, paste, exit, startup resume, no orphan `OpenConsole.exe`) is in `docs/HUMAN_CHECKS.md`.
  The installer must replace both files while panes run (it already stops its own daemon first).
- `scripts/verify.sh` and the packaging scripts download the NuGet package once into a cache; a machine with no network and an empty cache cannot run the Windows tests that need the files (the daemon e2e of the fake agent in `tsp` mode).
  No test runs a real agent or reaches the network.
- ADR 0105 is amended for the Chat composer: the dock refactor moved its meter, card, model pill, chips and placeholder into shared pieces in `shell/composer.rs`, and in a narrow card Send is never clipped; the Prompts and sparkle tools drop whole when the row has no room (`chat::tools_fit`).
  Nothing else in Chat's look changed, and no snapshot changed.
- The session log and `session.history` hold only plain bytes, so a TSP session's replay and `mk attach` show little of what the surface drew.
- A surface that gets trimmed shows "Earlier messages were trimmed" at the top; the cut text is gone from the pane (the transcript file still has it).
- A daemon restart (startup resume) starts a new omp, which opens a new surface; nothing of the old document is restored.
- The first version draws the kinds listed in the hello; the others (`picker tree tool media prefs`) make omp send its card or list variants, and `agent chart diff math prefs` or any unknown kind draw generically (text-like props, then children) until P2 and P3.
- Until omp exposes more, the UI sends only `action`, `toggle`, `select`, `activate`, `focus` and `theme`; `edit`, `undo`, `send`, `motion` and `visible` are not sent, so omp keeps owning text editing through the PTY.
- The ask card, the todo popover and the dock depend on omp 18.13.2's role names and prop shapes (`overlays/ask-dialog.ts`, `tools/todo.ts`); a renamed role draws as a plain row or list, never a blank pane.
- A Nerd Font glyph missing from every installed Nerd Font (for example U+F14F7 with an older MesloLGS NF) still shows a box; installing a current Nerd Font (Symbols Nerd Font Mono) fixes it.
