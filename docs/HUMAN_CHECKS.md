# Human checks

The build loop never stops to ask you. Everything that needs your eyes, hands or decision
lands here. Skim it at the end of each milestone (or when `scripts/status.sh` says so).

How to answer: write your answer under the item, move it to "Done", and if the loop status is
`WAITING_HUMAN` set `status: RUNNING` in `docs/PROGRESS.md`. Then restart the loop.

## Waiting (blocking)

_Items that stop the loop until you answer._

## To verify (non-blocking)

_Things the agent could not verify automatically (visual checks, real agents, Windows-only behavior).
The loop keeps going; report problems by adding a task line to PROGRESS.md or writing them here._

### To verify — 2026-10-13 Simpler Terminals screen (ADR 0054, docs/terminals-simplify-checklist.md)

- [ ] **Look (dark + light):** title bar, sidebar, tab strip and panes match ADR 0054; no
  status bar; title bar icons are gray; color only on status (run pill, dots, amber counts).
- [ ] **Run pill:** with servers running it reads `● 2 running :port ▾`; the menu's per-command
  ↻ / ■, Restart all, Stop all, Show Run tab and Run setup… work; all stopped → `Run ▾` with
  Start all; a workspace without commands → Run setup… saves and runs.
- [ ] **Sidebar:** `/` focuses the filter (not while typing in a terminal); × clears it; the RUN
  group lists commands with ports and opens a running one's pane; "N empty workspaces" expands and
  its rows open their workspace; footer Inbox jumps to the next entry; compact toggle works.
- [ ] **Panes:** a single pane has no header and its pill / Share / ctx % sit in the tab strip;
  F2 renames it there; split panes show compact headers with an accent line on the focused one.
- [ ] **Reading width:** an agent pane wraps at about 130 columns, centered; 📖 switches it to full
  width and back (the agent redraws); shells stay full width; `[ui] reading_width = false` makes
  agents start full width.
- [ ] **Voice:** with the sidebar hidden, recording still shows its meter in the title bar.

### To verify — 2026-10-13 Run setup: Add command scrolls, card titles, scroll bar

- [ ] **Run setup (dark + light):** with 2+ commands, "Add command" scrolls the list to the new
  empty card ("Command N" title, name focused); Duplicate and Tab into a hidden card scroll too.
  Each card's top shows its name (or "Command N"). A thumb on the list's right edge scrolls when
  the cards overflow and is hidden when they fit. At 8 commands, Add shows the limit.

### To verify — 2026-10-13 Mkrew web extras (plan docs/superpowers/plans/2026-10-13-mobile-web-extras.md)

- [ ] **Icon:** remove the old Home Screen bookmark, open the link in Safari, Share → Add to Home
  Screen: the preview and the Home Screen show the Mkrew lanes icon (not a page screenshot).
- [ ] **Stop:** open a running Claude session, tap the square button → "Stop session …?" → OK:
  within a few seconds the pill shows "Exited", and the desktop pane shows the session ended.
  Cancel leaves it running.
- [ ] **Quick replies:** send "continue" from the Reply box, open another session: the chip
  "continue" leads the row; tapping it sends at once and the keyboard does not jump.
- [ ] **Ended sessions:** a session that ended in the last hours is listed (✕, muted, "2h") below
  the live ones; opening it shows the end of its output at its own width and "Session ended".
  A session that ended yesterday morning is gone from the list.

### To verify — 2026-10-12 Phone access from the UI (plan docs/superpowers/plans/2026-10-12-phone-access-ui.md, ADR 0053)

- [ ] **Dialog, dark and light:** palette → "Global: Phone access…" and the title bar phone icon both
  open "Phone access". Check Off / On / `✕ Not running` rows, the Tailscale row, the QR panel (dark
  modules on white in both themes), the link line and Copy link in dark and light; no clipped text at
  the 520 px width; Esc closes.
- [ ] **Start → icon accent:** Start → the row shows "Starting…" then `● On`, the title bar icon turns
  accent and its tooltip reads "Phone access: on"; Stop → `○ Off`, muted icon, "Phone access: off".
  Close Mkrew and sign out and in again, then open Mkrew: the icon is accent (the daemon started
  `mk web` again; phone access comes back whenever the daemon starts).
- [ ] **Scan:** with Tailscale set up, scan the QR with the iPhone camera → Safari opens the Mkrew
  page and shows Sessions (no token screen). Copy link pastes the same link.
- [ ] **Stop while the phone page is open:** Stop → within a few seconds the phone page shows "Can't
  reach Mkrew on <pc>. Retrying…" and does not come back until Start.
- [ ] **Set up on a PC without serve config:** `tailscale serve reset`, open the dialog → "Tailscale
  doesn't publish Mkrew yet" + Set up; click it → the button reads "Setting up…", then "✓ Tailscale
  serve is set up". Sign out of Tailscale → "Install and sign in to Tailscale first", no Set up.
- [ ] **Port in use:** run `mk web` by hand, then Start → `✕ Not running: <bind error>. Retrying…`;
  stop the hand-started one → the row becomes `● On` within about a minute.
- [ ] **Daemon killed:** with phone access on, End task on `mkrew-daemon` → `mk.exe` is gone too; the
  phone shows "Can't reach Mkrew…".

### To verify — 2026-10-12 Mkrew web on a phone (plan docs/superpowers/plans/2026-10-10-mobile-web.md)

- [ ] **Link:** with Phone access off, `tailscale serve --bg 8787`, then `mk web` prints `https://<pc>.<tailnet>.ts.net/#t=…`.
  Opening it on the iPhone (Safari) shows Sessions with "Needs you" on top, the PC name next to
  a green dot. Add to Home Screen, then open the icon: it connects without asking for the token.
- [ ] **Answer:** a Claude/Codex session asks a question on the PC → it shows under "Needs you"
  within a few seconds. Tap it: the terminal shows the question at the session's own width
  (small font, pinch-zoom works); the desktop pane keeps its size. Tap `y` / Enter or type a
  reply and Send: the agent continues; the keyboard stays open between quick keys. Swipe up on
  the terminal: it scrolls back through earlier output.
- [ ] **New agent:** + New agent → pick a workspace and Claude, type a prompt, Start → the
  session opens on the phone and a task card shows on the desktop board; an uninstalled agent
  is greyed "not installed".
- [ ] **Disconnect:** with Phone access off (a hand-started `mk web`), stop `mk web` → within seconds the banner "Can't reach Mkrew on <pc>.
  Retrying…" shows; start it again → the banner goes and an open session repaints.
- [ ] **Token:** with Phone access off, stop `mk web`, run `mk web --new-token` → the old Home Screen icon shows the token screen with
  "That token didn't work. Check it and try again."; pasting the new link (or token) connects.
- [ ] **Light and dark:** switch iOS appearance; colors follow it, the terminal stays dark.
- [ ] **Chrome on another computer in the tailnet:** the same link works; Ctrl+Enter sends.

### To verify — 2026-10-12 Lucide icons (ADR 0051, plan docs/superpowers/plans/2026-10-12-lucide-icons.md)

- [ ] **Icons (dark + light):** title bar (theme toggle cycles sun-moon → sun → moon, History
  clock), sidebar header (+, collapse chevron, remove-idle), tab strip (+, split, ⋯, tab ✕),
  status bar (sidebar, compact, session panel toggles), Git (repos ▾, fetch, ‹ ›, ⋯), Files
  (search row, find bar ▸ ↑ ↓ ✕), terminal find ✕, composer ✦ and queue ✕, Run (live ↻ ■, menu
  rows ↻ ■ ▶): every icon is drawn (no blank or box), centered, muted, same stroke weight.
- [ ] **Window controls (Windows):** hover close → red background and a white X; minimize and
  maximize hover show the soft background. Maximize (button, title bar double-click, Snap) →
  the middle icon becomes the restore icon; restore → a square again.
- [ ] **Hidden / disabled:** session ✕, workspace ⋯ and inactive-tab ✕ appear only on hover; the
  composer ✦ is faded while the prompt is being improved.
- [ ] **Chevrons:** workspace rows, Git group headers and History commits, Files tree folders, Run
  setup folders: chevrons flip on expand/collapse; names stay aligned between files and folders.
  Git Working: the change letters sit under the group header's text, the Staged hint too; an
  expanded History commit's files and notes start under its summary (with and without the lane
  graph), also at `[ui] font_size = 18`.
- [ ] **Font size:** set `[ui] font_size = 18`, restart: icons grow with the text and stay
  centered; nothing clips; the Run button's two halves keep one height.
- [ ] **Icons beside text (dark + light):** tab strip ends with Close pane (square ✕), +, split,
  ⋯; pane header Share and Turns…; title bar `+ New ▾`, idle Run (▶ icon) and its ▾, switcher
  ▾; empty states (New agent, New shell, Add workspace, New task, Open prompts folder); card
  Undo / Changes / Compare / Run with… / Several agents…; Docs and Git Comment, Send to agent,
  Open in Zed; Files Review in Docs, Save, Find; Git Discard… (red icon), Push, Generate,
  `By: … ▾`; session panel Open transcript, Link plan…, Unlink; composer Prompts…; Run setup
  Duplicate, Remove, Add command, Up. Icon and text share one color and sit on one line.
- [ ] **Primary ↔ ghost:** edit a file → Save turns blue with a white icon, save → muted again.
  Select a card for Compare → white icon on blue. Run idle (blue, white ▶ and ▾) → start → quiet
  (muted ▾). Share while following → icon as bright as the text.

### To verify — 2026-10-12 sidebar marks, remove idle agents

- [ ] **Sidebar marks (dark + light):** idle agent rows show no `○`; a working agent shows one
  blinking dot in a narrow slot; chips of idle / working / `?` rows stay in one column. A shell
  you type in shows no mark, no time, and is not counted as working (summary, Working chip).
- [ ] **Remove idle agents:** with an idle agent, `⌫` appears left of `▾` in the sidebar header;
  click → "Remove N idle agents?" → Remove: they vanish from the sidebar and their panes close.

### To verify — 2026-10-12 Files tree on the right (ADR 0050)

- [ ] **Files layout (dark + light):** Restart the daemon first (protocol v39). Tree on the right of the file; the search row shows `⌕ Search
  files…` with the Quick open shortcut and opens Quick open on click; `Ignored` toggles ignored
  files, looks pressed while on, its width never changes; ←/→/↑/↓, right-click, New file… still work.

### To verify — 2026-10-12 equalize panes, drag pane dividers, sidebar width

- [ ] **Equalize (dark + light):** Split right twice → 50/25/25. The split button `◫` menu shows
  "Equalize panes  Alt+Shift+0" under Split down; click it (or press Alt+Shift+0, or the palette's
  "Panes: Equalize pane sizes") → three equal columns. With a column split down inside, that column
  counts as one. Restart the app: the sizes stay.
- [ ] **Drag pane dividers:** hover the gap between two panes → resize cursor (↔ side by side,
  ↕ stacked) and a faint strip; drag it → the panes follow the pointer, never below 10 %; release
  → restart the app, the sizes stay. Zoomed pane (Ctrl+Shift+M): no dividers. Terminals redraw to
  the new size; check an agent TUI does not flicker badly while dragging.
- [ ] **Sidebar width:** drag the full sidebar's right edge → it follows the pointer between 200 and
  480 px (at most half the window); release → `settings.toml` gets `[ui] sidebar_width = …`;
  restart → same width. The compact rail and the session panel's edge (`inspector_width`) still
  work as before.

### To verify — 2026-10-12 task dialog caret, sidebar not redrawn by pane output

- [ ] **Caret (dark + light):** in the task dialog (Title and Prompt), ← / → / Ctrl+← / Home /
  End / ↑ / ↓ (Prompt) move the caret; typing, Backspace and Delete act there; Shift+arrows
  highlight, typing replaces it; Ctrl+C / Ctrl+X / Ctrl+V work. "From prompt…" types at the caret.
  Typing at the end of a long prompt still scrolls to the bottom.
- [ ] **Sidebar stays live (cached view):** with agents working, the sidebar's dots, `5s`/`17m`
  times, filter typing, chips, hover, right-click menus, pin, rename, color, collapse, the grip
  drag and the compact rail (Ctrl+Alt+B) all update at once, in dark and light, after a theme or
  font change too. While 2–3 agents print a lot, typing in the task dialog and the composer
  feels immediate.

### To verify — 2026-10-12 Run setup caret and Browse… (ADR 0048)

- [ ] **Caret (dark + light):** in Run setup, type a command, press ← / Ctrl+← / Home: the caret
  moves and typing, Backspace and Delete act there; Shift+arrows highlight part of the text,
  typing replaces it; Ctrl+C / Ctrl+X / Ctrl+V work. Spaces before the caret keep their width.
  Env: ↑ / ↓ move between lines. Other dialogs (task, composer, commit box) behave as before.
- [ ] **Browse… (dark + light):** Folder's `Browse…` lists the workspace's folders; clicking one
  opens it, `↑ Up` goes back, `Use this folder` (or Enter) fills Folder (`.` at the root). Env
  file's `Browse…` starts in the card's folder and shows `.env` (git-ignored, muted); clicking it
  fills the workspace-relative path, e.g. `EA-BE/.env` (ADR 0049). Esc closes only the picker.
- [ ] **Env file picker (EA, dark + light):** Env file `Browse…` starts in the card's folder and
  `↑ Up` reaches the workspace root; a card with folder `.worktrees/fix-alembic-heads/EA-BE` and
  env file `EA-BE/.env` starts there with the main `.env`.
- [ ] **Duplicate (dark + light):** in Run setup, Duplicate on BE adds `BE copy` below with the
  same fields and focuses its name; at 8 commands it shows the limit.

### To verify — 2026-10-11 title bar / sidebar / status bar polish

- [ ] **Title bar (dark + light):** History is a `◷` icon button (tooltip "History: …", opens the
  dialog; glyph renders, not a box). "+ Agent" / "+ Shell" are one `+ New ▾` button: its dropdown
  lists Agent and Shell with their shortcuts; each starts its pane (switching to Terminals); a click
  outside, Esc-less shortcuts (Ctrl+Shift+P) and switching workspace close it. While a pane starts
  the button reads `Starting agent` / `Starting shell` with the dots.
- [ ] **Run color:** idle `▶ Run` and `▾` use the accent blue (the logo's), text readable in both
  themes; the warning text color still shows on an exited command.
- [ ] **Sidebar filter:** a muted `⌕` sits before `Filter sessions…` and before typed text; the
  caret and Ctrl+A highlight are not under it.
- [ ] **Status bar:** no elapsed time on the right for a focused agent; usage fields remain.

### To verify — 2026-10-11 gutter, graph, layouts (plan docs/superpowers/plans/2026-10-11-gutter-graph-layouts.md)

- [ ] **Files change gutter (dark + light):** open a tracked file; add, change and delete lines →
  green / amber bars and a short red mark appear ~150 ms after typing stops; the text never
  shifts; Save + stage the file in Git → the bars go; an untracked file has no bars; a file in a
  child repository (ADR 0029) uses that repository. On Windows with `core.autocrlf=true` an
  unchanged file shows no bars. Not seen on screen by the agent (headless).
- [ ] **History lane graph (dark + light):** Git → History (`h`) on a repo with merges → dots and
  lanes join across rows with no gaps; a merge opens a second lane that ends at its fork point;
  the expanded commit's files show the lanes running through; a linear repo shows one line; the
  selected row's fill does not hide the lines.
- [ ] **Layout menu (dark + light):** the split button opens Split right / Split down / Layouts
  with five drawings (2 columns, 2 rows, 3 columns, 1 + 2, 2 × 2) and Alt+Shift+1…5; picking
  2 × 2 on a one-pane tab starts three panes of the focused kind, the old pane stays top-left and
  keeps the focus; on a 3-pane tab the 2-pane layouts are greyed with a tooltip; the layout
  survives a UI restart. Note: the split button no longer splits on one click (Alt+Shift+= still
  does). Say so if you want the one-click split back.

### To verify — 2026-10-11 feature switches (ADR 0046)

- [ ] **Defaults off:** without `[features]` in `settings.toml`, the view rail has no Docs tab,
  Ctrl+Shift+3 does nothing, the palette has no `View: Docs` / `Agent: Turns…`, a `.md` file in
  Files has no "Review in Docs", agent pane headers show `Share` but no "Turns…".
- [ ] **Live switch:** add `[features]` `docs = true` and save → the Docs tab appears within ~1 s;
  set it back to `false` while on Docs → the workspace shows Terminals.
- [ ] **Turns:** `turns = true`, start a new agent session, let it edit a file → "Turns…" lists the
  turn with +/− counts and Restore works. With `turns = false`, no `refs/mkrew/turns/*` appear
  (`git for-each-ref refs/mkrew`) for new sessions.

### To verify — 2026-10-11 in-app confirmation dialogs (plan docs/superpowers/plans/2026-10-11-in-app-confirm-dialogs.md)

- [ ] **Look (dark and light):** close the pane of a running agent, close a tab with a running
  agent, ✕ a running session in the sidebar, Stop in the session panel, Delete in Edit task,
  Remove workspace, and leave an edited file in Files. Each shows a Mkrew card over a dimmed
  window (no Windows message box). Destructive answers (Kill session, Remove, Delete, Stop
  session) are red; Save is the accent button; Cancel is on the left.
- [ ] **Keys:** Esc = Cancel; Tab / → / ← / Shift+Tab move the ring and wrap; Enter and Space
  pick the ringed button. A click on the dim area does nothing. Holding Enter (e.g. from the
  palette's Remove workspace) does not answer the prompt. Alt+F4 still closes the window and
  Alt+Space still opens the system menu; with unsaved Files changes, Alt+F4 during a prompt
  does nothing until the prompt is answered.
- [ ] **Keyboard stays in the prompt:** open the close-pane prompt while another agent is
  starting; Esc still closes the prompt (it never reaches a terminal). After Cancel, typing
  goes to the pane that had focus before.
- [ ] **Over the task dialog:** Edit task → Delete → Cancel: the task dialog is still open and
  its fields take typing; Esc then closes the task dialog.
- [ ] **Unsaved on close:** edit a file in Files, close the window: the in-app "Save changes to
  …?" shows; Cancel keeps the window; Don't save closes it.
- [ ] **Long text:** a deep file path in the save prompt and a long workspace name in Remove
  wrap inside the card at `[ui] font_size = 18`; the buttons stay on one row and visible.
- [ ] **Title bar:** while a prompt is open, the ─ □ ✕ window buttons and dragging the title bar
  behave acceptably (note what happens).

### To verify — 2026-10-11 Git tab redesign (checklist docs/git-tab-redesign-checklist.md)

Build, clippy and tests pass; the app starts and connects. The Git tab itself was not clicked
through by the agent. Check in dark and light:

- [ ] **Side panel:** repo strip (EA-BE / EA-FE tabs, amber dot, ▾ menu with search), branch
  header (Fetch icon, Push N, tracking line), Working N / Vs. base / History, Filter files, By: ▾
  menu, Staged / Changes / Untracked headers (fold, Stage all on hover), 30 px rows, commit box
  (0 / 72 counter, Generate, readable disabled Commit), clean state card.
- [ ] **Diff toolbar:** ‹ › with "1 / 6", status badge, `folder / name`, Unified | Split, Wrap
  (survives a restart), Open in Zed, red Discard…, primary Stage file, ⋯ (Comment, Send to agent).
- [ ] **Diff body:** long lines wrap (Wrap on) or scroll sideways (Wrap off); hunk headers with
  Stage hunk; "no newline at end of file" pill; Split hatch on one-sided rows, halves stay aligned
  when wrapped.
- [ ] **Selection bar:** click a line (Shift+click extends) → bar under it with Comment
  (Ctrl+Alt+M), Send to agent, Clear; Esc clears; the Discard dialog preview still renders.

### To verify — 2026-10-10 large-file split (plan docs/superpowers/plans/2026-10-10-split-large-files.md)

Structure-only refactor (main.rs → `crates/mkrew-ui/src/shell/`, daemon tests and nine inline test modules split). Tests, clippy and a verbatim-move check passed. Please click through once: Board (drag a card), Docs (open, comment), Files (open, find, edit, save), Git (stage, diff split/unified, commit box), palette, quick open, peek, a run, the worktree/turns/history dialogs, theme toggle, compact sidebar. Expected: everything behaves exactly as before.

### To verify — 2026-10-10 voice input (plan docs/superpowers/plans/2026-10-10-voice-input.md)

- [ ] **Setup:** run `setx GROQ_API_KEY gsk_…` in a terminal, then quit and restart Mkrew. In
  Windows Settings → Privacy → Microphone, "Let desktop apps access your microphone" is on.
- [ ] **Composer:** open the composer (Ctrl+Shift+Enter) and press Ctrl+Shift+D. The status bar
  shows five bars and `0:01 · Ctrl+Shift+D stops`, counting up. Say "Sửa lỗi đăng nhập rồi chạy
  test lại": the bars rise while you talk and sink to a flat line when you pause.
  Press Ctrl+Shift+D: the status bar shows the three moving dots and `Transcribing`. Then the
  Vietnamese text, with diacritics, appears at the end of the draft and the status item
  disappears.
- [ ] **Meter look:** the bars and dots read clearly in both dark and light themes, line up
  with the status bar text, and the bar does not jump in height while recording.
- [ ] **Accented English and mixed:** repeat with "Please refactor the login handler and add a
  test", then "Commit cái này rồi push lên branch main". English words stay English.
- [ ] **Terminal:** focus an agent pane and dictate a sentence. It appears in the agent's input
  as one paste and is **not** sent (no Enter).
- [ ] **Other fields:** dictate into the task dialog prompt, the Git commit box, the sidebar
  filter and the Files editor. The text lands in the focused one.
- [ ] **No target:** focus the Git file list and dictate. Nothing is staged or toggled, and the
  status bar says the text is on the clipboard. Ctrl+V pastes it into any field.
- [ ] **Slips:** press Ctrl+Shift+D twice quickly → `✕ Voice: too short…`. Record 3 s of silence
  → `✕ Voice: heard nothing…`. Nothing is typed in either case.
- [ ] **Errors:** run `setx GROQ_API_KEY ""` and restart → `✕ Voice: set the GROQ_API_KEY
  environment variable…`, and no recording starts. A wrong key gives `✕ Voice: HTTP 401: Invalid
  API Key`. With Wi-Fi off: `✕ Voice: cannot reach …`.
- [ ] **Auto stop:** set `[voice] max_seconds = 5` in settings.toml, start recording and keep
  talking. After 5 s it stops and transcribes by itself.
- [ ] **Other provider:** set `[voice] base_url = "https://api.openai.com/v1"`,
  `model = "gpt-4o-mini-transcribe"` and `api_key_env = "OPENAI_API_KEY"` (with that variable
  set). Voice input works the same.
- [ ] **Focus moved:** start dictating in the composer, stop, then click the Git file list while
  `Transcribing` shows. Nothing is staged or toggled; the text is on the clipboard and the
  status bar says so.
- [ ] **Broken settings:** put an invalid line in settings.toml, then press Ctrl+Shift+D. The
  status bar shows `✕ Voice: …` naming the file and line, and no recording starts.
- [ ] **Exited pane:** focus a pane whose session has exited and dictate. The text is on the
  clipboard.
- [ ] **Terminal find bar:** press Ctrl+F in a pane and dictate. The text lands in the find
  query, not in the agent.

### To verify — 2026-10-10 "Starting agent" loading mark

- [ ] **Loading mark (dark and light):** + Agent (title bar), New agent (empty tab) and
  Ctrl+Shift+A / Alt+Shift+A: until the pane shows, the clicked button reads `Starting agent`
  with the three working dots taking turns, the other (+ Shell) is faded; clicking again or the
  shortcut does not start a second agent. Same with + Shell (`Starting shell`). A spawn error
  (e.g. rename the agent's exe) brings the buttons back with the error in the tab bar.
- [ ] **Faster cold start (optional, admin):** most of the ~2 s is Windows Defender scanning the
  225 MB `omp.exe` / 260 MB `claude.exe` on their first start (measured: 1.6 s first spawn, 11 ms
  after). `Add-MpPreference -ExclusionProcess omp.exe,claude.exe` cuts it, at some security cost.

### To verify — 2026-10-10 file viewer scroll bar and large files (plan docs/superpowers/plans/2026-10-10-file-viewer-scroll.md)

- [ ] **Editor bar (dark and light):** open a long source file in Files. A thin rounded thumb
  sits on the right edge; its size is the visible share; it moves as you scroll with the wheel,
  PgDn, Ctrl+End and Find (Enter).
- [ ] **Drag:** click in the text (the editor has focus), then drag the thumb up and down, also
  with the pointer over the text and outside the window, and release there. Only scrolling
  happens: the caret does not move, nothing gets selected, and the thumb goes back to its idle
  color on release. Pressing the track above or below the thumb jumps there.
- [ ] **Short file:** a file that fits on screen shows no bar; a click at the far right end of a
  long line still puts the caret there.
- [ ] **Large Markdown (the reported lag):** in Files, open
  `docs/superpowers/plans/2026-10-10-inbox-perf.md` (about 6,400 lines). The preview shows within
  about a second. Wheel, PgDn and thumb drag scroll smoothly, side by side with Zed. The thumb
  reaches the very end and the last block is fully visible. Same file in Source: smooth too.
  Same in the Docs view.
- [ ] **Keeps its place:** scroll to the middle of a `.md` preview and let an agent (or Zed) save
  the file: the page updates and stays where it was. Open another `.md`: it starts at the top.
- [ ] **Resize:** with that large preview open, resize the window and drag the session panel's
  edge: it stays usable, and afterwards the column is centered and the thumb matches the
  position.
- [ ] **Diagrams:** a `.md` with a mermaid block lower down. Scroll to it: it appears without the
  page jumping; the thumb keeps its size.
- [ ] **Docs picking:** in the Docs preview, click a block: it highlights and the comment margin
  offers to comment on its lines; Shift+click extends; hover shows the hover band. Dark and
  light.
- [ ] **Large source file:** open a 1 MB or larger source file in Files (not Markdown). Type, hold
  an arrow key, scroll and drag the thumb: smooth, no per-frame stall. The horizontal scroll
  width still fits the widest line (a very long line scrolls fully; after deleting it the width
  shrinks; a line with tabs scrolls as wide as it looks).

### To verify — 2026-10-10 sidebar session rows

- [ ] **Rows (dark and light):** an omp session's row shows its title without `omp · ` and
  without the leading `π`; the selected row looks the same (one line, title only, no second `omp` line). A terminal
  row (PowerShell…) still shows `powershell · title`. Times (`5s`, `43m`, `1h 5m`) end in one
  right-aligned column; rows without a time keep the same title width. Existing titles update
  on the next title change of the agent.

### To verify — 2026-10-10 compact sidebar (plan docs/superpowers/plans/2026-10-10-compact-sidebar.md)

- [ ] **Toggle:** Ctrl+Alt+B (also with a terminal focused), the status bar's `⇤` and the palette's
  "Global: Toggle compact sidebar" switch full width ↔ the rail; the icon flips `⇤` / `⇥` and its
  tooltip "Compact sidebar" / "Expand sidebar" shows Ctrl+Alt+B; panes re-wrap. `◧` /
  Ctrl+Shift+B still hide and show; hidden → Ctrl+Alt+B shows the rail; rail → hide → show
  returns the rail. After a restart the sidebar is full width.
- [ ] **Rail content (dark and light):** each workspace shows its avatar in its color, `?` in the
  needs-input color while a session asks, else its working count; the active workspace and the
  focused session are highlighted; session rows show the working dots / status glyph and the agent
  chip; the `⇤` / `⇥` glyphs render (not boxes). Nothing is clipped at `[ui] font_size = 18`.
- [ ] **Tooltips:** hover a session with a long title → `claude · <the full title>` and
  `working · 2m` on a second line; a session without a title → agent and status only. Hover a
  workspace → name, summary (if any) and folder. No peek opens on hover in the rail.
- [ ] **Clicks and menus from the rail:** click a workspace → shown; click the shown one →
  collapses/expands its sessions; right-click → the workspace menu opens fully visible; click a
  session → its pane; right-click a session → the pane menu; `+` adds a workspace.
- [ ] **Filter and keyboard:** type in the filter and pick `Working`, keep the filter focused, press
  Ctrl+Alt+B → the filter text and chip are gone, every workspace shows and the open ones list
  all their live sessions; typing goes to the focused terminal (not to
  an invisible filter); same when clicking `⇤` instead. Ctrl+Shift+Space in the rail → the peek
  opens beside the row and reply / Esc work.
- [ ] **◧ with the filter focused:** type in the filter, click `◧` with the mouse → the sidebar
  hides and typing goes to the focused terminal, and shortcuts (Ctrl+Shift+1, Ctrl+Alt+B) work
  (before: the keyboard stayed in the hidden filter until a pane was clicked).

### To verify — 2026-10-10 inbox: redesign (plan docs/superpowers/plans/2026-10-10-inbox-redesign.md)

- [ ] **Run colors:** in light and dark, with a workspace that has two run commands:
  - Idle: `▶ Run` and `▾` are accent blue (text readable). Hover dims them slightly.
  - Running: `↻` is amber and `■` red; hovering each shows a faint amber / red fill and keeps the glyph color.
  - Open `▾`: the row `↻` is amber, `■` red, `▶` green; hovering shows the same tints (visible on the panel in both themes).

- [ ] **Title bar, window (Windows):** `cargo run -p mkrew-ui` (or the installed app). One 44 px bar on top, no OS title bar:
  - Drag the empty part of the bar → the window moves. Double-click it → maximizes; again → restores.
  - Hover □ (also when maximized, then ❐) → the Windows 11 Snap Layouts flyout shows; pick a layout → the window snaps. Click □ → maximize / restore; ─ minimizes; ✕ closes (terminals keep running in the daemon); ✕ is red on hover.
  - Resize from all four edges and all four corners, including the top edge (a thin strip at the very top) and the top-right corner next to ✕. Try at 100 % and 150 % display scaling.
  - Drag it down from maximized → restores under the cursor. Win+Arrow keys work. Minimum size stops at 1100 × 560 with nothing overlapping; a long workspace name / branch truncate with `…`.
  - F11: full screen hides ─ □ ✕; F11 again brings them back.
  - Open a dialog (Ctrl+Shift+P palette, a task dialog with some typed text): the shade covers the sidebar and main area but not the title bar. Drag, double-click and ─ □ ✕ still work; the logo, workspace name, branch chip, views, theme toggle, History, + Agent / + Shell and Run do nothing (no hover) and the dialog keeps its text and focus; Esc closes the dialog, then they work again.
  - With the daemon stopped (no workspace shown): the logo, drag area and ─ □ ✕ still show and work.
- [ ] **Title bar, macOS / Linux (if available):** macOS: traffic lights sit at the left (not over the logo), dragging the bar moves the window, double-click zooms or minimizes as System Settings says. Linux: the OS title bar is there and the Mkrew bar below it has no ─ □ ✕.
- [ ] **Workspace switcher:** click the workspace name ▾ → every workspace with its avatar, ✓ on the shown one; click another → switches (sidebar highlight, tabs, branch chip follow) and the dropdown closes; click outside / Esc-less outside click closes it; "Add workspace…" opens the folder picker. Alt+1/Alt+2 and sidebar clicks still switch and close an open dropdown.
- [ ] **Branch chip and `Git N` badge:**
  - A workspace that is one repo (branch `master`) → `⎇ master`; edit a file → `Git 1` appears within about a second; stage + edit the same file again → still `Git 1`; add an untracked file → `Git 2`; commit everything → the badge disappears.
  - A folder holding `fe/` and `be/` repos, no `.git` at the root → `⎇ 2 repos`; hovering lists `fe — main · 1a2b3c4` and `be — …`; change one file in each → `Git 2`; a click on the chip opens the Git view.
  - A folder with a single child repo → `⎇ <its branch>`. A folder with no repository → no chip and no badge (the bar is not blank otherwise).
  - Detached HEAD (`git checkout <hash>`) → `⎇ detached`.
  - Switch workspaces back and forth: each shows its own branch; with the Terminals view shown (Git view never opened), a commit made in a terminal updates the chip.
- [ ] **Layout after the title bar:** sidebar header is "Workspaces +" (no logo), the sidebar and the main area reach the window bottom, no clipped tab strip; the Peek popover, quick open and the palette still open centered and the Run ▾ menu and the workspace menu (⋯) still drop below their buttons.
- [ ] **Usage cache split (Claude, Codex):** after one turn in each, restart the daemon, run another
  turn, hover the pane header `ctx N%`: the text reads `… tokens used (X in, Y cached, Z out)`, `X`
  is small (fresh input only) and `Y` is most of the input. `ctx N%` and the total on the task
  card equal what they were before this change for the same session (compare with
  Claude `/context` and Codex `/status`). A session that had no turn end since the upgrade still
  shows its old `ctx` and total, with no `cached` in the hover. The model name is not shown yet
  (the status bar of a later task shows it).
- [ ] **omp usage:** run `mk integration install omp` if not done, start an omp agent pane, finish one
  turn: the pane header shows `ctx N%` and a token total on the task card. If it only
  appears after the NEXT turn, omp's turn-end hook fires before the last line is flushed: note it
  here (the reader keeps its offset, so nothing is lost). Interrupt a turn with Esc: ctx % and total
  do not drop to 0. Compare `ctx N%` with omp's own context figure. `context_window` in
  `agents/omp.toml` is 200000 for every model: set your model's real window in your profile copy
  and check the percentage follows.
- [ ] **Status bar:** a thin bar sits at the bottom in every view (Board, Terminals, Docs, Files,
  Git), in dark and light; nothing overlaps the dialogs; the `◧` glyph renders (not a box); with a
  dialog open the bar's `◧` and Inbox text do nothing.
- [ ] **Toggle sidebar:** the `◧` button, Ctrl+Shift+B (also with a terminal focused) and the
  palette's "Global: Toggle sidebar" hide and show the sidebar; panes fill the width and the shell
  inside re-wraps; the tooltip flips between "Hide sidebar" and "Show sidebar"; the sidebar is back
  after a restart (not remembered).
- [ ] **Status bar Inbox:** with nothing waiting it reads `Inbox clear` (no pointer, no click);
  when an agent asks a question it reads `1 needs you` in amber; clicking it jumps to that pane
  like Ctrl+Shift+I; focusing a done pane makes it count down.
- [ ] **Status bar git:** one repo → the 7-char HEAD hash (hover: `branch · hash`); a folder with
  no `.git` holding two repos → `2 repos`, hover lists both; with one child repo it shows that
  hash; a new repo with no commit shows nothing; after `git commit` the hash changes without
  reopening the Git view.
- [ ] **Status bar agent fields:** focus a claude / codex / omp pane: after a turn the bar shows
  the context meter and `ctx N%`, `in … · out …`, cache and the model (no elapsed time; the pane
  header's `working · 12s` still counts up). A fresh
  agent with no turn yet shows no usage fields at all (no `0`). A shell pane, and the Board / Docs
  / Files / Git views, show none. An agent whose profile has no `[usage]` shows no usage fields.
- [ ] **Status bar redraw:** with Task Manager open and every pane idle, the app's CPU stays at
  zero (the bar adds no timer).
- [ ] **Hidden sidebar and its popovers:** hide the sidebar, then (a) Ctrl+Shift+Space (or the
  palette's Peek) → the sidebar comes back with the peek open and typing/Esc work; (b) with a peek
  focused press Ctrl+Shift+B → the sidebar hides, the peek closes and the terminal gets the
  keyboard back; (c) with no default agent set, + Agent while hidden → the sidebar returns with the
  workspace menu open (not an invisible stuck focus).
- [ ] **Change color stays put:** with workspace A shown, right-click workspace B (or its `⋯`) →
  pick a color. The view stays on A (no switch to B, no session opened), the menu closes, B's
  avatar takes the color. Repeat with the menu sitting over session rows of B and over A's rows;
  also click the menu's padding and a separator: nothing behind reacts. The Run menu (▾), the
  workspace switcher, "Run with…" and "Several agents…" still close on a click outside.
- [ ] **Workspace palette:** the workspace menu shows 9 swatches + "no color"; in dark and in light
  every swatch is easy to tell from its neighbours and visible on the menu background; give
  workspaces 3 neighbouring colors (e.g. red, orange, yellow) and check the sidebar avatars (letter
  on a tint) are distinct and the letters stay readable on the hovered and the selected row. A
  workspace that was `teal`/`pink`/… before keeps its color (a new shade of the same name).
- [ ] **Sidebar filter and chips:** `Filter sessions…` at the top; type a workspace name, an agent
  name and part of a session title: only matching sessions show, in open workspaces, others vanish;
  two words match across fields; Backspace, Ctrl+Backspace, Ctrl+A and paste work; Esc clears it and
  gives the keyboard back to the terminal; an unmatched text shows "No sessions match."; Vietnamese
  Telex typing works. Chips `All N` / `Working N` / `Needs you N` show the right numbers and narrow
  the list; `Needs you` matches the status bar's "N need you" (needs input + done not yet viewed;
  focusing a done pane counts it down).
- [ ] **No Inbox card:** nothing sits under the workspace list; hovering a session row still opens
  the peek popover; Ctrl+Shift+Space peeks the first session that needs you even when its
  workspace is collapsed or the filter hides it (the sidebar opens it), ↑ / ↓ cycle, Esc closes.
- [ ] **Peek leaves with its row:** chip `Needs you`, hover a needs-input row, click the reply field,
  send `y`: the session turns working, its row leaves the list and the peek closes with the keyboard
  back in the terminal (no invisible popover holding the keys). Typing in the sidebar filter with
  AltGr characters types them (no tab switch).
- [ ] **Sidebar header and rows:** `+` adds a workspace; the `▾`/`▸` button collapses or expands
  all (its tooltip flips); the workspace row says `1 working` / `2 idle` (no token total) and
  keeps the amber `?` when something needs input; a session row shows only the time (`17m`) on
  the right (none for needs-input / done), the status icon hover still explains the state; the
  selected row shows the session title on a second line (rows without a title stay one line), the
  row does not jump in height otherwise.
- [ ] **Tab strip (item 5):** each tab shows an icon before its status mark: the agent's badge chip
  for an agent pane, `>_` for a shell, nothing for a tab with no panes; a tab with two panes shows
  the icon of the pane that has focus and changes when you focus the other. The status mark still
  shows the most urgent pane. `+`, `◫` and `⋯` sit at the right end (check the `◫` glyph is not a
  box). `+` makes a tab; `◫` splits the focused pane to the right (same as Alt+Shift+=).
- [ ] **Tab menu:** `⋯` and a right-click on any tab (also an inactive one) open Split right / Split
  down / Rename / Close with their shortcuts; the clicked tab becomes active; Rename edits that
  tab's title inline; Close closes that tab; a click outside closes the menu.
- [ ] **Shortcuts close menus:** open the tab `⋯` menu, the Run ▾ menu and the workspace switcher
  (one at a time), press Ctrl+Shift+P or Ctrl+Shift+1 → the menu closes (no menu floats over the
  palette or the Board).
- [ ] **Pane header (item 5):** `<workspace> › <agent>` after the agent chip (an agent started on a
  worktree task shows `<workspace> › <branch> › <agent>`); one pill `Working · 1m` with the three
  dots, then the pane title; no status/time text at the right, only `⧗ queued`, the context share,
  Share and Turns…; the pill turns `Needs input` / `Idle · 3m` / `Done · 2 files · 1m` with the
  state; hovering it shows `working · from hook · 12s ago`. Narrow a pane (2×2 split): the
  breadcrumb is clipped with `…`, the pill and the buttons stay whole.
- [ ] **Pane menu (item 5):** right-click a pane header → Rename / Change color; Rename edits the title
  in place (Enter saves, Esc cancels; the menu closes); on a Run pane Rename is greyed. Pick a color:
  the agent chip is tinted, a thin line of that color shows under the header, the border and pill are
  unchanged; the first swatch (empty ring) clears it. Colors look distinct in light and dark.
  Restart the daemon (`mk daemon stop`, reopen): the color and name come back on the resumed pane.
  Open the same pane from the sidebar and from a second UI window: the color shows in both.
- [ ] **Share menu:** agent pane header: `Share` opens a menu at the click with "Follow in Zed";
  clicking it turns following on (✓ next time, button highlighted), again turns it off; a click
  outside closes the menu (dark and light).
- [ ] **Pin workspace:** right-click a workspace row → Pin: it jumps to the top of the sidebar with
  📌 after its name; Unpin puts it back among the others by age. Restart the app and the daemon:
  the pin is still there. The next / previous workspace shortcuts follow the new order.
- [ ] **Pin session:** right-click a sidebar session row → the pane menu opens at the pointer with
  Pin, Rename, Change color; Pin moves the row above newer ones with 📌; Unpin returns it.
  Right-clicking a pane header shows the same Pin / Unpin. Stop and restart the daemon: the
  resumed pane is still pinned.
- [ ] **Pin tab:** right-click a tab (or its `⋯`) → Pin: the tab moves to the far left with 📌;
  Unpin returns it to its old place; Ctrl+Tab visits tabs in the shown order.
- [ ] **Pin icon:** 📌 is readable in the dark and light themes and at a small and a large
  `[ui] font_size`, sits on the same line as the name, and does not change any row or tab height.
- [ ] **Session panel:** it is closed at start. Click `◨` at the right end of the status bar (or
  `Ctrl+Shift+Y` with a terminal focused): a column opens right of the terminals; the agent TUI
  redraws once, narrower. Click another pane: the card follows it. `Ctrl+Shift+Y` again closes it.
  In Git or Files, `Ctrl+Shift+Y` shows Terminals with the panel open.
- [ ] **Resize:** hover the panel's left edge (the cursor becomes ↔), drag left and right: the
  width follows the pointer between 240 and 560 px, never more than half the window. Release,
  quit and restart Mkrew, open the panel: same width, and `settings.toml` has `inspector_width`
  (comments and other keys untouched). A click on the edge without dragging writes nothing.
- [ ] **Session card:** for a Claude pane after a turn: agent, title, `working · 2m` / `idle · 30s`,
  ctx %, tokens, cache and model rows. A shell pane shows no usage rows and no transcript.
  **Open transcript** opens the `.jsonl` in Zed (disabled with the tooltip "No transcript yet"
  before the first turn ends); with Zed missing, the sidebar shows the install hint.
- [ ] **Stop:** on a running agent, **Stop** asks "This session is still running."; Cancel does
  nothing; Stop ends it and the pane closes by itself. On a shell, Stop ends it without asking.
- [ ] **Changes card (one repo):** in a git workspace with 2 edited files and 1 new file the panel
  shows `<workspace> M 2 ?1 Review diff`; edit another file: it updates without reopening the
  panel. **Review diff** opens the Git view on that repo; clean → `Working tree clean`.
- [ ] **Changes card (several repos):** on a folder without `.git` holding repos `fe` and `be`,
  only the one with changes has a line (`fe M 3`), and Review diff opens the Git view on `fe`
  (the Repo chips show `fe` selected). A folder with no repo shows no Changes card.
- [ ] **Plan card (item 8):** open the right panel on an agent pane → "Plan" with **Link plan…** →
  pick a `.md` with `- [ ]` / `- [x]` lines → the card shows `Plan <name> a/b`, a bar, up to 8
  rows with the first unchecked one in the accent color and `N more` below; the selected sidebar
  row shows the same bar under its name line, other rows none. Tick a box in the file with an
  editor (also one that saves through a temp file, e.g. Notepad or VS Code) → the counts and bar
  change within about a second, no click. A prose-only edit changes nothing on screen.
- [ ] **Plan link life:** **Change…** with a `.txt` file → an error line in the card, the old plan
  stays; **Unlink** → the card offers **Link plan…** again and the sidebar bar is gone; editing the
  file afterwards does nothing. Link, delete the file → the card shows "no such file" and keeps
  Change / Unlink; put the file back → the counts return. Link, restart the daemon and the UI →
  the resumed pane still shows the plan. Light and dark theme: the bar track and fill are visible.
- [ ] **Open in File Explorer / Zed:** right-click a file in the Files tree → "Open in File
  Explorer" opens Explorer with that file selected; on a folder row it opens the folder; on the
  blank part below the rows it opens the workspace folder (with a task worktree chosen: the
  worktree). "Open in Zed" opens Zed on the file or folder (an error shows in the sidebar if `zed`
  is missing). A click outside closes the menu; right-clicking another row moves it. Right-click
  a workspace in the sidebar → "Open in File Explorer" opens its folder.
- [ ] **Files menu actions (dark and light):** right-click a folder → "New file…": a field
  `New file in <folder>/` shows over the tree; type `a/b.txt`, Enter → `b.txt` exists in
  `<folder>/a`, is open in the editor and selected in the tree. On a file row the entry goes next
  to the file; on the blank part, in the root. "New folder…" → the folder shows open and selected.
  Esc cancels; a blank name or clicking away with nothing typed creates nothing; an existing name
  or `..` shows an error in the sidebar. "Copy path" → the absolute path (backslashes on Windows)
  pastes in Zed. "Add to .gitignore" on a row → `.gitignore` gets `/path` (`/path/` for a
  folder) once, the row disappears (or turns muted with Show ignored). Not offered on the blank
  part. With a task worktree chosen, everything happens in the worktree.
- [ ] **Ctrl+Click links:** in an agent pane or a shell, `echo https://example.com/a?b=1.` then
  hold Ctrl over the URL: underline + hand pointer, only on the URL (not the final `.`); Ctrl+Click
  opens the browser. `echo crates/mkrew-ui/src/main.rs:120:5` (and `.\Cargo.toml`, and an absolute
  `C:\…\main.rs:12`): underline, Ctrl+Click opens the Files view on that file with the caret on
  line 120, scrolled into view; a path that does not exist, a folder, or a file outside the
  workspace gets no underline and Ctrl+Click selects as usual. Pressing/releasing Ctrl with the
  pointer at rest toggles the underline in the focused pane (in another pane it updates when the
  pointer moves); the pointer leaving the pane clears it; scrolling the wheel clears it, and a
  Ctrl+Click opens what is under the pointer now. A pane of a worktree task resolves paths in its
  worktree and switches Files to it. With unsaved text in Files, Ctrl+Click on another file asks
  Save / Don't save / Cancel. On macOS use Cmd. Terminal CPU stays flat while a flood runs with
  Ctrl held.
- [ ] **Markdown preview in Files:** open a `.md` file in Files → rendered page (headings with
  rules, lists, tables, highlighted code blocks, a ```` ```mermaid ```` block drawn as a diagram),
  `Preview | Source` in the header. Source = the editor: type a few words (● shows), switch to
  Preview → the words are in the page, switch back → caret and undo intact, nothing lost; Ctrl+S
  saves from either mode. Ctrl+F in Preview switches to Source with the bar open. Edit the file in
  Zed (no unsaved text) → the page follows. Ctrl+Click `README.md:12` in a terminal → Source,
  caret on line 12. Toggle the theme: the page and the diagram follow (light + dark). A 2 MB
  `.md` does not freeze the window while it renders.
  Source → Ctrl+F (bar focused) → click Preview → Ctrl+Shift+P opens the palette and Esc works
  (the keyboard is not stuck in the hidden find field).
- [ ] **Git diff Unified / Split:** in Git, select a changed text file. The header shows
  `Unified | Split` (not for a binary or too-large file). Click Split: two columns, old on the
  left and new on the right, one scroll bar moving both; a line edited in place is amber on both
  sides, a new line is green on the right with an empty left cell, a removed line is red on the
  left with an empty right cell, unchanged lines show on both sides, `⋯ N unchanged lines` and the
  hunk header span the full width (Stage hunk still on it). Both themes: text readable on all
  colors. Narrow window: long lines clip inside their half, nothing overlaps. In Unified too, a
  line edited in place (its `−` and `+` rows) is amber, a pure add green, a pure delete red.
- [ ] **Diff mode is remembered and global:** pick Split, select other files, switch to History and a
  commit's file, switch the repo chip of a workspace with several repos: still Split. Quit and
  restart: still Split; `settings.toml` has `diff_mode = "split"`. Edit it to `"unified"` while the
  app runs: the diff changes within a second. `diff_mode = "side"` shows a settings error in the
  palette and keeps the current mode.
- [ ] **Diff keys and comments in both modes:** with the Git list focused, `d` toggles the mode (the
  tooltip and the palette entry "Git: Toggle Unified / Split diff" show it); `n` / `p` bring the
  hunk header to the top in both; switching mode keeps the current hunk in view. In Split, click a
  right-column line: accent bar and number on the right half; Shift+click extends; Ctrl+Alt+M adds
  a comment that quotes those lines; clicking a left-only (removed) line does nothing.

### To verify — 2026-10-09 inbox: Shift+Enter, F11, Files editor, several repos, scrollback, pane keys, shells, rename (plan docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md)

- [ ] **Shift+Enter:** in claude, codex, copilot and omp panes, type `a`, Shift+Enter, `b` → two
  lines in the prompt, nothing sent; Enter sends. In pwsh and Git Bash, Shift+Enter does not run the
  half-typed command.
- [ ] **F11:** F11 fills the screen (no title bar, taskbar hidden); F11 again restores the previous
  size and position; works with a terminal focused; the palette lists "Global: Toggle full screen".
- [ ] **Save (Files):** restart the daemon first (protocol bumped, now 32; stop it as in the
  2026-10-07 section below). Then open a CRLF file, edit, Ctrl+S → only the edit changed in
  Zed/`git diff` (line endings and final newline kept). Edit the same file in Zed while Mkrew has
  unsaved text and save → "Changed on disk — Reload · Keep mine"; Keep mine writes your text,
  Reload takes the disk's.
- [ ] **Files editor:** restart the daemon (protocol v32). Open a `.rs` file:
  - A click puts the caret there (I-beam pointer).
  - Arrows, Ctrl+arrows, Home (indent, then line start), End, PgUp and PgDn move the caret;
    Shift extends; dragging selects; Ctrl+A selects all.
  - Type Vietnamese in the middle of a line, once with Unikey and once with Windows Telex.
  - Ctrl+C / X / V work with Zed. Ctrl+V with an empty clipboard changes nothing (no `●`).
  - Ctrl+Shift+T and the other window shortcuts still work while the editor has focus; on an
    AltGr layout, AltGr characters type into the text.
  - Enter keeps the indent. Tab in a Makefile inserts a tab.
  - Ctrl+Z undoes a typed word at once; Ctrl+Y redoes; Ctrl+Shift+Z still opens Zed.
  - While there are changes, `●` shows before the path and Save turns primary.
  - Ctrl+S saves, also from the find field (the find field keeps the keyboard): `●` goes away,
    `git diff` shows only your change, and a CRLF file stays CRLF (no `^M` churn in `git diff`).
  - Header shows "Ln N, Col M" for text files only (not for a binary or too large file).
  - Ctrl+Shift+L with a selection sends the selected lines; without one, the caret's line.
  - Colors come back ~150 ms after you stop typing.
  - A long line scrolls sideways.
  - In light and dark: the caret, the selection and the raised caret row are readable.
- [ ] **Unsaved edits are kept:**
  - Edit a file, then let an agent (or Zed) change it. Your text stays and "⚠ Changed on disk —
    Reload · Keep mine" shows. Reload shows the agent's text. Keep mine, then Ctrl+S, writes yours.
  - Edit, then click another file, use quick open (Ctrl+Shift+O), or pick a task worktree.
    "Save changes to <name>?" appears:
    - Cancel (and Esc) stays.
    - Don't save opens the other file.
    - Save saves, then opens it.
  - Edit, switch workspace and come back: the edit is still unsaved.
  - Edit and close the window: the same question. Save and Don't save close it; Cancel keeps it
    open.
- [ ] **Find & replace:**
  - Select `foo` on one line and press Ctrl+F: the bar opens with `foo`, and the selected one is
    current.
  - Ctrl+H: the replace row opens and its field has the keyboard. Type `bar`. Enter replaces the
    current match and selects the next one. Ctrl+Alt+Enter (or "Replace all") replaces the rest.
    `●` shows and nothing is saved until Ctrl+S, which also works from the bar.
  - Ctrl+Z brings everything back at once.
  - Replace `a` with `aa`: no endless loop.
  - The Vietnamese query `việt` matches `VIỆT`.
  - Tab / Shift+Tab switch fields; Esc closes the bar and typing goes to the editor.
  - The ▸ / ▾ button shows and hides the replace row; "Replace" and "Replace all" buttons work.
  - Ctrl+V in both fields pastes one line. Ctrl+Alt+1…9 does not switch tabs while a find/replace
    field has the keyboard (it is AltGr there); after Esc it jumps tabs again.
  - Ctrl+Shift+H still toggles ignored files; Ctrl+Shift+1 still shows the Board.
  - Check light and dark.
- [ ] **Several git repositories in one workspace (ADR 0029):** make a folder `root/` with `fe/`
  and `be/`, each a `git init` repository with a commit, and no `.git` in `root/`. Add `root/` as
  a workspace.
  - Files view: the `fe` and `be` folders show the change dot when they have changes; inside
    `fe/`, a modified file shows `M`, a new one `?`.
  - In a terminal in `fe/`, `git commit --allow-empty -m x` → the Git view of that repo refreshes
    by itself (no Refresh click).
  - A third folder `docs/` without `.git` never shows as a repository.
- [ ] **Git view Repo chooser (ADR 0029):** with the same `root/` (each repo with an edited file):
  the Git view shows no "Cannot read the git status" (at most a blink) and the header shows
  "Repo  be  fe" with `be` active, listing `be`'s files. Click `fe` → its branch, files and diff;
  the commit draft stays. With `root/` itself a repository too, the first chip is the workspace
  name. A workspace with one repository shows no Repo row; with a task worktree shown the Repo row
  is hidden. Check dark and light.
- [ ] **Stage / commit per repository:** in `fe` stage a file and a hunk, Discard another, ✦
  writes a message about `fe` only, Ctrl+Enter commits → `git -C fe log -1` has it, `be`
  unchanged. History lists `fe`'s commits only; Open in Zed opens `fe/<file>`; a diff comment
  lands on `fe/<file>`. Fetch / Push act on `fe` (its upstream shown).
- [ ] **Terminal commit refreshes:** with `fe` shown, run `git -C fe commit --allow-empty -m x`
  in a terminal → the History / ahead count updates within ~1 s without clicking.
- [ ] **Scrollback after a tab switch (ADR 0030):** in a pwsh pane run
  `1..300 | % { "line $_" }`, then Ctrl+Tab to another tab and back. Also try opening the Board and
  coming back, and minimizing and restoring. Each time, the mouse wheel and Shift+PgUp scroll back
  about 200 lines above the screen (to around `line 100`), the screen looks the same as before the
  switch, and typing returns to the live prompt. Do the same in a claude pane after a long answer.
  `mk attach <session>` in Windows Terminal: its scrollbar shows the same lines above the screen.
- [ ] **Alt+Shift+A / Alt+Shift+S:** with a terminal focused, Alt+Shift+A splits the focused pane to
  the right with the workspace's default agent and Alt+Shift+S with its default shell, the same as
  + Agent / + Shell; from Board / Files / Git they switch to Terminals first. The buttons' tooltips
  and the empty tab's chips show the keys; the palette lists "Panes: New agent pane" / "… shell pane".
  - A terminal no longer gets Alt+Shift+A / Alt+Shift+S as ESC A / ESC S (readline); rebind
    `split_agent` / `split_shell` in keymap.toml if needed.
- [ ] **Default shell (ADR 0032):** after the daemon restart, `<config>\mkrew\agents\` holds
  `pwsh.toml`, `powershell.toml`, `cmd.toml`, `bash.toml` and `git-bash.toml` next to your edited
  files (none overwritten). In a workspace without a default shell, + Shell (or Alt+Shift+S,
  Ctrl+Shift+T) shows "Pick a default shell…" and opens the workspace menu. "Default shell for new
  panes" lists only shells found on this machine. Pick PowerShell → + Shell opens pwsh in the
  workspace folder (`$PSVersionTable` shows 7). Pick Git Bash → `pwd` is the workspace folder.
  The Run button still runs through `shell` (cmd), as before. If `pwsh` is installed only as a
  WindowsApps app-execution alias, check it still shows as installed in the menu (the lookup uses
  `is_file`).
- [ ] **Rename pane (ADR 0031):**
  - Focus an agent pane and press F2: the header title becomes a field. Type `Fix login` and
    press Enter. The header, the sidebar row, the Inbox entry (when it needs input) and its OS
    notification all show `Fix login`.
  - F2, then Esc: nothing changes. F2, clear the text, Enter: the pane has no title again.
  - Double-click the header title, or the empty space after the agent name: the same field opens.
  - Stop and restart the daemon (`mk daemon stop`, reopen Mkrew): the resumed pane keeps its name.
  - On a Run pane (`▶ Run`), F2 and double-click do nothing.
  - A TUI in a pane (vim, htop) no longer gets F2; rebind `rename_pane` in keymap.toml if needed.
- [ ] **Auto pane name (ADR 0031):** restart the daemon first (protocol bumped, now 32).
  - Start a claude pane and ask it to fix something. Within a turn, the header, the sidebar row
    and the palette's "Go to session" show Claude's conversation title without the spinner glyph,
    and it does not flicker while Claude works.
  - F2, type `Mine`, Enter: `Mine` shows everywhere and stays while Claude keeps working.
  - F2, clear the text, Enter: Claude's title is back.
  - A pwsh pane never gets an automatic name.
- [ ] **Field keys (Ctrl+A, Ctrl+Backspace, Ctrl+V):** in each of these, type `hello world`,
  press Ctrl+Backspace → `hello ` (again → empty); type `one two`, press Ctrl+A (the text is
  highlighted, no caret), type `x` → just `x`; Ctrl+A then Backspace → empty; Ctrl+V pastes the
  clipboard text (one line in single-line fields). Fields: task dialog title and prompt, card
  composer (Ctrl+Shift+Enter), commit box, Run setup (name, folder, command, env file and env),
  comment box, broadcast, peek reply, quick open (Ctrl+Shift+O), command palette (the list
  re-filters), Docs filter, base branch menu, terminal find (Ctrl+Shift+F; Ctrl+V pastes, matches
  follow), Files find and replace (Ctrl+F, Ctrl+H) and the tab / pane rename (F2 / Ctrl+Shift+R).
  In the Vietnamese IME (Unikey, Telex): type `việt nam`, Ctrl+Backspace → `việt `; Ctrl+A, then
  type `tieengs` → only the new word; plain Backspace still removes one letter. Ctrl+Backspace in
  a terminal pane still deletes a word of the shell line (^W). Ctrl+Alt+1…9 types the AltGr
  character (or nothing) in the Files find bar, and jumps tabs again after Esc.
- [ ] **Run ■ / ↻:** set up two commands (a dev server and one more) and press ▶. ■ on one (the
  Run menu row) → its pane leaves the `run` tab at once and the other keeps running; ■ on the last
  → the `run` tab is empty. ▶ again → panes are back. ↻ on one → still one pane per command, in the
  same place, with fresh output (no "Session exited" pane beside it). A command that ends by itself
  (`echo hi`) → its pane leaves the tab.
- [ ] **Exited sessions clean up (ADR 0033):** `exit` in a shell pane → the pane and its sidebar row
  disappear (the sibling pane fills the space; a zoomed pane unzooms); quit an agent (`/exit`) →
  the same; ✕ on a live shell pane → no `✕ exited` row left behind. History (toolbar) (superseded by ADR 0034/0036) still lists
  them with their exit code and replays their output. A workspace with no live session shows
  "No sessions". **Daemon restart:** with a live agent pane (one with a resumable conversation) open,
  `mk daemon stop`, start Mkrew again → the pane resumes in the same place (it was not removed).
  Remove a workspace that has a live agent pane → its panes leave the tabs; adding the folder again
  resumes a task's agent (the pane is not restored; open it from the sidebar).

### To verify — 2026-10-08 inbox 3: dots, drop, paste, icons, redesign (plan docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md)

- [ ] **Hooks (user setup, item 18):** run `mk integration install claude`, `… codex`, `… omp`,
  `… copilot`, restart the agents and the daemon. Claude: prompt → three dots; a permission
  prompt → amber `?` + amber pane border; turn ends → `✓ done` + Inbox entry; later `○ idle`.
  Hover a status: source `hook`. Without the install `done` never appears (SPEC §6).
- [ ] **Working dots:** a working session shows three dots taking turns (~1.5 s cycle) in the
  sidebar row, the workspace pill, the tab, the pane header, the Inbox and board cards; rows with
  other states line up with it.
- [ ] **File drop:** drag 2 files (one path with a space) from Explorer onto (1) the terminal,
  (2) the open composer (Ctrl+Shift+Enter), (3) the pane header → paths typed, the spaced one
  quoted, no Enter; with the composer open they go into the draft and it keeps the keyboard.
  (4) Type a letter in a pane, Alt+Tab to Explorer overlapping Mkrew, drop without moving over
  Mkrew first → still typed. (5) Drag over a pane, release outside the window, click the pane →
  nothing typed. (6) Open the palette (Ctrl+Shift+P) over an agent pane and drop on it → nothing
  typed into the pane. Mkrew must not run as administrator.
- [ ] **Ctrl+V:** in Claude, omp, codex and pwsh panes: text → pasted (bracketed); Win+Shift+S
  then Ctrl+V → `…attachments\<id>\<ulid>.png` typed; PrintScreen then Ctrl+V → a `.png` path;
  Explorer copy of 2 files → 2 paths; empty clipboard in vim → Ctrl+V starts block visual. In the
  composer, an image clipboard + Ctrl+V → path in the draft. No visible stall on a 4K screenshot.
- [ ] **Agent icons:** after a daemon restart + UI reconnect, your existing profiles show ✻ claude,
  ⌬ codex, π omp, ⊙ copilot, >_ shell as bordered chips in the sidebar rows, pane headers, Inbox
  rows, board cards and the Run with… / default agent menus (glyphs, not boxes; light + dark).
- [ ] **Theme:** the toolbar toggle (superseded by ADR 0034/0036) cycles System (◐) → Light (☀) → Dark (☾); every surface
  updates at once, no dark pieces left in light or the reverse; `settings.toml` gets
  `[ui] theme = "…"` with your comments kept; Ctrl+Shift+J does the same. In System mode, change
  Windows' app mode → the app follows live. A broken settings.toml → the toggle still switches,
  the palette shows the error, the file is untouched.
- [ ] **Both themes, these states visible:** hover, selected session, active tab, working dots,
  the Inbox card empty and non-empty (superseded by ADR 0034/0036), the empty-state buttons with their Ctrl+Shift+A / T chips.
  Muted text on the sidebar and status labels is readable in light mode. Screenshots of each
  view in light and dark for §14 (`docs/screenshots/`).
- [ ] **Layout:** no right rail; toolbar 44 px with the segmented views (superseded by ADR 0034/0036); switching views, Run and
  its ▾ menu, History, + Agent, + Shell (also from Board/Docs/Files/Git), creating / closing /
  switching tabs, ✕ on an inactive tab closes it without activating it, expanding and collapsing
  workspaces, ⋯ on hover, selecting sessions. At `[ui] font_size = 20` nothing clips.

### To verify — 2026-10-08 inbox: status, drop, badges (plan docs/superpowers/plans/2026-10-08-inbox-status-drop-badges.md)

- [ ] **States with hooks (user setup):** run `mk integration install claude`, `… codex`,
  `… omp`, `… copilot`, then restart the agents. Claude: send a prompt → the three dots take turns; it asks for a
  permission → `?` and an amber pane border; the turn ends → `✓ done` and an Inbox entry; wait →
  `○ idle`. Same in omp. Hover a status: the source says `hook`. Without the install, `done`
  never appears and omp/Copilot never show `?` (SPEC §6: done is hook-only).
- [ ] Collapse a workspace with a working agent: its `••• N` count steps.
- [ ] **File drop** (T10.8 check below, still open): drag 2 files (one with a space) from
  Explorer onto a Claude pane and a pwsh pane → both paths typed, the spaced one quoted, no Enter.
  Mkrew must not run as administrator: Windows blocks Explorer drops into elevated windows.
- [ ] **Agent badges:** the sample profiles show ✻ claude, ⌬ codex, π omp, ⊙ copilot, >_ shell
  in the sidebar rows and pane headers (light + dark; check the glyphs render, not boxes). Your
  existing `%APPDATA%\mkrew\agents\*.toml` [INFERENCE: `dirs` config dir] are not updated: they
  show claude `C`, codex `C`, omp `O`, copilot `C` until you add `icon = "…"` to each; the change
  shows after the next refresh (reconnect or restart the UI). `icon = "claude"` → the profile
  error lists `file:line:col`.

### To verify — 2026-10-08 inbox: keys, fonts, view rail (plan docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md)

- [ ] Git commit box: type two lines, Ctrl+A highlights them, Backspace empties the box; Ctrl+A then
  typing or Ctrl+V replaces the text. Same in the composer and the card follow-up.
- [ ] Terminal: drag-select text, Ctrl+C → it is on the clipboard (paste into Zed). Without a
  selection Ctrl+C still stops `ping -t 127.0.0.1` in a shell pane. Select, type a letter, then
  Ctrl+C → it interrupts (the selection was cleared). Same after Ctrl+Shift+V paste. In an
  exited pane, select text and Ctrl+C copies it.
- [ ] In omp, Claude Code and a pwsh pane: type `hello world`, Ctrl+Backspace deletes `world`
  only (it sends ^W; PSReadLine's Windows mode binds Ctrl+w to BackwardKillWord).
- [ ] Ctrl+A in omp moves to the line start (by design, terminals send ^A); Ctrl+U clears the input.
- [ ] Palette → `Global: Open settings` opens a commented settings.toml in Zed. `[terminal]
  font_size = 18` → glyphs grow within ~1 s, a running agent redraws to fewer columns.
  `[ui] font_size = 15` → sidebar/board text grows. `font_family = "Consolas"` changes the
  terminal; a family that is not installed changes nothing. `font_size = 4` → nothing changes,
  the palette shows the error.
- [ ] (superseded by ADR 0027: the toolbar replaces the rail) Views are a rail on the right; the main area starts at the top. Tooltips show
  Ctrl+Shift+1..5. In Terminals `▶ Run` sits before History; in other views the compact Run at
  the rail foot opens its ▾ menu. `New task` is in the Todo column header. Light + dark.

### To verify — 2026-10-07 workspace, files and docs fixes (plan docs/superpowers/plans/2026-10-07-workspace-files-docs-fixes.md)

The protocol was v26 at the end of this plan (later plans bump it further), and
`target\debug\mk.exe` is rebuilt with every bump, so
`target\debug\mk daemon stop` cannot stop an older daemon (it answers "version mismatch"). Every
build (dev and the installed Mkrew in `%LOCALAPPDATA%\Programs\Mkrew`) uses the same daemon, and
ending it stops all running agents and run commands at once, with no graceful shutdown. So first
let your agents finish or stop them, close the Mkrew window (or `Stop-Process -Name mkrew`), and
only then run `Stop-Process -Name mkrew-daemon` (PowerShell). Then start `cargo run -p mkrew-ui`.

- [ ] 1. **Remove a workspace.** Workspace menu → "Remove workspace…" on a workspace with a running
  agent and its run commands. Expected: a confirmation that names the workspace and says how many
  running sessions stop; after Remove, the row is gone, the agents stop, and another workspace is
  shown. Then add the same folder again: it returns with its tasks and history.
- [ ] 2. **Remove the shown workspace.** Workspace menu → "Remove workspace…" on the workspace that
  is currently shown → Remove: the view moves to another workspace (not an empty or stale view).
- [ ] 3. **Remove an agent row.** Hover an exited agent row: ✕ removes it. On a running one, ✕ asks
  first, then stops it and closes its pane. Restart the app: removed rows stay gone.
- [ ] 4. **Workspace color.** Workspace menu → Color: pick teal, then "No color". The dot before the
  name follows, in the dark and light themes.
- [ ] 5. **Full-width rows.** In the Files tree and the Git list, click anywhere on a row, also right
  of the name. The whole row width highlights, and the markers sit at the right edge.
- [ ] 6. **Find in file.** In Files, open a file, press Ctrl+Shift+F (or "Find"), and type `việt` /
  `TODO`. Expected: matches are highlighted; Enter, Shift+Enter, ↑ and ↓ move between them and
  scroll; "N of M" updates; Esc closes the bar. Click the bar's ↑/↓ buttons: the cursor stays in
  the field (typing still goes there). Let an agent edit the open file: the count follows.
- [ ] 7. **Docs filter.** In Docs, Ctrl+Shift+F focuses "Filter files…". Typing filters the list,
  with the matched letters bold; ↑/↓ open the previous/next file; Esc clears the filter.
- [ ] 8. **Docs preview.** Open a plan in dark and light with headings h1–h4, nested and numbered
  lists, a table, a quote, `inline code`, and ```rust, ```ts and ```mermaid blocks (a flowchart and
  a sequenceDiagram), plus a ```mermaid `pie` block with a large and a tiny slice (`pie`,
  `"A": 95`, `"B": 3`). Expected: a readable, centered column; colored code (```ts too); diagrams
  drawn in theme colors; in dark mode the pie labels (inside and outside the slices) are readable.
  Retake the §14 screenshots.
- [ ] 9. **Broken mermaid.** A malformed block (e.g. `flowchart LR` then `A[/]`) shows its source
  and the reason; the app does not crash.
- [ ] 10. **Memory.** With a doc that contains 3 diagrams open: the UI process stays under 150 MB
  (Task Manager); in a release build (`cargo run --release -p mkrew-ui`), the first diagram shows
  within ~1 s.

### 2026-10-07 Run button (plan docs/superpowers/plans/2026-10-07-run-button.md)

- [ ] EA: Run setup with BE (folder `EA-BE`, command `.venv\Scripts\python.exe main.py`, env file
  `.env`, env `ENVIRONMENT=dev` and `PYTHONPATH=.`) and FE (folder `EA-FE`, `npm run dev`).
  ▶ Run opens the "run" tab with two panes; the button shows `● Running 2/2 · :8080 :5180`.
  `[INFERENCE]` uvicorn prints its URL through EA's console log handler; if `:8080` is missing,
  note it.
- [ ] ↻ on BE in the ▾ menu restarts only BE (the FE pane keeps its output); the view stays.
- [ ] Ctrl+C in BE's pane: the button turns `✕ Running 1/2` in the warning color; ▶ on BE in the
  menu starts it again.
- [ ] ■ stops both; the button returns to `▶ Run`; the sidebar ▶ disappears.
- [ ] A command in a missing folder: ▶ Run shows "Cannot run: …" in the sidebar notice; nothing starts.
- [ ] Open ▾ over a Board card or a terminal pane: clicking a menu row does not click what is
  underneath; clicking outside closes the menu.
- [ ] Known gaps: the ▾ menu is mouse-only (the palette has Run: Start/Restart/Stop all, Setup…,
  Show Run tab); the default shell on Windows is cmd, so a `.ps1` script needs
  `pwsh -File .\run.ps1` as its command.

- [ ] **2026-10-07 resume omp/Copilot/free panes** (plan `docs/superpowers/plans/2026-10-07-resume-omp-copilot-panes.md`).
  1. Add `resume = "omp --resume {session_id}"` to `%APPDATA%\mkrew\agents\omp.toml` and set
     `mode = "hook+screen"`. Run `mk integration install omp` and `mk integration install copilot`.
  2. Open a project, `+ Agent` with omp, chat one turn; same with copilot, claude, codex (no task).
     Close the UI, reopen it: all panes are still live (unchanged behaviour).
  3. `mk daemon stop`, then reopen the UI: each pane comes back in the same tab position with the
     same conversation (`omp --resume`, `copilot --resume=`, …).
  4. Reboot Windows, open the UI: same as 3.
  5. In omp, ask it to run `omp -p hi` in its bash tool; after the restart the pane still resumes
     the original conversation (nested guard).
  6. Close an agent pane (kill), restart: it does not come back.
  7. In omp/copilot: a tool approval shows `needs_input` (`?`), and finishing a turn shows `done`.

- [ ] **2026-10-07 daily fixes** (plan `docs/superpowers/plans/2026-10-07-daily-fixes-and-ai-helpers.md`).
      Stop the old daemon first (`target\debug\mk daemon stop`, protocol v24), then
      `cargo run -p mkrew-ui`:
  1. Vietnamese: with Unikey (Telex, default settings) and with Windows "Vietnamese Telex", type
     `Tiếng Việt có dấu` into a terminal (pwsh, then claude/omp), the composer, the commit box,
     the palette and quick open. Expected: exact text, no lost or doubled chars.
  2. Ctrl+Shift+1…5 switch views and Alt+Shift+= / Alt+Shift+- split, with a terminal focused
     and without (no NUL reaches the agent). Switch the Windows layout US ↔ Vietnamese while the
     app runs and repeat.
  3. Spawn omp and leave it at the prompt: after ~3 s the status shows `idle · Ns`. Send a
     prompt: `working · Ns` counts only during the turn.
  4. In a terminal (Git Bash): `printf '\ue0b0 \uf015 \U000f0001 ✔ 😀 abc\n'`, then launch omp.
     Expected: icons instead of boxes, monospaced Latin text.
  5. Git view: the file list and commit box sit right of the diff, in dark and light themes
     (retake the §14 screenshots).
  6. Files view: "Search files…" at the top opens quick open, as Ctrl+Shift+O does. With the
     Files view switched to a task worktree, picks open the worktree's copy.
  7. ✦ in the commit box with claude, codex and omp as the default agent: a Conventional
     Commits message; Commit still needs a click. With an old profile without `oneshot`, the
     error names the line to add and the file.
  8. ✦ in the composer rewrites the draft (Vietnamese stays Vietnamese); Send stays manual.
  9. Confirm `omp -p --no-session` reads the prompt from stdin (item 7 with omp). If not,
     record the working `oneshot` line in `agents/omp.toml`.
- [x] (T7.4) **Build machine:** a leftover `mkrew-fakeagent.exe` process (probably from a test
      daemon that survived a killed run) locks `target\debug\mkrew-fakeagent.exe`, so
      `scripts/verify.sh` fails to relink it ("Access is denied"). Please end stray
      `mkrew-fakeagent.exe` / `mkrew-daemon.exe` processes. Also, drive C: has only ~7 GB free,
      and a second target dir filled it up during T7.4 (LNK1318). T7.5: still locked (4.9 GB
      free); renamed it to `target\debug\mkrew-fakeagent.locked-old.exe` (allowed while it
      runs) so verify could relink — delete that file once the process is gone.
      T7.6: drive C: was full (313 MB free, LNK1318/LNK1140). `cargo clean --release` and
      `cargo clean --target x86_64-unknown-linux-gnu` (the latter also emptied `target\debug`)
      freed it to ~91 GB; only build output inside `target\` was removed (rebuilt by verify).
      Cleanup 2026-10-06: stray `mkrew-fakeagent.exe` killed and its `.locked-old.exe` deleted.
      A live `target\debug\mkrew-daemon.exe` (user's sessions) was renamed to
      `mkrew-daemon.locked-old.exe` so verify can relink; delete it once that daemon stops.
      Uncommitted `.gitignore` edit (`*.log` commented out) stashed as
      `recovered-2026-10-06-gitignore-log-rule`. Verify green afterwards.

- [ ] (T0.1) Run `pwsh scripts/verify.ps1` once. The headless session was not allowed to run
  `pwsh`; `bash scripts/verify.sh` (same fmt/clippy/test steps) passed on Windows.
- [x] (T0.2) Before T0.8 (measurements, release build): install the Windows SDK so `fxc.exe`
  is available, or set `GPUI_FXC_PATH`. Release gpui builds panic without it. — Resolved:
  `fxc.exe` is on PATH (`~/msvc/Windows Kits/10/bin/10.0.28000.0/x64`); T0.8 built release.
- [ ] (T0.4) `crates/spike-term/tests/fixtures/tui_frame.ans` is a hand-made Claude-Code-like
  frame. Record a real `claude` session (e.g. via `script`/a PTY logger), convert it to the
  escaped fixture format (`tests/fixtures/README.md`), add it as a fixture and accept its
  snapshot. Until then the M0 "recorded agent output" criterion stays unticked.
- [ ] (T0.5–T0.7) Visual, input, resize and real-agent checks moved to "Waiting (blocking)"
  (T0.9). Also: the Unix exec-bit branch of `launch::is_runnable` was never compiled here.
- [ ] (T0.8) Real key-to-glyph latency: the automated number (ADR 0001 "Measurements") uses
  synthetic keys written to the PTY and stops at paint end, so it misses OS input dispatch and
  GPU present. Optionally measure with a high-speed camera / `Typometer`-style tool.
- [ ] (T1.5) The Unix branch of `mkrew-daemon/src/ipc.rs` (`try_overwrite`, Linux `mode`)
  was never compiled here; the CI Linux job (T1.12) covers it.
- [ ] (T1.6) `mk daemon start` from a normal terminal: no console window appears, the daemon
  survives closing that terminal, `mk daemon status`/`stop` work afterwards. The Unix
  `process_group` branch was never compiled here.
- [ ] (T1.8) The Unix branches of `mkrew-daemon/src/session.rs` (SIGHUP via portable-pty,
  `rustix` SIGKILL) were never compiled here; the CI Linux job (T1.12) covers it.
- [ ] (T1.10) `mk attach` in a real terminal (Windows Terminal + a Unix terminal/SSH): raw
  keys incl. arrows, Ctrl+C and paste reach the agent, resizing the window resizes the
  session, `Ctrl+]` detaches and leaves the terminal sane (cursor, no mouse/bracketed-paste
  leftovers), and re-attach repaints the same screen. Tests only cover the piped (non-tty) path.
- [ ] (T1.12) Push the repo to GitHub and check that `.github/workflows/ci.yml` runs green on
  both `windows-latest` and `ubuntu-latest`. Never run here (no push allowed). Not compiled
  locally for Linux: anything depending on bundled SQLite (`mkrew-store`, `mkrew-daemon`,
  `mkrew-cli` incl. the Unix branches from T1.5/T1.6/T1.8) — no Linux C compiler on this machine.
  `mkrew-core/protocol/fakeagent` and `spike-term` pass `cargo clippy --target
  x86_64-unknown-linux-gnu -D warnings`. The Linux job installs no apt packages (gpui-pre's
  `x11`/`wayland` features are off, so no xkbcommon/wayland/fontconfig; the cross-check never
  linked, so a link error would show only in the test step); if linking fails, add
  `sudo apt-get install -y libxkbcommon-dev libwayland-dev libfontconfig-dev libfreetype-dev`.
  Watch for timing-sensitive daemon/CLI tests on slower runners.
- [ ] (T1.12) Native Windows: `mk daemon start` from Windows Terminal opens **no** console
  window, and `mk attach` to a session of `agents/shell.toml` and `agents/claude.toml` works
  (see the T1.6 and T1.10 items above for the details to check).
- [ ] (T2.2) Run `cargo run -p mkrew-ui` with no daemon running: a "Mkrew" window opens
  ("Connecting to the daemon…" → "Connected." + version), a detached `mkrew-daemon` starts
  (no console window), and it keeps running after the window closes; a second launch reuses
  it. Check the shell in dark and light OS themes (§14 design check; running GUI binaries
  needs approval in headless sessions, so not done automatically).
  (T2.3: with no live session the line now reads "No running sessions."; the version is logged.)
- [ ] (T2.3) Start a session (`mk session spawn <project> --agent shell`), then `cargo run -p
  mkrew-ui`: the window shows it in a terminal pane (colors, cursor block, wide chars); typing,
  Ctrl+C, arrows and Ctrl+Shift+V paste reach the shell; resizing the window reflows and
  `mk session list` shows the new cols/rows; minimizing does not shrink the session; `exit`
  shows "✕ Session exited (code 0)." under the pane; a `flood` fake-agent session stays
  responsive. Check dark and light themes.
- [ ] (T2.4) `cargo run -p mkrew-ui`: the sidebar lists workspaces; with none, "No workspaces
  yet." + "Add workspace Ctrl+Shift+K". The button, the header "+" and Ctrl+Shift+K (also while
  a terminal has focus) open the OS folder picker; the picked folder appears expanded ("No
  sessions yet."); adding the same folder again shows an error under the header. Expand /
  collapse a workspace; sessions show icon + status text; the workspace row shows the working
  count and `?` when one needs input (fake agent `ask`). Clicking another session switches the
  pane. Hover tooltips (path, shortcut). Check dark and light themes.
- [ ] (T2.5b) `cargo run -p mkrew-ui` with two live sessions in one workspace: a tab bar shows
  "Terminal"; clicking the second session in the sidebar splits the pane side by side (4 px
  gaps, rounded frames, focused pane has a faint accent border, the other is slightly dimmed);
  clicking a pane moves focus and keyboard input. "+" adds "Terminal 2" (empty state), clicking
  tabs switches panes (hidden ones stop updating). Close the window and reopen: same tabs and
  split, both panes live. Restart the daemon (`mk daemon stop`): panes show "✕ Session
  exited", layout kept. Check dark and light themes.
- [ ] (T2.6) `cargo run -p mkrew-ui` with a workspace whose default agent is set (`mk` /
  `project.update`): "+ Agent" and "+ Shell" open panes in the project folder next to the focused
  one (shell = cmd/pwsh, `cd` shows the project path); an empty tab offers "New agent" / "New
  shell". Ctrl+Shift+X (also while the terminal has focus — must not reach the PTY) on a shell
  closes it; on a running agent the dialog offers Kill / Hide / Cancel; hidden sessions stay in
  the sidebar and clicking them brings the pane back; close and reopen the UI: still alive.
  With 13+ live sessions the tab bar shows the "⚠ … sessions running" notice.
- [ ] (T2.7a) `cargo run -p mkrew-ui`, focus a terminal: Alt+1…9 / Ctrl+Alt+↑↓ switch
  workspaces, Ctrl+Shift+T / Ctrl+Shift+A open a tab with a shell / the default agent,
  Ctrl+Tab / Ctrl+Shift+Tab / Ctrl+Alt+1…9 switch tabs, Ctrl+Shift+W closes the tab (its sessions
  stay in the sidebar), **Alt+Shift+= / Alt+Shift+-** split right/down (fixed 2026-10-07: bindings
  are mapped through the platform keyboard mapper, so Windows' shift-folded keys match; check with
  a US and a Vietnamese layout, also after switching layout while the app runs; also that Ctrl+Tab is not eaten by the OS), Alt+arrows move focus, Ctrl+Shift+V pastes (bracketed in Claude Code). None of these
  reach the PTY; **Ctrl+C, Ctrl+R, Esc inside Claude Code** still work. Put
  `close_pane = "ctrl-shift-q"` and `bogus = "x"` in `%APPDATA%\mkrew\keymap.toml`: Ctrl+Shift+Q
  closes panes, the sidebar shows "✕ unknown command `bogus` …", the tooltip says Ctrl+Shift+Q.
  AltGr layouts: Ctrl+Alt+digit is AltGr+digit (e.g. `{` on German) and is intercepted by the
  tab-jump defaults — rebind if you type those characters.
- [ ] (T2.7b) `cargo run -p mkrew-ui`, in a shell pane run a command with long output (`dir /s`):
  mouse-drag selects text (accent-tinted, readable, I-beam pointer), Ctrl+Shift+C copies it
  (paste elsewhere matches; the highlight clears), Shift+PgUp/PgDn and the wheel page through
  the history, typing returns to the live screen. In Claude Code (full-screen): the wheel
  scrolls its view (arrow keys), selection + copy still work. Ctrl+Shift+C is not sent to the PTY.
- [ ] (T2.7c) In a pane with long output press Ctrl+Shift+F: the find bar appears top-right
  (light + dark), typing highlights matches (amber, readable) and jumps to the nearest one,
  Enter / Shift+Enter walk older / newer matches and scroll the view, "No matches" / "✕ Invalid
  pattern" show, Esc or ✕ closes it, clears the highlights and typing goes to the terminal
  again. Clicking the bar of an unfocused split focuses that pane (border moves) and the bar.
- [ ] (T2.7d) In a tab with splits: Alt+Shift+←/→/↑/↓ moves the divider next to the focused
  pane and the sizes survive a UI restart; Ctrl+Shift+M fills the tab with the focused pane
  ("Pane zoomed" notice), again shows all panes; Ctrl+Shift+R (or double-click a tab) edits its
  title inline — Enter / clicking elsewhere saves, Esc cancels — and the new title survives a
  restart.
- [ ] (T2.8) Click "⋯" on the active workspace (or right-click a workspace row): the menu
  drops down below the row. "Open in Zed" (or Ctrl+Shift+Z) opens the folder in Zed; with
  `zed` off PATH the sidebar shows the "not found … Install Zed" hint instead. Pick another
  default agent: ✓ moves, and "+ Agent" / Ctrl+Shift+A (new agent tab) now starts that agent; survives a
  restart.
- [ ] (T2.9) **§15 stress, automated part**: `cargo build --release -p mkrew-daemon -p
  mkrew-fakeagent` then `cargo test --release -p mkrew-ui --test stress -- --nocapture` (6
  attached floods of ~14 MB each through the UI's `Router` + `Emulator`). Here it passed 3/3:
  round trip on the UI connection p50 ≈ 0.23 ms, p95 ≈ 1.3 ms; feeding one drained batch p50
  ≈ 3.7 ms, p95 ≈ 17 ms, max ≈ 23 ms; daemon RSS 33→37 MiB, test-process RSS 128→130 MiB (flat
  once the six 10k-line scrollbacks are full); daemon CPU was ≈ 1.5 cores, after T2.9a's flood
  pacing (ADR 0002) 66–76 % of a core, now asserted < 100 % in release. `verify` runs the same
  test in debug, where only correctness and daemon memory are asserted.
- [ ] (T2.9) **§15 stress, in the real UI**: open 6 panes (3 workspaces) with the fake agent
  flood profile (`agents/fake.toml` copy with `args = ["--scenario", "<file>"]`, the file a
  `flood` step with `lines = 750000` ≈ 50 MB), run them at once in a release build
  (`cargo run --release -p mkrew-ui`): typing in a 7th shell pane stays instant, the window
  repaints smoothly, every pane ends on "Flood done.", and memory (below) stays flat.
- [ ] (T2.9) **RAM < 150 MB** (§15, M2 criterion), procedure: release build, the daemon started
  by the UI, 3 workspaces, 6 live sessions (shell + fake agent; no flood), all panes shown once,
  then idle 1 min. Read "Memory (private working set)" of `mkrew.exe` and `mkrew-daemon.exe` in
  Task Manager → Details (or `Get-Process mkrew,mkrew-daemon | Select Name,WS,PM`). Expected:
  UI < 150 MB, daemon < 50 MB. Then repeat after the 6-pane flood above: the UI keeps a 10k-line
  scrollback per attached pane (≈ 100 MB for six full ones in the headless test process alone),
  so the UI may exceed 150 MB after floods — note the numbers here if it does.
- [ ] (T2.9) **§14 design check for M2** (no automatic screenshot capture): in light and dark OS
  theme take screenshots into `docs/screenshots/` of: empty window (no workspaces), sidebar with
  workspaces + sessions in every status, a tab bar with 2 tabs, a 3-pane split (focused border,
  dimmed others), an empty tab, the find bar with matches, the inline tab rename, the workspace
  menu, the kill/hide dialog, the "⚠ … sessions running" notice, an exited pane. Self-review per
  §14: no hard-coded colors/spacing (all in `theme.rs`), hover/focus/disabled states, empty
  states with one action + shortcut, body text contrast ≥ 4.5:1, status never by color alone.
- [ ] (T2.9) **Real Claude Code** in a UI pane (`agents/claude.toml`, your keys/login): Ctrl+C,
  Ctrl+R, Esc, arrows, Shift+Tab and bracketed paste reach it; full-screen redraws are clean;
  its status in the sidebar follows working → needs input → idle.
- [ ] (T2.9) **Reopen UI**: with 6 live sessions in split tabs, close the window and start the
  UI again: same workspaces, tabs, splits and sizes; every visible pane re-attached showing the
  current screen; hidden sessions still in the sidebar.

- [ ] (M2 wrap-up) M2 tagged `m2-done` with 8 of 11 criteria unticked. All of them wait on the
  manual T2.4–T2.9 checks above (sidebar/picker, tabs/splits, persistence, §13 shortcuts +
  Ctrl+C/Ctrl+R/Esc in Claude Code, Open in Zed, §14 design check, RAM < 150 MB, stress in the
  real UI). One §10 rule is still **not implemented**: detaching panes while the window is
  minimized. It is carried over to M3 as the first task. If a manual check fails, add a fix task
  to the M3 queue.

- [ ] (T3.1) **Minimize detaches**: with a busy agent pane visible, minimize the window
  (`mk session list` / daemon log: the UI's attachment is gone), let output continue, restore:
  every visible pane is back with the current screen and the focused pane has keyboard focus.

- [ ] (T3.4) **Integration install on the real home**: `mk integration install claude` and
  `codex`, then `status` (exit 0, no PATH warning); check `~/.claude/settings.json` keeps your
  settings + a `.bak` exists; start `claude` in a Mkrew pane, submit a prompt → working, finish →
  done (Claude's `/hooks` lists the 4 `mk hook claude` entries). `uninstall` both → your files
  as before.

- [ ] (T3.5) **OS notification + click**: restart the daemon (protocol v3), spawn the fake
  agent → a toast "<project> · fake / Needs input" appears (Windows: attributed to "Windows
  PowerShell" — no own AppUserModelID until packaging, M8); a second needs_input/done of the
  same session within 30 s shows none. Click the toast (also once from Action Center) → `mk`
  or the UI receives `notification.clicked` (the UI handles it from T3.7). Linux: same with
  your notification server.

- [ ] (T3.6) **Per-session status, dark and light** (§11, §14): spawn the fake agent and a
  shell in split panes. Each pane header shows icon, agent, title and `working · Ns` with
  three dots taking turns, then `needs input` with an amber pane border; the tab label shows the
  most urgent state's icon (needs input over done over working); the sidebar row text matches;
  the workspace row shows the working count and `?`. Hover the status in a pane header and in
  the sidebar → "<state> · from hook|screen|daemon · Ns ago". Idle shows `idle · Nm`; an
  exited session shows `exited (code)`. Check contrast and that the header does not crowd the
  terminal.

- [ ] (T3.7) **Inbox and notification click** (§10, §11): with two workspaces, let the fake
  agent ask in workspace A while B is shown → the Inbox lists `A: fake` with `?  needs input`;
  click it → A is shown with that pane focused. Answer and let it finish while B is shown →
  `✓ done` entry stays until you focus that pane, then leaves. With nothing pending the Inbox
  reads "Nothing needs you right now.". Minimize Mkrew, trigger needs_input, click the OS
  notification → the window is restored, in front, on that pane. Windows: check it really comes
  to the front and not only flashes in the taskbar (the click reaches the daemon, a background
  process; if it only flashes, the daemon should call `AllowSetForegroundWindow` for the UI
  process before sending `notification.clicked`).

- [ ] (T3.9) **M3 criteria that need real agents and eyes.** Do these after the T3.4–T3.7 checks
  above; tick the M3 criterion in `docs/PROGRESS.md` (or §18) once its check passes, else add a fix
  task. Restart the daemon first (protocol v3) with `mk` on PATH and `mk integration install
  claude` done.
  - **Claude approval → needs_input in ≤ 2 s** (criterion 2): in a Mkrew pane run `claude` and ask
    for something that needs a tool approval (e.g. "create a file hello.txt"). From the moment
    the approval prompt appears: within 2 s the pane header shows `? needs input` (hover: "from
    hook" or "from screen" — the screen pattern may win the race; note which, both pass), the
    sidebar Inbox lists the session, and one OS toast appears. Approve → `working` (from screen:
    no hook fires on approval), turn ends → `done` (from hook). If it takes longer, note the time and whether the first `mk hook` run
    was the slow part (first run can take seconds).
  - **Notification click focuses the pane** (criterion 3): the T3.7 click check, but with a real
    Claude approval toast and Mkrew minimized behind another app.
  - **Status on header, tab and sidebar** (criterion 4): the T3.6 check with real Claude in one
    pane and a shell in another, dark and light OS theme (§11, §14 design check); also
    `Ctrl+Shift+I` cycles through Inbox entries (T3.7a).
  - **Real omp working ↔ idle** (criterion 5): run `omp` in a Mkrew pane (`agents/omp.toml`,
    screen mode), send a prompt → `working` (hover: "from screen") while it streams, `idle`
    ≈ 3 s after output stops; send another prompt → `working` again. If it flips to idle while
    omp is still thinking or never leaves working (e.g. a blinking cursor/clock redraws), note
    what the screen shows; the next session tunes `idle_after_ms` / patterns (see the T3.8 note under "Questions with assumed defaults").

- [ ] (T4.7) **Task dialog.** Restart the daemon and app, open a workspace's Board
  (Ctrl+Shift+1). Empty board → "New task" button; Ctrl+Shift+N opens the dialog from anywhere
  (also from Terminals, switching to the Board). Type a title, Tab to the prompt, type several
  lines past the field height (it should scroll with the caret), paste with Ctrl+V, pick an
  agent chip, Enter in the title or Ctrl+Enter saves → card appears in Todo. Hover a card →
  "Edit" appears; change fields → Save updates the card; `mk task list` shows the change.
  Delete → native confirm → card disappears; delete a task whose session still runs → error
  in the dialog. Esc cancels. Check AltGr characters (e.g. `{ [ ] }` on a German layout) type
  into the fields. Dark + light theme screenshots for the §14 design check.

- [ ] (T4.8) **Board drag & drop.** With 3+ cards in Todo: drag one between two others → the
  accent line shows in the gap where it will land; drop → order changes. Drag to the end of
  Todo, into an empty column (free space under "No tasks") and to Review/Done → the card moves
  and `mk task list` shows the new status. Restart the app → order and columns persist. Check
  the drag preview follows the pointer smoothly, Edit still opens on click, and nothing moves
  when dropping a card back on its own place. Dark + light screenshots for §14.

- [ ] (T4.9) **Card actions.** On the Board: Run a Todo card (button, and click + Ctrl+Enter)
  → it moves to Running with the right agent; click it → the Terminals view shows its pane.
  Back on the Board, click its follow-up field, type a line, Enter → the agent receives it
  (check in its pane); Esc empties the field. A needs-input card takes a follow-up too. When
  it reaches Review, Approve → Done. Check that dragging a card does not also open its
  terminal, that Ctrl+Enter inside a terminal still reaches the agent, and AltGr characters
  type into the follow-up field. Dark + light screenshots for §14.

- [ ] (T4.10) **M4 wrap-up — the unticked M4 criteria.** `mk task` ↔ UI sharing is covered by
  `crates/mkrew-ui/tests/board_shared.rs`; the rest needs eyes or real agents:
  - **Create/edit/delete, drag, order persists** (criterion 1): the T4.7 + T4.8 checks above.
  - **Real agents with `arg` prompts** (criterion 2): give `agents/claude.toml` and
    `agents/codex.toml` `prompt_mode = "arg"` (or a copy), set each as a task's agent, prompt
    `fix "a&b" 100%`, Run from the Board → the session starts in the project folder (check
    `pwd`/the agent's cwd) and the agent receives the prompt verbatim (npm `.cmd` shims on
    Windows: T4.4). Also run a task with no agent → the workspace's default agent is used, and
    one with `prompt_mode = "type"` → the prompt is typed once the agent is ready.
  - **Card opens its terminal / follow-up** (criteria 4, 5): the T4.9 check above, with real
    Claude (follow-up while it works, answer at a permission prompt from the card).
  - **Board drag feel + §14 design check**: drag preview smoothness, drop line, card density,
    dark + light screenshots of a board with cards in every column (incl. an error row).

- [ ] (T5.4) **Undo task dialog.** In a git repo workspace run a task that edits a tracked file,
  adds a new file and `git add`s another; once it is in Review (close its terminal), hover the
  card → "Undo" → the dialog lists the files (M restored / A deleted / ? deleted) and a colored,
  scrollable diff; with the terminal still open the confirm is disabled with "Close the task's
  terminal first."; "Discard changes" restores the folder and closes the dialog. Edit a file
  after opening the dialog, then confirm → refused ("preview again"), the error shows and the
  preview reloads. A task whose run changed nothing → one sentence + "Close". Check a long diff
  (> 2000 lines → "cut short" note) scrolls smoothly, long paths ellipsize, Esc closes, and
  dark + light screenshots for §14 (destructive button legibility, hover).

- [ ] (T5.7) **Conflict badges.** Run two tasks in one workspace whose agents edit the same file
  within 60 s (e.g. with Claude hooks installed, or one working at a time): both cards on the
  Board and both pane headers show "⚠ Conflict: <path>", the tooltip names the file(s); hover
  the badge → "Dismiss" clears it on both cards and panes; without dismissing, approving/moving
  one card to Review clears that card only. Long paths ellipsize; the ⚠ renders as a monochrome
  amber glyph (not a color emoji); hovering Dismiss shows its own tooltip; dark + light
  screenshots §14.

- [ ] (T5.9) **History viewer.** Run a fake/real agent session to exit, `mk daemon stop`, start
  the app: Terminals tab bar → "History" opens the dialog listing the workspace's finished
  sessions newest first (title/agent, "exited (code) · ran … · … ago"); the newest log replays
  in the right pane (colors, last screen, mouse-wheel scrollback, Ctrl+Shift+F find,
  Ctrl+Shift+C copy of a selection); ↑ / ↓ and clicks pick other sessions; Esc / Close returns
  the keyboard to the focused pane. A log > 2 MB shows the "Older output is left out" note.
  Empty workspace → "No finished sessions in this workspace yet." Dark + light screenshots §14.

- [ ] (T5.11) **M5 wrap-up — the unticked M5 criteria.** Watcher ignores and daily backups are
  ticked (automated: `file_watcher_records_changed_files_of_live_sessions_except_ignored_ones`,
  backup unit tests + `startup_marks_stale_sessions_exited`). Restart the daemon first
  (protocol v8), with `mk` on PATH and `mk integration install claude` / `codex` done (the
  native session id comes from hooks).
  - **Real Claude/Codex resume across a daemon restart** (criterion 1): run a Board task with
    `claude` and one with `codex` (`agents/claude.toml` `resume = "claude --resume {session_id}"`,
    `agents/codex.toml` `resume = "codex resume {session_id}"`); after the first turn, check a
    hook arrived (the pane header status hover says "from hook"; `mk session list` does not
    print the native id). `mk daemon stop`
    while both are running/needs_input, start the app → each task gets a new session in the
    same tab slot, and the agent shows the SAME conversation (scroll back: the earlier prompt
    and answer). Send a follow-up → it continues with that context. A task whose agent has no
    `resume` (e.g. `shell`) → needs input + error row. Also check on Windows that the npm
    `.cmd` shim launch passes the resume argv (session id) through unchanged, and note if Codex's `resume` subcommand
    syntax differs in the installed version.
  - **Undo shows the diff and needs confirmation** (criterion 2): the T5.4 check above (backend
    already covered by daemon tests + `mk task undo` without `--yes` only previewing).
  - **UI warns on both cards** (criterion 3): the T5.7 check above, with two real Claude tasks
    (hooks installed) editing one file within 60 s; also note false positives from the
    "single working session" watcher heuristic (T5.6 note below).
  - **History viewable after a restart** (criterion 5): the T5.9 check above, with a real
    Claude session's log (alternate screen, colors, wide glyphs).
  - **§14 design check**: dark + light screenshots of the Undo dialog, a conflict badge on a
    card and a pane header, and the History dialog.
- [ ] (T6.8) **Docs view** (logic covered by `docs.rs` unit tests + `tests/docs_view.rs`
  against the real daemon; rendering never seen): add a workspace with a `PLAN.md` using
  headings, nested lists, `- [ ]`/`- [x]` items, a table with `:-:`/`--:` columns, a fenced
  code block, a quote, a link, inline `code`, **bold**/_italic_/~~strike~~ and a rule.
  Ctrl+Shift+3 (or the "Docs" tab) → files listed (root `*.md`, `docs/**`), folder shown
  under the name, "N open" badge after creating a comment (`mk comment` has no create — use
  T6.9 or raw IPC). Check every block renders legibly in dark + light; Ctrl+Shift+E and the
  Preview/Source toggle switch to numbered source lines (long lines are not wrapped — check
  they stay readable; scroll a 5,000-line file smoothly). Edit the file in Zed → preview
  updates within ~1 s; delete it → "This file was deleted."; "Open in Zed" / Ctrl+Shift+Z
  opens the file (without `zed` installed → the install hint shows under the header). Empty
  workspace → "No Markdown files to review yet." + "Show terminals". Screenshots for §14.
- [ ] (T6.9) **Docs comments** (logic covered by `comments.rs` unit tests + `tests/docs_view.rs`;
  the margin was never seen): in Docs, click a paragraph (highlighted), Shift+click a later
  block → both picked; "Comment" / Ctrl+Alt+M → draft in the margin with "Lines a–b"; type,
  Enter → thread appears, the file's "N open" badge goes up. Same in Source view by clicking
  line numbers/lines. Hover a thread → Reply / Resolve; reply with Enter; Resolve → "✓
  Resolved" (dimmed), Reopen brings it back. Esc drops a draft. Edit the passage away in Zed →
  "⚠ Outdated"; `mk comment reply` from a session → reply shows live with "↩ Addressed".
  On a layout where Ctrl+Alt+M is AltGr+M (e.g. German "µ"), typing in a draft still inserts
  the character. Dark + light screenshots for §14.
- [ ] (T6.10) **Docs "Send to agent" / "View changes"** (logic covered by `doc_dialog.rs` unit
  tests; daemon delivery by the T6.5/T6.6 e2e; the dialogs were never seen): with 2 open
  comments on a plan an agent wrote, the header's "Send to agent" tooltip says "Send 2 open
  comments"; Ctrl+Shift+S opens the picker with that agent preselected ("edited this file"),
  ↑ / ↓ move, Enter sends → dialog closes, threads show "↑ Sent", the prompt is typed into
  the session (queued while it works). Select one thread's lines first → only that one goes.
  No live agent → "No agent is running…" + Close. After the agent edits the file, hover the
  thread → "View changes" shows the diff (−/+ rows, hunk band); unchanged file → "Nothing
  changed…". Dark + light screenshots for §14.
- [ ] (T7.7) **Files view** (logic covered by `files_view.rs` / `highlight.rs` unit tests;
  daemon side by `fs_list_read_and_find_follow_file_changes`; the view was never seen):
  Ctrl+Shift+4 (or the "Files" tab) → tree of the workspace, folders first, ▸/▾ chevrons,
  M/A/D/? letters on changed files (tooltip names them), a dot on folders holding changes.
  ↑ ↓ move the focus-ringed cursor (the list scrolls along), → opens a folder / goes to its
  first child, ← closes / goes to the parent, Enter or click opens a file. Ctrl+Shift+H and
  the "Show ignored" button show `target/`, `node_modules/` (muted names). Have an agent
  create, edit, rename and delete files → the tree and the opened file update within ~1 s.
  Open a `.rs`, `.ts`, `.py`, `.toml`, `.md` file → line numbers + syntax colors readable in
  dark and light (TOML has no bundled syntax → plain); click a line → raised band + accent
  number + "Line N"; "Open in Zed" / Ctrl+Shift+Z opens Zed at that line. A PNG → "This is a
  binary file." + size + "Open in Zed"; a > 2 MB log → "too large" notice; a 1 MB text file
  shows plain and scrolls smoothly. A `.md` → "Review in Docs" switches to Docs with it
  selected. Dark + light screenshots for §14.
- [ ] (T7.8) **Quick open** (logic covered by `quick_open.rs` / `files_view.rs` `reveal` unit
  tests; speed by the daemon `fs.find` perf test; the finder was never seen): Ctrl+Shift+O →
  "Open file · <workspace>" dialog listing files; type `mainrs` → fuzzy matches with matched
  chars in the accent color, the best one ringed; ↑ ↓ move (wraps, list scrolls along), Enter
  or a click opens it in Files with its folders opened and the cursor on it; Esc closes and
  the pane gets the keyboard back. Ctrl+V pastes into the query. In a 10,000-file repo typing
  feels instant. Dark + light screenshots for §14.
- [ ] (T7.9a) **Git view** (logic covered by `git_view.rs` unit tests; daemon `git.status`/
  `git.diff` by daemon e2e; the view was never seen): Ctrl+Shift+5 or the "Git" entry in the
  view rail (superseded by ADR 0027: the toolbar's "Git" segment) → branch name, "→ origin/main  ↑2 ↓1" (or "No upstream"), Working changes /
  Against base segments, "Base: main (auto-detected)". Changed files grouped Staged /
  Unstaged / Untracked with change letters and +/− counts; the first is selected and its diff
  shows: hunk headers on `diff-hunk-header`, added/deleted lines in the diff colors with old /
  new line numbers, "⋯ N unchanged lines" between hunks. ↑ ↓ / j k move files (list scrolls
  along), n / p move hunks (▸ marker, "Hunk 2 of 5", diff scrolls to it), b toggles the mode.
  Have an agent edit / create / delete files → list and diff update within ~1 s. A binary file
  or > 1 MB diff → notice + "Open in Zed". Clean repo → "No uncommitted changes." + "Against
  base" button. Group-title rows and file rows are the same height (scrolling a long list
  lands on the right file). Dark + light screenshots for §14.
- [ ] (T7.9b) **Git stage / unstage / discard** (logic covered by `git_view.rs` unit tests;
  daemon `git.stage/unstage/discard` by daemon e2e; the UI was never seen): in Git, hover an
  Unstaged file → "Stage" appears (always on the selected file), a click moves it to Staged;
  `s` / `u` stage / unstage the selected file, or after `n` / `p` only the current hunk of a
  modified file; "Stage hunk" / "Unstage hunk" on hunk headers do the same; a new/untracked
  file stages by path. "Discard…" on an Unstaged/Untracked file → "Discard changes?" dialog
  shows the file's diff, confirm is disabled until it loads, Esc/Cancel keep the file, the red
  confirm discards it (untracked file deleted). A failed stage shows "✕ Staging failed: …".
  Row heights stay uniform with the hover action. Dark + light screenshots for §14.
- [ ] (T7.9c) **Git base menu + Git keys** (logic covered by `base_dialog.rs`/`keymap.rs` unit
  tests; daemon `git.set_base` by e2e; the UI was never seen): "Change…" next to "Base: …" in
  the Git header opens "Base branch" with the current base, the field prefilled when set by
  hand (empty when auto-detected); type `develop` + Enter → header "Base: develop", Against
  base re-lists; "Auto-detect" → "Base: main (auto-detected)"; an invalid ref (`a..b`) shows
  "✕ …" and keeps the dialog; Esc/Cancel keep the base; typing b/n/p/s/u in the field types
  them. A repo with no main/master/origin → Against base shows "No base branch found." with
  a primary "Set base…". With the file list focused, b/n/p/s/u still work and their tooltips
  show the keys; rebinding e.g. `git_next_hunk = "alt-n"` in keymap.toml works. Screenshots
  dark + light for §14.
- [ ] (T7.9d) **Git diff syntax highlighting** (spans covered by `highlight.rs`/`git_view.rs`
  unit tests; the UI was never seen): open a `.rs`/`.ts` change in the Git view → keywords,
  strings, comments, numbers colored on context, added and deleted lines (on the green/red
  line backgrounds; check contrast dark + light); a deleted line inside a removed block
  comment stays comment-colored; a big diff (thousands of lines) opens without freezing the
  UI; an unknown file type (`LICENSE`) stays plain; the discard dialog's preview is colored
  too.
- [ ] (T7.10a) **Git commit box, Fetch / Push** (state covered by `git_actions.rs` unit tests;
  the UI was never seen): stage a file, press `c`, type a 2-line message (Enter breaks the line, typing
  `s`/`u`/`b` only types), Ctrl+Enter commits → box empties, list + ahead count refresh; with
  nothing staged the button is faded; a failing pre-commit hook shows its error under the box.
  Fetch on a repo with a real remote → "Fetching…", then behind/base update; Push → ahead goes
  to 0; a rejected push shows git's message wrapped under the header; Push is faded on a
  branch without upstream. Check the layout (box at the bottom of the left column, long
  messages scroll at ~8 lines) in dark + light.
- [ ] (T7.10b) **Git History** (state covered by `git_view.rs` / `git_history.rs` unit tests;
  the UI was never seen): press `h` or click "History" → the last 50 commits, the newest
  expanded with its files and the first file's diff; click another commit / file; j k walk
  across commits; hover a commit → full message + author; commit in a terminal → it appears
  on top while the selection stays; `b` / `h` go back. Check the 3 header segments fit
  without cutting (Windows font), row heights stay uniform, dark + light.
- [ ] (T7.10c) **Task "View changes"** (model covered by `git_view.rs` unit tests, daemon
  `since` by `git_view`/`daemon.rs` tests; the UI was never seen). Restart the daemon first
  (protocol v13). Run a task in a git workspace, let the agent edit + commit something, card in
  Review → hover → "Changes" → the Git view lists everything since the checkpoint (committed,
  uncommitted, untracked) with "Since abc1234 · task: …" and "Close"; no segment is active
  then — check that reads clearly; `h` and back returns to the task's list; `b` / Close → Working.
  Check the card footer (Changes / Undo / Approve) at the narrowest column width, dark + light.
- [ ] (T7.11b) **Diff-line comments in the Git view** (model covered by `git_view.rs` /
  `comments.rs` unit tests, the backend by `fake_agent_fixes_code_for_diff_comments_and_replies`;
  the UI was never seen). Restart the daemon first (protocol v14). In a task's "Changes", click a
  `+` line, Shift+click a later one → accent bar + accent numbers; Ctrl+Alt+M (or "Comment") →
  margin opens with the draft; Enter saves; reply / resolve / reopen; Ctrl+Shift+S → the task's
  session is offered first ("ran this task"); send to a real Claude / Codex → it fixes the code
  and `mk comment reply` shows "↩ Addressed" live. Check the margin + diff width at 1280 px,
  that typing in the draft never stages (`s`), and that Docs never lists the diff threads. Dark
  + light.
- [ ] (T7.12) **M7 wrap-up — the unticked M7 criteria.** Two criteria are ticked (automated:
  real-`git` integration tests on `tempfile` repos; `GitBackend` + the 10,000-file status perf
  test `git_view::tests::status_of_a_10000_file_repo_meets_the_budget`). The backend of every
  other criterion is covered by daemon/CLI e2e (`fs_list_read_and_find_follow_file_changes`,
  `git_set_base_is_announced_persists_and_null_means_auto`,
  `git_status_and_diff_follow_the_project_base_and_reject_bad_input`,
  `git_stage_unstage_discard_and_commit_with_git_changed_events`,
  `git_log_show_push_and_fetch_with_a_local_remote`, `fake_agent_fixes_code_for_diff_comments_and_replies`,
  CLI `project_update_sets_and_clears_the_git_base`), the `fs.find` perf test
  `files::tests::find_over_10000_files_meets_the_budget` and the UI model unit tests; no M7
  view was ever seen. Restart the daemon first (protocol v14), with `mk` on PATH.
  - **Files view** (criterion 1): the T7.7 check above.
  - **Quick open < 100 ms** (criterion 2): the T7.8 check above, in a real 10,000-file repo
    (e.g. a Zed or rust-lang checkout); typing must feel instant end to end (daemon + UI).
  - **Git header + grouping** (criterion 3): the T7.9a check above.
  - **Against base** (criterion 4): on a feature branch of a repo with `origin/HEAD`, Against
    base lists only the branch's own changes (compare with `git diff $(git merge-base HEAD
    origin/main)`); delete `origin/HEAD` → falls back to `main`, then `master`; the T7.9c base
    menu and `mk project update <p> --base develop` / `--auto-base` both change the header.
  - **Unified diff, stage/unstage, discard** (criterion 5): the T7.9a, T7.9b and T7.9d checks.
  - **Commit, push, fetch** (criterion 6): the T7.10a check against a **real remote** (GitHub
    or similar, your credentials): Fetch brings new upstream commits (behind count), Push sends
    the current branch to its upstream only; a diverged branch → push is rejected with git's
    message, never forced (check the remote's history afterwards).
  - **History** (criterion 7): the T7.10b check above in a repo with > 50 commits (exactly 50
    listed, per-commit diffs open).
  - **Task "View changes"** (criterion 8): the T7.10c check above.
  - **Diff-line comments, real agent** (criterion 9; the fake-agent e2e passes): the T7.11b
    check with real Claude **and** Codex — the code prompt arrives as one message, the agent
    fixes the code (not just answers) and runs `mk comment reply c_… "…"` per comment. If it
    does not, note what the §12 code prompt is missing.
  - **§14 design check**: dark + light screenshots of Files (tree + highlighted viewer), Quick
    open, Git (Working / Against base / History / task changes, commit box, margin threads),
    the base and discard dialogs.
- [ ] (T6.11) **M6 wrap-up — the unticked M6 criteria.** The fake-agent E2E criterion is ticked
  (automated: `fake_agent_addresses_sent_comments_and_replies_to_each`). The backend of every
  other criterion is covered by daemon/CLI e2e (`docs_are_listed_read_and_external_edits_reanchor_comments`,
  `comment_threads_reply_resolve_reopen_and_agent_replies_address_them`,
  `comments_sent_to_a_working_agent_are_typed_once_it_asks`, `comment_list_and_reply_from_inside_a_session`)
  and `crates/mkrew-ui/tests/docs_view.rs`; what is left needs eyes or real agents. Restart the
  daemon first (protocol v11), with `mk` on PATH.
  - **Docs list + preview** (criterion 1): the T6.8 check above; also have an agent create a
    `.md` outside the review globs (e.g. `notes/x.md`) → it is listed (recently changed).
  - **Comment on a selection, threads** (criterion 2): the T6.9 check above; restart the daemon
    and the app → threads and statuses are still there.
  - **External edits in Zed** (criterion 3): edit the commented passage in Zed (save) → preview
    refreshes, a comment above a newly inserted paragraph moves down with its passage, a
    deleted passage → "⚠ Outdated"; undo in Zed → back to open.
  - **Real Claude/Codex review round trip** (criteria 4, 5): Claude writes `PLAN.md` from a
    Board task; add 2 comments; "Send to agent" while Claude is still working → the prompt is
    typed only once it stops, arrives as ONE pasted message with its line breaks (T6.5/T6.6
    notes below: bracketed paste through ConPTY), Claude edits the plan and runs
    `mk comment reply c_… "…"` per comment → replies appear live, threads show "↩ Addressed".
    Repeat with Codex. If the agent does not know `mk`, note what the §12 prompt is missing.
  - **View changes** (criterion 6): the T6.10 check above, on the threads Claude just addressed.
  - **§14 design check**: dark + light screenshots of the Docs view (preview with margin
    threads, source view), the Send-to-agent picker and the View-changes dialog.
- [ ] (T8.3) **Dev server UI.** — superseded by the 2026-10-07 Run button section (ADR 0018).
  Restart the daemon (protocol v16). Workspace menu (⋯ /
  right-click) → "Dev server settings…": type `npm run dev` (or `python -m http.server`), Tab
  flips "Start it when the daemon starts", Enter saves (Esc keeps). The tab bar then shows
  "Start dev server" (tooltip = the command); clicking it switches to Terminals and shows the
  tab labelled "Dev server" with the command running; the sidebar row shows "▶ dev"; the
  button and the menu item become "Stop dev server" → the pane shows it exited, "▶ dev" goes
  away. Start again → the same tab is reused. Clear the command → no Start button. With
  auto-start on: `mk daemon stop`, start the app → the dev server runs. Check the dialog and
  the indicator in dark + light (§14).
- [ ] (T8.5) **Command palette.** Ctrl+Shift+P opens "Command palette"; typing filters
  (matched chars in accent, shortcuts muted on the right), ↑↓ / click pick, Enter runs, Esc
  closes and the keyboard is back where it was. Check: in a terminal pane "Terminal: Copy" /
  "Find in scrollback" act on that pane; in Git (list focused) "Git: Stage…", "Git: Fetch",
  "Git: Push", "Git: Discard changes to selected file…" (opens the confirmation); on the board
  with a Review card selected "Task: View changes of selected task"; "Dev server: Start/Stop"
  with a dev command set (— superseded by the 2026-10-07 Run button section (ADR 0018));
  "Go to workspace / session / task" switch to it (a task selects its
  card on the board). Dark + light screenshots (§14).
- [ ] (T8.6) **Keymap settings.** With no `<config>/mkrew/keymap.toml`, Ctrl+, (and the palette's
  "Global: Open keymap settings") creates it with every command commented at its default and
  opens it in Zed; a second Ctrl+, opens the same file unchanged. Uncomment
  `close_pane = "ctrl-shift-k"` (taken by add_workspace), add `nope = "ctrl-1"` and
  `find = ""`, restart: the sidebar shows the first error (+2 more), Ctrl+Shift+P lists three
  "Keymap error: …" rows at the top, picking one opens the file; Ctrl+Shift+X still closes a
  pane. Without Zed on `PATH` the error shows in the sidebar.
- [ ] (T8.7) **Windows installer.** Never built here (no WiX/.NET; the test only string-checks
  `packaging/mkrew.wxs`). Run the Release workflow by hand (`workflow_dispatch`) or locally:
  `dotnet tool install --global wix --version 7.0.0`, `pwsh scripts/package.ps1 -AcceptWixEula`.
  On a clean Windows: `msiexec /i dist\mkrew-<v>-x64.msi` installs `Program Files\Mkrew\bin\
  {mkrew,mk,mkrew-daemon}.exe` + `agents\*.toml`, adds `bin` to the system PATH (new terminal:
  `mk --help`), adds a Start Menu "Mkrew" shortcut that opens the app with no console window;
  installing a newer version replaces it; uninstall removes files, shortcut and PATH entry.
- [ ] (T8.7) **tar.gz.** Built and listed only with fake binaries (test). Check the Release
  workflow artifacts on Linux and macOS (macOS is not in CI, so gpui may not build there):
  unpack, `bin/mk --help`, `bin/mkrew` starts and spawns `bin/mkrew-daemon`.
- [ ] (T8.8) **README walkthrough.** Follow README "Install" → "Configure agents" → "Uninstall"
  once on a clean machine (MSI on Windows, tar.gz on Linux) and fix any step that does not match.
- [ ] (T8.9) **§15 measurements not taken here** (ADR 0009): run `bash scripts/measure.sh` /
  `pwsh scripts/measure.ps1` once yourself (the headless session ran the three tests they call,
  not the scripts) and on Linux; measure key press → glyph on screen (< 16 ms, needs eyes or a
  screen recorder: `scripts/measure-spike.sh`'s `--probe-keys` only covers the M0 spike); the
  50 MB-per-pane flood is the T2.9 check above. Update the ADR 0009 / README tables if the
  numbers differ.
- [ ] (T8.10) **M8 wrap-up — the unticked M8 criteria.** The README criterion is ticked (written,
  walkthrough above still open). The backend of every other criterion is covered by automated
  tests (`project_dev_server_settings_are_added_updated_and_cleared`,
  `dev_server_runs_once_in_its_own_tab_stops_and_auto_starts_after_a_restart`, CLI
  `project_dev_server_is_configured_on_add_and_update`, `keymap::tests::{template_parses_to_the_defaults_commented_or_not,
  edited_template_reports_a_duplicate, ensure_file_creates_the_template_once}`, `palette::tests`, `crates/mkrew-cli/tests/packaging.rs` and `crates/mkrew-ui/tests/measure.rs`); no
  M8 view was ever seen and no installer was ever built. Restart the daemon first (protocol v16),
  with `mk` on PATH.
  - **Command palette + keymap** (criterion 1): the T8.5 and T8.6 checks above.
  - **Dev server** (criterion 2): the T8.3 check above, with a real dev command (`npm run dev`
    or similar) — it keeps running while you switch views, and auto-start works after a reboot.
  - **Installer + tar.gz** (criterion 3): the two T8.7 checks above — the MSI on a **clean**
    Windows (VM or Windows Sandbox), the tar.gz on Linux.
  - **§15 checks** (criterion 5): the T8.9 check above (key → glyph latency is the only row
    not measured).
  - **§14 design check**: dark + light screenshots of the command palette (with keymap error
    rows), the dev server settings dialog, the tab-bar Start/Stop button and the sidebar "▶ dev".
- [ ] (T9.11) **M9 wrap-up — the unticked M9 criteria.** The settings criterion is ticked. The
  backend of every other criterion is covered by automated tests (daemon e2e
  `session_read_shows_the_prompt_line_and_the_answer_goes_through`,
  `a_turn_that_edits_two_files_reports_both`, `settings_are_reread_and_errors_reported_in_status`;
  `notify::tests::switches_and_sound_follow_the_settings_file` + `notify::decide` unit tests;
  `awake::tests::{held_while_an_agent_works_and_released_five_seconds_after_the_last_stops,
  working_again_within_the_linger_keeps_it_held}`; UI `peek`, `broadcast`, `turn`, `inbox` unit
  tests + snapshots); no M9 view was ever seen, no sound heard, no real inhibitor checked. Restart
  the daemon first (protocol v17), with `mk` on PATH. Details of each check: the "Questions with
  assumed defaults" entries T9.6–T9.10.
  - **Peek + quick reply** (criterion 2): fake agent `ask` scenario → hover its Inbox row and its
    sidebar row 300 ms → popover with the last lines, state + age; click `y` → the agent goes on
    without leaving the view. Then Ctrl+Shift+Space from a focused terminal, ↑/↓, type a reply +
    Enter, Esc → the terminal has the keyboard again. Repeat once with a real Claude permission
    prompt (numbered choices, not y/n: answer through the typed input, e.g. `1` + Enter).
  - **Turn summary** (criterion 3): fake agent `edit_files` with 2 paths (or Claude editing 2
    files in one prompt) → Inbox row and pane header show `done · 2 files · …`.
  - **Sound** (criterion 4): `notify.sound = true` → a needs_input toast plays the sound on
    Windows (macOS/Linux if available); `sound = false` → silent; `notify.done = false` → no toast
    on done; `notify.needs_input = false` → no toast on needs_input.
  - **Keep awake** (criterion 5): while an agent works, `powercfg /requests` (admin) lists
    `mkrew-daemon` under SYSTEM; gone ~5 s after it stops; a shell running a command never shows.
    `cargo clippy --workspace --all-targets -- -D warnings` on Linux and macOS (backends never
    compiled there); on Linux `systemd-inhibit --list` shows it while an agent works.
  - **Broadcast** (criterion 6): two real agents (Claude + Codex, one working, one idle) →
    palette "Agent: Broadcast to agents…" → both checked → type + Enter → each agent gets the text
    once, rows show "queued" / "sent"; kill one and send again → "failed: …".
  - **Inbox last line** (superseded by ADR 0034/0036) (criterion 7): an agent waiting in the Inbox shows its last non-blank
    screen line (e.g. the `(y/n)` prompt) under its name.
  - **§14 design check**: dark + light screenshots of the peek popover (needs_input with `y`/`n`
    and a plain one), the broadcast dialog after a send with mixed results, an Inbox row with last
    line + turn summary, and a pane header with the summary.
- [ ] (T10.10) **M10 wrap-up — the unticked M10 criteria.** The send queue criterion is ticked.
  The backend of every other criterion is covered by automated tests (daemon e2e
  `a_queued_text_removed_while_working_is_never_typed`,
  `attachments_are_saved_per_session_and_bad_ones_rejected`; `attachment::tests::cleanup_*`;
  core `prompt::tests` (seed + render); UI `composer::tests` (`file_ref`, `selection_prefill`,
  `typed_paths`, `attachment_ext`, drafts), `lifecycle::tests::run_with_updates_the_agent_then_runs`,
  `board::tests::run_with_is_offered_on_todo_and_review_cards_without_a_live_session`); no M10
  view was ever seen and no real agent received a composer send. Restart the daemon first
  (protocol v19), with `mk` on PATH. Details of each check: the "Questions with assumed defaults"
  entries T10.2–T10.8.
  - **Composer feel** (criterion 1): Claude pane → Ctrl+Shift+Enter → type 3 lines (Enter) →
    Ctrl+Enter → Claude receives one paste (one prompt, not 3) + Enter. Type a draft, Esc, focus
    another pane and come back, Ctrl+Shift+Enter → draft intact. Send while Claude works → the
    text is listed with ✕, header `⧗ 1 queued`, typed once Claude is idle.
  - **`@` picker** (criterion 1): in the composer type `@` → quick-open over the workspace → pick
    a file with a space in its path → `@"dir/my file.md"` typed; Esc on the picker types `@`;
    `composer.file_ref = "{path}"` in `settings.toml` → the next pick types the bare path.
  - **Prompts** (criterion 3): delete/rename `<config>/mkrew/prompts/` → the next pick re-seeds
    the 3 samples; insert one from the palette ("Insert prompt…"), the composer ("Prompts…") and
    the task dialog ("From prompt…"); a prompt with `{file}` while no file is open shows the
    muted "left empty" note.
  - **Send selection** (criterion 4): Ctrl+Shift+L with a picked line in Files, picked new-side
    lines in Git and selected text in a terminal → the dialog defaults to the most recently
    active agent → Enter → that session's composer opens prefilled (`` `path:a-b` `` for
    Files/Git, then the fenced text).
  - **Drop/paste** (criterion 5): the T10.8 entry below (Explorer drop, Win+Shift+S +
    Ctrl+Shift+V, browser image); after a session has ended > 7 days ago (or set its `ended_at`
    back in the DB) its `<data>/mkrew/attachments/<session>/` dir is gone within an hour.
  - **Run with…** (criterion 6): hover a Todo card → "Run with…" → pick Codex → the card's agent
    becomes Codex and a Codex session starts; on a Review card with no live session too; a card
    with a live session shows no button.
  - **§14 design check**: dark + light screenshots of the docked composer (draft + 2 queued
    rows), the prompt picker with preview + "left empty" note, the send-selection dialog, the
    task dialog multi-line prompt, and the "Run with…" picker anchored on a card.
- [ ] (T11.9) **M11 wrap-up — the unticked M11 criterion and the M11 views.** Four of five
  criteria are ticked; their backend is covered by automated tests (snapshot real-git +
  `snapshot_of_a_10000_file_repo_meets_the_budget` (release only), daemon e2e
  `a_hook_turn_start_is_snapshotted_before_the_agent_edits`,
  `turns_are_listed_previewed_and_restored`,
  `hook_edits_are_announced_attributed_and_diffed_since_a_turn`; UI `turns_dialog::tests`,
  `follow::tests`, `git_view` since/changed-by tests + snapshots). No M11 view was ever seen and
  no real agent ran a turn. Restart the daemon first (protocol v21), with `mk` on PATH and `zed`
  on PATH. Details: the "Questions with assumed defaults" entries T11.1–T11.7.
  - **Turns + restore** (criterion 2): Claude in a git project, two prompts that each edit a
    file (one prompt also creates a new file) → pane header "Turns…" lists both newest first
    with time, files and +/−; "Restore before turn 2" → preview lists the changed file and the
    new file as deleted → confirm → files are back as before turn 2, `git status` shows HEAD,
    branch and index unchanged; while Claude works the button is disabled. Cancel in the preview
    returns to the list.
  - **Since turn / Changed by** (criterion 4): "Diff since" on a Turns row → Git view header
    `Since turn n of claude #k` with only the files changed since then (incl. untracked);
    with two agents editing different files → "Changed by" chips filter the list per session.
  - **Follow in Zed** (criterion 5, the unticked one): `Share` → "Follow in Zed" in a Claude pane header
    (and the palette command) → while Claude edits several files, Zed opens the last edited file
    ~1.5 s after the edits stop (`zed.follow_debounce_ms`), at the edited line (checks the
    undocumented `structuredPatch` line, T11.5); off again → no more opens. Without `zed` on PATH
    nothing breaks (only logged).
  - **Perf on Windows** (criterion 3): run
    `cargo test --release -p mkrew-daemon snapshot_of_a_10000_file_repo_meets_the_budget` once on
    your machine (and with a large real repo: a turn start should not feel slower).
  - **§14 design check**: dark + light screenshots of the Turns dialog (a working session with
    restore disabled, and an idle one), the restore-before-turn preview, the Git view in Since
    turn mode with the Changed by chips, and the pane header with `Share` highlighted (following).
- [ ] (T13.11) **M13 wrap-up — the M13 views with real agents.** The first four M13 criteria are
  ticked: their backend is covered by automated tests (daemon real-git tests in the worktree,
  merge and remove modules, e2e with the fake agent for worktree run/resume, `fs.*`/`git.*` on a
  worktree and `task.fanout` with 2 fake profiles; core usage readers on fixture transcripts;
  UI `usage::tests`, compare/fan-out/worktree dialog tests + snapshots). No M13 view was ever
  seen and no real agent ran in a worktree. Restart the daemon first (protocol v23), with `mk`
  on PATH. Details: the "Questions with assumed defaults" entries T13.3–T13.10b.
  - **Worktree run** (criterion 1): in a git project with an ignored `.env` listed in
    `.worktreeinclude`, create a task with "Run in a worktree" ticked → Run Claude → it works in
    `<data>/mkrew/worktrees/<project>/<task>` on branch `mkrew/<slug>-…` (card shows `⎇`),
    `.env` is copied; your checkout's branch, HEAD, `git status` and files are unchanged. Check
    the `mkrew: .worktreeinclude skipped …` banner line with an unignored entry (a full-screen
    agent may paint over it). Quit the daemon mid-task and restart → the session resumes in the
    same worktree.
  - **Files / Git on a worktree** (criterion 2): the "Showing Project | <task>" chips in Files
    and Git switch to the worktree; commit the agent's work there from the Git view.
  - **Merge / Remove** (criterion 2): "Merge back…" on the Review card lists the commits →
    Merge → `git log` on your branch shows the `--no-ff` merge. Make a conflicting change on
    both sides → "Merge aborted" + the files, your checkout untouched. Close the agent's terminal
    (removal is refused while it runs), then "Remove worktree…" with an unstaged/untracked file
    → listed, "Discard…" → removed (staged files must be unstaged or committed first); with an
    unmerged branch → "branch kept".
  - **Fan-out, Compare, Pick** (criterion 3): "Several agents…" on a Todo card → Claude + Codex
    → two child cards run in their own worktrees, the group card shows both states; once both
    are in Review → "Compare" shows files/±/run time per child → "Pick" one → merge → original
    moves to Review; close the other child's terminal → "Remove the other worktree" (plural
    with more children) removes it.
  - **Usage** (criterion 4): with the hook-enabled sample profiles (usage needs the native
    session id from a hook), after a few Claude and Codex turns, the pane header shows `ctx N%`
    (amber `⚠` from 80 %), cards `12.3k tok`, sidebar rows a total; compare the numbers with
    Claude's `/cost` / `/context` and Codex's `/status`. Check `context_window` in the sample
    profiles against your models (T13.9).
  - **Known M13 UI leftovers** (decide if any needs a fix task): no palette/keyboard for
    Compare, cancelling the Pick merge does not reopen Compare, Compare is not live; the
    worktree checkbox and fan-out picker are mouse-only; Merge/Remove are hover buttons (no "…"
    card menu); no CLI merge/remove; `turn.restore` is refused for worktree tasks; rerunning a
    task after removal with a kept branch fails (branch exists).
  - **§14 design check**: dark + light screenshots of the task dialog with the worktree
    checkbox, a card with `⎇ mkrew/…` and `tok`, the worktree target chips, the Merge back and
    Remove worktree dialogs (incl. conflict and discard steps), the fan-out picker, a group card,
    the Compare modal, and a pane header with `ctx` normal and amber.

## Questions with assumed defaults

_Decisions the agent made on its own. Override them here if you disagree._

- 2026-10-10 voice input (ADR 0045): a toggle (press, then press again) instead of
  hold-to-talk, because the UI has no key-up handling. Shortcut Ctrl+Shift+D. The key is read
  from an environment variable, never from settings.toml. The language is detected per
  recording. The text goes to whatever has the keyboard when it arrives, else to the clipboard.
  Override in keymap.toml / `[voice]`.
- (compact sidebar) **Rail details:** key Ctrl+Alt+B (Ctrl+Shift+B stays hide/show); icon
  `⇤` / `⇥` in the status bar next to `◧`; width 56 px scaled with the font size; entering the rail
  clears the filter and chip; no hover peek on the rail (the tooltip replaces it, keyboard peek
  stays); pin marks, times, `▶` and hover actions are left out (menus still reach Pin and the
  workspace actions; removing a session needs the full sidebar); Ctrl+Alt+B is AltGr+B; no
  workspaces = only `+`; not remembered across restarts. ADR 0040.
- (T13.8b) **Compare and Pick:** "Compare" is an always-visible button on a group card once every
  child is in Review (accent when selected). The modal shows one column per child: agent, branch,
  run time, files changed and +/− since the child's checkpoint (`git.status` base mode on its
  worktree; binary files count as a file with no lines), "Git view" (= the card's Changes) and
  "Pick". It is read once when opened (not live). Pick opens the normal Merge back dialog for that
  child; only a successful merge moves the original to the end of Review (a conflict moves
  nothing). The merged dialog then offers "Remove the other N worktrees" (one confirmed click, no
  second step): each other child with a worktree gets `task.remove_worktree`; a child whose
  terminal still runs is skipped with "Close its terminal first", a dirty worktree reports the
  daemon's refusal; unmerged branches are kept (`branch -d`). The children stay in Review.
- (T13.8a) **"Run with several agents…" UI:** no "…" card menu yet, so it is a hover-shown
  "Several agents…" button next to "Run with…" on Todo / Review cards (not on fan-out children,
  not on an original that already fanned out), plus the palette entry "Agent: Run selected task
  with several agents…". It shows on non-git workspaces too; the daemon's refusal appears as the
  board's card error. The picker keeps tick order (= child order), caps at 5 (other rows faded),
  and runs children at the new-pane size (80×24, panes resize them). The group card's agent line
  reads "N agents", one line per child (agent + status), and hides any run of its own from
  before the fan-out. An original can no longer be Run / Run with… from the UI. Picker has no
  keyboard support (like the M10 "Run with…" leftover).

- (T13.7) **`task.fanout` rules:** refused outside a git repo (agents would share one folder),
  for a running task, a fan-out child, a task that already fanned out, and duplicate profiles
  (two children with the same title). ` · <agent>` is the profile's `name`. Children are created
  even if some fail to start: the call errors naming the failures, the original moves to Running
  only if one started. The original can no longer be `task.run` (its children run instead), and
  startup resume skips it. `cols`/`rows` (default 120×30) added to the request (SPEC updated).
- (T13.6) **Merge back… / Remove worktree… UI:** there is no "…" card menu yet, so both are
  hover-shown quiet buttons on Review / Done cards whose worktree exists (next to Undo /
  Changes); they move into the "…" menu once it exists (M10 leftover). The merge dialog lists
  the worktree's `git.log` minus the checkout's (50 each, so a checkout > 50 commits past the
  fork may list a few shared ones) and disables Merge while the worktree has any uncommitted
  file, the checkout has staged/unstaged changes (untracked allowed, as the daemon), HEAD is
  detached, or there are no new commits; "Open Git view" goes to the worktree. The remove
  dialog lists uncommitted files; "Discard…" → "Discard files" (Esc / "Keep files" go back)
  discards unstaged + untracked ones via `git.discard`; staged changes must be unstaged or
  committed in the Git view. Results: "Merged", "Merge aborted" + conflicting files,
  "Worktree removed" (+ branch kept or deleted). Hidden card buttons still take layout space.

- (T13.5) **Merge back / Remove worktree rules:** merge is refused (`invalid_state`, files named)
  when the worktree has any uncommitted or untracked file, or the user's checkout has staged or
  unstaged changes; untracked files in the user's checkout are allowed (git itself refuses a
  merge that would overwrite one). Merge is allowed while the task's agent session is still
  live (it does not touch the worktree); removal is refused while one is live ("stop it
  first") because the agent's cwd is the worktree. Neither changes the task's status. After
  removal the task keeps `worktree` on; running it again creates a fresh worktree — which
  fails while a kept (unmerged) branch of the same name still exists (delete or rename it by
  hand). A worktree folder deleted by hand is pruned and its branch deleted with `-d`.
  `task.remove_worktree` on a dirty worktree returns `invalid_state` naming the files; the UI
  (T13.6) shows them via `git.status` on the worktree target.
- (T13.4b) **Worktree target chooser:** "Showing Project | <task title>…" chips (hidden while no
  task of the workspace has a worktree), under the Files tree header and at the top of the Git
  header; switching starts that view over on the target (the Git commit draft is kept).
  "Changes" on a worktree card and "Diff since" on a worktree task's turn switch the Git view to
  that worktree. On a worktree: "Review in Docs" is hidden (Docs are project-only), "Changed by"
  and diff comments stay project-scoped, quick open still finds project files. Chips are
  mouse-only like "Changed by" (no palette command / focus ring yet). Check that `⎇` renders
  (font fallback) on Windows in both themes.

- (T11.6) **"Turns…" in the pane header:** panes have no header menu yet, so §20C.2's "pane
  header menu Turns…" is a quiet "Turns…" button on every agent pane header (always shown, not
  hover-only) plus the palette command "Agent: Turns…". The dialog lists turns newest first
  (↑/↓, Enter restores before the selected one); "Restore before" opens the task-undo preview
  and confirm (titled "Restore before turn n?"); Cancel goes back to the list. The "Diff since"
  entry moves to T11.7, which builds the Git view's "Since turn" mode it opens.
- (T10.6) **Send selection (Ctrl+Shift+L):** the session picker is §12's "Send to agent"
  dialog in a selection mode (title `Send selection · path:a-b`, confirm "Open composer").
  Files picks only one line (§12A), so it sends that line (`` `path:n-n` ``); multi-line picks
  in Files (Shift+click) would be a follow-up. "Most recently active" = the live agent whose
  state changed last. Git sends the picked new-side lines with their indent; Files sends tab-
  expanded lines (as shown). The prefill is typed after the draft on a new line. Nothing picked
  → the key does nothing (the palette still lists it in Files/Git). `{selection}` in prompts =
  the raw picked lines of the shown view (Files or Git), not fenced.

- (T10.5) The three prompt entry points ("Insert prompt…" in the palette, "Prompts…" in the
  composer, "From prompt…" in the task dialog) open one modal picker (list + rendered preview +
  muted "left empty" note), not dropdown menus. A pick is appended to the composer draft / task
  prompt. `{file}` = the workspace's file open in Files, else the one selected in Git; `{task}` =
  the session's task (composer) or the dialog's title; `{selection}` stays empty until T10.6.
  The Board follow-up is now the multi-line box: Enter = new line, Ctrl+Enter sends (§20B.1).

- (T4.1, ADR 0004) Tasks without an agent store `agent_profile = ''` = workspace default at run
  time; deleting a task is refused while one of its sessions is live, exited sessions are kept
  as free terminals (`task_id` = NULL); new `task.deleted` event. `task.move` accepts any status
  (the user can drag cards anywhere, SPEC §5).

- (T0.1) Crate license: no `license` field set yet (SPEC §19 leaves it open). Default: decide
  when the repo goes public.
- (T0.1) Rust edition 2024 for all crates; clippy `unwrap_used`/`expect_used` warn (denied by
  verify), allowed in tests via `clippy.toml`.
- (T0.2) gpui source: use crates.io `gpui-pre = "=0.3.7"` (renamed to `gpui`) instead of a git
  dep on zed `main`, because `gpui-component` 0.7 pins exactly that version. Note: `gpui-pre`
  is a third-party snapshot of Zed published by the gpui-component maintainers, not by Zed
  itself (supply-chain trust choice). See ADR 0001.
- (T0.2) Keep `portable-pty` 0.9 (SPEC §4) despite known ConPTY EOF quirks; alacritty's own
  `tty` module (what Zed uses) is the fallback if it blocks. Use crates.io
  `alacritty_terminal` 0.26, not Zed's fork.
- (T0.2) Release builds of gpui on Windows need `fxc.exe` (Windows SDK) — the spike is built in
  debug mode by verify, so no install is required now.
- (T0.3) `spike-term` has a lib target but uses `anyhow` (not `thiserror`, SPEC §17) because it
  is throwaway spike code and `portable-pty` itself returns `anyhow::Error`. Real crates
  (`mkrew-daemon` etc.) will follow §17.
- (T0.4) Terminal fixtures are stored as escaped text (`\e`, `\r`, `\xNN`; literal line breaks
  ignored) instead of raw bytes: readable, diffable, immune to git CRLF conversion, and the
  agent's file tools cannot write raw control bytes. Raw recordings must be converted first.
- (T0.5) Spike design tokens live in `crates/spike-term/src/theme.rs` (no `mkrew-ui` crate
  yet). The ANSI palette is the agent's own choice harmonized with §14; light-theme "white"
  ANSI colors are darkened (`#8A8C94`, `#55575F`) so they stay readable on `#FAFAF9`. Cursor is
  always an accent block (shape/unfocused style later).
- (T0.6) Spike input has no IME/`InputHandler`: IME composition (Vietnamese Telex, CJK) and
  dead keys may not reach the PTY. Done 2026-10-07: `mkrew_ui::ime::Typing` is the input
  handler of every text field and terminal. Paste is Ctrl+Shift+V only (§13); Shift+Insert/right-click later.
- (T0.6) PTY writes (keys, paste) are blocking writes on the UI thread; a huge paste into a
  child that is not reading could stall the window. Default: acceptable for the spike; the
  daemon (M1) owns PTY writes on its own thread.
- (T0.7) `spike-term -- <cmd>` resolves the program itself (PATH + `PATHEXT`, then `.ps1`;
  never the extensionless npm sh shim) and runs `.ps1` via `powershell.exe -File`. `.cmd`/`.bat`
  go straight to CreateProcessW, so cmd.exe re-parses args: quoted args with a space in the
  shim path, or `& | % ^` in args, can break (BatBadBut). Default: fine for the spike; the
  daemon's command lookup (M1) must build `cmd.exe /d /s /c "..."` with cmd-aware escaping.
  `-- <cmd>` starts in the current directory.
- (T0.8) SPEC §15 "Idle CPU < 1 %" is read as % of one core (stricter); measured ~3 % of one
  core (~0.25 % of a 16-thread machine). Stats use `sysinfo` 0.31 (already in the tree via gpui)
  because `unsafe` is denied and `powershell`/`tasklist` are not available to headless sessions.
  Added T0.8a (wake-on-output + reader coalescing, re-measure) before the T0.9 wrap-up, since
  idle CPU, key latency and the flood frame gap miss §15 for likely fixable reasons.
- (T0.8a) Output drains are paced by painted frames (at most one per display refresh, §10), not
  capped at 60 Hz. On this 165 Hz display a 50 MiB flood costs ~0.8 core of UI CPU (frames
  track refresh, gap max 18–24 ms); a 60 Hz cap would cut CPU but raise the gap. Idle CPU stays
  ~3.5 % of a core because GPUI's Windows vsync thread invalidates every window each vsync —
  not fixable without patching gpui. Default: accept both for the spike, weigh them in T0.9.
  The spike still quits on child exit and may drop output ConPTY delivers after the exit; the
  real pane (M2) must keep a finished session's final screen.
- (T0.9) Tauri was not prototyped; its ADR column is qualitative. SPEC §4 UI row now names
  `gpui-pre` + `gpui-kit` (matches the recommendation; revert if you choose Tauri). M0 is
  tagged `m0-done` with five criteria unticked (real agents, input, resize, native Windows,
  recorded agent fixture); they are covered by the checks above and the T0.4 item.
- (T0.9) SPEC §15 "Idle CPU < 1 %" is read as % of the **machine** from M2 on (GPUI's Windows
  vsync floor is ~3.5 % of one core, ~0.25 % of 16 threads); T0.8 had read it per core.
  Override here if you want the stricter reading (would require patching gpui).
- (M1 plan) Agent hooks (`mk hook`, `hook.report`) belong to M3, so in M1 the E2E "`mk wait`
  until done" is satisfied by screen detection (`idle` after `idle_after_ms`) or process exit
  of the fake agent, not by a `done` hook event. `mk attach` uses `crossterm` for raw mode
  (not listed in SPEC §4; small, cross-platform, Windows-native console API).
- (T1.2) Profile schema details not fixed by SPEC §6, now written into §6: optional
  `kind = "agent"|"shell"`; shell may omit `command` (daemon picks `%COMSPEC%`/`$SHELL`);
  unknown keys are errors (catches typos); `prompt_mode` defaults to `none`, `[state]` to
  `mode = "screen"`, `idle_after_ms = 3000`; name defaults to the file stem. A missing
  `command` is reported with the file but no line (no span for an absent key).
- (T1.4) State machine details SPEC §6/§8 leave open: a `needs_input_patterns` match on the
  last 4 non-blank screen lines (up to the cursor) gives `needs_input` at once, no idle wait;
  only `working` decays to `idle`; `done` is never downgraded by the screen; hook-only
  profiles go `starting` → `unknown` on first output. The send queue also delivers in
  `unknown` (otherwise a session we cannot read never gets input), one message at a time
  (the next one waits for PTY output). Tail size needs tuning with recorded agent output.
- (T1.5) Daemon details: env overrides `MKREW_DATA_DIR` / `MKREW_CONFIG_DIR` (the `mkrew` dir
  itself) and `MKREW_SOCKET` (endpoint; same var the PTYs get). A second daemon exits with code
  2. Startup marks **every** non-`exited` session exited (incl. starting/done/unknown — no PTY
  survives a restart), wider than SPEC §8's "working/needs_input/idle". Methods not built yet
  answer `invalid_request`. Windows pipe DACL is `D:P(A;;GA;;;SY)(A;;GA;;;OW)` (SYSTEM + pipe
  owner): a daemon started from an **elevated** shell is owned by Administrators, so a
  non-elevated `mk`/UI of the same user may be refused — use the user SID if that matters.
  Unix socket mode 0600 is set on Linux only (macOS rejects it before bind; relies on the
  0700 runtime dir / data dir there).
- (T1.6) `mk` details: sync (non-tokio) client; `mk daemon start` runs `mkrew-daemon` from
  next to `mk`, else `PATH`; it is a no-op if a daemon already answers, and `stop` returns
  once the daemon released its lock. `mk daemon status` exits 1 when not running. Other
  commands never auto-start the daemon. `mk project add --dev` is not built yet
  (`ProjectAddParams`/store have no dev field) — added with `mk project update` later.
  Windows: `CreateProcess` hands every inheritable handle to the child, so a detached daemon
  kept `mk`'s caller's stdout pipe open (`mk daemon start | cat` hung). `mk` clears the
  inherit flag on its own stdio with a scoped `#[allow(unsafe_code)]` `SetHandleInformation`
  (`windows-sys`) — the only `unsafe` in the workspace. Also adds `CREATE_NEW_PROCESS_GROUP`
  (Ctrl+C in the shell does not reach the daemon); Unix uses its own process group, not
  `setsid`.
- (T1.7) `mkrew-fakeagent`: TOML scenarios only (no JSON yet); `[[step]]` tables tagged by
  `action` (`print`/`spinner`/`ask`/`sleep`/`flood`/`write_file`/`exit`), unknown keys are
  errors. Scenario = `--scenario <file|name>`, else `MKREW_FAKE_SCENARIO`, else built-in `ask`
  (an existing file wins over a built-in name; written into SPEC §16). `ask`'s answer is not
  checked (`n` behaves like `y`). `write_file` overwrites. The `mk hook fake <event>` and
  `mk comment reply` steps of §16 come with those commands (M3 / comments milestone).
  Colors use `anstyle-query` to enable Windows VT processing (no own `unsafe`).
- (T1.8) `.cmd`/`.bat` programs: portable-pty 0.9 cannot pass a raw Windows command line, so
  the planned `cmd.exe /d /s /c "..."` escaping is impossible without vendoring it. Default:
  args containing `" % & | < > ^ !` or CR/LF/NUL are **refused** for `.cmd`/`.bat` programs
  (`invalid_request` naming the arg); `.exe`/`.ps1` get any args. This blocks `prompt_mode =
  "arg"` prompts with such characters for npm shims like `claude.cmd` — M2 (`task.run`) must
  choose: vendor portable-pty's `cmdline()` with std's BatBadBut escaping, or parse the npm
  shim and run `node <script.js>` directly (**resolved in T4.4**: npm shims are unwrapped, the
  refusal stays for other batch files). Other T1.8 defaults: the daemon seeds the repo's
  sample profiles into `<config>/agents` per missing file (never overwrites) and re-reads
  profiles on every spawn; `session.kill` on an exited session is ok (idempotent); graceful
  kill = close the pseudo console (Windows) / SIGHUP (Unix), forced after 5 s
  (`TerminateProcess` / SIGKILL); every greeted connection receives all events (no per-event
  subscription yet); session state is persisted on every change.
- (T1.9) Attach details SPEC §7 leaves open: the snapshot is the **visible screen only** (no
  scrollback) as styled runs per row (trailing default blanks trimmed, wide chars in their own
  `WIDE` run, ANSI 0–15 + dim variants → indexed, theme colors → `default`) + cursor + input
  modes; `session.attach` returns `{state, snapshot}`. Overflow of the 4 MB per-client queue
  sends a new per-client event `session.snapshot` (added to the §7 table), produced by the
  session task when the client has drained; an exiting session hands its final screen to
  overflowed clients. Output frames: first byte sent at once, then at most one frame per 16 ms
  and one frame in flight per client. `session.input/resize/attach` on an exited session →
  `invalid_state`, unknown → `not_found`; `session.detach` is idempotent; resize replies once
  queued (applied right after, in order with later commands); other attached clients are not
  told about a resize. PTY input writes are still blocking writes in the session task: a big
  paste while the child's stdout is backed up can deadlock that session — queued as T1.9a
  (dedicated PTY writer thread).
- (T1.10) Sessions are started from the CLI with a separate helper `mk session spawn <project>
  --agent <profile>` (prints only the id; size = the terminal's, else 120x40), not an
  `--agent` flag on `mk attach` (added to SPEC §9). Projects are named by id or name. `mk
  attach` uses two connections (streaming + control for input/resize), polls the terminal size
  every 250 ms (no resize signal on Windows), and detaches by closing the stream connection.
  Painting a snapshot clears the screen (and toggles `?1049` to match `alt_screen`); after
  detach the agent's last screen stays visible (like `tmux detach`). Mouse mode is restored as
  `?1002` (the snapshot only says "some mode"). Windows: raw mode + `ENABLE_VIRTUAL_TERMINAL_
  INPUT/PROCESSING` via a second scoped `unsafe` (`Get/SetConsoleMode`); std's console reader
  swallows a lone **Ctrl+Z** (0-byte read), so Ctrl+Z does not reach the agent. When stdin is
  not a terminal, raw mode/resizing are skipped and stdin EOF detaches. The session's exit ends
  attach with `[session … exited with code N]`.
- (T1.11) `session.send` with `submit` appends `\r` to the text in ONE write (an agent that
  treats fast input as a paste might not submit; check with `claude` in M2). Queued texts go out
  one at a time: the next waits for PTY output that arrives after the previous one was written
  (echo counts). `session.wait` also returns when the session **exits**, whatever `until` says
  (the state can never change again); `mk wait` prints the state (`exited (exit code N)`) and
  exits 0, a timeout exits 1 — scripts tell "reached" from "exited" by the printed state. A
  `session.wait` without a timeout blocks its connection; if the client vanishes, the daemon
  keeps that wait until the session reaches the state or exits (no default timeout, as SPEC §9).
- (T1.12) CI = one job per OS running `bash scripts/verify.sh` (one source of truth with local
  sessions), on pushes to `main`/`master` and on pull requests, `fail-fast: false`, cached with
  `Swatinem/rust-cache`. Windows checkout forces `core.autocrlf false` (repo is LF; fixtures and
  snapshots depend on it). Added the `x86_64-unknown-linux-gnu` rustup target on this machine
  for cross-checking (only the Rust std component, no system install).
- (T2.1) Tabs/defaults (written into SPEC §7): the daemon keeps `terminal_tab.layout` as
  **opaque JSON** (the UI owns the split-tree model, T2.5); new tabs have layout `null`, and
  `session.spawn` with `tab_id` into a `null`-layout tab makes the session its only pane
  (`tab.updated`); otherwise the client places panes. `tab.close` deletes the row, leaves its
  sessions running and sends **no event** (one UI per user; add `tab.closed` if a second
  client ever needs it). `session.spawn` without `agent_profile` uses the project's
  `default_agent` (agent; `invalid_request` if unset) or `shell` (shell); a named profile sets
  the session kind (a conflicting `kind` is ignored). `project.update` supports `name` and
  `default_agent` only (absent = keep, `null` = clear; must be an agent-kind profile).
- (T2.2) `mkrew-ui` depends on the `mkrew-daemon` library (like `mk`) for `ipc::name`,
  `EXIT_ALREADY_RUNNING` and the detached-spawn helpers (moved from the CLI to
  `mkrew_daemon::spawn`); a small shared client crate is possible later. The UI passes its
  resolved paths to the daemon it spawns via `MKREW_DATA_DIR/CONFIG_DIR/SOCKET`. A protocol
  version mismatch is shown as an error; restarting the daemon automatically is left for later.
- (T2.2) Contrast: §14 light `status-idle` #9A9CA6 is ~2.6:1 on `bg` and the light ANSI bright
  yellow #D69E2E ~2.3:1 — below 3:1 for icons/terminal text. Kept the spec values; proposal:
  `status-idle` ≈ #8A8C94, bright yellow ≈ #B8860B (spec change + ADR if you agree).
- (T2.3) The UI emulator is rebuilt from a daemon snapshot by replaying it as ANSI (the
  `paint` module moved from `mk` to `mkrew_protocol::paint`, shared by CLI and UI). A
  `session.snapshot` after an overflow therefore resets the pane's UI-side scrollback (the
  dropped output left a gap anyway). Until the sidebar/tabs land (T2.4–T2.6) the window shows
  the most recently started live session full-window.
- (T2.4) Sidebar lists every session of a workspace, exited ones included (✕ + exit code),
  newest first; pruning/hiding old exited sessions is left for later. The first load expands
  workspaces with a live session; the app still opens the newest live session on start.
  `project.add` now emits `project.updated` (SPEC §7 "entity changed") so every UI shows a
  workspace added from `mk`. Working uses a static `◐` glyph (animated spinner, the
  elapsed/idle times and the status-source hover of §11 come later); exited rows are inert
  until "Run again"/"Resume" exist; sidebar keyboard navigation + focus rings ship with
  the keymap (T2.7). Ctrl+Shift+K is bound on every OS (§13: only global/tab commands use Cmd).
- (T2.5b) Tabs/panes UI: tabs are loaded per workspace when it is first shown (start = the
  workspace of the newest live session); a workspace with no tabs gets a "Terminal" tab. Panes
  of exited or unknown sessions keep their slot in the layout (placeholder "✕ Session exited"
  / "Cannot attach") so layouts survive daemon restarts — nothing is pruned automatically.
  Clicking a session in the sidebar that is in no tab splits the focused pane **vertically**
  (side by side); an empty active tab is filled with the workspace's newest live session not
  in any tab (keeps the old "opens the newest session" behaviour, also for `mk` spawns).
  Clicking a workspace row shows it; clicking the shown one expands/collapses it. Deferred:
  close tab / split / new pane actions (T2.6/T2.7), tab status labels + pane headers (§11),
  focus ring on tabs, a primary action in the empty-tab state, detaching panes while the
  window is minimized (§10), not stealing focus back on every layout event.
- (T2.6) Pane lifecycle: closing a **live shell** pane kills the shell without asking (§10 only
  asks for a running agent) — change `lifecycle::close_action` if shells running a dev server
  should ask too. A session the UI has not listed yet (just spawned) asks. The close dialog is
  the native `window.prompt` (Kill session / Hide pane / Cancel; dismiss = cancel). New panes
  split the focused pane **vertically** (Alt+Shift+= / Alt+Shift+- split right/down arrive with
  T2.7); "+ Agent" fails with the daemon's "no default agent" error until the workspace has one
  (settings UI = T2.8). Close pane is bound to Ctrl+Shift+X now; "New agent/shell" shortcut
  hints wait for the T2.7 keymap. The >12 running warning is a tab-bar notice, not a toast.
  Deferred: disabled/in-flight state on the new-pane buttons (a double click spawns two),
  focus rings on buttons, a per-pane close icon on hover.
- (T2.7a) Keymap: `keymap.toml` is a flat table `command_name = "gpui-keystroke"` (names in
  `keymap::Command::name`, e.g. `close_pane`, `workspace_3`, `focus_left`); single keystrokes
  only, no unbinding yet; key names are not validated beyond GPUI's parser. A bad entry (unknown
  command, not a string, unparsable, already bound to another command) keeps its default and
  the first problem is shown in the sidebar (all are logged). Loaded once at start (no live
  reload). Split right/down starts a session of the **focused pane's kind** (agent → the
  workspace default agent, shell → shell; nothing focused → shell). Ctrl+Shift+A "pick
  profile" uses the default agent for now (profile picker later). Close tab does **not** ask
  and does not kill: its sessions keep running and stay in the sidebar; closing the last tab
  leaves a fresh empty tab. macOS Cmd mapping not done (Windows first). Only implemented
  commands are bound; palette, quick open, views, inbox, task, Docs, Zed, Files, Git shortcuts
  come with their features; copy, find, Shift+PgUp/PgDn, pane resize/zoom, rename tab = T2.7b.
- (T2.7b) Selection/scrollback: plain drag = character selection only (no double-click word,
  triple-click line, block selection or drag-past-edge auto-scroll; the drag stops at the pane
  edge because GPUI only reports moves over the pane). Copy clears the highlight (like Windows
  Terminal). Selected cells use `text` on `terminal-selection-bg` (program colors dropped for
  readability). Typing/paste scrolls back to the live screen; the selection stays. Wheel = UI
  history; in the alternate screen with alternate-scroll on and no mouse mode it sends Up/Down
  arrows; programs that enable mouse reporting get **no** mouse events yet (selection wins) —
  forwarding mouse/wheel to such programs is backlog. No "scrolled back" indicator yet.
- (T2.7c) Find bar: Enter = next match towards **older** output (up the history, the usual
  direction when the newest output is at the bottom), Shift+Enter = towards newer; both wrap
  around the 10k-line history. Pattern = regex (alacritty `RegexSearch`): case-insensitive
  unless it contains an uppercase letter; an invalid regex shows "✕ Invalid pattern". Each
  query edit jumps to the nearest match up to 1,000 lines above the view's bottom (Enter then
  searches everything). Minimal hand-rolled input: type, Backspace, Ctrl+Shift+V pastes the
  first line; no caret movement or selection (IME/Unikey text is typed at the end), and a query longer than the field is
  clipped at its end. No "n of m" count. Clicking the terminal keeps the bar open (Esc then
  goes to the agent); close with ✕ or Ctrl+Shift+F → Esc. Matches wrapped across 3+ rows
  starting above the view are not highlighted on its top rows. A match scrolled to the top row
  can sit under the bar.
- (T2.7d) Resize moves the innermost divider around the focused pane on the arrow's axis
  (→/↓ = divider right/down, whichever side the pane is on) by 5 % of that split, clamped so
  each side keeps ≥ 10 %; ratios rounded to 4 decimals. Zoom is UI-only and not persisted;
  panes hidden by a zoom are **detached** like other invisible panes (§10), so unzooming
  re-attaches them from the daemon snapshot and their UI-side scrollback restarts. Moving
  focus, splitting or closing the zoomed pane ends the zoom; resize is ignored while zoomed.
  Rename: same minimal input as the find bar (no caret movement; IME/Unikey text is typed at the end); blank or unchanged
  titles are not sent; the title changes when the daemon's `tab.updated` arrives.
- (T2.8) Workspace menu = "⋯" on the active workspace row or right-click any row; it holds
  "Open in Zed" (also Ctrl+Shift+Z, opens the workspace folder) and the default-agent list
  (agent profiles only, from `agent.profiles`). Deferred: keyboard navigation of the menu (it
  takes no focus; Esc does not close it, click outside does), no "None" choice to clear the
  default agent, no menu-hover token in `theme.rs` (uses `border` as hover fill), "⋯" does not
  toggle the menu shut. `zed` is looked up and spawned on the UI thread (fast; a slow network
  drive on PATH would stall a frame). PATH lookup moved to `mkrew_core::lookup` (shared with
  the daemon). `.ps1` zed shims run via `powershell.exe -File`.
- (T2.9) The automated §15 stress test (`crates/mkrew-ui/tests/stress.rs`) stands in for the
  window. "Input latency" there means the `daemon.status` round trip on the UI's own
  connection (behind six attached floods), plus the time to feed one drained batch (≤ 256
  frames, as the window does per frame). Gpui paint and real key dispatch are not covered.
  Floods are ~14 MB per pane in release and ~2 MB in debug (not 50 MB), so the test stays
  quick. Timings and client memory
  are asserted only in release builds (`verify` is debug). "RAM stays flat" means at most
  16 MiB growth after every scrollback is full. The daemon now merges queued PTY chunks
  (≤ 256 KiB) before its per-output work, which halved its CPU per flood byte. §15 "daemon
  CPU < 1 core" is still missed (~1.5 cores, because the floods are unthrottled) → T2.9a.
- (T2.9a) Flood pacing (ADR 0002): a session over 64 KiB of output per 100 ms is "flooding";
  flood batches of all sessions run one at a time and flood work is held to ≤ 2/3 of a core
  (floods ~20 % slower). Sessions under that rate never wait, so typing in a 7th pane during
  six floods should stay instant — not measured automatically (covered by the "§15 stress, in
  the real UI" check above). UI incoming channel: bounded to 16 MiB of output (+ 256 frames).
- (T3.1) "Minimized" = gpui `Window::is_visible()` is false. On Windows that is minimized or
  hidden (`IsIconic`/`!IsWindowVisible`); on macOS gpui may also report a fully covered window
  as hidden, which then detaches too (still §10 "not visible"). Restoring re-attaches from
  daemon snapshots; panes lose UI scrollback and an open find bar, like a tab switch.
- (T3.2) Hook mapping (`mkrew_core::hook`, SPEC §6): Claude `Notification` → needs_input only
  for `permission_prompt`/`elicitation_*`/`agent_needs_input` or a missing `notification_type`
  (assumed older builds); `idle_prompt` (~60 s after `Stop`) is recorded only. `SessionStart`
  only records the native id. Codex `native_session_id` = notify `thread-id` (docs call it the
  session id; the Codex source has a separate session id — check `codex resume <thread-id>`
  works for real). A hook that repeats the current state leaves `source` unchanged (hover
  may say "screen" for a hook-confirmed state). Unknown events of a known agent: stored, ok;
  unknown agent: `invalid_request`. Schema change → migration 0003 + ADR 0003, protocol v2
  (restart an old daemon after updating).
- (T3.4) `mk integration`: config dirs follow the agents' own overrides (`CLAUDE_CONFIG_DIR`,
  `CODEX_HOME`, else `~/.claude`, `~/.codex`). Hooks run bare `mk` as SPEC §6 says, so `mk` must
  be on PATH (`status` warns if not); an absolute path would survive a missing PATH but break on
  moves. Backup = `<file>.<unix ms>.bak`, only when the file changes. `settings.json` is
  re-printed (2-space, LF; key order kept), so uninstall restores content, not bytes. Codex
  allows one `notify`: install refuses to replace a foreign one (status says "not installed").
  Not used: Codex's newer `[hooks]` system (needs the user to trust hooks via `/hooks`, no
  Notification event) — revisit if `notify` is deprecated. SPEC §19 "status reports hook
  version" is not implemented (no version marker in the hooks).
- (T3.5) Notifications: `MKREW_NOTIFY` on the daemon (unset = OS via `notify-rust`, `off` =
  none, anything else = JSON-lines file path; tests use `off` or a file, so `cargo test` shows
  no toasts). Throttle = 30 s per session across needs_input and done together. Each toast
  keeps one daemon thread waiting for its click/dismissal (a Linux server that never closes a
  notification keeps it until the daemon exits). macOS: no click reporting (the daemon has no
  main run loop). A click with no UI connected is dropped (the UI is not launched). New
  `notification.clicked` event → protocol v3 (restart an old daemon after updating).
- (T3.6) Status display: a 500 ms check (`theme::STATUS_TICK`) redraws only while the window
  is visible and something shown changed: every tick while a session works (4-frame spinner),
  for idle only when its time text changes (each second, then each minute). Hover ages are
  computed when the tooltip opens. gpui has no
  reduce-motion query, so the spinner always turns (slow enough to pass; revisit if gpui adds
  one). Time = since the last state change (working → elapsed, idle → idle time), `12s` / `4m` /
  `1h 5m`. Tab label ranks starting/unknown below exited. Pane header = icon, agent profile id,
  title, status text; it takes one row from the terminal. "Run again" / "Resume" on exited
  sessions are not in T3.6 (still backlog; not yet in a task).
- (T3.7) Inbox: "viewed" = the done session is the focused pane of the shown tab while the
  Mkrew window is active (checked on every render; coming back to the window counts). Viewed
  state is in memory only (a restarted UI lists done sessions again) and keyed by the state
  change time, so a later `done` re-lists it. needs_input stays listed while looked at. Sessions
  of archived workspaces are left out. The Inbox sits under the workspace list (scrolls with
  it). Clicking an entry or a notification = same as clicking the sidebar row (focus its pane, or
  split the focused pane to show it). A notification click for an unlisted session only shows its
  workspace. `Ctrl+Shift+I` (jump to next Inbox item, §13) is not in T3.7 (backlog).
- (T3.8) `agents/omp.toml` is unchanged (screen, idle 3000 ms, no patterns): there is no real
  omp recording to refine it from. The fake `omp` scenario (spinner turns + quiet gaps) is only
  modelled on how omp is assumed to look. If real omp has an approval prompt or a quiet "thinking"
  phase > 3 s, add a pattern / raise `idle_after_ms` and record its output as a fixture. Screen-only
  profiles never reach `done` (by design, §6): a finished omp turn shows as `idle`.
- (T4.2) Task status rules (`mkrew_core::task`): session events move a card only while it is
  `running`/`needs_input`. A card the user dragged to todo/review/done (or one already in review)
  stays put on later events, so a follow-up typed into a review card's terminal does not move it
  back to running. A session start (Run) always sets running and clears `error_flag`. An exit
  with no code (killed) counts as non-zero. Only review cards can be approved. Session `idle`
  does not move a card: screen-only agents such as omp never reach `done`, so their cards stay
  running until the session exits or the user drags them.
- (T4.3) `task.run`: refused (`invalid_state`) while the task has a non-exited session (checked
  again under the spawn lock); an empty prompt sends nothing in every mode; the session title is
  the task title. `type` writes the prompt + Enter through the send queue at the first
  idle/needs_input/done, unchanged, so a multi-line prompt presses Enter per line (like
  `session.send`; bracketed paste = backlog). A card moved by the run lifecycle keeps its
  `position` (it may land mid-column). Sessions ended by daemon shutdown do not move their card
  (it stays running/needs_input for the §8 startup resume instead of getting `error_flag`).
  `arg` prompts with cmd metacharacters for `.cmd` shims: see T4.4.
- (T4.4) `.cmd` prompt args: chose **npm-shim unwrapping** over vendoring portable-pty's
  `cmdline()` (no fork; SPEC only asks to honor PATHEXT, so no ADR). A `.cmd` whose `%*` line is
  exactly `<launcher> "%dp0%\<target>" %*` (or `%~dp0`; launcher = nothing, `&`, `"%_prog%"`,
  `node`, `…\node.exe"`) runs its target directly: `.exe` as is, `.js/.cjs/.mjs` via the shim
  dir's `node.exe`, else `node.exe` on PATH (a `node.cmd`/`.ps1` wrapper is refused). The
  shim's `SET`s are skipped (e.g. pnpm's `NODE_PATH`) — fine for claude/codex. Shebang flags,
  extra args, missing targets, other batch files → old rule (metachar args refused). Ctrl+C no
  longer shows cmd's "Terminate batch job". Check: real `claude.cmd`/`codex.cmd` task with an
  `arg` prompt like `fix "a&b" 100%` (listed with the T4.10 manual checks).
- (T4.6) Each workspace opens on **Terminals**; the Board / Terminals choice is kept per
  workspace for the app run (not persisted). On the Board, tab/pane-creating shortcuts (new
  tab, next/previous/nth tab, split) switch to Terminals first; shortcuts that act on hidden
  panes (rename/close tab, focus/resize/zoom/close pane) do nothing. A card's session = the
  task's newest session; run time = its start to its exit, or to its `done` state change while
  the agent waits at its prompt, else now. "Last run failed" shows only when the card's status
  is not already `exited (code)`. The Board/Terminals switcher has no keyboard focus or focus
  ring yet (deferred with the tab/button focus rings above). Ctrl+Shift+1 / 2 (§13): fixed
  2026-10-07 by mapping bindings through the platform keyboard mapper (Windows folds Shift into
  the digit). Check Ctrl+Shift+1…5 with a US and a Vietnamese layout, also after switching the
  layout while the app runs and with a terminal focused (no NUL must reach the agent). The empty board names `mk task add` until T4.7 adds the New task
  action.
- (T4.7) Task dialog: modal; clicking the shade does **not** cancel (a stray click never throws
  a draft away; Esc / Cancel do), and shortcuts are ignored while it is open. Enter saves from
  the title and adds a new line in the prompt; Ctrl/Cmd+Enter saves anywhere; Tab toggles
  title ↔ prompt. Title and prompt are trimmed; a blank title is refused in the dialog. Edit
  sends only changed fields, diffed against the board's current task. Edit and delete live in
  the dialog, opened from a hover-revealed "Edit" on the card (no keyboard path until T4.9
  adds a selected card; agent chips are mouse-only for now). Text fields only append/backspace
  at the end (no caret movement or selection; IME composition and Unikey text are typed in place
  at the end since 2026-10-07 — backlog for the rest). A daemon
  disconnect drops an open draft (the whole UI resets). Delete confirm text says past sessions
  stay in the sidebar (ADR 0004 detaches them).
- (T4.8) Drag & drop: a card lands **before** the card under the pointer (the 2 px accent line
  sits in the gap above it), or at the end when dropped in the free space under the last card.
  Dropping where the card already is sends nothing. Any status can be dragged to any column
  (the user may override §5 transitions; a later session event still moves it per §5). No
  motion yet: the indicator and the drop snap without the §14 120–180 ms ease-out, and the
  original card stays visible (not dimmed) while its copy is dragged — polish backlog. No
  keyboard way to move a card (§13 lists none; backlog with the command palette). Two drops
  in quick succession during a column renumber may compute from stale positions (rare;
  `task.list` on reconnect fixes the order). A failed move shows a notice on that workspace's
  board until the next drop.
- (T4.9) Card actions: clicking a card selects it (accent border); a non-Todo card with a
  session also opens its terminal (Todo cards only select — a card dragged back to Todo is
  about to re-run). Ctrl+Enter (`run_task`) runs the selected Todo card; it is bound in a
  `Board` key context set on the window root only while a board is shown, so terminals still
  get Ctrl+Enter. Run keeps the user on the board (the card follows its session; click to
  watch). Approve = move to the end of Done. Follow-up is a single line (pasted line breaks
  become spaces), Enter sends text + Enter via `session.send`, Esc drops it; while it has the
  keyboard every shortcut goes to the field (like the task dialog). A typed draft survives
  losing focus; an empty one disappears; a draft whose session was replaced (re-run) or
  ended is dropped. Run shows "Starting…" (disabled) until `task.run` replies. Failed
  Run/Approve show the board notice. Run is quiet (secondary) on every card and accent only on
  the selected one, whose tooltip shows Ctrl+Enter. Gaps (backlog): cards can only be
  selected with the mouse (no arrow-key selection / focus ring yet, so Ctrl+Enter needs a
  click first); Approve has no shortcut/`Command`; a double-click on Approve sends two
  (harmless) moves; pressing in the follow-up field and moving the mouse may start a card
  drag (stopping mouse-down propagation there may also swallow the field's own click —
  untested without a display).
- (T5.1) Startup resume: Task has no note column, so "session stopped — run again?" is
  `needs_input` + `error_flag` (the card's error row) plus a daemon log line, not a stored
  note. "Last session" = the task's newest session by `started_at`; its `agent_profile` (not
  the task's current one) picks the `resume` template. The resume template is split on
  whitespace with `"…"`/`'…'` grouping and no escapes (SPEC §6), then `{session_id}` is
  substituted per word. The resumed session keeps the old title/size, carries the native id
  over (a second restart before the next hook still resumes), gets no prompt, and replaces
  the old session id in every tab layout of the project. A resume spawn failure (e.g.
  command not found) falls back to the error flag.
- (T5.2) Checkpoint: every `task.run` overwrites `task.checkpoint` with HEAD (`git rev-parse
  --verify HEAD^{commit}` in the project folder, so a subfolder of a repo uses that repo's
  HEAD); outside a repo, before the first commit, or when git cannot run, it is cleared and the
  run still starts (warning in the daemon log). Resume after a restart is not a new run and
  keeps the old checkpoint. HEAD does not capture uncommitted changes made before the run —
  T5.3's undo must account for that (preview shows everything that differs from HEAD).
- (T5.3, ADR 0005) Undo: tracked files go back to the checkpoint commit, so uncommitted edits
  the user had BEFORE the run are discarded too (they appear in the preview diff; there is no
  record of them). "Created since" untracked files = untracked, non-ignored files with mtime ≥
  `task.checkpoint_at` (taken just before HEAD is read in `task.run`); a pre-existing untracked
  file the run edited is deleted, an untouched one is kept, ignored files never. HEAD/commits
  are left alone (agent commits stay, their changes are reverted in the working tree). Empty
  directories left by deleted files stay. Files tracked since the checkpoint (e.g. an agent's
  `git add -A` of the user's old untracked notes) are deleted by the restore; the preview
  lists them under `added` ("delete"). Undo only deletes created files the client confirmed
  (previewed `created` list) and refuses when the tracked changes differ from the previewed
  `fingerprint` (std `DefaultHasher`, fixed keys: stable per daemon build; a daemon upgrade
  between preview and confirm only refuses). Review SHOULDs left open: `added` files are
  deleted whatever their age (could unstage old ones instead), `git diff` output is read
  whole before the 1 MiB cut, nested repos show as `sub/` in `created` but `git clean -f`
  keeps them. The preview is allowed while the task runs; undo is not — but
  only the task's OWN sessions block it: another agent live in the same folder gets its edits
  reverted, and a `task.run` racing an in-flight undo is not locked out. `mk task undo` without `--yes` prints the preview and exits 0. Tests pin
  `core.autocrlf=false`; with a global `autocrlf=true` a restore rewrites LF files with CRLF
  (normal git behaviour).
- (T5.4) Undo UI: "Undo" is a hover-revealed secondary button on Review/Done cards with a
  checkpoint (no keyboard path yet — like card Edit; a "Undo selected task" command could act on
  the selected card). The destructive confirm uses the existing `diff-del-bg`/`diff-del-text`
  tokens (no danger token in §14; status colors stay for agent states). The UI disables confirm
  while the preview is empty or any session of the task is live (same rule as the daemon). The
  diff renders at most 2000 lines as plain divs (rows parsed once per preview); a virtualized
  `uniform_list` and per-row backgrounds wider than the box on horizontal scroll are left for
  the M7 Git view. An undo that fails after Esc closed the dialog is only logged (no toasts yet).
  Open ui-review SHOULDs (left for a later polish task): no keyboard path to Undo / Tab to
  "Discard changes" (§14.7); buttons have no focus ring; hover-only Undo still reserves a row on
  Done cards; the diff builds up to 2000 row divs per render (`uniform_list` later). "Discard
  changes" stays enabled when the diff is cut short (the dialog points at `mk task undo`).
- (T5.5) File watcher: one recursive `notify` watcher per project while it has a live session,
  on the project folder (the session cwd). Only the ROOT `.gitignore` is read, once when the
  watch starts (nested `.gitignore`, `.git/info/exclude`, the global excludes file and edits to
  `.gitignore` while sessions run are not applied); matching is case-sensitive even on Windows.
  `.git`, `node_modules`, `target` are always ignored at any depth. Batches = 300 ms quiet, at
  most 2 s; existing directories are dropped, removed paths are reported (a removed directory
  looks like a file). Each batch becomes a `files_changed` session event (`{"paths": [...]}`,
  `/`-separated, relative) on EVERY live session of the project (attribution is T5.6). Review
  SHOULD left open: on Linux/macOS a recursive inotify/kqueue watch walks the whole tree
  (including `node_modules`/`target`) synchronously in `Hub::spawn` and can hit
  `fs.inotify.max_user_watches` on big repos (Windows ReadDirectoryChangesW is cheap); Windows
  drops events silently on buffer overflow (no rescan). Unwatching after the last session
  exits is not asserted by a test.
- (T5.6) Conflict attribution heuristic — who "edited" a file: (1) a watcher batch is blamed on
  the project's session in `working` when the batch lands, but ONLY when exactly one session is
  working; with two or more working at once the editor is unknown and the batch is not
  attributed (default chosen to avoid flagging every file while two agents run together; the
  alternative "blame every working session" would catch more real conflicts but warn on every
  concurrent edit — your call). Sessions in idle/needs_input/done are never blamed (an agent
  waiting for input does not write). Your own edits (e.g. in Zed) while exactly one agent is
  working are blamed on that agent. (2) Hook payloads with `tool_input.file_path` /
  `tool_input.notebook_path` (Claude `PreToolUse`/`PostToolUse` shape) are attributed to the
  reporting session exactly, whatever is working (a `tool_name` other than Edit/Write/MultiEdit/
  NotebookEdit is skipped; `PreToolUse` of a denied edit still counts); Codex `notify` payloads
  name no files. `hook_edits` does a store read + `metadata()` inside the actor task (cheap, once
  per hook).
  `mk integration install claude` does NOT install `PostToolUse` yet (it would run `mk hook`
  after every tool call) — decide whether to add it with an `Edit|Write|MultiEdit|NotebookEdit`
  matcher. Hook paths go through the watcher filter (ignored dirs, root `.gitignore`); absolute
  paths must start with the project path as stored (case-sensitive prefix, so `c:\` vs `C:\`
  misses). `conflict.detected` fires when a file gets a second (or further) editor within 60 s;
  repeat edits by the same editors stay quiet; trackers are dropped when a project's last
  session exits. The event is not persisted: a UI that connects later does not see it.
- (T5.7) Conflict warnings in the UI: each card / pane stops warning on its own — a task when it
  leaves Running/Needs input (re-running it later does not bring the warning back), a session
  when it exits or its task stops warning; the other party keeps its warning until it leaves
  too. "Dismiss" (hover-revealed, on the badge) clears that conflict on every card and pane, not
  only the clicked one. Several files → "Conflict: <first> +N", tooltip lists all. Warnings live
  in UI memory only (lost on UI restart, like the event itself). Badge color = the
  `status-needs-input` amber (no dedicated warning token in §14; add a `warning` token if you
  want status colors reserved for agent states). Dismiss is mouse-only for now (no focus ring /
  command), like the card's Edit and Undo — backlog with keyboard card selection. A session
  whose task is missing from a loaded board stops warning (covers deleted tasks; a task whose
  first `task.updated` has not arrived yet would lose its pane warning — unlikely race, accepted).
- (T5.8) `session.history` returns `files[]` (rotated `.log.1` then `.log`, existing only)
  instead of SPEC's single "path", so the rotated part is not lost; SPEC §7 updated (a
  refinement, no ADR). Clients read the files directly (daemon and clients share a machine).
  For a live session it is best effort (unflushed buffer, rotation race). `mk session history`
  prints raw PTY bytes including ANSI escapes (no stripping).
- (T5.9) History viewer = a modal dialog opened by a "History" button in the Terminals tab bar
  (the task allowed sidebar or Terminals menu; a dialog needs no layout slot and never touches
  `terminal_tab`). No keyboard shortcut (SPEC §13 has none; add one there if wanted). The log is
  replayed into a fresh UI emulator at the session's recorded cols/rows (no resize, no input),
  newest 2 MiB only, starting after the first line break past the cut; a session that ended on
  the alternate screen (full-screen TUI) shows that last screen with little scrollback. The list
  is read once on open (sessions exiting meanwhile appear on the next open). Open review
  SHOULDs (backlog): parse the tail off the UI thread (a 2 MiB log may stall one frame), a
  Retry button on list/log failures, focusable rows with a focus ring (↑ / ↓ work), a primary
  action in the empty state, a clean exit's ✕ is muted (only non-zero codes use the exited color).
- (T5.10) Daily backups are named by the **UTC** date (no time-zone crate; SPEC does not say
  which), so the "day" flips at local UTC midnight. The daemon checks at startup and then every
  hour (not every 24 h: monotonic timers pause during suspend) and writes `VACUUM INTO`
  `mkrew-<date>.db.partial` → rename, then keeps the newest 7 `mkrew-YYYY-MM-DD.db` (other files
  untouched). Only while the daemon runs: a day it never ran has no backup. The copy runs under
  the store mutex (requests wait while it runs; fine at today's DB size — review SHOULD: lock
  only around `backup_to` if the DB grows). No restore command (copy the file over `mkrew.db`
  with the daemon stopped).
- (T6.3) Review prompt (`mkrew_core::review::prompt`): besides the §12 template, a resent
  thread carries its replies as `  <author> replied: <body>` lines under the comment (later
  reply lines indented by four), so the agent sees the follow-up; quotes are not escaped
  (a `"` inside stays as is); a multi-line quote is collapsed to one line (whitespace runs →
  one space), a body keeps its lines (indented by two), no trailing newline. Re-anchoring
  ignores whitespace differences, picks the match whose first line is nearest the old start
  (tie → earlier) and does not strip Markdown markup — the UI (T6.9) must store the quote as
  source text, not rendered text. A comment without a quote keeps its lines while they exist.
  A quote may match inside a word (plain substring). A comment without lines is labelled
  `whole file` in the prompt (unreachable today: `comment.create` requires lines).
  Review globs: `*`/`?` never cross `/` (`*.md` = root files only), `**` spans directories;
  matching is case-sensitive (`PLAN.MD` does not match `*.md`, also on Windows).
- (T6.4) **How md files are watched with no live session:** `doc.list` or `doc.read` on a
  project starts (or marks) its recursive file watcher as "docs", and it then stays until the
  daemon stops — even after the last session exits (one watcher per project ever opened in
  the Docs view; no unwatch on client disconnect / archive). `doc.changed` fires for any `.md`
  file (any case) the watcher reports (`.md` files are reported even when `.gitignore`d, so
  `files_changed`/conflicts now see ignored Markdown too) or `doc.read` returns, whose content
  hash differs from the last one this daemon saw — so the first sight after a daemon start
  fires (and catches up edits made while it was down); a deleted file sends `hash: null`.
  Files > 2 MiB or not UTF-8 are skipped; they are read on the watcher's tokio task (review
  SHOULD: move to `spawn_blocking` if large docs ever stall it). Re-anchor
  rules: a passage that is gone → `outdated` (lines kept); an `outdated` comment whose passage
  is back → `open` (its earlier status is not remembered); `resolved` comments only follow
  their passage, never change status. `doc.list`: walks the project for glob matches
  **including** git-ignored files (plans often live in ignored folders) but never enters `.git`,
  `node_modules`, `target`; "recently changed" = `files_changed` events of the last **24 h**;
  files with unresolved comments are listed too; `open_comments` counts every unresolved
  thread root (open/sent/addressed/outdated). `last_session_id` = the newest event naming the
  file — a batch is recorded on every live session of the project, so ties go to the most
  recently started session (not necessarily the editor). Hash = std `DefaultHasher` (stable
  per daemon build only).
- (T6.5) **How `comment.send_to_agent` types the prompt:** the §12 prompt + Enter is queued
  like `session.send` (written once the session is idle/done/needs_input/unknown). It has
  newlines, so when the agent enabled bracketed paste (mode 2004, as Claude Code/Codex do) the
  daemon wraps it in `ESC[200~ … ESC[201~` then sends `\r` — this applies to every multi-line
  `session.send` now. Without mode 2004 the text is typed as is (each `\n` may submit early).
  To verify with real agents: does ConPTY pass mode 2004 through so the wrap happens, and does
  Claude/Codex accept the pasted prompt as one message? Other choices: any thread status can be
  sent (resolved/outdated → `sent`; the UI preselects open ones); all ids must be on one file
  of the session's project; comments are marked `sent` (snapshot = file content at send time)
  when queued, not when typed; reply ids stand for their thread; the reply is the sent roots.
- (T6.6) **Observed: without mode 2004, ConPTY drops the `\n`s of a typed multi-line text.**
  The fake agent (cooked console, no bracketed paste) receives the whole review prompt as ONE
  line (lines glued, only the final `\r` submits). The fake parses that form; the e2e
  `fake_agent_addresses_sent_comments_and_replies_to_each` passes. For real agents that enable
  mode 2004 this should not matter, but check during the T6.5 real-agent round trip that
  Claude/Codex see the prompt with its line breaks. If not, a fix task could type `\r\n` or
  Shift+Enter-style newlines instead.
- (T6.8) **Docs view choices:** a workspace opens Docs on its first listed file (sorted by
  path); the list is re-read when the view is shown, on `doc.changed` and on `comment.updated`
  — only for the workspace on screen (hidden ones catch up when shown again). The empty
  state's primary action is "Show terminals" (agents write the docs). Ctrl+Shift+E is bound
  only while Docs are shown (`Docs` key context), so terminals still get it; Ctrl+Shift+Z in
  Docs opens the shown file (no line yet; T6.9 may pass the selected line). Code spans keep
  the UI font on a raised background (gpui highlights cannot switch fonts); code blocks use
  the terminal font. The whole preview is laid out each frame (no virtualization; the source
  view is a `uniform_list`) — fine for plans, revisit if 2 MiB docs feel slow. Links are
  styled, not clickable yet. Open review SHOULDs: no keyboard navigation / focus ring in the
  file list; source lines neither wrap nor scroll sideways (long lines are cut); links look
  interactive but do nothing; the selected row / active Preview-Source segment use
  `surface_raised` (same as hover) instead of an accent tint; `docs` entries of removed
  workspaces are never pruned.
- (T6.9) **Comment selection = lines, not characters:** GPUI text has no built-in selection,
  so "select text" picks whole lines: a click on a top-level preview block (a whole list or
  table counts as one block) or on a source line; Shift+click extends to cover both. The quote
  is the selected source lines, trimmed (Markdown syntax included — the daemon re-anchors on
  the raw file). Use Source view for finer anchors. Threads are listed in the right margin in
  anchor order (not pixel-aligned next to their anchor); clicking a thread's "Lines a–b"
  selects those lines. Enter saves a draft (no multi-line typing; pasted line breaks are
  kept). Resolve is offered on every unresolved thread (incl. outdated), Reopen only on
  resolved ones; resolved threads hide Reply. Selection highlight reuses the
  `terminal_selection_bg` token. See ADR 0006 (SPEC §12 amended). The quote is taken when
  the draft opens; the margin stays while the doc is re-read after an edit. Open review
  items: Reply/Resolve are hover-revealed (also shown on a picked thread / while replying)
  and not keyboard-reachable; nothing clears a selection (no Esc / empty-space click); a
  click on a (future clickable) link also picks its block; agent replies show the raw
  profile id as author; the 300 px margin never collapses on narrow windows; A→B→A file
  switches can let a stale `comment.list` answer drop an update in between (narrow window);
  a double click on Resolve sends it twice (harmless).
- (T6.10) **"Send to agent" picks:** "the selected comments" = open threads whose anchor
  overlaps the current line selection (there is no per-thread selection); if none overlap,
  all open threads of the file go. Only `open` roots are sent (sent / addressed / outdated /
  resolved are not; a user reply on an addressed thread reopens it, so it can be resent).
  The picker lists the workspace's live agent sessions (shells excluded) newest first; the
  default is the file's `last_session_id` from `doc.list` when it is still live, else the
  newest. "View changes" is a per-thread action (shown when the root has a `sent_snapshot`)
  diffing that snapshot against the doc as currently shown (line endings ignored, 3 lines
  of context, capped at the undo dialog's 2000 lines); the dialog does not refresh if the
  file changes while it is open. Open review items: the error row is copy-pasted per dialog
  and uses `status_exited`; send errors show the whole error chain; titles ellipsize long
  paths at the end; quiet buttons inside thread cards have no visible hover; "Send to agent"
  looks enabled while disconnected (does nothing); a dialog left open survives a workspace
  switch via a notification click (as the other dialogs do).
- (T7.1) **Git base setting:** `git.set_base` trims the ref, treats blank like `null`
  (auto-detect) and refuses refs starting with `-` or containing whitespace; it does NOT
  check that the ref exists (it may only appear after a fetch — T7.3's status reports a
  missing base). Returns `null` and broadcasts `project.updated` (Project gained `base_ref`).
  CLI: `mk project update <p> --base <ref>` or `--auto-base` (one is required); `--dev` from
  SPEC §9 is still not implemented (no task asks for it). Protocol shapes chosen for
  T7.3–T7.6: `GitStatus` has separate `staged/unstaged/untracked/against_base` lists
  (conflicts are `unstaged` with kind `conflicted`), `base_auto`, `merge_base` (base mode
  only); `git.diff` has `staged` (working mode) and `commit?`; stage/unstage send back the
  whole `Hunk` as shown; `git.log` limit defaults to 50; `fs.find` limit defaults to 50.
- (T7.2) **Git parsers:** `DiffLine` gained `crlf` (SPEC §7 row updated, no version bump —
  no client uses `git.diff` yet) so a hunk patch reproduces CRLF lines byte-exactly instead
  of leaking `\r` into `text`. `parse_diff` assumes `a/`/`b/` prefixes (daemon must pass
  `--src-prefix=a/ --dst-prefix=b/`), is lenient (combined `diff --cc` hunks are skipped),
  and a deleted file's `path` is its old path. `hunk_patch` names only the new path (no
  rename headers) and never writes `new file mode`/`deleted file mode`: per-hunk staging of an
  untracked file fails, of a deleted file leaves it empty in the index, and unstaging a new
  file's hunk empties it — T7.4 stages/unstages those by path instead. Status `R`/`C` on
  either side carries `old_path`; ignored entries dropped.
- (T7.3) **git.status / git.diff:** ADR 0007 — a refresh runs status ∥ base lookup ∥
  staged/unstaged numstat (not ONE process; SPEC §12B updated). Base is resolved in both
  modes (header); `merge_base` only in base mode. Base mode also lists untracked files
  (kind `untracked`, no counts) and clears the staged/unstaged/untracked lists. Untracked files
  never have +/− counts. Not a repo / no merge-base → `invalid_state`; git failure →
  `internal`. `git.diff` of an untracked file = `git diff --no-index /dev/null <path>`; a
  per-file diff does not detect renames (pathspec is the one path). Diff cache key = target +
  HEAD + file mtime/size + index mtime (staging invalidates it), 256 entries then cleared;
  commit diffs = `git show --diff-merges=first-parent` (needs git ≥ 2.31). Status uses
  `--untracked-files=all` and `GIT_OPTIONAL_LOCKS=0`.
- (T7.4) **git.stage/unstage/discard/commit + git.changed:** stage = `git add -A`, unstage =
  `git reset -q` (also on an unborn branch), paths on stdin (`--pathspec-from-file`, literal;
  needs git ≥ 2.26); a hunk = `git apply --cached [-R]` with `--directory=<repo prefix>` (the
  client must stage new/deleted/untracked files by path, see T7.2). **Discard** = lose the
  *unstaged* changes only: tracked files go back to their index version, untracked ones are
  deleted (`git clean`, ignored files never); staged changes are kept. **Commit** = the whole
  index (also changes staged outside a subfolder project), hooks run, no amend. A stale hunk or
  "nothing to commit" comes back as `internal` with git's message (not `invalid_state`).
  `git.changed`: every watcher batch (working files or root `.git/{HEAD,index,packed-refs,refs/…}`)
  + right after each successful daemon mutation, so two events may come < 300 ms apart.
  `git.status` keeps the project watched until the daemon stops (like docs). Outside changes to a
  `.git` dir outside the project folder (repo subfolder, worktree) only show via working-file
  changes. Open review items: discard runs one `ls-files` per path; paths are `trim()`med
  (`docs::relative_path`).
- (T7.5) **git.log/show/fetch/push:** `git.log` without `path` covers the project folder only
  (`-- .`, like status), newest first, `limit` not clamped (0 = none), empty before the first
  commit; summary = first message line, body = the rest. `git.show` = commit info + files of
  the folder it changed (first parent for merges). **Fetch** = plain `git fetch` (the current
  branch's remote, else `origin`), so a configured base on another remote is not refreshed.
  **Push** = `git push -- <branch.<b>.remote> refs/heads/<b>:<branch.<b>.merge>` (no `+`, so
  never forced; `push.default`/`remote.*.push` ignored); detached HEAD / no upstream =
  `invalid_state`, rejected push = `internal` with git's message. Both run hooks and set
  `GIT_TERMINAL_PROMPT=0`; **no timeout**: a stalled remote or an SSH passphrase/host-key prompt
  keeps the request waiting (a Git Credential Manager GUI may still pop up — intended). §15 perf
  test (`status_of_a_10000_file_repo_meets_the_budget`) asserts < 150 ms only under
  `cargo test --release`; debug run here: median ≈ 87–117 ms. The view's status never writes the
  index (`GIT_OPTIONAL_LOCKS=0`), so it is slower until the user's own git refreshes it.
- (T7.6) **fs.list/read/find:** `.gitignore` (+ `.git/info/exclude`, global excludes) applies
  only inside a git repo (`ignore` crate default; the watcher applies the root `.gitignore`
  everywhere); hidden files are shown; `.git` is never listed; children of an ignored folder
  are all `ignored`; `node_modules`/`target` count as ignored (shown only with `show_ignored`) and are
  never searched by `fs.find`, like the watcher. **Markers** come from one `git status` per
  `fs.list` (not cached): untracked `?` > deleted `D` (either side) > added `A` > `M` (also
  renames, type changes, conflicts). **Deleted files** still known to git are listed as
  entries with marker `D` (only in an existing folder). Folder dot = any marker below it.
  Symlinks are followed when telling folders from files. **fs.read**: binary = a NUL byte in
  the first 8,000 bytes or not UTF-8 (so Latin-1 text counts as binary); files > 2 MiB come
  without content (`too_large`, binary sniffed from the head); `language` = a fixed extension
  map (`rust`, `typescript`, `python`, …, plus `Makefile`/`Dockerfile`) for T7.7's syntect
  lookup. **fs.find**: the file list is walked once per project and dropped on any watcher
  batch with changed files (so the watcher's misses = stale results until the next change);
  empty query = first `limit` files by path; ties = shorter path first. Perf test
  `files::tests::find_over_10000_files_meets_the_budget` (warm list) asserts < 100 ms only in
  release; debug here: median ≈ 47 ms, first walk ≈ 140 ms. `dir`/`path` with a `.git` part
  = `invalid_request`. Open review items: a deleted *folder* shows only its parent's dot (no
  phantom folder, listing it = `not_found`); every content edit drops the find cache (the
  next quick open re-walks, ≈ 140 ms debug on 10k files); symlinks may list/read outside
  the root (like `doc.read`); FileIndex entries of removed projects are never dropped;
  `fs.find` caches the walk even when the watcher failed to start (stale until restart); a
  folder moved in from outside only yields a directory event, which does not invalidate the
  find cache; e2e `fs_list_read_and_find_follow_file_changes` uses a 500 ms sleep to drain
  watcher batches (could poll `fs.find` with a deadline instead).
- (T7.7) **Files view:** syntax colors are 5 new theme tokens `syntax_keyword/string/number/
  type/function` (ANSI-harmonized in dark, darkened in light; comments = `text_muted`; all
  tested ≥ 4.5:1 on `bg`/`surface`/`surface_raised`) — not yet in the SPEC §14 table.
  syntect 5.3 bundled syntaxes (no TOML/TypeScript: `.ts/.tsx/.jsx` use JavaScript, TOML is
  plain); files > 512 KB or with a line > 4,096 bytes are shown unhighlighted (highlighting
  runs on the background executor, whole file at once). Tabs show as 4 spaces. Marker colors:
  M = accent, A/? = `diff_add_text`, D = `diff_del_text` (letters carry the meaning; the
  ui-reviewer suggested amber for M, but status colors are reserved for agent states). Only
  the Files view on screen follows `git.changed` (re-lists the root + open folders on screen,
  re-reads the opened file; reads coalesce while one is in flight); hidden ones refresh when
  shown. A selected line is cleared by clicking it again; no multi-line selection (T7.11
  adds line picking in the Git diff). Open review NITs: `rows()` is rebuilt per
  click/key/frame (O(n) of the visible tree); code lines have no hover state; the "Select a
  file" empty state has a key hint but no button (Quick open lands in T7.8).
- (T7.8) **Quick open:** finds files only. SPEC §13 also lists workspaces, sessions and tasks
  for Ctrl+Shift+O; that part is not built yet (no task queued, decide if it is wanted). Every
  query edit sends one `fs.find` (no debounce; the daemon's cached index answers in < 100 ms);
  late answers are dropped. Ctrl+Shift+O does nothing with no workspace or no connection, and
  cannot open on top of another dialog. Opening a pick switches to its workspace, shows Files,
  opens its folders and reads it. Open review items: the tree only scrolls to the revealed
  file if its folders were already listed (otherwise it may end up off-screen in a long tree);
  removing the workspace while the finder is open is not handled (finder stays); no "pick a
  workspace first" note.
- (T7.9a) **Git view split:** T7.9 was split; T7.9a is read-only (header, toggle, grouped
  list, diff, live refresh). T7.9b adds the base menu, stage/unstage, discard, diff syntax
  highlighting and palette commands for the Git keys. Choices: Git single-key shortcuts
  (↑↓ j k n p b) only work while the Git list has focus (like the Files tree arrows); they
  are not keymap commands yet. The first changed file is selected automatically; after a
  refresh the selection follows its path into another group (e.g. once staged), else the
  first file. A failed refresh keeps the shown list with "✕ Refresh failed: …" under the
  header. No gap row after the last hunk (file length unknown). "Open in Zed" opens the
  selected file (no line; a deleted file opens a missing path). The no-base empty state
  points to `mk project update --base` until T7.9b's menu. Open review items: the diff panel
  is not shown while the status is loading/failed/empty (the message sits in the 300 px
  column); long diff lines are clipped (no horizontal scroll, like the Files viewer);
  `GIT_FILE_LIST_WIDTH` 300 vs `FILE_TREE_WIDTH` 280; M/R/C/T letters use the accent like
  Files' markers; "↑2 ↓1" has no tooltip; CRLF lines are not marked.
- (T7.9b) **Git actions split + choices:** T7.9b was split again: T7.9b = stage/unstage +
  discard; T7.9c = base menu, diff syntax highlighting, palette commands. Choices: `s`/`u`
  act on the current hunk (after `n`/`p`) else the whole file; only `Modified` files go by
  hunk (the daemon's patch has no file-mode headers), others by path. Discard is only on
  Unstaged/Untracked files (restore from the index; staged changes stay, the sentence says
  so), via a "Discard…" button in the diff header — no key (SPEC §13 lists none). Opening it
  selects the file so its diff (what is lost, ≤ 500 rows) shows; confirm stays disabled until
  then (a failed diff = cannot confirm). One stage at a time; after any stage/discard the
  status+diff are re-read (also `git.changed`). A stage error clears on the next stage or file
  selection. A rename stages/unstages both its paths. Open review items: stage actions do not
  look disabled while a stage is in flight (extra clicks are ignored); "✕ Staging failed"
  uses the plain notice style (like the Zed error) while dialogs color the ✕; clicking a row
  action briefly dims the whole row (`.active`); no palette command for "Discard…".
- (T7.9c) **Base menu + Git keys choices:** diff syntax highlighting moved to a new task
  T7.9d. The Git single keys (b, n, p, s, u) are now keymap commands (`git_toggle_mode`,
  `git_next_hunk`, `git_previous_hunk`, `git_stage`, `git_unstage`) in a `Git` key context
  set on the changed-file column only (not the window root), so they only act while the
  list has focus — as before — and never reach a dialog's text field; ↑↓/j k stay hard-coded.
  The base menu has no shortcut (SPEC §13 lists none) and is a small modal, not a dropdown of
  branches: a free-text ref (validated by the daemon's `check_ref`; a ref that does not exist
  is accepted; Against base then says "Cannot compare with <ref>." + "Set base…"), Enter on an empty field =
  auto-detect. After saving, the Git view re-reads its status (mode kept).
- (T7.9d) **Diff highlighting choices:** each hunk side is highlighted as one run (old =
  context + deleted, new = context + added; context lines use the new side), starting with
  no state at each hunk (the rest of the file is not read), so a hunk that opens inside a
  block comment/string is miscolored. Syntax is chosen by file extension only (the diff has
  no language field). Same limits as the Files viewer (512 KB of diff text, 4096-char
  lines). In a highlighted diff, text outside tokens is `text` (not green/red); the `+`/`−`
  sign and the line background keep the diff colors. Light `diff-del-bg` lightened
  #FCE8E8 → #FDEEEE (SPEC §14 table updated; no ADR, token tweak only) so comments /
  line numbers (`text-muted`) reach 4.5:1 on deleted lines (was 4.38); new test
  `highlighted_diff_lines_stay_readable`. Dark `text-muted` on `diff-add-bg` is 4.53:1 —
  little headroom (ui-reviewer SHOULD, left as is).
- (T7.10a) **Commit box / Fetch / Push choices:** T7.10 was split (a: commit + Fetch/Push,
  b: History, c: task "View changes", which needs a daemon diff-since-commit). The commit box
  sits under the changed-file list, only in Working changes, and only when there are changes
  or a draft. `c` (new `git_focus_commit` command, §13) focuses it from the list; Enter = new
  line, Ctrl+Enter = commit, Esc = back to the list (draft kept per workspace). While it has
  focus, every shortcut goes to the box (like the comment draft) — press Esc first to switch
  views; once the box goes away (all committed) the list gets the keyboard back. Not done
  (ui-reviewer SHOULDs): palette commands for Fetch/Push (§13 lists no keys), keeping the
  caret in view when a long message scrolls, a platform-aware "Ctrl+Enter" hint on macOS. Commit is disabled with nothing staged ("Stage changes to
  commit them"); the message is trimmed, internal blank lines kept. Fetch / Push are small
  actions at the end of the upstream line; one runs at a time ("Fetching…"); Push is faded
  without a branch + upstream (no `push -u` in v1). Errors are wrapped under the header (push)
  or under the box (commit); every outcome re-reads the status.
- (T7.10b) **History choices:** a third header segment "History" (labels shortened to
  "Working" / "Against base" / "History", sized from their text so all fit the 300 px column)
  and a new `h` key (`git_toggle_history`, GIT_CONTEXT; SPEC §13 row added — an addition, no
  ADR). The list shows the last 50 commits (summary, then short hash · age muted; message +
  author in the tooltip); the selected commit expands in place with its files indented under
  it, and its first file's diff opens. ↑↓ / j k walk the expanded commit's files and continue
  into the previous / next commit (its first file). `b` from History returns to the changes
  section it left; `h` toggles back too. Commit files have no Stage / Discard. `git.changed`
  re-reads the log (selection kept if still listed) but not the commit's diff (immutable).
  Merge commits list whatever `git.show` gives (not checked in the UI); an unborn branch shows
  "Cannot read the history." with git's error rather than "No commits yet." (not checked).
  Open review NITs: "Open in Zed" on a commit's file opens the working-tree file (may be gone
  or moved); ↑ from a commit's first file lands on the previous commit's first file (not its
  last); the segment says "Working" while SPEC §12B/§13 say "Working changes" (tooltip has
  the full name) — check in the §14 design pass.
- (T7.10c) **Task changes choices:** `git.status`/`git.diff` gained `since?` (base mode only,
  replaces the merge-base; SPEC §7 rows updated, protocol v13) rather than a third `GitMode`.
  The card action "Changes" is a hover-shown quiet button on **Review** cards only (SPEC §12B
  says Review column; Done cards still have Undo), left of Undo. In the Git view it is a fourth
  section with no header segment; the base line is replaced by "Since <short hash> · task:
  <title>" + "Close" (→ Working, like `b`); no stage/discard on its files; a failed status
  (e.g. checkpoint gc'd) offers "Working changes" instead of Retry. Open review items: like
  Undo, "Changes" has no keyboard / palette path (§14 "keyboard = mouse") — add a "View task
  changes" command for the selected card later; `Target.commit` carries both a History commit
  and the task checkpoint (told apart by `group`).
- (T7.11a) **Diff comment choices:** `DocComment.context` / `CommentCreateParams.context`
  (default `doc`) and a `CommentListParams.context?` filter (protocol v14); replies take their
  root's context. Code prompt = §12 template with "Fix the code in <file>", "Line numbers are of
  the current file." (no "Only edit this file": a fix may touch other files) and `code "…"`.
  Doc and diff threads cannot be sent together (`invalid_request`). `doc.list` ignores diff
  comments, the Docs view lists only `doc` ones. Open review items: comments on non-Markdown
  files are never re-anchored / outdated when the code changes (only watched `.md` files are),
  so their lines can go stale after the agent's fix (SPEC §12B says so; backlog);
  `Store::create_comment` has 7 args (`allow(clippy::too_many_arguments)`).
- (T7.11b) **Diff comment UI choices:** only lines with a new-side number can be picked
  (removed lines cannot); the quote is the picked lines the diff shows, so a Shift+click across
  a collapsed gap quotes only the shown lines (the daemon may then mark it outdated). Comments
  are allowed in Working, Staged, Against base and task changes, not in History (a past
  commit's line numbers are not the current file's); the anchor is always "new side" of that
  diff, which for Staged is the index, not the working tree file. The comment margin shows only while the file has threads, picked lines, a draft or
  a list failure (a diff keeps its full width otherwise). Ctrl+Alt+M / Ctrl+Shift+S bind with
  the context `Docs || Git` (the Git list must have focus; clicking a diff line gives it focus).
  Default "Send to agent" session in a task's changes = the task's newest session ("ran this
  task"), else the newest live one (no per-file "last session" for code files). Diff threads
  have no "View changes" (no whole-file text to compare). Open review items (not fixed):
  picked diff lines use an accent bar + accent numbers while Docs uses a selection fill and the
  thread card a focus border (maybe one `comment_pick` token); the "N lines selected" margin hint
  has no primary "Comment" button (Docs neither); the first pick narrows the diff by the 300 px
  margin; the hover bar (`border`) is faint on add/del rows; Staged and Unstaged diffs of one
  file share its threads although their line numbers differ; Git buttons reuse the
  `doc-comment` / `doc-send` ids.
- (T8.1) **Dev server config choices:** the dev command is trimmed and a blank one counts as
  "none" (same as `null` / `--no-dev`); auto-start is only set via `project.update` / `mk project
  update --dev-autostart on|off` (not on add) and is kept when the command is cleared (it does
  nothing without one). `mk project update` accepts base and dev flags together (two requests,
  base first); `--base`/`--auto-base` and `--dev`/`--no-dev` conflict pairwise. `mk project list`
  does not show the dev command.
- (T8.2, ADR 0008) **Dev server choices:** `dev_command` is typed into a `shell`-profile session
  (not run as `sh -c`/`cmd /c`), so it starts after the shell's first quiet period (~3 s with the
  sample `idle_after_ms`) and the pane keeps a prompt when the server stops. "The" dev server =
  the project's live shell titled `dev` (no DB column); auto-started ones use 120×30 until the UI
  resizes them. The `dev` tab is reused only when empty or holding one exited pane, else a new
  `dev` tab is created. `dev_stop` terminates gracefully (like `session.kill` without force) and
  leaves the tab with its exited pane. Manual check (with T8.3): a real `npm run dev` starts,
  serves, stops with `mk project dev <p> --stop` on Windows (ConPTY: does the child tree die?)
  and Linux; auto-start after `mk daemon stop` + start. Open review SHOULDs: `dev_stop` only
  queues a graceful kill, so an immediate `dev_start` returns the dying session (`started: false`)
  — a UI "Restart" must wait for `session.exited` first (or the daemon should skip stopping
  sessions); a running dev server whose pane was closed comes back with `tab_id: None` (not
  re-placed in the `dev` tab).
- (T8.3) **Dev server UI choices:** the settings live in a "Dev server settings…" dialog opened
  from the workspace menu (one-line command + auto-start checkbox; Tab toggles it since there is
  one field; pasted line breaks are dropped). Start/Stop sit in the workspace menu and on the
  tab bar's right (Start only when a command is set; no "Restart", see the T8.2 SHOULD). The
  tab holding the running dev server is labelled "Dev server" (its stored title stays `dev`, so
  renaming shows `dev`). Sidebar indicator = muted "▶ dev". Start/stop errors and "runs but has
  no tab" show in the sidebar. Open review items: the reply-vs-`tab.updated` ordering
  (`dev_reveal`) is glue in main.rs without a unit test; the "running" rule is duplicated from
  the daemon's private `dev_session` (could move to `mkrew-protocol`); no busy state while
  `project.dev_start` is in flight.
- (T8.4) **Command palette model choices:** commands that cannot act in the current view are
  hidden, not greyed out (e.g. Git commands outside Git, pane commands outside Terminals,
  "Jump to tab 7" with 3 tabs, terminal copy/paste/find unless a pane had the keyboard). Titles
  read "Group: Action" like the §13 table. "Go to" items cover the §13 "quick open workspace /
  session / task": every non-archived workspace, every non-exited session (`title · workspace`)
  and every task (any status) the UI knows. Ctrl+Shift+O stays the Files quick open (T7.8), so
  its palette title is "Files: Quick open file". Fuzzy match is our own (case-insensitive
  subsequence, best alignment scored by word starts + runs), not `nucleo-matcher`.
- (T8.5) **Command palette UI choices:** a picked command runs from the focus the palette
  opened over (deferred `window.dispatch_action(Run)`; Git / comment commands in the Git view
  refocus the Git list first), so it behaves like its shortcut. Palette-only actions show only
  where their buttons are: Git fetch / push in the Git view (errors show under its header),
  discard when the selected file is unstaged / untracked, "View changes" on the board for the
  selected Review card with a checkpoint, dev server Start when a command is set and it is not
  running, Stop while it runs. "Open keymap settings" comes with T8.6's
  `Command::OpenKeymapSettings` (listed as a command automatically). "Go to task" switches to
  the board and selects the card (no scroll to it). Ctrl+Shift+P again closes it. Open review
  items: the "Command palette" heading is kept (like quick open's) though palettes often start
  with the field; a click outside does not close it (shared `modal_shade`, quick open too); rows
  are plain elements (`uniform_list` for big boards later); shortcuts are muted text, not keycaps.
- (T8.6) **Keymap settings choices:** edits still apply after a restart (no live reload; the
  template header says so). Keymap errors are those found at start, listed first in the palette
  as "Keymap error: <message>" with the Ctrl+, shortcut, and always shown (they are not filtered
  by view). The template uses GPUI key syntax with `ctrl-` defaults; the §13 macOS Ctrl→Cmd
  mapping is still not applied to any default (pre-existing gap). An existing file is never
  overwritten; a failed write removes the partial file.
- (T8.7) **Packaging choices:** plain WiX 7 `wix build` (cargo-wix still defaults to WiX 3), MSI
  per machine. **WiX 7 needs its OSMF EULA accepted** (the Open Source Maintenance Fee applies
  above US$10k/yr revenue from projects using WiX): `package.ps1` only passes `-acceptEula wix7`
  with `-AcceptWixEula`, but the Release workflow passes it by default — remove the switch from
  `.github/workflows/release.yml` (and accept with `wix eula accept wix7` yourself) or pin WiX
  6.0.2 if you disagree. The release job runs on `v*` tags and by hand, uploads workflow artifacts
  only (no GitHub release). Archives ship `agents/*.toml` as reference copies (the daemon seeds
  the config dir itself) and README.md; no code signing.
- (T8.8) **README is now the product README** (install, configure agents, daily use, uninstall,
  files and folders); the former build-kit README moved to `docs/BUILD_KIT.md` (linked from the
  README's "Development" section) because the README ships inside the MSI/tar.gz. The tar.gz
  section suggests `~/.local/opt/mkrew` + a `PATH` line (no installer script); "From source" uses
  `cargo install --path` for the three crates. Linux build dependencies are only described as
  "the system libraries GPUI needs (the same ones as building Zed)" — add an exact package list
  if you want one. `crates/mkrew-cli/tests/readme.rs` checks the sections and that every `mk`
  command and flag in the README exists.
- (T8.9) **Measurement choices:** "first frame" = the UI's first render, timed from launch by
  the measure test (includes process start); also reported once connected. Measurements report
  only — no new assertions, so noisy machines do not fail `verify`. Idle UI CPU measured 1.9–2.5%
  with six panes (budget 1%); recorded as over budget with fix task T8.9a rather than relaxing §15.
- (T8.9a) **Idle CPU judged per machine**, the reading accepted at T0.9: 0.14% of 16 threads
  (UI ~2% of one core). Profiling showed no idle repaints; an empty GPUI window alone costs
  1.25–2.03% of a core (vsync thread invalidates every window each vblank), now measured by
  `examples/idle_floor.rs` next to Mkrew. SPEC §15's row now says "of the machine". Lowering the
  per-core number would need a patched gpui — say so if you want that.
- (T9.4) **Turn at `needs_input`:** SPEC §20.0 says a turn ends at `needs_input` and that a
  `needs_input` → `working` resume continues it. Chosen: `needs_input` ends the turn (so
  `session.turn` is sent), and going straight back to `working` reopens it with the same `n`,
  start and files; it ends (and is sent) again later. A `needs_input` with no turn open (e.g. a
  startup trust prompt, or after `done`) then `working` starts a new turn. Files are attributed
  only while a turn is open.
- (T9.5) **Late file-watcher batches:** the watcher flushes ~300 ms after the last change, so an
  edit made right before `done` can arrive after the turn ended. Chosen: such a batch is dropped
  (not added to the ended turn, not to the next one), matching M5, where only a session in
  `working` gets batches attributed. Hook-reported edits arrive before the `done` hook, so
  agents with hooks (Claude, fake) lose nothing. Only agent sessions have turns (shells: none).
- (T9.6) **Notification sound per OS:** Windows: `sound_name("Default")`, because notify-rust
  shows a *silent* toast when no name is set (so before M9, toasts were silent).
  macOS: `"NSUserNotificationDefaultSoundName"`. Linux has no default sound, so the freedesktop
  `sound-name` hint is `message-new-instant`, plus `suppress-sound` when the switch is off.
  A switched-off kind does not use up the 30 s window; an allowed notice does.
  To verify: with `notify.sound = true`, a needs_input toast plays the sound on Windows (and on
  macOS/Linux if you have them). With `sound = false` it is silent. With `notify.done = false`,
  no toast appears on done.
- (T9.7) **Keep awake details:** `awake.keep_awake` is read on each session state event, so
  turning it off while agents work releases at the next state change (+5 s), not instantly.
  The Linux/macOS backend could not be compiled on this Windows box (reviewed by reading only).
  Linux limits: if the daemon is killed, `systemd-inhibit … sleep infinity` is orphaned and keeps
  blocking sleep (no parent-death signal; that needs `unsafe pre_exec`); if `systemd-inhibit`
  exits at once (no logind / polkit denial), nothing is logged. macOS `-w <pid>` has neither issue.
  To verify: `cargo clippy` on Linux/macOS; on Windows run a fake agent that works for a while
  and check `powercfg /requests` (as admin) lists `mkrew-daemon` under SYSTEM, and that it is gone
  ~5 s after the agent stops; a shell session running a command never appears there.
- (T9.8, ADR 0011) **Turn summary + Inbox last line:** (superseded by ADR 0034/0036) the summary reads `done · 3 files · 4m`
  (app duration style, not `4 min`); only the latest turn of a session is kept, and only while
  the UI runs (a UI started after the turn ended shows plain `done`). The Inbox last line is read
  once per entry appearance (`session.read lines=1`), so it does not follow later output; the row
  grows by one line when the read returns. To verify: run a fake agent `ask` scenario → the
  Inbox row shows the `(y/n)` line under the name; a fake agent that edits 2 files → Inbox row
  and pane header show `✓ done · 2 files · …`.
- (T9.9, ADR 0012) **Peek popover:** the lines are plain monospace text (`session.read` has no
  colors; SPEC said "colors kept"). ↑/↓ cycle only from an Inbox peek. A hover peek does not take the keyboard (a terminal
  keeps typing); clicking its reply field does. It closes ~150 ms after the mouse leaves both the
  row and the popover, unless a reply is typed or it has the keyboard. Ctrl+Shift+Space opens the
  first Inbox entry with the keyboard in it (pressed again: closes); ↑/↓ cycle the Inbox; Esc or
  a click outside closes and the keyboard goes back. The lines are read again on each state change
  of the peeked session (not while it streams output). Only live sessions peek (not exited ones).
  An Inbox peek whose session leaves the Inbox moves to the session's sidebar row if it holds a
  reply or the keyboard (else it closes); ↑/↓ to another entry drops a typed reply. The popover
  is anchored to its row; a row scrolled out of the Inbox list is not scrolled into view.
  To verify: fake agent `ask` → hover its Inbox row 300 ms → popover with the `(y/n)` line,
  state and age; click `y` → the agent continues, the popover shows the new screen; Ctrl+Shift+Space
  from a focused terminal, type a reply + Enter, Esc → the terminal has the keyboard again.
- (T9.10) **Broadcast:** `session.send` returns only `ok` until M10 (`{send_id, queued}`), so a
  row shows "queued" when its session was `working` at send time (the daemon queues then) and
  "sent" otherwise. The palette offers "Agent: Broadcast to agents…" only while the active
  workspace has a live agent session. The text is one line (pasted line breaks become spaces);
  ↑/↓ move between sessions, Tab or a click checks/unchecks one, Enter sends, Esc closes. The
  text clears once every send succeeded; after a failure it stays so the failed rows can be
  re-sent alone. The dialog stays open after sending (to show the results). The session list
  is fixed when the dialog opens (an agent exiting meanwhile shows "failed: …" on send).
  To verify: two fake agents (one working, one idle) → palette → Broadcast → both checked →
  type + Enter → rows show "queued" / "sent"; kill one agent and send again → "failed: …".
- (T10.2) **Prompt templates:** a variable is `{name}` with `name` made of ASCII letters,
  digits, `_` or `-`; any other brace (`{}`, `{ a }`, a lone `{`) stays literal. An identifier in
  braces inside code (`format!("{name}")`, `${HOME}`) IS treated as a variable, blanked and
  listed; there is no escape for a literal `{file}`. CRLF in prompt files becomes `\n`. A first line `# Title` is removed
  from the body (with the blank lines after it); without one the title is the file stem.
  Values are not expanded again. The three seeded samples' wording is the agent's choice
  (`mkrew_core::prompt::SAMPLES`); they are written only when `prompts/` does not exist.
- (T10.3) **Composer:** the text box types and deletes at the end only (no caret movement,
  like the task dialog / follow-up fields). While it has the keyboard every shortcut goes to it
  (like the follow-up field; Esc gets out), except Ctrl+Shift+Enter which just keeps it.
  The sent text drops trailing blanks and leading blank lines (the first line's indent stays).
  Drafts of ended sessions stay in memory until the UI restarts. The composer opens on exited
  agents too (send then shows the daemon's error). Shown composers stay docked when another
  pane is focused; only the focused one shows the key hint.
  To verify: agent pane → Ctrl+Shift+Enter → type 2 lines (Enter) → Ctrl+Enter → the agent gets
  one paste + Enter; type, Esc, switch panes and back, Ctrl+Shift+Enter → draft still there;
  check the composer visually (dark/light).
- (T10.4) **Composer `@`:** typing `@` opens the file picker instead of typing `@` (so the
  default `@{path}` form is not doubled); Esc on it types the `@` (e-mails, handles), and with no
  daemon the `@` is typed directly. `settings.toml` is re-read at each pick (a broken file =
  the default `@{path}`). Paths are those of the session's workspace (`fs.find`), even when the
  session runs in a task worktree. Removing a queued text has no keyboard path (✕ click only);
  an unqueue error is only logged. Queues are not re-fetched after a daemon reconnect.
  To verify: composer → `@` → pick `docs/my notes.md` → `@"docs/my notes.md"` typed; make the
  agent work, send twice → both listed with ✕ and the header shows `⧗ 2 queued`; ✕ one → it is
  never typed.
- (T10.8) **Drop files / paste image:** works on every live terminal pane (agent or plain
  shell), not only agent panes. Paths are typed as one paste (bracketed when the program asked
  for it), quoted with `"` only when they contain whitespace (a `"` inside a Unix path is not
  escaped). If the clipboard has text AND an image (e.g. Excel cells), the text wins. Images in
  other formats (bmp, tiff, svg…) or over 10 MiB, and `attachment.save` errors, are only logged
  (no visible message yet). No drag-over highlight on the pane.
  To verify (Windows): drag 2 files (one with a space in its path) from Explorer onto Claude's
  pane → both absolute paths typed, the spaced one quoted, no Enter. Win+Shift+S screenshot, then
  Ctrl+Shift+V in the pane → a path under `<data>/mkrew/attachments/<session>/` is typed and
  Claude picks the image up; repeat with an image copied from a browser.
- (T11.1) **Turn snapshot cost:** every git process costs ~50 ms on this Windows machine, so a
  snapshot (`add -A` + `write-tree` + `update-ref` on a temp index copy) is ~250 ms on the
  10,000-file repo (release, budget 300 ms). To stay under it the index path of each folder is
  resolved once and cached per project (re-resolved after a failed snapshot). The temp index sits
  next to the real one (`index.mkrew-snapshot-<pid>-<n>`); a daemon killed mid-snapshot leaves it
  behind. A file locked by another process makes `add -A` fail and that turn has no snapshot
  (T11.3 logs and skips it rather than failing the turn).
- (T11.3) **Turn recording:** the `hook.report` reply waits for the start snapshot of any
  hook-started turn (not only `UserPromptSubmit`; a superset of §20C.1). A turn the screen
  starts (e.g. the agent's start-up output) is snapshotted best effort and can include the first
  edits. End snapshots are kept by `refs/mkrew/turns/<sid>/<n>-end` (deleted with the others
  after 7 days). A turn ended at `needs_input` gets the next turn's start tree as its end tree,
  or an end snapshot when the session closes. Line counts are kept only for files attributed to
  the session (watcher/hooks), matched against `git diff --numstat` run in the session's cwd
  (cwd-relative paths; equal to project-relative except in worktrees). Daemon shutdown waits
  for exit-turn snapshots within the existing session budget (5 s grace + 2 s); past it they are
  lost.
- (T11.5) **Since a turn / Changed by / edit lines:** `since` = a tree is compared with a fresh
  unreferenced snapshot of the working state (it holds untracked files), taken on every
  `git.status` and every uncached `git.diff` in that mode: ~250 ms on the 10,000-file repo, it
  holds the project's snapshot lock (a turn start can wait that long) and leaves unreferenced
  objects for `git gc`. `session.files` = files of turns ended since `since_ms` + hook-reported
  edits (session event `edited`); files only the watcher saw in a still-running turn appear
  when the turn ends. The `session.edited` line comes from Claude's `PostToolUse`
  `tool_response.structuredPatch` (first hunk's `newStart` + leading context lines), which is
  undocumented: check with a real Claude edit that `line` is present and right; without it
  Follow in Zed opens the file at its top.
- (T11.7) **Since turn / Changed by UI:** "Diff since" lives on each Turns dialog row only (no
  separate pane-menu entry; the dialog is the pane's "Turns…"). It does nothing (closes) when the
  session is not in the active workspace. `#n` in `claude #2` numbers same-named sessions of the
  workspace by start among those the sidebar lists. "Changed by" is a chip row under the Git
  header (hidden when no session changed files in 24 h, and in the History), mouse only (no
  palette command / key yet); `session.files` is re-read with every `git.status`. Sessions no
  longer listed show as `ended session` (numbered when several). Renamed files match on the new
  path only. The "Send to agent" default label is now "made these changes" (tasks and turns).
- (T13.3) **Worktree runs:** a worktree task whose project is not in a git repo runs in the
  project folder, with the first output line `mkrew: not a git repository; running in the
  project folder` (SPEC only says "on Run in a git repo"). Skipped `.worktreeinclude` entries
  show as one line `mkrew: .worktreeinclude skipped <entry: reason>; …` before the agent's output.
  A failed `worktree add` (e.g. a repo without commits) refuses the run (`invalid_state`); a
  recorded worktree whose folder was deleted refuses the run and is not resumed. Undo of a
  worktree task resets the worktree's copy of the project folder and is refused once the task has
  no worktree (its checkpoint may be a worktree commit). Turn snapshots of a worktree session
  are of the worktree, so `turn.restore` is refused for worktree tasks until T13.4; the file
  watcher and hook-edit attribution still cover the project folder only. Two simultaneous first
  Runs of one worktree task: the second fails on `worktree add` (branch exists), nothing moves.
  If `worktree add` succeeds but recording it in the store fails, the run errors naming the
  folder and branch to remove by hand. The banner line is written before the agent's first frame,
  so a full-screen agent (claude, codex) may paint over it on screen (it stays in the session log)
  — check with a real agent.
- (T13.9) **Usage counting:** Claude "input" = `input_tokens` + `cache_creation_input_tokens` +
  `cache_read_input_tokens` summed over API messages (deduped by `message.id` + `requestId`,
  since one message is written as one line per content block), so it counts every token the
  model read, cache reads included, and is the same meaning as Codex's `input_tokens`. Claude
  context = that sum on the newest non-sidechain, non-`<synthetic>` message (output not
  included); sidechain messages count in the totals. Codex: the newest cumulative
  `total_token_usage` (not summed) and context = `last_token_usage.total_tokens`. Field shapes
  checked against Claude Code 2.1.250 and Codex CLI 0.130.0 files plus codex-rs source; the
  fixtures are hand-trimmed copies of those shapes. An unknown `[usage] format` name is not a
  profile error (no usage + one warning). Sample profiles now set `context_window` 200000
  (claude) and 258400 (codex): check both against the models you use.
  Changed 2026-10-10: `input` is fresh input only, the cache has its own fields (ADR 0035).
- (T13.10a) **Usage wiring:** added request `session.usage` (`project_id` → newest usage per
  session, stored as `session_event` kind `usage` on change) so the UI can show totals after a
  reconnect or daemon restart (SPEC §20E protocol line updated; protocol v23). Usage is read only
  at turn ends, and only once the agent reported its native session id through a hook (a
  screen-only profile gets no usage). Totals are per native conversation: a resumed session
  re-reads the same transcript from the start, so the UI must not add up a task's sessions
  (show the newest one). Claude `/clear` (new session id) starts a new transcript and totals.
  Codex: when several rollout files contain the id, the last by path (newest date) is read.
- (T13.10b) **Usage display:** total = input + cache_read + cache_write + output (`12.3k tok` on cards, bare `12.3k` on
  sidebar rows, details on hover). A card shows its newest session that reported usage (a just
  resumed run keeps showing the earlier one until its first turn ends); a group (fan-out
  original) card shows none. A sidebar workspace row adds, per task, that same session plus
  every session without a task. The pane header shows `ctx 62%` only when the profile sets
  `context_window`; from 80 % it turns amber with `⚠` (`status_needs_input`, as conflict
  warnings; no separate warning token). Each workspace's `session.usage` is read once, when it
  first appears in the lists (events keep it live after); a failed read is logged and retried on
  the next refresh. Check the labels and amber warning with real Claude/Codex.

## Done

- [x] (T0.9) **M0 gate — UI framework.** Read `docs/decisions/0001-ui-framework.md`
  (recommendation: GPUI; switch criteria listed under "Decision"). Do the manual checks below,
  then answer here: "GPUI" (ADR → accepted, tick SPEC §19 open question) or "Tauri" (the next
  session writes ADR 0002 and replans). Then set `status: RUNNING`; the next session plans M1.
  Note: the automated "grid-dump snapshot fed with recorded agent output" criterion is still
  open (only a hand-made TUI frame exists, see T0.4 below); answering GPUI accepts M0 without it
  — or record a real session first.
  **Answer (2026-10-05): GPUI straight away — follow the recommendation; all assumed defaults
  accepted.** ADR 0001 → accepted, SPEC §19 question ticked, `status: RUNNING`.
- [x] (T0.9 / T0.7) Real agents: `cargo run -p spike-term -- claude` (and `-- codex`, `-- omp`)
  on native Windows. The npm `.cmd`/`.ps1` shim is found via `PATHEXT`, the TUI renders with
  correct colors and no broken frames. Tests only used fake shims.
- [x] (T0.9 / T0.6) Input: type in the shell and in `claude` (letters, AltGr chars on your
  layout, dead keys / IME if you use one — expected to fail, no `InputHandler` yet), Enter,
  Backspace, arrows/history, Tab, Ctrl+C on a running command, Esc;
  Ctrl+Shift+V of multi-line text (shell and a bracketed-paste TUI). — ok
- [x] (T0.9 / T0.6) Resize: drag the window edges with a shell and with `claude` running; the
  prompt/TUI reflows to the new size without leftovers. — ok
- [x] (T0.9 / T0.5) Visuals on native Windows, dark and light OS themes: prompt readable, `dir`
  / TUI colors, accent cursor block, wide chars/emoji aligned. — ok
- [x] (T0.9 / T0.8) Perceived latency and idle cost: type in
  `cargo run --release -p spike-term -- --stats` (read `key_p50` lines) and judge the idle CPU
  (~3.5 % of one core, GPUI vsync floor) and flood CPU (~0.8 core at 165 Hz) in the ADR. — ok
