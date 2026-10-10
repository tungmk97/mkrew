# Inbox — bugs, enhancements, features

Write one line per item under **Items**: `- [ ] <what is wrong / what you want>`.
Optional tag at the start: `[bug]`, `[enh]`, `[feat]` (the skill guesses if missing).
Indented lines under an item are part of it (steps to reproduce, screenshots, ideas).

Then run the `mkrew-inbox` skill. It only reads `[ ]` items.

Markers: `[ ]` new · `[~]` planned (skill adds `→ plan: <file>`) · `[x]` done · `[-]` dropped.

## Items

- [x] In the git commit box: cannot Ctrl+A to select all to remove → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 1
- [~] Add a feature to config font style, font size... for the app and for the terminal → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 4, 5
- [~] I want more view on the main content, so the top bar (Board, Terminal...) should move somewhere else — I'm thinking the right bar. The Run button can move inline with the History, + Agent bar. So we clean up the top bar and get more space → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 6
- [~] In terminal, Ctrl+C now cannot be used to copy text, please fix → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 2
- [~] In terminal when typing e.g in OMP cannot Ctrl + Backspace to delete a text now just delete by character. And cannot Ctrl + A to select all to remove → plan: docs/superpowers/plans/2026-10-08-inbox-keys-fonts-view-rail.md Task 3
- [~] Should have icon the the agent that showing when agent inprogress, iddle, done, or ask me → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 1
- [~] Check if we don't have feature to drag drop a file into the terminal chat if not need to add in → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 2
- [x] Check if we can Ctrl+V to paste text or image into the terminal chat, if not need to add in → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 3
- [x] For the agent in the workspace do we have icon for each agent, if not yet then need to add in for better view → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 4
- [x] In case agent running, we're shoing circle loop icon, but I dont like that, I like dot rather than circle, can think a bout multiple dot aninamtion look beautifull → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 1
- [~] [feat] Use the new Mkrew logo everywhere (source: `icon/mkrew-1-lanes/`, picked `tile-dark.svg`) → plan: docs/superpowers/plans/2026-10-08-inbox-dots-drop-paste-redesign.md Task 11
  - Done: `mkrew.exe` icon (Explorer, window, taskbar, Alt+Tab) embedded via `crates/mkrew-ui/build.rs` + `resources/windows/mkrew.ico` (16–256 px).
  - Left: installer icon (`SetupIconFile=` in `packaging/mkrew.iss`, `.wxs` icon); `mk.exe` / `mkrew-daemon.exe` icons if wanted; logo (glyph) in the app (sidebar header / About); lockup in `README.md`.
  - Regenerate the `.ico` whenever the SVG changes (render 16,20,24,32,40,48,64,256 px).
- [x] Terminal and inside agent: add Shift + Enter for wrting in the new line → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 1
- [x] Files module: when viewing a file need to have a short cut for find and replace content: can follow to vscode style if needed, and also need short cut for find, replate replace all action → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 7
- [x] Files Module: I expected I can edit information there not just read → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 3, 4, 5, 6
- [x] Git module: also need support a case where I open an Root folter container FE and BE, then git shold dected there are 2 diffreence git repo and also need to support in this case → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 8, 9
- [x] Support F11 to view full screen → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 2
- [x] Add a short cut to rename pain, good if we can also auto rename base on what I'm working, if this feature is not too big → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 13, 14
- [x] in terminal agent, sometime I cannot scroll on top over 1 page showing in the screen, please fix → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 10
- [x] Add short cut for '+ Agent' which is open new agent by split current tab now → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 11
- [x] Add short cut for '+ Shell' and user must to select default shell user want to use same as agent concept → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 11, 12
- [x] Check all input field in this app should suppport Ctrl A, Ctrl Backspace... same as standard for input field so that can easy to update the value.... → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 15
- [x] Running command, when a server user click stop: should remove this terminal, user click restart: don't create new one stop exisitng terminal then start again → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 16
- [x] when a shell or agent is stopped, it still showing in the sidebar cannot clickable can just remove, so need auto clean up the workspace also, there're no meaning when juz show to remove because cannot click able anymore juz to close → plan: docs/superpowers/plans/2026-10-09-inbox-editor-git-repos-keys.md Task 17
- [x] **1. Title bar + toolbar redesign** [feat] (redesign A1, A2, A3, A4; merged: "Run toolbar colors") → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 1, 2, 3
  - Source: `docs/superpowers/specs/2026-10-10-ui-redesign-options.md` (all redesign IDs below point there). Keep the current background colors.
  - One custom title bar replaces the OS title bar and the 44 px toolbar: Mkrew logo (see the logo item above), workspace name ▾ (switcher dropdown), git branch, views Board / Terminals / Docs / Files / Git, then History, + Agent, + Shell, Run, and drawn ─ □ ✕ window controls.
  - Windows: drag area, double-click to maximize, Win11 Snap Layouts on □, resize edges. macOS/Linux keep working. Needs an ADR (amends ADR 0027) and a SPEC §10 update.
  - Git branch + `Git N` badge on the Git view (N = changed files). Must work for a workspace with 1 repo AND a folder holding N repos (ADR 0029 child repos, the root may have no `.git`): 1 repo → `⎇ master`; N repos → `⎇ N repos`, hover/menu lists each repo's branch; badge = changed files summed over all repos. Never blank just because the root has no `.git`.
  - Run button colors: Stop red, Restart amber/orange, Run/Start green — same colors on the icons and hover states, in the title bar and in the ▾ dropdown.
  - Perf: `git.status` only for the shown workspace, on switch and on `git.changed`; no polling, nothing for hidden workspaces.

- [x] **2. Usage data: omp usage, cache split, model** [feat] (redesign B6, B8, B9) → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 4, 5
  - omp sessions get token usage like Claude/Codex: a new `[usage] format` for omp's session file (check the current omp format, record fixtures from a real file) and `[usage]` in `agents/omp.toml`. Then `ctx %` and token totals show for omp panes.
  - Split cache tokens (read / write) out of `input` in `session.usage`, for every format that has them.
  - Read the model name of the newest message (Claude `message.model`, Codex/omp equivalent) and send it with `session.usage`.
  - Protocol bump + SPEC §20E.3 update. Still no cost/$ (SPEC: no money estimate).
  - Perf: same incremental read at turn end as today; no file watching, no mid-turn reads.

- [x] **3. Status bar + hide sidebar** [feat] (redesign A13, A12 status part, C1 sidebar part; after item 2) → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 6
  - Thin bar at the window bottom: sidebar toggle button, inbox state (`Inbox clear` / `N need you`; click = jump to the next one, like Ctrl+Shift+I), git HEAD short hash; for the focused agent pane: context % bar, in/out tokens, cache, model, elapsed time; right end: right-panel toggle (wired in item 7).
  - Git: 1 repo → short hash; N repos → `N repos`, hover lists each repo's branch + hash. A root without `.git` still shows its child repos.
  - Only real data: no $, no tok/s. A field with no data is hidden, not shown as 0.
  - Toggle sidebar: palette command + keymap default (SPEC §13).
  - Perf: re-render only on `session.usage`, focus change, inbox change, `git.changed`.

- [x] **4. Workspace sidebar redesign + cleanup** [enh] (redesign A5, A6, A7, A8, A9 second line, A12 card removal; after item 3) → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 7, 8, 9
  - Filter box "Filter sessions…" at the top (matches workspace, agent, title).
  - Chips `All N` / `Working N` / `Needs you N` under it; "Needs you" counts needs_input and done-not-yet-viewed sessions (what the Inbox lists today).
  - Remove the Inbox card at the sidebar bottom (the chips + status bar replace it; peek on hover stays).
  - Header: + add workspace and a collapse/expand-all button.
  - Workspace row: summary text `1 working` / `2 idle` instead of the token total.
  - Session row: only the time on the right (e.g. `17m`), no "idle" word — the status icon (working, asking, idle) already shows it. The selected row gets a second line with the session/task title.
  - After right-click → Change color on a project, stay on the current view instead of navigating to that project.
  - Replace the project color palette with 8–10 clearly distinct hues that are readable in both light and dark themes.
  - Perf: filter and counts on the client, O(sessions) per render.

- [x] **5. Terminal tabs + pane header** [enh] (redesign A10, A11; merged: "Pane improvements") → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 10, 11, 12
  - Tab strip: icon per tab type (agent badge / shell `>_`), status dot, title; on the right `+` (new tab), a split button and a `⋯` menu (split right/down, rename, close).
  - Pane header: breadcrumb `<workspace> › <folder> › <agent>` + one status pill `Working · 1h 25m` on the left. Remove the status/time text from the pane's top-right bar (the pill replaces it, so status shows once). Follow in Zed and Turns stay on the right.
    - Open question: keep the pill (mockup A11) or no status in the pane at all (older note "already shown elsewhere")? Default: one pill on the left.
  - Right-click menu on panes:
    - "Rename" (inline edit, Enter to save, Esc to cancel)
    - "Change color" (same palette as projects, item 4)

- [x] **6. Pin feature** (after items 4 and 5) → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 13, 14
  - Add Pin/Unpin to the right-click menu for:
    - Workspace → Project
    - Workspace → Project → Terminal / Agent
    - Terminal → Terminal tab
  - Pinned items move to the top of their list (or the left, for tabs) and show a pin icon.
  - Pin state persists across app restarts.

- [x] **7. Right panel (session inspector)** [feat] (redesign B1, B2, B3, C1 panel part; after items 2 and 3) → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 15, 16
  - Panel on the right of Terminals, closed by default; toggled by the status bar button + a key (SPEC §13); width resizable and remembered. Shows the focused pane's session.
  - Session card: agent, title, state + time, usage from item 2 (ctx %, tokens, model); buttons "Open transcript" (the agent's transcript file, paths already located by the usage reader) and "Stop" (asks first, like closing a running pane).
  - Changes card: `M 23 ?3` + "Review diff" → opens the Git view on that repo. 1 or N repos: one line per repo with changes; a root without `.git` still lists its child repos.
  - Not included (dropped in the review): cost, subagent card, tool calls, "Rules in effect".
  - Perf: the terminal loses ~300 px of columns while open (agent TUI reflows once); git data shared with item 1.

- [x] **8. Plan checklist in the right panel** [feat] (redesign B4, A9 progress bar; after item 7) → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 17, 18
  - Link a plan `.md` to a session (default: pick the file from the panel; remembered per session across restarts).
  - Panel shows `Plan <name> 6/20`, a progress bar and the checklist from `- [ ]` / `- [x]` lines (first unchecked highlighted, "N more" collapsed). The selected sidebar session row shows the same progress bar.
  - Live: the daemon watches only the linked file (debounced) and sends the counts. No "Setup/Tasks/Finish" phase bar.
  - Perf: one file watcher per linked plan; parsing is tiny.

- [x] **9. File navigation & Markdown preview** (merged: redesign B16) → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 19, 20, 21
  - Add "Open in File Explorer", "Open In Zed" item to the right-click menu on files in the file view and on workspace/project folders and the Files module. It opens the OS file explorer at that location (Explorer, Finder, or the Linux file manager).
  - In terminal panes (agent conversations and shells), detect file paths (relative or absolute, optionally with `:line`) and URLs. Ctrl+Click (Cmd+Click on macOS) opens a file in the Files view at the line, a URL in the browser. Show an underline/pointer while Ctrl/Cmd is held.
  - When opening a `.md` file in Files, show a rendered preview by default with a toggle for raw/source mode. Render ` ```mermaid ` blocks as diagrams. Use clean typography for headings, lists, tables, and code blocks, and support light/dark themes.
  - Perf: detect links only on the hovered row while Ctrl/Cmd is held; never in the terminal render/parse path.

- [x] **10. Git module: diff view modes** → plan: docs/superpowers/plans/2026-10-10-inbox-redesign.md Task 22
  - In the file changes view, add a toggle to switch between:
    - **Unified (1 pane)**: additions and deletions inline in a single view
    - **Split (2 pane)**: original on the left, modified on the right, with synced scrolling
  - Highlight added, removed, and modified lines with clear colors in both modes.
  - Remember the selected mode across files and app restarts.
  - Works for every repo of the workspace (1 or N repos, ADR 0029 repo chooser).